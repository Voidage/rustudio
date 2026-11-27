# Handoff — start here (Phase 1 in progress)

Read this first, then `docs/FORK-PLAN.md` §2 (maintenance rules), §12
(Phase 1) and `game/docs/PHASE0.md`. The plan is the source of truth; this
page is the state of the world.

## What we're building

A downstream fork of rustudio (Roblox-style local-first engine/editor):
self-contained game bundles that run offline with no account and no Roblox
Studio install, plus direct KeyframeSequence playback. Upstream is under
active development, so every change must stay cheap to rebase.

## Current state (2026-11-26)

Phase 0 is complete and visually confirmed by the user.

Phase 1 so far:

- **Sync done (twice).** Base moved from 0.5.2 (`3f1ada8`) to 0.6.0
  (`7c2ce14`, upstream main with the Animation Editor, script auto-indent,
  batch asset uploader), then to `ade1ff5` when **PR #46 (U1, sound
  autoplay) merged upstream** — the local U1 patch was dropped by the
  rebase. `range-diff` shows every remaining fork commit identical.
  Workspace tests and the three Phase 0 smoke runs pass on the new base.
  **F2 is upstream PR #47** (open).
- **Cook tool** (`game/cook`, downstream): static scan of content
  properties (reflection DB + the asset_report name list) and script
  `Source`; a clean-cache headless playthrough capture; fetch of missed ids
  (public/session) or import from `--local-assets`; relative local files
  copied into `place/` with `.rustudio-local-content`; `assets.json` with
  status, size, sha256, cache path and referrers; exits 1 on missing or
  unsupported content. 10 unit tests including an offline end-to-end cook;
  it cooks the Phase 0 fixture. Docs: `game/docs/COOK.md`.
- **F2 offline mode.** `RUSTUDIO_OFFLINE=1` makes the asset layer serve
  the cache, the upload ledger and the local store only; misses return
  `Unavailable` immediately, and `wait_for_session` gives up at once. The
  launcher sets it from `game.json`'s `offline` flag (the fixture says
  true, so the smoke exercises it). Unit test in `assets.rs`.
- **F7 direct clip playback.** `Humanoid`/`Animator`/`AnimationController`
  `LoadAnimation` accept a `KeyframeSequence` or `AnimationClip` directly:
  a parentless `Animation` wrapper with a `rbxtemp://` id reuses the
  existing track pipeline unchanged. Parented clips get a stable path id
  (`rbxtemp://kfs/ReplicatedStorage.Animations.Walk`) for replication;
  parentless clips use the registry counter. `KeyframeSequence` is an
  `AnimationClip` subclass, so the keyframe parser wins over the curve one.
  Test covers Length/Looped/wrapper/Motor6D.Transform/Humanoid/parentless/
  CurveAnimation; the rest of the anim suite still passes.
  **Accepted on a real Studio-authored place** (user-made, 2026-11-26,
  `ANIMTESTPLACE.rbxl` at the repo root, not committed):
  `Workspace.ANIMATION` (length 1, loop true) played on the R15 avatar as
  `rbxtemp://kfs/Workspace.ANIMATION`; the rig has 15
  `AnimationConstraint`s (0.6's upgraded joints), 12 moved and 20 distinct
  poses were sampled. The reproducible probe is
  `game/fixtures/f7/acceptance.luau` (run with
  `--luau client game/fixtures/f7/acceptance.luau@5`; see its README).

- **Content pack v1 + `gamepack`.** `game/content` has our
  `PlayerScriptsLoader`, the PlayerModule (`GetControls`/`GetCameras`), a
  third-person CameraModule and a ControlModule (WASD/arrows ->
  `Humanoid:Move`, Space jump). R6/R15 bodies and Animate come from
  `scripts/fetch-studio-avatar.sh` (local Studio files, git-ignored; the
  licensing note is in `game/docs/PACK.md`). `gamepack` builds a
  deterministic, versioned, hashed pack (`build`/`update`/`verify`/
  `publish`); `scripts/pack-smoke.sh [--r6]` passes for both rigs. The
  user's offline playtest confirmed walking, Animate, right-drag rotation,
  wheel zoom, jump and a visible head. Two engine facts are worked around
  in the module: `IsMouseButtonPressed` is a stub (events instead), and
  raw mouse deltas need `MouseBehavior = LockCenter`. The R15 fallback
  head's transparent face texture is cleared on the client (the renderer
  hides the head otherwise) — see `game/docs/PACK.md`.

