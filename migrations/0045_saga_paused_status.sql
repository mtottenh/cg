-- Add 'paused' status to saga executions for review workflow integration
ALTER TABLE saga_executions DROP CONSTRAINT saga_executions_check_status;
ALTER TABLE saga_executions ADD CONSTRAINT saga_executions_check_status CHECK (status IN (
    'pending', 'running', 'paused', 'completed', 'failed', 'compensating', 'compensated'
));
