//! Database seeding for frontend development.
//!
//! Seeds a live database with realistic test data and outputs
//! JWT tokens for various personas (admin, organizer, captains, players).

mod credentials;
mod reset;
pub mod scenario;

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use portal_db::PgPool;

use crate::output::{OutputFormat, info, success};
use chrono::{Duration, NaiveTime, Utc};
use scenario::{PERSONAS, Persona, SEED_PASSWORD, TEAMS};

/// Seed commands for frontend development.
#[derive(Args)]
pub struct SeedCommand {
    #[command(subcommand)]
    command: SeedSubcommand,
}

#[derive(Subcommand)]
enum SeedSubcommand {
    /// Seed full development scenario (idempotent)
    Full {
        /// JWT secret for token generation
        #[arg(long, env = "JWT_SECRET")]
        jwt_secret: String,

        /// Token expiry in days
        #[arg(long, default_value = "30")]
        token_expiry_days: i64,
    },
    /// Remove all seeded data
    Reset,
    /// Show credentials for existing seeded users
    Credentials {
        /// JWT secret for token generation
        #[arg(long, env = "JWT_SECRET")]
        jwt_secret: String,

        /// Token expiry in days
        #[arg(long, default_value = "30")]
        token_expiry_days: i64,
    },
}

impl SeedCommand {
    pub async fn execute(&self, pool: &PgPool, format: OutputFormat) -> Result<()> {
        match &self.command {
            SeedSubcommand::Full {
                jwt_secret,
                token_expiry_days,
            } => {
                seed_full(pool).await?;
                credentials::print_credentials(pool, jwt_secret, *token_expiry_days, format).await
            }
            SeedSubcommand::Reset => reset::reset_seed_data(pool).await,
            SeedSubcommand::Credentials {
                jwt_secret,
                token_expiry_days,
            } => credentials::print_credentials(pool, jwt_secret, *token_expiry_days, format).await,
        }
    }
}

/// Hash a password with argon2 (same pattern as bootstrap.rs).
fn hash_password(password: &str) -> Result<String> {
    let salt =
        argon2::password_hash::SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    let argon2_hasher = argon2::Argon2::default();
    let hash = argon2::PasswordHasher::hash_password(&argon2_hasher, password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("Failed to hash password: {e}"))?
        .to_string();
    Ok(hash)
}

/// Look up the CS2 game UUID by slug.
async fn get_cs2_game_id(pool: &PgPool) -> Result<uuid::Uuid> {
    let row: (uuid::Uuid,) = sqlx::query_as("SELECT id FROM games WHERE slug = 'cs2'")
        .fetch_one(pool)
        .await
        .context("CS2 game not found. Ensure migrations have run.")?;
    Ok(row.0)
}

