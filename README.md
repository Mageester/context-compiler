# Context Compiler

**Natural language → optimized AI context window.** Describe a coding task and get a compact, ranked set of files for your AI coding agent.

```bash
ctx "fix the race condition in auth middleware"

# → selected files ranked by relevance
# → context trimmed to fit the token budget
# → copied to clipboard when available
```

Context Compiler is a local Rust CLI for preparing high-signal context packs for tools like Cursor, Claude Code, Codex, and other AI coding agents.

## Install

```bash
curl -fsSL https://ctx-compiler.getaxiom.ca/install.sh | sh
```

The installer tries the latest GitHub release binary first. If no release binary exists yet, it falls back to building from source with Cargo.

Source install manually:

```bash
git clone https://github.com/Mageester/context-compiler.git
cd context-compiler
cargo install --path .
```

## Quickstart

```bash
cd your-project
ctx init
ctx "add tests for the payment webhook handler"
```

`ctx init` creates a local `.ctx/` index. Add `.ctx/` to `.gitignore`.

## Commands

- `ctx init [path]` — index a codebase.
- `ctx init --force` — rebuild an existing index.
- `ctx "task"` — shorthand compile with the default budget.
- `ctx compile "task"` — explicit compile command.
- `ctx compile -b 16000 -m 8 "task"` — custom token budget and max file count.
- `ctx compile -o context.md "task"` — write context to a file.
- `ctx compile --no-clipboard "task"` — print context to stdout.
- `ctx status` — show index stats.
- `ctx reindex [path]` — force rebuild.
- `ctx watch [path]` — periodically rebuild while working.
- `ctx history -l 20` — show previous compile tasks.
- `ctx done` — mark the latest task complete for history learning.

## How it works

1. **Index:** walks the repo, parses supported code files, extracts summaries/imports, and stores metadata in `.ctx/index.db`.
2. **Score:** embeds the task and each file summary/path with a local lexical embedding, then combines semantic, dependency, and history signals.
3. **Select:** chooses top files within the token budget.
4. **Trim:** removes obvious noise while preserving signatures, types, and logic.
5. **Output:** copies the formatted context to clipboard or writes it to a file/stdout.

## Features

- Local/private by default — no hosted API required for the current MVP.
- Rust single-binary CLI.
- SQLite-backed project index.
- Tree-sitter language detection/parsing for common languages.
- Shorthand task UX: `ctx "task"`.
- Clipboard, file, and stdout output modes.

## Website and wiki

- Website: `https://ctx-compiler.getaxiom.ca`
- Wiki: [`docs/WIKI.md`](docs/WIKI.md)
- Install script: [`site/install.sh`](site/install.sh)

## Development

```bash
cargo fmt
cargo build
cargo test
cargo build --release
```

Local website preview:

```bash
cd site
python3 -m http.server 8080
```

## Repository layout

```text
src/                 Rust CLI source
site/                Cloudflare Pages static site and install script
docs/                Source-controlled wiki/docs
.github/workflows/   CI
```

## Status

MVP. The core CLI builds and runs locally. Public release binaries are not required for installation because the installer falls back to source builds when needed.

## License

MIT
