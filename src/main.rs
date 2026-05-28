use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod cli;
mod compile;
mod embed;
mod index;
mod signal;
mod store;
mod tree;
mod trim;

#[derive(Parser)]
#[command(
    name = "ctx",
    about = "Context Compiler — natural language → optimized AI context",
    version,
    styles = cli::styles()
)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize the context index for a codebase
    Init {
        /// Path to the codebase (default: current directory)
        path: Option<PathBuf>,

        /// Force re-index even if index exists
        #[arg(long, short)]
        force: bool,
    },

    /// Compile context for a natural language task
    Compile {
        /// The task description
        task: String,

        /// Token budget for the context window (default: 8192)
        #[arg(long, short = 'b', default_value = "8192")]
        budget: usize,

        /// Max files to include (default: 0 = auto)
        #[arg(long, short = 'm', default_value = "0")]
        max_files: usize,

        /// Write output to file instead of clipboard
        #[arg(long, short = 'o')]
        output: Option<PathBuf>,

        /// Don't copy to clipboard
        #[arg(long, default_value = "false")]
        no_clipboard: bool,
    },

    /// Show index statistics and status
    Status,

    /// Rebuild the index from scratch
    Reindex {
        /// Path to the codebase (default: current directory)
        path: Option<PathBuf>,
    },

    /// Watch mode: watch files and auto-rebuild index
    Watch {
        /// Path to the codebase (default: current directory)
        path: Option<PathBuf>,
    },

    /// Mark the last compilation task as complete (saves to history)
    Done,

    /// Show compilation history
    History {
        /// How many past entries to show
        #[arg(long, short, default_value = "10")]
        limit: usize,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();

    match args.command {
        Commands::Init { path, force } => {
            cli::cmd_init(path.unwrap_or_default(), force).await?;
        }
        Commands::Compile {
            task,
            budget,
            max_files,
            output,
            no_clipboard,
        } => {
            cli::cmd_compile(&task, budget, max_files, output, no_clipboard).await?;
        }
        Commands::Status => {
            cli::cmd_status().await?;
        }
        Commands::Reindex { path } => {
            cli::cmd_reindex(path.unwrap_or_default()).await?;
        }
        Commands::Watch { path } => {
            cli::cmd_watch(path.unwrap_or_default()).await?;
        }
        Commands::Done => {
            cli::cmd_done().await?;
        }
        Commands::History { limit } => {
            cli::cmd_history(limit).await?;
        }
    }

    Ok(())
}
