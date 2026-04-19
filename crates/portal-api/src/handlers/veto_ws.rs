//! WebSocket handler for veto lobby real-time communication.
//!
//! Not annotated with `#[utoipa::path]` — OpenAPI 3.0 describes
//! request/response HTTP contracts and has no first-class representation
//! for WebSocket channel protocols. The client-side message schema for
//! this endpoint lives in `crate::websocket::{ClientMessage,
//! ServerMessage}`; keep that in sync with any protocol changes.

use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, State, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use portal_core::TournamentMatchId;
use portal_domain::validate_token;
use tokio::sync::broadcast;
use tokio::time::timeout;
use tracing::{error, info, warn};

use crate::state::AppState;
use crate::websocket::{
    ChatBroadcast, ClientChatType, ClientMessage, ClientVetoAction, CoinFlipResultBroadcast,
    ConnectionId, LobbyBroadcast, ParticipantConnectionBroadcast, ServerMessage,
    VetoActionBroadcast, VetoCompleteBroadcast, VetoConnection, VetoLobby, VetoStateBroadcast,
};

/// Authentication timeout in seconds.
const AUTH_TIMEOUT_SECS: u64 = 10;

/// Ping interval in seconds.
const PING_INTERVAL_SECS: u64 = 30;

/// WebSocket upgrade handler for veto lobby.
///
/// Upgrades an HTTP connection to a WebSocket connection for real-time
/// veto lobby communication.
pub async fn ws_upgrade(
    ws: WebSocketUpgrade,
    Path(match_id): Path<TournamentMatchId>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, match_id, state))
}

