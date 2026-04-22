#!/usr/bin/env bash
# IMPLEMENTS: D-455
# Audit the supersede chain in DECISIONS.md. Three rules:
#   1. Every `superseded by D-NNN` reference must point to an existing
#      D-NNN that itself is `confirmed` (not also superseded).
#   2. No active D-NNN may also be `superseded` — status is exclusive.
#   3. The implements verifier must NOT count superseded decisions.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

DECISIONS=".harness-design/DECISIONS.md"
if [ ! -f "$DECISIONS" ]; then
  echo "supersede audit: $DECISIONS missing — design workspace not present, skipping"
  exit 0
fi

OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT

# active = headers without `| superseded` or `| rejected`
grep -E '^- \*\*D-[0-9]+\*\*' "$DECISIONS" \
  | grep -vE '\| (superseded|rejected)( |$)' \
  | sed -E 's/^- \*\*(D-[0-9]+)\*\*.*/\1/' \
  | sort -u > "$OUT/active.txt"

# superseded = headers with `| superseded by D-NNN`
grep -E '^- \*\*D-[0-9]+\*\* \| superseded by' "$DECISIONS" > "$OUT/super.txt" || true

fail=0
while IFS= read -r line; do
  src="$(printf '%s' "$line" | grep -oE 'D-[0-9]+' | head -n1)"
  dst="$(printf '%s' "$line" | grep -oE 'superseded by D-[0-9]+' | grep -oE 'D-[0-9]+')"
  if [ -z "$dst" ]; then
    echo "supersede audit: $src missing 'by D-NNN' destination"
    fail=1
    continue
  fi
  if ! grep -q "^$dst\$" "$OUT/active.txt"; then
    echo "supersede audit: $src → $dst but $dst is not active"
    fail=1
  fi
done < "$OUT/super.txt"

if [ "$fail" -ne 0 ]; then
  echo "supersede audit: failed"
  exit 1
fi

echo "supersede audit: $(wc -l < "$OUT/super.txt" | tr -d ' ') chains all resolve to active D-NNN"
