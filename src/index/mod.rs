use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use walkdir::WalkDir;

use crate::embed::Embedder;
use crate::store::{FileEntry, ImportEdge, Store};
use crate::tree::Parser;

/// Builds the index from a codebase directory.
pub struct IndexBuilder;

impl IndexBuilder {
    /// Index or re-index a codebase at the given path
    pub fn build(path: &Path, store: &Store, embedder: &Embedder, force: bool) -> Result<()> {
        if !force && store.file_count()? > 0 {
            log::info!(
                "Index already exists ({} files). Use --force to rebuild.",
                store.file_count()?
            );
            return Ok(());
        }

        log::info!("Indexing codebase at: {}", path.display());
        let files = Self::collect_files(path)?;

        let pb = ProgressBar::new(files.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} files ({msg})")
                .unwrap()
                .progress_chars("##-"),
        );

        for file_path in &files {
            pb.set_message(file_path.trim_start_matches(&format!("{}", path.display())).to_string());

            let source = match std::fs::read_to_string(file_path) {
                Ok(s) => s,
                Err(_) => continue,
            };

            let language = Parser::detect_language(file_path);
            let lang = language.unwrap_or("unknown");

            if language.is_none() && !file_path.ends_with(".md") && !file_path.ends_with(".txt") && !file_path.ends_with(".yaml") && !file_path.ends_with(".yml") && !file_path.ends_with(".json") && !file_path.ends_with(".toml") {
                pb.inc(1);
                continue;
            }

            let parsed = match Parser::parse(&source, lang) {
                Ok(p) => p,
                Err(_) => {
                    pb.inc(1);
                    continue;
                }
            };

            // Build summary from file content
            let summary = if parsed.summary.is_empty() {
                // Use first meaningful lines as fallback
                source
                    .lines()
                    .take(10)
                    .filter(|l| !l.trim().is_empty())
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                parsed.summary.clone()
            };

            let summary = summary.chars().take(500).collect::<String>();

            // Embed the file summary
            let embedding = embedder.embed(&summary);

            let relative_path = file_path
                .strip_prefix(path)
                .unwrap_or(file_path)
                .to_string_lossy()
                .to_string();

            let entry = FileEntry {
                path: relative_path.clone(),
                summary,
                token_count: parsed.token_count,
                language: lang.to_string(),
                tree_hash: parsed.tree_hash,
                embedding: Some(embedding),
            };

            if let Err(e) = store.upsert_file(&entry) {
                log::warn!("Failed to index {}: {}", relative_path, e);
            }

            // Store imports
            for imp in &parsed.imports {
                let resolved = Self::resolve_import(&relative_path, imp);
                if let Some(to_path) = resolved {
                    let edge = ImportEdge {
                        from_path: relative_path.clone(),
                        to_path,
                    };
                    let _ = store.upsert_import(&edge);
                }
            }

            pb.inc(1);
        }

        pb.finish_with_message(format!("{} files indexed", files.len()));
        Ok(())
    }

    /// Collect all code files in a directory
    fn collect_files(path: &Path) -> Result<Vec<std::path::PathBuf>> {
        let mut files = Vec::new();
        for entry in WalkDir::new(path)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !Parser::is_ignored(e.path().to_string_lossy().as_ref()))
        {
            let entry = entry?;
            if entry.file_type().is_file() {
                files.push(entry.path().to_path_buf());
            }
        }
        files.sort();
        Ok(files)
    }

    /// Resolve an import statement to a file path
    fn resolve_import(relative_path: &str, import_stmt: &str) -> Option<String> {
        // Extract the module path from various import syntaxes
        let import_path = if import_stmt.starts_with("import ") {
            // TypeScript/JS: import { X } from './foo'
            import_stmt
                .split('\'')
                .nth(1)
                .or_else(|| import_stmt.split('"').nth(1))?
                .to_string()
        } else if import_stmt.starts_with("use ") {
            // Rust: use crate::module::foo
            import_stmt
                .split(';')
                .next()?
                .trim_start_matches("use ")
                .trim()
                .replace("::", "/")
                .replace("crate/", "")
                .replace("self/", "")
                .replace("super/", "../")
        } else if import_stmt.starts_with("from ") {
            // Python: from foo import bar
            import_stmt
                .split(' ')
                .nth(1)?
                .replace('.', "/")
        } else if import_stmt.starts_with("import ") {
            // Python: import foo
            import_stmt
                .split(' ')
                .nth(1)?
                .replace('.', "/")
        } else if import_stmt.starts_with("require(") {
            // JS: require('./foo')
            import_stmt
                .split('\'')
                .nth(1)
                .or_else(|| import_stmt.split('"').nth(1))?
                .to_string()
        } else {
            return None;
        };

        // Don't resolve external packages (npm, crates.io, etc.)
        if import_path.starts_with('.') || import_path.starts_with('/') || import_path.starts_with("..") {
            // Resolve relative to the file's directory
            let base_dir = std::path::Path::new(relative_path).parent()?;
            let resolved = base_dir.join(&import_path);
            let resolved_str = resolved.to_string_lossy().to_string();
            Some(resolved_str)
        } else {
            None
        }
    }
}
