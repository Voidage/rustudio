# rustudio fork — downstream code

This folder is the fork's own code. It is a separate Cargo workspace so the
upstream root `Cargo.toml` stays untouched (FORK-PLAN §3, option A). The
launcher has no dependency on any `rustudio-*` crate, so upstream engine
changes cannot break it.

```
game/
  Cargo.toml          downstream workspace (members: launcher, cook, pack)
  launcher/           spawn-mode launcher: reads game.json, sets the env
                      contract, runs engine/rustudio-player
  cook/               developer tool: scans a place's content ids, captures
                      what Play requests, writes the bundle cache +
                      assets.json (Phase 1)
  pack/               gamepack: build/update/verify/publish the content pack
  content/            the content pack source: PlayerModule, loader, and
                      (from the Studio install) the R6/R15 bodies + Animate
  fixtures/phase0/    the Phase 0 fixture (place, assets, manifest) and its
                      generator
  fixtures/pack/      the content-pack smoke probe
  fixtures/f7/        the direct KeyframeSequence acceptance probe
  docs/BUNDLE.md      bundle format + manifest + environment contract
  docs/COOK.md        the cook tool: usage, report format, statuses
  docs/PACK.md        the content pack: layout, PlayerModule, gamepack
  docs/RELEASE.md     releases, scripts/release-bundle.sh, CI
  docs/PHASE0.md      the Phase 0 spike: what ran, what it found
  build/              assembled bundles (generated; git-ignored)
```

Everything here is downstream-only. Anything generally useful belongs in an
upstream PR first (FORK-PLAN §2).

## Build, run, test (Phase 0)

```sh
# Build the unmodified player + launcher and assemble the bundle.
scripts/phase0-bundle.sh

# Play it. Resolves the bundle from its own path, applies the env contract,
# sets the window to 1280x720, and logs to logs/latest.log.
game/build/phase0-game/phase0-game

# Headless self-checks (R15, R6, and a clean-HOME/no-Studio fallback run).
scripts/phase0-smoke.sh
```

The fixture has a baseplate, a wall with a texture, a looping sound, and a
`KeyframeSequence` in `ReplicatedStorage/Animations/Wave` that plays on the
spawned character with the register-then-`LoadAnimation` form (the direct
`LoadAnimation(kfs)` form is F7). `scripts/phase0-smoke.sh` asserts on the
`PROBE` lines the fixture's LocalScript prints.

## Cook a place (Phase 1)

```sh
cargo build --release --manifest-path game/Cargo.toml
game/target/release/rustudio-cook --place my_place.rbxlx \
    --out game/build/my-game --content game/content \
    --player target/release/rustudio-player
```

See [docs/COOK.md](docs/COOK.md): what it scans, the playthrough capture,
`assets.json`, `--signed-in` / `--local-assets`, and `export` ("Save assets
for offline use") for what the person captured while playing or editing.

## Build the content pack and test it

```sh
scripts/fetch-studio-avatar.sh        # R6/R15 bodies + Animate from Studio
game/target/release/rustudio-gamepack build game/content \
    --out game/build/content-pack --version 0.1.0
scripts/pack-smoke.sh                 # R15; --r6 for R6
```

See [docs/PACK.md](docs/PACK.md): the layout the engine reads, the
PlayerModule's camera/controls, and gamepack's manifest.

## Release a game

```sh
scripts/release-bundle.sh --bundle game/build/my-game --version 1.0.0 \
    --pack game/content --out dist
```

See [docs/RELEASE.md](docs/RELEASE.md): the version scheme, what goes into
the archive, `release.json`, checksums, and `scripts/offline-smoke.sh`.

## Downstream unit tests

```sh
cargo test --manifest-path game/Cargo.toml
```
