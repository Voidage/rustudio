#!/usr/bin/env bash
# Assembles the Phase 0 game bundle (FORK-PLAN §4, §12).
#
#   scripts/phase0-bundle.sh [BUNDLE_DIR]
#
# Builds the unmodified player and the launcher, then lays out a portable,
# writable bundle:
#
#   BUNDLE_DIR/
#     phase0-game            launcher binary
#     game.json              manifest (fixtures/phase0/game.json)
#     engine/rustudio-player unmodified upstream build
#     place/game.rbxlx       R15 entry place (game-r6.rbxlx alongside it)
#     cache/assets/<id>_vlatest.bin   seeded assets (texture, sound)
#     assets/<id>.<ext>      the same files for the local store
#     logs/                  created at runtime
#
# The cache is checked before the network for every asset type, so the
# fixture's texture and sound never leave the bundle.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FIXTURE="$ROOT/game/fixtures/phase0"
OUT="${1:-$ROOT/game/build/phase0-game}"

echo "== building the unmodified player"
cargo build --release -p rustudio-player --manifest-path "$ROOT/Cargo.toml"

echo "== building the launcher"
cargo build --release --manifest-path "$ROOT/game/Cargo.toml"

echo "== assembling $OUT"
rm -rf "$OUT"
mkdir -p "$OUT"/{engine,place,cache/assets,assets,logs}

cp "$ROOT/target/release/rustudio-player" "$OUT/engine/"
cp "$FIXTURE/place/game.rbxlx" "$FIXTURE/place/game-r6.rbxlx" "$OUT/place/"
cp "$FIXTURE/game.json" "$OUT/game.json"
cp "$ROOT/game/target/release/rustudio-launcher" "$OUT/phase0-game"

# Seed both the cache (<id>_vlatest.bin, checked first) and the optional
# local store (<id>.<ext>) with the same bytes.
for file in "$FIXTURE"/assets/*; do
    name="$(basename "$file")"
    id="${name%%.*}"
    ext="${name#*.}"
    cp "$file" "$OUT/cache/assets/${id}_vlatest.bin"
    cp "$file" "$OUT/assets/${id}.${ext}"
done

echo "== bundle ready: $OUT"
find "$OUT" -maxdepth 2 -type f -printf '   %P\n' | sort