- **F4 capture ledger + "Save assets for offline use".**
  `crates/rustudio-core/src/assets.rs` appends one JSON line per settled
  cloud request to `<cache>/capture-ledger.jsonl` (`cached` with size and
  cache path, or `failed`), tagged with the place from
  `RUSTUDIO_ASSET_PLACE`; the launcher sets it from the bundle entry, and
  the player/editor set it when a place opens.
  `rustudio-cook export --place FILE --out DIR` copies a place's ledger
  entries into `<out>/cache/assets` and writes `offline-assets.json` with
  sizes and sha256, reporting `missing` and `failed` ids. Unit-tested in
  both crates; docs in `game/docs/COOK.md`.
  There is no "keep downloaded assets" toggle: the cache has no eviction,
  so retention is already the behaviour; the toggle belongs with the Phase 2
  Asset Manager UI (§8.4 item 2). The asset-manager local-copy idea (§18.19)
  is **not done** either: it patches the upstream upload UI, which §18.19
  itself rates as not worth the rebase surface; the ledger + export cover
  the offline goal.

- **Launcher, CI and release (Phase 1 item 3).**
  The launcher's log now starts with the game's name, version and id; a
  failure before the player starts is written to `logs/latest.log` and shown
  to the person (a native message box on windowless Windows, stderr
  elsewhere). `.github/workflows/game.yml` tests the downstream workspace on
  Linux/Windows/macOS and runs the Phase 0 offline bundle on Linux with
  software Vulkan. `scripts/offline-smoke.sh` runs the Phase 0 smoke inside
  `unshare -n` (or complains), and `scripts/release-bundle.sh` builds a
  versioned, immutable `.tar.gz` with `release.json` (engine version,
  upstream base, per-file sha256) and its `.sha256`. Docs:
  `game/docs/RELEASE.md`.

Not started (Phase 1 remaining):

1. **Pack v1.1:** our own Animate with in-place KeyframeSequences (no cloud
   clip ids, so a cook is not needed for the default animations). v1 uses
   Studio's Animate and the cook's one online capture.

## Git state

- Base: **`ade1ff5`** — upstream `main` after PR #46 merged, `Cargo.toml`
  0.6.0.
- `game` branch (local only, not pushed) is a linear stack on top of it:
  the Phase 0/cook F0 commits, F2, F7, and the Phase 1 docs/handoff
  commits. U1 is gone (merged upstream).
- Tags: `pre-sync-20261126` (before the 0.6.0 sync) and
  `pre-sync-20261126b` (before the U1-merge sync).
- `../upstream-u1` / `u1-autoplay-sounds` is merged PR #46 (can be
  cleaned up). `../upstream-f2` / `f2-offline-assets` is PR #47's branch.
- Remotes: `upstream` → rippelz/rustudio, `fork` → Voidage/rustudio.

## Verify before you change anything

```sh
cargo test --workspace                       # upstream suite (incl. F2/F7)
cargo test --manifest-path game/Cargo.toml   # launcher + cook + gamepack
scripts/phase0-bundle.sh                     # player + bundle
scripts/phase0-smoke.sh                      # 3 headless checks (offline)
scripts/offline-smoke.sh                     # the same inside unshare -n
scripts/pack-smoke.sh                        # pack: R15; add --r6
scripts/release-bundle.sh --help             # versioned release archive
game/build/phase0-game/phase0-game           # visual check — ask the user
game/build/pack-smoke/pack-smoke             # pack visual check — ask
```

## Rules and user preferences (important)

- **No screenshots and no mouse/keyboard control.** The user is at the
  computer. Verify with Luau probes, Output logs, `--result`, strace; for
  visual checks, launch the game and ask the user what they see.
- Upstream-first: fixes and generic features go upstream as small PRs
  (FORK-PLAN §2). Downstream code lives in `game/`, `scripts/`.
- Patch budget: small, single-purpose commits tagged `[fork] F<n>:` (or
  `U<n>` for upstream fixes); §11 inventory must match the log.
- The user is new to git: explain branch/remote actions plainly, and
  confirm before any push or public change.
- The user can create a real R15 place with a Studio-authored
  KeyframeSequence on request (offered 2026-11-26): use it as the F7
  acceptance test when available.
- Player tests can play the classic "oof" sound locally — that is normal.
- `game/build/` and `game/target/` are git-ignored build output.
- The game workspace patches `rbx_dom_weak` to `vendor/rbx_dom_weak`
  because `rustudio-core` calls methods the crates.io release lacks.
