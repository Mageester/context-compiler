#!/bin/sh
# Context Compiler install script
# Usage: curl -fsSL https://ctx-compiler.getaxiom.ca/install.sh | sh
#        curl -fsSL https://ctx-compiler.getaxiom.ca/install.sh | sh -s -- --dry-run
#        curl -fsSL https://ctx-compiler.getaxiom.ca/install.sh | sh -s -- --version

set -eu

REPO="Mageester/context-compiler"
BIN="ctx"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
GITHUB_API="https://api.github.com/repos/$REPO/releases/latest"
SCRIPT_VERSION="0.2.0"

say()    { printf '%s\n' "$*"; }
fail()   { say "✗ $*"; exit 1; }
warn()   { say "⚠ $*"; }

# ── Parse flags ──
DRY_RUN=false
SHOW_VERSION=false
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=true ;;
    --version) SHOW_VERSION=true ;;
  esac
done

if [ "$SHOW_VERSION" = true ]; then
  say "Context Compiler install.sh version ${SCRIPT_VERSION}"
  say "Repository: https://github.com/${REPO}"
  say "Source:     https://ctx-compiler.getaxiom.ca/install.sh"
  exit 0
fi

if [ "$DRY_RUN" = true ]; then
  say "🧪 Dry run — no changes will be made"
fi

ARCH=$(uname -m)
OS=$(uname -s | tr '[:upper:]' '[:lower:]')

case "$ARCH" in
  x86_64|amd64) ARCH="x86_64" ;;
  aarch64|arm64) ARCH="aarch64" ;;
  *) fail "Unsupported architecture: $ARCH (expected x86_64 or amd64 for Intel/AMD, aarch64 or arm64 for ARM)" ;;
esac

case "$OS" in
  darwin) OS="apple-darwin" ;;
  linux)  OS="unknown-linux-gnu" ;;
  *)      fail "Unsupported OS: $OS (expected macOS 'darwin' or Linux)" ;;
esac

TARGET="${ARCH}-${OS}"
say "↓ Context Compiler — installing for ${TARGET}"

install_binary() {
  say "  → Checking latest release from GitHub…"
  API_OUT=$(curl -fsSL "$GITHUB_API" 2>/dev/null || true)

  if [ -z "$API_OUT" ]; then
    warn "  → GitHub API returned no response"
    return 1
  fi

  VERSION=$(echo "$API_OUT" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1)
  if [ -z "$VERSION" ]; then
    warn "  → No release tag found in API response"
    return 1
  fi

  FILENAME="${BIN}-${VERSION}-${TARGET}.tar.gz"
  URL="https://github.com/$REPO/releases/download/$VERSION/$FILENAME"
  TMP="/tmp/$FILENAME"

  say "  → Found release ${VERSION}"
  say "  → Downloading ${FILENAME}"

  HTTP_CODE=$(curl -fsSL -w '%{http_code}' "$URL" -o "$TMP" 2>/dev/null || echo "000")

  if [ "$HTTP_CODE" = "404" ]; then
    rm -f "$TMP"
    warn "  → No binary for ${TARGET} in release ${VERSION}"
    return 1
  fi

  if [ "$HTTP_CODE" = "000" ]; then
    rm -f "$TMP"
    warn "  → Network error downloading ${FILENAME} (check your internet connection)"
    return 1
  fi

  if [ ! -s "$TMP" ]; then
    rm -f "$TMP"
    warn "  → Downloaded file is empty"
    return 1
  fi

  say "  → Extracting archive…"
  tar xzf "$TMP" -C /tmp 2>/dev/null || {
    rm -f "$TMP"
    warn "  → Failed to extract archive (corrupted download?)"
    return 1
  }

  if [ ! -f "/tmp/$BIN" ]; then
    rm -f "$TMP"
    warn "  → Binary not found inside archive (expected: ctx)"
    return 1
  fi

  say "  → Installing to ${INSTALL_DIR}/${BIN}"

  if [ "$DRY_RUN" = true ]; then
    say "  → [DRY RUN] Would install: ${INSTALL_DIR}/${BIN}"
    rm -f "$TMP" "/tmp/$BIN"
    return 0
  fi

  if install -m 755 "/tmp/$BIN" "$INSTALL_DIR/$BIN" 2>/dev/null; then
    rm -f "$TMP" "/tmp/$BIN"
    return 0
  fi

  say "  → Permission denied for ${INSTALL_DIR}; retrying with sudo"
  if command -v sudo >/dev/null 2>&1; then
    sudo install -m 755 "/tmp/$BIN" "$INSTALL_DIR/$BIN"
    rm -f "$TMP" "/tmp/$BIN"
    return 0
  fi

  warn "  → Permission denied and sudo is not available"
  return 1
}

