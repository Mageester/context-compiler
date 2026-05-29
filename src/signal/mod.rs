use anyhow::Result;
use std::collections::HashMap;

use crate::embed::Embedder;
use crate::store::Store;

/// The relevance engine: computes a composite score for every file.
/// Uses a 4-stage pipeline:
///   1. Fast lexical filter (FTS5 BM25 + filename boost)
///   2. Embedding similarity scoring
///   3. Dependency propagation
///   4. Optional AI reranker (external module)
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
    pub lexical_score: f32,
}

impl RelevanceEngine {
    /// Score all files in the index against a task.
    pub fn score(
        store: &Store,
        _embedder: &Embedder,
        task_embedding: &[f32],
        task_text: &str,
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

        // --- Stage 1: FTS5 BM25 lexical scores ---
        let fts_scores = store.search_fts(task_text, 100)?;

        // --- Stage 1b: Exact filename boost ---
        let filename_boost_patterns: Vec<String> = task_text
            .split_whitespace()
            .filter(|w| w.contains('.') || w.contains('/') || w.contains('\\'))
            .map(|w| w.trim_end_matches(|c: char| c == ',' || c == '.' || c == '!' || c == '?'))
            .map(|w| w.to_lowercase())
            .collect();

        // --- Stage 2: History similarity ---
        let similar_history: Vec<&crate::store::HistoryEntry> = history
            .iter()
            .filter(|h| Embedder::cosine_similarity(task_embedding, &h.task_embedding) > 0.3)
            .collect();

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
            // --- Stage 1: Lexical score (BM25 from FTS5) ---
            let lexical = fts_scores
                .get(&file.path)
                .copied()
                .unwrap_or(0.0);

            // --- Stage 1b: Filename boost ---
            // Exact filename mention in the task is a VERY strong signal
            let path_lower = file.path.to_lowercase();
            let filename_boost = if filename_boost_patterns
                .iter()
                .any(|pat| {
                    // Exact match: pat is the exact filename
                    path_lower == *pat
                        // End match: path ends with the filename (e.g., "src/App.tsx" ends with "app.tsx")
                        || path_lower.ends_with(&format!("/{}", pat))
                        // Partial match: path contains the filename somewhere
                        || path_lower.contains(pat)
                })
            {
                // Higher boost if the task word is an exact path match
                let has_exact = filename_boost_patterns.iter().any(|pat| {
                    path_lower == *pat || path_lower.ends_with(&format!("/{}", pat))
                });
                if has_exact { 0.5 } else { 0.3 }
            } else {
                0.0
            };

            // --- Stage 2: Semantic similarity ---
            let semantic = match &file.embedding {
                Some(e) if !e.is_empty() => {
                    Embedder::cosine_similarity(task_embedding, e)
                }
                _ => 0.0,
            };

            // --- Stage 3: Dependency score ---
            // Files importing this file (it's a dependency of high-value targets) get boosted.
            // Also hub files (many imports) get a small boost.
            let dependency = Self::compute_dependency_score(&file.path, &import_map, &fts_scores);

            // --- Stage 4: History score ---
            let history_score = Self::compute_history_score(&file.path, &history_counts);

            // --- Composite score ---
            // Three primary signals are combined additively, then boosted by dependency/history.
            // Filename boost is a hard override: if the task explicitly names a file, it must appear.
            let lexical_weight = 0.40;
            let semantic_weight = 0.30;
            let filename_weight = 0.30;

            let base_score = lexical * lexical_weight
                + semantic * semantic_weight
                + filename_boost * filename_weight;

            // Boost by dependency and history scores
            let dep_boost = 1.0 + (dependency * 0.5);
            let hist_boost = 1.0 + (history_score * 0.3);
            let score = base_score * dep_boost * hist_boost;

            // Keep files with any score signal
            if score > 0.0 || filename_boost > 0.0 {
                scored.push(ScoredFile {
                    path: file.path.clone(),
                    summary: file.summary.clone(),
                    token_count: file.token_count,
                    language: file.language.clone(),
                    score,
                    semantic_score: semantic,
                    dependency_score: dependency,
                    history_score,
                    lexical_score: lexical,
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

    /// Compute dependency score: files imported by high-relevance files get a boost.
    /// Also, files that import high-relevance files get a reverse-dependency boost.
    fn compute_dependency_score(
        file_path: &str,
        import_map: &HashMap<&str, Vec<&str>>,
        fts_scores: &HashMap<String, f32>,
    ) -> f32 {
        let mut deps = 0.0_f32;

        // Forward dependency: this file is imported by others
        let depends_on_this: Vec<&&str> = import_map
            .iter()
            .filter(|(_, imported)| imported.contains(&file_path))
            .map(|(importer, _)| importer)
            .collect();

        if !depends_on_this.is_empty() {
            // Boost based on how many importers and their FTS scores
            let total: f32 = depends_on_this
                .iter()
                .map(|importer| {
                    let path: &str = importer;
                    fts_scores.get(path).copied().unwrap_or(0.1)
                })
                .sum();
            deps = deps.max((total / depends_on_this.len() as f32).min(0.8));
        }

        // Reverse dependency: this file imports others (hub file)
        if let Some(imports) = import_map.get(file_path) {
            if imports.len() >= 3 {
                deps = deps.max(0.3);
            }
            // If a high-FTS-score file is imported, this file gets a boost
            let imported_fts: f32 = imports
                .iter()
                .map(|imp| {
                    let path: &str = imp;
                    fts_scores.get(path).copied().unwrap_or(0.0)
                })
                .sum::<f32>()
                / imports.len() as f32;
            if imported_fts > 0.3 {
                deps = deps.max(imported_fts * 0.5);
            }
        }

        deps
    }

    /// Compute history score based on past similar tasks
    fn compute_history_score(file_path: &str, history_counts: &HashMap<&str, usize>) -> f32 {
        match history_counts.get(file_path) {
            Some(count) => (*count as f32).min(10.0) / 10.0,
            None => 0.0,
        }
    }

    /// Select top files up to the given token budget.
    pub fn select_top(scored: Vec<ScoredFile>, budget: usize, max_files: usize) -> Vec<ScoredFile> {
        let mut selected = Vec::new();
        let mut total_tokens = 0;

        if scored.is_empty() {
            return selected;
        }

        for file in scored {
            if max_files > 0 && selected.len() >= max_files {
                break;
            }

            let trimmed_tokens = (file.token_count as f64 * 0.4) as usize;
            if total_tokens + trimmed_tokens > budget && !selected.is_empty() {
                break;
            }

            total_tokens += trimmed_tokens;
            selected.push(file);
        }

        selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_scored(path: &str, tokens: usize, score: f32) -> ScoredFile {
        ScoredFile {
            path: path.to_string(),
            summary: String::new(),
            token_count: tokens,
            language: "rust".to_string(),
            score,
            semantic_score: score,
            dependency_score: 0.0,
            history_score: 0.0,
            lexical_score: 0.0,
        }
    }

    #[test]
    fn test_select_top_empty_input() {
        let result = RelevanceEngine::select_top(vec![], 8192, 0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_select_top_respects_budget() {
        let files = vec![
            make_scored("a.rs", 2000, 0.9),
            make_scored("b.rs", 2000, 0.8),
            make_scored("c.rs", 2000, 0.7),
            make_scored("d.rs", 2000, 0.6),
        ];
        let result = RelevanceEngine::select_top(files, 2500, 0);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_select_top_respects_max_files() {
        let files = vec![
            make_scored("a.rs", 100, 0.9),
            make_scored("b.rs", 100, 0.8),
            make_scored("c.rs", 100, 0.7),
        ];
        let result = RelevanceEngine::select_top(files, 10000, 2);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_select_top_always_includes_top_scorer() {
        let files = vec![
            make_scored("a.rs", 100_000, 0.9),
            make_scored("b.rs", 100, 0.8),
        ];
        let result = RelevanceEngine::select_top(files, 1000, 0);
        assert!(!result.is_empty());
        assert_eq!(result[0].path, "a.rs");
    }

    #[test]
    fn test_select_top_keeps_order() {
        let files = vec![
            make_scored("a.rs", 100, 0.9),
            make_scored("b.rs", 100, 0.5),
            make_scored("c.rs", 100, 0.3),
        ];
        let result = RelevanceEngine::select_top(files, 10000, 0);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].path, "a.rs");
        assert_eq!(result[1].path, "b.rs");
        assert_eq!(result[2].path, "c.rs");
    }

    #[test]
    fn test_select_top_includes_low_score_with_exact_match() {
        let files = vec![
            make_scored("src/App.tsx", 500, 0.1),
            make_scored("src/lib/repos.ts", 300, 0.05),
        ];
        let result = RelevanceEngine::select_top(files, 10000, 0);
        assert_eq!(result.len(), 2);
    }
}
