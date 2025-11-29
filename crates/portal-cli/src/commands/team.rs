//! Team management commands.

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use portal_core::{PlayerId, TeamId};
use portal_db::entities::{NewTeam, NewTeamMember, UpdateTeam, UpdateTeamMember};
use portal_db::repositories::{TeamMemberRepository, TeamRepository};
use portal_db::PgPool;
use uuid::Uuid;

use crate::output::{
    error, format_optional, format_uuid, output_list, success, warn, OutputFormat, TeamTableRow,
};

/// Team management commands.
#[derive(Args)]
pub struct TeamCommand {
    #[command(subcommand)]
    command: TeamSubcommand,
}

#[derive(Subcommand)]
enum TeamSubcommand {
    /// List teams
    List {
        /// Search by name
        #[arg(long)]
        search: Option<String>,
        /// Maximum results
        #[arg(long, default_value = "50")]
        limit: i64,
    },

    /// Get team details
    Get {
        /// Team ID
        id: Uuid,
    },

    /// Create a team
    Create {
        /// Team name
        #[arg(long)]
        name: String,
        /// Team tag (2-5 characters)
        #[arg(long)]
        tag: String,
        /// Creator player ID
        #[arg(long)]
        creator: Uuid,
        /// Game ID
        #[arg(long)]
        game: Option<String>,
    },

    /// Update team
    Update {
        /// Team ID
        id: Uuid,
        /// New name
        #[arg(long)]
        name: Option<String>,
        /// New tag
        #[arg(long)]
        tag: Option<String>,
        /// Description
        #[arg(long)]
        description: Option<String>,
    },

    /// Disband team
    Disband {
        /// Team ID
        id: Uuid,
        /// Reason
        #[arg(long)]
        reason: Option<String>,
    },

    /// Add member to team
    AddMember {
        /// Team ID
        team_id: Uuid,
        /// Player ID
        player_id: Uuid,
        /// Role (captain, officer, player, substitute)
        #[arg(long, default_value = "player")]
        role: String,
    },

    /// Remove member from team
    RemoveMember {
        /// Team ID
        team_id: Uuid,
        /// Player ID
        player_id: Uuid,
    },

    /// Set member role
    SetRole {
        /// Team ID
        team_id: Uuid,
        /// Player ID
        player_id: Uuid,
        /// New role
        role: String,
    },
}

impl TeamCommand {
    pub async fn execute(&self, pool: &PgPool, format: OutputFormat) -> Result<()> {
        let team_repo = TeamRepository::new(pool.clone());
        let member_repo = TeamMemberRepository::new(pool.clone());

        match &self.command {
            TeamSubcommand::List { search, limit } => {
                list_teams(&team_repo, search.as_deref(), *limit, format).await
            }
            TeamSubcommand::Get { id } => get_team(&team_repo, &member_repo, *id, format).await,
            TeamSubcommand::Create {
                name,
                tag,
                creator,
                game,
            } => {
                create_team(&team_repo, &member_repo, name, tag, *creator, game.as_deref()).await
            }
            TeamSubcommand::Update {
                id,
                name,
                tag,
                description,
            } => {
                update_team(
                    &team_repo,
                    *id,
                    name.as_deref(),
                    tag.as_deref(),
                    description.as_deref(),
                )
                .await
            }
            TeamSubcommand::Disband { id, reason } => {
                disband_team(&team_repo, *id, reason.as_deref()).await
            }
            TeamSubcommand::AddMember {
                team_id,
                player_id,
                role,
            } => add_member(&member_repo, *team_id, *player_id, role).await,
            TeamSubcommand::RemoveMember { team_id, player_id } => {
                remove_member(&member_repo, *team_id, *player_id).await
            }
            TeamSubcommand::SetRole {
                team_id,
                player_id,
                role,
            } => set_member_role(&member_repo, *team_id, *player_id, role).await,
        }
    }
}

async fn list_teams(
    repo: &TeamRepository,
    search: Option<&str>,
    limit: i64,
    format: OutputFormat,
) -> Result<()> {
    let teams = if let Some(query) = search {
        repo.search(query, limit, 0)
            .await
            .context("Failed to fetch teams")?
    } else {
        // For now, search with empty string to get all teams
        repo.search("", limit, 0)
            .await
            .context("Failed to fetch teams")?
    };

    let rows: Vec<TeamTableRow> = teams
        .into_iter()
        .map(|t| TeamTableRow {
            id: format_uuid(&t.id),
            name: t.name,
            tag: t.tag,
            game: format_optional(&t.game_id),
            status: t.status,
            member_count: 0, // Would need additional query
        })
        .collect();

    output_list(&rows, format)
}

