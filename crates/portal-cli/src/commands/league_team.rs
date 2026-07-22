//! League team management commands.
//!
//! Provides commands for managing league teams including:
//! - Team CRUD operations
//! - Member management
//! - Invitation management
//! - Season registration

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use portal_db::PgPool;
use portal_db::entities::{
    LeagueTeamInvitationRow, LeagueTeamMemberRow, LeagueTeamRow, LeagueTeamSeasonRow,
};
use serde::Serialize;
use tabled::Tabled;
use uuid::Uuid;

use crate::output::{
    OutputFormat, error, format_optional, format_timestamp, format_uuid, info, output_list, success,
};

/// League team management commands.
#[derive(Args)]
pub struct LeagueTeamCommand {
    #[command(subcommand)]
    command: LeagueTeamSubcommand,
}

#[derive(Subcommand)]
enum LeagueTeamSubcommand {
    /// List teams in a league
    List {
        /// League ID
        league_id: Uuid,

        /// Filter by status (active, inactive, disbanded)
        #[arg(long)]
        status: Option<String>,

        /// Maximum number of results
        #[arg(long, default_value = "50")]
        limit: i64,
    },

    /// Get team details
    Get {
        /// Team ID
        team_id: Uuid,
    },

    /// Search teams by name or tag
    Search {
        /// Search query
        query: String,

        /// Filter by league ID
        #[arg(long)]
        league_id: Option<Uuid>,

        /// Maximum number of results
        #[arg(long, default_value = "20")]
        limit: i64,
    },

    /// Update team status
    UpdateStatus {
        /// Team ID
        team_id: Uuid,

        /// New status (active, inactive, disbanded)
        status: String,
    },

    /// Member management subcommands
    #[command(subcommand)]
    Member(MemberSubcommand),

    /// Invitation management subcommands
    #[command(subcommand)]
    Invitation(InvitationSubcommand),

    /// Season registration subcommands
    #[command(subcommand)]
    Season(SeasonSubcommand),
}

#[derive(Subcommand)]
enum MemberSubcommand {
    /// List team members for a season
    List {
        /// Team ID
        team_id: Uuid,

        /// Season ID (required for seasonal roster)
        #[arg(long)]
        season_id: Uuid,

        /// Include inactive members
        #[arg(long)]
        include_inactive: bool,
    },

    /// Get member details
    Get {
        /// Member ID
        member_id: Uuid,
    },

    /// Update member role
    UpdateRole {
        /// Member ID
        member_id: Uuid,

        /// New role (captain, player, substitute)
        role: String,
    },

    /// Remove member from team
    Remove {
        /// Member ID
        member_id: Uuid,

        /// Reason for removal
        #[arg(long)]
        reason: Option<String>,
    },

    /// Add player directly to team (admin action)
    Add {
        /// Team season ID
        team_season_id: Uuid,

        /// Player ID to add
        player_id: Uuid,

        /// Role (captain, player, substitute)
        #[arg(long, default_value = "player")]
        role: String,
    },
}

#[derive(Subcommand)]
enum InvitationSubcommand {
    /// List pending invitations for a team
    List {
        /// Team ID
        team_id: Uuid,

        /// Season ID
        #[arg(long)]
        season_id: Uuid,

        /// Include expired/responded invitations
        #[arg(long)]
        include_all: bool,
    },

    /// Get invitation details
    Get {
        /// Invitation ID
        invitation_id: Uuid,
    },

    /// Cancel an invitation
    Cancel {
        /// Invitation ID
        invitation_id: Uuid,
    },

    /// List invitations for a player
    ForPlayer {
        /// Player ID
        player_id: Uuid,

        /// Include expired/responded invitations
        #[arg(long)]
        include_all: bool,
    },
}

#[derive(Subcommand)]
enum SeasonSubcommand {
    /// List team's season registrations
    List {
        /// Team ID
        team_id: Uuid,
    },

    /// Get team season details
    Get {
        /// Team season ID
        team_season_id: Uuid,
    },

