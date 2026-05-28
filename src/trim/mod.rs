use anyhow::Result;

use crate::signal::ScoredFile;

/// Trims code files to fit the token budget.
/// Strips comments, blank lines, logging, and boilerplate while keeping
/// function signatures, types, interfaces, and core logic.
pub struct Trimmer;

impl Trimmer {
    /// Trim a file's content, keeping only the important parts.
    /// Returns (trimmed_content, original_tokens, trimmed_tokens).
    pub fn trim(source: &str, language: &str) -> Result<(String, usize, usize)> {
        let original_tokens = (source.len() + 3) / 4;
        let trimmed = Self::trim_content(source, language);
        let trimmed_tokens = (trimmed.len() + 3) / 4;
        Ok((trimmed, original_tokens, trimmed_tokens))
    }

    fn trim_content(source: &str, _language: &str) -> String {
        let mut out = Vec::new();
        let mut in_block_comment = false;

        for line in source.lines() {
            let trimmed = line.trim();

            // Track block comments
            if trimmed.starts_with("/*") && !trimmed.ends_with("*/") {
                in_block_comment = true;
                continue;
            }
            if in_block_comment {
                if trimmed.ends_with("*/") || trimmed.contains("*/") {
                    in_block_comment = false;
                }
                continue;
            }

            // Skip single-line comments (keep docstrings)
            if trimmed.starts_with("//")
                || trimmed.starts_with("# ")
                || trimmed.starts_with("#!")
                || trimmed.starts_with("-- ")
            {
                // Keep docstrings (///, /**)
                if !trimmed.starts_with("///") && !trimmed.starts_with("/**") {
                    continue;
                }
            }

            // Skip logging lines
            let lower = trimmed.to_lowercase();
            if lower.contains("console.log")
                || lower.contains("logger.debug")
                || lower.contains("print(")
                || lower.contains("println!(")
                || lower.contains("dbg!(")
                || lower.contains("log::debug")
                || lower.contains("debug!(")
            {
                continue;
            }

            // Skip empty lines but keep structural whitespace
            if trimmed.is_empty() {
                out.push(String::new());
                continue;
            }

            out.push(line.to_string());
        }

        out.join("\n")
            .lines()
            .filter(|l| !l.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Format a compiled context output for a set of files
    pub fn format_context(
        files: &[ScoredFile],
        task: &str,
        read_source: impl Fn(&str) -> Option<String>,
    ) -> String {
        let mut output = String::new();

        output.push_str(&format!("// Context Compiler — task: {}\n", task));
        output.push_str(&format!(
            "// Files: {} · Total tokens: (trimmed)\n",
            files.len()
        ));
        output.push_str("// ─────────────────────────────────────────\n\n");

        for file in files {
            // File header
            output.push_str(&format!(
                "// ═══ {} — {} tok (score: {:.2}) ═══\n",
                file.path, file.token_count, file.score
            ));

            // Read and trim the actual file content
            if let Some(source) = read_source(&file.path) {
                let language = &file.language;
                if let Ok((trimmed, _orig, trimmed_tok)) = Self::trim(&source, language) {
                    output.push_str(&trimmed);
                    output.push_str(&format!("\n// — trimmed to {} tokens —", trimmed_tok));
                } else {
                    output.push_str(&source);
                }
            } else {
                output.push_str(&format!("// File not found: {}\n", file.path));
            }

            output.push_str("\n\n");
        }

        output
    }
}
