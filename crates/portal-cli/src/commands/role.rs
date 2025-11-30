//! Role and permission management commands.

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use portal_db::entities::{NewRole, NewUserRole};
use portal_db::repositories::{PermissionRepository, RoleRepository};
use portal_db::PgPool;
use uuid::Uuid;

use crate::output::{error, format_uuid, output_list, success, OutputFormat, RoleTableRow};

/// Role management commands.
#[derive(Args)]
pub struct RoleCommand {
    #[command(subcommand)]
    command: RoleSubcommand,
}

#[derive(Subcommand)]
enum RoleSubcommand {
    /// List all roles
    List,

    /// Get role details with permissions
    Get {
        /// Role ID or name
        role: String,
    },

    /// Create a new role
    Create {
        /// Role name (unique identifier)
        #[arg(long)]
        name: String,
        /// Display name
        #[arg(long)]
        display_name: String,
        /// Category
        #[arg(long, default_value = "custom")]
        category: String,
        /// Priority (higher = more powerful)
        #[arg(long, default_value = "0")]
        priority: i32,
    },

    /// Delete a role
    Delete {
        /// Role ID or name
        role: String,
    },

    /// Add permission to role
    AddPermission {
        /// Role name
        role: String,
        /// Permission name
        permission: String,
    },

    /// Remove permission from role
    RemovePermission {
        /// Role name
        role: String,
        /// Permission name
        permission: String,
    },

    /// Assign role to user
    Assign {
        /// User ID
        user_id: Uuid,
        /// Role name
        role: String,
        /// Scope type (team, tournament, league)
        #[arg(long)]
        scope_type: Option<String>,
        /// Scope ID
        #[arg(long)]
        scope_id: Option<Uuid>,
    },

    /// Revoke role from user
    Revoke {
        /// User ID
        user_id: Uuid,
        /// Role name
        role: String,
        /// Scope type
        #[arg(long)]
        scope_type: Option<String>,
        /// Scope ID
        #[arg(long)]
        scope_id: Option<Uuid>,
    },

    /// List permissions
    ListPermissions,
}

impl RoleCommand {
    pub async fn execute(&self, pool: &PgPool, format: OutputFormat) -> Result<()> {
        let role_repo = RoleRepository::new(pool.clone());
        let perm_repo = PermissionRepository::new(pool.clone());

        match &self.command {
            RoleSubcommand::List => list_roles(&role_repo, format).await,
            RoleSubcommand::Get { role } => get_role(&role_repo, role, format).await,
            RoleSubcommand::Create {
                name,
                display_name,
                category,
                priority,
            } => create_role(&role_repo, name, display_name, category, *priority).await,
            RoleSubcommand::Delete { role } => delete_role(&role_repo, role).await,
            RoleSubcommand::AddPermission { role, permission } => {
                add_permission(&role_repo, &perm_repo, role, permission).await
            }
            RoleSubcommand::RemovePermission { role, permission } => {
                remove_permission(&role_repo, &perm_repo, role, permission).await
            }
            RoleSubcommand::Assign {
                user_id,
                role,
                scope_type,
                scope_id,
            } => {
                assign_role(
                    &role_repo,
                    *user_id,
                    role,
                    scope_type.as_deref(),
                    *scope_id,
                )
                .await
            }
            RoleSubcommand::Revoke {
                user_id,
                role,
                scope_type,
                scope_id,
            } => {
                revoke_role(
                    &role_repo,
                    *user_id,
                    role,
                    scope_type.as_deref(),
                    *scope_id,
                )
                .await
            }
            RoleSubcommand::ListPermissions => list_permissions(&perm_repo, format).await,
        }
    }
}

async fn list_roles(repo: &RoleRepository, format: OutputFormat) -> Result<()> {
    let roles = repo.list().await.context("Failed to fetch roles")?;

    let rows: Vec<RoleTableRow> = roles
        .into_iter()
        .map(|r| RoleTableRow {
            id: format_uuid(&r.id),
            name: r.name,
            display_name: r.display_name,
            category: r.category,
            priority: r.priority,
            is_system: if r.is_system { "Yes" } else { "No" }.to_string(),
        })
        .collect();

    output_list(&rows, format)
}

async fn get_role(repo: &RoleRepository, role: &str, format: OutputFormat) -> Result<()> {
    let role_row = repo
        .find_by_id_or_name(role)
        .await
        .context("Failed to fetch role")?;

    if let Some(r) = role_row {
        let permissions = repo
            .get_permissions(r.id)
            .await
            .context("Failed to fetch permissions")?;

        if matches!(format, OutputFormat::Json) {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "id": r.id,
                    "name": r.name,
                    "display_name": r.display_name,
                    "description": r.description,
                    "category": r.category,
                    "priority": r.priority,
                    "is_system": r.is_system,
                    "is_default": r.is_default,
                    "color": r.color,
                    "permissions": permissions.iter().map(|p| &p.name).collect::<Vec<_>>()
                }))?
            );
        } else {
            println!("Role: {}", r.display_name);
            println!("  ID:       {}", r.id);
            println!("  Name:     {}", r.name);
            println!("  Category: {}", r.category);
            println!("  Priority: {}", r.priority);
            println!("  System:   {}", r.is_system);
            println!("  Default:  {}", r.is_default);
            println!("\nPermissions ({}):", permissions.len());
            for p in &permissions {
                println!("  - {} ({})", p.name, p.category);
            }
        }
        Ok(())
    } else {
        error(&format!("Role not found: {role}"));
        std::process::exit(1);
    }
}

