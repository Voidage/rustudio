#!/usr/bin/env bash
# Offline smoke test (FORK-PLAN §14.3): the Phase 0 checks inside a fresh
# network namespace, so an asset request that reaches for the network fails
# at connect() instead of quietly succeeding through a dev machine's cache.
#
#   scripts/offline-smoke.sh [--soft] [BUNDLE_DIR]
#
#   --soft   run the smoke without network isolation (a machine that
#            refuses unshare; the F2 offline flag still makes the engine
#            serve only cache/local-store bytes, but this is not the honest
#            test)
#
# Needs Linux user namespaces (`unshare -n`); macOS/Windows: use a VM with
# networking off. The bundle is built if it does not exist.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SOFT=0
BUNDLE="$ROOT/game/build/phase0-game"
for arg in "$@"; do
    case "$arg" in
        --soft) SOFT=1 ;;
        *) BUNDLE="$arg" ;;
    esac
done

if [[ ! -x "$BUNDLE/phase0-game" ]]; then
    echo "== building the bundle"
    "$ROOT/scripts/phase0-bundle.sh" "$BUNDLE"
fi

if [[ $SOFT -eq 0 ]]; then
    if unshare -n true 2>/dev/null; then
        echo "== offline: no network interface"
        exec unshare -n "$ROOT/scripts/phase0-smoke.sh" "$BUNDLE"
    fi
    echo "offline-smoke: unshare -n is not permitted here." >&2
    echo "  Run on a machine with user namespaces, in a VM with networking off," >&2
    echo "  or pass --soft to run the smoke anyway (reported as not isolated)." >&2
    exit 1
fi

echo "== soft: running the smoke without network isolation (not the §14.3 gate)"
exec "$ROOT/scripts/phase0-smoke.sh" "$BUNDLE"
