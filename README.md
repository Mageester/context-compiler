# Context Compiler

[![CI](https://github.com/Mageester/context-compiler/actions/workflows/ci.yml/badge.svg)](https://github.com/Mageester/context-compiler/actions/workflows/ci.yml)
[![MIT License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2021-edition?logo=rust&label=Rust)](https://www.rust-lang.org)

**Natural language → optimized AI context window.** Describe a coding task and get a compact, ranked set of files for your AI coding agent.

```
$ ctx "fix the race condition in auth middleware"

Context Compiler — Compile
──────────────────────────
  Task:    fix the race condition in auth middleware
  Budget:  8192 tokens

 ✓ Selected 4 files · ~3,200 tokens (from 147 indexed files)

  1. src/auth/middleware.rs (892)   ████████░░  80%
  2. src/auth/session.rs   (1,204) ███████░░░  72%
  3. src/auth/mod.rs       (456)   █████░░░░░  51%
  4. src/db/users.rs       (648)   ████░░░░░░  42%

 ✓ Context copied to clipboard! Paste into any AI coding agent.
```

Context Compiler is a local Rust CLI for preparing high-signal context packs for tools like Cursor, Claude Code, Codex, and other AI coding agents.

## Install

```bash
curl -fsSL https://context-compiler.pages.dev/install.sh | sh
```

The installer tries the latest GitHub release binary first. If no release binary
exists yet, it falls back to building from source with Cargo.

### Fallback: manual source install

If the installer fails (missing Rust, network issues, etc.), install manually:

```bash
# Requires Rust (install from https://rustup.rs)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Build from source
git clone https://github.com/Mageester/context-compiler.git
cd context-compiler
cargo install --path .

# Verify
ctx --version
```

### Requirements

| Requirement | Version | Notes |
|---|---|---|
| Rust toolchain | stable (edition 2021) | Only needed for source builds |
| OS | Linux, macOS | Windows via WSL |
| Clipboard | Wayland / X11 / macOS | Falls back to stdout on headless |

## Quickstart

```bash
cd your-project
ctx init
ctx "add tests for the payment webhook handler"
```

`ctx init` creates a local `.ctx/` SQLite index. Add `.ctx/` to `.gitignore`.

## Commands

| Command | Description |
|---|---|
| `ctx init [path]` | Index a codebase |
| `ctx init --force` | Rebuild an existing index |
| `ctx "task"` | Shorthand compile with the default 8,192-token budget |
| `ctx compile "task"` | Explicit compile command |
| `ctx compile -b 16000 -m 8 "task"` | Custom token budget and max file count |
| `ctx compile -o context.md "task"` | Write context to a file |
| `ctx compile --no-clipboard "task"` | Print context to stdout |
| `ctx status` | Show index stats |
| `ctx reindex [path]` | Force rebuild |
| `ctx watch [path]` | Periodically rebuild while working |
| `ctx history -l 20` | Show previous compile tasks |
| `ctx done` | Mark the latest task complete for history learning |

### Shell completions

```bash
ctx completions bash   # source this in ~/.bashrc
ctx completions zsh    # save to $fpath
ctx completions fish   # save to ~/.config/fish/completions/
```

## Examples

### Index a project and compile context

```bash
$ ctx init --force

Context Compiler — Init
───────────────────────
  Codebase: /home/user/project

 → Rebuilding index from scratch...

 ✓ Indexed 147 files
```

### Compile with a custom budget

```bash
$ ctx compile -b 16000 -m 10 "implement Stripe webhook signature verification"

Context Compiler — Compile
──────────────────────────
  Task:    implement Stripe webhook signature verification
  Budget:  16000 tokens

 ✓ Selected 6 files · ~8,400 tokens (from 147 indexed files)

  1. src/billing/webhook.rs (2,104)   █████████░  91%
  2. src/billing/signature.rs (1,856) ████████░░  83%
  3. src/billing/mod.rs (892)         ███████░░░  67%
  4. src/config/secrets.rs (512)      █████░░░░░  54%
  5. src/api/routes.rs (1,448)        ████░░░░░░  43%
  6. tests/billing_test.rs (2,560)    ███░░░░░░░  32%

 ✓ Context copied to clipboard! Paste into any AI coding agent.
```

### Write to a file instead of clipboard

```bash
ctx compile -o context.md "refactor the user authentication flow"

 # → writes context to context.md
 # → useful for headless/SSH environments
```

### Check index status

```bash
$ ctx status

Context Compiler — Status
─────────────────────────
  Codebase:  /home/user/project
  Index:     147 files
  Size:      ~58,200 tokens
  Imports:   431 edges
  History:   3 past sessions

 → Languages:
    Rust: 67 files
    TypeScript: 41 files
    Python: 22 files
    Go: 17 files

 → Recent tasks:
    fix the race condition in auth middleware — 4 files
    implement Stripe webhook signature verification — 6 files
```

### View compile history

```bash
$ ctx history -l 5

Context Compiler — History
──────────────────────────
  1. fix the race condition in auth middleware — 4 files
     src/auth/middleware.rs
     src/auth/session.rs
     src/auth/mod.rs
     ... and 1 more

  2. implement Stripe webhook signature verification — 6 files
     src/billing/webhook.rs
     src/billing/signature.rs
     src/billing/mod.rs
```

### Watch mode

```bash
$ ctx watch

Context Compiler — Watch
────────────────────────
  Watching: /home/user/project for changes...
  Press Ctrl+C to stop.
```

Rebuilds the index every 30 seconds when file changes are detected.

## How it works

1. **Index:** walks the repo, parses supported code files, extracts summaries
   and imports, and stores metadata in `.ctx/index.db`.
2. **Score:** embeds the task and each file summary/path with a local lexical
   embedding, then combines semantic, dependency, and history signals.
3. **Select:** chooses top files within the token budget.
4. **Trim:** removes obvious noise while preserving signatures, types, and logic.
5. **Output:** copies the formatted context to clipboard, writes it to a file,
   or prints to stdout.

## Features

- **Local/private by default** — no hosted API required for the current MVP.
- **Rust single-binary CLI** — fast, portable, statically linked.
- **SQLite-backed project index** — persistent, queryable.
- **Tree-sitter parsing** — language-aware code analysis for Rust, TypeScript,
  JavaScript, Python, Go, Java, Ruby.
- **Shorthand task UX** — `ctx "task"` compiles in one command.
- **Clipboard, file, and stdout output modes.**
- **Relevance scoring** — combines semantic similarity, dependency graph
  analysis, and history-based boosting.
- **Watch mode** — auto-rebuilds the index as files change.

## Website and wiki

- Website: `https://context-compiler.pages.dev`
- Wiki: [`docs/WIKI.md`](docs/WIKI.md)
- Install script: [`site/install.sh`](site/install.sh)

## Repository layout

```text
src/                 Rust CLI source
  ├── main.rs        CLI entry point
  ├── lib.rs         Module declarations
  ├── cli/           CLI commands + UI helpers
  ├── compile/       Context compilation pipeline
  ├── embed/         Embedding engine
  ├── index/         Index builder (file walker + parser)
  ├── signal/        Relevance engine (3 signals)
  ├── store/         SQLite index store
  ├── tree/          Tree-sitter code parser
  └── trim/          Code trimmer
site/                Cloudflare Pages static site and install script
docs/                Source-controlled wiki/docs
.github/workflows/   CI (Linux + macOS)
```

## Development

```bash
cargo fmt
cargo build
cargo test
cargo clippy
cargo build --release
```

Local website preview:

```bash
cd site
python3 -m http.server 8080
```

## Status

MVP. The core CLI builds and runs locally. Public release binaries are not
required for installation because the installer falls back to source builds
when needed.

## License

MIT — see [LICENSE](LICENSE).
