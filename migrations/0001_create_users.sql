-- Migration: Create users table
-- Description: Primary user account table for authentication

-- Helper function for updated_at timestamps
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TABLE users (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Identity
    username VARCHAR(32) NOT NULL,
    email VARCHAR(255) NOT NULL,
    email_verified BOOLEAN NOT NULL DEFAULT FALSE,
    email_verified_at TIMESTAMPTZ,

    -- Authentication
    password_hash VARCHAR(255),
    password_changed_at TIMESTAMPTZ,

    -- Two-Factor Authentication
    two_factor_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    two_factor_secret VARCHAR(255),
    two_factor_backup_codes JSONB,

    -- Account Status
    status VARCHAR(20) NOT NULL DEFAULT 'active',
    status_reason TEXT,
    status_changed_at TIMESTAMPTZ,

    -- Metadata
    locale VARCHAR(10) DEFAULT 'en-US',
    timezone VARCHAR(64) DEFAULT 'UTC',

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_login_at TIMESTAMPTZ,

    -- Constraints
    CONSTRAINT users_username_unique UNIQUE (username),
    CONSTRAINT users_email_unique UNIQUE (email),
    CONSTRAINT users_check_status CHECK (status IN (
        'active', 'inactive', 'suspended', 'banned', 'pending_verification'
    )),
    CONSTRAINT users_check_username_format CHECK (
        username ~ '^[a-zA-Z0-9_-]{3,32}$'
    ),
    CONSTRAINT users_check_email_format CHECK (
        email ~ '^[^@]+@[^@]+\.[^@]+$'
    )
);

-- Indexes
CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_username ON users(lower(username));
CREATE INDEX idx_users_status ON users(status) WHERE status != 'active';
CREATE INDEX idx_users_created_at ON users(created_at DESC);

-- Triggers
CREATE TRIGGER users_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();
