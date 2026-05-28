# Contributing to Context Compiler

## Development

```bash
# Clone and build
git clone https://github.com/Mageester/context-compiler
cd context-compiler
cargo build

# Run tests
cargo test

# Check formatting
cargo fmt --check

# Lint
cargo clippy

# Build release binary
cargo build --release
```

## Shell completions

If you add or remove subcommands/flags, update the completions generator in
`src/cli/mod.rs` so users can regenerate their shell completions:

```bash
ctx completions bash
ctx completions zsh
ctx completions fish
```

The completions command uses Clap's built-in `ShellCompletions` generator and
should reflect whatever the current `Args` / `Commands` structs define.

## Project Structure

```text
src/
├── main.rs          # CLI entry point (clap parser, tokio::main)
├── lib.rs           # Module declarations
├── cli/mod.rs       # CLI commands, UI helpers, completions
├── compile/mod.rs   # Context compilation pipeline (scoring + selection)
├── embed/mod.rs     # Embedding engine (hash-based lexical embeddings)
├── index/mod.rs     # Index builder (file walker + Tree-sitter parser)
├── signal/mod.rs    # Relevance engine (3 signals: semantic, dep, history)
├── store/mod.rs     # SQLite index store (rusqlite)
├── tree/mod.rs      # Tree-sitter code parser (per-language queries)
└── trim/mod.rs      # Code trimmer (preserves signatures, types, logic)
site/
├── index.html       # Cloudflare Pages landing page
└── install.sh       # Install script (attempts release, falls back to Cargo)
docs/
└── WIKI.md          # Source-controlled wiki
.github/
└── workflows/
    └── ci.yml       # CI: fmt check, clippy, build, test (Linux + macOS)
```

## Guidelines

- **One PR per feature** — keep changes focused and reviewable.
- **Tests** — add tests for new functionality. Run `cargo test` before pushing.
- **Formatting** — run `cargo fmt` before committing. CI enforces `cargo fmt --check`.
- **Clippy** — address warnings. Suppress with `#[allow(...)]` only when
  justified by a comment.
- **Commit messages** — use conventional commits format:
  `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`.
- **Shell completions** — if you change the CLI interface, regenerate
  completions or update the generator.

## Architecture

### Index pipeline

1. `index::IndexBuilder::build()` walks the directory with `ignore` (respects
   `.gitignore`), filters supported extensions, and passes each file to the
   Tree-sitter parser.
2. Each file is parsed into an AST; summaries, import statements, and function
   signatures are extracted.
3. Metadata is stored in SQLite via `store::Store`.

### Compile pipeline

1. `compile::Compiler::compile()` loads the index from SQLite.
2. `signal::` computes three relevance scores per file:
   - **Semantic**: lexical embedding similarity between the task and file content.
   - **Dependency**: graph-based relevance from import relationships.
   - **History**: boost files that were relevant in past similar tasks.
3. Files are ranked by combined score and selected up to the token budget.
4. Selected files are trimmed by `trim::` to remove noise (boilerplate,
   long comments) while preserving signatures and logic.
5. The context pack is assembled and output to clipboard, file, or stdout.

## Getting help

- Open a [GitHub Issue](https://github.com/Mageester/context-compiler/issues)
  for bugs, feature requests, or questions.
- See [`docs/WIKI.md`](docs/WIKI.md) for usage guides and troubleshooting.
- See [`CHANGELOG.md`](CHANGELOG.md) for version history.
