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
  datastores/             local DataStore records (created at runtime, F5)
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
  "datastores": "datastores",
  "content": { "mode": "bundled", "path": "content", "version": "1.0.0" },
  "window": { "title": "My Game", "width": 1600, "height": 900 },
  "args": ["--window", "1600x900"],
  "offline": true
}
```

| Field | Required | Meaning |
|---|---|---|
| `manifest_version` | yes | Format version; this launcher reads `1` and rejects others. |
| `id`, `name` | yes | Identity; both are logged at launch. `name` is the window title when `window.title` is absent (F1). |
| `version` | no | Recorded in the manifest; used by release tooling. |
| `engine` | no | Engine/base description; informational. |
| `entry` | yes | Entry place, relative to the bundle. |
| `cache` | no | Game asset cache (default `cache`). Must be writable: the engine checks it before the network and keeps its pipeline cache there. |
| `assets` | no | Optional local asset store (default `assets`). Set only when the folder exists. |
| `datastores` | no | Local DataStore records (default `datastores`, F5). Created if missing. Game scripts still use `DataStoreService`; this is only where the local backend keeps its records. |
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

1. `bundled` (or a plain string path): `<bundle>/<path>` must exist and is
   set as `RUSTUDIO_ENGINE_CONTENT_DIR` (F3). The engine uses its `content/`
   for `rbxasset://` and its `ExtraContent/scripts/PlayerScripts` for the
   default player scripts, validating each half on its own: a pack with only
   one half still works for that half.
2. `download`: not implemented yet. The launcher logs the fallback and starts
   without a pack.
3. `engine`: leaves `RUSTUDIO_ENGINE_CONTENT_DIR` unset so the engine's own
   lookup decides (development only).
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
| `RUSTUDIO_DATASTORE_DIR` | `<bundle>/datastores` (F5) |
| `RUSTUDIO_WINDOW_TITLE` | `window.title`, else the game name (F1) |
| `RUSTUDIO_LOCAL_ASSETS` | `<bundle>/assets` (only if it exists) |
| `RUSTUDIO_ENGINE_CONTENT_DIR` | `<pack>` (only when a pack resolves) |
| `RUSTUDIO_OFFLINE` | `1` when the manifest's `offline` is true (F2) |
| `RUSTUDIO_ASSET_PLACE` | `<bundle>/<entry>`: the capture ledger's place key (F4) |

`RUSTUDIO_DATASTORE_DIR` also tells the engine to use its local DataStore
backend for the bundle's place: a place loaded from a file has `PlaceId 0`,
which normally refuses DataStores. The value survives restarts; it is not
stored in the place file. If the bundle is read-only, this directory must be
redirected (a per-user location is future work).

The launcher changes its working directory to the bundle root, wires the
player's stdout and stderr to `logs/latest.log`, propagates the player's exit
code (128 + signal on Unix when it was killed), and writes a final log line
with the status and duration. The log's first line is the game's name,
version and id. A failure before the player starts is also shown to the
person: a native message box on Windows (the build has no console) and
stderr elsewhere, and the reason is written to `logs/latest.log`.

## Why spawn mode

The launcher has no dependency on any rustudio crate, so upstream engine API
changes cannot break it; it only needs the bundle format. In-process mode
(F1) is a later option for a single binary and per-game window identity.
