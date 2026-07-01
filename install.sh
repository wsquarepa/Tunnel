#!/usr/bin/env bash
set -euo pipefail

REPO="wsquarepa/Tunnel"
MODE=""             # user | system (from --user/--system)
DEST="${DEST:-}"    # explicit destination (DEST env or --dest)
ASSUME_YES=0

usage() {
  cat >&2 <<EOF
Install the tunnel-client binary from the latest nightly release.

Usage: install.sh [options]
  --system      install to /usr/local/bin (uses sudo if needed)
  --user        install to ~/.local/bin (no root)
  --dest DIR    install to DIR
  -y, --yes     non-interactive; never prompt
  -h, --help    show this help

Environment:
  DEST=DIR      same as --dest DIR

On a terminal with no location given, the script asks where to install.
Non-interactively it installs system-wide when it already has the rights,
otherwise to ~/.local/bin (no root). Autonomous examples:
  curl -fsSL .../install.sh | bash -s -- --user
  curl -fsSL .../install.sh | bash -s -- --system -y
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --system) MODE=system ;;
    --user) MODE=user ;;
    --dest) DEST="${2:?--dest needs a directory}"; shift ;;
    --dest=*) DEST="${1#--dest=}" ;;
    -y|--yes) ASSUME_YES=1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage; exit 1 ;;
  esac
  shift
done

OS="$(uname -s)"
ARCH="$(uname -m)"
case "$OS" in
  Linux)
    case "$ARCH" in
      x86_64|amd64) TARGET=x86_64-unknown-linux-musl ;;  # static: runs on any Linux
      aarch64|arm64) TARGET=aarch64-unknown-linux-gnu ;;
      *) echo "unsupported arch: $ARCH" >&2; exit 1 ;;
    esac ;;
  Darwin)
    case "$ARCH" in
      x86_64) TARGET=x86_64-apple-darwin ;;
      arm64|aarch64) TARGET=aarch64-apple-darwin ;;
      *) echo "unsupported arch: $ARCH" >&2; exit 1 ;;
    esac ;;
  *)
    echo "unsupported OS: $OS. Download a binary from https://github.com/$REPO/releases/tag/nightly" >&2
    exit 1 ;;
esac

# Resolve the install directory: explicit DEST wins, then --user/--system, then
# a rights-based default (prompting only on a terminal).
if [ -z "$DEST" ]; then
  case "$MODE" in
    user) DEST="$HOME/.local/bin" ;;
    system) DEST=/usr/local/bin ;;
    *)
      if [ "$(id -u)" = 0 ] || [ -w /usr/local/bin ]; then
        DEST=/usr/local/bin
      elif [ "$ASSUME_YES" = 0 ] && [ -r /dev/tty ]; then
        printf 'Install to /usr/local/bin (system-wide, needs sudo) or ~/.local/bin (just you, no root)? [S/u] ' >/dev/tty
        read -r reply </dev/tty || reply=""
        case "$reply" in u|U) DEST="$HOME/.local/bin" ;; *) DEST=/usr/local/bin ;; esac
      else
        DEST="$HOME/.local/bin"  # non-interactive and unprivileged: never sudo
      fi ;;
  esac
fi

URL="https://github.com/$REPO/releases/download/nightly/tunnel-client-$TARGET"
echo "Downloading $URL"
tmp="$(mktemp)"
curl -fsSL "$URL" -o "$tmp"
chmod +x "$tmp"

mkdir -p "$DEST" 2>/dev/null || true
if [ -w "$DEST" ] || [ "$(id -u)" = 0 ]; then
  mv "$tmp" "$DEST/tunnel-client"
else
  sudo mv "$tmp" "$DEST/tunnel-client"
fi
echo "Installed to $DEST/tunnel-client"

case ":$PATH:" in
  *":$DEST:"*) ;;
  *) echo "Note: $DEST is not in your PATH; add it to run 'tunnel-client' directly." >&2 ;;
esac
