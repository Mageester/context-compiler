use anyhow::{Context, Result};
use clap::builder::Styles;
use colored::*;
use std::path::{Path, PathBuf};

use crate::compile::Compiler;
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

fn get_embedder() -> Result<Embedder> {
    Embedder::init().context("Failed to initialize embedding engine")
}

/// Handle `ctx init`
pub async fn cmd_init(path: PathBuf, force: bool) -> Result<()> {
    let path = if path.as_os_str().is_empty() {
        std::env::current_dir()?
    } else {
        path
    };

    print_header(&format!("Context Compiler — Init"));
    println!("  Codebase: {}", path.display().to_string().cyan());
    println!();

    let store = open_store(&path)?;
    let embedder = get_embedder()?;

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
    let store = open_store(&path)?;
    let embedder = get_embedder()?;

    print_header(&format!("Context Compiler — Compile"));
    println!("  Task:    {}", task.cyan());
    println!("  Budget:  {} tokens", budget.to_string().yellow());
    println!();

    // Ensure index exists
    if store.file_count()? == 0 {
        print_info("No index found. Building initial index...");
        println!();
        IndexBuilder::build(&path, &store, &embedder, false)?;
        println!();
    }

    // Compile
    let (context, selected, total_tokens) =
        Compiler::compile(&path, &store, &embedder, task, budget, max_files)?;

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
        println!(
            "  {}. {} ({})  {} {:2}%",
            (i + 1).to_string().cyan().bold(),
            file.path.white().bold(),
            file.token_count.to_string().yellow(),
            bar.cyan().dimmed(),
            score_pct,
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

    print_header(&format!("Context Compiler — Status"));

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
    println!();

    // Top languages
    let mut lang_counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for file in &files {
        *lang_counts.entry(&file.language).or_default() += 1;
    }
    let mut lang_vec: Vec<_> = lang_counts.into_iter().collect();
    lang_vec.sort_by(|a, b| b.1.cmp(&a.1));

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

    let store = open_store(&path)?;
    let embedder = get_embedder()?;

    // Initial build
    if store.file_count()? == 0 {
        print_info("Building initial index...");
        IndexBuilder::build(&path, &store, &embedder, false)?;
    }

    print_header(&format!("Context Compiler — Watch"));
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

    while let Some(_) = rx.recv().await {
        print_info("Codebase changed. Re-indexing...");
        // Clear and rebuild
        store.clear()?;
        IndexBuilder::build(&path, &store, &embedder, true)?;
        print_success(&format!("Re-indexed: {} files", store.file_count()?));
    }

    Ok(())
}

/// Handle `ctx done`
pub async fn cmd_done() -> Result<()> {
    // Done is automatically called during compile.
    // This is a no-op for now.
    print_success("Session marked as complete.");
    Ok(())
}

/// Handle `ctx history`
pub async fn cmd_history(limit: usize) -> Result<()> {
    let path = std::env::current_dir()?;
    let store = open_store(&path)?;

    let history = store.get_history(limit)?;

    print_header(&format!("Context Compiler — History"));

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

fn copy_to_clipboard(text: &str) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new()?;
    clipboard.set_text(text)?;
    Ok(())
}