/// Handle a WebSocket connection.
async fn handle_socket(socket: WebSocket, match_id: TournamentMatchId, state: AppState) {
    let connection_id = ConnectionId::new_v4();
    info!(%match_id, %connection_id, "New WebSocket connection");

    // Split the socket into sender and receiver
    let (mut sender, mut receiver) = socket.split();

    // Wait for authentication message
    let auth_result = timeout(
        Duration::from_secs(AUTH_TIMEOUT_SECS),
        wait_for_auth(&mut receiver, match_id, &state),
    )
    .await;

    let (connection, lobby_state) = match auth_result {
        Ok(Ok((conn, lobby_state))) => (conn, lobby_state),
        Ok(Err(err)) => {
            warn!(%match_id, %connection_id, error = %err, "Authentication failed");
            let msg = ServerMessage::AuthError {
                error: err.clone(),
            };
            let _ = sender
                .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                .await;
            return;
        }
        Err(_) => {
            warn!(%match_id, %connection_id, "Authentication timed out");
            let msg = ServerMessage::AuthError {
                error: "Authentication timed out".to_string(),
            };
            let _ = sender
                .send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
                .await;
            return;
        }
    };

    // Get or create the lobby
    let lobby = state.veto_lobby_manager.get_or_create_lobby(match_id);

    // Add connection to lobby
    lobby.add_connection(connection_id, connection.clone());

    // Subscribe to broadcasts
    let mut broadcast_rx = lobby.subscribe();

    // Send auth success with lobby state
    let auth_success = ServerMessage::AuthSuccess {
        role: connection.role.as_str().to_string(),
        registration_id: connection.registration_id.map(|id| id.to_string()),
        team_name: connection.team_name.clone(),
        lobby_state,
    };

    if sender
        .send(Message::Text(
            serde_json::to_string(&auth_success).unwrap().into(),
        ))
        .await
        .is_err()
    {
        lobby.remove_connection(&connection_id);
        return;
    }

    // Broadcast participant connected if applicable
    if connection.is_participant() {
        if let Some(reg_id) = connection.registration_id {
            lobby.broadcast(LobbyBroadcast::ParticipantConnected(
                ParticipantConnectionBroadcast {
                    registration_id: reg_id,
                    team_name: connection.team_name.clone().unwrap_or_default(),
                    username: connection.username.clone(),
                },
            ));

            // Auto coin flip if both teams present and session is in CoinFlip status
            try_auto_coin_flip(&state, match_id, &lobby).await;
        }
    } else if connection.is_spectator() {
        lobby.broadcast(LobbyBroadcast::SpectatorCountUpdate(lobby.spectator_count()));
    }

    // Send chat history
    if let Ok(history) = get_chat_history(&state, match_id, &connection).await {
        let history_msg = ServerMessage::ChatHistory { messages: history };
        let _ = sender
            .send(Message::Text(
                serde_json::to_string(&history_msg).unwrap().into(),
            ))
            .await;
    }

    // Set up ping interval
    let mut ping_interval = tokio::time::interval(Duration::from_secs(PING_INTERVAL_SECS));

    // Main event loop
    loop {
        tokio::select! {
            // Handle incoming messages from client
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Err(err) = handle_client_message(
                            &text,
                            &connection,
                            match_id,
                            &state,
                            &lobby,
                            &mut sender,
                        ).await {
                            warn!(%connection_id, error = %err, "Error handling client message");
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        info!(%connection_id, "Connection closed");
                        break;
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = sender.send(Message::Pong(data)).await;
                    }
                    Some(Err(err)) => {
                        error!(%connection_id, error = %err, "WebSocket error");
                        break;
                    }
                    _ => {}
                }
            }
            // Handle broadcasts from lobby
            broadcast = broadcast_rx.recv() => {
                match broadcast {
                    Ok(msg) => {
                        if let Some(server_msg) = filter_broadcast_for_connection(&msg, &connection) {
                            if sender.send(Message::Text(
                                serde_json::to_string(&server_msg).unwrap().into()
                            )).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!(%connection_id, lagged = n, "Broadcast receiver lagged");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
            // Send ping periodically
            _ = ping_interval.tick() => {
                if sender.send(Message::Ping(vec![].into())).await.is_err() {
                    break;
                }
            }
        }
    }

    // Clean up
    if let Some(removed) = lobby.remove_connection(&connection_id) {
        if removed.is_participant() {
            if let Some(reg_id) = removed.registration_id {
                lobby.broadcast(LobbyBroadcast::ParticipantDisconnected(
                    ParticipantConnectionBroadcast {
                        registration_id: reg_id,
                        team_name: removed.team_name.clone().unwrap_or_default(),
                        username: removed.username,
                    },
                ));
            }
        } else if removed.is_spectator() {
            lobby.broadcast(LobbyBroadcast::SpectatorCountUpdate(lobby.spectator_count()));
        }
    }

    // Clean up empty lobbies
    if lobby.is_empty() {
        state.veto_lobby_manager.remove_lobby(&match_id);
    }

    info!(%match_id, %connection_id, "WebSocket connection closed");
}

/// Wait for authentication message and validate.
async fn wait_for_auth(
    receiver: &mut futures_util::stream::SplitStream<WebSocket>,
    match_id: TournamentMatchId,
    state: &AppState,
) -> Result<(VetoConnection, crate::websocket::messages::LobbyStatePayload), String> {
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                let client_msg: ClientMessage =
                    serde_json::from_str(&text).map_err(|e| format!("Invalid message: {e}"))?;

                match client_msg {
                    ClientMessage::Auth { token } => {
                        return authenticate_user(&token, match_id, state).await;
                    }
                    _ => {
                        return Err("First message must be authentication".to_string());
                    }
                }
            }
            Ok(Message::Close(_)) | Err(_) => {
                return Err("Connection closed before authentication".to_string());
            }
            _ => continue,
        }
    }

    Err("Connection closed before authentication".to_string())
}

