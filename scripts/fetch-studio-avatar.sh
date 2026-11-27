#!/usr/bin/env bash
# Copies the Studio content the engine reads at run time into the content
# pack (game/content): the R6/R15 bodies and Animate scripts, the legacy
# surface/sky/water textures and terrain materials (PlatformContent), fonts,
# cursors and character sounds, plus RbxCharacterSounds.
#
#   scripts/fetch-studio-avatar.sh [STUDIO_DIR]
#
# The Studio root is found from the argument, RUSTUDIO_STUDIO_DIR, Vinegar's
# newest version, or the copy rustudio downloaded from Roblox.
#
# These files are Roblox's and are NOT committed (.gitignore): they are copied
# from the developer's own Studio install for local/personal builds. For a
# public release, replace them with your own assets or confirm the asset terms
# first.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PACK="$ROOT/game/content"
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

STUDIO="$(find_studio "${1:-}")" || {
    echo "no Studio content found: pass a Studio dir, set RUSTUDIO_STUDIO_DIR," >&2
    echo "install Studio (Vinegar), or run 'rustudio --install-studio-content'." >&2
    exit 1
}

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
