use anyhow::Result;
use chrono::Utc;
use std::path::Path;
use uuid::Uuid;

use crate::embed::Embedder;
use crate::signal::RelevanceEngine;
use crate::store::{HistoryEntry, Store};
use crate::trim::Trimmer;

/// The context compilation pipeline.
pub struct Compiler;

impl Compiler {
    /// Compile context for a natural language task.
    /// Returns (formatted_context_string, scored_files, total_tokens_trimmed).
    pub fn compile(
        path: &Path,
        store: &Store,
        embedder: &Embedder,
        task: &str,
        budget: usize,
        max_files: usize,
    ) -> Result<(String, Vec<crate::signal::ScoredFile>, usize)> {
        // 1. Embed the task
        let task_embedding = embedder.embed(task);
        log::info!("Task embedded: {:?}", task);

        // 2. Score all files
        let scored = RelevanceEngine::score(store, embedder, &task_embedding, task)?;
        log::info!("Scored {} files", scored.len());

        // 3. Select top files within budget
        let selected = RelevanceEngine::select_top(scored, budget, max_files);
        log::info!(
            "Selected {} files within {} token budget",
            selected.len(),
            budget
        );

        if selected.is_empty() {
            return Ok((
                "// No relevant files found for this task.".to_string(),
                Vec::new(),
                0,
            ));
        }

        // 4. Format context with trimmed file contents
        let codebase_path = path.canonicalize()?;
        let context = Trimmer::format_context(&selected, task, |file_path| {
            let full_path = codebase_path.join(file_path);
            std::fs::read_to_string(&full_path).ok()
        });

        // 5. Calculate total trimmed tokens
        let total_trimmed: usize = selected.iter().map(|f| f.token_count).sum();

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
