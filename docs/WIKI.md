# Context Compiler Wiki

This wiki is the source-controlled companion to the hosted documentation at `https://ctx-compiler.getaxiom.ca/#wiki`.

## Quickstart

```bash
curl -fsSL https://ctx-compiler.getaxiom.ca/install.sh | sh
cd your-project
ctx init
ctx "fix the auth race condition"
```

What happens:

1. `ctx init` builds a local `.ctx/` SQLite index.
2. `ctx "task"` scores indexed files against the natural-language task.
3. The selected files are trimmed and formatted as a context pack.
4. The context is copied to the clipboard when clipboard access is available.

## Commands

- `ctx init [path]` — index a codebase.
- `ctx init --force` — rebuild an existing index.
- `ctx "task"` — shorthand compile with the default 8,192-token budget.
- `ctx compile "task"` — explicit compile command.
- `ctx compile -b 16000 -m 8 "task"` — custom token budget and max file count.
- `ctx compile -o context.md "task"` — write context to a file.
- `ctx compile --no-clipboard "task"` — print context to stdout.
- `ctx status` — show indexed files, languages, imports, and recent history.
- `ctx reindex [path]` — force rebuild.
- `ctx watch [path]` — periodically rebuild while working.
- `ctx history -l 20` — show previous compile tasks.
- `ctx done` — mark the most recent task as complete.

## Recommended workflow

1. Add `.ctx/` to `.gitignore`.
2. Run `ctx init` once per project.
3. Before a coding-agent session, run a precise task prompt:
   - Good: `ctx "fix duplicate Stripe webhook processing"`
   - Better: `ctx "fix duplicate Stripe webhook processing in the billing API and include tests"`
4. Paste the result into Cursor, Claude Code, Codex, or any other coding agent.
5. Run `ctx done` when the result was useful so history can improve future selections.

## Troubleshooting

### Installer says Cargo is missing

No release binaries exist yet, so the installer falls back to building from source. Install Rust first:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
curl -fsSL https://ctx-compiler.getaxiom.ca/install.sh | sh
```

### Clipboard does not work

Headless Linux/SSH sessions often block clipboard access. Use file output instead:

```bash
ctx compile -o context.md "your task"
```

### Results seem stale

Rebuild the index:

```bash
ctx reindex
ctx status
```

### It returns too few files

Increase the budget or set max files:

```bash
ctx compile -b 16000 -m 10 "your task"
```

## Current implementation notes

- Embeddings are local lightweight lexical embeddings in the MVP.
- Index storage is SQLite under `.ctx/`.
- Tree-sitter is used for language-aware parsing where supported.
- No source code is sent to a hosted API by default.
