#!/usr/bin/env bash
# Content pack download mode smoke test (FORK-PLAN §13.13c).
#
#   scripts/pack-download-smoke.sh [BUNDLE_DIR]
#
# Builds a pack, publishes its zip, assembles a minimal bundle whose
# game.json uses `"mode": "download"` with a file:// URL, and checks:
#
#   * first launch installs the pack and starts the player with it;
#   * a second launch reuses the installed pack (the zip is removed first);
#   * a wrong sha256 falls back to engine content and still launches.
#
# Release binaries must exist (same builds as pack-smoke.sh):
#   cargo build --release -p rustudio-player
#   cargo build --release --manifest-path game/Cargo.toml
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUNDLE="${1:-$ROOT/game/build/pack-download}"
DATA="$BUNDLE/data"
PLAYER="$ROOT/target/release/rustudio-player"
LAUNCHER="$ROOT/game/target/release/rustudio-launcher"
GAMEPACK="$ROOT/game/target/release/rustudio-gamepack"
FIXTURE="$ROOT/game/fixtures/phase0"

for bin in "$PLAYER" "$LAUNCHER" "$GAMEPACK"; do
    if [[ ! -x "$bin" ]]; then
        echo "missing $bin: build the release player and game workspace first" >&2
        exit 1
    fi
done

echo "== build and publish the pack"
rm -rf "$BUNDLE"
"$GAMEPACK" build "$ROOT/game/content" --out "$BUNDLE/pack" --version 0.1.0
"$GAMEPACK" publish "$BUNDLE/pack" "$BUNDLE/serve"
SHA=$(sed -n 's/.*"sha256": "\([0-9a-f]\{64\}\)".*/\1/p' "$BUNDLE/pack/content-pack.json" | head -1)
[[ ${#SHA} -eq 64 ]] || { echo "no sha256 in content-pack.json" >&2; exit 1; }

mkdir -p "$BUNDLE/engine" "$BUNDLE/place" "$BUNDLE/logs"
cp "$PLAYER" "$BUNDLE/engine/"
cp "$FIXTURE/place/game.rbxlx" "$BUNDLE/place/"
cp "$LAUNCHER" "$BUNDLE/pack-download"
manifest() { # SHA OFFLINE
    cat > "$BUNDLE/game.json" <<JSON
{
  "manifest_version": 1,
  "id": "pack-download",
  "name": "rustudio download pack smoke",
  "version": "0.1.0",
  "entry": "place/game.rbxlx",
  "cache": "cache",
  "content": {
    "mode": "download",
    "url": "file://$BUNDLE/serve/0.1.0.zip",
    "sha256": "$1",
    "version": "0.1.0"
  },
  "args": [],
  "offline": false
}
JSON
}
manifest "$SHA"

echo "== first launch installs the pack"
(cd "$BUNDLE" && RUSTUDIO_GAME_DATA_DIR="$DATA" ./pack-download --headless --seconds 3 >/dev/null 2>&1)
grep -Fq "content: installed pack 0.1.0" "$BUNDLE/logs/latest.log" || {
    echo "pack was not installed:" >&2
    cat "$BUNDLE/logs/latest.log" >&2
    exit 1
}
grep -Fq "set RUSTUDIO_ENGINE_CONTENT_DIR=$DATA/pack-download/content-pack/0.1.0" "$BUNDLE/logs/latest.log" || {
    echo "the player was not pointed at the installed pack" >&2
    exit 1
}

echo "== second launch reuses it with the archive gone"
mv "$BUNDLE/serve/0.1.0.zip" "$BUNDLE/serve/removed.zip"
(cd "$BUNDLE" && RUSTUDIO_GAME_DATA_DIR="$DATA" ./pack-download --headless --seconds 3 >/dev/null 2>&1)
grep -Fq "content: reusing installed pack 0.1.0" "$BUNDLE/logs/latest.log" || {
    echo "the installed pack was not reused:" >&2
    cat "$BUNDLE/logs/latest.log" >&2
    exit 1
}

echo "== a wrong hash falls back without blocking the launch"
mv "$BUNDLE/serve/removed.zip" "$BUNDLE/serve/0.1.0.zip"
manifest "$(printf '0%.0s' {1..64})"
rm -rf "$BUNDLE/data-bad"
(cd "$BUNDLE" && RUSTUDIO_GAME_DATA_DIR="$BUNDLE/data-bad" ./pack-download --headless --seconds 3 >/dev/null 2>&1)
grep -Fq "content: download pack unavailable" "$BUNDLE/logs/latest.log" || {
    echo "a bad hash did not fall back:" >&2
    cat "$BUNDLE/logs/latest.log" >&2
    exit 1
}
grep -Fq "player exited with status 0" "$BUNDLE/logs/latest.log" || {
    echo "the player did not run after the fallback" >&2
    exit 1
}

echo "pack download smoke: passed"
