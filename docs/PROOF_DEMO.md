# Context Compiler Proof Demo

This is a concrete launch demo captured on a real local repo, not a toy example.

## Repo

- Repo: `axiom-workspace`
- Git-tracked files: 132
- Indexed source/text files: 50
- Text files counted for baseline: 76
- Approx full textual repo context: ~140,376 tokens

## Task

```text
debug why Start Session modal does not warn about overlapping claimed files
```

## Command

```bash
ctx compile "debug why Start Session modal does not warn about overlapping claimed files" \
  --budget 8000 \
  --max-files 4 \
  --output /tmp/context-compiler-proof-demo.txt \
  --no-clipboard
```

## Result

Context Compiler selected 4 files, ~6,296 estimated tokens:

1. `src/components/StartSessionModal.tsx` — UI path where the overlap warning is rendered and confirmed
2. `src/lib/sessions.test.ts` — tests for overlap behavior
3. `src/pages/ProjectsPage.tsx` — caller/context around project/session flow
4. `src/lib/sessions.ts` — core `detectSessionOverlap` implementation

## Compression

- Naive full textual repo: ~140,376 tokens
- Generated context pack: ~6,255 tokens
- Reduction: ~22.4x

## Raw CLI output

```text
Context Compiler — Compile
────────────────────────────
  Task:    debug why Start Session modal does not warn about overlapping claimed files
  Budget:  8000 tokens

 ✓ Selected 4 files · ~6296 tokens (from 50 indexed files)

  1. src/components/StartSessionModal.tsx (2117)  ██░░░░░░░░ 22%
  2. src/lib/sessions.test.ts (492)  █░░░░░░░░░ 18%
  3. src/pages/ProjectsPage.tsx (1294)  █░░░░░░░░░ 17%
  4. src/lib/sessions.ts (2244)  █░░░░░░░░░ 11%

 ✓ Context written to /tmp/context-compiler-proof-demo.txt
```

## Honest notes

This demo is good because it found the UI entry point, the overlap tests, and the core overlap function. It also pulled in `ProjectsPage.tsx`, which is related workflow context but probably less essential than the other three.

The tool is useful as a local context preprocessor, especially when you want a paste-ready packet for ChatGPT, Claude, Codex, Cursor, or another AI coding tool. It is not magic semantic code understanding yet; relevance scoring still needs improvement for deeper/ambiguous tasks.

## Launch post based on this proof

```text
I tested Context Compiler on a real repo instead of a toy app.

Task:
"debug why Start Session modal does not warn about overlapping claimed files"

The repo had ~140k tokens of text source.

Context Compiler turned that into a ~6.3k-token context pack and picked:

- StartSessionModal.tsx — the UI path
- sessions.ts — the core overlap logic
- sessions.test.ts — tests for the behavior
- ProjectsPage.tsx — related workflow context

That is the actual reason I built it: not to replace Cursor/Claude/Codex, but to give them cleaner starting context.

One command:
ctx "debug why Start Session modal does not warn about overlapping claimed files"

Local only. No API key. No code leaves your machine.

https://github.com/Mageester/context-compiler
```
