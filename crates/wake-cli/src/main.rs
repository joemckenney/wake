use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser)]
#[command(name = "wake")]
#[command(
    about = "Development context recorder - capture terminal sessions for AI-assisted development"
)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start a recorded shell session
    Shell,
    /// Show current/recent session summary
    Status,
    /// Show recent commands with output
    Log {
        /// Number of commands to show
        #[arg(short, long, default_value = "10")]
        count: usize,
    },
    /// Search command history
    Search {
        /// Search query
        query: String,
    },
    /// Export context as markdown (for AI consumption)
    Dump,
    /// Add manual annotation/breadcrumb
    Annotate {
        /// Note text
        note: String,
    },
    /// Output shell hook configuration
    Init {
        /// Shell type (zsh, bash)
        shell: String,
    },
    /// Manage local LLM for summarization
    Llm {
        #[command(subcommand)]
        action: LlmAction,
    },
    /// Internal: called by shell hooks
    #[command(name = "__hook", hide = true)]
    Hook {
        #[command(subcommand)]
        event: HookEvent,
    },
    /// Delete old sessions and their data
    Prune {
        /// Preview what would be deleted without deleting
        #[arg(long)]
        dry_run: bool,
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
        /// Override retention period in days (default: from config or 21)
        #[arg(long, value_name = "DAYS")]
        older_than: Option<u32>,
    },
}

#[derive(Subcommand)]
enum LlmAction {
    /// Download the summarization model
    Download,
    /// Show model status and path
    Status,
}

#[derive(Subcommand)]
enum HookEvent {
    /// Signal command start
    CmdStart {
        #[arg(long)]
        cmd: String,
    },
    /// Signal command end
    CmdEnd {
        #[arg(long)]
        exit_code: i32,
        #[arg(long)]
        duration: u64,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Shell => commands::shell::run().await,
        Commands::Status => commands::status::run().await,
        Commands::Log { count } => commands::log::run(count).await,
        Commands::Search { query } => commands::search::run(&query).await,
        Commands::Dump => commands::dump::run().await,
        Commands::Annotate { note } => commands::annotate::run(&note).await,
        Commands::Init { shell } => commands::init::run(&shell).await,
        Commands::Llm { action } => match action {
            LlmAction::Download => commands::llm::download().await,
            LlmAction::Status => commands::llm::status().await,
        },
        Commands::Hook { event } => match event {
            HookEvent::CmdStart { cmd } => commands::hook::cmd_start(&cmd).await,
            HookEvent::CmdEnd { exit_code, duration } => {
                commands::hook::cmd_end(exit_code, duration).await
            }
        },
        Commands::Prune { dry_run, force, older_than } => {
            commands::prune::run(commands::prune::PruneOptions {
                dry_run,
                force,
                older_than_days: older_than,
            })
            .await
        }
    }
}
