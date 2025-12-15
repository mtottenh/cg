-- Progression tracking and saga execution state
-- Handles bracket progression, standings updates, and saga orchestration

-- Extend standings table with tiebreaker fields
ALTER TABLE tournament_standings ADD COLUMN IF NOT EXISTS
    head_to_head JSONB NOT NULL DEFAULT '{}';

ALTER TABLE tournament_standings ADD COLUMN IF NOT EXISTS
    tiebreaker_score DECIMAL(10,4) NOT NULL DEFAULT 0;

ALTER TABLE tournament_standings ADD COLUMN IF NOT EXISTS
    is_tied BOOLEAN NOT NULL DEFAULT false;

CREATE INDEX IF NOT EXISTS idx_tournament_standings_position_points
    ON tournament_standings(bracket_id, points DESC, tiebreaker_score DESC);

COMMENT ON COLUMN tournament_standings.head_to_head IS 'Record vs other participants: {registration_id: {wins, losses, draws}}';
COMMENT ON COLUMN tournament_standings.tiebreaker_score IS 'Computed tiebreaker score for ranking';
COMMENT ON COLUMN tournament_standings.is_tied IS 'Whether this participant is tied with others';

-- Progression log tracks match completion progressions
CREATE TABLE progression_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,
    target_match_id UUID REFERENCES tournament_matches(id) ON DELETE SET NULL,
    registration_id UUID NOT NULL REFERENCES tournament_registrations(id),
    progression_type VARCHAR(32) NOT NULL,
    target_position INTEGER,
    saga_id UUID,
    progressed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT progression_log_check_type CHECK (progression_type IN (
        'winner_advance', 'loser_drop', 'loser_eliminate', 'bye_advance'
    )),
    CONSTRAINT progression_log_check_position CHECK (target_position IN (1, 2))
);

CREATE INDEX idx_progression_log_source ON progression_log(source_match_id);
CREATE INDEX idx_progression_log_target ON progression_log(target_match_id);
CREATE INDEX idx_progression_log_saga ON progression_log(saga_id);
CREATE INDEX idx_progression_log_registration ON progression_log(registration_id);

COMMENT ON TABLE progression_log IS 'Audit log of bracket progression events';

-- Saga execution state
CREATE TABLE saga_executions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    saga_type VARCHAR(64) NOT NULL,
    saga_version INTEGER NOT NULL DEFAULT 1,
    tournament_id UUID REFERENCES tournaments(id) ON DELETE SET NULL,
    match_id UUID REFERENCES tournament_matches(id) ON DELETE SET NULL,
    correlation_id VARCHAR(128),
    input_data JSONB NOT NULL,
    current_step INTEGER NOT NULL DEFAULT 0,
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    step_history JSONB NOT NULL DEFAULT '[]',
    last_error TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT saga_executions_check_status CHECK (status IN (
        'pending', 'running', 'completed', 'failed', 'compensating', 'compensated'
    ))
);

CREATE INDEX idx_saga_executions_status ON saga_executions(status);
CREATE INDEX idx_saga_executions_type ON saga_executions(saga_type, status);
CREATE INDEX idx_saga_executions_tournament ON saga_executions(tournament_id)
    WHERE tournament_id IS NOT NULL;
CREATE INDEX idx_saga_executions_match ON saga_executions(match_id)
    WHERE match_id IS NOT NULL;
CREATE INDEX idx_saga_executions_stuck ON saga_executions(started_at)
    WHERE status = 'running';

CREATE TRIGGER saga_executions_updated_at
    BEFORE UPDATE ON saga_executions
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE saga_executions IS 'State tracking for multi-step saga operations';
COMMENT ON COLUMN saga_executions.saga_type IS 'Type of saga (e.g., match_completion)';
COMMENT ON COLUMN saga_executions.step_history IS 'JSON array of step execution records';
COMMENT ON COLUMN saga_executions.correlation_id IS 'Optional correlation ID for tracing';

-- Add progression_log foreign key to saga_executions
ALTER TABLE progression_log
    ADD CONSTRAINT progression_log_saga_fk
    FOREIGN KEY (saga_id) REFERENCES saga_executions(id) ON DELETE SET NULL;
