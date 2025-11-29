-- Migration: Create user_roles table
-- Description: Role assignments to users with optional scoping

CREATE TABLE user_roles (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Relationships
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_id UUID NOT NULL REFERENCES roles(id) ON DELETE CASCADE,

    -- Scoping (for contextual roles like team/league roles)
    scope_type VARCHAR(32),
    scope_id UUID,

    -- Grant metadata
    granted_by UUID REFERENCES users(id) ON DELETE SET NULL,
    granted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Expiration (for temporary roles)
    expires_at TIMESTAMPTZ,

    -- Revocation
    revoked_at TIMESTAMPTZ,
    revoked_by UUID REFERENCES users(id) ON DELETE SET NULL,

    -- Constraints
    CONSTRAINT user_roles_check_scope CHECK (
        (scope_type IS NULL AND scope_id IS NULL) OR
        (scope_type IS NOT NULL AND scope_id IS NOT NULL)
    ),
    CONSTRAINT user_roles_check_scope_type CHECK (
        scope_type IS NULL OR scope_type IN ('team', 'league', 'tournament', 'match')
    )
);

-- Indexes
CREATE INDEX idx_user_roles_user ON user_roles(user_id);
CREATE INDEX idx_user_roles_role ON user_roles(role_id);
CREATE INDEX idx_user_roles_scope ON user_roles(scope_type, scope_id) WHERE scope_type IS NOT NULL;
CREATE INDEX idx_user_roles_not_revoked ON user_roles(user_id, role_id, expires_at)
    WHERE revoked_at IS NULL;

-- Unique constraint for non-scoped roles (user can have each role once globally)
CREATE UNIQUE INDEX idx_user_roles_unique_global ON user_roles(user_id, role_id)
    WHERE scope_type IS NULL AND revoked_at IS NULL;

-- Unique constraint for scoped roles (user can have each role once per scope)
CREATE UNIQUE INDEX idx_user_roles_unique_scoped ON user_roles(user_id, role_id, scope_type, scope_id)
    WHERE scope_type IS NOT NULL AND revoked_at IS NULL;
