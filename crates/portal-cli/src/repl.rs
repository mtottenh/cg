//! Interactive REPL mode for the Portal CLI.
//!
//! Provides an interactive shell where administrators can run commands
//! without repeatedly typing `portal` prefix.

use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use reedline::{
    default_emacs_keybindings, ColumnarMenu, DefaultCompleter, DefaultHinter, DefaultPrompt,
    DefaultPromptSegment, Emacs, FileBackedHistory, KeyCode, KeyModifiers, MenuBuilder, Reedline,
    ReedlineEvent, ReedlineMenu, Signal,
};
use sqlx::PgPool;

use crate::commands::{audit, ban, bootstrap, db, game, league_team, player, role, user};
use crate::output::OutputFormat;

/// Commands available in REPL mode (subset of main CLI commands).
#[derive(Parser)]
#[command(name = "portal", about = "Portal CLI - Interactive Mode", disable_help_subcommand = true)]
enum ReplCommands {
    /// User management commands
    User(user::UserCommand),

    /// Role and permission management
    Role(role::RoleCommand),

    /// Player profile management
    Player(player::PlayerCommand),

    /// Game configuration
    Game(game::GameCommand),

    /// Database utilities
    Db(db::DbCommand),

    /// Bootstrap commands (initial setup)
    Bootstrap(bootstrap::BootstrapCommand),

    /// Ban management
    Ban(ban::BanCommand),

    /// Audit log viewing
    Audit(audit::AuditCommand),

    /// League team management
    LeagueTeam(league_team::LeagueTeamCommand),

    /// Exit the REPL
    Exit,

    /// Show help
    Help,
}

/// Run the interactive REPL.
pub async fn run(pool: &PgPool) -> Result<()> {
    println!("{}", "Portal CLI - Interactive Mode".cyan().bold());
    println!("Type {} for available commands, {} to quit.\n", "help".green(), "exit".green());

    // Set up command completer
    let commands = vec![
        "user".into(),
        "user list".into(),
        "user get".into(),
        "user create".into(),
        "user update".into(),
        "user disable".into(),
        "user enable".into(),
        "user reset-password".into(),
        "user ban".into(),
        "user unban".into(),
        "role".into(),
        "role list".into(),
        "role get".into(),
        "role create".into(),
        "role delete".into(),
        "role add-permission".into(),
        "role remove-permission".into(),
        "role assign".into(),
        "role revoke".into(),
        "role list-permissions".into(),
        "player".into(),
        "player list".into(),
        "player get".into(),
        "player create".into(),
        "player update".into(),
        "player stats".into(),
        "player reset-rating".into(),
        "game".into(),
        "game list".into(),
        "game get".into(),
        "game create".into(),
        "game update".into(),
        "game enable".into(),
        "game disable".into(),
        "db".into(),
        "db migrate".into(),
        "db status".into(),
        "db stats".into(),
        "db seed".into(),
        "db clear".into(),
        "bootstrap".into(),
        "bootstrap admin".into(),
        "ban".into(),
        "ban list".into(),
        "ban get".into(),
        "ban create".into(),
        "ban lift".into(),
        "ban history".into(),
        "ban check".into(),
        "audit".into(),
        "audit list".into(),
        "audit get".into(),
        "audit entity".into(),
        "audit user".into(),
        "audit search".into(),
        "audit stats".into(),
        "league-team".into(),
        "league-team list".into(),
        "league-team get".into(),
        "league-team search".into(),
        "league-team update-status".into(),
        "league-team member".into(),
        "league-team member list".into(),
        "league-team member get".into(),
        "league-team member add".into(),
        "league-team member remove".into(),
        "league-team member update-role".into(),
        "league-team invitation".into(),
        "league-team invitation list".into(),
        "league-team invitation get".into(),
        "league-team invitation cancel".into(),
        "league-team invitation for-player".into(),
        "league-team season".into(),
        "league-team season list".into(),
        "league-team season get".into(),
        "league-team season update-status".into(),
        "help".into(),
        "exit".into(),
    ];

    let completer = Box::new(DefaultCompleter::new_with_wordlen(commands, 2));
    let hinter = Box::new(DefaultHinter::default().with_style(nu_ansi_term::Style::new().dimmed()));

    let completion_menu = Box::new(ColumnarMenu::default().with_name("completion_menu"));

    // Set up keybindings
    let mut keybindings = default_emacs_keybindings();
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("completion_menu".to_string()),
            ReedlineEvent::MenuNext,
        ]),
    );

    let edit_mode = Box::new(Emacs::new(keybindings));

    // Set up history
    let history_path = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("portal-cli")
        .join("history.txt");

    // Ensure directory exists
    if let Some(parent) = history_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let history = Box::new(
        FileBackedHistory::with_file(1000, history_path)
            .expect("Failed to create history file"),
    );

    // Build the line editor
    let mut line_editor = Reedline::create()
        .with_completer(completer)
        .with_hinter(hinter)
        .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
        .with_edit_mode(edit_mode)
        .with_history(history);

    // Custom prompt
    let prompt = DefaultPrompt::new(
        DefaultPromptSegment::Basic("portal".to_string()),
        DefaultPromptSegment::Empty,
    );

    loop {
        match line_editor.read_line(&prompt) {
            Ok(Signal::Success(line)) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                // Parse the line into arguments
                let Some(args) = shlex::split(line) else {
                    eprintln!("{} Invalid command syntax", "Error:".red());
                    continue;
                };

                if args.is_empty() {
                    continue;
                }

                // Try to parse as a command
                match ReplCommands::try_parse_from(std::iter::once(String::new()).chain(args)) {
                    Ok(cmd) => {
                        if let Err(e) = execute_command(cmd, pool).await {
                            eprintln!("{} {}", "Error:".red(), e);
                        }
                    }
                    Err(e) => {
                        // Clap errors include help text
                        eprintln!("{e}");
                    }
                }
            }
            Ok(Signal::CtrlC) => {
                println!("Use {} or {} to exit", "exit".green(), "Ctrl+D".green());
            }
            Ok(Signal::CtrlD) => {
                println!("\nGoodbye!");
                break;
            }
            Err(e) => {
                eprintln!("{} {}", "Error:".red(), e);
            }
        }
    }

    Ok(())
}