/// Seed the full development scenario.
///
/// Schema notes (migration 0026 restructure):
/// - `league_teams` are scoped to a LEAGUE (persistent identity), with `owner_player_id`
/// - `league_team_seasons` is a join table for seasonal participation
/// - `league_team_members` references `team_season_id` (not team_id directly)
/// - A BEFORE INSERT trigger on `leagues` auto-creates "Season 1"
async fn seed_full(pool: &PgPool) -> Result<()> {
    info("Looking up CS2 game...");
    let game_id = get_cs2_game_id(pool).await?;

    info("Hashing password...");
    let password_hash = hash_password(SEED_PASSWORD)?;

    let mut tx = pool.begin().await.context("Failed to start transaction")?;

    // 1. Users
    info("Seeding users...");
    for p in PERSONAS {
        seed_user(&mut tx, p, &password_hash).await?;
    }

    // 2. Players
    info("Seeding player profiles...");
    for p in PERSONAS {
        seed_player(&mut tx, p).await?;
    }

    // 3. Admin role assignment
    info("Assigning admin role...");
    let admin = scenario::persona("admin");
    seed_admin_role(&mut tx, admin.user_id()).await?;

    // 4. League (trigger auto-creates "Season 1" with a random UUID)
    info("Seeding league...");
    let organizer = scenario::persona("organizer");
    seed_league(&mut tx, game_id, organizer.user_id()).await?;

    // 5. League members
    info("Seeding league members...");
    seed_league_member(&mut tx, organizer.user_id(), "admin").await?;
    for p in PERSONAS {
        if p.key != "organizer" {
            seed_league_member(&mut tx, p.user_id(), "member").await?;
        }
    }

    // 5b. Premier League (application-based with entry requirements)
    info("Seeding premier league...");
    seed_premier_league(&mut tx, game_id, organizer.user_id()).await?;

    // Premier league members (only high-rated players qualify)
    seed_premier_league_member(&mut tx, organizer.user_id(), "admin").await?;
    let captain_alpha = scenario::persona("captain_alpha");
    let captain_bravo = scenario::persona("captain_bravo");
    seed_premier_league_member(&mut tx, captain_alpha.user_id(), "member").await?;
    seed_premier_league_member(&mut tx, captain_bravo.user_id(), "member").await?;

    // Pending applications from lower-rated players
    info("Seeding premier league applications...");
    let captain_charlie = scenario::persona("captain_charlie");
    let captain_delta = scenario::persona("captain_delta");
    seed_premier_league_application(
        &mut tx,
        captain_charlie.user_id(),
        Some("I'm improving fast!"),
    )
    .await?;
    seed_premier_league_application(
        &mut tx,
        captain_delta.user_id(),
        Some("Please let me in, I want to improve"),
    )
    .await?;

    // 6. Look up the auto-created season (trigger creates it on league insert)
    info("Looking up auto-created season...");
    let season_id: uuid::Uuid =
        sqlx::query_scalar("SELECT current_season_id FROM leagues WHERE id = $1")
            .bind(scenario::league_id())
            .fetch_one(&mut *tx)
            .await
            .context("Failed to find auto-created season")?;
    info(&format!("  Season ID: {season_id}"));

    // Update season to 'active' status and set team sizes for CS2
    sqlx::query(
        "UPDATE league_seasons SET status = 'active', team_size_min = 5, team_size_max = 5 WHERE id = $1",
    )
    .bind(season_id)
    .execute(&mut *tx)
    .await?;

    // 7. Teams (scoped to league, owner_player_id)
    info("Seeding teams...");
    for team in TEAMS {
        let captain = scenario::persona(team.captain_key);
        seed_team(&mut tx, team, captain.player_id()).await?;
    }

    // 8. Team-season registrations
    info("Seeding team-season registrations...");
    for team in TEAMS {
        seed_team_season(&mut tx, team.team_id(), season_id).await?;
    }

    // 9. Team members (via team_season_id)
    info("Seeding team members...");
    for team in TEAMS {
        // Look up the team_season_id we just created
        let team_season_id: uuid::Uuid = sqlx::query_scalar(
            "SELECT id FROM league_team_seasons WHERE team_id = $1 AND season_id = $2",
        )
        .bind(team.team_id())
        .bind(season_id)
        .fetch_one(&mut *tx)
        .await
        .with_context(|| format!("Failed to find team_season for {}", team.key))?;

        let captain = scenario::persona(team.captain_key);
        // Captain
        seed_team_member(
            &mut tx,
            team_season_id,
            captain.player_id(),
            "captain",
            captain.user_id(),
        )
        .await?;
        // Members
        for member_key in team.member_keys {
            let member = scenario::persona(member_key);
            seed_team_member(
                &mut tx,
                team_season_id,
                member.player_id(),
                "player",
                captain.user_id(),
            )
            .await?;
        }
    }

    // 10. Rating histories
    info("Seeding player rating histories...");
    seed_rating_histories(&mut tx, game_id).await?;

    // 11. Availability windows
    info("Seeding player availability windows...");
    seed_availability_windows(&mut tx).await?;

    // 12. Tournament 1 (live, registration status)
    info("Seeding tournament...");
    seed_tournament(&mut tx, game_id, season_id, organizer.user_id()).await?;

    // 13. Tournament 1 stage
    seed_tournament_stage(&mut tx).await?;

    // 14. Tournament 2 (self-scheduled, in_progress)
    info("Seeding self-scheduled tournament...");
    seed_tournament_2(&mut tx, game_id, season_id, organizer.user_id()).await?;
    seed_tournament_2_stage(&mut tx).await?;
    seed_tournament_2_bracket(&mut tx).await?;

    // 15. Tournament 2 map pool + registrations + matches
    info("Seeding tournament 2 map pool...");
    seed_tournament_2_map_pool(&mut tx).await?;

    info("Seeding tournament 2 registrations and matches...");
    seed_tournament_2_registrations_and_matches(&mut tx, season_id).await?;

    tx.commit().await.context("Failed to commit transaction")?;

    success("Seed data created successfully!");
    println!();

    Ok(())
}

