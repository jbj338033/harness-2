#!/usr/bin/env bash
# active D-NNN list (excluding superseded · rejected) vs source `// IMPLEMENTS:` markers
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

DECISIONS=".harness-design/DECISIONS.md"
OUT_DIR="$(mktemp -d)"
trap 'rm -rf "$OUT_DIR"' EXIT

grep -E '^- \*\*D-[0-9]+\*\*' "$DECISIONS" \
  | grep -vE '\| (superseded|rejected)( |$)' \
  | sed -E 's/^- \*\*(D-[0-9]+)\*\*.*/\1/' \
  | sort -u > "$OUT_DIR/active.txt"

grep -rhoE '// IMPLEMENTS: D-[0-9]+(, D-[0-9]+)*' crates/ 2>/dev/null \
  | grep -oE 'D-[0-9]+' \
  | sort -u > "$OUT_DIR/implemented.txt"

missing="$(comm -23 "$OUT_DIR/active.txt" "$OUT_DIR/implemented.txt")"
total_active="$(wc -l < "$OUT_DIR/active.txt" | tr -d ' ')"
total_done="$(wc -l < "$OUT_DIR/implemented.txt" | tr -d ' ')"

if [ -n "$missing" ]; then
  missing_count="$(printf '%s\n' "$missing" | wc -l | tr -d ' ')"
  echo "missing $missing_count / $total_active active decisions ($total_done implemented):"
  printf '%s\n' "$missing"
  exit 1
fi

echo "all $total_active active D-NNN covered ($total_done implemented markers)"
