#!/usr/bin/env bash
set -euo pipefail

# Requires GitHub CLI auth: gh auth login
gh repo edit Mageester/context-compiler   --add-topic rust   --add-topic cli   --add-topic ai-coding   --add-topic cursor   --add-topic claude-code   --add-topic codex   --add-topic copilot   --add-topic developer-tools   --add-topic context-management   --add-topic llm-tools   --add-topic ai-agents

echo "✓ GitHub topics updated"
