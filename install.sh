#!/usr/bin/env bash
# harness installer.
#
# Usage:
#   curl -sSL https://raw.githubusercontent.com/jbj338033/harness/main/install.sh | sh
#
# Modes:
#   (default)                 install harnessd + harness CLI + register service (host machine)
#   --client-only             install harness CLI only, no daemon, no service (pair to remote host)
#
# Env vars / flags:
#   HARNESS_VERSION=vX.Y.Z    pin to a specific release (default: latest)
#   HARNESS_INSTALL_DIR=...   binary install dir (default: /usr/local/bin)
#   --from-source             build from source via codeload tarball
#   --version vX.Y.Z          same as HARNESS_VERSION
#   --install-dir <path>      same as HARNESS_INSTALL_DIR

set -euo pipefail

REPO="jbj338033/harness"
INSTALL_DIR="${HARNESS_INSTALL_DIR:-/usr/local/bin}"
VERSION="${HARNESS_VERSION:-}"
FROM_SOURCE=0
CLIENT_ONLY=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --from-source) FROM_SOURCE=1; shift ;;
    --client-only) CLIENT_ONLY=1; shift ;;
    --version) VERSION="$2"; shift 2 ;;
    --install-dir) INSTALL_DIR="$2"; shift 2 ;;
    -h|--help)
      grep '^#' "$0" | sed -e 's/^# \{0,1\}//'
      exit 0
      ;;
    *) echo "unknown flag: $1" >&2; exit 2 ;;
  esac
done

OS="$(uname -s)"
ARCH="$(uname -m)"

detect_linux_libc() {
  if ldd --version 2>&1 | grep -qi musl; then echo "musl"; else echo "gnu"; fi
}

VARIANT=""
case "$OS" in
  Darwin)
    PLATFORM="macos"
    case "$ARCH" in
      arm64)  TARGET="aarch64-apple-darwin" ;;
      x86_64) TARGET="x86_64-apple-darwin" ;;
      *) echo "unsupported arch on macOS: $ARCH" >&2; exit 1 ;;
    esac
    ;;
  Linux)
    PLATFORM="linux"
    LIBC="$(detect_linux_libc)"
    case "$ARCH" in
      x86_64)          TARGET="x86_64-unknown-linux-${LIBC}" ;;
      aarch64|arm64)   TARGET="aarch64-unknown-linux-${LIBC}" ;;
      *) echo "unsupported arch on Linux: $ARCH" >&2; exit 1 ;;
    esac
    if [[ "$TARGET" == "x86_64-unknown-linux-gnu" ]] \
       && { [[ -n "${WAYLAND_DISPLAY:-}" ]] || [[ -n "${DISPLAY:-}" ]]; }; then
      VARIANT="-desktop"
    fi
    ;;
  *) echo "unsupported OS: $OS" >&2; exit 1 ;;
esac

need() { command -v "$1" >/dev/null 2>&1 || { echo "missing: $1" >&2; exit 1; }; }
need curl
need tar

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{print $1}';
  elif command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | awk '{print $1}';
  else echo "missing: sha256sum or shasum" >&2; exit 1;
  fi
}

writable_dir() {
  [[ -w "$1" ]] || { [[ ! -e "$1" ]] && [[ -w "$(dirname "$1")" ]]; }
}
if writable_dir "$INSTALL_DIR"; then SUDO=""; else SUDO="sudo"; fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

echo "harness installer — $PLATFORM/$TARGET"

if [[ "$FROM_SOURCE" -eq 1 ]]; then
  need cargo
  REF="${VERSION:-main}"
  echo "==> fetching source ($REPO@$REF)"
  curl -fsSL "https://codeload.github.com/$REPO/tar.gz/$REF" | tar -xz -C "$WORK"
  SRC="$(find "$WORK" -mindepth 1 -maxdepth 1 -type d | head -n 1)"
  echo "==> building release binaries"
  if [[ "$CLIENT_ONLY" -eq 1 ]]; then
    (cd "$SRC" && cargo build --release -p harness-tui)
  else
    FEATURE_FLAG=""
    if [[ "$PLATFORM" == "macos" ]] || [[ -n "$VARIANT" ]]; then
      FEATURE_FLAG="--features screen-capture"
    fi
    (cd "$SRC" && cargo build --release -p harnessd -p harness-tui $FEATURE_FLAG)
  fi
  BIN_DIR="$SRC/target/release"
  SERVICE_DIR="$SRC/dist"
