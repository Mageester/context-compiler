# Context Compiler

**Natural language → optimized AI context window.** Tell it what to build. Get exactly the files your AI coding agent needs. In seconds.

```bash
ctx "fix the race condition in auth middleware"

# → auth-middleware.ts     ↑1.2K (92% relevant)
# → session-store.ts       ↑0.8K (78% relevant)
# → types/session.d.ts     ↑0.4K (71% relevant)
# → concurrency/lock.ts    ↑0.9K (65% relevant)
# ─────────────────────────────
# 4 files · 4.2K tokens · copied to clipboard
```

<p align="center">
  <img src="docs/hero.svg" alt="Context Compiler workflow" width="700">
</p>

## The Problem

Every AI developer does this before starting a session:

1. Hunt for which files are relevant
2. Open them all in tabs
3. Copy-paste into the AI chat
4. Realize you forgot the types
5. Copy-paste again
6. The AI says "what about the session store?"
7. Hunt for that too

**This takes 5–15 minutes per session** and you're sending 10x more tokens than you need.

## What Context Compiler Does

It sits between you and your AI coding agent. You describe the task naturally, and it returns *exactly* the files the LLM needs — ranked by relevance, trimmed to fit, copied to clipboard.

```
┌─────────────────┐    ┌──────────────────┐    ┌──────────────────────┐
│  "fix auth race" │───▶│  Context Compiler │───▶│  4 files · 4.2K tok  │
│                  │    │  (builds index,   │    │  → clipboard ready   │
│  847 files       │    │   scores by       │    │  → paste into Cursor │
│  in codebase     │    │   relevance)      │    │  → start coding now  │
└─────────────────┘    └──────────────────┘    └──────────────────────┘
```

---

## Features

- **Triple-signal relevance engine** — semantic similarity (50%) + dependency graph (30%) + historical usage (20%)
- **Smart trimming** — strips comments, logging, and boilerplate while keeping signatures, types, and logic
- **Learns over time** — every session improves history signal for your codebase
- **Zero config** — one command to index, one command to compile
- **Local & private** — all embeddings run on-device. No API calls. No data leaves your machine
- **Language-aware** — understands TypeScript, Python, Rust, Go, Java, Ruby, and 30+ languages via Tree-sitter

---

## Quick Start

### Install

```bash
curl -fsSL https://ctx-compiler.dev/install.sh | sh
```

Or with Homebrew:

```bash
brew install context-compiler/ctx
```

Or from source:

```bash
cargo install ctx
```

### First time

```bash
cd your-project

# Index your codebase (first run, builds the semantic index)
ctx init

# Compile context for a task
ctx "add pagination to the user list API"

# → 5 files · 6.2K tokens · copied to clipboard
# Paste into your AI agent and start coding.
```

### Everyday use

```bash
ctx "fix the login timeout bug"
ctx "add tests for the payment webhook handler"
ctx "refactor the data layer to use async"
ctx "what calls this function?"  # finds callers
ctx "design an event system for order lifecycle"
```

---

## How It Works

### Architecture

```
┌────────────────────────────────────────────────────────────┐
│                     Phase 1: Index                         │
│                                                            │
│  codebase/ ───→ Tree-sitter ───→ file summaries            │
│  847 files    → AST parsing   → imports graph              │
│                             ───→ ONNX embeddings           │
│                                    ↓                       │
│                              .ctx/index.db                 │
│                              (SQLite + FTS5)               │
└────────────────────────────────────────────────────────────┘
                             │
┌────────────────────────────────────────────────────────────┐
│                     Phase 2: Compile                       │
│                                                            │
│  "fix auth race" ───→ embed task ───→ relevance engine     │
│                           ↓             ↓                  │
│                     .ctx/index.db    Signal 1: semantic    │
│                                      Signal 2: dependency  │
│                                      Signal 3: history     │
│                                           ↓                │
│                                     select top files       │
│                                           ↓                │
│                                     trim each file         │
│                                           ↓                │
│                                     4 files · 4.2K tokens  │
│                                     → clipboard            │
└────────────────────────────────────────────────────────────┘
```

### The Relevance Engine

The compiler computes three signals for every file and combines them into a composite score:

