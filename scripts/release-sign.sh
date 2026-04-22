#!/usr/bin/env bash
# IMPLEMENTS: D-214, D-420
# 2-of-2 detached signing per D-420. Signs every artifact under
# `dist/` with two independent keys; release passes only when both
# signatures verify.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

DIST="${HARNESS_DIST:-dist}"
PRIMARY_KEY="${HARNESS_PRIMARY_KEY:-}"
SECONDARY_KEY="${HARNESS_SECONDARY_KEY:-}"

if [ ! -d "$DIST" ]; then
  echo "release-sign: $DIST not found — build artifacts first" >&2
  exit 1
fi
if [ -z "$PRIMARY_KEY" ] || [ -z "$SECONDARY_KEY" ]; then
  echo "release-sign: set HARNESS_PRIMARY_KEY and HARNESS_SECONDARY_KEY (gpg key ids)" >&2
  exit 1
fi

shopt -s nullglob
artifacts=("$DIST"/harness-*.tar.gz "$DIST"/harness-*.zip "$DIST"/harnessd "$DIST"/harness)

if [ "${#artifacts[@]}" -eq 0 ]; then
  echo "release-sign: no artifacts under $DIST" >&2
  exit 1
fi

for art in "${artifacts[@]}"; do
  echo "signing $art"
  gpg --batch --yes --local-user "$PRIMARY_KEY" \
      --output "$art.primary.sig" --detach-sign "$art"
  gpg --batch --yes --local-user "$SECONDARY_KEY" \
      --output "$art.secondary.sig" --detach-sign "$art"
  gpg --verify "$art.primary.sig" "$art" >/dev/null
  gpg --verify "$art.secondary.sig" "$art" >/dev/null
  echo "  ok — both signatures verify"
done

echo "release-sign: 2-of-2 signed ${#artifacts[@]} artifact(s)"
