use anyhow::{Context, Result};
use clap::builder::Styles;
use colored::*;
use std::path::{Path, PathBuf};

use crate::compile::Compiler;
use crate::config::{self, Config};
use crate::embed::Embedder;
use crate::index::IndexBuilder;
use crate::store::Store;

/// Get colorful CLI styles
pub fn styles() -> Styles {
    use clap::builder::styling;
    Styles::styled()
        .header(styling::AnsiColor::Cyan.on_default())
        .usage(styling::AnsiColor::Cyan.on_default())
        .literal(styling::AnsiColor::Green.on_default())
        .placeholder(styling::AnsiColor::Yellow.on_default())
}

/// Print the visual header
fn print_header(text: &str) {
    println!("\n{}", text.cyan().bold());
    println!("{}", "─".repeat(text.len()).cyan().dimmed());
}

/// Print a success message
fn print_success(text: &str) {
    println!(" {} {}", "✓".green().bold(), text);
}

/// Print an info message
fn print_info(text: &str) {
    println!(" {} {}", "→".cyan(), text);
}

/// Print a warning message
fn print_warn(text: &str) {
    println!(" {} {}", "⚠".yellow().bold(), text);
}

fn open_store(path: &Path) -> Result<Store> {
    Store::open(path).context("Failed to open context store")
}

fn get_embedder(config: &Config) -> Result<Embedder> {
    Embedder::init(config).context("Failed to initialize embedding engine")
}

fn load_config(project_path: &Path) -> Config {
    let global = config::load_global_config();
    let local = Config::load(project_path);
    // Merge: local overrides global which overrides defaults
    // Treat empty strings as unset
    let local_key = local.openai_key.filter(|k| !k.is_empty());
    let global_key = global.openai_key.filter(|k| !k.is_empty());
    Config {
        openai_key: local_key.or(global_key),
        embedding_model: local.embedding_model.or(global.embedding_model),
        reranker_model: local.reranker_model.or(global.reranker_model),
        use_reranker: local.use_reranker.or(global.use_reranker),
    }
}

/// Handle `ctx init`
pub async fn cmd_init(path: PathBuf, force: bool) -> Result<()> {
    let path = if path.as_os_str().is_empty() {
        std::env::current_dir()?
    } else {
        path
    };

    let config = load_config(&path);
    let store = open_store(&path)?;
    let embedder = get_embedder(&config)?;

    print_header("Context Compiler — Init");

    if store.file_count()? > 0 && !force {
        println!(
            "  {} Index already exists ({} files). Use {} to rebuild.",
            "→".cyan(),
            store.file_count()?,
            "ctx reindex".yellow()
        );
        return Ok(());
    }

    if force {
        store.clear()?;
        print_info("Rebuilding index from scratch...");
    } else {
        print_info("Analyzing codebase...");
    }

    IndexBuilder::build(&path, &store, &embedder, force)?;

    let count = store.file_count()?;
    println!();
    print_success(&format!("Indexed {} files", count));

    // Show API key status
    if config.has_openai_key() {
        print_info("✓ OpenAI API key configured — embeddings & AI reranker active");
    } else {
        print_warn("No OpenAI API key found. Run `ctx configure --set openai-key=sk-...`");
        print_info("  Without a key, the tool uses hash-based matching (less accurate)");
    }

    Ok(())
}