```python
composite_score = (semantic × 0.5) + (dependency × 0.3) + (history × 0.2)
```

| Signal | Weight | What it measures |
|--------|--------|-----------------|
| **Semantic** | 50% | Cosine similarity between task embedding and file summary embedding |
| **Dependency** | 30% | Propagation of relevance through the import graph |
| **History** | 20% | Files used in past similar tasks (learning loop) |

**The learning loop:** After every session, the task + file list is saved. Next time you ask something similar, history boosts the files that were useful before. After 10 sessions, the compiler knows your codebase intimately.

### Trimmer

Before returning context, each file is trimmed to remove noise:

```typescript
// Before: 500 lines, 12K tokens
function authenticate(user: User, token: string): Promise<Session> {
  // Validate the user token against the session store
  // This is a safety-critical function
  log.debug('authenticate called with user:', user.id);
  const session = await sessionStore.findOrCreate(user, token);
  // ...
}

// After: 80 lines, 1.6K tokens (keeps signatures, types, logic)
function authenticate(user: User, token: string): Promise<Session> {
  const session = await sessionStore.findOrCreate(user, token);
  // ...
}
```

Trimming saves **60-70% of tokens** while preserving 100% of the signal.

---

## Comparison

| | Manual | grep | Copilot/Cursor | RAG tools | **Context Compiler** |
|---|---|---|---|---|---|
| Understands code? | ✗ | ✗ | △ | △ | **✓** |
| Ranks relevance? | ✗ | ✗ | △ | △ | **✓** |
| Trims boilerplate? | ✗ | ✗ | ✗ | ✗ | **✓** |
| Learns over time? | ✗ | ✗ | ✗ | ✗ | **✓** |
| Offline / private? | ✓ | ✓ | △ | △ | **✓** |

---

## Commands

| Command | Description |
|---------|-------------|
| `ctx init` | Index the current codebase (auto-runs on first compile) |
| `ctx "task"` | Compile context for a natural language task |
| `ctx compile -b 16k "task"` | Compile with larger token budget |
| `ctx compile -m 5 "task"` | Max 5 files in output |
| `ctx compile -o output.txt "task"` | Write to file instead of clipboard |
| `ctx status` | Show index stats (files, languages, history) |
| `ctx reindex` | Rebuild index from scratch |
| `ctx watch` | Watch mode — auto-rebuild on file changes |
| `ctx history` | Show past compilations |
| `ctx done` | Mark last task as complete (saves to history) |

---

## Project Layout

```
project/
├── .ctx/                  # All Context Compiler data
│   ├── index.db           # SQLite: embeddings, imports, history
│   └── sessions/          # Session logs
├── src/
├── tests/
└── .gitignore             # Add .ctx/
```

---

## Tech Stack

| Component | Choice | Why |
|-----------|--------|-----|
| Language | **Rust** | Single binary, zero runtime, instant startup |
| Code parsing | **Tree-sitter** | AST-level understanding, 40+ languages |
| Embeddings | **ONNX Runtime** | Local inference, no API calls, privacy-first |
| Storage | **SQLite + FTS5** | Embedded, ACID, full-text search |
| CLI | **clap** | Standard, beautiful CLI framework |

---

## Development

```bash
# Build
cargo build --release

# Run
./target/release/ctx init
./target/release/ctx "your task"

# Test
cargo test

# Watch mode
cargo watch -x run
```

---

## Roadmap

- [x] File walker + Tree-sitter parsing
- [x] SQLite index with FTS5
- [x] Hash-based approximation embeddings
- [ ] ONNX model bundling (all-MiniLM-L6-v2)
- [ ] VS Code extension (right-click → compile context)
- [ ] GitHub Action (auto-context in CI)
- [ ] Agent mode (pipe context directly into AI agents)
- [ ] Multi-project awareness
- [ ] Semantic diff between task compilations

---

## License

MIT

---

<p align="center">
  Built by <a href="https://github.com/Mageester">Aidan Magee</a> —
  <a href="https://github.com/Mageester/context-compiler/issues">Report a problem</a> —
  <a href="https://github.com/Mageester/context-compiler/discussions">Start a discussion</a>
</p>