// ---------------------------------------------------------------------------
// Individual seed helpers
// ---------------------------------------------------------------------------

async fn seed_user(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    p: &Persona,
    password_hash: &str,
) -> Result<()> {
    sqlx::query(
        r"INSERT INTO users (id, username, email, password_hash, status, email_verified)
          VALUES ($1, $2, $3, $4, 'active', TRUE)
          ON CONFLICT DO NOTHING",
    )
    .bind(p.user_id())
    .bind(p.username)
    .bind(p.email)
    .bind(password_hash)
    .execute(&mut **tx)
    .await
    .with_context(|| format!("Failed to seed user {}", p.key))?;
    Ok(())
}

async fn seed_player(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, p: &Persona) -> Result<()> {
    sqlx::query(
        r"INSERT INTO players (id, user_id, display_name, country_code)
          VALUES ($1, $2, $3, $4)
          ON CONFLICT DO NOTHING",
    )
    .bind(p.player_id())
    .bind(p.user_id())
    .bind(p.display_name)
    .bind(p.country_code)
    .execute(&mut **tx)
    .await
    .with_context(|| format!("Failed to seed player {}", p.key))?;
    Ok(())
}

async fn seed_admin_role(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: uuid::Uuid,
) -> Result<()> {
    sqlx::query(
        r"INSERT INTO user_roles (user_id, role_id, granted_by)
          SELECT $1, id, NULL FROM roles WHERE name = 'super_admin'
          ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .execute(&mut **tx)
    .await
    .context("Failed to assign admin role")?;
    Ok(())
}

async fn seed_league(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    game_id: uuid::Uuid,
    created_by: uuid::Uuid,
) -> Result<()> {
    // Note: BEFORE INSERT trigger auto-creates "Season 1" and sets current_season_id
    sqlx::query(
        r"INSERT INTO leagues (id, game_id, name, slug, description, access_type, status, created_by)
          VALUES ($1, $2, 'Competitive CS2', 'competitive-cs2', 'Seed league for development testing', 'open', 'active', $3)
          ON CONFLICT DO NOTHING",
    )
    .bind(scenario::league_id())
    .bind(game_id)
    .bind(created_by)
    .execute(&mut **tx)
    .await
    .context("Failed to seed league")?;
    Ok(())
}

async fn seed_league_member(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: uuid::Uuid,
    membership_type: &str,
) -> Result<()> {
    sqlx::query(
        r"INSERT INTO league_members (league_id, user_id, membership_type)
          VALUES ($1, $2, $3)
          ON CONFLICT (league_id, user_id) DO NOTHING",
    )
    .bind(scenario::league_id())
    .bind(user_id)
    .bind(membership_type)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn seed_premier_league(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    game_id: uuid::Uuid,
    created_by: uuid::Uuid,
) -> Result<()> {
    sqlx::query(
        r#"INSERT INTO leagues (id, game_id, name, slug, description, access_type, status, settings, created_by)
          VALUES ($1, $2, 'CS2 Premier League', 'cs2-premier-league',
                  'Competitive league for high-rated CS2 players. Minimum CS Rating of 12,000 required.',
                  'application', 'active',
                  '{"eligibility": {"min_rating_per_player": 12000}}'::jsonb,
                  $3)
          ON CONFLICT DO NOTHING"#,
    )
    .bind(scenario::premier_league_id())
    .bind(game_id)
    .bind(created_by)
    .execute(&mut **tx)
    .await
    .context("Failed to seed premier league")?;
    Ok(())
}

async fn seed_premier_league_member(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: uuid::Uuid,
    membership_type: &str,
) -> Result<()> {
    sqlx::query(
        r"INSERT INTO league_members (league_id, user_id, membership_type)
          VALUES ($1, $2, $3)
          ON CONFLICT (league_id, user_id) DO NOTHING",
    )
    .bind(scenario::premier_league_id())
    .bind(user_id)
    .bind(membership_type)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn seed_premier_league_application(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: uuid::Uuid,
    message: Option<&str>,
) -> Result<()> {
    let id = scenario::seed_uuid(&format!("premier_app:{user_id}"));
    sqlx::query(
        r"INSERT INTO league_invitations (id, league_id, user_id, invitation_type, status, message)
          VALUES ($1, $2, $3, 'application', 'pending', $4)
          ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .bind(scenario::premier_league_id())
    .bind(user_id)
    .bind(message)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn seed_team(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    team: &scenario::TeamDef,
    owner_player_id: uuid::Uuid,
) -> Result<()> {
    // league_teams are scoped to league (persistent identity), with owner_player_id
    sqlx::query(
        r"INSERT INTO league_teams (id, league_id, name, tag, owner_player_id, status)
          VALUES ($1, $2, $3, $4, $5, 'active')
          ON CONFLICT DO NOTHING",
    )
    .bind(team.team_id())
    .bind(scenario::league_id())
    .bind(team.name)
    .bind(team.tag)
    .bind(owner_player_id)
    .execute(&mut **tx)
    .await
    .with_context(|| format!("Failed to seed team {}", team.key))?;
    Ok(())
}

async fn seed_team_season(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    team_id: uuid::Uuid,
    season_id: uuid::Uuid,
) -> Result<()> {
    sqlx::query(
        r"INSERT INTO league_team_seasons (team_id, season_id, status)
          VALUES ($1, $2, 'active')
          ON CONFLICT DO NOTHING",
    )
    .bind(team_id)
    .bind(season_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn seed_team_member(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    team_season_id: uuid::Uuid,
    player_id: uuid::Uuid,
    role: &str,
    added_by: uuid::Uuid,
) -> Result<()> {
    // season_id is auto-populated by trigger from team_season_id
    sqlx::query(
        r"INSERT INTO league_team_members (team_season_id, player_id, season_id, role, added_by)
          VALUES ($1, $2, (SELECT season_id FROM league_team_seasons WHERE id = $1), $3, $4)
          ON CONFLICT DO NOTHING",
    )
    .bind(team_season_id)
    .bind(player_id)
    .bind(role)
    .bind(added_by)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn seed_tournament(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    game_id: uuid::Uuid,
    season_id: uuid::Uuid,
    created_by: uuid::Uuid,
) -> Result<()> {
    sqlx::query(
        r"INSERT INTO tournaments (
            id, game_id, league_id, season_id,
            name, slug, description,
            format, format_settings, participant_type, team_size,
            min_participants, max_participants,
            registration_type,
            scheduling_mode, default_match_format, default_map_veto_format,
            withdrawal_policy, settings,
            status, created_by
          ) VALUES (
            $1, $2, $3, $4,
            'CS2 Weekly Cup #1', 'cs2-weekly-cup-1', 'Weekly single-elimination tournament for development testing',
            'single_elimination', '{}', 'team', 5,
            4, 8,
            'open',
            'live', 'bo1', 'standard',
            'forfeit', '{}',
            'registration', $5
          )
          ON CONFLICT DO NOTHING",
    )
    .bind(scenario::tournament_id())
    .bind(game_id)
    .bind(scenario::league_id())
    .bind(season_id)
    .bind(created_by)
    .execute(&mut **tx)
    .await
    .context("Failed to seed tournament")?;
    Ok(())
}

async fn seed_tournament_stage(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>) -> Result<()> {
    sqlx::query(
        r"INSERT INTO tournament_stages (
            id, tournament_id,
            name, stage_order,
            format, format_settings,
            advancement_rule,
            match_format, map_veto_format,
            status
          ) VALUES (
            $1, $2,
            'Main Bracket', 1,
            'single_elimination', '{}',
            'top_n',
            'bo1', 'standard',
            'pending'
          )
          ON CONFLICT DO NOTHING",
    )
    .bind(scenario::tournament_stage_id())
    .bind(scenario::tournament_id())
    .execute(&mut **tx)
    .await
    .context("Failed to seed tournament stage")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Rating histories
// ---------------------------------------------------------------------------

async fn seed_rating_histories(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    game_id: uuid::Uuid,
) -> Result<()> {
    let now = Utc::now();
    let history_span = Duration::weeks(14);
    let step = history_span / (scenario::RATING_HISTORY_COUNT as i32 - 1);

    for (player_idx, &(persona_key, base, trend)) in scenario::RATING_PROFILES.iter().enumerate() {
        let p = scenario::persona(persona_key);
        for i in 0..scenario::RATING_HISTORY_COUNT {
            let id = scenario::seed_uuid(&format!("rating:{persona_key}:{i}"));
            let recorded_at = now - history_span + step * (i as i32);
            let wobble = scenario::deterministic_wobble(player_idx, i);
            let rating = (base + trend * i as i32 + wobble).max(0);

            sqlx::query(
                r"INSERT INTO player_rating_history
                    (id, player_id, game_id, rating, source, rank_type_id, recorded_at)
                  VALUES ($1, $2, $3, $4, 'seed', 11, $5)
                  ON CONFLICT (id) DO NOTHING",
            )
            .bind(id)
            .bind(p.player_id())
            .bind(game_id)
            .bind(rating)
            .bind(recorded_at)
            .execute(&mut **tx)
            .await
            .with_context(|| format!("Failed to seed rating history for {persona_key}:{i}"))?;
        }
    }
    info(&format!(
        "  Inserted {} rating history entries",
        scenario::RATING_PROFILES.len() * scenario::RATING_HISTORY_COUNT,
    ));
    Ok(())
}

// ---------------------------------------------------------------------------
// Availability windows
// ---------------------------------------------------------------------------

async fn seed_availability_windows(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>) -> Result<()> {
    for &(persona_key, day, sh, sm, eh, em, preferred, tz) in scenario::AVAILABILITY_WINDOWS {
        let p = scenario::persona(persona_key);
        let id = scenario::seed_uuid(&format!("avail:{persona_key}:{day}:{sh:02}{sm:02}"));
        let start_time = NaiveTime::from_hms_opt(sh.into(), sm.into(), 0)
            .expect("Invalid start_time in AVAILABILITY_WINDOWS");
        let end_time = NaiveTime::from_hms_opt(eh.into(), em.into(), 0)
            .expect("Invalid end_time in AVAILABILITY_WINDOWS");

        sqlx::query(
            r"INSERT INTO availability_windows
                (id, player_id, day_of_week, start_time, end_time, timezone, is_preferred)
              VALUES ($1, $2, $3, $4, $5, $6, $7)
              ON CONFLICT (id) DO NOTHING",
        )
        .bind(id)
        .bind(p.player_id())
        .bind(i16::from(day))
        .bind(start_time)
        .bind(end_time)
        .bind(tz)
        .bind(preferred)
        .execute(&mut **tx)
        .await
        .with_context(|| format!("Failed to seed availability for {persona_key} day={day}"))?;
    }
    info(&format!(
        "  Inserted {} availability windows",
        scenario::AVAILABILITY_WINDOWS.len(),
    ));
    Ok(())
}

// ---------------------------------------------------------------------------
// Tournament 2 (self-scheduled)
// ---------------------------------------------------------------------------

async fn seed_tournament_2(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    game_id: uuid::Uuid,
    season_id: uuid::Uuid,
    created_by: uuid::Uuid,
) -> Result<()> {
    sqlx::query(
        r"INSERT INTO tournaments (
            id, game_id, league_id, season_id,
            name, slug, description,
            format, format_settings, participant_type, team_size,
            min_participants, max_participants,
            registration_type,
            scheduling_mode, default_match_format, default_map_veto_format,
            withdrawal_policy, settings,
            status, created_by, started_at
          ) VALUES (
            $1, $2, $3, $4,
            'CS2 Showdown #2', 'cs2-showdown-2',
            'Self-scheduled single-elimination for match scheduling testing',
            'single_elimination', '{}', 'team', 5,
            4, 8,
            'open',
            'self_scheduled', 'bo3', 'standard',
            'forfeit', '{}',
            'in_progress', $5, NOW() - INTERVAL '2 days'
          )
          ON CONFLICT DO NOTHING",
    )
    .bind(scenario::tournament_2_id())
    .bind(game_id)
    .bind(scenario::league_id())
    .bind(season_id)
    .bind(created_by)
    .execute(&mut **tx)
    .await
    .context("Failed to seed tournament 2")?;
    Ok(())
}

async fn seed_tournament_2_stage(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>) -> Result<()> {
    sqlx::query(
        r"INSERT INTO tournament_stages (
            id, tournament_id,
            name, stage_order,
            format, format_settings,
            advancement_rule,
            match_format, map_veto_format,
            status
          ) VALUES (
            $1, $2,
            'Main Bracket', 1,
            'single_elimination', '{}',
            'top_n',
            'bo3', 'standard',
            'active'
          )
          ON CONFLICT DO NOTHING",
    )
    .bind(scenario::tournament_2_stage_id())
    .bind(scenario::tournament_2_id())
    .execute(&mut **tx)
    .await
    .context("Failed to seed tournament 2 stage")?;
    Ok(())
}

async fn seed_tournament_2_bracket(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>) -> Result<()> {
    sqlx::query(
        r"INSERT INTO tournament_brackets (
            id, stage_id, tournament_id,
            name, bracket_type,
            total_rounds, current_round,
            status
          ) VALUES (
            $1, $2, $3,
            'Main Bracket', 'single_elim',
            2, 1,
            'active'
          )
          ON CONFLICT DO NOTHING",
    )
    .bind(scenario::tournament_2_bracket_id())
    .bind(scenario::tournament_2_stage_id())
    .bind(scenario::tournament_2_id())
    .execute(&mut **tx)
    .await
    .context("Failed to seed tournament 2 bracket")?;
    Ok(())
}

/// CS2 competitive map pool.
const CS2_MAP_POOL: &[&str] = &[
    "de_mirage",
    "de_inferno",
    "de_nuke",
    "de_overpass",
    "de_ancient",
    "de_anubis",
    "de_vertigo",
];

async fn seed_tournament_2_map_pool(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>) -> Result<()> {
    sqlx::query(
        r"INSERT INTO tournament_map_pools (id, tournament_id, maps, veto_format_id)
          VALUES ($1, $2, $3, 'bo3_veto')
          ON CONFLICT DO NOTHING",
    )
    .bind(scenario::tournament_2_map_pool_id())
    .bind(scenario::tournament_2_id())
    .bind(CS2_MAP_POOL)
    .execute(&mut **tx)
    .await
    .context("Failed to seed tournament 2 map pool")?;
    Ok(())
}

/// Seed order maps: Alpha=1, Bravo=2, Charlie=3, Delta=4.
/// Single-elim matchups: (seed1 vs seed4) and (seed2 vs seed3).
const TOURNAMENT_2_SEED_ORDER: &[(&str, i32)] =
    &[("alpha", 1), ("bravo", 2), ("charlie", 3), ("delta", 4)];

async fn seed_tournament_2_registrations_and_matches(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    season_id: uuid::Uuid,
) -> Result<()> {
    let tournament_id = scenario::tournament_2_id();
    let bracket_id = scenario::tournament_2_bracket_id();
    let stage_id = scenario::tournament_2_stage_id();

    // Registrations
    for &(team_key, seed) in TOURNAMENT_2_SEED_ORDER {
        let team = TEAMS
            .iter()
            .find(|t| t.key == team_key)
            .expect("Unknown team key");
        let captain = scenario::persona(team.captain_key);

        let team_season_id: uuid::Uuid = sqlx::query_scalar(
            "SELECT id FROM league_team_seasons WHERE team_id = $1 AND season_id = $2",
        )
        .bind(team.team_id())
        .bind(season_id)
        .fetch_one(&mut **tx)
        .await
        .with_context(|| format!("Failed to find team_season for {team_key}"))?;

        sqlx::query(
            r"INSERT INTO tournament_registrations (
                id, tournament_id, team_season_id,
                participant_name, registered_by,
                status, seed, seed_rating
              ) VALUES ($1, $2, $3, $4, $5, 'active', $6, $7)
              ON CONFLICT DO NOTHING",
        )
        .bind(scenario::tournament_2_registration_id(team_key))
        .bind(tournament_id)
        .bind(team_season_id)
        .bind(team.name)
        .bind(captain.user_id())
        .bind(seed)
        .bind(scenario::team_seed_rating(team_key))
        .execute(&mut **tx)
        .await
        .with_context(|| format!("Failed to seed tournament 2 registration for {team_key}"))?;
    }

    let reg_alpha = scenario::tournament_2_registration_id("alpha");
    let reg_bravo = scenario::tournament_2_registration_id("bravo");
    let reg_charlie = scenario::tournament_2_registration_id("charlie");
    let reg_delta = scenario::tournament_2_registration_id("delta");
    let match_r2m1 = scenario::tournament_2_match_id("R2M1");

    // R2M1: Final (pending, no participants yet) — insert first so R1 matches
    // can reference it via winner_progresses_to FK
    sqlx::query(
        r#"INSERT INTO tournament_matches (
            id, bracket_id, stage_id, tournament_id,
            round, match_number, bracket_position,
            participant1_source, participant2_source,
            match_format, maps_required,
            status
          ) VALUES (
            $1, $2, $3, $4,
            2, 3, 'R2M1',
            '{"WinnerOf": "R1M1"}'::jsonb, '{"WinnerOf": "R1M2"}'::jsonb,
            'bo3', 2,
            'pending'
          )
          ON CONFLICT DO NOTHING"#,
    )
    .bind(match_r2m1)
    .bind(bracket_id)
    .bind(stage_id)
    .bind(tournament_id)
    .execute(&mut **tx)
    .await
    .context("Failed to seed tournament 2 match R2M1")?;

    // R1M1: Alpha (seed 1) vs Delta (seed 4) — in pick_ban with veto session
    sqlx::query(
        r#"INSERT INTO tournament_matches (
            id, bracket_id, stage_id, tournament_id,
            round, match_number, bracket_position,
            participant1_registration_id, participant2_registration_id,
            participant1_name, participant1_seed,
            participant2_name, participant2_seed,
            participant1_source, participant2_source,
            match_format, maps_required,
            veto_required, check_in_required,
            participant1_checked_in_at, participant2_checked_in_at,
            winner_progresses_to,
            status
          ) VALUES (
            $1, $2, $3, $4,
            1, 1, 'R1M1',
            $5, $6,
            'Team Alpha', 1,
            'Team Delta', 4,
            '{"Seed": 1}'::jsonb, '{"Seed": 4}'::jsonb,
            'bo3', 2,
            TRUE, TRUE,
            NOW(), NOW(),
            $7,
            'pick_ban'
          )
          ON CONFLICT DO NOTHING"#,
    )
    .bind(scenario::tournament_2_match_id("R1M1"))
    .bind(bracket_id)
    .bind(stage_id)
    .bind(tournament_id)
    .bind(reg_alpha)
    .bind(reg_delta)
    .bind(match_r2m1)
    .execute(&mut **tx)
    .await
    .context("Failed to seed tournament 2 match R1M1")?;

    // Veto session for R1M1 (coin_flip phase — waiting for coin flip)
    sqlx::query(
        r"INSERT INTO veto_sessions (
            id, match_id, veto_format_id, map_pool, remaining_maps,
            status, current_action_number, timeout_seconds, started_at
          ) VALUES (
            $1, $2, 'bo3_veto', $3, $3,
            'coin_flip', 1, 30, NOW()
          )
          ON CONFLICT DO NOTHING",
    )
    .bind(scenario::tournament_2_veto_session_id("R1M1"))
    .bind(scenario::tournament_2_match_id("R1M1"))
    .bind(CS2_MAP_POOL)
    .execute(&mut **tx)
    .await
    .context("Failed to seed veto session for R1M1")?;

    // R1M2: Bravo (seed 2) vs Charlie (seed 3) — ready, will go through check-in flow
    sqlx::query(
        r#"INSERT INTO tournament_matches (
            id, bracket_id, stage_id, tournament_id,
            round, match_number, bracket_position,
            participant1_registration_id, participant2_registration_id,
            participant1_name, participant1_seed,
            participant2_name, participant2_seed,
            participant1_source, participant2_source,
            match_format, maps_required,
            veto_required, check_in_required,
            winner_progresses_to,
            status
          ) VALUES (
            $1, $2, $3, $4,
            1, 2, 'R1M2',
            $5, $6,
            'Team Bravo', 2,
            'Team Charlie', 3,
            '{"Seed": 2}'::jsonb, '{"Seed": 3}'::jsonb,
            'bo3', 2,
            TRUE, TRUE,
            $7,
            'ready'
          )
          ON CONFLICT DO NOTHING"#,
    )
    .bind(scenario::tournament_2_match_id("R1M2"))
    .bind(bracket_id)
    .bind(stage_id)
    .bind(tournament_id)
    .bind(reg_bravo)
    .bind(reg_charlie)
    .bind(match_r2m1)
    .execute(&mut **tx)
    .await
    .context("Failed to seed tournament 2 match R1M2")?;

    Ok(())
}
