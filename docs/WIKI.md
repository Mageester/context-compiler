# Context Compiler Wiki

This wiki is the source-controlled companion to the hosted documentation at
`https://ctx-compiler.getaxiom.ca/#wiki`.

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
4. The context is copied to the clipboard when clipboard access is available,
   or printed to stdout when it is not.

## Commands

### `ctx init [path]`

Index a codebase. Walks the directory tree, parses supported code files with
Tree-sitter, and stores metadata (summaries, imports, token counts) in
`.ctx/index.db`.

| Flag | Description |
|---|---|
| `--force` / `-f` | Rebuild the index from scratch even if one exists |

### `ctx compile <task>`

Compile context for a natural-language task. Selects and ranks files from the
index that are most relevant to the task description.

| Flag | Description | Default |
|---|---|---|
| `-b, --budget <TOKENS>` | Token budget for the context window | 8192 |
| `-m, --max-files <N>` | Maximum number of files to include | 0 (auto) |
| `-o, --output <FILE>` | Write output to a file instead of clipboard | — |
| `--no-clipboard` | Print context to stdout | false |

### `ctx "task"` (shorthand)

Shorthand for `ctx compile` with the default 8,192-token budget. Any
positional arguments that are not a recognized subcommand are treated as the
task text.

### `ctx status`

Show index statistics: total files, token count, import edges, recent sessions,
and a breakdown by language.

### `ctx reindex [path]`

Force rebuild the index from scratch. Equivalent to `ctx init --force`.

### `ctx watch [path]`

Watch mode. Polls the codebase every 30 seconds and automatically rebuilds the
index when file changes are detected. Press Ctrl+C to stop.

### `ctx history -l <N>`

Show the last `N` compilation sessions. Each entry lists the task description,
how many files were selected, and the top file paths.

| Flag | Description | Default |
|---|---|---|
| `-l, --limit <N>` | Number of past entries to show | 10 |

### `ctx done`

Mark the most recent compilation task as complete. Saves it to history for
improving future relevance scoring.

### Shell completions

```bash
ctx completions bash   # source this in ~/.bashrc
ctx completions zsh    # save to $fpath
ctx completions fish   # save to ~/.config/fish/completions/
```

## Recommended workflow

1. Add `.ctx/` to `.gitignore`.
2. Run `ctx init` once per project.
3. Before a coding-agent session, run a precise task prompt:
   - Good: `ctx "fix duplicate Stripe webhook processing"`
   - Better: `ctx "fix duplicate Stripe webhook processing in the billing API and include tests"`
4. Paste the result into Cursor, Claude Code, Codex, or any other coding agent.
5. Run `ctx done` when the result was useful so history can improve future
   selections.

## Troubleshooting

### Installer says Cargo is missing

No release binaries exist yet, so the installer falls back to building from
source. Install Rust first:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
curl -fsSL https://ctx-compiler.getaxiom.ca/install.sh | sh
```

If curl fails, clone and build manually:

```bash
git clone https://github.com/Mageester/context-compiler.git
cd context-compiler
cargo install --path .
```

### Clipboard does not work

Headless Linux/SSH sessions often block clipboard access. Use file output
instead:

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

### Shell completions not loading

```bash
# bash
ctx completions bash > ~/.bash_completion.d/ctx
echo "source ~/.bash_completion.d/ctx" >> ~/.bashrc

# zsh
ctx completions zsh > /usr/local/share/zsh/site-functions/_ctx

# fish
ctx completions fish > ~/.config/fish/completions/ctx.fish
```

### No index found

If `ctx compile` or `ctx status` says no index exists, run `ctx init` first.

## Implementation notes

- Embeddings are local lightweight lexical embeddings in the MVP.
- Index storage is SQLite under `.ctx/`.
- Tree-sitter is used for language-aware parsing where supported.
- Supported languages: Rust, TypeScript, JavaScript, Python, Go, Java, Ruby.
- No source code is sent to a hosted API by default.
- Relevance combines three signals: semantic similarity, dependency graph
  analysis, and history-based boosting.