/// Authenticate user from JWT token.
async fn authenticate_user(
    token: &str,
    match_id: TournamentMatchId,
    state: &AppState,
) -> Result<(VetoConnection, crate::websocket::messages::LobbyStatePayload), String> {
    use portal_domain::repositories::TournamentMatchRepository;

    // Validate JWT
    let claims = validate_token(token, &state.jwt_secret).map_err(|e| format!("Invalid token: {e}"))?;

    let user_id = claims
        .user_id()
        .map_err(|_| "Invalid user ID in token")?;
    let user_id = portal_core::UserId::from(user_id);
    let player_id = portal_core::PlayerId::from(claims.player_id);

    // Look up the match
    let match_ = state
        .tournament_match_repo
        .find_by_id(match_id)
        .await
        .map_err(|e| format!("Database error: {e}"))?
        .ok_or_else(|| format!("Match {match_id} not found"))?;

    use portal_domain::services::tournament::VetoAuthorizationRole;

    // Check if user can act for either participant
    let mut connection: Option<VetoConnection> = None;

    // Check participant 1
    if let Some(reg_id) = match_.participant1_registration_id {
        if let Ok(auth_role) = state
            .veto_authorization_service
            .can_perform_veto_action(reg_id, user_id, player_id)
            .await
        {
            // Tournament admins get admin role, others get participant role
            if matches!(auth_role, VetoAuthorizationRole::TournamentAdmin) {
                connection = Some(VetoConnection::admin(
                    user_id,
                    player_id,
                    claims.username.clone(),
                ));
            } else {
                let team_name = match_.participant1_name.clone().unwrap_or_default();
                connection = Some(VetoConnection::participant(
                    user_id,
                    player_id,
                    claims.username.clone(),
                    reg_id,
                    team_name,
                ));
            }
        }
    }

    // Check participant 2 (only if not already found as participant 1)
    if connection.is_none() {
        if let Some(reg_id) = match_.participant2_registration_id {
            if let Ok(auth_role) = state
                .veto_authorization_service
                .can_perform_veto_action(reg_id, user_id, player_id)
                .await
            {
                // Tournament admins get admin role, others get participant role
                if matches!(auth_role, VetoAuthorizationRole::TournamentAdmin) {
                    connection = Some(VetoConnection::admin(
                        user_id,
                        player_id,
                        claims.username.clone(),
                    ));
                } else {
                    let team_name = match_.participant2_name.clone().unwrap_or_default();
                    connection = Some(VetoConnection::participant(
                        user_id,
                        player_id,
                        claims.username.clone(),
                        reg_id,
                        team_name,
                    ));
                }
            }
        }
    }

    // Check if user is a tournament admin (fallback for users not on any team)
    if connection.is_none()
        && state
            .permission_service
            .has_permission(user_id, "tournament.manage")
            .await
            .unwrap_or(false)
        {
            connection = Some(VetoConnection::admin(user_id, player_id, claims.username.clone()));
        }

    // Default to spectator if not authorized for any role
    let connection = connection.unwrap_or_else(|| {
        VetoConnection::spectator(user_id, player_id, claims.username)
    });

    // Build lobby state with real match data
    let lobby_state = crate::websocket::messages::LobbyStatePayload {
        match_id: match_id.to_string(),
        session: None, // Will be populated by caller if veto session exists
        participants: crate::websocket::messages::ParticipantsPayload {
            participant1: crate::websocket::messages::ParticipantPayload {
                registration_id: match_
                    .participant1_registration_id
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
                name: match_.participant1_name.unwrap_or_default(),
                is_connected: false, // Will be updated after joining lobby
            },
            participant2: crate::websocket::messages::ParticipantPayload {
                registration_id: match_
                    .participant2_registration_id
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
                name: match_.participant2_name.unwrap_or_default(),
                is_connected: false, // Will be updated after joining lobby
            },
        },
        spectator_count: 0,
        connected_participants: vec![],
    };

    Ok((connection, lobby_state))
}

/// Handle a client message.
async fn handle_client_message(
    text: &str,
    connection: &VetoConnection,
    match_id: TournamentMatchId,
    state: &AppState,
    lobby: &Arc<VetoLobby>,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
) -> Result<(), String> {
    let msg: ClientMessage =
        serde_json::from_str(text).map_err(|e| format!("Invalid message: {e}"))?;

    match msg {
        ClientMessage::Auth { .. } => {
            // Already authenticated
            let err = ServerMessage::Error {
                code: "already_authenticated".to_string(),
                message: "Already authenticated".to_string(),
            };
            let _ = sender
                .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
                .await;
        }
        ClientMessage::Chat { chat_type, content } => {
            handle_chat_message(connection, match_id, chat_type, content, state, lobby).await?;
        }
        ClientMessage::VetoAction { action } => {
            handle_veto_action(connection, match_id, action, state, sender).await?;
        }
        ClientMessage::Ping => {
            let _ = sender
                .send(Message::Text(
                    serde_json::to_string(&ServerMessage::Pong).unwrap().into(),
                ))
                .await;
        }
    }

    Ok(())
}