install_from_source() {
  if [ "$DRY_RUN" = true ]; then
    say "  → [DRY RUN] Would build from source at https://github.com/${REPO}"
    return 0
  fi

  if ! command -v cargo >/dev/null 2>&1; then
    say ""
    say "Rust/Cargo is required to build from source. Install Rust, then rerun:"
    say "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    say "  curl -fsSL https://ctx-compiler.getaxiom.ca/install.sh | sh"
    return 1
  fi

  say "  → No release binary for ${TARGET} — building from source"
  say "  → This will take a few minutes on the first build."

  TMPDIR=$(mktemp -d)
  say "  → Cloning https://github.com/${REPO} (shallow clone)"
  git clone --depth 1 "https://github.com/$REPO.git" "$TMPDIR" >/dev/null 2>&1 || {
    rm -rf "$TMPDIR"
    warn "  → Failed to clone repository (network error?)"
    return 1
  }

  say "  → Building optimized binary (cargo build --release)"
  (cd "$TMPDIR" && cargo build --release) 2>&1 || {
    rm -rf "$TMPDIR"
    warn "  → Build failed — check the output above for errors"
    [ -f "$TMPDIR/target/release/$BIN" ] && ls -la "$TMPDIR/target/release/$BIN"
    return 1
  }

  if [ ! -f "$TMPDIR/target/release/$BIN" ]; then
    rm -rf "$TMPDIR"
    warn "  → Build succeeded but binary not found at target/release/${BIN}"
    return 1
  fi

  say "  → Installing to ${INSTALL_DIR}/${BIN}"
  if install -m 755 "$TMPDIR/target/release/$BIN" "$INSTALL_DIR/$BIN" 2>/dev/null; then
    rm -rf "$TMPDIR"
    return 0
  fi

  say "  → Permission denied for ${INSTALL_DIR}; retrying with sudo"
  if command -v sudo >/dev/null 2>&1; then
    sudo install -m 755 "$TMPDIR/target/release/$BIN" "$INSTALL_DIR/$BIN"
    rm -rf "$TMPDIR"
    return 0
  fi

  say "Could not write to ${INSTALL_DIR}. Try:"
  say "  INSTALL_DIR=\$HOME/.local/bin sh install.sh"
  return 1
}

# ── Main install sequence ──
if [ "$DRY_RUN" = true ]; then
  install_binary || install_from_source || true
else
  install_binary || install_from_source || fail "Install failed — see messages above for details."
fi

if [ "$DRY_RUN" = true ]; then
  say ""
  say "✓ Dry run complete — no changes were made."
  exit 0
fi

# ── Post-install: verify binary on PATH ──
if command -v "$BIN" >/dev/null 2>&1; then
  INSTALLED_VERSION=$(ctx --version 2>/dev/null || true)
  if [ -n "$INSTALLED_VERSION" ]; then
    say ""
    say "✓ ${INSTALLED_VERSION} installed successfully"
  else
    say ""
    say "✓ ${BIN} installed successfully (${INSTALL_DIR}/${BIN})"
  fi
  say ""
  say "Quick start:"
  say "  cd your-project"
  say "  ${BIN} init"
  say "  ${BIN} 'fix the login timeout bug'"
  say ""
  say "Docs: https://ctx-compiler.getaxiom.ca/#wiki"
else
  if [ -f "$INSTALL_DIR/$BIN" ]; then
    say ""
    say "⚠ Installed to ${INSTALL_DIR}/${BIN} but it is not on PATH."
    say "Add it with: export PATH=\"${INSTALL_DIR}:\$PATH\""
    say "Or rerun with: INSTALL_DIR=\$HOME/.local/bin sh install.sh"
  else
    say ""
    say "⚠ Binary was not installed (check errors above)"
    exit 1
  fi
fi
