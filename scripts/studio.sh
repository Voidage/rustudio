#!/usr/bin/env bash
# Launch the rustudio editor from this checkout (no install step).
#
#   scripts/studio.sh [PLACE] [editor flags...]
#   scripts/studio.sh --debug [PLACE] [...]
#
# Runs target/release/rustudio, building it first when it is missing (or when
# RUSTUDIO_REBUILD=1). --debug uses target/debug instead. Everything after the
# optional --debug goes to the editor unchanged, e.g.:
#
#   scripts/studio.sh my_place.rbxlx
#   scripts/studio.sh --demo
#
# ./install.sh is the other path: it installs a `rustudio` command on PATH,
# a desktop entry and the icon, for a build you do not start from here.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROFILE=release
if [[ "${1-}" == --debug ]]; then
    PROFILE=debug
    shift
fi
BIN="$ROOT/target/$PROFILE/rustudio"
if [[ ! -x "$BIN" || "${RUSTUDIO_REBUILD:-0}" == 1 ]]; then
    echo "== building the editor ($PROFILE)"
    if [[ "$PROFILE" == release ]]; then
        cargo build --release --manifest-path "$ROOT/Cargo.toml"
    else
        cargo build --manifest-path "$ROOT/Cargo.toml"
    fi
fi
exec "$BIN" "$@"
