#!/usr/bin/env bash
# Content-pack smoke test (FORK-PLAN §12 Phase 1, §14.13).
#
#   scripts/pack-smoke.sh [BUNDLE_DIR] [--no-warm]
#
# Builds the pack with gamepack, assembles a game bundle around the Phase 0
# R15 place, optionally warms the bundle cache once online (avatar meshes and
# Animate clips — the cook's capture step in a real release), then runs it
# headless and offline with a probe:
#
#   * PlayerModule, GetControls() and GetCameras() exist;
#   * holding W moves the character;
#   * the camera follows at a third-person distance;
#   * the character has a Head.
#
# The avatar body files are copied from the local Studio install by
# scripts/fetch-studio-avatar.sh; without Studio the engine's blocky fallback
# is used and the same checks still apply.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUNDLE="$ROOT/game/build/pack-smoke"
WARM=1
R6=0
for arg in "$@"; do
    case "$arg" in
        --no-warm) WARM=0 ;;
        --r6) R6=1 ;;
        *) BUNDLE="$arg" ;;
    esac
done
PLACE=game.rbxlx
[[ $R6 -eq 1 ]] && PLACE=game-r6.rbxlx
FIXTURE="$ROOT/game/fixtures/phase0"

PLAYER="$ROOT/target/release/rustudio-player"
LAUNCHER="$ROOT/game/target/release/rustudio-launcher"
GAMEPACK="$ROOT/game/target/release/rustudio-gamepack"

for bin in "$PLAYER" "$LAUNCHER" "$GAMEPACK"; do
    if [[ ! -x "$bin" ]]; then
        echo "missing $bin: build with 'cargo build --release -p rustudio-player' and" >&2
        echo "'cargo build --release --manifest-path game/Cargo.toml'" >&2
        exit 1
    fi
done

echo "== avatars from Studio (blocky fallback if none)"
"$ROOT/scripts/fetch-studio-avatar.sh" >/dev/null 2>&1 || true

echo "== building the content pack"
rm -rf "$BUNDLE"
"$GAMEPACK" build "$ROOT/game/content" --out "$BUNDLE/content" --version 0.1.0

mkdir -p "$BUNDLE/engine" "$BUNDLE/place" "$BUNDLE/cache/assets" "$BUNDLE/assets" "$BUNDLE/logs"
cp "$PLAYER" "$BUNDLE/engine/"
cp "$FIXTURE/place/game.rbxlx" "$FIXTURE/place/game-r6.rbxlx" "$BUNDLE/place/"
cp "$FIXTURE/assets/900000001.png" "$BUNDLE/cache/assets/900000001_vlatest.bin"
cp "$FIXTURE/assets/900000002.wav" "$BUNDLE/cache/assets/900000002_vlatest.bin"
cp "$FIXTURE/assets/900000001.png" "$BUNDLE/assets/900000001.png"
cp "$FIXTURE/assets/900000002.wav" "$BUNDLE/assets/900000002.wav"
cp "$LAUNCHER" "$BUNDLE/pack-smoke"
cat > "$BUNDLE/game.json" <<JSON
{
  "manifest_version": 1,
  "id": "pack-smoke",
  "name": "rustudio content pack smoke",
  "version": "0.1.0",
  "entry": "place/$PLACE",
  "cache": "cache",
  "assets": "assets",
  "content": { "mode": "bundled", "path": "content", "version": "0.1.0" },
  "window": { "title": "rustudio content pack smoke", "width": 1280, "height": 720 },
  "args": [],
  "offline": true
}
JSON

if [[ $WARM -eq 1 ]]; then
    echo "== warming avatar/animation assets (online, once)"
    (
        cd "$BUNDLE"
        RUSTUDIO_NO_STUDIO_FETCH=1 RUSTUDIO_CORE_SCRIPTS=0 RUSTUDIO_LIVE_DATA=0 \
        RUSTUDIO_CACHE_DIR="$BUNDLE/cache" RUSTUDIO_LOCAL_ASSETS="$BUNDLE/assets" \
        RUSTUDIO_CONTENT_DIR="$BUNDLE/content/content" RUSTUDIO_STUDIO_DIR="$BUNDLE/content" \
        RUSTUDIO_OFFLINE=0 \
            ./engine/rustudio-player place/game.rbxlx --headless --seconds 18 \
            --log logs/warm.log >/dev/null 2>&1
    )
fi

echo "== offline run: module, movement, camera, head"
rm -f "$BUNDLE/logs/smoke.log"
"$BUNDLE/pack-smoke" --headless --seconds 10 --key W@4:2 \
    --luau client "$ROOT/game/fixtures/pack/probe.luau@2" \
    --log logs/smoke.log

LOG="$BUNDLE/logs/smoke.log"
fail=0
check() {
    if ! grep -Fq -- "$1" "$LOG"; then
        echo "  missing: $1" >&2
        fail=1
    fi
}
expect_number() { # FIELD LINE
    local field=$1 line=$2 value
    value=$(sed -n "s/.*$field\([0-9.]*\).*/\1/p" <<<"$line" | head -1)
    if [[ -z "$value" ]] || ! awk -v v="$value" 'BEGIN { exit !(v > 2) }'; then
        echo "  $field is not above 2: $line" >&2
        fail=1
    fi
}

check "PACKPROBE module=true controls=true cameras=true"
check "PACKPROBE done"
line=$(grep -m1 "PACKPROBE moved=" "$LOG" || true)
if [[ -n "$line" ]]; then
    expect_number "moved=" "$line"
    if ! awk -v v="$(sed -n 's/.*cam=\([0-9.]*\)\.\..*/\1/p' <<<"$line")" 'BEGIN { exit !(v > 4 && v < 30) }'; then
        echo "  the camera is not at a third-person distance: $line" >&2
        fail=1
    fi
else
    echo "  the probe did not report movement" >&2
    fail=1
fi
check "PACKPROBE module=true"

if [[ $fail -ne 0 ]]; then
    echo "--- $LOG" >&2
    grep -E "PACKPROBE|error|Error" "$LOG" >&2 || true
    exit 1
fi
echo "pack smoke: passed"
