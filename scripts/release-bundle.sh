#!/usr/bin/env bash
# Assembles a release bundle from a cooked game (FORK-PLAN §15).
#
#   scripts/release-bundle.sh --bundle DIR --version X.Y.Z [options]
#
#   --bundle DIR      cooked bundle: game.json, place/, cache/
#                     (rustudio-cook's --out, plus a manifest)
#   --version X.Y.Z   downstream version
#   --out DIR         release folder and archive go here (default: dist/)
#   --name NAME       launcher and archive name (default: game.json's id)
#   --pack DIR        build this content pack into content/ (gamepack)
#   --pack-version V  content pack version (default: the release version)
#   --upstream BASE   upstream base commit (default: git merge-base HEAD
#                     upstream/main, else HEAD)
#   --no-build        use the existing release binaries (no cargo)
#
# The release is <out>/<name>-<version>-<os>-<arch>/ and a .tar.gz beside it
# with a .sha256. A release folder or archive that already exists is never
# overwritten: releases are immutable, and a sync starts the next version.
#
# release.json records the versions and the sha256 of every file:
#   { "game", "version", "release": "game-vX.Y.Z+upstream.A.B.C",
#     "upstream_base", "engine_version", "built", "platform", "files" }
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUNDLE=""
VERSION=""
OUT="$ROOT/dist"
NAME=""
PACK=""
PACK_VERSION=""
UPSTREAM_BASE=""
BUILD=1

while [[ $# -gt 0 ]]; do
    case "$1" in
        --bundle) BUNDLE="${2:?--bundle needs a value}"; shift 2 ;;
        --version) VERSION="${2:?--version needs a value}"; shift 2 ;;
        --out) OUT="${2:?--out needs a value}"; shift 2 ;;
        --name) NAME="${2:?--name needs a value}"; shift 2 ;;
        --pack) PACK="${2:?--pack needs a value}"; shift 2 ;;
        --pack-version) PACK_VERSION="${2:?--pack-version needs a value}"; shift 2 ;;
        --upstream) UPSTREAM_BASE="${2:?--upstream needs a value}"; shift 2 ;;
        --no-build) BUILD=0; shift ;;
        -h|--help) sed -n '2,28p' "$0"; exit 0 ;;
        *) echo "release-bundle: unknown option $1" >&2; exit 2 ;;
    esac
done

if [[ -z "$BUNDLE" || -z "$VERSION" ]]; then
    echo "release-bundle: --bundle and --version are required (--help)" >&2
    exit 2
fi
BUNDLE="$(cd "$BUNDLE" && pwd)"
if [[ ! -f "$BUNDLE/game.json" ]]; then
    echo "release-bundle: $BUNDLE has no game.json (cook first, then add the manifest)" >&2
    exit 2
fi

if [[ -z "$NAME" ]]; then
    NAME="$(sed -n 's/^[[:space:]]*"id"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$BUNDLE/game.json" | head -1)"
fi
if [[ -z "$NAME" ]]; then
    echo "release-bundle: no \"id\" in game.json; pass --name" >&2
    exit 2
fi

ENGINE_VERSION="$(sed -n 's/^version[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' "$ROOT/Cargo.toml" | head -1)"
if [[ -z "$ENGINE_VERSION" ]]; then
    echo "release-bundle: can't read the engine version from Cargo.toml" >&2
    exit 2
fi
if [[ -z "$UPSTREAM_BASE" ]]; then
    UPSTREAM_BASE="$(git -C "$ROOT" merge-base HEAD upstream/main 2>/dev/null || git -C "$ROOT" rev-parse HEAD)"
fi
UPSTREAM_BASE="${UPSTREAM_BASE:0:12}"

case "$(uname -s)" in
    Linux) OS=linux ;;
    Darwin) OS=macos ;;
    MINGW*|MSYS*|CYGWIN*) OS=windows ;;
    *) OS="$(uname -s | tr '[:upper:]' '[:lower:]')" ;;
esac
case "$(uname -m)" in
    x86_64|amd64) ARCH=x86_64 ;;
    arm64|aarch64) ARCH=aarch64 ;;
    *) ARCH="$(uname -m)" ;;
esac
if [[ "$OS" == windows ]]; then EXE=.exe; else EXE=; fi