async fn get_team(
    team_repo: &TeamRepository,
    member_repo: &TeamMemberRepository,
    id: Uuid,
    format: OutputFormat,
) -> Result<()> {
    let team_id = TeamId::from_uuid(id);
    let team = team_repo
        .find_by_id(team_id)
        .await
        .context("Failed to fetch team")?;

    match team {
        Some(t) => {
            let members = member_repo
                .list_by_team(team_id)
                .await
                .context("Failed to fetch members")?;

            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "id": t.id,
                            "name": t.name,
                            "tag": t.tag,
                            "description": t.description,
                            "game_id": t.game_id,
                            "status": t.status,
                            "members": members.iter().map(|m| serde_json::json!({
                                "player_id": m.player_id,
                                "role": m.role,
                                "is_founder": m.is_founder
                            })).collect::<Vec<_>>()
                        }))?
                    );
                }
                _ => {
                    println!("Team: {} [{}]", t.name, t.tag);
                    println!("  ID:          {}", t.id);
                    println!("  Game:        {}", format_optional(&t.game_id));
                    println!("  Status:      {}", t.status);
                    println!("  Description: {}", format_optional(&t.description));
                    println!("\nMembers ({}):", members.len());
                    for m in &members {
                        let founder = if m.is_founder { " (founder)" } else { "" };
                        println!("  - {} [{}]{}", m.player_id, m.role, founder);
                    }
                }
            }
            Ok(())
        }
        None => {
            error(&format!("Team not found: {id}"));
            std::process::exit(1);
        }
    }
}

async fn create_team(
    team_repo: &TeamRepository,
    member_repo: &TeamMemberRepository,
    name: &str,
    tag: &str,
    creator: Uuid,
    game: Option<&str>,
) -> Result<()> {
    let new_team = NewTeam {
        name: name.to_string(),
        tag: tag.to_uppercase(),
        created_by: creator,
        description: None,
        logo_url: None,
        game_id: game.map(String::from),
    };

    let team = team_repo
        .create(new_team)
        .await
        .context("Failed to create team")?;

    // Add creator as founding captain
    let new_member = NewTeamMember {
        team_id: team.id,
        player_id: creator,
        role: "captain".to_string(),
        is_founder: true,
        invited_by: None,
    };

    member_repo
        .create(new_member)
        .await
        .context("Failed to add founder")?;

    success(&format!("Created team: {} [{}] ({})", name, tag, team.id));
    Ok(())
}

async fn update_team(
    repo: &TeamRepository,
    id: Uuid,
    name: Option<&str>,
    tag: Option<&str>,
    description: Option<&str>,
) -> Result<()> {
    let team_id = TeamId::from_uuid(id);
    let update = UpdateTeam {
        name: name.map(String::from),
        tag: tag.map(|t| t.to_uppercase()),
        description: description.map(String::from),
        ..Default::default()
    };

    let team = repo
        .update(team_id, update)
        .await
        .context("Failed to update team")?;

    success(&format!("Updated team: {}", team.id));
    Ok(())
}

async fn disband_team(repo: &TeamRepository, id: Uuid, reason: Option<&str>) -> Result<()> {
    warn("Disbanding team - this will remove all members and cancel active tournaments");

    let team_id = TeamId::from_uuid(id);
    let update = UpdateTeam {
        status: Some("disbanded".to_string()),
        disbanded_at: Some(chrono::Utc::now()),
        disbanded_reason: reason.map(String::from),
        ..Default::default()
    };

    let team = repo
        .update(team_id, update)
        .await
        .context("Failed to disband team")?;

    success(&format!("Disbanded team: {}", team.id));
    Ok(())
}

async fn add_member(
    repo: &TeamMemberRepository,
    team_id: Uuid,
    player_id: Uuid,
    role: &str,
) -> Result<()> {
    let new_member = NewTeamMember {
        team_id,
        player_id,
        role: role.to_string(),
        is_founder: false,
        invited_by: None,
    };

    repo.create(new_member)
        .await
        .context("Failed to add member")?;

    success(&format!(
        "Added player {player_id} to team {team_id} as {role}"
    ));
    Ok(())
}

async fn remove_member(repo: &TeamMemberRepository, team_id: Uuid, player_id: Uuid) -> Result<()> {
    let tid = TeamId::from_uuid(team_id);
    let pid = PlayerId::from_uuid(player_id);

    repo.remove(tid, pid)
        .await
        .context("Failed to remove member")?;

    success(&format!(
        "Removed player {player_id} from team {team_id}"
    ));
    Ok(())
}

async fn set_member_role(
    repo: &TeamMemberRepository,
    team_id: Uuid,
    player_id: Uuid,
    role: &str,
) -> Result<()> {
    let tid = TeamId::from_uuid(team_id);
    let pid = PlayerId::from_uuid(player_id);

    let update = UpdateTeamMember {
        role: Some(role.to_string()),
        ..Default::default()
    };

    let member = repo
        .update(tid, pid, update)
        .await
        .context("Failed to set role")?;

    if member.is_founder && role != "captain" {
        warn("Note: Founders cannot be demoted from captain via normal means");
    }

    success(&format!("Set role to '{role}' for player {player_id}"));
    Ok(())
}
