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

- **Sync done.** Base moved from 0.5.2 (`3f1ada8`) to 0.6.0 (`7c2ce14`,
  upstream main with the Animation Editor, script auto-indent, batch asset
  uploader). The five fork commits re-applied cleanly; `range-diff` shows
  them identical. Workspace tests and the three Phase 0 smoke runs pass on
  the new base. U1 (sound autoplay) stays: upstream PR **#46 is still
  open**.
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
  poses were sampled. The probe used was
  `--luau client f7_acceptance.luau@5` (the harness runs a client script
  without touching the place).

Not started (Phase 1 remaining):

1. **Content pack v1 + `gamepack`.** The exact files the engine reads are
   in `crates/rustudio-core/src/default_scripts.rs`:
   - `ExtraContent/scripts/PlayerScripts/StarterPlayerScripts/PlayerScriptsLoader.lua`
   - `ExtraContent/scripts/PlayerScripts/StarterPlayerScripts/PlayerModule.module.lua`
     plus `PlayerModule.module/<Module>.lua` (`module_children`: a `.lua`
     becomes a ModuleScript, a subfolder a Folder with the same rules)
   - `ExtraContent/scripts/PlayerScripts/StarterPlayerScriptsCommon/RbxCharacterSounds.lua`
     and `RbxCharacterSounds/AtomicBinding.lua`
   - `content/avatar/scripts/humanoidAnimateR15_DEPRECATED_20260813.rbxm`
     (R15), `humanoidAnimatePlayEmote.rbxm` (R6),
     `humanoidHealthRegenScript.rbxmx`
   - `content/avatar/character.rbxm` and the head models are optional
     (blocky fallback otherwise).
   The pack root is `RUSTUDIO_STUDIO_DIR` (validated by
   `ExtraContent/scripts/PlayerScripts`); `RUSTUDIO_CONTENT_DIR` is its
   `content/`. If the loader file is missing the engine installs its own
   `PLAYER_SCRIPTS_LOADER`; with no module at all it has a keyboard
   stand-in for movement but no module camera/controls. There are no
   Roblox PlayerModule sources in the repo (they normally come from a
   Studio install), so v1 needs our own module, or a licence-approved
   source. `gamepack build/update/publish` is then a small CLI.
2. **F4 capture ledger + "Save assets for offline use"** (§8.4), including
   the asset-manager local-copy idea in §18.19 (upstream 0.6's uploader
   copies imported/uploaded files into the project's `assets/<id>.<ext>` +
   manifest; the cook consumes that folder; publishing reuses
   `rustudio_account`'s upload call).
3. **Launcher logging/error UI, CI, release script and docs** (§14/§15).

## Git state

- Base: **`7c2ce14`** — upstream `main` at sync time, `Cargo.toml` 0.6.0.
- `game` branch (local only, not pushed) is a linear stack on top of it:
  U1, the Phase 0 F0 commits, the cook tool, F2, F7, and the Phase 1
  docs/handoff commit.
- Tag `pre-sync-20261126` is the state before the 0.6.0 sync.
- `u1-autoplay-sounds` lives in `../upstream-u1`; it is PR #46's head.
- Remotes: `upstream` → rippelz/rustudio, `fork` → Voidage/rustudio.

## Verify before you change anything

```sh
cargo test --workspace                       # upstream suite (incl. F2/F7)
cargo test --manifest-path game/Cargo.toml   # launcher + cook
scripts/phase0-bundle.sh                     # player + bundle
scripts/phase0-smoke.sh                      # 3 headless checks (offline)
game/build/phase0-game/phase0-game           # visual check — ask the user
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
