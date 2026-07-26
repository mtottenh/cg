-- P-80: "Assign to Me" recorded no assignee — the disputes table had no
-- column for one, so the button flipped status to under_review and posted a
-- system message while two admins could both "take" the same dispute and
-- neither the queue nor the modal showed ownership.
ALTER TABLE disputes
    ADD COLUMN assigned_to_user_id UUID REFERENCES users(id);

COMMENT ON COLUMN disputes.assigned_to_user_id IS 'Admin who took the dispute for review; NULL until assigned';
