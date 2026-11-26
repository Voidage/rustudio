#!/usr/bin/env bash
# Phase 0 offline-ish smoke test (FORK-PLAN §12, §14).
#
#   scripts/phase0-smoke.sh [BUNDLE_DIR]
#
# Runs the bundled game headless three times — the launcher for the R15 entry
# place, the engine directly with the same environment contract for the R6
# variant, then the launcher with a clean HOME and no Studio content (the
# blocky fallback) — and asserts on the fixture's "PROBE ..." Output lines:
#
#   * the bundled texture and sound come from cache (no download);
#   * the in-place KeyframeSequence gives the track a real Length;
#   * the shoulder Motor6D.Transform moves and the pose varies;
#   * the sound decodes and is playing;
#   * the launcher propagates a successful exit code.
#
# The runs are headless (no window): visuals are checked separately by
# launching the bundle normally. Networking is not isolated here; the point
# of this pass is that the fixture's assets never ask for the network. The
# `unshare -n` clean-room run is FORK-PLAN §14's later gate.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUNDLE="${1:-$ROOT/game/build/phase0-game}"

if [[ ! -x "$BUNDLE/phase0-game" ]]; then
    echo "no bundle at $BUNDLE; run scripts/phase0-bundle.sh first" >&2
    exit 1
fi

check() {
    local log=$1
    local ok=1

    expect() {
        if ! grep -Fq -- "$1" "$log"; then
            echo "  missing: $1" >&2
            ok=0
        fi
    }
    expect_not() {
        if grep -Fq -- "$1" "$log"; then
            echo "  unexpected: $1" >&2
            ok=0
        fi
    }
    number() { # FIELD LINE
        local field=$1 line=$2
        awk -v line="$line" -v field="$field" '
            index(line, field) {
                rest = substr(line, index(line, field) + length(field))
                sub(/[^0-9.].*$/, "", rest)
                print rest
                exit
            }' <<<"$line"
    }

    expect "PROBE texture=Success sound=Success"
    expect "PROBE done"

    local track moved distinct sound_length
    track=$(grep -m1 "PROBE track " "$log" || true)
    moved=$(grep -m1 "PROBE joint=" "$log" || true)
    sound_length=$(grep -m1 "PROBE sound " "$log" || true)
    if [[ -z "$track" || -z "$moved" || -z "$sound_length" ]]; then
        echo "  the fixture did not reach all checks" >&2
        ok=0
    else
        local length
        length=$(number "PROBE track length=" "$track")
        if ! awk -v v="$length" 'BEGIN { exit !(v > 0.5) }'; then
            echo "  track length is not real: $track" >&2
            ok=0
        fi
        local m d
        m=$(number "moved=" "$moved")
        d=$(number "distinct=" "$moved")
        if ! awk -v m="$m" -v d="$d" 'BEGIN { exit !(m > 0 && d > 2) }'; then
            echo "  the rig did not animate: $moved" >&2
            ok=0
        fi
        if ! grep -Fq "PROBE sound playing=true" "$log"; then
            echo "  the sound is not playing" >&2
            ok=0
        fi
        local slen
        slen=$(number "length=" "$sound_length")
        if ! awk -v v="$slen" 'BEGIN { exit !(v > 0) }'; then
            echo "  the sound did not decode: $sound_length" >&2
            ok=0
        fi
    fi

    # Cache-first: the fixture ids must never have been downloaded.
    expect_not "Downloaded asset 900000001"
    expect_not "Downloaded asset 900000002"
    expect_not "Asset 900000001 could not be downloaded"
    expect_not "Asset 900000002 could not be downloaded"

    if [[ $ok -eq 0 ]]; then
        echo "--- $log" >&2
        grep -E "PROBE|error|Error|Warn|failed|could not" "$log" >&2 || true
        return 1
    fi
}

echo "== R15 through the launcher"
rm -f "$BUNDLE/logs/smoke-r15.log"
"$BUNDLE/phase0-game" --headless --seconds 12 --log logs/smoke-r15.log --fail-on-error
check "$BUNDLE/logs/smoke-r15.log"
echo "   ok"

echo "== R6 through the engine (same environment contract)"
rm -f "$BUNDLE/logs/smoke-r6.log"
(
    cd "$BUNDLE"
    RUSTUDIO_NO_SESSION=1 \
    RUSTUDIO_NO_STUDIO_FETCH=1 \
    RUSTUDIO_CORE_SCRIPTS=0 \
    RUSTUDIO_LIVE_DATA=0 \
    RUSTUDIO_CACHE_DIR="$BUNDLE/cache" \
    RUSTUDIO_LOCAL_ASSETS="$BUNDLE/assets" \
        ./engine/rustudio-player place/game-r6.rbxlx \
        --headless --seconds 12 --log logs/smoke-r6.log --fail-on-error
)
check "$BUNDLE/logs/smoke-r6.log"
echo "   ok"

echo "== clean HOME, no Studio content (blocky fallback)"
CLEAN_HOME=$(mktemp -d)
RUSTUDIO_STUDIO_DIR=/nonexistent \
    HOME="$CLEAN_HOME" \
    "$BUNDLE/phase0-game" --headless --seconds 12 --log logs/smoke-clean.log --fail-on-error
check "$BUNDLE/logs/smoke-clean.log"
if find "$CLEAN_HOME" -mindepth 1 -maxdepth 2 ! -path '*/.cache' ! -path '*/.cache/*' | grep -q .; then
    echo "  the player wrote outside the bundle's cache:" >&2
    find "$CLEAN_HOME" -mindepth 1 | head >&2
    rm -rf "$CLEAN_HOME"
    exit 1
fi
rm -rf "$CLEAN_HOME"
echo "   ok"

echo "phase0 smoke: passed"
