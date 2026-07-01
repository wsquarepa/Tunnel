#!/usr/bin/env bash
set -euo pipefail
REPO="wsquarepa/Tunnel"
DEST="${DEST:-/usr/local/bin}"
OS=linux
ARCH="$(uname -m)"
case "$ARCH" in
  x86_64) TARGET=x86_64-unknown-linux-gnu ;;
  aarch64|arm64) TARGET=aarch64-unknown-linux-gnu ;;
  *) echo "unsupported arch: $ARCH" >&2; exit 1 ;;
esac
URL="https://github.com/$REPO/releases/latest/download/tunnel-client-$TARGET"
echo "Downloading $URL"
curl -fsSL "$URL" -o /tmp/tunnel-client
chmod +x /tmp/tunnel-client
sudo mv /tmp/tunnel-client "$DEST/tunnel-client"
echo "Installed to $DEST/tunnel-client"
