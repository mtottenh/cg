-- Migration: Create bans table
-- Description: User bans with scoping and lift tracking

CREATE TABLE bans (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Target user
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    -- Ban type and reason
    ban_type VARCHAR(32) NOT NULL,
    reason TEXT NOT NULL,

    -- Scoping (for context-specific bans)
    scope_type VARCHAR(32),
    scope_id UUID,

    -- Issue metadata
    issued_by UUID REFERENCES users(id) ON DELETE SET NULL,
    starts_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ends_at TIMESTAMPTZ,

    -- Lift metadata
    lifted_at TIMESTAMPTZ,
    lifted_by UUID REFERENCES users(id) ON DELETE SET NULL,
    lift_reason TEXT,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT bans_check_type CHECK (ban_type IN ('platform', 'matchmaking', 'chat', 'league', 'tournament')),
    CONSTRAINT bans_check_scope CHECK (
        (scope_type IS NULL AND scope_id IS NULL) OR
        (scope_type IS NOT NULL AND scope_id IS NOT NULL)
    ),
    CONSTRAINT bans_check_scope_type CHECK (
        scope_type IS NULL OR scope_type IN ('league', 'tournament', 'team')
    ),
    CONSTRAINT bans_check_dates CHECK (ends_at IS NULL OR ends_at > starts_at)
);

-- Indexes
CREATE INDEX idx_bans_user ON bans(user_id);
CREATE INDEX idx_bans_type ON bans(ban_type);
CREATE INDEX idx_bans_not_lifted ON bans(user_id, ban_type, ends_at)
    WHERE lifted_at IS NULL;
CREATE INDEX idx_bans_scope ON bans(scope_type, scope_id) WHERE scope_type IS NOT NULL;

-- Triggers
CREATE TRIGGER bans_updated_at
    BEFORE UPDATE ON bans
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
