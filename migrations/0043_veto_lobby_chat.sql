-- Veto lobby chat messages for real-time WebSocket communication
-- Supports team chat (private), all chat (public), admin messages, and system messages

CREATE TABLE veto_lobby_messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,
    veto_session_id UUID REFERENCES veto_sessions(id) ON DELETE SET NULL,

    -- Author information
    author_user_id UUID NOT NULL REFERENCES users(id),
    author_registration_id UUID REFERENCES tournament_registrations(id),

    -- Message content
    message_type VARCHAR(32) NOT NULL,
    content TEXT NOT NULL,

    -- For team chat: which team can see this message
    -- NULL for all/admin/system messages
    team_registration_id UUID REFERENCES tournament_registrations(id),

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT veto_lobby_messages_check_type CHECK (
        message_type IN ('team', 'all', 'admin', 'system')
    ),
    CONSTRAINT veto_lobby_messages_team_required CHECK (
        message_type != 'team' OR team_registration_id IS NOT NULL
    )
);

-- Index for fetching messages by match
CREATE INDEX idx_veto_lobby_messages_match ON veto_lobby_messages(match_id);

-- Index for fetching team-specific messages
CREATE INDEX idx_veto_lobby_messages_team ON veto_lobby_messages(match_id, team_registration_id)
    WHERE team_registration_id IS NOT NULL;

-- Index for chronological ordering
CREATE INDEX idx_veto_lobby_messages_created ON veto_lobby_messages(match_id, created_at);

COMMENT ON TABLE veto_lobby_messages IS 'Chat messages in veto lobby WebSocket sessions';
COMMENT ON COLUMN veto_lobby_messages.message_type IS 'Message visibility: team (private), all (public), admin (staff), system (automated)';
COMMENT ON COLUMN veto_lobby_messages.team_registration_id IS 'For team messages, the registration ID of the team that can see it';