    /// Update team season status
    UpdateStatus {
        /// Team season ID
        team_season_id: Uuid,

        /// New status (pending, approved, rejected, withdrawn)
        status: String,
    },
}

impl LeagueTeamCommand {
    pub async fn execute(&self, pool: &PgPool, format: OutputFormat) -> Result<()> {
        match &self.command {
            LeagueTeamSubcommand::List {
                league_id,
                status,
                limit,
            } => list_teams(pool, *league_id, status.as_deref(), *limit, format).await,
            LeagueTeamSubcommand::Get { team_id } => get_team(pool, *team_id, format).await,
            LeagueTeamSubcommand::Search {
                query,
                league_id,
                limit,
            } => search_teams(pool, query, *league_id, *limit, format).await,
            LeagueTeamSubcommand::UpdateStatus { team_id, status } => {
                update_team_status(pool, *team_id, status).await
            }
            LeagueTeamSubcommand::Member(cmd) => match cmd {
                MemberSubcommand::List {
                    team_id,
                    season_id,
                    include_inactive,
                } => list_members(pool, *team_id, *season_id, *include_inactive, format).await,
                MemberSubcommand::Get { member_id } => get_member(pool, *member_id, format).await,
                MemberSubcommand::UpdateRole { member_id, role } => {
                    update_member_role(pool, *member_id, role).await
                }
                MemberSubcommand::Remove { member_id, reason } => {
                    remove_member(pool, *member_id, reason.as_deref()).await
                }
                MemberSubcommand::Add {
                    team_season_id,
                    player_id,
                    role,
                } => add_member(pool, *team_season_id, *player_id, role).await,
            },
            LeagueTeamSubcommand::Invitation(cmd) => match cmd {
                InvitationSubcommand::List {
                    team_id,
                    season_id,
                    include_all,
                } => list_invitations(pool, *team_id, *season_id, *include_all, format).await,
                InvitationSubcommand::Get { invitation_id } => {
                    get_invitation(pool, *invitation_id, format).await
                }
                InvitationSubcommand::Cancel { invitation_id } => {
                    cancel_invitation(pool, *invitation_id).await
                }
                InvitationSubcommand::ForPlayer {
                    player_id,
                    include_all,
                } => list_player_invitations(pool, *player_id, *include_all, format).await,
            },
            LeagueTeamSubcommand::Season(cmd) => match cmd {
                SeasonSubcommand::List { team_id } => {
                    list_team_seasons(pool, *team_id, format).await
                }
                SeasonSubcommand::Get { team_season_id } => {
                    get_team_season(pool, *team_season_id, format).await
                }
                SeasonSubcommand::UpdateStatus {
                    team_season_id,
                    status,
                } => update_team_season_status(pool, *team_season_id, status).await,
            },
        }
    }
}

// =============================================================================
// Table Row Types
// =============================================================================

#[derive(Tabled, Serialize)]
struct TeamTableRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Tag")]
    tag: String,
    #[tabled(rename = "Owner")]
    owner: String,
    #[tabled(rename = "Status")]
    status: String,
    #[tabled(rename = "Created")]
    created_at: String,
}

#[derive(Tabled, Serialize)]
struct MemberTableRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "Player")]
    player_id: String,
    #[tabled(rename = "Role")]
    role: String,
    #[tabled(rename = "Status")]
    status: String,
    #[tabled(rename = "Joined")]
    joined_at: String,
}

#[derive(Tabled, Serialize)]
struct InvitationTableRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "Player")]
    player_id: String,
    #[tabled(rename = "Role")]
    role: String,
    #[tabled(rename = "Status")]
    status: String,
    #[tabled(rename = "Expires")]
    expires_at: String,
}

#[derive(Tabled, Serialize)]
struct TeamSeasonTableRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "Season")]
    season_id: String,
    #[tabled(rename = "Status")]
    status: String,
    #[tabled(rename = "W")]
    wins: i32,
    #[tabled(rename = "L")]
    losses: i32,
    #[tabled(rename = "D")]
    draws: i32,
    #[tabled(rename = "Registered")]
    registered_at: String,
}

