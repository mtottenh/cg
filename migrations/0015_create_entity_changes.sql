-- Migration: Create entity_changes table
-- Description: Audit trail for entity modifications with revert capability

CREATE TABLE entity_changes (
    -- Primary Key
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Entity Reference
    entity_type VARCHAR(50) NOT NULL,
    entity_id UUID NOT NULL,

    -- Change Details
    change_type VARCHAR(20) NOT NULL,
    field_name VARCHAR(100),
    old_value JSONB,
    new_value JSONB,

    -- Actor
    changed_by UUID NOT NULL REFERENCES players(id) ON DELETE SET NULL,

    -- Revert State
    reverted_at TIMESTAMPTZ,
    reverted_by UUID REFERENCES players(id) ON DELETE SET NULL,
    revert_reason TEXT,

    -- Metadata
    request_id VARCHAR(64),
    ip_address INET,
    user_agent TEXT,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Constraints
    CONSTRAINT entity_changes_check_type CHECK (change_type IN (
        'create', 'update', 'delete', 'revert'
    ))
);

-- Indexes for common queries
CREATE INDEX idx_entity_changes_entity ON entity_changes(entity_type, entity_id);
CREATE INDEX idx_entity_changes_changed_by ON entity_changes(changed_by);
CREATE INDEX idx_entity_changes_created_at ON entity_changes(created_at DESC);
CREATE INDEX idx_entity_changes_field ON entity_changes(entity_type, entity_id, field_name)
    WHERE field_name IS NOT NULL;

-- Composite index for entity history queries
CREATE INDEX idx_entity_changes_history ON entity_changes(entity_type, entity_id, created_at DESC);

-- Comments for documentation
COMMENT ON TABLE entity_changes IS 'Audit trail tracking all entity modifications';
COMMENT ON COLUMN entity_changes.entity_type IS 'Type of entity (team, player, etc.)';
COMMENT ON COLUMN entity_changes.entity_id IS 'UUID of the modified entity';
COMMENT ON COLUMN entity_changes.change_type IS 'Type of change: create, update, delete, revert';
COMMENT ON COLUMN entity_changes.field_name IS 'Name of the modified field (NULL for create/delete)';
COMMENT ON COLUMN entity_changes.old_value IS 'Previous value as JSON (NULL for create)';
COMMENT ON COLUMN entity_changes.new_value IS 'New value as JSON (NULL for delete)';
COMMENT ON COLUMN entity_changes.changed_by IS 'Player who made the change';
COMMENT ON COLUMN entity_changes.reverted_at IS 'When this change was reverted (if applicable)';
COMMENT ON COLUMN entity_changes.reverted_by IS 'Player who reverted the change';
