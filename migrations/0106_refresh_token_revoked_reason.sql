-- Why a refresh token was revoked.
--
-- Rotation revokes the presented token. Two tabs sharing the session cookie
-- can both refresh with it at once: one rotates it, the other then presents a
-- token that was revoked a moment ago. Treating that as theft revoked every
-- token the user had, logging them out of every tab and device. With the
-- reason recorded, a token presented shortly after it was rotated is told to
-- retry instead; every other revoked token keeps the revoke-all response.
--
-- NULL for rows revoked before this column existed: they are treated as not
-- rotated, which keeps the strict response for them.
ALTER TABLE refresh_tokens
    ADD COLUMN revoked_reason VARCHAR(16)
        CONSTRAINT refresh_tokens_check_revoked_reason
        CHECK (revoked_reason IS NULL
               OR revoked_reason IN ('rotated', 'logout', 'revoke_all', 'expired'));