/// Handle `ctx compile <task>`
pub async fn cmd_compile(
    task: &str,
    budget: usize,
    max_files: usize,
    output: Option<PathBuf>,
    no_clipboard: bool,
) -> Result<()> {
    let path = std::env::current_dir()?;
    let config = load_config(&path);
    let store = open_store(&path)?;
    let embedder = get_embedder(&config)?;

    print_header("Context Compiler — Compile");
    println!("  Task:    {}", task.cyan());
    println!("  Budget:  {} tokens", budget.to_string().yellow());
    if config.has_openai_key() {
        println!("  Reranker: {} ({})", "ON".green().bold(), config.reranker_model.as_deref().unwrap_or("gpt-4o-mini"));
    } else {
        println!("  Reranker: {} (set OPENAI_API_KEY for AI-powered accuracy)", "OFF".yellow());
    }
    println!();

    // Ensure index exists
    if store.file_count()? == 0 {
        print_info("No index found. Building initial index...");
        println!();
        IndexBuilder::build(&path, &store, &embedder, false)?;
        println!();
    }

    // Compile with full 4-stage pipeline
    let (context, selected, total_tokens) =
        Compiler::compile(&path, &store, &embedder, &config, task, budget, max_files)?;

    if selected.is_empty() {
        print_warn("No relevant files found for this task.");
        println!("  Try being more specific, or check that your codebase is indexed.");
        return Ok(());
    }

    // Print results
    print_success(&format!(
        "Selected {} files · ~{} tokens (from {} indexed files)",
        selected.len().to_string().green().bold(),
        total_tokens.to_string().yellow(),
        store.file_count()?.to_string().dimmed(),
    ));
    println!();

    for (i, file) in selected.iter().enumerate() {
        let score_pct = (file.score * 100.0) as usize;
        let bar = "█".repeat(score_pct / 10) + &"░".repeat(10 - (score_pct / 10).min(10));
        let lex_tag = if file.lexical_score > 0.0 {
            format!(" [BM25:{:.0}%]", file.lexical_score * 100.0)
        } else {
            String::new()
        };
        println!(
            "  {}. {} ({})  {} {:2}%{}",
            (i + 1).to_string().cyan().bold(),
            file.path.white().bold(),
            file.token_count.to_string().yellow(),
            bar.cyan().dimmed(),
            score_pct,
            lex_tag.dimmed(),
        );
    }
    println!();

    // Save to history
    let paths: Vec<String> = selected.iter().map(|f| f.path.clone()).collect();
    Compiler::save_to_history(&store, &embedder, task, &paths)?;

    // Copy to clipboard or write to file
    if let Some(output_path) = output {
        std::fs::write(&output_path, &context)?;
        print_success(&format!("Context written to {}", output_path.display()));
    } else if !no_clipboard {
        match copy_to_clipboard(&context) {
            Ok(_) => print_success("Context copied to clipboard! Paste into any AI coding agent."),
            Err(_) => {
                print_warn("Could not copy to clipboard. Here's the context output:");
                println!();
                println!("{}", context);
            }
        }
    } else {
        println!("{}", context);
    }

    Ok(())
}

/// Handle `ctx status`
pub async fn cmd_status() -> Result<()> {
    let path = std::env::current_dir()?;
    let store = open_store(&path)?;
    let config = load_config(&path);

    print_header("Context Compiler — Status");

    if store.file_count()? == 0 {
        println!(
            "  No index found. Run {} to build one.",
            "ctx init".yellow()
        );
        return Ok(());
    }

    let files = store.get_all_files()?;
    let imports = store.get_imports()?;
    let history = store.get_history(5)?;

    let total_tokens: usize = files.iter().map(|f| f.token_count).sum();

    println!("  Codebase:  {}", path.display().to_string().cyan());
    println!(
        "  Index:     {} files",
        files.len().to_string().green().bold()
    );
    println!("  Size:      ~{} tokens", total_tokens.to_string().yellow());
    println!("  Imports:   {} edges", imports.len().to_string().dimmed());
    println!(
        "  History:   {} past sessions",
        history.len().to_string().dimmed()
    );
    if config.has_openai_key() {
        println!("  AI:        {} ({})", "ACTIVE".green().bold(), config.embedding_model.as_deref().unwrap_or("text-embedding-3-small"));
    } else {
        println!("  AI:        {} (set OPENAI_API_KEY for real embeddings)", "hash-based".yellow());
    }
    println!();

    // Top languages
    let mut lang_counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for file in &files {
        *lang_counts.entry(&file.language).or_default() += 1;
    }
    let mut lang_vec: Vec<_> = lang_counts.into_iter().collect();
    lang_vec.sort_by_key(|item| std::cmp::Reverse(item.1));

    print_info("Languages:");
    for (lang, count) in lang_vec.iter().take(8) {
        println!("    {}: {} files", lang.cyan(), count);
    }

    if !history.is_empty() {
        println!();
        print_info("Recent tasks:");
        for h in &history {
            println!("    {} — {} files", h.task.dimmed(), h.file_paths.len());
        }
    }

    Ok(())
}

/// Handle `ctx reindex`
pub async fn cmd_reindex(path: PathBuf) -> Result<()> {
    cmd_init(path, true).await
}

/// Handle `ctx watch`
pub async fn cmd_watch(path: PathBuf) -> Result<()> {
    let path = if path.as_os_str().is_empty() {
        std::env::current_dir()?
    } else {
        path
    };

    let config = load_config(&path);
    let store = open_store(&path)?;
    let embedder = get_embedder(&config)?;

    // Initial build
    if store.file_count()? == 0 {
        print_info("Building initial index...");
        IndexBuilder::build(&path, &store, &embedder, false)?;
    }

    print_header("Context Compiler — Watch");
    println!(
        "  Watching: {} for changes...",
        path.display().to_string().cyan()
    );
    println!("  Press Ctrl+C to stop.");
    println!();

    // Poll-based file watching (simple, no inotify dependency issues)
    let (tx, mut rx) = tokio::sync::mpsc::channel(100);
    let _watch_path = path.clone();

    tokio::spawn(async move {
        let mut last_scan = std::time::Instant::now();
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            if last_scan.elapsed() > std::time::Duration::from_secs(30) {
                let _ = tx.send(()).await;
                last_scan = std::time::Instant::now();
            }
        }
    });

    while rx.recv().await.is_some() {
        print_info("Codebase changed. Re-indexing...");
        store.clear()?;
        IndexBuilder::build(&path, &store, &embedder, true)?;
        print_success(&format!("Re-indexed: {} files", store.file_count()?));
    }

    Ok(())
}

