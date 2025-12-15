-- Evidence system for match results
-- Stores demo files, screenshots, videos, and links attached to matches

CREATE TABLE match_evidence (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- What this evidence is for
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,
    game_number INTEGER,  -- NULL for match-level evidence

    -- Type and source
    evidence_type VARCHAR(32) NOT NULL,
    evidence_source VARCHAR(32) NOT NULL,

    -- Metadata
    name VARCHAR(256) NOT NULL,
    description TEXT,
    file_size_bytes BIGINT,
    mime_type VARCHAR(128),

    -- Storage location
    storage_type VARCHAR(32) NOT NULL,  -- 's3', 'url', 'inline'
    storage_path VARCHAR(512),          -- S3 key or URL
    storage_bucket VARCHAR(128),        -- S3 bucket name

    -- Plugin-provided metadata
    plugin_metadata JSONB NOT NULL DEFAULT '{}',

    -- Validation
    validated BOOLEAN NOT NULL DEFAULT false,
    validated_at TIMESTAMPTZ,
    validation_result JSONB,

    -- Uploaded by
    uploaded_by_registration_id UUID REFERENCES tournament_registrations(id),
    uploaded_by_user_id UUID REFERENCES users(id),

    -- Discovery source (for plugin-discovered evidence)
    discovered_by_plugin VARCHAR(64),
    discovered_at TIMESTAMPTZ,

    -- Status
    status VARCHAR(32) NOT NULL DEFAULT 'active',

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,

    -- Constraints
    CONSTRAINT match_evidence_check_type CHECK (evidence_type IN (
        'demo', 'screenshot', 'video', 'link', 'server_log'
    )),
    CONSTRAINT match_evidence_check_source CHECK (evidence_source IN (
        'manual_upload', 'plugin_discovery', 'game_server', 'external_api'
    )),
    CONSTRAINT match_evidence_check_storage CHECK (storage_type IN (
        's3', 'url', 'inline'
    )),
    CONSTRAINT match_evidence_check_status CHECK (status IN (
        'active', 'expired', 'deleted', 'quarantined'
    ))
);

CREATE INDEX idx_match_evidence_match ON match_evidence(match_id);
CREATE INDEX idx_match_evidence_match_game ON match_evidence(match_id, game_number);
CREATE INDEX idx_match_evidence_type ON match_evidence(evidence_type);
CREATE INDEX idx_match_evidence_expires ON match_evidence(expires_at)
    WHERE expires_at IS NOT NULL AND status = 'active';

CREATE TRIGGER match_evidence_updated_at
    BEFORE UPDATE ON match_evidence
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE match_evidence IS 'Evidence files and links for match results';

-- Evidence access log for audit
CREATE TABLE evidence_access_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    evidence_id UUID NOT NULL REFERENCES match_evidence(id) ON DELETE CASCADE,
    accessed_by_user_id UUID REFERENCES users(id),
    access_type VARCHAR(32) NOT NULL,  -- 'view', 'download', 'share'
    ip_address INET,
    user_agent TEXT,
    accessed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_evidence_access_log_evidence ON evidence_access_log(evidence_id);
CREATE INDEX idx_evidence_access_log_user ON evidence_access_log(accessed_by_user_id);

COMMENT ON TABLE evidence_access_log IS 'Audit log of evidence access';