/// Handle a chat message.
async fn handle_chat_message(
    connection: &VetoConnection,
    match_id: TournamentMatchId,
    chat_type: ClientChatType,
    content: String,
    state: &AppState,
    lobby: &Arc<VetoLobby>,
) -> Result<(), String> {
    match chat_type {
        ClientChatType::Team => {
            if !connection.can_send_team_chat() {
                return Err("Cannot send team chat".to_string());
            }

            let registration_id = connection
                .registration_id
                .ok_or("No registration ID for team chat")?;

            let message = state
                .veto_lobby_chat_service
                .send_team_message(
                    match_id,
                    connection.user_id,
                    registration_id,
                    None,
                    content,
                )
                .await
                .map_err(|e| e.to_string())?;

            lobby.broadcast(LobbyBroadcast::Chat(ChatBroadcast {
                message,
                author_username: connection.username.clone(),
                author_team_name: connection.team_name.clone(),
            }));
        }
        ClientChatType::All => {
            if !connection.can_send_all_chat() {
                return Err("Cannot send all chat".to_string());
            }

            let message = state
                .veto_lobby_chat_service
                .send_all_message(
                    match_id,
                    connection.user_id,
                    connection.registration_id,
                    None,
                    content,
                )
                .await
                .map_err(|e| e.to_string())?;

            lobby.broadcast(LobbyBroadcast::Chat(ChatBroadcast {
                message,
                author_username: connection.username.clone(),
                author_team_name: connection.team_name.clone(),
            }));
        }
    }

    Ok(())
}

/// Handle a veto action.
async fn handle_veto_action(
    connection: &VetoConnection,
    match_id: TournamentMatchId,
    action: ClientVetoAction,
    state: &AppState,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
) -> Result<(), String> {
    use crate::dto::responses::{VetoActionResponse, VetoSessionResponse};

    // Check if user can perform veto actions
    if !connection.can_perform_veto_action() {
        let err = ServerMessage::Error {
            code: "not_authorized".to_string(),
            message: "Only participants can perform veto actions".to_string(),
        };
        let _ = sender
            .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
            .await;
        return Ok(());
    }

    // Get registration ID from connection
    let registration_id = connection.registration_id.ok_or_else(|| {
        "Connection has no registration ID".to_string()
    })?;

    // Get the veto session state
    let session_state = state
        .veto_service
        .get_session_state(match_id)
        .await
        .map_err(|e| e.to_string())?;

    // Handle the action type
    match action {
        ClientVetoAction::Ban { map_id } | ClientVetoAction::Pick { map_id } => {
            // Perform the veto action
            // The service will validate that it's the correct team's turn
            let result = state
                .veto_service
                .perform_action(
                    session_state.session.id,
                    &map_id,
                    registration_id,
                    connection.user_id,
                )
                .await;

            match result {
                Ok(action_result) => {
                    // Broadcast to lobby (the REST handlers do this, so we mirror the behavior)
                    if let Some(lobby) = state.veto_lobby_manager.get_lobby(&match_id) {
                        if action_result.veto_complete {
                            let () = lobby.broadcast(LobbyBroadcast::VetoComplete(VetoCompleteBroadcast {
                                session: VetoSessionResponse::from(action_result.session.clone()),
                                selected_maps: action_result.session.selected_maps.clone(),
                            }));
                        } else {
                            let () = lobby.broadcast(LobbyBroadcast::VetoActionPerformed(VetoActionBroadcast {
                                session: VetoSessionResponse::from(action_result.session.clone()),
                                action: VetoActionResponse::from(action_result.action.clone()),
                                is_complete: false,
                            }));
                        }
                    }
                    // Send success acknowledgment to sender
                    let ack = ServerMessage::VetoActionAck {
                        success: true,
                        message: None,
                    };
                    let _ = sender
                        .send(Message::Text(serde_json::to_string(&ack).unwrap().into()))
                        .await;
                }
                Err(e) => {
                    let (code, message) = if e.to_string().contains("not your turn")
                        || e.to_string().contains("not authorized")
                    {
                        ("not_your_turn", "It's not your team's turn")
                    } else {
                        ("veto_error", &*e.to_string())
                    };
                    let err = ServerMessage::Error {
                        code: code.to_string(),
                        message: message.to_string(),
                    };
                    let _ = sender
                        .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
                        .await;
                }
            }
        }
        ClientVetoAction::SelectSide { action_number, side } => {
            // Select a side for a picked map
            // The service will validate that this is the correct team to select side
            let result = state
                .veto_service
                .select_side(
                    session_state.session.id,
                    action_number,
                    &side,
                    registration_id,
                    connection.user_id,
                )
                .await;

            match result {
                Ok(updated_action) => {
                    // Broadcast to lobby
                    if let Some(lobby) = state.veto_lobby_manager.get_lobby(&match_id) {
                        // Re-fetch session state to get the latest
                        if let Ok(new_session_state) = state.veto_service.get_session_state(match_id).await {
                            let () = lobby.broadcast(LobbyBroadcast::VetoActionPerformed(VetoActionBroadcast {
                                session: VetoSessionResponse::from(new_session_state.session),
                                action: VetoActionResponse::from(updated_action.clone()),
                                is_complete: false,
                            }));
                        }
                    }
                    // Send success acknowledgment
                    let ack = ServerMessage::VetoActionAck {
                        success: true,
                        message: None,
                    };
                    let _ = sender
                        .send(Message::Text(serde_json::to_string(&ack).unwrap().into()))
                        .await;
                }
                Err(e) => {
                    let err = ServerMessage::Error {
                        code: "side_select_error".to_string(),
                        message: e.to_string(),
                    };
                    let _ = sender
                        .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
                        .await;
                }
            }
        }
    }

    Ok(())
}

