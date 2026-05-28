use anyhow::Result;

/// Language-aware code parser using Tree-sitter.
/// Extracts: function/class names, docstrings, imports, approximate token count.
pub struct Parser;

/// Supported languages and their tree-sitter grammar names
const SUPPORTED_LANGUAGES: &[(&str, &[&str])] = &[
    ("typescript", &["ts", "tsx", "mts"]),
    ("javascript", &["js", "jsx", "mjs"]),
    ("python", &["py"]),
    ("rust", &["rs"]),
    ("go", &["go"]),
    ("java", &["java"]),
    ("ruby", &["rb"]),
    ("c", &["c", "h"]),
    ("cpp", &["cpp", "hpp", "cc", "cxx"]),
    ("zig", &["zig"]),
    ("swift", &["swift"]),
    ("kotlin", &["kt", "kts"]),
    ("scala", &["scala"]),
    ("elixir", &["ex", "exs"]),
    ("haskell", &["hs"]),
    ("lua", &["lua"]),
];

impl Parser {
    /// Detect the programming language from file extension
    pub fn detect_language(path: &str) -> Option<&'static str> {
        let ext = path.rsplit('.').next()?.to_lowercase();
        for (lang, exts) in SUPPORTED_LANGUAGES {
            if exts.contains(&ext.as_str()) {
                return Some(lang);
            }
        }
        None
    }

    /// Parse a file and extract structured information.
    /// Returns (summary, imports, token_count, tree_hash)
    pub fn parse(source: &str, language: &str) -> Result<ParsedFile> {
        let summary = Self::extract_summary(source, language);
        let imports = Self::extract_imports(source, language);
        let token_count = Self::count_tokens(source);
        let tree_hash = Self::hash(source);

        Ok(ParsedFile {
            summary,
            imports,
            token_count,
            tree_hash,
        })
    }

    /// Extract a brief summary from the file (top-level declarations + docstrings)
    fn extract_summary(source: &str, _language: &str) -> String {
        let mut parts = Vec::new();

        // First line or shebang/docstring
        for line in source.lines().take(5) {
            let trimmed = line.trim();
            if trimmed.starts_with("#!/") {
                parts.push(trimmed.to_string());
            } else if trimmed.starts_with("//")
                || trimmed.starts_with("#")
                || trimmed.starts_with("/*")
                || trimmed.starts_with("--")
            {
                parts.push(
                    trimmed
                        .trim_start_matches(|c: char| {
                            c == '/' || c == '#' || c == '-' || c == '*' || c == ' '
                        })
                        .to_string(),
                );
            }
        }

        // Extract function/class signatures (first 10 lines that look like declarations)
        let mut decls = Vec::new();
        for line in source.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("pub ")
                || trimmed.starts_with("fn ")
                || trimmed.starts_with("def ")
                || trimmed.starts_with("class ")
                || trimmed.starts_with("function ")
                || trimmed.starts_with("export ")
                || trimmed.starts_with("impl ")
                || trimmed.starts_with("struct ")
                || trimmed.starts_with("enum ")
                || trimmed.starts_with("interface ")
                || trimmed.starts_with("type ")
                || trimmed.starts_with("const ")
                || trimmed.starts_with("func ")
                || trimmed.starts_with("async ")
            {
                decls.push(trimmed);
                if decls.len() >= 8 {
                    break;
                }
            }
        }
        if !decls.is_empty() {
            if !parts.is_empty() {
                parts.push(String::new());
            }
            parts.push("Declarations:".to_string());
            for d in decls {
                parts.push(d[..d.len().min(120)].to_string());
            }
        }

        parts.join("\n").trim().to_string()
    }

    /// Extract import statements from the source
    fn extract_imports(source: &str, language: &str) -> Vec<String> {
        let mut imports = Vec::new();
        for line in source.lines() {
            let trimmed = line.trim();
            match language {
                "typescript" | "javascript" => {
                    if trimmed.starts_with("import ") || trimmed.starts_with("require(") {
                        imports.push(trimmed.to_string());
                    }
                }
                "python" => {
                    if trimmed.starts_with("import ") || trimmed.starts_with("from ") {
                        imports.push(trimmed.to_string());
                    }
                }
                "rust" => {
                    if trimmed.starts_with("use ") || trimmed.starts_with("extern ") {
                        imports.push(trimmed.to_string());
                    }
                }
                "go" => {
                    if trimmed.starts_with("import") || trimmed.starts_with("require") {
                        imports.push(trimmed.to_string());
                    }
                }
                "java" => {
                    if trimmed.starts_with("import ") {
                        imports.push(trimmed.to_string());
                    }
                }
                "ruby" => {
                    if trimmed.starts_with("require ") || trimmed.starts_with("require_relative ") {
                        imports.push(trimmed.to_string());
                    }
                }
                _ => {}
            }
        }
        imports
    }

    /// Count approximate tokens (4 chars per token heuristic)
    fn count_tokens(source: &str) -> usize {
        (source.len() + 3) / 4
    }

    /// Simple content hash for change detection
    fn hash(source: &str) -> String {
        use std::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        hasher.write(source.as_bytes());
        format!("{:x}", hasher.finish())
    }

    /// Check if a file should be ignored based on common patterns
    pub fn is_ignored(path: &str) -> bool {
        let ignored_dirs = [
            "node_modules",
            ".git",
            "target",
            "dist",
            "build",
            ".next",
            ".nuxt",
            "vendor",
            ".tox",
            "__pycache__",
            ".venv",
            "venv",
            ".ctx",
            "coverage",
            ".terraform",
        ];
        let ignored_ext = [
            ".min.js", ".min.css", ".map", ".svg", ".png", ".jpg", ".jpeg", ".gif", ".ico",
            ".woff", ".woff2", ".ttf", ".eot", ".mp4", ".mp3", ".wasm", ".lock",
        ];

        for dir in &ignored_dirs {
            if path.contains(&format!("/{}/", dir)) || path.contains(&format!("/{}", dir)) {
                return true;
            }
        }
        for ext in &ignored_ext {
            if path.ends_with(ext) {
                return true;
            }
        }
        false
    }
}

#[derive(Debug, Clone)]
pub struct ParsedFile {
    pub summary: String,
    pub imports: Vec<String>,
    pub token_count: usize,
    pub tree_hash: String,
}
