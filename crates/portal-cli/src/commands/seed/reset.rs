//! Reset (delete) all seeded data.

use anyhow::{Context, Result};
use portal_db::PgPool;

use super::scenario::{self, TEAMS};
use crate::output::{info, success, warn};

/// Delete all seeded data in reverse FK order.
pub async fn reset_seed_data(pool: &PgPool) -> Result<()> {
    let league_id = scenario::league_id();
    let premier_league_id = scenario::premier_league_id();
    let league_ids = vec![league_id, premier_league_id];
    let tournament_id = scenario::tournament_id();
    let tournament_stage_id = scenario::tournament_stage_id();
    let team_ids: Vec<uuid::Uuid> = TEAMS.iter().map(|t| t.team_id()).collect();
    let user_ids = scenario::all_user_ids();
    let player_ids = scenario::all_player_ids();

    let tournament_2_id = scenario::tournament_2_id();

    let mut tx = pool.begin().await.context("Failed to start transaction")?;

    // Tournament 2 matches
    info("Removing tournament 2 matches...");
    let r = sqlx::query("DELETE FROM tournament_matches WHERE tournament_id = $1")
        .bind(tournament_2_id)
        .execute(&mut *tx)
        .await?;
    info(&format!("  Deleted {} tournament 2 match(es)", r.rows_affected()));

    // Tournament 2 brackets
    info("Removing tournament 2 brackets...");
    let r = sqlx::query("DELETE FROM tournament_brackets WHERE tournament_id = $1")
        .bind(tournament_2_id)
        .execute(&mut *tx)
        .await?;
    info(&format!("  Deleted {} tournament 2 bracket(s)", r.rows_affected()));

    // Tournament 2 stages
    info("Removing tournament 2 stages...");
    let r = sqlx::query("DELETE FROM tournament_stages WHERE tournament_id = $1")
        .bind(tournament_2_id)
        .execute(&mut *tx)
        .await?;
    info(&format!("  Deleted {} tournament 2 stage(s)", r.rows_affected()));

    // Tournament 2 registrations
    info("Removing tournament 2 registrations...");
    let r = sqlx::query("DELETE FROM tournament_registrations WHERE tournament_id = $1")
        .bind(tournament_2_id)
        .execute(&mut *tx)
        .await?;
    info(&format!("  Deleted {} tournament 2 registration(s)", r.rows_affected()));

    // Tournament 2
    info("Removing tournament 2...");
    let r = sqlx::query("DELETE FROM tournaments WHERE id = $1")
        .bind(tournament_2_id)
        .execute(&mut *tx)
        .await?;
    info(&format!("  Deleted {} tournament(s)", r.rows_affected()));

    // Rating histories
    info("Removing player rating histories...");
    let r = sqlx::query("DELETE FROM player_rating_history WHERE player_id = ANY($1)")
        .bind(&player_ids)
        .execute(&mut *tx)
        .await?;
    info(&format!("  Deleted {} rating history entries", r.rows_affected()));

    // Availability windows
    info("Removing availability windows...");
    let r = sqlx::query("DELETE FROM availability_windows WHERE player_id = ANY($1)")
        .bind(&player_ids)
        .execute(&mut *tx)
        .await?;
    info(&format!("  Deleted {} availability window(s)", r.rows_affected()));

    // Tournament stage
    info("Removing tournament stage...");
    let r = sqlx::query("DELETE FROM tournament_stages WHERE id = $1")
        .bind(tournament_stage_id)
        .execute(&mut *tx)
        .await?;
    info(&format!("  Deleted {} tournament stage(s)", r.rows_affected()));

    // Tournament registrations (if any)
    info("Removing tournament registrations...");
    let r = sqlx::query("DELETE FROM tournament_registrations WHERE tournament_id = $1")
        .bind(tournament_id)
        .execute(&mut *tx)
        .await?;
    info(&format!("  Deleted {} registration(s)", r.rows_affected()));

    // Tournament
    info("Removing tournament...");
    let r = sqlx::query("DELETE FROM tournaments WHERE id = $1")
        .bind(tournament_id)
        .execute(&mut *tx)
        .await?;
    info(&format!("  Deleted {} tournament(s)", r.rows_affected()));

    // Clean up any remaining tournament data for seeded teams
    // (e.g. tournaments created via API that reference seeded teams).
    // Deleting matches cascades to schedule_proposals, result_claims,
    // evidence, disputes, forfeits, veto sessions, sagas, etc.
    info("Cleaning up remaining tournament data for seeded teams...");
    let r = sqlx::query(
        "DELETE FROM tournament_matches WHERE participant1_registration_id IN (
            SELECT tr.id FROM tournament_registrations tr
            JOIN league_team_seasons lts ON tr.team_season_id = lts.id
            WHERE lts.team_id = ANY($1)
        ) OR participant2_registration_id IN (
            SELECT tr.id FROM tournament_registrations tr
            JOIN league_team_seasons lts ON tr.team_season_id = lts.id
            WHERE lts.team_id = ANY($1)
        )",
    )
    .bind(&team_ids)
    .execute(&mut *tx)
    .await?;
    if r.rows_affected() > 0 {
        info(&format!(
            "  Deleted {} remaining match(es)",
            r.rows_affected()
        ));
    }

    // Delete remaining tournament registrations for our teams
    let r = sqlx::query(
        "DELETE FROM tournament_registrations WHERE team_season_id IN (
            SELECT id FROM league_team_seasons WHERE team_id = ANY($1)
        )",
    )
    .bind(&team_ids)
    .execute(&mut *tx)
    .await?;
    if r.rows_affected() > 0 {
        info(&format!(
            "  Deleted {} remaining registration(s)",
            r.rows_affected()
        ));
    }

    // Team members (via team_season_id from league_team_seasons)
    info("Removing team members...");
    let r = sqlx::query(
        "DELETE FROM league_team_members WHERE team_season_id IN (
            SELECT id FROM league_team_seasons WHERE team_id = ANY($1)
        )",
    )
    .bind(&team_ids)
    .execute(&mut *tx)
    .await?;
    info(&format!("  Deleted {} team member(s)", r.rows_affected()));

    // Team invitations
    let r = sqlx::query(
        "DELETE FROM league_team_invitations WHERE team_season_id IN (
            SELECT id FROM league_team_seasons WHERE team_id = ANY($1)
        )",
    )
    .bind(&team_ids)
    .execute(&mut *tx)
    .await?;
    if r.rows_affected() > 0 {
        info(&format!("  Deleted {} team invitation(s)", r.rows_affected()));
    }

    // Team seasons
    info("Removing team-season registrations...");
    let r = sqlx::query("DELETE FROM league_team_seasons WHERE team_id = ANY($1)")
        .bind(&team_ids)
        .execute(&mut *tx)
        .await?;
    info(&format!(
        "  Deleted {} team-season registration(s)",
        r.rows_affected()
    ));

    // Teams
    info("Removing teams...");
    let r = sqlx::query("DELETE FROM league_teams WHERE id = ANY($1)")
        .bind(&team_ids)
        .execute(&mut *tx)
        .await?;
    info(&format!("  Deleted {} team(s)", r.rows_affected()));

    // Premier league invitations/applications
    info("Removing league invitations...");
    let r = sqlx::query("DELETE FROM league_invitations WHERE league_id = ANY($1)")
        .bind(&league_ids)
        .execute(&mut *tx)
        .await?;
    if r.rows_affected() > 0 {
        info(&format!("  Deleted {} league invitation(s)", r.rows_affected()));
    }

    // Clear league current_season_id before deleting seasons
    sqlx::query("UPDATE leagues SET current_season_id = NULL WHERE id = ANY($1)")
        .bind(&league_ids)
        .execute(&mut *tx)
        .await?;

    // Seasons (auto-created by trigger, delete by league_id)
    info("Removing seasons...");
    let r = sqlx::query("DELETE FROM league_seasons WHERE league_id = ANY($1)")
        .bind(&league_ids)
        .execute(&mut *tx)
        .await?;
    info(&format!("  Deleted {} season(s)", r.rows_affected()));

    // League members
    info("Removing league members...");
    let r = sqlx::query("DELETE FROM league_members WHERE league_id = ANY($1)")
        .bind(&league_ids)
        .execute(&mut *tx)
        .await?;
    info(&format!("  Deleted {} league member(s)", r.rows_affected()));

    // Leagues
    info("Removing leagues...");
    let r = sqlx::query("DELETE FROM leagues WHERE id = ANY($1)")
        .bind(&league_ids)
        .execute(&mut *tx)
        .await?;
    info(&format!("  Deleted {} league(s)", r.rows_affected()));

    // Clean up any remaining tournaments created by seeded users
    info("Cleaning up remaining tournaments by seeded users...");
    let r = sqlx::query(
        "DELETE FROM tournament_stages WHERE tournament_id IN (
            SELECT id FROM tournaments WHERE created_by = ANY($1)
        )",
    )
    .bind(&user_ids)
    .execute(&mut *tx)
    .await?;
    if r.rows_affected() > 0 {
        info(&format!("  Deleted {} stage(s)", r.rows_affected()));
    }
    let r = sqlx::query(
        "DELETE FROM tournament_registrations WHERE tournament_id IN (
            SELECT id FROM tournaments WHERE created_by = ANY($1)
        )",
    )
    .bind(&user_ids)
    .execute(&mut *tx)
    .await?;
    if r.rows_affected() > 0 {
        info(&format!("  Deleted {} registration(s)", r.rows_affected()));
    }
    let r = sqlx::query("DELETE FROM tournaments WHERE created_by = ANY($1)")
        .bind(&user_ids)
        .execute(&mut *tx)
        .await?;
    if r.rows_affected() > 0 {
        info(&format!("  Deleted {} tournament(s)", r.rows_affected()));
    }

    // Clean up any remaining leagues created by seeded users
    info("Cleaning up remaining leagues by seeded users...");
    // First clean tournament data for teams in these leagues (FK chain:
    // leagues → league_teams → league_team_seasons → tournament_registrations,
    // but schedule_proposals blocks registration cascade-delete)
    let r = sqlx::query(
        "DELETE FROM tournament_matches WHERE participant1_registration_id IN (
            SELECT tr.id FROM tournament_registrations tr
            JOIN league_team_seasons lts ON tr.team_season_id = lts.id
            JOIN league_teams lt ON lts.team_id = lt.id
            JOIN leagues l ON lt.league_id = l.id
            WHERE l.created_by = ANY($1)
        ) OR participant2_registration_id IN (
            SELECT tr.id FROM tournament_registrations tr
            JOIN league_team_seasons lts ON tr.team_season_id = lts.id
            JOIN league_teams lt ON lts.team_id = lt.id
            JOIN leagues l ON lt.league_id = l.id
            WHERE l.created_by = ANY($1)
        )",
    )
    .bind(&user_ids)
    .execute(&mut *tx)
    .await?;
    if r.rows_affected() > 0 {
        info(&format!("  Deleted {} match(es)", r.rows_affected()));
    }
    let r = sqlx::query(
        "DELETE FROM tournament_registrations WHERE team_season_id IN (
            SELECT lts.id FROM league_team_seasons lts
            JOIN league_teams lt ON lts.team_id = lt.id
            JOIN leagues l ON lt.league_id = l.id
            WHERE l.created_by = ANY($1)
        )",
    )
    .bind(&user_ids)
    .execute(&mut *tx)
    .await?;
    if r.rows_affected() > 0 {
        info(&format!("  Deleted {} registration(s)", r.rows_affected()));
    }
    sqlx::query("UPDATE leagues SET current_season_id = NULL WHERE created_by = ANY($1)")
        .bind(&user_ids)
        .execute(&mut *tx)
        .await?;
    let r = sqlx::query("DELETE FROM leagues WHERE created_by = ANY($1)")
        .bind(&user_ids)
        .execute(&mut *tx)
        .await?;
    if r.rows_affected() > 0 {
        info(&format!("  Deleted {} league(s)", r.rows_affected()));
    }

    // Clean up remaining references to seeded users/players in non-cascading tables.
    // Most match-child rows were already cascade-deleted when we deleted tournament
    // matches/stages above, but seeded users may have acted in non-seeded contexts.
    info("Cleaning up remaining user/player references...");

    // DELETE rows where seeded user is the primary actor (NOT NULL FK, can't SET NULL)
    for stmt in [
        "DELETE FROM dispute_messages WHERE author_user_id = ANY($1)",
        "DELETE FROM disputes WHERE disputed_by_user_id = ANY($1)",
        "DELETE FROM result_claims WHERE submitted_by_user_id = ANY($1)",
        "DELETE FROM schedule_proposals WHERE proposed_by_user_id = ANY($1)",
        "DELETE FROM veto_lobby_messages WHERE author_user_id = ANY($1)",
        "DELETE FROM veto_delegates WHERE delegated_by_user_id = ANY($1)",
        "DELETE FROM league_members WHERE user_id = ANY($1)",
        "DELETE FROM league_invitations WHERE user_id = ANY($1)",
        "DELETE FROM api_keys WHERE created_by = ANY($1)",
    ] {
        sqlx::query(stmt)
            .bind(&user_ids)
            .execute(&mut *tx)
            .await?;
    }

    // SET NULL on nullable audit/secondary columns that reference seeded users
    for stmt in [
        "UPDATE demos SET categorized_by_user_id = NULL WHERE categorized_by_user_id = ANY($1)",
        "UPDATE demos SET hidden_by_user_id = NULL WHERE hidden_by_user_id = ANY($1)",
        "UPDATE demo_match_links SET linked_by_user_id = NULL WHERE linked_by_user_id = ANY($1)",
        "UPDATE match_evidence SET uploaded_by_user_id = NULL WHERE uploaded_by_user_id = ANY($1)",
        "UPDATE evidence_access_log SET accessed_by_user_id = NULL WHERE accessed_by_user_id = ANY($1)",
        "UPDATE result_claims SET confirmed_by_user_id = NULL WHERE confirmed_by_user_id = ANY($1)",
        "UPDATE disputes SET resolved_by_user_id = NULL WHERE resolved_by_user_id = ANY($1)",
        "UPDATE forfeit_records SET triggered_by_user_id = NULL WHERE triggered_by_user_id = ANY($1)",
        "UPDATE match_status_log SET triggered_by_user_id = NULL WHERE triggered_by_user_id = ANY($1)",
        "UPDATE schedule_proposals SET responded_by_user_id = NULL WHERE responded_by_user_id = ANY($1)",
        "UPDATE veto_actions SET performed_by_user_id = NULL WHERE performed_by_user_id = ANY($1)",
        "UPDATE result_reviews SET captain1_acknowledged_by_user_id = NULL WHERE captain1_acknowledged_by_user_id = ANY($1)",
        "UPDATE result_reviews SET captain2_acknowledged_by_user_id = NULL WHERE captain2_acknowledged_by_user_id = ANY($1)",
        "UPDATE result_reviews SET reviewed_by_user_id = NULL WHERE reviewed_by_user_id = ANY($1)",
        "UPDATE veto_delegates SET revoked_by_user_id = NULL WHERE revoked_by_user_id = ANY($1)",
        "UPDATE league_invitations SET invited_by = NULL WHERE invited_by = ANY($1)",
        "UPDATE league_invitations SET responded_by = NULL WHERE responded_by = ANY($1)",
        "UPDATE tournament_matches SET participant1_checked_in_by = NULL WHERE participant1_checked_in_by = ANY($1)",
        "UPDATE tournament_matches SET participant2_checked_in_by = NULL WHERE participant2_checked_in_by = ANY($1)",
        "UPDATE tournament_matches SET dispute_resolved_by = NULL WHERE dispute_resolved_by = ANY($1)",
        "UPDATE tournament_registrations SET checked_in_by = NULL WHERE checked_in_by = ANY($1)",
    ] {
        sqlx::query(stmt)
            .bind(&user_ids)
            .execute(&mut *tx)
            .await?;
    }

    // User roles (seeded users only)
    info("Removing user roles...");
    let r = sqlx::query("DELETE FROM user_roles WHERE user_id = ANY($1)")
        .bind(&user_ids)
        .execute(&mut *tx)
        .await?;
    info(&format!("  Deleted {} user role(s)", r.rows_affected()));

    // Clean up player references in non-cascading tables before deleting players
    for stmt in [
        "DELETE FROM league_team_members WHERE player_id = ANY($1)",
        "DELETE FROM league_team_invitations WHERE player_id = ANY($1)",
        "DELETE FROM league_season_participants WHERE player_id = ANY($1)",
        "DELETE FROM veto_delegates WHERE player_id = ANY($1)",
        "DELETE FROM player_rating_history WHERE player_id = ANY($1)",
    ] {
        sqlx::query(stmt)
            .bind(&player_ids)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query(
        "UPDATE entity_changes SET changed_by = NULL WHERE changed_by = ANY($1)",
    )
    .bind(&player_ids)
    .execute(&mut *tx)
    .await?;

    // Players
    info("Removing players...");
    let r = sqlx::query("DELETE FROM players WHERE id = ANY($1)")
        .bind(&player_ids)
        .execute(&mut *tx)
        .await?;
    info(&format!("  Deleted {} player(s)", r.rows_affected()));

    // Users
    info("Removing users...");
    let r = sqlx::query("DELETE FROM users WHERE id = ANY($1)")
        .bind(&user_ids)
        .execute(&mut *tx)
        .await?;
    info(&format!("  Deleted {} user(s)", r.rows_affected()));

    tx.commit().await.context("Failed to commit transaction")?;

    println!();
    success("All seed data removed.");
    warn("Note: Any additional data created through the API referencing seed entities may have been cascade-deleted.");

    Ok(())
}