/// Auto-perform coin flip when both participants are connected and session is in CoinFlip status.
async fn try_auto_coin_flip(
    state: &AppState,
    match_id: TournamentMatchId,
    lobby: &Arc<VetoLobby>,
) {
    use portal_domain::repositories::TournamentMatchRepository;
    use rand::Rng;

    // Look up the match to get participant registration IDs
    let match_ = match state.tournament_match_repo.find_by_id(match_id).await {
        Ok(Some(m)) => m,
        _ => return,
    };

    let (p1_reg, p2_reg) = match (
        match_.participant1_registration_id,
        match_.participant2_registration_id,
    ) {
        (Some(p1), Some(p2)) => (p1, p2),
        _ => return,
    };

    // Verify both DISTINCT participants are connected (not same user in two tabs)
    let connected = lobby.connected_participant_ids();
    let p1_str = p1_reg.to_string();
    let p2_str = p2_reg.to_string();
    if !connected.contains(&p1_str) || !connected.contains(&p2_str) {
        return;
    }

    // Get veto session — must be in CoinFlip status
    let session_state = match state.veto_service.get_session_state(match_id).await {
        Ok(s) => s,
        Err(_) => return,
    };

    if session_state.session.status != portal_domain::entities::VetoStatus::CoinFlip {
        return;
    }

    // Randomly pick winner
    let winner = if rand::rng().random_bool(0.5) {
        p1_reg
    } else {
        p2_reg
    };

    // Record coin flip (winner goes first by default)
    let updated_session = match state
        .veto_service
        .record_coin_flip(session_state.session.id, winner, true)
        .await
    {
        Ok(s) => s,
        Err(e) => {
            warn!(%match_id, error = %e, "Auto coin flip failed");
            return;
        }
    };

    let first_action = updated_session
        .first_action_registration_id
        .unwrap_or(winner);

    let winner_name = if winner == p1_reg {
        match_.participant1_name.clone().unwrap_or_default()
    } else {
        match_.participant2_name.clone().unwrap_or_default()
    };

    let first_action_name = if first_action == p1_reg {
        match_.participant1_name.unwrap_or_default()
    } else {
        match_.participant2_name.unwrap_or_default()
    };

    // Broadcast coin flip result
    lobby.broadcast(LobbyBroadcast::CoinFlipResult(CoinFlipResultBroadcast {
        winner_registration_id: winner,
        winner_name,
        first_action_registration_id: first_action,
        first_action_name,
    }));

    // Broadcast updated session state
    let session_response =
        crate::dto::responses::VetoSessionResponse::from(updated_session);
    lobby.broadcast(LobbyBroadcast::VetoStateUpdate(VetoStateBroadcast {
        session: session_response,
    }));

    info!(%match_id, %winner, "Auto coin flip completed");
}