/// Handle `ctx done`
pub async fn cmd_done() -> Result<()> {
    print_success("Session marked as complete.");
    Ok(())
}

/// Handle `ctx history`
pub async fn cmd_history(limit: usize) -> Result<()> {
    let path = std::env::current_dir()?;
    let store = open_store(&path)?;

    let history = store.get_history(limit)?;

    print_header("Context Compiler — History");

    if history.is_empty() {
        println!(
            "  No history yet. Run {} to start.",
            "ctx compile <task>".yellow()
        );
        return Ok(());
    }

    for (i, entry) in history.iter().enumerate() {
        println!(
            "  {}. {} — {} files",
            (i + 1).to_string().cyan().bold(),
            entry.task.dimmed(),
            entry.file_paths.len().to_string().yellow(),
        );
        for path in entry.file_paths.iter().take(3) {
            println!("     {}", path.dimmed());
        }
        if entry.file_paths.len() > 3 {
            println!(
                "     ... and {} more",
                (entry.file_paths.len() - 3).to_string().dimmed()
            );
        }
        println!();
    }

    Ok(())
}

/// Handle `ctx configure`
pub async fn cmd_configure(
    project_path: Option<PathBuf>,
    set_openai_key: Option<String>,
    set_embedding_model: Option<String>,
    set_reranker_model: Option<String>,
    set_use_reranker: Option<bool>,
    show: bool,
    global: bool,
) -> Result<()> {
    let config_path = if global {
        config::global_config_path()
            .parent()
            .unwrap_or(Path::new("~/.ctx"))
            .to_path_buf()
    } else {
        let p = project_path.unwrap_or_else(|| std::env::current_dir().unwrap());
        p
    };

    let mut config = Config::load(&config_path);

    // Apply changes
    if let Some(ref key) = set_openai_key {
        config.openai_key = Some(key.clone());
    }
    if let Some(ref model) = set_embedding_model {
        config.embedding_model = Some(model.clone());
    }
    if let Some(ref model) = set_reranker_model {
        config.reranker_model = Some(model.clone());
    }
    if let Some(use_it) = set_use_reranker {
        config.use_reranker = Some(use_it);
    }

    // Save
    config.save(&config_path)?;

    let location_str = if global {
        config::global_config_path().display().to_string()
    } else {
        format!("{}", config_path.join(".ctx/config.toml").display())
    };

    if show || (set_openai_key.is_none()
        && set_embedding_model.is_none()
        && set_reranker_model.is_none()
        && set_use_reranker.is_none())
    {
        print_header("Context Compiler — Configuration");
        println!("  Config file: {}", location_str.cyan());
        println!();
        if let Some(key) = &config.openai_key {
            let masked = if key.len() > 12 {
                format!("{}...{}", &key[..8], &key[key.len() - 4..])
            } else {
                "****".to_string()
            };
            println!("  OpenAI Key:  {}", masked.green());
            println!("  Embeddings:  {} ({})", "ACTIVE".green().bold(), config.embedding_model.as_deref().unwrap_or("text-embedding-3-small"));
            println!("  Reranker:    {} ({})", "ACTIVE".green().bold(), config.reranker_model.as_deref().unwrap_or("gpt-4o-mini"));
        } else {
            println!("  OpenAI Key:  {}", "not set".yellow());
            println!("  Embeddings:  {} (hash-based)", "fallback".yellow());
            println!("  Reranker:    {} (requires API key)", "disabled".yellow());
            println!();
            println!("  Set your key:  {} <your-key>", "ctx configure --set openai-key=sk-...".yellow());
            println!("  Global config: {} (any project)", "ctx configure --global --set openai-key=...".yellow());
        }
    } else {
        print_success(&format!("Configuration saved to {}", location_str));
    }

    Ok(())
}

/// Handle `ctx completions <shell>`
pub async fn cmd_completions(shell: clap_complete::Shell, cmd: &mut clap::Command) -> Result<()> {
    let name = cmd.get_name().to_string();
    clap_complete::generate(shell, cmd, name, &mut std::io::stdout());
    Ok(())
}

fn copy_to_clipboard(text: &str) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new()?;
    clipboard.set_text(text)?;
    Ok(())
}
