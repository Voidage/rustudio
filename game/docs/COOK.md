# The cook tool (FORK-PLAN §8.2)

`game/cook` turns a place into the asset half of a game bundle: every
content id the place names, every asset a playthrough actually requests, and
a report of what the bundle now contains and what it could not get.

It is a developer tool. Unlike the launcher it may depend on upstream crates
(FORK-PLAN §3): it reuses the engine's `Document` reader and `AssetStore`
so the bytes it stages are the bytes Play would load.

## Run

```sh
cargo build --release --manifest-path game/Cargo.toml
game/target/release/rustudio-cook \
    --place my_place.rbxlx \
    --out game/build/my-game \
    --content game/content \
    --player target/release/rustudio-player \
    --seconds 8
```

`--content` is the pack root (`content/` + `ExtraContent/`, as the
launcher's `RUSTUDIO_ENGINE_CONTENT_DIR`); a bare Studio `content/` folder
still works. The playthrough runs **without a saved session**, as a shipped
game does, so the pack's local R15 body, head and Animate clips are what
land in `cache/assets`; `--signed-in` only affects the fetch step for
scanned private assets.

`--player` defaults to `$RUSTUDIO_PLAYER`, else `target/{release,debug}/rustudio-player`
in the current directory or one of its ancestors. Build the player first
(`cargo build --release -p rustudio-player`). `--no-capture` skips the
playthrough when no player is available.

`--signed-in` opens the saved Roblox session and lets the fetch step use it
for assets that public delivery refuses (many cloud assets need it). Without
it, and without a populated cache, private assets are reported as failed.

`--local-assets DIR` is an offline library (`<id>.<ext>`, the
`RUSTUDIO_LOCAL_ASSETS` layout): a scanned id the playthrough did not touch
is imported from there instead of downloaded. This is the path for a cook
with no network, and it makes the cook testable end to end.

## What it does

| Step | What happens |
|---|---|
| Clean cache | removes `<out>/cache` and starts from an empty staging cache — a cook never inherits bytes from an earlier run |
| Static scan | walks the DataModel: `Content`/`ContentId` properties (the reflection database, with the property-name list from `crates/rustudio-player/src/asset_report.rs` as fallback) and every `Source` for `rbxassetid://`, `?id=`, `assetId = 123`, `rbxasset://` |
| Place copy | copies the place to `<out>/place/<name>` |
| Runtime capture | plays the place headless for `--seconds` with `RUSTUDIO_CACHE_DIR=<out>/cache`, `RUSTUDIO_LOCAL_ASSETS` unset, core scripts off; every request lands in the cache, including ids only built by a script |
| Fetch missing | scanned cloud ids the playthrough did not request go through `AssetStore` (cache → public delivery → session), versioned ids to `<id>_v<version>.bin` |
| Local files | a path relative to the place is copied to `<out>/place/<path>`; `place/.rustudio-local-content` is written so Play may read it (an absolute path fails the cook — its id would still name the old file) |
| Report | `<out>/assets.json` (below) |

`rbxasset://` ids belong to the content pack (FORK-PLAN §7.1): the cook only
checks that the file resolves and never copies it. `rbxthumb://` and
`http(s)://` content are not cookable offline and fail the cook.

## `assets.json`

```json
{
  "manifest_version": 1,
  "place": "place/game.rbxlx",
  "source_place": "/path/to/game.rbxlx",
  "capture": { "seconds": 8.0, "player": "…", "log": "logs/cook-play.log", "exit": 0 },
  "summary": { "captured": 2, "downloaded": 1 },
  "assets": [
    {
      "id": "rbxassetid://900000001",
      "kind": "cloud",
      "asset_id": 900000001,
      "status": "captured",
      "size": 1234,
      "sha256": "…",
      "cache": "cache/assets/900000001_vlatest.bin",
      "refs": ["property Workspace.Wall.Decal.Texture"]
    }
  ],
  "problems": []
}
```

Statuses:

| Status | Meaning |
|---|---|
| `captured` | the playthrough requested it; the bytes are in `cache/` |
| `cached` | it was already in the clean cache when the playthrough ended |
| `downloaded` | the fetch step downloaded it |
| `imported` | it came from `--local-assets` instead of the network |
| `content-pack` | an `rbxasset://` file the content pack ships (not in the cache) |
| `local` | a local file copied into `place/` |
| `runtime` | `rbxtemp://` generated at run time; not a bundle asset |
| `live-thumbnail` / `unsupported` | a web image or URL: not cookable |
| `missing-content` | `rbxasset://` with no pack file and no legacy id |
| `failed` | the download failed; `why` says how (usually "sign in" or private) |

The cook exits 1 when `problems` is non-empty (a failed asset, a live
thumbnail, an unreadable local file, a crashed capture), 2 for bad arguments
or I/O errors. `--allow-missing` keeps the report but exits 0.

## Save assets for offline use (export, F4)

Everything Play downloads is already in the disk cache, so an asset is usable
offline on the machine that played it. The capture ledger records which
assets a place asked for: one JSON line per settled cloud request in
`<cache>/capture-ledger.jsonl` (`RUSTUDIO_CAPTURE_LEDGER` names another
file), with id, version, status, size, cache path, time and the place
(`RUSTUDIO_ASSET_PLACE`, set by the launcher and by the player/editor when a
place opens). Local and `rbxasset://` content is not captured; a failed
download is recorded too, so an export can fail loudly.

```sh
game/target/release/rustudio-cook export \
    --place my_place.rbxlx \
    --out game/build/my-game
```

This copies every ledger entry for that place out of the asset cache into
`<out>/cache/assets/<id>_v<version>.bin` (the layout the engine reads), and
writes `<out>/offline-assets.json`:

```json
{
  "manifest_version": 1,
  "kind": "offline-assets",
  "place": "/path/to/my_place.rbxlx",
  "ledger": "/home/me/.cache/rustudio/capture-ledger.jsonl",
  "summary": { "exported": 12, "failed": 1, "missing": 0 },
  "assets": [
    { "id": "rbxassetid://900000001", "status": "exported",
      "file": "cache/assets/900000001_vlatest.bin",
      "size": 1234, "sha256": "…", "ledger_time": 1795800000 }
  ]
}
```

Statuses: `exported` (bytes copied and hashed), `missing` (the ledger says
cached but the file is gone), `failed` (the request was refused; no cache
file). It exits 1 when anything is not exported, 2 on bad arguments or a
missing ledger/invalid place. `--ledger FILE` reads another ledger and
`--all` exports every place's captures in it.

This is the post-play companion to the cook: the cook starts from a clean
cache and plays the place itself; `export` keeps what the person captured
while editing or playing and pours it into an existing bundle. No network
call is made.

## After the cook

The bundle still needs its launcher, player, `game.json` and content pack;
`scripts/release-bundle.sh` (Phase 1) assembles those. The cook's cache is
the canonical asset set: the bundle's `cache/assets` is exactly what this
tool wrote. F2 (`RUSTUDIO_OFFLINE=1`) turns "the cache covers everything"
into an enforced promise; until then the smoke test is the check.

## Tests

```sh
cargo test --manifest-path game/Cargo.toml -p rustudio-cook

# Offline end-to-end, with no player and no network: the Phase 0 fixture's
# own asset library stands in for the editor cache.
game/target/debug/rustudio-cook \
    --place game/fixtures/phase0/place/game.rbxlx \
    --out /tmp/phase0-cook --local-assets game/fixtures/phase0/assets \
    --no-capture
```

The unit tests cover the scan (properties, strings, scripts, line numbers),
cache-name parsing and hashing, id classification, and argument parsing.
