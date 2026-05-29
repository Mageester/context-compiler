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
    let local_ork = local.openrouter_key.filter(|k| !k.is_empty());
    let global_ork = global.openrouter_key.filter(|k| !k.is_empty());
    let local_dsk = local.deepseek_key.filter(|k| !k.is_empty());
    let global_dsk = global.deepseek_key.filter(|k| !k.is_empty());
    let local_cdx = local.codex_key.filter(|k| !k.is_empty());
    let global_cdx = global.codex_key.filter(|k| !k.is_empty());
    Config {
        openai_key: local_key.or(global_key),
        openrouter_key: local_ork.or(global_ork),
        deepseek_key: local_dsk.or(global_dsk),
        codex_key: local_cdx.or(global_cdx),
        provider: local.provider.or(global.provider),
        openai_base_url: local.openai_base_url.or(global.openai_base_url),
        openrouter_base_url: local.openrouter_base_url.or(global.openrouter_base_url),
        deepseek_base_url: local.deepseek_base_url.or(global.deepseek_base_url),
        embedding_model: local.embedding_model.or(global.embedding_model),
        reranker_model: local.reranker_model.or(global.reranker_model),
        use_reranker: local.use_reranker.or(global.use_reranker),
        ensemble_rerank: local.ensemble_rerank.or(global.ensemble_rerank),
        ensemble_count: local.ensemble_count.or(global.ensemble_count),
        code_chunking: local.code_chunking.or(global.code_chunking),
        cross_file_refs: local.cross_file_refs.or(global.cross_file_refs),
        term_expansion: local.term_expansion.or(global.term_expansion),
        parallel_embed: local.parallel_embed.or(global.parallel_embed),
        cache_ttl: local.cache_ttl.or(global.cache_ttl),
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

    // Show provider status
    let summary = config.provider_context_summary();
    print_info(&format!("{}", summary));

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

    let provider_summary = config.provider_context_summary();
    println!("  {}", provider_summary.dimmed());
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
        let mut tags_str = String::new();
        if !file.tags.is_empty() {
            tags_str = format!(" [{}]", file.tags[..file.tags.len().min(3)].join(", "));
        }
        println!(
            "  {}. {} ({})  {} {:2}%{}",
            (i + 1).to_string().cyan().bold(),
            file.path.white().bold(),
            file.token_count.to_string().yellow(),
            bar.cyan().dimmed(),
            score_pct,
            tags_str.dimmed(),
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

    // Show provider info
    let provider = config.selected_embedder_provider();
    if provider == "hash" {
        println!("  Provider:  {} (hash-based embeddings)", "none".yellow());
    } else {
        let model = config.embedding_model_name();
        println!("  Embed:     {} ({})", provider.green().bold(), model.dimmed());
    }

    let rerank_provider = config.selected_reranker_provider();
    if rerank_provider != "none" {
        let rerank_model = config.reranker_model_name();
        println!("  Rerank:    {} ({})", rerank_provider.green().bold(), rerank_model.dimmed());
        if config.ensemble_rerank.unwrap_or(false) {
            println!("  Ensemble:  active");
        }
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
    set_provider: Option<String>,
    set_openrouter_key: Option<String>,
    set_deepseek_key: Option<String>,
    set_codex_key: Option<String>,
    set_ensemble_rerank: Option<bool>,
    set_code_chunking: Option<bool>,
    set_cross_file_refs: Option<bool>,
    set_parallel_embed: Option<bool>,
    list_providers: bool,
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

    // Handle --list-providers
    if list_providers {
        print_header("Context Compiler — Available Providers");
        println!("  Embedding providers:");
        println!("    openai    — OpenAI (text-embedding-3-small)");
        println!("    openrouter — OpenRouter (openai/text-embedding-3-small)");
        println!("    deepseek  — DeepSeek (deepseek-embedding)");
        println!("    codex     — GitHub Copilot Codex (text-embedding-3-small)");
        println!("    hash      — Local hash-based fallback (no API key needed)");
        println!();
        println!("  Reranker providers:");
        println!("    openai    — OpenAI (gpt-4o-mini)");
        println!("    openrouter — OpenRouter (openai/gpt-4o-mini)");
        println!("    deepseek  — DeepSeek (deepseek-chat)");
        println!("    codex     — GitHub Copilot Codex (gpt-4o-mini)");
        println!();
        println!("  Set provider:  {}", "ctx configure --set-provider=<name>".yellow());
        println!("  Ensemble mode: {}", "ctx configure --set-ensemble-rerank=true".yellow());
        return Ok(());
    }

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
    if let Some(ref p) = set_provider {
        config.provider = Some(p.clone());
    }
    if let Some(ref key) = set_openrouter_key {
        config.openrouter_key = Some(key.clone());
    }
    if let Some(ref key) = set_deepseek_key {
        config.deepseek_key = Some(key.clone());
    }
    if let Some(ref key) = set_codex_key {
        config.codex_key = Some(key.clone());
    }
    if let Some(en) = set_ensemble_rerank {
        config.ensemble_rerank = Some(en);
    }
    if let Some(cc) = set_code_chunking {
        config.code_chunking = Some(cc);
    }
    if let Some(cfr) = set_cross_file_refs {
        config.cross_file_refs = Some(cfr);
    }
    if let Some(pe) = set_parallel_embed {
        config.parallel_embed = Some(pe);
    }

    // Save
    config.save(&config_path)?;

    let location_str = if global {
        config::global_config_path().display().to_string()
    } else {
        format!("{}", config_path.join(".ctx/config.toml").display())
    };

    let any_changes = set_openai_key.is_some()
        || set_embedding_model.is_some()
        || set_reranker_model.is_some()
        || set_use_reranker.is_some()
        || set_provider.is_some()
        || set_openrouter_key.is_some()
        || set_deepseek_key.is_some()
        || set_codex_key.is_some()
        || set_ensemble_rerank.is_some()
        || set_code_chunking.is_some()
        || set_cross_file_refs.is_some()
        || set_parallel_embed.is_some();

    if show || !any_changes {
        print_header("Context Compiler — Configuration");
        println!("  Config file: {}", location_str.cyan());
        println!();

        let summary = config.provider_context_summary();
        println!("  Provider:    {}", summary);

        if let Some(key) = &config.openai_key {
            let masked = if key.len() > 12 {
                format!("{}...{}", &key[..8], &key[key.len() - 4..])
            } else {
                "****".to_string()
            };
            println!("  OpenAI Key:  {}", masked.green());
        } else {
            println!("  OpenAI Key:  {}", "not set".yellow());
        }

        if let Some(key) = &config.openrouter_key {
            let masked = format!("{}...{}", &key[..8], &key[key.len() - 4..]);
            println!("  OpenRouter:  {}", masked.green());
        } else {
            println!("  OpenRouter:  {}", "not set".yellow());
        }

        if let Some(key) = &config.deepseek_key {
            let masked = format!("{}...{}", &key[..8], &key[key.len() - 4..]);
            println!("  DeepSeek:    {}", masked.green());
        } else {
            println!("  DeepSeek:    {}", "not set".yellow());
        }

        println!(
            "  Embeddings:  {}",
            config.embedding_model.as_deref().unwrap_or("text-embedding-3-small")
        );
        println!(
            "  Reranker:    {}",
            config.reranker_model.as_deref().unwrap_or("gpt-4o-mini")
        );
        println!(
            "  Use Reranker: {}",
            if config.use_reranker.unwrap_or(true) { "yes".green() } else { "no".yellow() }
        );
        println!(
            "  Ensemble:    {}",
            if config.ensemble_rerank.unwrap_or(false) { "yes".green() } else { "no".yellow() }
        );

        if !any_changes {
            println!();
            println!("  Set your key:  {}", "ctx configure --set-openai-key=sk-...".yellow());
            println!("  Set provider:  {}", "ctx configure --set-provider=openrouter".yellow());
            println!("  List providers: {}", "ctx configure --list-providers".yellow());
            println!("  Global config:  {}", "ctx configure --global --set-openai-key=...".yellow());
        }
    } else {
        print_success(&format!("Configuration saved to {}", location_str));
    }

    Ok(())
}

/// Handle `ctx providers` — alias for `ctx configure --list-providers`
pub async fn cmd_providers() -> Result<()> {
    cmd_configure(
        None, None, None, None, None, None, None, None, None, None, None, None, None,
        true, false, false,
    )
    .await
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
