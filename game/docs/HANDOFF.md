# Handoff — start here (Phase 2 in progress)

Read this first, then `docs/FORK-PLAN.md` §2 (maintenance rules), §12
(Phase 1) and `game/docs/PHASE0.md`. The plan is the source of truth; this
page is the state of the world.

## What we're building

A downstream fork of rustudio (Roblox-style local-first engine/editor):
self-contained game bundles that run offline with no account and no Roblox
Studio install, plus direct KeyframeSequence playback. Upstream is under
active development, so every change must stay cheap to rebase.

## Current state (2026-11-27)

Phase 0 and Phase 1 are complete (pack v1.1 is deferred until distribution).
Phase 2 so far:

- **Direct-clip replication (F7's second half) — done.** `clip_by_path` used
  `string.match(id, "^rbxtemp://(kfs|clip)/...")`; Lua patterns have no
  alternation, so it matched the literal `kfs|clip` and never resolved a
  path id outside the VM that registered it. Fixed in `library.luau`, with
  the two-VM acceptance test `direct_clip_track_replicates_by_path_id` and
  the deliberate parentless-clip case in `replicate.rs`.
- **F5 local DataStores — implemented; run the full gate before shipping.**
  The local backend persists records under `<bundle>/datastores`
  (`RUSTUDIO_DATASTORE_DIR`, set by the launcher) with atomic writes and a
  schema version. Game scripts still use the ordinary `DataStoreService`
  API; there is no separate save API. The directory also enables DataStores
  for a local `PlaceId 0` game, which the editor rule otherwise refuses.
  Test: `datastores_survive_a_restart`.
- **F1 window title — done (icon optional).** The launcher sets
  `RUSTUDIO_WINDOW_TITLE` from `window.title` (else the game name); the
  player uses it as the window title, falling back to `rustudio-player`.
- **F3 clean engine content root — done.** The launcher sets one variable,
  `RUSTUDIO_ENGINE_CONTENT_DIR=<pack>`; `assets::content_dir` and
  `default_scripts::studio_dir` validate the pack's `content/` and
  `ExtraContent/scripts/PlayerScripts` halves separately. The Studio-layout
  variables remain for development and the zero-patch smoke test.
- **Asset Manager per-place view and local imports (F4) — done.** Asset
  Manager has a **This Place** scope: the open place's captured assets from
  the capture ledger (copy `rbxassetid://…`) and the files in its own
  `assets/` folder (copy the relative reference, `assets/<name>`), plus
  **Export for Offline Use**. Bulk Import's **Import Locally into This
  Place** copies images, sounds and meshes into the place folder's `assets/`
  and writes `.rustudio-local-content` so the engine resolves them; models
  are inserted. No upload is involved. The captured list also caught the
  once-per-line JSON ledger writes being interleaved by concurrent
  captures — fixed with one locked write per record.
- **F9 game-scoped paths — done.** The launcher sets
  `RUSTUDIO_HOME=<bundle>/home`; `paths::data/config/state/cache/home` all
  resolve under it, so a game cannot read the person's settings, Studio
  content links, caches or crash reports. An empty value opts back into the
  person's folders. Fork-only: upstream has no bundle use case, so no PR is
  planned.
- **Content pack download mode — done.** `game.json` can say
  `"content": {"mode":"download","url":...,"sha256":...,"version":...}`;
  the launcher reuses an installed pack, else fetches the zip
  `gamepack publish`/`archive` writes, verifies the pack hash and the
  per-file contents, and installs it at
  `<per-user data>/<game-id>/content-pack/<version>/`
  (`RUSTUDIO_GAME_DATA_DIR` overrides). Later launches never touch the
  archive. Network/hash/version failures and `offline: true` fall back to
  engine content without blocking the launch. `file://` URLs work for
  local testing. Bundled mode is unchanged and stays the default;
  `scripts/pack-download-smoke.sh` proves install, reuse with the archive
  removed, and bad-hash fallback.
- **Network policy — done, opt-out only.** Network is allowed by default:
  a shipped game that calls the internet keeps working, and the launcher
  sets `RUSTUDIO_HTTP_ENABLED=1` so HttpService is enabled (the editor has
  no Experience Settings > "Allow HTTP requests" toggle yet — a parity
  gap). `"network": false` in `game.json` sets `RUSTUDIO_NO_NETWORK=1`;
  HttpService and `CreateWebStreamClient` then fail before opening a
  connection and log one line per service/request, and asset downloads
  behave as offline. `offline: true` stays about game assets and does not
  cut the network.
- **U3 (not Phase 2):** attachment selection, gizmos and property frames
  landed as a separate upstream-bound commit.

Phase 2 remaining (suggested order):

1. Packaging, signing/notarization as distribution requires.

Skipped by decision (2026-11-27): Studio UI actions for the content pack.
The `gamepack` CLI is enough; the editor should not put pack management in
a game developer's way.

Cache management is out of scope: the download cache is never pruned.

Phase 1 (complete):

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
  There is no "keep downloaded assets" toggle or cache UI, and none is
  planned: the download cache is never pruned (§8.4 item 2). The
  asset-manager local-copy idea (§18.19) is **done** in the small downstream
  patch described under "Current state": This Place lists the ledger and the
  place's `assets/` files, and Bulk Import can copy files there without an
  upload. "Export for Offline Use" writes beside the place, outside the
  download cache.

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

Phase 1 remaining (deferred):

1. **Pack v1.1:** our own Animate with in-place KeyframeSequences (no cloud
   clip ids, so a cook is not needed for the default animations). v1 uses
   Studio's Animate and the cook's one online capture.
   **Deferred until distribution (2026-11-26):** v1 already delivers
   download-once-then-offline (warm the bundle, then every run is local);
   v1.1 removes the one-time session, the redistribution of Roblox's
   animation bytes, and the dependence on Roblox's ids — all of which
   matter when shipping to someone else, not for local use. Reasoning and
   the reduced-clip intermediate option are in `game/docs/PACK.md`.

## Git state

- Base: **`ade1ff5`** — upstream `main` after PR #46 merged, `Cargo.toml`
  0.6.0.
- `game` branch (local only, not pushed) is a linear stack on top of it:
  the Phase 0/cook/content-pack F0 commits, F2, F4, F7 (including its
  replication fix), F5, U2, U3, and the Phase 1/2 docs/handoff commits.
  U1 is gone (merged upstream).
- **U2** (upstream bug, 2026-11-26): a saved dock layout whose leaf names
  a tab index past the end (a tab was removed without clamping `active`)
  draws an empty panel — the 3D viewport went blank in every build that
  read the same `layout.json`, while `--self-test` (clean layout) passed
  `viewport drawn: true`. `retain_tabs` now clamps, and
  `DockState::sanitize` repairs a saved layout on load. Upstreamable; PR
  not sent yet.
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
- No virtual machines. Test locally; the user handles visual checks.
- One subagent at a time, when parallel work genuinely helps.
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
