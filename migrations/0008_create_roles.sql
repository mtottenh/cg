-- Migration: Create roles table
-- Description: Role definitions for RBAC system

CREATE TABLE roles (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Role Identity
    name VARCHAR(64) NOT NULL UNIQUE,
    display_name VARCHAR(128) NOT NULL,
    description TEXT,

    -- Categorization
    category VARCHAR(32) NOT NULL DEFAULT 'custom',
    priority INTEGER NOT NULL DEFAULT 0,
    color VARCHAR(7),

    -- System Flags
    is_system BOOLEAN NOT NULL DEFAULT FALSE,
    is_default BOOLEAN NOT NULL DEFAULT FALSE,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT roles_check_category CHECK (category IN ('system', 'platform', 'league', 'team', 'custom')),
    CONSTRAINT roles_check_priority CHECK (priority >= 0 AND priority <= 1000)
);

-- Indexes
CREATE INDEX idx_roles_category ON roles(category);
CREATE INDEX idx_roles_priority ON roles(priority DESC);
CREATE INDEX idx_roles_is_default ON roles(is_default) WHERE is_default = TRUE;

-- Triggers
CREATE TRIGGER roles_updated_at
    BEFORE UPDATE ON roles
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
