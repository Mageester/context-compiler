use anyhow::Result;
use chrono::Utc;
use std::path::Path;
use uuid::Uuid;

use crate::config::Config;
use crate::embed::Embedder;
use crate::rerank::Reranker;
use crate::signal::RelevanceEngine;
use crate::store::{HistoryEntry, Store};
use crate::trim::Trimmer;

/// The context compilation pipeline.
/// 4-stage pipeline:
///   1. Fast lexical filter (FTS5 BM25 + filename boost)
///   2. Embedding similarity scoring
///   3. Dependency propagation + history
///   4. AI reranker (optional, premium)
pub struct Compiler;

impl Compiler {
    /// Compile context for a natural language task.
    /// Returns (formatted_context_string, scored_files, total_tokens_trimmed).
    pub fn compile(
        path: &Path,
        store: &Store,
        embedder: &Embedder,
        config: &Config,
        task: &str,
        budget: usize,
        max_files: usize,
    ) -> Result<(String, Vec<crate::signal::ScoredFile>, usize)> {
        // 1. Embed the task (for stage 2 - semantic scoring)
        let task_embedding = embedder.embed(task);
        log::info!("[Stage 1/4] Task embedded for semantic matching");

        // 2. Score all files (stages 1-3: FTS5 + embedding + dependencies)
        let scored = RelevanceEngine::score(store, embedder, &task_embedding, task)?;
        log::info!("[Stage 2/4] Scored {} files", scored.len());

        // 3. AI reranker (stage 4 - premium accuracy)
        let has_key = config.has_openai_key()
            || config.has_openrouter_key()
            || config.has_deepseek_key()
            || config.has_codex_key();
        let use_reranker = config.use_reranker.unwrap_or(true) && has_key;

        let ranked = if use_reranker && scored.len() > 1 {
            log::info!(
                "[Stage 3/4] Running AI reranker ({}) on top candidates...",
                config.selected_reranker_provider()
            );
            let reranker = Reranker::new(config);
            match reranker.rerank(task, &scored, path) {
                Ok(reranked) => {
                    log::info!("[Stage 3/4] Reranker returned {} files", reranked.len());
                    reranked
                }
                Err(e) => {
                    log::warn!("[Stage 3/4] Reranker failed: {}. Using algorithmic scores.", e);
                    scored
                }
            }
        } else {
            if !use_reranker {
                log::info!("[Stage 3/4] AI reranker skipped (no API key configured)");
            }
            scored
        };

        // 4. Format context with trimmed file contents while enforcing the requested budget.
        log::info!("[Stage 4/4] Formatting context within {} token budget", budget);
        let codebase_path = path.canonicalize()?;
        let (context, total_trimmed, selected) =
            Trimmer::format_context(&ranked, task, budget, max_files, |file_path| {
                let full_path = codebase_path.join(file_path);
                std::fs::read_to_string(&full_path).ok()
            });

        if selected.is_empty() {
            return Ok((
                "// No relevant files found for this task.".to_string(),
                Vec::new(),
                0,
            ));
        }

        log::info!(
            "[Done] Selected {} files ({} trimmed tokens) using {} for: {}",
            selected.len(),
            total_trimmed,
            config.provider_context_summary(),
            task
        );

        Ok((context, selected, total_trimmed))
    }

    /// Save a compilation result to history
    pub fn save_to_history(
        store: &Store,
        embedder: &Embedder,
        task: &str,
        file_paths: &[String],
    ) -> Result<()> {
        let task_embedding = embedder.embed(task);
        let entry = HistoryEntry {
            id: Uuid::new_v4().to_string(),
            task: task.to_string(),
            task_embedding,
            file_paths: file_paths.to_vec(),
            created_at: Utc::now().to_rfc3339(),
        };
        store.add_history(&entry)?;
        log::info!("Saved session to history");
        Ok(())
    }
}
