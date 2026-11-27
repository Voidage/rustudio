# Releases (FORK-PLAN §15)

A release is an immutable, versioned, checksummed archive of a game bundle:
launcher, player, `game.json`, place, asset cache and the content pack. The
engine is the unmodified upstream build at a recorded upstream base.

## Versioning

- Downstream release name: `game-vX.Y.Z+upstream.A.B.C`, where `A.B.C` is
  the engine version in the root `Cargo.toml`.
- Bundle file: `<game>-<version>-<os>-<arch>.tar.gz` beside its `.sha256`.
- Release metadata is `release.json` in the bundle root: game, version,
  release name, upstream base commit, engine version, build time, platform,
  and the sha256 of every file. (The runtime manifest `game.json` still
  holds only what the launcher reads.)
- Releases are never overwritten: a new version is a new folder and
  archive. Sync work happens on the `game` branch; releases are tags.

## Build a release

```sh
# 1. Cook the place into a bundle (game/docs/COOK.md). The cook's --out has
#    place/ and cache/; the game's game.json goes beside it.
game/target/release/rustudio-cook --place my_place.rbxlx \
    --out game/build/my-game --content game/content

# 2. Optionally add what the person captured while playing (F4).
game/target/release/rustudio-cook export --place my_place.rbxlx \
    --out game/build/my-game

# 3. Assemble, version and archive.
scripts/release-bundle.sh --bundle game/build/my-game \
    --version 1.0.0 --pack game/content \
    --out dist
```

The script builds the player, launcher and gamepack (skip with `--no-build`),
stages `dist/<game>-<version>-<os>-<arch>/` with:

```text
<game>                  launcher (the manifest's id, or --name)
engine/rustudio-player  the unmodified player
game.json               runtime manifest
place/ cache/ assets/   from the cooked bundle (assets/ if present)
assets.json             cook report, when present
offline-assets.json     F4 export report, when present
content/                gamepack-built content pack
release.json            versions + per-file sha256
```

then writes the `.tar.gz` and its `.sha256`. A release folder or archive
that already exists is refused, so bump the version instead of overwriting.

A shipped game runs the launcher (`<game>`) from its bundle root; it never
needs cargo, Rust, Studio or a Roblox account. See
[game/docs/BUNDLE.md](BUNDLE.md).

## Verify a release

```sh
sha256sum -c dist/<game>-<version>-<os>-<arch>.tar.gz.sha256
tar -tzf dist/<game>-<version>-<os>-<arch>.tar.gz | head
# Unpack, then run the launcher with networking off (F2/FORK-PLAN §14.3):
unshare -n ./<game>
scripts/offline-smoke.sh          # the Phase 0 fixture's equivalent check
```

The pack itself is verified by `rustudio-gamepack verify <pack>` (see
[game/docs/PACK.md](PACK.md)); `release.json` hashes every file in the
bundle, the pack's `content-pack.json` included.

## CI

`.github/workflows/game.yml` runs on every push and pull request:

- `downstream`: `cargo test --manifest-path game/Cargo.toml` and the release
  build of the launcher/cook/gamepack on Linux, Windows and macOS.
- `phase0-smoke`: the Phase 0 bundle and its headless offline checks on
  Linux with software Vulkan (lavapipe), so the gate does not need a GPU.

The upstream `ci.yml` keeps testing the editor. A local pass is still the
release gate: `cargo test --workspace`, `cargo test --manifest-path
game/Cargo.toml`, `scripts/phase0-smoke.sh`, `scripts/pack-smoke.sh`, and a
visual check of the game window with the user.
