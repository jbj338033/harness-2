#!/usr/bin/env bash
# IMPLEMENTS: D-420
# Reproducible build wrapper. Pins SOURCE_DATE_EPOCH to the latest commit
# timestamp so two builds of the same git rev produce byte-identical
# binaries — the precondition for SLSA L3 provenance.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

SOURCE_DATE_EPOCH="$(git log -1 --pretty=%ct)"
export SOURCE_DATE_EPOCH

# Strip absolute paths from debug info so /home/$USER/... doesn't end up
# baked into the binary.
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$HOME=/home/builder \
                                 --remap-path-prefix=$(pwd)=/build/harness"
# Cargo's own cache lives outside the build tree; point it at a stable
# location so locally-cached crate sources don't perturb the output.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target/reproducible}"

mkdir -p dist

cargo build --release --workspace --locked

cp target/release/harnessd dist/
cp target/release/harness  dist/

echo "reproducible build:"
echo "  source_date_epoch  $SOURCE_DATE_EPOCH"
echo "  target_dir         $CARGO_TARGET_DIR"
echo "  artifacts:"
for f in dist/harnessd dist/harness; do
  printf '  %-20s sha256=%s\n' "$f" "$(shasum -a 256 "$f" | awk '{print $1}')"
done
