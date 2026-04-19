//! Credential output for seeded users.

use anyhow::{Context, Result};
use portal_db::PgPool;
use serde::Serialize;

use super::scenario::{self, PERSONAS, SEED_PASSWORD, TEAMS};
use crate::output::OutputFormat;

#[derive(Serialize)]
struct CredentialOutput {
    scenario: &'static str,
    password: &'static str,
    personas: Vec<PersonaCredential>,
    entities: EntityInfo,
}

#[derive(Serialize)]
struct PersonaCredential {
    role: &'static str,
    username: &'static str,
    email: &'static str,
    user_id: String,
    player_id: String,
    is_admin: bool,
    jwt_token: String,
}

#[derive(Serialize)]
struct EntityInfo {
    league: EntityRef,
    season: EntityRef,
    teams: Vec<TeamRef>,
    tournaments: Vec<EntityRef>,
}

#[derive(Serialize)]
struct EntityRef {
    id: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
}

#[derive(Serialize)]
struct TeamRef {
    id: String,
    name: &'static str,
    tag: &'static str,
    captain: &'static str,
}

/// Print credentials for all seeded personas.
pub async fn print_credentials(
    pool: &PgPool,
    jwt_secret: &str,
    token_expiry_days: i64,
    _format: OutputFormat,
) -> Result<()> {
    let expiry_minutes = token_expiry_days * 24 * 60;

    // Verify at least the admin user exists
    let admin = scenario::persona("admin");
    let exists: Option<(uuid::Uuid,)> =
        sqlx::query_as("SELECT id FROM users WHERE id = $1")
            .bind(admin.user_id())
            .fetch_optional(pool)
            .await
            .context("Failed to query users")?;

    if exists.is_none() {
        anyhow::bail!("Seed users not found. Run `seed full` first.");
    }

    // Look up season from league
    let season_row: Option<(uuid::Uuid, String, String, String)> = sqlx::query_as(
        r"SELECT ls.id, ls.name, ls.slug, ls.status
          FROM league_seasons ls
          JOIN leagues l ON l.current_season_id = ls.id
          WHERE l.id = $1",
    )
    .bind(scenario::league_id())
    .fetch_optional(pool)
    .await
    .context("Failed to query season")?;

    let (season_id, season_name, season_slug, season_status) = season_row
        .unwrap_or_else(|| {
            (
                uuid::Uuid::nil(),
                "Unknown".to_string(),
                "unknown".to_string(),
                "unknown".to_string(),
            )
        });

    // Check admin role
    // Admin status is enforced by RBAC, not by JWT claim (see
    // portal-domain::jwt). The seed previously embedded an is_admin claim
    // gated on the super_admin role; that claim is gone, so now we
    // compute the role tag purely for downstream presentation.
    let has_admin_role: bool = sqlx::query_scalar(
        r"SELECT EXISTS(
            SELECT 1 FROM user_roles ur
            JOIN roles r ON r.id = ur.role_id
            WHERE ur.user_id = $1 AND r.name = 'super_admin'
        )",
    )
    .bind(admin.user_id())
    .fetch_one(pool)
    .await
    .unwrap_or(false);

    let mut personas = Vec::new();
    for p in PERSONAS {
        let is_admin = p.is_admin && has_admin_role;
        let token = portal_domain::jwt::generate_access_token_with_expiry(
            p.user_id(),
            p.player_id(),
            p.username,
            jwt_secret,
            expiry_minutes,
        )
        .map_err(|e| anyhow::anyhow!("Failed to generate token for {}: {e}", p.key))?;

        let role = if p.is_admin {
            "admin"
        } else if p.key.starts_with("captain_") {
            "captain"
        } else if p.key == "organizer" {
            "organizer"
        } else if p.key.starts_with("player_") {
            "player"
        } else {
            "spectator"
        };

        personas.push(PersonaCredential {
            role,
            username: p.username,
            email: p.email,
            user_id: p.user_id().to_string(),
            player_id: p.player_id().to_string(),
            is_admin,
            jwt_token: token,
        });
    }

    let teams = TEAMS
        .iter()
        .map(|t| TeamRef {
            id: t.team_id().to_string(),
            name: t.name,
            tag: t.tag,
            captain: t.captain_key,
        })
        .collect();

    let output = CredentialOutput {
        scenario: "full",
        password: SEED_PASSWORD,
        personas,
        entities: EntityInfo {
            league: EntityRef {
                id: scenario::league_id().to_string(),
                name: "Competitive CS2".to_string(),
                slug: Some("competitive-cs2".to_string()),
                status: Some("active".to_string()),
            },
            season: EntityRef {
                id: season_id.to_string(),
                name: season_name,
                slug: Some(season_slug),
                status: Some(season_status),
            },
            teams,
            tournaments: vec![
                EntityRef {
                    id: scenario::tournament_id().to_string(),
                    name: "CS2 Weekly Cup #1".to_string(),
                    slug: Some("cs2-weekly-cup-1".to_string()),
                    status: Some("registration".to_string()),
                },
                EntityRef {
                    id: scenario::tournament_2_id().to_string(),
                    name: "CS2 Showdown #2".to_string(),
                    slug: Some("cs2-showdown-2".to_string()),
                    status: Some("in_progress".to_string()),
                },
            ],
        },
    };

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
