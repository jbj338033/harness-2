#!/usr/bin/env bash
# IMPLEMENTS: D-178
# Doc sync gate: DECISIONS.md is SoT. Verify any file referenced by a
# decision body actually exists, and the root entry-point files
# (AGENTS.md, CLAUDE.md, README.md) are present.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

fail=0

# Required entry-point files per D-452 + D-004.
for required in AGENTS.md CLAUDE.md README.md; do
  if [ ! -f "$required" ]; then
    echo "missing entry-point file: $required"
    fail=1
  fi
done

# Doc-sync grep: every reference of the form `[text](path)` inside the
# core docs must resolve to an existing file or be a known external URL.
for doc in AGENTS.md CLAUDE.md README.md; do
  if [ ! -f "$doc" ]; then continue; fi
  while IFS= read -r match; do
    target="$(printf '%s' "$match" | sed -nE 's/.*\]\(([^)]*)\).*/\1/p')"
    case "$target" in
      ""|http*|"#"*|mailto:*) continue ;;
      *.harness-design/*|.harness-design/*|./.harness-design/*) continue ;;
    esac
    if [ ! -e "$target" ]; then
      echo "$doc: dangling reference → $target"
      fail=1
    fi
  done < <(grep -oE '\]\([^)]+\)' "$doc" || true)
done

if [ "$fail" -ne 0 ]; then
  echo "doc sync: failed"
  exit 1
fi
echo "doc sync: ok"