RELEASE="game-v${VERSION}+upstream.${ENGINE_VERSION}"
STAGE="$OUT/${NAME}-${VERSION}-${OS}-${ARCH}"
ARCHIVE="$OUT/${NAME}-${VERSION}-${OS}-${ARCH}.tar.gz"
if [[ -e "$STAGE" || -e "$ARCHIVE" ]]; then
    echo "release-bundle: $STAGE (or its archive) exists; releases are immutable — bump --version" >&2
    exit 1
fi

PLAYER="$ROOT/target/release/rustudio-player$EXE"
LAUNCHER="$ROOT/game/target/release/rustudio-launcher$EXE"
GAMEPACK="$ROOT/game/target/release/rustudio-gamepack$EXE"
if [[ $BUILD -eq 1 ]]; then
    echo "== building the player, launcher, cook and gamepack"
    cargo build --release -p rustudio-player --manifest-path "$ROOT/Cargo.toml"
    cargo build --release --manifest-path "$ROOT/game/Cargo.toml"
fi
for bin in "$PLAYER" "$LAUNCHER"; do
    if [[ ! -x "$bin" ]]; then
        echo "release-bundle: missing $bin (build first, or without --no-build)" >&2
        exit 2
    fi
done

echo "== staging $RELEASE ($OS-$ARCH)"
mkdir -p "$STAGE/engine"
cp "$LAUNCHER" "$STAGE/$NAME$EXE"
cp "$PLAYER" "$STAGE/engine/"
cp "$BUNDLE/game.json" "$STAGE/game.json"
for part in place assets; do
    if [[ -d "$BUNDLE/$part" ]]; then
        cp -R "$BUNDLE/$part" "$STAGE/"
    fi
done
# Only the asset bytes are shipped from the cache. A cooked bundle that was
# played in place also holds cache/pipelines/ (GPU-specific, rebuilt) and
# cache/capture-ledger.jsonl (the developer's place paths and history);
# neither belongs in a release.
if [[ -d "$BUNDLE/cache/assets" ]]; then
    mkdir -p "$STAGE/cache"
    cp -R "$BUNDLE/cache/assets" "$STAGE/cache/"
fi
for report in assets.json offline-assets.json; do
    if [[ -f "$BUNDLE/$report" ]]; then
        cp "$BUNDLE/$report" "$STAGE/"
    fi
done
if [[ -n "$PACK" ]]; then
    if [[ ! -x "$GAMEPACK" ]]; then
        echo "release-bundle: missing $GAMEPACK (build the game workspace)" >&2
        exit 2
    fi
    echo "== content pack from $PACK"
    "$GAMEPACK" build "$PACK" --out "$STAGE/content" \
        --version "${PACK_VERSION:-$VERSION}"
fi

sha256() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    else
        shasum -a 256 "$1" | cut -d' ' -f1
    fi
}
json_string() {
    local s=$1
    s="${s//\\/\\\\}"
    s="${s//\"/\\\"}"
    printf '"%s"' "$s"
}

echo "== release.json"
{
    printf '{\n'
    printf '  "game": %s,\n' "$(json_string "$NAME")"
    printf '  "version": %s,\n' "$(json_string "$VERSION")"
    printf '  "release": %s,\n' "$(json_string "$RELEASE")"
    printf '  "upstream_base": %s,\n' "$(json_string "$UPSTREAM_BASE")"
    printf '  "engine_version": %s,\n' "$(json_string "$ENGINE_VERSION")"
    printf '  "built": %s,\n' "$(json_string "$(date -u +%Y-%m-%dT%H:%M:%SZ)")"
    printf '  "platform": %s,\n' "$(json_string "$OS-$ARCH")"
    printf '  "files": {\n'
    first=1
    while IFS= read -r -d '' file; do
        rel="${file#"$STAGE"/}"
        [[ $first -eq 1 ]] || printf ',\n'
        first=0
        printf '    %s: %s' "$(json_string "$rel")" "$(json_string "$(sha256 "$file")")"
    done < <(find "$STAGE" -type f ! -name release.json -print0 | sort -z)
    printf '\n  }\n'
    printf '}\n'
} > "$STAGE/release.json"

echo "== archive"
tar -czf "$ARCHIVE" -C "$OUT" "$(basename "$STAGE")"
HASH="$(sha256 "$ARCHIVE")"
# `sha256sum -c` format: the digest, two spaces, the archive's name.
printf '%s  %s\n' "$HASH" "$(basename "$ARCHIVE")" > "$ARCHIVE.sha256"
echo "$RELEASE"
echo "   folder  $STAGE"
echo "   archive $ARCHIVE"
echo "   sha256  $HASH"