/// Get chat history for a connection.
async fn get_chat_history(
    state: &AppState,
    match_id: TournamentMatchId,
    connection: &VetoConnection,
) -> Result<Vec<crate::websocket::messages::ChatMessagePayload>, String> {
    let messages = if let Some(reg_id) = connection.registration_id {
        state
            .veto_lobby_chat_service
            .get_participant_history(match_id, reg_id)
            .await
    } else {
        state
            .veto_lobby_chat_service
            .get_spectator_history(match_id)
            .await
    }
    .map_err(|e| e.to_string())?;

    // Look up match for team name resolution
    use portal_domain::repositories::TournamentMatchRepository;
    let match_ = state
        .tournament_match_repo
        .find_by_id(match_id)
        .await
        .ok()
        .flatten();

    // Collect unique user IDs and batch-resolve usernames
    let unique_ids: std::collections::HashSet<_> =
        messages.iter().map(|m| m.author_user_id).collect();
    let mut usernames = std::collections::HashMap::new();
    for uid in unique_ids {
        if let Ok(user) = state.user_service.get_user(uid).await {
            usernames.insert(uid, user.username);
        }
    }

    let payloads = messages
        .into_iter()
        .map(|msg| {
            let username = usernames
                .get(&msg.author_user_id)
                .cloned()
                .unwrap_or_else(|| "Unknown".to_string());
            let team_name = match_.as_ref().and_then(|m| {
                if msg.author_registration_id == m.participant1_registration_id {
                    m.participant1_name.clone()
                } else if msg.author_registration_id == m.participant2_registration_id {
                    m.participant2_name.clone()
                } else {
                    None
                }
            });
            crate::websocket::messages::ChatMessagePayload {
                id: msg.id.to_string(),
                chat_type: msg.message_type.to_string(),
                author: crate::websocket::messages::ChatAuthorPayload {
                    user_id: msg.author_user_id.to_string(),
                    username,
                    registration_id: msg.author_registration_id.map(|id| id.to_string()),
                    team_name,
                },
                content: msg.content,
                timestamp: msg.created_at,
            }
        })
        .collect();

    Ok(payloads)
}

/// Filter a broadcast message for a specific connection.
///
/// Returns `Some(ServerMessage)` if the connection should receive the message,
/// or `None` if it should be filtered out.
fn filter_broadcast_for_connection(
    broadcast: &LobbyBroadcast,
    connection: &VetoConnection,
) -> Option<ServerMessage> {
    match broadcast {
        LobbyBroadcast::Chat(chat) => {
            // Check visibility based on connection role
            if connection.is_spectator() {
                if chat.is_visible_to_spectators() {
                    Some(chat.to_server_message())
                } else {
                    None
                }
            } else if let Some(reg_id) = connection.registration_id {
                if chat.is_visible_to_participant(reg_id) {
                    Some(chat.to_server_message())
                } else {
                    None
                }
            } else {
                // Admin or no registration - show all public
                if chat.is_visible_to_spectators() {
                    Some(chat.to_server_message())
                } else {
                    None
                }
            }
        }
        LobbyBroadcast::CoinFlipResult(result) => Some(ServerMessage::CoinFlipResult {
            winner_registration_id: result.winner_registration_id.to_string(),
            winner_name: result.winner_name.clone(),
            first_action_registration_id: result.first_action_registration_id.to_string(),
            first_action_name: result.first_action_name.clone(),
        }),
        LobbyBroadcast::VetoStateUpdate(update) => Some(ServerMessage::VetoStateUpdate {
            session: update.session.clone(),
        }),
        LobbyBroadcast::VetoActionPerformed(action) => Some(ServerMessage::VetoActionPerformed {
            session: action.session.clone(),
            action: action.action.clone(),
            is_complete: action.is_complete,
        }),
        LobbyBroadcast::VetoComplete(complete) => Some(ServerMessage::VetoComplete {
            selected_maps: complete.selected_maps.clone(),
            session: complete.session.clone(),
        }),
        LobbyBroadcast::TimeoutWarning(warning) => Some(ServerMessage::TimeoutWarning {
            seconds_remaining: warning.seconds_remaining,
            current_team: warning.current_team_name.clone(),
            current_team_registration_id: warning.current_team_registration_id.to_string(),
        }),
        LobbyBroadcast::ParticipantConnected(conn) => Some(ServerMessage::PlayerConnected {
            registration_id: conn.registration_id.to_string(),
            team_name: conn.team_name.clone(),
            username: conn.username.clone(),
        }),
        LobbyBroadcast::ParticipantDisconnected(conn) => Some(ServerMessage::PlayerDisconnected {
            registration_id: conn.registration_id.to_string(),
            team_name: conn.team_name.clone(),
            username: conn.username.clone(),
        }),
        LobbyBroadcast::SpectatorCountUpdate(count) => {
            Some(ServerMessage::SpectatorCount { count: *count })
        }
    }
}
