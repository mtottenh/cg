-- Migration: Create permissions table
-- Description: Permission definitions for RBAC system

CREATE TABLE permissions (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Permission Identity
    name VARCHAR(128) NOT NULL UNIQUE,
    display_name VARCHAR(256) NOT NULL,
    description TEXT,

    -- Categorization
    category VARCHAR(64) NOT NULL DEFAULT 'general',

    -- Flags
    is_dangerous BOOLEAN NOT NULL DEFAULT FALSE,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE INDEX idx_permissions_category ON permissions(category);
CREATE INDEX idx_permissions_name ON permissions(name);
