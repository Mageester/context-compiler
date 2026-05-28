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
        let original_tokens = source.len().div_ceil(4);
        let trimmed = Self::trim_content(source, language);
        let trimmed_tokens = trimmed.len().div_ceil(4);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trim_rust_code_preserves_signatures() {
        let source = r#"
fn main() {
    println!("hello");
    let x = 1;
}
"#;
        let (trimmed, original, after) = Trimmer::trim(source, "rust").unwrap();
        assert!(original > 0);
        assert!(after <= original);
        assert!(trimmed.contains("fn main()"));
    }

    #[test]
    fn test_trim_removes_console_log() {
        let source = "function foo() {\n  console.log('debug');\n  return 42;\n}";
        let (trimmed, _, _) = Trimmer::trim(source, "typescript").unwrap();
        assert!(!trimmed.contains("console.log"));
        assert!(trimmed.contains("return 42"));
    }

    #[test]
    fn test_trim_removes_println() {
        let source = "fn main() {\n    println!(\"hello\");\n    let x = 1;\n}";
        let (trimmed, _, _) = Trimmer::trim(source, "rust").unwrap();
        assert!(!trimmed.contains("println!"));
    }

    #[test]
    fn test_trim_keeps_docstrings() {
        let source = "/// Important docs\nfn documented() {}";
        let (trimmed, _, _) = Trimmer::trim(source, "rust").unwrap();
        assert!(trimmed.contains("///"));
        assert!(trimmed.contains("documented"));
    }

    #[test]
    fn test_trim_removes_single_line_comments() {
        let source = "// comment\nlet x = 1;";
        let (trimmed, _, _) = Trimmer::trim(source, "rust").unwrap();
        assert!(!trimmed.contains("// comment"));
        assert!(trimmed.contains("x = 1"));
    }

    #[test]
    fn test_trim_removes_block_comments() {
        let source = "let a = 1;\n/* block\ncomment */\nlet b = 2;";
        let (trimmed, _, _) = Trimmer::trim(source, "rust").unwrap();
        assert!(!trimmed.contains("/*"));
        assert!(!trimmed.contains("block"));
        assert!(trimmed.contains("a = 1"));
        assert!(trimmed.contains("b = 2"));
    }

    #[test]
    fn test_trim_empty_source() {
        let (trimmed, orig, after) = Trimmer::trim("", "rust").unwrap();
        assert!(trimmed.is_empty());
        assert_eq!(orig, 0);
        assert_eq!(after, 0);
    }

    #[test]
    fn test_format_context_basic() {
        let files = vec![ScoredFile {
            path: "test.rs".into(),
            summary: "test file".into(),
            token_count: 50,
            language: "rust".into(),
            score: 0.95,
            semantic_score: 0.0,
            dependency_score: 0.0,
            history_score: 0.0,
        }];
        let result = Trimmer::format_context(&files, "test task", |_path| {
            Some("fn hello() {\n    println!(\"hi\");\n}".into())
        });
        assert!(result.contains("test task"));
        assert!(result.contains("test.rs"));
        assert!(result.contains("0.95"));
        assert!(result.contains("trimmed to"));
    }

    #[test]
    fn test_format_context_file_not_found() {
        let files = vec![ScoredFile {
            path: "missing.rs".into(),
            summary: "".into(),
            token_count: 0,
            language: "rust".into(),
            score: 0.5,
            semantic_score: 0.0,
            dependency_score: 0.0,
            history_score: 0.0,
        }];
        let result = Trimmer::format_context(&files, "test", |_| None);
        assert!(result.contains("File not found"));
    }

    #[test]
    fn test_trim_removes_hash_comments() {
        let source = "# a comment\ndef foo(): pass";
        let (trimmed, _, _) = Trimmer::trim(source, "python").unwrap();
        assert!(!trimmed.contains("# a comment"));
        assert!(trimmed.contains("def foo"));
    }

    #[test]
    fn test_trim_preserves_core_logic() {
        let source = "fn calculate(x: i32) -> i32 {\n    // temporary debug\n    x * 2\n}";
        let (trimmed, _, _) = Trimmer::trim(source, "rust").unwrap();
        // Should keep the function signature and return expression
        assert!(trimmed.contains("calculate"));
        assert!(trimmed.contains("x * 2"));
    }
}