// =============================================================================
// Team Operations
// =============================================================================

async fn list_teams(
    pool: &PgPool,
    league_id: Uuid,
    status: Option<&str>,
    limit: i64,
    format: OutputFormat,
) -> Result<()> {
    let rows = sqlx::query_as::<_, LeagueTeamRow>(
        r"
        SELECT * FROM league_teams
        WHERE league_id = $1
          AND ($2::text IS NULL OR status = $2)
        ORDER BY name
        LIMIT $3
        ",
    )
    .bind(league_id)
    .bind(status)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to fetch teams")?;

    let table_rows: Vec<TeamTableRow> = rows
        .iter()
        .map(|r| TeamTableRow {
            id: format_uuid(&r.id),
            name: r.name.clone(),
            tag: r.tag.clone(),
            owner: format_uuid(&r.owner_player_id),
            status: r.status.clone(),
            created_at: format_timestamp(&r.created_at),
        })
        .collect();

    output_list(&table_rows, format)
}

async fn get_team(pool: &PgPool, team_id: Uuid, format: OutputFormat) -> Result<()> {
    let team = sqlx::query_as::<_, LeagueTeamRow>("SELECT * FROM league_teams WHERE id = $1")
        .bind(team_id)
        .fetch_optional(pool)
        .await
        .context("Failed to fetch team")?;

    let Some(team) = team else {
        error(&format!("Team not found: {team_id}"));
        std::process::exit(1);
    };

    if matches!(format, OutputFormat::Json) {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "id": team.id,
                "league_id": team.league_id,
                "name": team.name,
                "tag": team.tag,
                "description": team.description,
                "logo_url": team.logo_url,
                "banner_url": team.banner_url,
                "primary_color": team.primary_color,
                "secondary_color": team.secondary_color,
                "owner_player_id": team.owner_player_id,
                "status": team.status,
                "created_at": team.created_at,
                "updated_at": team.updated_at,
                "disbanded_at": team.disbanded_at,
            }))?
        );
    } else {
        println!("Team Details:");
        println!("  ID:             {}", team.id);
        println!("  League:         {}", team.league_id);
        println!("  Name:           {}", team.name);
        println!("  Tag:            {}", team.tag);
        println!("  Description:    {}", format_optional(&team.description));
        println!("  Logo URL:       {}", format_optional(&team.logo_url));
        println!("  Owner:          {}", team.owner_player_id);
        println!("  Status:         {}", team.status);
        println!("  Created:        {}", format_timestamp(&team.created_at));
        if let Some(disbanded) = team.disbanded_at {
            println!("  Disbanded At:   {}", format_timestamp(&disbanded));
        }
    }

    Ok(())
}

