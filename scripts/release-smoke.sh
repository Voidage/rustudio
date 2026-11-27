#!/usr/bin/env bash
# Linux release smoke test (FORK-PLAN §14.3/§15).
#
#   scripts/release-smoke.sh [OUT_DIR]
#
# Builds a release from the Phase 0 fixture with the real content pack, then
# checks the release as a person would receive it:
#
#   * the .sha256 line passes `sha256sum -c`;
#   * release.json hashes every staged file and the hashes match;
#   * the archive has no developer cache (pipelines, capture ledger), no
#     saves and no home;
#   * unpacked in a clean HOME with network disabled, the launcher uses the
#     bundled pack and the pack probe still finds PlayerModule, movement,
#     camera and a Head;
#   * the game's DataStores/home/cache stay inside the unpacked bundle.
#
# Linux only: this is the platform we ship and test first.  `unshare -n`
# needs privileges on some machines, so the network is disabled through the
# game's own switch (RUSTUDIO_NO_NETWORK=1), which is also the sealed-build
# path (`"network": false`).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${1:-$ROOT/game/build/release-smoke}"
FIXTURE="$ROOT/game/fixtures/phase0"
VERSION=0.1.0
NAME=release-smoke

if [[ "$(uname -s)" != Linux ]]; then
    echo "release-smoke: Linux only (this is Linux; run it here)" >&2
    exit 2
fi

echo "== cooking the fixture bundle"
rm -rf "$OUT"
mkdir -p "$OUT/bundle/place" "$OUT/bundle/cache/assets" "$OUT/bundle/assets"
cp "$FIXTURE/place/game.rbxlx" "$OUT/bundle/place/"
cp "$FIXTURE/assets/900000001.png" "$OUT/bundle/cache/assets/900000001_vlatest.bin"
cp "$FIXTURE/assets/900000002.wav" "$OUT/bundle/cache/assets/900000002_vlatest.bin"
cp "$FIXTURE/assets/900000001.png" "$OUT/bundle/assets/900000001.png"
cp "$FIXTURE/assets/900000002.wav" "$OUT/bundle/assets/900000002.wav"
cat > "$OUT/bundle/game.json" <<JSON
{
  "manifest_version": 1,
  "id": "$NAME",
  "name": "rustudio release smoke",
  "version": "$VERSION",
  "entry": "place/game.rbxlx",
  "cache": "cache",
  "assets": "assets",
  "content": { "mode": "bundled", "path": "content", "version": "$VERSION" },
  "window": { "title": "rustudio release smoke", "width": 1280, "height": 720 },
  "args": [],
  "offline": true
}
JSON

echo "== building the release"
"$ROOT/scripts/release-bundle.sh" --bundle "$OUT/bundle" --version "$VERSION" \
    --pack "$ROOT/game/content" --out "$OUT/dist" | tail -5

STAGE="$OUT/dist/$NAME-$VERSION-linux-x86_64"
ARCHIVE="$STAGE.tar.gz"
(cd "$OUT/dist" && sha256sum -c "$(basename "$ARCHIVE").sha256")

echo "== release.json covers every staged file"
python3 - "$STAGE" <<'PY'
import hashlib, json, pathlib, sys
root = pathlib.Path(sys.argv[1])
meta = json.loads((root / "release.json").read_text())
staged = {p.relative_to(root).as_posix() for p in root.rglob("*") if p.is_file()}
staged.discard("release.json")
listed = set(meta["files"])
assert staged == listed, (staged - listed, listed - staged)
for rel, want in meta["files"].items():
    got = hashlib.sha256((root / rel).read_bytes()).hexdigest()
    assert got == want, rel
for forbidden in ("cache/pipelines", "cache/capture-ledger.jsonl", "datastores/", "home/"):
    assert not any(p.startswith(forbidden) for p in listed), forbidden
print(f"   {len(listed)} files, all hashes match, no developer cache")
PY

echo "== unpack and run with a clean HOME, network disabled"
mkdir -p "$OUT/run" "$OUT/fakehome"
tar -xzf "$ARCHIVE" -C "$OUT/run"
GAME="$OUT/run/$NAME-$VERSION-linux-x86_64"
(
    # Run from elsewhere: the launcher resolves the bundle from its own path.
    cd "$OUT"
    env HOME="$OUT/fakehome" \
        XDG_CONFIG_HOME="$OUT/fakehome/.config" \
        XDG_CACHE_HOME="$OUT/fakehome/.cache" \
        XDG_DATA_HOME="$OUT/fakehome/.local/share" \
        RUSTUDIO_NO_NETWORK=1 \
        "$GAME/$NAME" --headless --seconds 10 --key W@4:2 \
        --luau client "$ROOT/game/fixtures/pack/probe.luau@2" \
        --log "$GAME/logs/run.log" >/dev/null 2>&1
)
grep -Fq "content: bundled pack $GAME/content" "$GAME/logs/latest.log"
grep -Fq "PACKPROBE module=true controls=true cameras=true" "$GAME/logs/run.log"
grep -Fq "PACKPROBE done" "$GAME/logs/run.log"
LINE=$(grep -m1 "PACKPROBE moved=" "$GAME/logs/run.log")
MOVED=$(sed -n 's/.*moved=\([0-9.]*\).*/\1/p' <<<"$LINE")
awk -v v="$MOVED" 'BEGIN { exit !(v > 2) }' || {
    echo "the character did not move: $LINE" >&2
    exit 1
}
grep -Fq "player exited with status 0" "$GAME/logs/latest.log"

# The game's private folders live in the bundle, not in the person's HOME.
test -d "$GAME/home" && test -d "$GAME/datastores" && test -d "$GAME/cache"
if find "$OUT/fakehome" -iname "*rustudio*" | grep -q .; then
    echo "the release wrote rustudio data into the person's HOME:" >&2
    find "$OUT/fakehome" -iname "*rustudio*" >&2
    exit 1
fi

echo "release smoke: passed ($MOVED studs moved)"
