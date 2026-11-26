//! Action item repository for captain pending actions.

use crate::DbPool;
use crate::error::RepositoryError;
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// A pending action item for a captain/player.
#[derive(Debug, sqlx::FromRow)]
pub struct ActionItem {
    /// Type of action required.
    pub action_type: String,
    /// Match ID this action relates to.
    pub match_id: Uuid,
    /// Tournament ID.
    pub tournament_id: Uuid,
    /// Tournament slug for URL construction.
    pub tournament_slug: String,
    /// Tournament name for display.
    pub tournament_name: String,
    /// Human-readable match label (e.g. "Team A vs Team B").
    pub match_label: String,
    /// Optional deadline for this action.
    pub deadline: Option<DateTime<Utc>>,
    /// When the action became available.
    pub created_at: DateTime<Utc>,
    /// The match's agreed start time, if it has one.
    pub scheduled_at: Option<DateTime<Utc>>,
}

/// Repository for fetching captain action items.
#[derive(Clone)]
pub struct ActionItemRepository {
    pool: DbPool,
}

impl ActionItemRepository {
    /// Create a new action item repository.
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Get all pending action items for a player.
    pub async fn list_by_player(
        &self,
        player_id: Uuid,
    ) -> Result<Vec<ActionItem>, RepositoryError> {
        let rows = sqlx::query_as::<_, ActionItem>(
            r"
            WITH my_registrations AS (
                -- `can_act`: the player may schedule, check in and veto for
                -- the registration — the registered player, or the team's
                -- captain, owner or an active delegate. Other roster members
                -- see the match but are not asked to do what they cannot.
                SELECT DISTINCT ON (tr.id)
                    tr.id AS reg_id,
                    tr.tournament_id,
                    (
                        tr.player_id = $1
                        OR ltm.role = 'captain'
                        OR EXISTS (
                            SELECT 1 FROM league_team_seasons lts
                            JOIN league_teams lt ON lt.id = lts.team_id
                            WHERE lts.id = tr.team_season_id AND lt.owner_player_id = $1
                        )
                        OR EXISTS (
                            SELECT 1 FROM veto_delegates vd
                            WHERE vd.team_season_id = tr.team_season_id
                              AND vd.player_id = $1
                              AND vd.revoked_at IS NULL
                              AND (vd.tournament_id IS NULL OR vd.tournament_id = tr.tournament_id)
                        )
                    ) AS can_act
                FROM tournament_registrations tr
                LEFT JOIN league_team_members ltm
                    ON tr.team_season_id = ltm.team_season_id
                    AND ltm.player_id = $1
                    AND ltm.left_at IS NULL
                WHERE tr.player_id = $1 OR ltm.player_id IS NOT NULL
            ),
            my_matches AS (
                SELECT DISTINCT ON (tm.id)
                    tm.id,
                    tm.tournament_id,
                    tm.status,
                    tm.check_in_deadline,
                    tm.schedule_deadline,
                    tm.participant1_name,
                    tm.participant2_name,
                    tm.updated_at,
                    mr.reg_id AS my_registration_id,
                    mr.can_act,
                    tm.participant1_registration_id,
                    tm.participant2_registration_id,
                    tm.participant1_checked_in_at,
                    tm.participant2_checked_in_at,
                    t.slug AS tournament_slug,
                    t.name AS tournament_name,
                    t.scheduling_mode
                FROM tournament_matches tm
                JOIN my_registrations mr
                    ON tm.participant1_registration_id = mr.reg_id
                    OR tm.participant2_registration_id = mr.reg_id
                JOIN tournaments t ON tm.tournament_id = t.id
                WHERE tm.status IN ('ready', 'scheduled', 'checking_in', 'pick_ban', 'in_progress', 'awaiting_result')
            ),
            items AS (
            -- 1. Propose schedule (match ready, self-scheduled, no pending proposal from user)
            SELECT
                'schedule_match'::text AS action_type,
                mm.id AS match_id,
                mm.tournament_id,
                mm.tournament_slug,
                mm.tournament_name,
                COALESCE(mm.participant1_name, 'TBD') || ' vs ' || COALESCE(mm.participant2_name, 'TBD') AS match_label,
                mm.schedule_deadline AS deadline,
                mm.updated_at AS created_at
            FROM my_matches mm
            WHERE mm.status = 'ready'
              AND mm.can_act
              AND mm.scheduling_mode = 'self_scheduled'
              AND NOT EXISTS (
                  SELECT 1 FROM schedule_proposals sp
                  WHERE sp.match_id = mm.id
                    AND sp.status = 'pending'
              )

            UNION ALL

            -- 2. Respond to schedule proposal (opponent proposed, status=pending)
            SELECT
                'respond_proposal'::text,
                mm.id,
                mm.tournament_id,
                mm.tournament_slug,
                mm.tournament_name,
                COALESCE(mm.participant1_name, 'TBD') || ' vs ' || COALESCE(mm.participant2_name, 'TBD'),
                sp.expires_at,
                sp.created_at
            FROM my_matches mm
            JOIN schedule_proposals sp
                ON sp.match_id = mm.id
                AND sp.status = 'pending'
                AND sp.proposed_by_registration_id != mm.my_registration_id
            WHERE mm.status = 'ready'
              AND mm.can_act

            UNION ALL

            -- 2. Check-in required: only while the window is open (the item
            --    carries its deadline), and only for whoever can check in.
            --    A match that merely has a time agreed is not an action yet.
            SELECT
                'check_in'::text,
                mm.id,
                mm.tournament_id,
                mm.tournament_slug,
                mm.tournament_name,
                COALESCE(mm.participant1_name, 'TBD') || ' vs ' || COALESCE(mm.participant2_name, 'TBD'),
                mm.check_in_deadline,
                mm.updated_at
            FROM my_matches mm
            WHERE mm.status = 'checking_in'
              AND mm.can_act
              AND NOT (
                (mm.participant1_registration_id = mm.my_registration_id AND mm.participant1_checked_in_at IS NOT NULL)
                OR
                (mm.participant2_registration_id = mm.my_registration_id AND mm.participant2_checked_in_at IS NOT NULL)
              )

            UNION ALL

            -- 2b. Veto turn: the map pick/ban is on this captain, with the turn
            --     clock as the deadline. The one step with a 30-second deadline
            --     used to be the one step with no action item at all.
            SELECT
                'veto_turn'::text,
                mm.id,
                mm.tournament_id,
                mm.tournament_slug,
                mm.tournament_name,
                COALESCE(mm.participant1_name, 'TBD') || ' vs ' || COALESCE(mm.participant2_name, 'TBD'),
                vs.action_deadline,
                COALESCE(vs.started_at, vs.updated_at)
            FROM my_matches mm
            JOIN veto_sessions vs
                ON vs.match_id = mm.id
                AND vs.status = 'in_progress'
                AND vs.current_team_turn = mm.my_registration_id
            WHERE mm.status = 'pick_ban'
              AND mm.can_act

            UNION ALL

            -- 3. Submit result (no pending claim exists)
            SELECT
                'submit_result'::text,
                mm.id,
                mm.tournament_id,
                mm.tournament_slug,
                mm.tournament_name,
                COALESCE(mm.participant1_name, 'TBD') || ' vs ' || COALESCE(mm.participant2_name, 'TBD'),
                NULL::timestamptz,
                mm.updated_at
            FROM my_matches mm
            WHERE mm.status IN ('in_progress', 'awaiting_result')
              AND NOT EXISTS (
                  SELECT 1 FROM result_claims rc
                  WHERE rc.match_id = mm.id AND rc.status = 'pending'
              )

            UNION ALL

            -- 4. Confirm/dispute result (opponent submitted, pending)
            SELECT
                'confirm_result'::text,
                mm.id,
                mm.tournament_id,
                mm.tournament_slug,
                mm.tournament_name,
                COALESCE(mm.participant1_name, 'TBD') || ' vs ' || COALESCE(mm.participant2_name, 'TBD'),
                rc.auto_confirm_at,
                rc.created_at
            FROM my_matches mm
            JOIN result_claims rc
                ON rc.match_id = mm.id
                AND rc.status = 'pending'
                -- IS DISTINCT FROM: a server-reported claim has no submitter
                -- (NULL), and `!=` silently dropped it for both teams.
                AND rc.submitted_by_registration_id IS DISTINCT FROM mm.my_registration_id
            WHERE mm.status = 'awaiting_result'

            UNION ALL

            -- 5. Acknowledge result review
            SELECT
                'acknowledge_review'::text,
                mm.id,
                mm.tournament_id,
                mm.tournament_slug,
                mm.tournament_name,
                COALESCE(mm.participant1_name, 'TBD') || ' vs ' || COALESCE(mm.participant2_name, 'TBD'),
                NULL::timestamptz,
                rr.created_at
            FROM my_matches mm
            JOIN result_reviews rr
                ON rr.match_id = mm.id
                AND rr.status = 'pending_acknowledgment'
            )
            SELECT items.*, tm.scheduled_at
            FROM items
            JOIN tournament_matches tm ON tm.id = items.match_id
            ORDER BY items.deadline ASC NULLS LAST, items.created_at ASC
            ",
        )
        .bind(player_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }
}