async fn search_teams(
    pool: &PgPool,
    query: &str,
    league_id: Option<Uuid>,
    limit: i64,
    format: OutputFormat,
) -> Result<()> {
    let pattern = format!("%{query}%");
    let rows = sqlx::query_as::<_, LeagueTeamRow>(
        r"
        SELECT * FROM league_teams
        WHERE (name ILIKE $1 OR tag ILIKE $1)
          AND ($2::uuid IS NULL OR league_id = $2)
        ORDER BY name
        LIMIT $3
        ",
    )
    .bind(&pattern)
    .bind(league_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to search teams")?;

    if rows.is_empty() {
        info(&format!("No teams found matching '{query}'"));
        return Ok(());
    }

    let table_rows: Vec<TeamTableRow> = rows
        .iter()
        .map(|r| TeamTableRow {
            id: format_uuid(&r.id),
            name: r.name.clone(),
            tag: r.tag.clone(),
            owner: format_uuid(&r.owner_player_id),
            status: r.status.clone(),
            created_at: format_timestamp(&r.created_at),
        })
        .collect();

    output_list(&table_rows, format)
}

async fn update_team_status(pool: &PgPool, team_id: Uuid, status: &str) -> Result<()> {
    let valid_statuses = ["active", "inactive", "disbanded"];
    if !valid_statuses.contains(&status) {
        error(&format!(
            "Invalid status: {status}. Valid values: {}",
            valid_statuses.join(", ")
        ));
        std::process::exit(1);
    }

    let result = sqlx::query(
        r"
        UPDATE league_teams
        SET status = $2,
            disbanded_at = CASE WHEN $2 = 'disbanded' THEN NOW() ELSE disbanded_at END,
            updated_at = NOW()
        WHERE id = $1
        ",
    )
    .bind(team_id)
    .bind(status)
    .execute(pool)
    .await
    .context("Failed to update team status")?;

    if result.rows_affected() == 0 {
        error(&format!("Team not found: {team_id}"));
        std::process::exit(1);
    }

    success(&format!("Updated team {team_id} status to '{status}'"));
    Ok(())
}

// =============================================================================
// Member Operations
// =============================================================================

async fn list_members(
    pool: &PgPool,
    team_id: Uuid,
    season_id: Uuid,
    include_inactive: bool,
    format: OutputFormat,
) -> Result<()> {
    // First get the team_season_id
    let team_season = sqlx::query_as::<_, LeagueTeamSeasonRow>(
        "SELECT * FROM league_team_seasons WHERE team_id = $1 AND season_id = $2",
    )
    .bind(team_id)
    .bind(season_id)
    .fetch_optional(pool)
    .await
    .context("Failed to fetch team season")?;

    let Some(team_season) = team_season else {
        error(&format!(
            "Team {team_id} is not registered for season {season_id}"
        ));
        std::process::exit(1);
    };

    let rows = sqlx::query_as::<_, LeagueTeamMemberRow>(
        r"
        SELECT * FROM league_team_members
        WHERE team_season_id = $1
          AND ($2 OR status = 'active')
        ORDER BY
            CASE role WHEN 'captain' THEN 0 WHEN 'player' THEN 1 ELSE 2 END,
            joined_at
        ",
    )
    .bind(team_season.id)
    .bind(include_inactive)
    .fetch_all(pool)
    .await
    .context("Failed to fetch team members")?;

    if rows.is_empty() {
        info("No members found for this team season");
        return Ok(());
    }

    let table_rows: Vec<MemberTableRow> = rows
        .iter()
        .map(|r| MemberTableRow {
            id: format_uuid(&r.id),
            player_id: format_uuid(&r.player_id),
            role: r.role.clone(),
            status: r.status.clone(),
            joined_at: format_timestamp(&r.joined_at),
        })
        .collect();

    output_list(&table_rows, format)
}

async fn get_member(pool: &PgPool, member_id: Uuid, format: OutputFormat) -> Result<()> {
    let member =
        sqlx::query_as::<_, LeagueTeamMemberRow>("SELECT * FROM league_team_members WHERE id = $1")
            .bind(member_id)
            .fetch_optional(pool)
            .await
            .context("Failed to fetch member")?;

    let Some(member) = member else {
        error(&format!("Member not found: {member_id}"));
        std::process::exit(1);
    };

    if matches!(format, OutputFormat::Json) {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "id": member.id,
                "team_season_id": member.team_season_id,
                "player_id": member.player_id,
                "season_id": member.season_id,
                "role": member.role,
                "position": member.position,
                "jersey_number": member.jersey_number,
                "status": member.status,
                "joined_at": member.joined_at,
                "left_at": member.left_at,
                "added_by": member.added_by,
            }))?
        );
    } else {
        println!("Member Details:");
        println!("  ID:            {}", member.id);
        println!("  Team Season:   {}", member.team_season_id);
        println!("  Player:        {}", member.player_id);
        println!("  Role:          {}", member.role);
        println!("  Position:      {}", format_optional(&member.position));
        println!(
            "  Jersey #:      {}",
            member
                .jersey_number
                .map_or_else(|| "-".to_string(), |n| n.to_string())
        );
        println!("  Status:        {}", member.status);
        println!("  Joined:        {}", format_timestamp(&member.joined_at));
        if let Some(left) = member.left_at {
            println!("  Left At:       {}", format_timestamp(&left));
        }
    }

    Ok(())
}

