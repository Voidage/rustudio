# Game bundle format (v1) and the launcher

This is the downstream runtime contract from
[docs/FORK-PLAN.md §4–6](../../docs/FORK-PLAN.md). It is a copy-once folder:
the launcher, the unmodified player, a manifest, the place, and the assets
the place needs.

## Layout

```
MyGame/
  MyGame(.exe)            launcher binary (copied per game)
  game.json               manifest (below)
  engine/
    rustudio-player(.exe) unmodified upstream build
  place/
    game.rbxlx            entry place (any .rbxl/.rbxlx/.rbxm/.rbxmx)
  cache/
    assets/               <id>_v<version>.bin or <id>_vlatest.bin
  assets/                 optional local store: <id>.<ext>
  content/                optional bundled engine content pack
    content/...
    ExtraContent/scripts/PlayerScripts/...
  logs/                   latest.log (launcher + player output)
```

All manifest paths are relative to the bundle root and may not leave it
(`..` and absolute paths are rejected). The launcher resolves the bundle from
**its own executable path**, not the current directory, so a shortcut with a
different working directory still works.

## `game.json`

```json
{
  "manifest_version": 1,
  "id": "mygame",
  "name": "My Game",
  "version": "1.0.0",
  "engine": "rustudio 0.5.2 + fork.1",
  "entry": "place/game.rbxlx",
  "cache": "cache",
  "assets": "assets",
  "content": { "mode": "bundled", "path": "content", "version": "1.0.0" },
  "window": { "title": "My Game", "width": 1600, "height": 900 },
  "args": ["--window", "1600x900"],
  "offline": true
}
```

| Field | Required | Meaning |
|---|---|---|
| `manifest_version` | yes | Format version; this launcher reads `1` and rejects others. |
| `id`, `name` | yes | Identity; both are logged at launch. `name` is the window title after F1. |
| `version` | no | Recorded in the manifest; used by release tooling. |
| `engine` | no | Engine/base description; informational. |
| `entry` | yes | Entry place, relative to the bundle. |
| `cache` | no | Game asset cache (default `cache`). Must be writable: the engine checks it before the network and keeps its pipeline cache there. |
| `assets` | no | Optional local asset store (default `assets`). Set only when the folder exists. |
| `content` | no | Engine content pack (§7.1): absent, a mode (`"bundled"`, `"download"`, `"engine"`), an object, or a plain string naming a bundled pack path. |
| `window` | no | `width`/`height` become `--window WxH` unless `args` already sets one. `title` is used from F1. |
| `args` | no | Extra player arguments, passed verbatim (the harness escape hatch). |
| `offline` | no | `true` sets `RUSTUDIO_OFFLINE=1` for the player (F2): the cache and the local store only, no downloads. |

The launcher forwards its own command-line arguments to the player after the
manifest's. That is the debugging escape hatch:

```sh
./MyGame --seconds 10 --log logs/run.log --fail-on-error
```

## Content resolution (§7.1)

1. `bundled` (or a plain string path): `<bundle>/<path>` must exist. Its
   `content/` becomes `RUSTUDIO_CONTENT_DIR`; the pack root becomes
   `RUSTUDIO_STUDIO_DIR` **only** when it has
   `ExtraContent/scripts/PlayerScripts` (the engine validates the same rule).
2. `download`: not implemented yet (phase 1). The launcher logs the fallback
   and starts without a pack.
3. `engine`: leaves both variables unset so the engine's own lookup decides
   (development only).
4. No pack or a missing/invalid one: the game still launches with the
   engine's blocky fallback and logs what is missing. A missing pack never
   fails the launch.

## Environment contract (§6)

The launcher sets these **only when the developer has not already set them**,
so a debug session can override any of them:

| Variable | Value |
|---|---|
| `RUSTUDIO_NO_SESSION` | `1` |
| `RUSTUDIO_NO_STUDIO_FETCH` | `1` |
| `RUSTUDIO_CORE_SCRIPTS` | `0` |
| `RUSTUDIO_LIVE_DATA` | `0` |
| `RUSTUDIO_CACHE_DIR` | `<bundle>/cache` |
| `RUSTUDIO_LOCAL_ASSETS` | `<bundle>/assets` (only if it exists) |
| `RUSTUDIO_CONTENT_DIR` | `<pack>/content` (only when a pack resolves) |
| `RUSTUDIO_STUDIO_DIR` | `<pack>` (only when it validates) |
| `RUSTUDIO_OFFLINE` | `1` when the manifest's `offline` is true (F2) |

The launcher changes its working directory to the bundle root, wires the
player's stdout and stderr to `logs/latest.log`, propagates the player's exit
code (128 + signal on Unix when it was killed), and writes a final log line
with the status and duration.

## Why spawn mode

The launcher has no dependency on any rustudio crate, so upstream engine API
changes cannot break it; it only needs the bundle format. In-process mode
(F1) is a later option for a single binary and per-game window identity.
