-- Migration: Add demo_link_ids to result_claims for demo catalog integration
-- This bridges result claims to the demo catalog without duplicating data
--
-- Part of Phase 4.2: Result Claim Demo Bridge

-- Add demo_link_ids column to result_claims
ALTER TABLE result_claims
ADD COLUMN demo_link_ids UUID[] NOT NULL DEFAULT '{}';

COMMENT ON COLUMN result_claims.demo_link_ids IS
    'Array of demo match link IDs from demo_match_links table. Separate from evidence_ids to maintain clean domain boundaries.';

-- Index for efficient lookup of claims by demo link
CREATE INDEX idx_result_claims_demo_links ON result_claims USING gin(demo_link_ids);
