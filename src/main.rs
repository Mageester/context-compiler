use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod cli;
mod compile;
mod config;
mod embed;
mod index;
mod rerank;
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
    /// Optional command. If omitted, remaining words are treated as the task to compile.
    #[command(subcommand)]
    command: Option<Commands>,

    /// Natural-language task shorthand: ctx "fix auth race condition"
    #[arg(value_name = "TASK", trailing_var_arg = true)]
    task: Vec<String>,
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

    /// Configure Context Compiler (API keys, models, etc.)
    Configure {
        /// Path to the project (default: current directory)
        path: Option<PathBuf>,

        /// Set OpenAI API key
        #[arg(long)]
        set_openai_key: Option<String>,

        /// Set embedding model name
        #[arg(long)]
        set_embedding_model: Option<String>,

        /// Set reranker model name
        #[arg(long)]
        set_reranker_model: Option<String>,

        /// Enable or disable AI reranker
        #[arg(long)]
        set_use_reranker: Option<bool>,

        /// Show current configuration
        #[arg(long, short)]
        show: bool,

        /// Save to global config (~/.ctx/config.toml) instead of project local
        #[arg(long, short)]
        global: bool,
    },

    /// Generate shell completion scripts
    Completions {
        /// Shell to generate completions for (bash, zsh, fish, powershell, elvish)
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let args = Args::parse();

    match args.command {
        Some(Commands::Init { path, force }) => {
            cli::cmd_init(path.unwrap_or_default(), force).await?;
        }
        Some(Commands::Compile {
            task,
            budget,
            max_files,
            output,
            no_clipboard,
        }) => {
            cli::cmd_compile(&task, budget, max_files, output, no_clipboard).await?;
        }
        Some(Commands::Status) => {
            cli::cmd_status().await?;
        }
        Some(Commands::Reindex { path }) => {
            cli::cmd_reindex(path.unwrap_or_default()).await?;
        }
        Some(Commands::Watch { path }) => {
            cli::cmd_watch(path.unwrap_or_default()).await?;
        }
        Some(Commands::Done) => {
            cli::cmd_done().await?;
        }
        Some(Commands::History { limit }) => {
            cli::cmd_history(limit).await?;
        }
        Some(Commands::Configure {
            path,
            set_openai_key,
            set_embedding_model,
            set_reranker_model,
            set_use_reranker,
            show,
            global,
        }) => {
            cli::cmd_configure(
                path,
                set_openai_key,
                set_embedding_model,
                set_reranker_model,
                set_use_reranker,
                show,
                global,
            )
            .await?;
        }
        Some(Commands::Completions { shell }) => {
            use clap::CommandFactory;
            cli::cmd_completions(shell, &mut Args::command()).await?;
        }
        None if !args.task.is_empty() => {
            let task = args.task.join(" ");
            cli::cmd_compile(&task, 8192, 0, None, false).await?;
        }
        None => {
            use clap::CommandFactory;
            Args::command().print_help()?;
            println!();
        }
    }

    Ok(())
}
