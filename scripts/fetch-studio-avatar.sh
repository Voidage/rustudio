#!/usr/bin/env bash
# Copies the Studio content the engine reads at run time into the content
# pack (game/content): the R6/R15 bodies and Animate scripts, the legacy
# surface/sky/water textures and terrain materials (PlatformContent), fonts,
# cursors and character sounds, plus RbxCharacterSounds.
#
#   scripts/fetch-studio-avatar.sh [--into DIR] [--roblox-runtime] [STUDIO_DIR]
#
# The Studio root is found from the argument, RUSTUDIO_STUDIO_DIR, Vinegar's
# newest version, or the copy rustudio downloaded from Roblox.
#
# --into DIR        target pack root (default: game/content)
# --roblox-runtime  copy Roblox's DataModelPatch and full PlayerScripts (the
#                   CoreScripts, CoreGui, SettingsHub and the real PlayerModule)
#                   instead of the avatar/content files. Package staging uses
#                   this so a shipped game runs Roblox's own client scripts;
#                   without them it falls back to rustudio's native UI.
#
# These files are Roblox's and are NOT committed (.gitignore): they are copied
# from the developer's own Studio install for local/personal builds. For a
# public release, replace them with your own assets or confirm the asset terms
# first.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PACK="$ROOT/game/content"
MODE=content
STUDIO_ARG=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --into) PACK="${2:?--into needs a directory}"; shift 2 ;;
        --roblox-runtime) MODE=roblox; shift ;;
        -h|--help) sed -n '2,22p' "$0"; exit 0 ;;
        *) STUDIO_ARG="$1"; shift ;;
    esac
done
AVATAR="$PACK/content/avatar"
FILES=(
    "character.rbxm"
    "characterR15DynamicHeadV2.rbxm"
    "defaultDynamicHead.rbxm"
    "scripts/humanoidAnimateR15_DEPRECATED_20260813.rbxm"
    "scripts/humanoidAnimatePlayEmote.rbxm"
    "scripts/humanoidHealthRegenScript.rbxmx"
)
# Directories the engine reads at Play: legacy surfaces (studs), sky, water,
# terrain, fonts, cursors/particles and the character sounds.
DIRS=(
    "PlatformContent/pc/textures"
    "PlatformContent/pc/terrain"
    "PlatformContent/pc/fonts"
    "content/avatar/heads"
    "content/avatar/meshes"
    "content/textures"
    "content/sky"
    "content/fonts"
    "content/sounds"
    "ExtraContent/scripts/PlayerScripts/StarterPlayerScriptsCommon"
)

is_root() {
    [[ -f "$1/content/avatar/character.rbxm" && -d "$1/PlatformContent/pc/textures" ]]
}

find_studio() {
    if [[ -n "${1:-}" ]] && is_root "$1"; then
        echo "$1"
        return
    fi
    if [[ -n "${RUSTUDIO_STUDIO_DIR:-}" ]] && is_root "${RUSTUDIO_STUDIO_DIR}"; then
        echo "${RUSTUDIO_STUDIO_DIR}"
        return
    fi
    local versions="${HOME}/.var/app/org.vinegarhq.Vinegar/data/vinegar/versions"
    if [[ -d "$versions" ]]; then
        while IFS= read -r dir; do
            if is_root "$dir"; then
                echo "$dir"
                return
            fi
        done < <(find "$versions" -mindepth 1 -maxdepth 1 -type d | sort -r)
    fi
    local downloaded="${HOME}/.local/share/rustudio/studio"
    if is_root "$downloaded"; then
        echo "$downloaded"
        return
    fi
    return 1
}

STUDIO="$(find_studio "$STUDIO_ARG")" || {
    echo "no Studio content found: pass a Studio dir, set RUSTUDIO_STUDIO_DIR," >&2
    echo "install Studio (Vinegar), or run 'rustudio --install-studio-content'." >&2
    exit 1
}

if [[ $MODE == roblox ]]; then
    if [[ "$PACK" == "$ROOT/game/content" ]]; then
        echo "--roblox-runtime needs --into DIR: it replaces PlayerScripts and" >&2
        echo "adds the ~18 MB data model patch, which are package staging only." >&2
        exit 2
    fi
    # The client scripts a retail Roblox client runs: CoreGui/CorePackages and
    # the CoreScripts from the data model patch, plus the full PlayerScripts
    # tree (the real PlayerModule with CameraModule, ControlModule and
    # MouseLockController, which is where shift lock lives).
    MODELS="$STUDIO/ExtraContent/models"
    if [[ ! -f "$MODELS/DataModelPatch/DataModelPatch.rbxm" \
        || ! -d "$STUDIO/ExtraContent/scripts/PlayerScripts/StarterPlayerScripts" ]]; then
        echo "missing in Studio: ExtraContent/models/DataModelPatch or PlayerScripts" >&2
        exit 1
    fi
    rm -rf "$PACK/ExtraContent/models"
    mkdir -p "$PACK/ExtraContent/models"
    cp -R "$MODELS/." "$PACK/ExtraContent/models/"
    rm -rf "$PACK/ExtraContent/scripts/PlayerScripts"
    mkdir -p "$PACK/ExtraContent/scripts"
    cp -R "$STUDIO/ExtraContent/scripts/PlayerScripts" "$PACK/ExtraContent/scripts/PlayerScripts"
    # CoreScripts draw their icons with BuilderIcons: a font family JSON and
    # its two ttfs, loaded as rbxasset://LuaPackages/... Without them the
    # text stays the ligature names ("two-people", "gear") over the labels.
    ICONS="$STUDIO/ExtraContent/LuaPackages/Packages/_Index/BuilderIcons/BuilderIcons"
    if [[ -d "$ICONS" ]]; then
        rm -rf "$PACK/ExtraContent/LuaPackages/Packages/_Index/BuilderIcons/BuilderIcons"
        mkdir -p "$PACK/ExtraContent/LuaPackages/Packages/_Index/BuilderIcons"
        cp -R "$ICONS" "$PACK/ExtraContent/LuaPackages/Packages/_Index/BuilderIcons/"
    else
        echo "missing in Studio: ExtraContent/LuaPackages BuilderIcons fonts" >&2
    fi
    echo "copied Roblox's CoreScripts, PlayerScripts and icon fonts from $STUDIO to $PACK"
    exit 0
fi

mkdir -p "$AVATAR/scripts"
for file in "${FILES[@]}"; do
    cp "$STUDIO/content/avatar/$file" "$AVATAR/$file"
done

for dir in "${DIRS[@]}"; do
    if [[ ! -d "$STUDIO/$dir" ]]; then
        echo "missing in Studio: $dir" >&2
        exit 1
    fi
    rm -rf "$PACK/$dir"
    mkdir -p "$(dirname "$PACK/$dir")"
    cp -R "$STUDIO/$dir" "$PACK/$dir"
done

echo "copied ${#FILES[@]} avatar files and ${#DIRS[@]} content folders from $STUDIO to $PACK"