async fn execute_command(cmd: ReplCommands, pool: &PgPool) -> Result<()> {
    let format = OutputFormat::Table;

    match cmd {
        ReplCommands::User(c) => c.execute(pool, format).await,
        ReplCommands::Role(c) => c.execute(pool, format).await,
        ReplCommands::Player(c) => c.execute(pool, format).await,
        ReplCommands::Game(c) => c.execute(pool, format).await,
        ReplCommands::Db(c) => c.execute(pool, format).await,
        ReplCommands::Bootstrap(c) => c.execute(pool, format).await,
        ReplCommands::Ban(c) => c.execute(pool, format).await,
        ReplCommands::Audit(c) => c.execute(pool, format).await,
        ReplCommands::LeagueTeam(c) => c.execute(pool, format).await,
        ReplCommands::Exit => {
            println!("Goodbye!");
            std::process::exit(0);
        }
        ReplCommands::Help => {
            print_help();
            Ok(())
        }
    }
}

fn print_help() {
    println!("{}", "Available Commands:".cyan().bold());
    println!();
    println!("  {}        User management (list, get, create, update, ban, unban)", "user".green());
    println!("  {}        Role and permission management", "role".green());
    println!("  {}      Player profile management", "player".green());
    println!("  {}        Game configuration", "game".green());
    println!("  {}          Database utilities (status, stats, seed)", "db".green());
    println!("  {}   Create initial admin user", "bootstrap".green());
    println!("  {}         Ban management (list, create, lift)", "ban".green());
    println!("  {}       Audit log viewing (list, entity, user, search)", "audit".green());
    println!("  {} League team management (member, invitation, season)", "league-team".green());
    println!();
    println!("  {}        Show this help", "help".green());
    println!("  {}        Exit the REPL", "exit".green());
    println!();
    println!("{}:", "Tips".cyan());
    println!("  - Use {} for command completion", "Tab".yellow());
    println!("  - Use {} / {} to navigate history", "Up".yellow(), "Down".yellow());
    println!("  - Add {} to any command for detailed help", "--help".yellow());
}
