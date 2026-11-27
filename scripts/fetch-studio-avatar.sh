#!/usr/bin/env bash
# Copies Studio's default R6/R15 bodies and Animate scripts into the content
# pack: game/content/content/avatar/. The engine reads them from a Studio
# layout (RUSTUDIO_STUDIO_DIR, then Vinegar's newest Studio version).
#
#   scripts/fetch-studio-avatar.sh [STUDIO_DIR]
#
# These files are Roblox's and are NOT committed (.gitignore): they are copied
# from the developer's own Studio install for local/personal builds. For a
# public release, replace them with your own rigs and Animate script or
# confirm the asset terms first.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$ROOT/game/content/content/avatar"
FILES=(
    "character.rbxm"
    "characterR15DynamicHeadV2.rbxm"
    "defaultDynamicHead.rbxm"
    "scripts/humanoidAnimateR15_DEPRECATED_20260813.rbxm"
    "scripts/humanoidAnimatePlayEmote.rbxm"
    "scripts/humanoidHealthRegenScript.rbxmx"
)

find_studio() {
    if [[ -n "${1:-}" && -f "$1/content/avatar/character.rbxm" ]]; then
        echo "$1"
        return
    fi
    if [[ -n "${RUSTUDIO_STUDIO_DIR:-}" && -f "${RUSTUDIO_STUDIO_DIR}/content/avatar/character.rbxm" ]]; then
        echo "${RUSTUDIO_STUDIO_DIR}"
        return
    fi
    local versions="${HOME}/.var/app/org.vinegarhq.Vinegar/data/vinegar/versions"
    if [[ -d "$versions" ]]; then
        while IFS= read -r dir; do
            if [[ -f "$dir/content/avatar/character.rbxm" ]]; then
                echo "$dir"
                return
            fi
        done < <(find "$versions" -mindepth 1 -maxdepth 1 -type d | sort -r)
    fi
    return 1
}

STUDIO="$(find_studio "${1:-}")" || {
    echo "no Studio content/avatar found: pass a Studio dir, set RUSTUDIO_STUDIO_DIR," >&2
    echo "or install Studio (Vinegar). The pack then falls back to blocky bodies." >&2
    exit 1
}
mkdir -p "$DEST/scripts"
for file in "${FILES[@]}"; do
    cp "$STUDIO/content/avatar/$file" "$DEST/$file"
done
echo "copied ${#FILES[@]} avatar files from $STUDIO to $DEST"