async fn create_role(
    repo: &RoleRepository,
    name: &str,
    display_name: &str,
    category: &str,
    priority: i32,
) -> Result<()> {
    let new_role = NewRole {
        name: name.to_string(),
        display_name: display_name.to_string(),
        description: None,
        category: category.to_string(),
        priority,
        color: None,
    };

    let role = repo.create(new_role).await.context("Failed to create role")?;

    success(&format!("Created role: {} ({})", name, role.id));
    Ok(())
}

async fn delete_role(repo: &RoleRepository, role: &str) -> Result<()> {
    let role_row = repo
        .find_by_id_or_name(role)
        .await
        .context("Failed to fetch role")?;

    if let Some(r) = role_row {
        if r.is_system {
            error("Cannot delete system roles");
            std::process::exit(1);
        }

        let deleted = repo.delete(r.id).await.context("Failed to delete role")?;

        if deleted {
            success(&format!("Deleted role: {}", r.id));
        } else {
            error("Role not found or is a system role");
            std::process::exit(1);
        }
        Ok(())
    } else {
        error(&format!("Role not found: {role}"));
        std::process::exit(1);
    }
}

async fn add_permission(
    role_repo: &RoleRepository,
    perm_repo: &PermissionRepository,
    role: &str,
    permission: &str,
) -> Result<()> {
    let role_row = role_repo
        .find_by_id_or_name(role)
        .await
        .context("Failed to fetch role")?
        .ok_or_else(|| anyhow::anyhow!("Role not found: {role}"))?;

    let perm_row = perm_repo
        .find_by_id_or_name(permission)
        .await
        .context("Failed to fetch permission")?
        .ok_or_else(|| anyhow::anyhow!("Permission not found: {permission}"))?;

    role_repo
        .add_permission(role_row.id, perm_row.id)
        .await
        .context("Failed to add permission")?;

    success(&format!(
        "Added permission '{permission}' to role '{role}'"
    ));
    Ok(())
}

async fn remove_permission(
    role_repo: &RoleRepository,
    perm_repo: &PermissionRepository,
    role: &str,
    permission: &str,
) -> Result<()> {
    let role_row = role_repo
        .find_by_id_or_name(role)
        .await
        .context("Failed to fetch role")?
        .ok_or_else(|| anyhow::anyhow!("Role not found: {role}"))?;

    let perm_row = perm_repo
        .find_by_id_or_name(permission)
        .await
        .context("Failed to fetch permission")?
        .ok_or_else(|| anyhow::anyhow!("Permission not found: {permission}"))?;

    let removed = role_repo
        .remove_permission(role_row.id, perm_row.id)
        .await
        .context("Failed to remove permission")?;

    if removed {
        success(&format!(
            "Removed permission '{permission}' from role '{role}'"
        ));
    } else {
        error("Permission not assigned to role");
    }
    Ok(())
}

async fn assign_role(
    repo: &RoleRepository,
    user_id: Uuid,
    role: &str,
    scope_type: Option<&str>,
    scope_id: Option<Uuid>,
) -> Result<()> {
    let role_row = repo
        .find_by_id_or_name(role)
        .await
        .context("Failed to fetch role")?
        .ok_or_else(|| anyhow::anyhow!("Role not found: {role}"))?;

    let assignment = NewUserRole {
        user_id,
        role_id: role_row.id,
        scope_type: scope_type.map(String::from),
        scope_id,
        granted_by: None,
        expires_at: None,
    };

    repo.assign_to_user(assignment)
        .await
        .context("Failed to assign role")?;

    let scope_str = match (scope_type, scope_id) {
        (Some(t), Some(id)) => format!(" (scope: {t}/{id})"),
        _ => String::new(),
    };

    success(&format!(
        "Assigned role '{role}' to user {user_id}{scope_str}"
    ));
    Ok(())
}

async fn revoke_role(
    repo: &RoleRepository,
    user_id: Uuid,
    role: &str,
    scope_type: Option<&str>,
    scope_id: Option<Uuid>,
) -> Result<()> {
    let role_row = repo
        .find_by_id_or_name(role)
        .await
        .context("Failed to fetch role")?
        .ok_or_else(|| anyhow::anyhow!("Role not found: {role}"))?;

    let revoked = repo
        .revoke_from_user(user_id, role_row.id, scope_type, scope_id, None)
        .await
        .context("Failed to revoke role")?;

    if revoked {
        success(&format!("Revoked role '{role}' from user {user_id}"));
    } else {
        error("Role assignment not found");
    }
    Ok(())
}

async fn list_permissions(repo: &PermissionRepository, format: OutputFormat) -> Result<()> {
    let perms = repo.list().await.context("Failed to fetch permissions")?;

    if matches!(format, OutputFormat::Json) {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &perms
                    .iter()
                    .map(|p| serde_json::json!({
                        "name": p.name,
                        "display_name": p.display_name,
                        "category": p.category,
                        "is_dangerous": p.is_dangerous
                    }))
                    .collect::<Vec<_>>()
            )?
        );
    } else {
        let mut current_cat = String::new();
        for p in &perms {
            if p.category != current_cat {
                println!("\n[{}]", p.category);
                current_cat.clone_from(&p.category);
            }
            let danger = if p.is_dangerous { " (DANGEROUS)" } else { "" };
            println!("  {} - {}{}", p.name, p.display_name, danger);
        }
    }
    Ok(())
}
