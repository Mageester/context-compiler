#!/bin/sh
# Context Compiler install script
# Usage: curl -fsSL https://context-compiler.pages.dev/install.sh | sh

set -eu

REPO="Mageester/context-compiler"
BIN="ctx"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
GITHUB_API="https://api.github.com/repos/$REPO/releases/latest"

say() { printf '%s\n' "$*"; }
fail() { say "✗ $*"; exit 1; }

ARCH=$(uname -m)
OS=$(uname -s | tr '[:upper:]' '[:lower:]')

case "$ARCH" in
  x86_64|amd64) ARCH="x86_64" ;;
  aarch64|arm64) ARCH="aarch64" ;;
  *) fail "Unsupported architecture: $ARCH (expected x86_64 or arm64)" ;;
esac

case "$OS" in
  darwin) OS="apple-darwin" ;;
  linux) OS="unknown-linux-gnu" ;;
  *) fail "Unsupported OS: $OS (expected macOS or Linux)" ;;
esac

say "↓ Context Compiler — installing for $ARCH-$OS"

install_binary() {
  VERSION=$(curl -fsSL "$GITHUB_API" 2>/dev/null | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1 || true)
  [ -n "$VERSION" ] || return 1

  FILENAME="ctx-${VERSION}-${ARCH}-${OS}.tar.gz"
  URL="https://github.com/$REPO/releases/download/$VERSION/$FILENAME"
  TMP="/tmp/$FILENAME"

  say "  → Found release $VERSION"
  say "  → Downloading $FILENAME"
  curl -fsSL "$URL" -o "$TMP" || return 1
  [ -s "$TMP" ] || return 1
  tar xzf "$TMP" -C /tmp || return 1
  [ -f "/tmp/$BIN" ] || return 1

  say "  → Installing to $INSTALL_DIR/$BIN"
  if install -m 755 "/tmp/$BIN" "$INSTALL_DIR/$BIN" 2>/dev/null; then
    rm -f "$TMP" "/tmp/$BIN"
    return 0
  fi

  say "  → Permission denied for $INSTALL_DIR; retrying with sudo"
  if command -v sudo >/dev/null 2>&1; then
    sudo install -m 755 "/tmp/$BIN" "$INSTALL_DIR/$BIN"
    rm -f "$TMP" "/tmp/$BIN"
    return 0
  fi

  return 1
}

install_from_source() {
  say "  → No release binary available yet; falling back to source install"
  command -v cargo >/dev/null 2>&1 || {
    say ""
    say "Rust/Cargo is required for source install. Install Rust, then rerun:"
    say "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    say "  curl -fsSL https://context-compiler.pages.dev/install.sh | sh"
    return 1
  }

  TMPDIR=$(mktemp -d)
  say "  → Cloning https://github.com/$REPO"
  git clone --depth 1 "https://github.com/$REPO.git" "$TMPDIR" >/dev/null 2>&1 || return 1
  say "  → Building optimized binary"
  (cd "$TMPDIR" && cargo build --release) || return 1

  say "  → Installing to $INSTALL_DIR/$BIN"
  if install -m 755 "$TMPDIR/target/release/$BIN" "$INSTALL_DIR/$BIN" 2>/dev/null; then
    rm -rf "$TMPDIR"
    return 0
  fi

  say "  → Permission denied for $INSTALL_DIR; retrying with sudo"
  if command -v sudo >/dev/null 2>&1; then
    sudo install -m 755 "$TMPDIR/target/release/$BIN" "$INSTALL_DIR/$BIN"
    rm -rf "$TMPDIR"
    return 0
  fi

  say "Could not write to $INSTALL_DIR. Try: INSTALL_DIR=\$HOME/.local/bin sh install.sh"
  return 1
}

install_binary || install_from_source || fail "Install failed"

if command -v ctx >/dev/null 2>&1; then
  say ""
  say "✓ Installed $(ctx --version 2>/dev/null || echo ctx)"
  say ""
  say "Quick start:"
  say "  cd your-project"
  say "  ctx init"
  say "  ctx 'fix the login timeout bug'"
  say ""
  say "Docs: https://context-compiler.pages.dev/#wiki"
else
  say ""
  say "⚠ Installed to $INSTALL_DIR/$BIN but it is not on PATH."
  say "Add it with: export PATH=\"$INSTALL_DIR:\$PATH\""
fi