async fn update_member_role(pool: &PgPool, member_id: Uuid, role: &str) -> Result<()> {
    let valid_roles = ["captain", "player", "substitute"];
    if !valid_roles.contains(&role) {
        error(&format!(
            "Invalid role: {role}. Valid values: {}",
            valid_roles.join(", ")
        ));
        std::process::exit(1);
    }

    let result = sqlx::query("UPDATE league_team_members SET role = $2 WHERE id = $1")
        .bind(member_id)
        .bind(role)
        .execute(pool)
        .await
        .context("Failed to update member role")?;

    if result.rows_affected() == 0 {
        error(&format!("Member not found: {member_id}"));
        std::process::exit(1);
    }

    success(&format!("Updated member {member_id} role to '{role}'"));
    Ok(())
}

async fn remove_member(pool: &PgPool, member_id: Uuid, _reason: Option<&str>) -> Result<()> {
    let result = sqlx::query(
        r"
        UPDATE league_team_members
        SET status = 'removed', left_at = NOW()
        WHERE id = $1
        ",
    )
    .bind(member_id)
    .execute(pool)
    .await
    .context("Failed to remove member")?;

    if result.rows_affected() == 0 {
        error(&format!("Member not found: {member_id}"));
        std::process::exit(1);
    }

    success(&format!("Removed member {member_id} from team"));
    Ok(())
}

async fn add_member(
    pool: &PgPool,
    team_season_id: Uuid,
    player_id: Uuid,
    role: &str,
) -> Result<()> {
    let valid_roles = ["captain", "player", "substitute"];
    if !valid_roles.contains(&role) {
        error(&format!(
            "Invalid role: {role}. Valid values: {}",
            valid_roles.join(", ")
        ));
        std::process::exit(1);
    }

    // Check if already a member
    let existing = sqlx::query_as::<_, LeagueTeamMemberRow>(
        r"
        SELECT * FROM league_team_members
        WHERE team_season_id = $1 AND player_id = $2 AND status = 'active'
        ",
    )
    .bind(team_season_id)
    .bind(player_id)
    .fetch_optional(pool)
    .await
    .context("Failed to check existing membership")?;

    if existing.is_some() {
        error("Player is already an active member of this team");
        std::process::exit(1);
    }

    let member = sqlx::query_as::<_, LeagueTeamMemberRow>(
        r"
        INSERT INTO league_team_members (team_season_id, player_id, role, status)
        VALUES ($1, $2, $3, 'active')
        RETURNING *
        ",
    )
    .bind(team_season_id)
    .bind(player_id)
    .bind(role)
    .fetch_one(pool)
    .await
    .context("Failed to add member")?;

    success(&format!(
        "Added player {player_id} to team as '{role}' (member ID: {})",
        member.id
    ));
    Ok(())
}

// =============================================================================
// Invitation Operations
// =============================================================================

async fn list_invitations(
    pool: &PgPool,
    team_id: Uuid,
    season_id: Uuid,
    include_all: bool,
    format: OutputFormat,
) -> Result<()> {
    let team_season = sqlx::query_as::<_, LeagueTeamSeasonRow>(
        "SELECT * FROM league_team_seasons WHERE team_id = $1 AND season_id = $2",
    )
    .bind(team_id)
    .bind(season_id)
    .fetch_optional(pool)
    .await
    .context("Failed to fetch team season")?;

    let Some(team_season) = team_season else {
        error(&format!(
            "Team {team_id} is not registered for season {season_id}"
        ));
        std::process::exit(1);
    };

    let rows = sqlx::query_as::<_, LeagueTeamInvitationRow>(
        r"
        SELECT * FROM league_team_invitations
        WHERE team_season_id = $1
          AND ($2 OR (status = 'pending' AND expires_at > NOW()))
        ORDER BY created_at DESC
        ",
    )
    .bind(team_season.id)
    .bind(include_all)
    .fetch_all(pool)
    .await
    .context("Failed to fetch invitations")?;

    if rows.is_empty() {
        info("No invitations found");
        return Ok(());
    }

    let table_rows: Vec<InvitationTableRow> = rows
        .iter()
        .map(|r| InvitationTableRow {
            id: format_uuid(&r.id),
            player_id: format_uuid(&r.player_id),
            role: r.role.clone(),
            status: r.status.clone(),
            expires_at: format_timestamp(&r.expires_at),
        })
        .collect();

    output_list(&table_rows, format)
}

