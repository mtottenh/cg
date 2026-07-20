-- Migration: 0069_schedule_proposal_rejection_reason.sql
-- Description: Optional reason recorded when a schedule proposal is rejected.
-- The existing `notes` column belongs to the proposer; the rejection reason
-- is authored by the responder, so it gets its own column.

ALTER TABLE schedule_proposals ADD COLUMN rejection_reason TEXT;

COMMENT ON COLUMN schedule_proposals.rejection_reason IS 'Optional reason provided by the responder when rejecting the proposal';
