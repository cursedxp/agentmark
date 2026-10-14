#!/bin/sh
# AgentMark installer for macOS and Linux.
#
#   curl -fsSL https://raw.githubusercontent.com/cursedxp/agentmark/main/install.sh | sh
#
# Clones the source into $AGENTMARK_HOME/src (default ~/.agentmark/src) and installs the
# `agentmark` compiler and the `agentmark-lsp` language server with `cargo install`.
# The checkout has to stay where it is: the compiler bakes in the path of crates/agentmark-rt
# and generated projects depend on it. Running the script again updates the checkout and
# reinstalls. `sh install.sh --uninstall` removes both binaries and the checkout.
#
# Environment:
#   AGENTMARK_HOME     where the source lives (default $HOME/.agentmark)
#   AGENTMARK_REF      branch or tag to install (default main)
#   AGENTMARK_REPO     repository to clone (default https://github.com/cursedxp/agentmark)
#   AGENTMARK_NO_LSP   set to 1 to skip the language server

set -eu

REPO="${AGENTMARK_REPO:-https://github.com/cursedxp/agentmark}"
REF="${AGENTMARK_REF:-main}"
HOME_DIR="${AGENTMARK_HOME:-$HOME/.agentmark}"
SRC="$HOME_DIR/src"

say() { printf '%s\n' "$*"; }
fail() { printf 'install.sh: %s\n' "$*" >&2; exit 1; }

usage() {
    cat <<'USAGE'
Usage: install.sh [--uninstall | --help]

Installs AgentMark (the agentmark compiler and the agentmark-lsp language server).
Run it again to update.

  --uninstall   cargo uninstall both binaries and delete the source folder
  --help        show this text

Environment: AGENTMARK_HOME (default ~/.agentmark), AGENTMARK_REF (default main),
AGENTMARK_REPO (default https://github.com/cursedxp/agentmark), AGENTMARK_NO_LSP=1.
USAGE
}

uninstall() {
    if command -v cargo >/dev/null 2>&1; then
        cargo uninstall agentmark >/dev/null 2>&1 || say "agentmark was not installed with cargo (skipped)"
        cargo uninstall agentmark-lsp >/dev/null 2>&1 || say "agentmark-lsp was not installed with cargo (skipped)"
    else
        say "cargo not found, so the binaries were not uninstalled"
    fi
    say "Removing $HOME_DIR"
    rm -rf "$HOME_DIR"
    say "AgentMark is uninstalled."
}

case "${1:-}" in
    --help | -h) usage; exit 0 ;;
    --uninstall) uninstall; exit 0 ;;
    "") ;;
    *) usage >&2; exit 2 ;;
esac

command -v git >/dev/null 2>&1 || fail "git is required but was not found. Install git and run this again."
if ! command -v cargo >/dev/null 2>&1; then
    say "cargo (the Rust toolchain) is required but was not found. Install it with:" >&2
    say "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh" >&2
    say "(see https://rustup.rs), open a new shell and run this again." >&2
    exit 1
fi

if [ -d "$SRC/.git" ]; then
    say "Updating the source in $SRC"
    git -C "$SRC" fetch --quiet origin "$REF"
    git -C "$SRC" checkout --quiet FETCH_HEAD
else
    say "Cloning $REPO into $SRC"
    mkdir -p "$HOME_DIR"
    git clone --quiet "$REPO" "$SRC"
    git -C "$SRC" fetch --quiet origin "$REF"
    git -C "$SRC" checkout --quiet FETCH_HEAD
fi

say "Installing agentmark"
cargo install --locked --path "$SRC"
if [ "${AGENTMARK_NO_LSP:-0}" = "1" ]; then
    say "Skipping agentmark-lsp (AGENTMARK_NO_LSP=1)"
else
    say "Installing agentmark-lsp"
    cargo install --locked --path "$SRC/crates/agentmark-lsp"
fi

BIN_DIR="${CARGO_INSTALL_ROOT:+$CARGO_INSTALL_ROOT/bin}"
BIN_DIR="${BIN_DIR:-${CARGO_HOME:-$HOME/.cargo}/bin}"
say ""
if [ -x "$BIN_DIR/agentmark" ]; then
    "$BIN_DIR/agentmark" --version
fi
case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) say "$BIN_DIR is not on your PATH. Add this to your shell profile:"
       say "  export PATH=\"$BIN_DIR:\$PATH\"" ;;
esac
say "Keep $SRC where it is: moving or deleting it breaks 'agentmark build' for generated projects."
say "Update later by running this script again; remove everything with 'sh install.sh --uninstall'."