async fn get_invitation(pool: &PgPool, invitation_id: Uuid, format: OutputFormat) -> Result<()> {
    let invitation = sqlx::query_as::<_, LeagueTeamInvitationRow>(
        "SELECT * FROM league_team_invitations WHERE id = $1",
    )
    .bind(invitation_id)
    .fetch_optional(pool)
    .await
    .context("Failed to fetch invitation")?;

    let Some(inv) = invitation else {
        error(&format!("Invitation not found: {invitation_id}"));
        std::process::exit(1);
    };

    if matches!(format, OutputFormat::Json) {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "id": inv.id,
                "team_season_id": inv.team_season_id,
                "player_id": inv.player_id,
                "invitation_type": inv.invitation_type,
                "role": inv.role,
                "message": inv.message,
                "invited_by": inv.invited_by,
                "status": inv.status,
                "responded_at": inv.responded_at,
                "response_message": inv.response_message,
                "expires_at": inv.expires_at,
                "created_at": inv.created_at,
            }))?
        );
    } else {
        println!("Invitation Details:");
        println!("  ID:            {}", inv.id);
        println!("  Team Season:   {}", inv.team_season_id);
        println!("  Player:        {}", inv.player_id);
        println!("  Type:          {}", inv.invitation_type);
        println!("  Role:          {}", inv.role);
        println!("  Message:       {}", format_optional(&inv.message));
        println!("  Status:        {}", inv.status);
        println!("  Expires:       {}", format_timestamp(&inv.expires_at));
        println!("  Created:       {}", format_timestamp(&inv.created_at));
        if let Some(responded) = inv.responded_at {
            println!("  Responded At:  {}", format_timestamp(&responded));
        }
    }

    Ok(())
}

async fn cancel_invitation(pool: &PgPool, invitation_id: Uuid) -> Result<()> {
    let result = sqlx::query(
        r"
        UPDATE league_team_invitations
        SET status = 'cancelled', responded_at = NOW()
        WHERE id = $1 AND status = 'pending'
        ",
    )
    .bind(invitation_id)
    .execute(pool)
    .await
    .context("Failed to cancel invitation")?;

    if result.rows_affected() == 0 {
        error("Invitation not found or already responded");
        std::process::exit(1);
    }

    success(&format!("Cancelled invitation {invitation_id}"));
    Ok(())
}

async fn list_player_invitations(
    pool: &PgPool,
    player_id: Uuid,
    include_all: bool,
    format: OutputFormat,
) -> Result<()> {
    let rows = sqlx::query_as::<_, LeagueTeamInvitationRow>(
        r"
        SELECT * FROM league_team_invitations
        WHERE player_id = $1
          AND ($2 OR (status = 'pending' AND expires_at > NOW()))
        ORDER BY created_at DESC
        ",
    )
    .bind(player_id)
    .bind(include_all)
    .fetch_all(pool)
    .await
    .context("Failed to fetch player invitations")?;

    if rows.is_empty() {
        info(&format!("No invitations found for player {player_id}"));
        return Ok(());
    }

    let table_rows: Vec<InvitationTableRow> = rows
        .iter()
        .map(|r| InvitationTableRow {
            id: format_uuid(&r.id),
            player_id: format_uuid(&r.player_id),
            role: r.role.clone(),
            status: r.status.clone(),
            expires_at: format_timestamp(&r.expires_at),
        })
        .collect();

    output_list(&table_rows, format)
}

// =============================================================================
// Season Operations
// =============================================================================

