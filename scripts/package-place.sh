#!/usr/bin/env bash
# Package a place into a playable folder (Studio's File > Package Game…).
#
#   scripts/package-place.sh --place FILE --id ID [options]
#
#   --place FILE      the .rbxl/.rbxlx to package
#   --id ID           game id: the launcher's name and the folder name
#   --name NAME       window title (default: the id)
#   --version X.Y.Z   recorded in game.json and the pack (default 0.1.0)
#   --out DIR         output root (default: <place folder>/package)
#   --signed-in       let the cook use the saved Roblox session
#   --allow-missing   package even when the cook reports a missing asset
#                     (the cook output still names it; a release should not)
#   --keep-build      keep the cook's staging folder (.build)
#
# The result is <out>/<id>/:
#
#   <id>                  the launcher, double-click or run it
#   engine/rustudio-player
#   game.json
#   place/ cache/assets/  the cooked place and its asset bytes
#   assets/ assets.json   when the cook produced them
#   content/              the build content pack
#
# This is the development "play this build" folder; rerunning replaces it.
# A distribution release (immutable, versioned, with release.json and an
# archive) is scripts/release-bundle.sh. Binaries must already be built
# (cargo build --release -p rustudio-player and the game workspace).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PLACE=""
ID=""
NAME=""
VERSION=0.1.0
OUT=""
SIGNED=0
ALLOW=0
KEEP=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --place) PLACE="${2:?--place needs a file}"; shift 2 ;;
        --id) ID="${2:?--id needs a value}"; shift 2 ;;
        --name) NAME="${2:?--name needs a value}"; shift 2 ;;
        --version) VERSION="${2:?--version needs a value}"; shift 2 ;;
        --out) OUT="${2:?--out needs a directory}"; shift 2 ;;
        --signed-in) SIGNED=1; shift ;;
        --allow-missing) ALLOW=1; shift ;;
        --keep-build) KEEP=1; shift ;;
        -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
        *) echo "package-place: unknown option $1" >&2; exit 2 ;;
    esac
done

if [[ -z "$PLACE" || ! -f "$PLACE" ]]; then
    echo "package-place: --place FILE is required (and must exist)" >&2
    exit 2
fi
PLACE="$(cd "$(dirname "$PLACE")" && pwd)/$(basename "$PLACE")"
if [[ -z "$ID" ]]; then
    ID="$(basename "${PLACE%.*}")"
    ID="$(sed 's/[^A-Za-z0-9._-]/_/g; s/^\.*//' <<<"$ID")"
    [[ -n "$ID" ]] || ID=game
fi
NAME="${NAME:-$ID}"
OUT="${OUT:-$(dirname "$PLACE")/package}"
BUILD="$OUT/.build"
DEST="$OUT/$ID"

find_bin() { # NAME ENV
    local name=$1 env=$2 override
    override="$(printenv "$env" || true)"
    if [[ -n "$override" ]]; then
        echo "$override"
        return
    fi
    for dir in target/release target/debug game/target/release game/target/debug; do
        if [[ -x "$ROOT/$dir/$name" ]]; then
            echo "$ROOT/$dir/$name"
            return
        fi
    done
}
PLAYER="$(find_bin rustudio-player RUSTUDIO_PLAYER)"
LAUNCHER="$(find_bin rustudio-launcher RUSTUDIO_LAUNCHER)"
GAMEPACK="$(find_bin rustudio-gamepack RUSTUDIO_GAMEPACK)"
COOK="$(find_bin rustudio-cook RUSTUDIO_COOK)"
for pair in "player:$PLAYER" "launcher:$LAUNCHER" "gamepack:$GAMEPACK" "cook:$COOK"; do
    if [[ -z "${pair#*:}" ]]; then
        echo "package-place: ${pair%%:*} binary not found; build it first:" >&2
        echo "  cargo build --release -p rustudio-player" >&2
        echo "  cargo build --release --manifest-path game/Cargo.toml" >&2
        exit 2
    fi
done

# The pack must carry Studio's run-time content (surfaces, sky, terrain,
# fonts, sounds), not just the avatar: a shipped game has no Studio install
# to fall back on.
if [[ ! -f "$ROOT/game/content/PlatformContent/pc/textures/studs.dds" \
    || ! -d "$ROOT/game/content/content/sky" \
    || ! -d "$ROOT/game/content/ExtraContent/scripts/PlayerScripts/StarterPlayerScriptsCommon" ]]; then
    echo "== fetching Studio content for the pack"
    "$ROOT/scripts/fetch-studio-avatar.sh" || {
        echo "package-place: the content pack is incomplete (no Studio content)." >&2
        echo "Install Studio or point RUSTUDIO_STUDIO_DIR at one; a package without" >&2
        echo "it ships without studs textures, sky, fonts and character sounds." >&2
        exit 1
    }
fi

echo "== cook $(basename "$PLACE") -> $BUILD"
rm -rf "$BUILD" "$DEST"
mkdir -p "$BUILD"
COOK_ARGS=(--place "$PLACE" --out "$BUILD" --content "$ROOT/game/content"
    --player "$PLAYER" --seconds 8)
[[ $SIGNED -eq 1 ]] && COOK_ARGS+=(--signed-in)
[[ $ALLOW -eq 1 ]] && COOK_ARGS+=(--allow-missing)
"$COOK" "${COOK_ARGS[@]}"

ENTRY="$(find "$BUILD/place" -maxdepth 1 -type f \( -name '*.rbxl' -o -name '*.rbxlx' \) | head -1)"
if [[ -z "$ENTRY" ]]; then
    echo "package-place: the cook produced no place under $BUILD/place" >&2
    exit 1
fi
ENTRY="place/$(basename "$ENTRY")"

echo "== game.json"
ASSETS_FIELD=""
if [[ -d "$BUILD/assets" ]]; then
    ASSETS_FIELD='"assets": "assets",'
fi
cat > "$BUILD/game.json" <<JSON
{
  "manifest_version": 1,
  "id": "$ID",
  "name": "$NAME",
  "version": "$VERSION",
  "entry": "$ENTRY",
  "cache": "cache",
  $ASSETS_FIELD
  "content": { "mode": "bundled", "path": "content", "version": "$VERSION" },
  "window": { "title": "$NAME", "width": 1280, "height": 720 },
  "args": [],
  "offline": true
}
JSON

echo "== assemble $DEST"
mkdir -p "$DEST/engine" "$DEST/cache"
cp "$LAUNCHER" "$DEST/$ID"
cp "$PLAYER" "$DEST/engine/"
cp "$BUILD/game.json" "$DEST/"
cp -R "$BUILD/place" "$DEST/"
if [[ -d "$BUILD/cache/assets" ]]; then
    cp -R "$BUILD/cache/assets" "$DEST/cache/"
fi
for part in assets assets.json; do
    if [[ -e "$BUILD/$part" ]]; then
        cp -R "$BUILD/$part" "$DEST/"
    fi
done
"$GAMEPACK" build "$ROOT/game/content" --out "$DEST/content" --version "$VERSION" >/dev/null

if [[ $KEEP -eq 0 ]]; then
    rm -rf "$BUILD"
fi

if [[ $ALLOW -eq 1 ]]; then
    echo "note: --allow-missing: the cook report may list assets that did not make it"
fi
echo "Packaged $DEST"
echo "Run: $DEST/$ID"
