use anyhow::Result;
use std::collections::HashMap;

use crate::embed::Embedder;
use crate::store::{HistoryEntry, Store};

/// The relevance engine: computes a composite score for every file.
/// Combines semantic similarity, dependency graph, and historical usage.
pub struct RelevanceEngine;

#[derive(Debug, Clone)]
pub struct ScoredFile {
    pub path: String,
    pub summary: String,
    pub token_count: usize,
    pub language: String,
    pub score: f32,
    pub semantic_score: f32,
    pub dependency_score: f32,
    pub history_score: f32,
}

impl RelevanceEngine {
    /// Score all files in the index against a task embedding.
    pub fn score(
        store: &Store,
        _embedder: &Embedder,
        task_embedding: &[f32],
        _task_text: &str,
    ) -> Result<Vec<ScoredFile>> {
        let files = store.get_all_files()?;
        let imports = store.get_imports()?;
        let history = store.get_history(20)?;

        // Build import graph
        let import_map: HashMap<&str, Vec<&str>> = {
            let mut map: HashMap<&str, Vec<&str>> = HashMap::new();
            for edge in &imports {
                map.entry(edge.from_path.as_str())
                    .or_default()
                    .push(edge.to_path.as_str());
            }
            map
        };

        // Find similar past tasks
        let similar_history: Vec<&HistoryEntry> = history
            .iter()
            .filter(|h| Embedder::cosine_similarity(task_embedding, &h.task_embedding) > 0.3)
            .collect();

        // Count how many times each file appeared in similar past contexts
        let history_counts: HashMap<&str, usize> = {
            let mut counts: HashMap<&str, usize> = HashMap::new();
            for entry in &similar_history {
                for path in &entry.file_paths {
                    *counts.entry(path.as_str()).or_default() += 1;
                }
            }
            counts
        };

        let mut scored = Vec::new();

        for file in &files {
            let embedding = match &file.embedding {
                Some(e) => e,
                None => continue,
            };

            // Signal 1: Semantic similarity (weight: 0.5)
            let semantic = Embedder::cosine_similarity(task_embedding, embedding);

            // Signal 2: Dependency score (weight: 0.3)
            let dependency = Self::compute_dependency_score(&file.path, &import_map, semantic);

            // Signal 3: History score (weight: 0.2)
            let history_score = Self::compute_history_score(&file.path, &history_counts);

            let score = semantic * 0.5 + dependency * 0.3 + history_score * 0.2;

            // Keep a low floor: small repos and one-word tasks should still return
            // useful candidates instead of a false "no files found".
            if score > 0.0 {
                scored.push(ScoredFile {
                    path: file.path.clone(),
                    summary: file.summary.clone(),
                    token_count: file.token_count,
                    language: file.language.clone(),
                    score,
                    semantic_score: semantic,
                    dependency_score: dependency,
                    history_score,
                });
            }
        }

        // Sort by score descending
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(scored)
    }

    /// Compute dependency score: files that are imported by high-relevance files get a boost.
    fn compute_dependency_score(
        file_path: &str,
        import_map: &HashMap<&str, Vec<&str>>,
        _base_semantic: f32,
    ) -> f32 {
        // If a file has high semantic relevance, propagate to its dependencies.
        // Simplified: boost files that appear as dependencies of other files.
        let mut deps = 0.0_f32;

        // Check if this file is imported by high-scoring files
        let dependency_of = import_map
            .iter()
            .filter(|(_, deps)| deps.iter().any(|d| *d == file_path))
            .count();

        if dependency_of > 0 {
            deps = (dependency_of as f32).min(5.0) / 5.0;
        }

        // Boost the file itself if it has imports (it's a "hub" file)
        if let Some(imports) = import_map.get(file_path) {
            if imports.len() >= 3 {
                deps = deps.max(0.3);
            }
        }

        deps * 0.5
    }

    /// Compute history score based on past similar tasks
    fn compute_history_score(file_path: &str, history_counts: &HashMap<&str, usize>) -> f32 {
        match history_counts.get(file_path) {
            Some(count) => {
                let normalized = (*count as f32).min(10.0) / 10.0;
                normalized
            }
            None => 0.0,
        }
    }

    /// Select top files up to the given token budget.
    pub fn select_top(scored: Vec<ScoredFile>, budget: usize, max_files: usize) -> Vec<ScoredFile> {
        let mut selected = Vec::new();
        let mut total_tokens = 0;

        // Always include the top scorer if it exists
        if scored.is_empty() {
            return selected;
        }

        for file in scored {
            if max_files > 0 && selected.len() >= max_files {
                break;
            }

            let trimmed_tokens = (file.token_count as f64 * 0.4) as usize; // Assume trim saves ~60%
            if total_tokens + trimmed_tokens > budget && !selected.is_empty() {
                break;
            }

            total_tokens += trimmed_tokens;
            selected.push(file);
        }

        selected
    }
}
