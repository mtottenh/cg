//! Releasing a roster when its team leaves a season.
//!
//! A seasonal roster seat is what `idx_one_primary_team_per_season` counts,
//! and that index only knows `status = 'active'`. Disbanding, archiving and
//! withdrawing used to change the team or its registration and leave the
//! seats alone, so the players stayed primary members of a team that no
//! longer existed and could not join another one that season — with the
//! index, not the service check, having the last word.
//!
//! The rule here: when a team stops taking part in a season, its seats are
//! released and its entries in cups that have not drawn a bracket are
//! withdrawn, in the same transaction as the status change. Only *live*
//! participation is touched — a registration that is not terminal, in a
//! season that is not completed or cancelled — so finished seasons keep the
//! roster they ended with.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;

use portal_core::DomainError;

/// The team's registrations that still count as taking part.
pub(super) async fn live_team_season_ids(
    conn: &mut PgConnection,
    team_id: uuid::Uuid,
) -> Result<Vec<uuid::Uuid>, DomainError> {
    let rows: Vec<(uuid::Uuid,)> = sqlx::query_as(
        r"
        SELECT lts.id
        FROM league_team_seasons lts
        JOIN league_seasons s ON s.id = lts.season_id
        WHERE lts.team_id = $1
          AND lts.status NOT IN ('eliminated', 'disqualified', 'withdrawn')
          AND s.status NOT IN ('completed', 'cancelled')
        ",
    )
    .bind(team_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| DomainError::Internal(e.to_string()))?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// Withdraw the registrations' entries in cups that have not started.
///
/// Refused when an entry is in a cup that has a bracket: matches reference
/// it, and pulling the roster out from under them leaves a hole nobody can
/// play. Same line `move_to_league` draws. Withdrawn, eliminated,
/// disqualified and no-show entries are already over and do not count.
pub(super) async fn withdraw_open_cup_entries(
    conn: &mut PgConnection,
    team_season_ids: &[uuid::Uuid],
    now: DateTime<Utc>,
) -> Result<(), DomainError> {
    if team_season_ids.is_empty() {
        return Ok(());
    }

    let bracketed: Option<(String,)> = sqlx::query_as(
        r"
        SELECT t.name
        FROM tournament_registrations tr
        JOIN tournaments t ON t.id = tr.tournament_id
        WHERE tr.team_season_id = ANY($1)
          AND tr.status IN ('pending', 'approved', 'checked_in', 'active')
          AND t.status::text NOT IN ('draft', 'published', 'registration', 'cancelled')
        ORDER BY t.name
        LIMIT 1
        ",
    )
    .bind(team_season_ids)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| DomainError::Internal(e.to_string()))?;
    if let Some((name,)) = bracketed {
        return Err(DomainError::InvalidState(format!(
            "team is entered in '{name}', which has a bracket; withdraw it from that cup \
             (or let the cup finish) first"
        )));
    }

    sqlx::query(
        r"
        UPDATE tournament_registrations
        SET status = 'withdrawn', withdrawn_at = $2, updated_at = $2
        WHERE team_season_id = ANY($1)
          AND status IN ('pending', 'approved', 'checked_in', 'active')
        ",
    )
    .bind(team_season_ids)
    .bind(now)
    .execute(&mut *conn)
    .await
    .map_err(|e| DomainError::Internal(e.to_string()))?;
    Ok(())
}

/// End every current seat on the registrations.
///
/// `status` is `'left'` for a release that is final (disband, withdraw) and
/// `'inactive'` for an archive, which restore may undo: it brings back the
/// `'inactive'` seats whose `left_at` is the team's `archived_at`, so `at`
/// must be that same instant.
pub(super) async fn release_seats(
    conn: &mut PgConnection,
    team_season_ids: &[uuid::Uuid],
    status: &str,
    at: DateTime<Utc>,
) -> Result<u64, DomainError> {
    if team_season_ids.is_empty() {
        return Ok(0);
    }
    let done = sqlx::query(
        r"
        UPDATE league_team_members
        SET status = $2, left_at = $3
        WHERE team_season_id = ANY($1) AND left_at IS NULL
        ",
    )
    .bind(team_season_ids)
    .bind(status)
    .bind(at)
    .execute(&mut *conn)
    .await
    .map_err(|e| DomainError::Internal(e.to_string()))?;
    Ok(done.rows_affected())
}
