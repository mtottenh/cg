//! Game-server registry service.
//!
//! Admin CRUD over registered servers, one-time agent enrollment, heartbeat
//! ingestion (including out-of-band busy detection, §6.7), and event
//! bookings. Design: docs/matchzy-integration.md §4–§6.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use portal_core::errors::DomainError;
use portal_core::ids::{GameServerId, ServerBookingId};
use portal_core::types::GameServerStatus;
use rand::Rng;
use sha2::{Digest, Sha256};

use crate::entities::{AgentCertificate, GameServer, HeartbeatUpdate, ServerBooking};
use crate::repositories::{
    AgentCertRepository, CreateAgentCertificate, CreateGameServer, CreateServerBooking,
    GameServerRepository, RecordHeartbeat, ServerBookingRepository, UpdateGameServer,
};

use super::ca::{AGENT_CERT_VALIDITY_DAYS, CertificateAuthority, IssuedCertificate};
use super::setup::generate_reservation_token;

/// Heartbeats older than this mark a server offline.
pub const HEARTBEAT_STALENESS_SECS: i64 = 90;

/// One-time enrollment tokens live this long.
pub const ENROLLMENT_TOKEN_TTL_HOURS: i64 = 24;

/// Prefix for enrollment tokens (mirrors the `cgp_` api-key convention).
const ENROLLMENT_TOKEN_PREFIX: &str = "cgs_";

/// Generate a random enrollment token (`cgs_` + 32 bytes hex).
#[must_use]
pub fn generate_enrollment_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    format!("{ENROLLMENT_TOKEN_PREFIX}{}", hex::encode(bytes))
}

/// SHA-256 hex of a token, the at-rest form (fits VARCHAR(64)).
#[must_use]
pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// Result of a successful enrollment.
#[derive(Debug, Clone)]
pub struct EnrollmentResult {
    pub server: GameServer,
    pub certificate: AgentCertificate,
    /// Signed client certificate for the agent.
    pub cert_pem: String,
    /// The CA trust anchor the agent pins.
    pub ca_cert_pem: String,
    /// Server-scoped demo-upload bearer token (raw — shown once at
    /// enrollment; goes into the permanent MatchZy config, §6.3).
    pub demo_token: String,
}

/// Registry service for game servers.
#[derive(Clone)]
pub struct GameServerRegistryService<S, C, B>
where
    S: GameServerRepository,
    C: AgentCertRepository,
    B: ServerBookingRepository,
{
    server_repo: Arc<S>,
    cert_repo: Arc<C>,
    booking_repo: Arc<B>,
    enrollment_ttl: Duration,
    cert_validity_days: i64,
}