else
  if [[ -z "$VERSION" ]]; then
    echo "==> resolving latest release"
    VERSION="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
      | awk -F'"' '/"tag_name":/ {print $4; exit}')"
    [[ -n "$VERSION" ]] || { echo "failed to resolve latest release" >&2; exit 1; }
  fi

  ASSET="harness-${VERSION}-${TARGET}${VARIANT}.tar.gz"
  URL="https://github.com/$REPO/releases/download/$VERSION/$ASSET"
  SUMS_URL="https://github.com/$REPO/releases/download/$VERSION/SHA256SUMS"

  echo "==> downloading $ASSET"
  curl -fsSL "$URL" -o "$WORK/$ASSET"
  curl -fsSL "$SUMS_URL" -o "$WORK/SHA256SUMS"

  echo "==> verifying checksum"
  EXPECTED="$(awk -v f="$ASSET" '$2 == f || $2 == "./"f {print $1; exit}' "$WORK/SHA256SUMS")"
  [[ -n "$EXPECTED" ]] || { echo "no checksum for $ASSET in SHA256SUMS" >&2; exit 1; }
  ACTUAL="$(sha256 "$WORK/$ASSET")"
  if [[ "$EXPECTED" != "$ACTUAL" ]]; then
    echo "checksum mismatch for $ASSET" >&2
    echo "  expected: $EXPECTED" >&2
    echo "  actual:   $ACTUAL"   >&2
    exit 1
  fi

  tar -xzf "$WORK/$ASSET" -C "$WORK"
  SRC="$WORK/harness-${VERSION}-${TARGET}${VARIANT}"
  BIN_DIR="$SRC"
  SERVICE_DIR="$SRC/service"
fi

$SUDO mkdir -p "$INSTALL_DIR"

if [[ "$CLIENT_ONLY" -eq 1 ]]; then
  echo "==> installing client to $INSTALL_DIR"
  $SUDO install -m 0755 "$BIN_DIR/harness" "$INSTALL_DIR/harness"
  echo ""
  echo "==> done (client-only). next steps:"
  echo "    harness connect wss://<host>:8384 <code> <name>"
  exit 0
fi

echo "==> installing binaries to $INSTALL_DIR"
$SUDO install -m 0755 "$BIN_DIR/harnessd" "$INSTALL_DIR/harnessd"
$SUDO install -m 0755 "$BIN_DIR/harness"  "$INSTALL_DIR/harness"

echo "==> registering service"
if [[ "$PLATFORM" == "macos" ]]; then
  PLIST_DIR="$HOME/Library/LaunchAgents"
  PLIST="$PLIST_DIR/com.harness.plist"
  mkdir -p "$PLIST_DIR"
  sed "s|__BIN__|$INSTALL_DIR/harnessd|g" "$SERVICE_DIR/launchd/com.harness.plist" > "$PLIST"
  UID_NUM="$(id -u)"
  launchctl bootout "gui/$UID_NUM/com.harness" 2>/dev/null || true
  launchctl bootstrap "gui/$UID_NUM" "$PLIST"
else
  UNIT_DIR="$HOME/.config/systemd/user"
  UNIT="$UNIT_DIR/harness.service"
  mkdir -p "$UNIT_DIR"
  sed "s|__BIN__|$INSTALL_DIR/harnessd|g" "$SERVICE_DIR/systemd/harness.service" > "$UNIT"
  systemctl --user daemon-reload
  systemctl --user enable --now harness.service
fi

echo ""
echo "==> done. next steps:"
echo "    harness auth login      # sign in to a provider"
echo "    harness                 # open the TUI"
echo "    harness doctor          # verify everything is wired up"
