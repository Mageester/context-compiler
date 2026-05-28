# Changelog

All notable changes to Context Compiler are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] — 2025-05-28

### Added

- **`ctx init [path]`** — Index a codebase. Walks the directory tree, parses
  supported code files with Tree-sitter, and stores metadata (summaries,
  imports, token counts) in `.ctx/index.db`.
  - `--force` flag to rebuild an existing index.
- **`ctx compile <task>`** — Compile context for a natural-language task.
  Selects and ranks files from the index by relevance.
  - `-b, --budget` flag for custom token budget (default: 8192).
  - `-m, --max-files` flag for maximum file count (default: auto).
  - `-o, --output` flag to write context to a file.
  - `--no-clipboard` flag to print context to stdout.
- **`ctx "task"` (shorthand)** — Positional task text triggers compile with
  default settings.
- **`ctx status`** — Show index stats: file count, token total, import edges,
  language breakdown, and recent history.
- **`ctx reindex [path]`** — Force rebuild the index from scratch.
- **`ctx watch [path]`** — Watch mode: polls every 30 seconds and auto-
  rebuilds the index on file changes.
- **`ctx history -l <N>`** — Show the last N compilation sessions with task
  descriptions and selected file paths.
- **`ctx done`** — Mark the latest task as complete for history learning.
- **Shell completions** — `ctx completions bash|zsh|fish`.
- **Relevance engine** — Three-signal scoring: semantic similarity (lexical
  embeddings), dependency graph analysis, and history-based boosting.
- **Code trimmer** — Removes obvious noise from selected files while preserving
  signatures, types, and logic.
- **Clipboard integration** — Copies context to clipboard via `arboard` when
  available. Falls back to stdout on headless systems.
- **Tree-sitter parsing** — Language-aware code analysis for Rust,
  TypeScript, JavaScript, Python, Go, Java, and Ruby.
- **Install script** — `site/install.sh` with automatic fallback from release
  binary to source build.
- **CI pipeline** — GitHub Actions workflow for Linux and macOS with
  formatting checks, Clippy linting, build, and test.
- **ONNX embedding support** — Unused in MVP (hash-based embedder used
  instead), but optional `ort` dependency declared for future use.

### Changed

- N/A (initial release).

### Fixed

- N/A (initial release).

### Security

- N/A (initial release).

[0.1.1]: https://github.com/Mageester/context-compiler/releases/tag/v0.1.1