async fn list_team_seasons(pool: &PgPool, team_id: Uuid, format: OutputFormat) -> Result<()> {
    let rows = sqlx::query_as::<_, LeagueTeamSeasonRow>(
        r"
        SELECT * FROM league_team_seasons
        WHERE team_id = $1
        ORDER BY created_at DESC
        ",
    )
    .bind(team_id)
    .fetch_all(pool)
    .await
    .context("Failed to fetch team seasons")?;

    if rows.is_empty() {
        info(&format!("No season registrations found for team {team_id}"));
        return Ok(());
    }

    let table_rows: Vec<TeamSeasonTableRow> = rows
        .iter()
        .map(|r| TeamSeasonTableRow {
            id: format_uuid(&r.id),
            season_id: format_uuid(&r.season_id),
            status: r.status.clone(),
            wins: r.matches_won,
            losses: r.matches_lost,
            draws: r.matches_drawn,
            registered_at: r
                .registered_at
                .map_or_else(|| "-".to_string(), |t| format_timestamp(&t)),
        })
        .collect();

    output_list(&table_rows, format)
}

async fn get_team_season(pool: &PgPool, team_season_id: Uuid, format: OutputFormat) -> Result<()> {
    let ts =
        sqlx::query_as::<_, LeagueTeamSeasonRow>("SELECT * FROM league_team_seasons WHERE id = $1")
            .bind(team_season_id)
            .fetch_optional(pool)
            .await
            .context("Failed to fetch team season")?;

    let Some(ts) = ts else {
        error(&format!("Team season not found: {team_season_id}"));
        std::process::exit(1);
    };

    if matches!(format, OutputFormat::Json) {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "id": ts.id,
                "team_id": ts.team_id,
                "season_id": ts.season_id,
                "status": ts.status,
                "registered_at": ts.registered_at,
                "registration_notes": ts.registration_notes,
                "matches_played": ts.matches_played,
                "matches_won": ts.matches_won,
                "matches_lost": ts.matches_lost,
                "matches_drawn": ts.matches_drawn,
                "seed": ts.seed,
                "rating": ts.rating,
                "created_at": ts.created_at,
                "updated_at": ts.updated_at,
            }))?
        );
    } else {
        println!("Team Season Details:");
        println!("  ID:            {}", ts.id);
        println!("  Team:          {}", ts.team_id);
        println!("  Season:        {}", ts.season_id);
        println!("  Status:        {}", ts.status);
        println!(
            "  Registered:    {}",
            ts.registered_at
                .map_or_else(|| "-".to_string(), |t| format_timestamp(&t))
        );
        println!(
            "  Notes:         {}",
            format_optional(&ts.registration_notes)
        );
        println!();
        println!("  Matches:");
        println!("    Played: {}", ts.matches_played);
        println!("    Won:    {}", ts.matches_won);
        println!("    Lost:   {}", ts.matches_lost);
        println!("    Drawn:  {}", ts.matches_drawn);
        println!();
        println!(
            "  Seed:          {}",
            ts.seed.map_or_else(|| "-".to_string(), |n| n.to_string())
        );
        println!(
            "  Rating:        {}",
            ts.rating.map_or_else(|| "-".to_string(), |n| n.to_string())
        );
    }

    Ok(())
}

async fn update_team_season_status(
    pool: &PgPool,
    team_season_id: Uuid,
    status: &str,
) -> Result<()> {
    let valid_statuses = [
        "pending",
        "approved",
        "rejected",
        "withdrawn",
        "disqualified",
    ];
    if !valid_statuses.contains(&status) {
        error(&format!(
            "Invalid status: {status}. Valid values: {}",
            valid_statuses.join(", ")
        ));
        std::process::exit(1);
    }

    let result = sqlx::query(
        r"
        UPDATE league_team_seasons
        SET status = $2, updated_at = NOW()
        WHERE id = $1
        ",
    )
    .bind(team_season_id)
    .bind(status)
    .execute(pool)
    .await
    .context("Failed to update team season status")?;

    if result.rows_affected() == 0 {
        error(&format!("Team season not found: {team_season_id}"));
        std::process::exit(1);
    }

    success(&format!(
        "Updated team season {team_season_id} status to '{status}'"
    ));
    Ok(())
}
