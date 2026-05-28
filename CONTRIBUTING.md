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
```

## Project Structure

```
src/
├── main.rs          # CLI entry point
├── lib.rs           # Module declarations
├── cli/mod.rs       # CLI commands + UI
├── compile/mod.rs   # Context compilation pipeline
├── embed/mod.rs     # ONNX embedding engine
├── index/mod.rs     # Index builder (file walker + parser)
├── signal/mod.rs    # Relevance engine (3 signals)
├── store/mod.rs     # SQLite index store
├── tree/mod.rs      # Tree-sitter code parser
└── trim/mod.rs      # Code trimmer
```

## Guidelines

- **One PR per feature** — keep changes focused
- **Tests** — add tests for new functionality
- **Formatting** — run `cargo fmt` before committing
- **Clippy** — address warnings, suppress with `#[allow(...)]` only with justification

## Architecture

See README.md for the full architecture diagram and explanation of the relevance engine.