impl<S, C, B> GameServerRegistryService<S, C, B>
where
    S: GameServerRepository,
    C: AgentCertRepository,
    B: ServerBookingRepository,
{
    /// Create a new registry service.
    pub fn new(server_repo: Arc<S>, cert_repo: Arc<C>, booking_repo: Arc<B>) -> Self {
        Self {
            server_repo,
            cert_repo,
            booking_repo,
            enrollment_ttl: Duration::hours(ENROLLMENT_TOKEN_TTL_HOURS),
            cert_validity_days: AGENT_CERT_VALIDITY_DAYS,
        }
    }

    // ------------------------------------------------------------------
    // Registry CRUD
    // ------------------------------------------------------------------

    /// Register a new server (starts `offline` until its agent connects).
    pub async fn register(&self, create: CreateGameServer) -> Result<GameServer, DomainError> {
        if create.name.trim().is_empty() {
            return Err(DomainError::InvalidState("server name is required".into()));
        }
        if create.region.trim().is_empty() {
            return Err(DomainError::InvalidState("region is required".into()));
        }
        if create.gotv_port == Some(create.port) {
            return Err(DomainError::InvalidState(
                "GOTV port must differ from the game port".into(),
            ));
        }
        self.server_repo.create(create).await
    }

    pub async fn get(&self, id: GameServerId) -> Result<GameServer, DomainError> {
        self.server_repo
            .find_by_id(id)
            .await?
            .ok_or(DomainError::GameServerNotFound(id))
    }

    pub async fn list(&self) -> Result<Vec<GameServer>, DomainError> {
        self.server_repo.list_all().await
    }

    pub async fn update(
        &self,
        id: GameServerId,
        update: UpdateGameServer,
    ) -> Result<GameServer, DomainError> {
        let existing = self.get(id).await?;
        if let Some(port) = update.port
            && update.gotv_port.unwrap_or(existing.gotv_port) == Some(port)
        {
            return Err(DomainError::InvalidState(
                "GOTV port must differ from the game port".into(),
            ));
        }
        self.server_repo.update(id, update).await
    }

    /// Remove a server. Refused while it is busy with a match — cancel the
    /// reservation first (the design's confirm-gated admin path).
    pub async fn delete(&self, id: GameServerId) -> Result<(), DomainError> {
        let server = self.get(id).await?;
        if server.status.is_busy() || server.current_match_id.is_some() {
            return Err(DomainError::InvalidState(
                "server is busy with a match; cancel its reservation first".into(),
            ));
        }
        self.server_repo.delete(id).await
    }

    // ------------------------------------------------------------------
    // Enrollment & certificates (§5.3)
    // ------------------------------------------------------------------

    /// Mint a one-time enrollment token for a server.
    ///
    /// Returns the raw token — shown exactly once. Only its hash is stored;
    /// minting again invalidates any previous token.
    pub async fn mint_enrollment_token(
        &self,
        id: GameServerId,
    ) -> Result<(String, DateTime<Utc>), DomainError> {
        let _server = self.get(id).await?;
        let token = generate_enrollment_token();
        let expires_at = Utc::now() + self.enrollment_ttl;
        self.server_repo
            .set_enrollment_token(id, &hash_token(&token), expires_at)
            .await?;
        Ok((token, expires_at))
    }

    /// Exchange a one-time token + CSR for a signed agent certificate.
    ///
    /// Consumes the token; the certificate CN is the server's UUID
    /// regardless of what the CSR requested.
    pub async fn enroll(
        &self,
        raw_token: &str,
        csr_pem: &str,
        ca: &CertificateAuthority,
    ) -> Result<EnrollmentResult, DomainError> {
        let server = self
            .server_repo
            .find_by_enrollment_token_hash(&hash_token(raw_token), Utc::now())
            .await?
            .ok_or_else(|| {
                DomainError::NotAuthorized("invalid or expired enrollment token".into())
            })?;

        let issued: IssuedCertificate =
            ca.sign_csr(csr_pem, &server.id.to_string(), self.cert_validity_days)?;

        let certificate = self
            .cert_repo
            .create(CreateAgentCertificate {
                server_id: server.id,
                serial: issued.serial.clone(),
                fingerprint_sha256: issued.fingerprint_sha256.clone(),
                not_before: issued.not_before,
                not_after: issued.not_after,
            })
            .await?;

        self.server_repo
            .complete_enrollment(server.id, &issued.serial, issued.not_after)
            .await?;

        // Rotate the server-scoped demo-upload token on every enrollment.
        let demo_token = generate_reservation_token();
        self.server_repo
            .set_demo_token(server.id, &hash_token(&demo_token))
            .await?;

        let server = self.get(server.id).await?;
        Ok(EnrollmentResult {
            server,
            certificate,
            cert_pem: issued.cert_pem,
            ca_cert_pem: ca.cert_pem().to_string(),
            demo_token,
        })
    }

    /// Resolve a presented client-cert serial to its server, enforcing
    /// validity window and revocation. The agent-WS auth path (§5.4).
    pub async fn authenticate_agent(&self, serial: &str) -> Result<GameServer, DomainError> {
        let cert = self
            .cert_repo
            .find_by_serial(serial)
            .await?
            .ok_or_else(|| DomainError::NotAuthorized("unknown agent certificate".into()))?;
        if !cert.is_valid(Utc::now()) {
            return Err(DomainError::NotAuthorized(
                "agent certificate expired or revoked".into(),
            ));
        }
        self.get(cert.server_id).await
    }

    /// Revoke all certificates for a server (drops its agent's access
    /// immediately; re-enrollment requires a fresh token).
    pub async fn revoke_agent(&self, id: GameServerId) -> Result<u64, DomainError> {
        let _server = self.get(id).await?;
        let revoked = self.cert_repo.revoke_for_server(id).await?;
        self.server_repo.clear_agent_cert(id).await?;
        Ok(revoked)
    }

    // ------------------------------------------------------------------
    // Heartbeats (§6.7 busyness rules)
    // ------------------------------------------------------------------

    /// Apply an agent heartbeat and derive the server's status.
    ///
    /// `has_live_reservation` is supplied by the caller (reservation lookup
    /// lands in Phase 2; Phase 1 always passes `false`).
    pub async fn record_heartbeat(
        &self,
        id: GameServerId,
        heartbeat: HeartbeatUpdate,
        has_live_reservation: bool,
    ) -> Result<GameServer, DomainError> {
        let server = self.get(id).await?;

        let status = if heartbeat.rcon_ok {
            match heartbeat.gamestate {
                // RCON up but no parseable get5_status: keep what we had,
                // unless we were offline/error — then we're at least reachable.
                None => match server.status {
                    GameServerStatus::Offline | GameServerStatus::Error => {
                        GameServerStatus::Available
                    }
                    other => other,
                },
                Some(gs) if gs.is_idle() => GameServerStatus::Available,
                // A match is loaded. Ours → the reservation pipeline owns the
                // reserved/configuring/in_match distinction; don't fight it.
                Some(_) if has_live_reservation => match server.status {
                    GameServerStatus::Offline
                    | GameServerStatus::Error
                    | GameServerStatus::Available
                    | GameServerStatus::BusyExternal => GameServerStatus::InMatch,
                    busy => busy,
                },
                // A match the portal didn't set up: out-of-band pug (§6.7).
                Some(_) => GameServerStatus::BusyExternal,
            }
        } else {
            GameServerStatus::Error
        };

        self.server_repo
            .record_heartbeat(
                id,
                RecordHeartbeat {
                    agent_version: heartbeat.agent_version,
                    gamestate: heartbeat.gamestate,
                    status,
                    at: Utc::now(),
                },
            )
            .await?;
        self.get(id).await
    }

    /// Store a (hashed) server-scoped demo token (enrollment mints one;
    /// this exposes rotation).
    pub async fn set_demo_token(
        &self,
        id: GameServerId,
        token_hash: &str,
    ) -> Result<(), DomainError> {
        self.server_repo.set_demo_token(id, token_hash).await
    }

    /// Resolve a demo-upload token hash to its server (§6.5 demo auth).
    pub async fn find_by_demo_token(
        &self,
        token_hash: &str,
    ) -> Result<Option<GameServer>, DomainError> {
        self.server_repo.find_by_demo_token_hash(token_hash).await
    }

    /// Set a server's status directly (reservation pipeline transitions:
    /// configuring/in_match are owned by the match-server flow, §6.4).
    pub async fn set_server_status(
        &self,
        id: GameServerId,
        status: GameServerStatus,
    ) -> Result<(), DomainError> {
        self.server_repo.set_status(id, status).await
    }

    /// Mark a server offline when its agent socket closes.
    pub async fn mark_disconnected(&self, id: GameServerId) -> Result<(), DomainError> {
        self.server_repo
            .set_status(id, GameServerStatus::Offline)
            .await
    }

    /// Mark servers with stale heartbeats offline. Returns transitioned ids.
    pub async fn sweep_stale(&self, now: DateTime<Utc>) -> Result<Vec<GameServerId>, DomainError> {
        self.server_repo
            .mark_stale_offline(now - Duration::seconds(HEARTBEAT_STALENESS_SECS))
            .await
    }

    // ------------------------------------------------------------------
    // Bookings (§6.7)
    // ------------------------------------------------------------------

    pub async fn create_booking(
        &self,
        booking: CreateServerBooking,
    ) -> Result<ServerBooking, DomainError> {
        if booking.ends_at <= booking.starts_at {
            return Err(DomainError::InvalidState(
                "booking must end after it starts".into(),
            ));
        }
        if booking.ends_at <= Utc::now() {
            return Err(DomainError::InvalidState(
                "booking window is entirely in the past".into(),
            ));
        }
        let _server = self.get(booking.server_id).await?;
        self.booking_repo.create(booking).await
    }

    /// Current and upcoming bookings for a server.
    pub async fn list_bookings(
        &self,
        server_id: GameServerId,
    ) -> Result<Vec<ServerBooking>, DomainError> {
        let _server = self.get(server_id).await?;
        self.booking_repo
            .list_for_server(server_id, Utc::now())
            .await
    }

    pub async fn delete_booking(&self, id: ServerBookingId) -> Result<(), DomainError> {
        self.booking_repo
            .find_by_id(id)
            .await?
            .ok_or(DomainError::ServerBookingNotFound(id))?;
        self.booking_repo.delete(id).await
    }
}
