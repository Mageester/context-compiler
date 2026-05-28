#!/bin/sh
# Context Compiler install script
# Usage: curl -fsSL https://ctx-compiler.getaxiom.ca/install.sh | sh

set -eu

REPO="Mageester/context-compiler"
ARCH=$(uname -m)
OS=$(uname -s | tr '[:upper:]' '[:lower:]')

case "$ARCH" in
  x86_64)  ARCH="x86_64" ;;
  aarch64|arm64) ARCH="aarch64" ;;
  *) echo "Unsupported arch: $ARCH"; exit 1 ;;
esac

case "$OS" in
  darwin) OS="apple-darwin" ;;
  linux)  OS="unknown-linux-gnu" ;;
  *) echo "Unsupported OS: $OS"; exit 1 ;;
esac

echo "↓ Context Compiler — installing for $OS/$ARCH"

# Get latest release
VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | cut -d'"' -f4)
FILENAME="ctx-${VERSION}-${ARCH}-${OS}.tar.gz"
DOWNLOAD_URL="https://github.com/$REPO/releases/download/$VERSION/$FILENAME"

curl -fsSL "$DOWNLOAD_URL" -o "/tmp/$FILENAME"
tar xzf "/tmp/$FILENAME" -C /tmp
install -m 755 /tmp/ctx /usr/local/bin/ctx
rm -f "/tmp/$FILENAME" /tmp/ctx

echo "✓ Installed $(ctx --version) to /usr/local/bin/ctx"
echo "  Run \`ctx 'describe your task'\` in any codebase."
