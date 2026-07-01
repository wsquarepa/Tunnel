#!/usr/bin/env bash
set -euo pipefail
REPO="wsquarepa/Tunnel"
DEST="${DEST:-/usr/local/bin}"
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

URL="https://github.com/$REPO/releases/download/nightly/tunnel-client-$TARGET"
echo "Downloading $URL"
curl -fsSL "$URL" -o /tmp/tunnel-client
chmod +x /tmp/tunnel-client
sudo mv /tmp/tunnel-client "$DEST/tunnel-client"
echo "Installed to $DEST/tunnel-client"
