# Fork plan: offline, self-contained game runtime

Status: draft
Base reviewed: rustudio 0.5.2 (`Cargo.toml`), upstream main at `3f1ada8`, 2026-11-26
Upstream: `rippelz/rustudio`
Scope: downstream fork only. This file is not part of upstream.

The goal is a fork that can:

- build places and create animations locally (editor, FBX import, or scripts),
- ship a game as a self-contained bundle,
- launch that bundle with no account, no Roblox Studio install, and no
  runtime downloads,
- play an animation saved in the place directly, with no upload and no asset
  id.

Product framing: a **Roblox-style, local-first engine/editor you own** — the
authoring and playing loop without the hosted platform. It is not a
general-purpose replacement for Unity or Unreal.

The short version: the engine already does the hard part. The fork should be
a **small launcher + bundle format + asset cook tool + default content pack +
local clip playback**, with a very small patch stack against upstream.
Everything generic enough to be useful upstream should be proposed upstream
first and dropped from the fork when it lands.

---

## 1. What already works in the engine

These are reasons the fork can stay thin, not work items:

| Capability | Where |
|---|---|
| Standalone player, opens a place and auto-starts Play | `crates/rustudio-player/src/player_app.rs` (`start_play: true`, start at frame 2) |
| Local place formats `.rbxl` / `.rbxlx` / `.rbxm` / `.rbxmx` | `crates/rustudio-core/src/doc.rs` |
| Server + client DataModels in one process, replication | `crates/rustudio-player/src/play.rs` |
| Renderer, physics, Luau, GUI, audio, terrain | `rustudio-render`, `rustudio-physics`, `rustudio-engine` |
| R15/R6 rigs, Motor6D posing, Animator / AnimationTrack | `crates/rustudio-engine/src/rig.rs`, `domains/anim*.rs` |
| Animations from registered KeyframeSequences (no asset fetch) | `crates/rustudio-engine/src/library.luau` (`RegisterKeyframeSequence`, `load_sequence`) |
| Animations from local asset bytes | same, via `public_prefetch` + `__rs_anim_asset` |
| In-place KeyframeSequences playable directly | design in §7.2 (fork extension) |
| Animation authoring UI | optional; upstream work in progress. Playback does not depend on it. |
| Downloaded assets persist to disk automatically (images, meshes, sounds, animations, models) | `assets::cache_path` / `write_cache`; the cache is checked first on later loads |
| Local asset store (`<id>.<ext>`) | `RUSTUDIO_LOCAL_ASSETS` |
| `rbxasset://` content root | `RUSTUDIO_CONTENT_DIR` |
| Runs signed out; local player is “Player” | `RUSTUDIO_NO_SESSION=1`, `mech::PLAYER_NAME` |
| Native fallback body when no Studio content | `rustudio-engine/src/rig.rs` (`build_default`) |
| DataStores local/in-memory by default | `rustudio-engine/src/domains/data.rs` |
| Local visual harness (`--play`, `--shots`, `--luau`, `--screenshot`) | `docs/DEVELOPING.md` |

The gaps are product gaps, not engine gaps: packaging, default controls and
animation for a self-contained content pack, animation authoring tools
(upstream and optional), direct KeyframeSequence playback (fork, independent),
offline-first asset lookup, per-place asset capture/export, and persistence.

---

## 2. Maintenance rules (the important part)

Upstream is moving. Treat every change as debt.

1. **Upstream-first.** If a change is generally useful, open an upstream PR
   first. If it lands, drop the local patch during the next sync.
   Candidates: offline asset mode, explicit engine content root, asset
   manifest export, local DataStore backend, player entry point.
2. **Zero-patch shell by default.** The launcher is a separate, tiny binary
   with no upstream Rust dependencies. It sets environment variables and
   spawns the unmodified `rustudio-player`. This is the MVP and the default.
3. **Downstream code lives in downstream paths.** `game/`, `patches/`,
   `scripts/`. Do not bury fork logic inside upstream crates.
4. **Patch budget: target six upstream-touching patches; hard ceiling
   eight.** Each patch does one thing, touches one file where possible,
   stays under ~100 lines, and compiles/tests on its own. Animation work
   (F7) is the newest patch and lives in upstream-active code, so it stays
   deliberately thin.
5. **No vendoring, no mass reformatting, no moving upstream files.** Those
   create merge pain disproportionate to their value.
6. **Configure, don't code.** Prefer env vars and settings over edits.
7. **Rebase, don't merge.** Keep `game` as a linear stack of fork commits on
   top of upstream. Never merge upstream into it.
8. **Every fork commit is buildable and tagged `[fork] F<n>:` in the subject.**
   The body says why, what it touches, and whether it is upstreamable.
9. **Test gate after every sync:** full workspace tests plus the offline
   smoke test. A patch that no longer applies semantically is rewritten, not
   mechanically resolved.

Patch inventory is tracked in §11 and must match
`git log --oneline upstream/main..game`.

---

## 3. Repository layout

```
game/
  Cargo.toml              # downstream workspace (optional nested workspace)
  launcher/               # tiny launcher: no upstream deps, spawns player
  cook/                   # asset collector; may depend on upstream crates
  pack/                   # content pack builder/updater (developer CLI)
  content/                # source files for the engine content pack
  docs/                   # downstream docs
patches/                  # exported patch stack (generated for review)
scripts/
  sync-upstream.sh
  offline-smoke.sh
  release-bundle.sh
```

Ideally the root `Cargo.toml` is untouched. Two options:

- **A (preferred):** `game/` is its own Cargo workspace with `[workspace]` in
  `game/Cargo.toml`; cook has path dependencies into `../crates/...`.
  Validate early in Phase 0.
- **B (fallback):** add one member line to the root workspace. This is a
  one-line recurring conflict at worst and is acceptable.

The launcher should not depend on upstream crates in either option. That keeps
it immune to engine API changes.

---

## 4. Bundle format

One folder, portable, writable in the MVP:

```
MyGame/
  MyGame(.exe)            # launcher binary (copy per game)
  game.json               # manifest
  engine/
    rustudio-player(.exe) # unmodified upstream build (spawn mode)
  place/
    game.rbxl             # entry place
  cache/
    assets/               # <id>_v<version>.bin or <id>_vlatest.bin
  assets/                 # optional local store: <id>.<ext>
  content/                # optional: bundled engine content pack
    content/...
    ExtraContent/scripts/PlayerScripts/...
    PlatformContent/...
  logs/                   # created at runtime
  datastores/             # local DataStore records (F5)
  home/                   # private settings/caches/crash reports (F9)
```

`game.json`:

```json
{
  "id": "mygame",
  "name": "My Game",
  "version": "1.0.0",
  "engine": "rustudio 0.5.2 + fork.1",
  "entry": "place/game.rbxl",
  "cache": "cache",
  "assets": "assets",
  "content": {
    "mode": "bundled",
    "path": "content",
    "version": "1.0.0"
  },
  "window": { "title": "My Game", "width": 1600, "height": 900 },
  "args": ["--window", "1600x900"],
  "offline": true
}
```

- All paths are relative to the bundle root.
- The launcher resolves the bundle from its own executable path, not the
  current directory.
- Keep the manifest small and versioned (`manifest_version: 1`).
- `args` is the escape hatch for player harness options (`--window`,
  `--fixed-dt`, …) without changing the launcher.
- `content.mode` is `"bundled"`, `"download"`, or `"engine"` (development).
  A plain string (`"content"`) is shorthand for a bundled pack. A downloaded
  pack lives under the game's per-user data directory, not in the bundle.
- `cache` / `assets` are **game assets**. The content pack is **engine
  defaults**. They are separate; see §7.1 and §8.4.

**Writable vs read-only installs.** The MVP assumes the bundle folder is
writable (portable zip/tar). If a game is installed read-only (e.g. Program
Files), either copy the bundled cache to a per-user cache on first run or
adopt F2 + a read-only local store. This is an open decision (§18). A
downloaded content pack and local DataStore records naturally live in the
per-user game data directory, so download mode works with a read-only
bundle.

---

## 5. Launch path

### 5.1 Spawn mode (MVP, no upstream changes)

The launcher:

1. Finds `game.json` beside itself.
2. Loads and validates the manifest.
3. Builds the environment contract (§6), without overwriting variables the
   developer set explicitly (debugging escape hatch).
4. Sets the working directory to the bundle root.
5. Opens `logs/latest.log` and wires the child's stdout/stderr to it.
6. Spawns `engine/rustudio-player` with the entry place and manifest args.
7. Waits, propagates the exit code, and writes a final log line.

Why this is the right default:

- The launcher has no dependency on `rustudio-player` as a library, so
  upstream API changes do not touch it.
- The player already auto-starts Play and already accepts a place path.
- It can be replaced by a single binary later without changing the bundle
  format.

Trade-offs: two binaries. F1's `RUSTUDIO_WINDOW_TITLE` (set from the
manifest) gives each game its own title without in-process mode.

On Windows the launcher should use `windows_subsystem = "windows"` so spawning
does not flash a console. The player already has that attribute.

### 5.2 In-process mode (F1, optional)

If a single executable or per-game window identity matters:

- Add `pub fn run_with_args(argv: Vec<String>)` to `player_app.rs`; keep
  `run()` as a wrapper around `std::env::args()`.
- Add `RUSTUDIO_WINDOW_TITLE` (and optionally an icon path) where the
  viewport is built.
- The launcher depends on `rustudio-player` and calls `run_with_args`.

This is a small patch but couples the launcher to upstream. Do it only after
the spawn MVP is proven.

---

## 6. Environment contract

The launcher sets these. Values are chosen for a shipped game, not for
development.

| Variable | Value | Why |
|---|---|---|
| `RUSTUDIO_NO_SESSION` | `1` | Skip account restore/keyring/avatar/session check. |
| `RUSTUDIO_NO_STUDIO_FETCH` | `1` | Stop the classic death-sound fetch (and any Studio download path). |
| `RUSTUDIO_CORE_SCRIPTS` | `0` | Use the native fallback UI instead of Studio CoreScripts. |
| `RUSTUDIO_LIVE_DATA` | `0` | Keep DataStores/MemoryStores/Messaging local. |
| `RUSTUDIO_CACHE_DIR` | `<bundle>/cache` | Bundled, first-priority disk asset cache. |
| `RUSTUDIO_DATASTORE_DIR` | `<bundle>/datastores` | F5: the local DataStore backend's files. Also selects local DataStores for a local place. |
| `RUSTUDIO_HOME` | `<bundle>/home` | F9: scopes settings, game settings, caches and crash reports to the game. Empty means the person's folders. |
| `RUSTUDIO_LOCAL_ASSETS` | `<bundle>/assets` | Optional secondary local store. |
| `RUSTUDIO_ENGINE_CONTENT_DIR` | resolved pack root | F3: the game content root. The engine validates `content/` and `ExtraContent/scripts/PlayerScripts` separately. |
| `RUSTUDIO_STUDIO_DIR` | a Studio root | Development only; still supported after F3. |
| `RUSTUDIO_CONTENT_DIR` | a `rbxasset://` root | Development only; still supported after F3. |
| `RUSTUDIO_OFFLINE` | `1` | After F2: no background downloads, local store first. |
| `RUSTUDIO_NO_NETWORK` | `1` only when `network: false` | §9: HttpService and web streams fail fast with one log line. The default allows the web. |
| `RUSTUDIO_HTTP_ENABLED` | `1` | §9: a game bundle runs with HttpService enabled (no Experience Settings UI yet); `network: false` blocks the requests at the gate. |
| `RUSTUDIO_WINDOW_TITLE` | `window.title`, else the game name | F1: the player's window title. |
| `RUSTUDIO_ALLOW_LOCAL_CONTENT` | unset | Prefer the `.rustudio-local-content` marker when a game needs relative files. |
| `RUSTUDIO_ASSET_REPORT` | test only | Writes the image/mesh audit JSON. |

Notes:

- The launcher resolves the content pack first (§7.1), then sets these. If no
  pack resolved, leave them unset; the engine's blocky fallback and
  missing-script warnings take over. Never point them at a missing directory.
- `RUSTUDIO_ENGINE_CONTENT_DIR` is the pack root; the engine uses its
  `content/` half for `rbxasset://` and its
  `ExtraContent/scripts/PlayerScripts` half for default scripts, validating
  each half on its own. A pack missing one half falls back for that half
  only. `RUSTUDIO_STUDIO_DIR` (validated the same way) and
  `RUSTUDIO_CONTENT_DIR` remain for development and for the zero-patch
  smoke test.
- A downloaded pack is installed under the game's per-user data directory
  (for example `<data>/<game-id>/content-pack/<version>/`) and reused.
- F9 sets `RUSTUDIO_HOME=<bundle>/home`, so settings, game settings, caches
  and crash reports live under the game folder and cannot read the person's
  rustudio config. `paths::home()` is scoped too, so Studio/Vinegar lookups
  and profile folders stay hermetic; set `RUSTUDIO_HOME` empty to opt back
  into the person's folders.
- `RUSTUDIO_CACHE_DIR` also holds the pipeline cache. A writable cache is
  required for the MVP.
- Crash reports go to `<bundle>/home/cache/rustudio/crash` (F9); launcher
  logs cover the common case. Copying them into `logs/` is still a polish
  item.

---

## 7. Default content and animation

### 7.1 Default content pack

Upstream reads its defaults from a Studio-style root. The fork supplies its
own root so the game never depends on a Studio install.

#### Delivery modes

The pack can arrive in three ways; the manifest says which:

1. **Bundled** (default for an offline release): `content/` ships in the
   bundle. This is the only mode that is fully offline on first launch.
2. **Downloaded once — done.** The launcher fetches a versioned pack from
   `content.url` (the zip `gamepack publish`/`archive` writes), checks the
   unpacked pack against `content.sha256`, installs it under
   `<per-user data>/<game-id>/content-pack/<version>/`
   (`RUSTUDIO_GAME_DATA_DIR` overrides the root) and reuses it later without
   the archive. Smaller download; the first launch needs network. Choose
   this when sharing one pack across many games matters more than an offline
   first launch. Any failure (network, hash, version) logs and falls back to
   engine content; the launch still happens, and `offline: true` forbids the
   fetch.
3. **Engine/local** (development only): reuse a content root already on the
   machine. Never required for a shipped game.

If none resolves, the game still starts with the engine's blocky fallback
and no default PlayerModule/Animate. It must never fail to launch because
the pack is missing.

The editor's existing `studio_content.rs` downloader targets Studio's own
content and is not the game pack path. The game pack manager is separate.

#### Contents

Required layout (names are upstream's expectations):

```
content/
  content/
    avatar/
      characterR15DynamicHeadV2.rbxm   # optional; blocky fallback otherwise
      character.rbxm                   # optional
      defaultDynamicHead.rbxm          # optional
      scripts/
        humanoidAnimateR15_DEPRECATED_20260813.rbxm
        humanoidAnimatePlayEmote.rbxm
        humanoidHealthRegenScript.rbxmx
    fonts/                             # optional; fallback fonts exist
  ExtraContent/
    scripts/PlayerScripts/             # must exist for RUSTUDIO_STUDIO_DIR
      StarterPlayerScripts/
        PlayerScriptsLoader.lua
        PlayerModule.module.lua
        PlayerModule.module/
          CameraModule.lua
          ControlModule.lua
          CommonUtils.lua
  PlatformContent/pc/                  # optional textures/fonts
```

Work items:

1. **PlayerModule equivalent.** Minimal loader plus CameraModule and
   ControlModule. It must expose the public surface games commonly use
   (`PlayerModule:GetControls()`, `GetCameras()`), and it must write
   `Humanoid.MoveDirection` and drive `Workspace.CurrentCamera`.
   Without it, a spawned character has no default movement or camera.
2. **Animate state machine.** Idle/walk/run/jump/swim/climb, using Humanoid
   state signals. Embed KeyframeSequences in the Animate model and register
   them at runtime, or point at IDs bundled in the cache. This avoids
   depending on any external animation asset.
3. **Rig.** Use Studio's R15/R6 body files as-is (the pack fetches them
   locally); the engine's blocky fallback covers a missing pack. No custom
   default rig is planned — a game that wants one ships its own, outside the
   pack.
4. **Fonts, materials, sounds.** Optional engine fallbacks. The default pack
   does not need to replace them; a game may add its own.
5. **Keep it small.** This pack is part of the game runtime and should be
   versioned. It can be bundled or downloaded once.

**Decision (2026-11-26): v1 ships; item 2's own clips wait for
distribution.** The asset cache already gives download-once-then-offline:
warm the bundle once (the cook, or F4's export after a play session), and
every later run reads only local files — `scripts/pack-smoke.sh` proves the
flow and the user's playtest confirmed it. Our own Animate with in-place
KeyframeSequences (v1.1) buys three things that matter when handing the game
to someone else: no signed-in session at cook time, no redistribution of
Roblox's animation bytes, and no dependence on Roblox keeping those ids. It
costs authoring the Animate state machine's clips. The reduced intermediate
(idle/walk/run/jump only, Studio's Animate as fallback) is in
`game/docs/PACK.md`.

Another development fallback: inject controller/animate scripts into the
place at cook time. That avoids the pack entirely, but the editor's own Play
mode then still needs a Studio install. Prefer a real pack for release; the
no-pack path must always launch.

#### First-launch resolution

1. `content.mode == "bundled"` and `content/` exists → use it.
2. A pack already installed for this game and version → use it.
3. `content.mode == "download"` → fetch, verify the hash, install, use.
4. `content.mode == "engine"` and a local content root is available → use it
   (development).
5. Otherwise → fallback; log what is missing; for download mode retry on the
   next launch.

A failed or corrupted download never blocks the game. A pack whose engine
range does not match the runtime is rejected and the next candidate is tried.

#### Updating the pack

A small developer tool (`gamepack`; pick Python, shell, or Rust) owns the
pack lifecycle:

- `gamepack build <source-dir>` → a versioned pack folder.
- `gamepack update` → bump the version, recompute `sha256`, write
  `content-pack.json`.
- `gamepack publish <target>` → copy the pack to the download location and
  write the `<version>.zip` download mode fetches (`gamepack archive`
  writes just the zip).

The runtime only consumes a versioned, hashed pack; it does not care which
tool built it. Later, the same actions can become Studio UI buttons
(“Build Content Pack”, “Update Content Pack”) with no runtime change.

#### Pack vs game assets

The pack is shared engine defaults: rig, PlayerModule, Animate, fonts,
materials. Game assets are the place's own content ids: sounds, images,
meshes, animation asset bytes, models. The latter are captured per place and
exported into the bundle's `cache/` (§8.4); they are not part of the pack.

F3 (done) makes the content root explicit with
`RUSTUDIO_ENGINE_CONTENT_DIR`; the launcher sets that one variable and the
engine validates the pack's two halves. The Studio-layout variables remain
for development.

### 7.2 KeyframeSequence playback (local, no upload)

Goal: **a KeyframeSequence is a playable animation.** It can live in the place
or be built at run time; there is no upload step, no asset id, and no second
copy in a cache. This is a first-class workflow, not a workaround.

The animation editor is only one possible producer of that KeyframeSequence.
Nothing in the playback path may depend on it. If the editor never ships,
this feature still works.

#### Where the KeyframeSequence comes from

Any of these is equivalent, and the runtime treats them identically:

1. The animation editor, when it is available: select a rig, create
   keyframes, save the result as a `KeyframeSequence` in the place — for
   example `ReplicatedStorage/Animations/Walk`.
2. FBX import: `Plugin:ImportFbxAnimation` already produces a
   KeyframeSequence in the place (`src/plugin_host/api_fbx_anim.rs`).
3. A script: `Instance.new("KeyframeSequence")`, then `AddKeyframe` /
   `AddPose`.
4. A saved model: load a `.rbxm` / `.rbxlx` containing a KeyframeSequence,
   or copy/paste one into the place.

The authoring tool is a producer, not a dependency. The only requirement is
that the KeyframeSequence ends up in the place (or another replicated
container, if clients must play it).

No upload, no waiting for an asset id, no copying an id into an `Animation`.

#### Desired playback API

The short form the fork should make true:

```lua
local track = humanoid:LoadAnimation(workspace.Animations.Walk)
track:Play()
```

`Humanoid:LoadAnimation`, `Animator:LoadAnimation` and
`AnimationController:LoadAnimation` should also accept a `KeyframeSequence`
(or any `AnimationClip`) directly. The engine wraps it in a normal
`AnimationTrack`, so `Length`, `Looped`, `Priority`, `Speed`,
`TimePosition`, `Stopped`, `KeyframeReached` and marker signals work
unchanged.

The old forms remain valid and unchanged:

```lua
-- Roblox-compatible: explicit registration, still useful for clips built
-- at run time in a script.
local anim = Instance.new("Animation")
anim.AnimationId = game:GetService("KeyframeSequenceProvider")
    :RegisterKeyframeSequence(kfs)
local track = humanoid:LoadAnimation(anim)

-- Bundled asset id, for imported or legacy content.
anim.AnimationId = "rbxassetid://123"
```

Implementation shape (F7):

- In `crates/rustudio-engine/src/library.luau`, extend `M.LoadAnimation` to
  accept a `KeyframeSequence` / `AnimationClip`. Wrap it in an internal
  `Animation` whose `AnimationId` is the existing provider registration
  (`rbxtemp://kfs/...` or `rbxtemp://clip/...`). This reuses the whole
  current track pipeline (`__rs_track_new`, `load_sequence`,
  `anim_tracks.rs`) and needs no second animation evaluator.
- Keep `track.Animation` non-nil (the wrapper), so scripts that read it do
  not break. Direct-clip tracks carry a local `rbxtemp://` id (or the
  path-based variant below) in `AnimationId`; code that needs a numeric
  asset id should use the explicit `Animation` + `AnimationId` path.
- Edit-mode preview is an editor convenience, not a dependency. The engine
  path is the same outside Play: put an Animator on the selected rig and step
  it with a small preview tick. A downstream action/plugin can offer “Play on
  selected rig” for any selected KeyframeSequence; the upstream animation
  editor can reuse that action later, but F7 does not wait for it.
- Do not fork the animation timeline UI. If/when upstream ships one, it is
  just another producer of the same KeyframeSequence.

#### Replication

A `rbxtemp://` id is local to one VM. Directly loaded clips therefore need a
stable reference when a server-side track replicates to clients. Both sides
hold the same place tree, so the simplest scheme is a **path-based clip id**:

- For a KeyframeSequence parented in the DataModel, use a stable id such as
  `rbxtemp://kfs/ReplicatedStorage.Animations.Walk` (or a `Guid` attribute
  on the clip) instead of a counter.
- `load_sequence` resolves that id by looking up the path/attribute before
  falling back to the local registry.
- The existing `anim_replicate.rs` keeps copying `AnimationId` strings; no
  wire-format change is needed.
- Parentless clips (built at run time in a script) keep the counter id and
  stay local-only unless the script itself sends them.

If path-based ids prove awkward, the fallback is to extend
`anim_replicate.rs` to replicate the clip reference itself. That is the
larger option and should be a separate patch (F8) only if needed.

#### Storage and cook

- Keep animations in the place: `ReplicatedStorage/Animations` is a good
  convention. They are then part of the place file and need no cache entry.
  A clip must be in a replicated container (`ReplicatedStorage` or
  `Workspace`) if clients are meant to play it; a `ServerStorage` clip is
  server-only.
- FBX-imported animations already become KeyframeSequences in the place
  (`src/plugin_host/api_fbx_anim.rs`), so they use the same route.
- If an animation is kept as a separate `.rbxm`, load it into the place
  before cooking or ship it in the content pack.

#### Acceptance

- A KeyframeSequence saved in the place plays on R15 and R6 rigs with no
  network and no asset id. The test uses a hand-built KFS, so it passes with
  no animation editor installed.
- `LoadAnimation(kfs)` returns a normal track; `Length`, `Looped`,
  `Priority`, `TimePosition`, `Stopped` and `KeyframeReached` behave.
- Edit-mode preview plays without entering Play.
- A server-loaded direct clip reaches the client; a client-only clip does
  not.
- The existing `AnimationId` / `RegisterKeyframeSequence` paths still pass
  the upstream animation tests.
- A screenshot or pose sample shows the rig actually moving
  (`Motor6D.Transform` changes), not just a zero-length track.

---

## 8. Assets: bundle, cook, and offline lookup

### 8.1 Current lookup order (important)

- Images/meshes (`AssetStore`): disk cache → network → local store.
- Animations/sounds (`public_prefetch`): disk cache/ledger → network → local
  store.
- The local store is a **fallback**, despite `MECHANICS.md` describing it as
  “before downloading.” This is the main reason a naive offline run can stall
  on failed downloads.

For the MVP, seed the **per-game cache**; it is checked first for every asset
type and supports versioned IDs.

Direct in-place KeyframeSequences are not assets at all: they live in the
place and load through the animation registry (§7.2), so this lookup order
does not apply to them.

### 8.2 Cook tool

Inputs: place file, content roots, output bundle directory.

Steps:

1. **Static scan.** Walk the DataModel and collect every
   `Variant::Content` / `Variant::ContentId`. Use the reflection database to
   catch content-typed string properties; fall back to a property list based
   on `crates/rustudio-player/src/asset_report.rs`.
2. **Script scan.** Regex the `Source` of scripts for `rbxassetid://`,
   `?id=`, and `assetId` patterns. This over-approximates dynamic references;
   over-collecting is acceptable, missing is not.
3. **Runtime capture.** Run the place with a *clean* `RUSTUDIO_CACHE_DIR` and
   no local store. Every asset actually requested lands in the staging
   cache. Play through all content paths for the release. If a signed-in
   playthrough has already filled the editor cache, the ledger from §8.4 can
   seed this step; the clean staging cache remains the source of truth.
4. **Fetch missing.** Resolve candidates through the engine's asset path with
   the developer's session where required; public assets work anonymously.
5. **Report.** Write `assets.json` with id, size, hash, status, and the
   property/script that referenced it. Fail the cook on unresolved local
   files, unsupported schemes, or missing required assets.
6. **Verify.** Run the bundle with the network disabled and a clean home
   directory; assert no failed assets and no outbound calls.

Details:

- Cache filenames are `<id>_vlatest.bin` or `<id>_v<version>.bin`; keep both.
- `rbxasset://` references belong to the content pack, not the cache.
- `file://` and relative paths must be copied into the bundle and either use
  the `.rustudio-local-content` marker or be converted to a bundled asset.
- `http(s)://` content is not cookable by default; report it and fail.
- `rbxthumb://` is a live web thumbnail; reject it in game UI.
- Per-game cache directories prevent ID collisions between games.
- Re-run the cook whenever the place or content changes.

### 8.3 Offline mode (F2)

F2 adds `RUSTUDIO_OFFLINE=1` semantics:

- check the local store before the network (and before scheduling downloads),
- return `Unavailable` immediately instead of attempting a download,
- suppress session waits and auth messages,
- keep behaviour identical when the flag is off.

Implementation should be a small helper called from the existing decision
points in `assets.rs` (`fetch_cached`, `public_prefetch_inner`,
`AssetStore::fetch_inner`), not a rewrite of the asset layer.

### 8.4 Save-as-you-load (automatic offline capture)

The engine already saves asset bytes to disk. Every successful download goes
through `write_cache` / `cache_path`, including assets fetched through the
signed-in session:

- images and meshes (`AssetStore::fetch_inner`),
- sounds and audio (`domains/media.rs` → `public_prefetch`),
- animation assets (`public_prefetch` → `__rs_anim_asset`),
- models and modules (`InsertService:LoadAsset`, `require(assetId)` →
  `remote_cached` / `public_cached`),
- clothing and other content-property assets,
- video assets where the media path supports them.

The disk cache is checked first on later loads, so an asset downloaded once is
already usable offline on that machine. The current code has no disk-cache
eviction; entries persist until the user or OS deletes them.

The same cache serves Edit and Play, so simply opening a place and looking at
it captures what it requests. Assets the engine never requests are not
captured; the cook's static scan (§8.2) covers those.

So the missing piece is **not the saving**. It is knowing what belongs to a
place, making retention explicit, and exporting the right subset into a game
bundle.

Plan:

1. **Capture ledger.** Extend the existing content tracking
   (`content_track` / `CONTENT_STATUS` in `assets.rs`) with a small JSON
   ledger: id, version, status, size, hash, cache path, time, and the
   requesting place/session. One record per id+version. (As built: the
   ledger keeps id, version, status, size, cache path, time and place; the
   export computes the sha256 as it copies, so a play run does no hashing.)
2. **Durable retention — out of scope.** The download cache is never
   pruned: no size budget, no eviction, no cache-clearing UI. It is a
   re-downloadable copy, not authored data, and deleting it by hand only
   costs a fetch. The per-game bundle's `cache/` is the canonical copy for a
   shipped game. “Export for Offline Use” writes beside the place, outside
   the download cache, so exports survive whatever happens to it.
3. **Per-place export.** Add “Save assets for offline use” for the open place:
   copy every ledger entry for that place into `<bundle>/cache/assets`, write
   a manifest plus hashes, and report ids that failed or were never
   requested.
4. **Signed-in capture.** A download made through the account session is
   written to the same cache, so it is captured automatically. Store bytes
   only; never store session/cookie data with the ledger.
5. **Asset types.** All binary content ids use the same route, so the ledger
   is type-agnostic. `rbxthumb://` is cached under `cache_dir()/thumbs/` and
   is UI-only; export it only if a game's UI needs it offline. Web JSON
   (avatar profiles, product info, DataStores) is not an asset and is not
   captured. Engine/Studio content is a separate download
   (`studio_content.rs`); the game bundle's content pack covers that.
6. **Cook integration.** The cook tool starts from the ledger when the place
   has been played signed in, then static-scans and downloads to fill gaps.
   The ledger is the authoritative list of what a real playthrough touched.
7. **UI — per-place view and local imports done.** Asset Manager’s **This
   Place** scope lists the open place’s captured assets (id, status, size,
   version, cache file) with a copy action for `rbxassetid://…`, and the
   files imported into the place’s own `assets/` folder with a copy action
   for their relative reference (`assets/<name>`). **Export for Offline
   Use** runs the same `rustudio-cook export` a bundle uses. Bulk Import
   also has **Import Locally into This Place**: images, sounds and meshes
   are copied into `<place folder>/assets/`, `.rustudio-local-content` is
   created so the engine resolves the relative paths, and `.rbxm/.rbxmx`
   are inserted into the DataModel as before. Nothing is uploaded. There
   is no cache size/retention UI: the download cache is never pruned
   (item 2), and the export lands beside the place.
8. **Failure and version handling.** Keep versioned entries
   (`<id>_v<version>.bin`); a later version never overwrites an older one
   when a place asks for it. Record failed requests so the export can fail
   loudly instead of shipping a broken game.

Maintenance: the cache writes are upstream behaviour. The ledger/export is
mostly downstream; if a small public accessor is needed, fold it into F4
rather than adding another patch. No new downloader is needed.

---

## 9. Offline hardening

Audit and decide per service. The default is fail fast with one clear log
line, not a timeout.

| Area | Current behaviour | Offline plan |
|---|---|---|
| Asset cache | cache → network → local | F2 makes it cache/local only |
| Account/session | restored unless `RUSTUDIO_NO_SESSION` | launcher disables |
| Avatars/thumbnails | web calls | not used by a shipped game; avoid in UI |
| DataStores | local in-memory by default | keep; phase 2 adds disk persistence |
| HttpService | real HTTP | real HTTP by default; with `"network": false` requests fail before a socket opens, one log line per service/request |
| Marketplace/Badges | session-backed web calls | fail fast: a shipped game has no session (`RUSTUDIO_NO_SESSION`) |
| TeleportService | multi-place join flow | local place-to-place later (F6) |
| Team Create | not in the player crate | n/a |

Also:

- Log every blocked network attempt once, with the service and URL. As
  built: `HttpService` and `CreateWebStreamClient` do this when network is
  disabled, and the asset downloader treats it as offline.
- **Network is allowed by default.** A game that wants to call the internet
  keeps working; a bundle also sets `RUSTUDIO_HTTP_ENABLED=1` so
  `HttpService` is enabled (the editor still needs an Experience Settings
  toggle). `"network": false` in the manifest is the opt-out for a sealed
  build; it sets `RUSTUDIO_NO_NETWORK=1`. `offline: true` is a separate
  promise about game assets (cache/local store only) and does not cut the
  network.
- Keep `RUSTUDIO_ALLOW_LOCAL_CONTENT` off; use the bundle marker instead.

---

## 10. Local DataStores (phase 2, F5)

These are Roblox DataStores, not a separate save API: scripts keep calling
`DataStoreService:GetDataStore(...):SetAsync` / `GetAsync`. rustudio's local
backend (`domains/data_local.rs`) is in-memory only, and a local place
(`PlaceId 0`) is refused before it gets there, so a shipped game's progress
is lost when the process exits.

F5 design:

- Add a native file bridge (`read`, `write`, `list`, `remove`) under
  `<bundle>/datastores` (`RUSTUDIO_DATASTORE_DIR`). The launcher sets it;
  the editor leaves it unset and keeps the current in-memory behaviour.
- A configured datastores directory selects the local backend for a local
  place, so an offline game's `DataStoreService` calls work.
- Use atomic writes (write temp, fsync, rename).
- Key by game id (the bundle directory) + DataStore name + scope + key.
- Version the on-disk schema; a corrupt or newer record is a loud error.
- Keep the cloud-backed path untouched and selected only when live data is
  explicitly enabled.
- Acceptance: a place writes a value, exits, relaunches, reads it back
  (`datastores_survive_a_restart` in `domains/data.rs`).

---

## 11. Patch stack

Target: six patches for the MVP, animation, and saves; seven with local
teleports. Every patch is listed here and in `git log`.

| ID | File(s) | Purpose | Est. size | Upstreamable | Phase |
|---|---|---|---|---|---|
| F0 | `game/`, `scripts/phase0-*.sh`, `.gitignore` | Downstream scaffold: spawn launcher, bundle format, Phase 0 fixture, cook tool (new paths; no upstream code) | — | n/a | 0 (cook: 1) |
| F1 | `crates/rustudio-player/src/player_app.rs` | Window title from `RUSTUDIO_WINDOW_TITLE` (`run_with_args`/icon still optional) | 20–40 lines | Maybe | 2 (title done; icon optional) |
| F2 | `crates/rustudio-core/src/assets.rs`, `rustudio_settings.rs` | Offline mode: local-first, no network scheduling | 30–60 lines | Yes | 1 |
| F3 | `crates/rustudio-core/src/default_scripts.rs`, `assets.rs` | `RUSTUDIO_ENGINE_CONTENT_DIR` as the first content root (validated per half) | 10–20 lines | Yes | 2 (done) |
| F4 | `crates/rustudio-core/src/assets.rs`, `player_app.rs` / `src/app.rs` (2-line place hooks) (+ `rustudio-cook export`) | Asset capture ledger + requested-ID manifest/export (the cache already persists the bytes) | 40–80 lines | Yes | 1 (done) |
| F5 | `crates/rustudio-engine/src/domains/data_local.rs`, new `data_store_files.rs` | File-backed local DataStores: the same `DataStoreService` API, records under `<bundle>/datastores` | 150–300 lines | Maybe | 2 |
| F9 | `crates/rustudio-core/src/paths.rs` | `RUSTUDIO_HOME` scopes data/config/state/cache/crash to a game folder | ~25 lines | No (fork-only; upstream has no bundle use case) | 2 |
| F6 | `crates/rustudio-player/src/play_teleport.rs` | Local multi-place teleports | larger | No | 3 |
| F7 | `crates/rustudio-engine/src/library.luau` (plus `anim_tracks.rs` / `anim_replicate.rs` only if needed) | Direct KeyframeSequence / AnimationClip loading, no upload or asset id; stable clip ids for replication | 20–80 lines | Maybe (upstream targets API parity; likely fork-only) | 1 (load), 2 (replication) |
| U2 | `vendor/egui_dock`, `src/layout_store.rs` | A dock leaf whose active tab index is past the end draws no tab body (a blank 3D viewport); clamp on tab removal and on layout load | ~25 lines | Yes | done (upstream bug; PR not sent) |
| U3 | `src/viewport.rs`, `src/properties.rs`, `src/props_layout.rs`, `src/app.rs`, `src/cloud_ui.rs`, `crates/rustudio-render/src/adorn.rs` | Attachment selection: Studio's sphere/Axis/SecondaryAxis gizmo, Move/Rotate drag, Visible spheres, and CFrame/WorldCFrame/Axis property frames | ~640 lines | Yes | done (upstream feature; PR not sent) |

The content pack manager (`gamepack`) and first-launch resolution are
downstream code. F3 only gives the engine a clean content root.

F0 is downstream-only: it adds `game/` and the Phase 0 scripts, and does not
touch upstream code. U1 (sound autoplay) was an upstream bug fix, not fork
behaviour; it merged as upstream PR **#46** (`ade1ff5`) and the local patch
was dropped in that sync (§2.1). F2 is now upstream PR **#47**.

F4 is in: `assets.rs` writes `<cache>/capture-ledger.jsonl` from
`content_track` (the one place every settled cloud request passes), and
`player_app.rs` / `app.rs` pass the opened place to it; the export itself is
`rustudio-cook export` (downstream). The launcher sets
`RUSTUDIO_ASSET_PLACE` for a shipped game.

Rules:

- Do not stack unrelated fixes in one patch.
- Add a test for each behavior change where practical.
- Keep context small: a helper function plus a call site, not edits spread
  across a file.
- If upstream's version of a hot function changes, re-implement the patch
  deliberately and re-run its test.
- If F7's replication half grows beyond a small change, split it into F8
  rather than letting F7 sprawl.

Hot files to watch: `assets.rs` (asset policy), `default_scripts.rs`
(default content), `player_app.rs` (entry/branding), `data_local.rs`, and
the animation path (`library.luau`, `anim_tracks.rs`, `anim_replicate.rs`),
which upstream is actively developing.

---

## 12. Phases

Estimates are rough and marked as such.

### Phase 0 — Spike (2–4 days, estimate)

- Build the unmodified player.
- Create a bundle with a cached asset set. The content pack is optional at
  this stage: use a local engine/Studio content root or the blocky fallback,
  so the spike does not depend on building a pack first.
- Write the spawn launcher.
- Run on a clean VM with networking disabled and no Roblox Studio install.
- Use a fixture place with a texture, sound, and an in-place KeyframeSequence
  animation. Play it first through the current register-then-`LoadAnimation`
  form; after F7, through the direct `LoadAnimation(kfs)` form.
- Verify with screenshots, not just exit codes.

Exit criteria: the fixture runs, animates, renders, and makes no outbound
connections from a clean environment.

### Phase 1 — MVP (3–6 weeks, estimate)

- Cook tool with static scan + runtime capture + report.
- Content pack v1: PlayerModule equivalent, Animate, blocky rig.
- `gamepack` build/update tool and the launcher's first-launch resolution
  (bundled mode first; download mode can follow).
- F2 (offline mode), F4 (asset capture ledger + manifest/export).
- Capture ledger + “Save assets for offline use” export; signed-in
  playthrough → bundle verification.
- F7: direct KeyframeSequence / AnimationClip loading. Test with a hand-built
  KFS; no animation editor is required.
- Launcher env contract, logging, error UI.
- CI: workspace tests, launcher build for Linux/Windows/macOS, offline smoke.
- Bundle release script and manifest docs.

Exit criteria: a small original game, cooked from a place, runs offline on a
machine that has never had rustudio or Roblox Studio. Movement, camera,
animation, GUI, and audio all work. An animation saved in the place plays
through `humanoid:LoadAnimation(kfs)` with no asset id. A game with no
content pack still launches with the blocky fallback; adding a pack enables
movement/camera and default animation.

### Phase 2 — Product pass (3–5 weeks, estimate)

- F5 local, file-backed DataStores (the same `DataStoreService` API; no new
  save API).
- F1 window title (done); the icon is optional.
- F3 clean engine content root (done).
- If/when upstream animation authoring tools land, their preview reuses the
  same direct playback path; no separate integration is required.
- Direct-clip replication, with F8 only if path-based ids are not enough.
- Asset Manager per-place view and local imports (done). Cache management
  is out of scope: the download cache is never pruned.
- Download mode for the content pack: fetch, hash check, per-user install,
  reuse, and update.
- Studio UI actions for Build/Update Content Pack: skipped (2026-11-27).
  The `gamepack` CLI is enough, and the editor should not put pack
  management in a game developer's way.
- Optional content pack additions only if a game needs them. The default
  pack keeps Studio's R15/R6 bodies, Animate and font fallbacks unchanged;
  no custom rig is planned.
- Game-scoped config/crash/log paths (F9, done).
- Offline service messages: network is allowed by default; `"network": false`
  is the explicit opt-out with one clear log line per blocked request (done).
- Packaging: Linux release bundle, checksum and smoke pass (done
  2026-11-27), and the editor has File > Package Game… for a playable
  test folder. Windows/macOS packaging is built by the same script but
  tested only when there is a machine for it; signing (GPG) and
  notarization wait for a distribution channel that requires them.

The content pack is a full Studio run-time root (PlatformContent,
content/{textures,sky,fonts,sounds}, avatar, PlayerScripts and character
sounds), ~53 MB. The cook captures with the pack and no session, so the
local body/head/Animate assets land in `cache/assets`; before this a
package had no studs textures and an invisible head.

Exit criteria: a game that a non-developer can unzip and play; DataStore
values survive restarts; logs are findable; no upstream configuration leaks
into gameplay. Met on Linux on 2026-11-27 (release smoke); Windows/macOS
await a machine to test on.

### Phase 3 — Distribution (ongoing)

- Signing, notarization, installer/portable choices.
- Per-game branding.
- Optional updater (signed manifest) if desired.
- Multi-place teleports (F6).
- Compatibility testing against each upstream sync.

---

## 13. Upstream sync runbook

### 13.1 Setup

```sh
git remote add upstream https://github.com/rippelz/rustudio.git
git fetch upstream --tags
git switch -c game upstream/main
git config rerere.enabled true
git config rerere.autoupdate true
```

Branches:

- `upstream/main`: fetched mirror, never commit.
- `game`: linear downstream branch.
- `release/*`: immutable release tags, never rebased.

### 13.2 Sync

```sh
git switch game
git status --short          # must be clean
git tag pre-sync-$(date +%Y%m%d)
OLD_UP=$(git rev-parse upstream/main)
OLD_GAME=$(git rev-parse game)
git fetch upstream --tags
git rebase upstream/main    # or: git rebase <latest-release-tag>
cargo test --workspace
scripts/offline-smoke.sh
git log --oneline upstream/main..game
```

Review after rebase (the old commits stay reachable through their hashes):

```sh
git range-diff "${OLD_UP}..${OLD_GAME}" upstream/main..game
```

`range-diff` is the check that the patch stack still means the same thing;
a clean rebase is not enough.

### 13.3 Conflict policy

- Resolve in the patch commit that owns the behavior.
- If upstream implemented the feature, delete the patch and adjust
  downstream code.
- If upstream changed the surrounding semantics, re-prove the patch with its
  test; do not trust a mechanical resolution.
- Keep the rebase linear. If a patch is no longer viable, rewrite it rather
  than merging upstream.

### 13.4 Cadence

- Weekly during active upstream development, or once per upstream release if
  `main` is unstable.
- Always sync before starting a new downstream feature.
- Record the tested base in §16.

### 13.5 Upstream submissions

Open PRs early for F2, F3, F4 and the local-save idea. A merged upstream
feature is worth more than any local abstraction: it removes a patch forever.

F7 (direct clip loading) is a deliberate fork extension. Upstream targets
Roblox API parity, so propose it only if upstream wants the extension;
otherwise keep it a thin, additive patch. F7 does not depend on any authoring
UI: a hand-built, imported, or script-built KeyframeSequence exercises it.
The animation editor remains upstream work — do not duplicate it in the fork.

---

## 14. Testing

Run all of these after every sync and before every release.

1. `cargo test --workspace`.
2. Downstream unit tests: manifest parsing, path resolution, cook scan,
   bundle verification.
3. Offline smoke test on Linux:

   ```sh
   unshare -n scripts/offline-smoke.sh
   ```

   Equivalent isolation on Windows/macOS: firewall or a VM with networking
   off. “No network interface” is the honest test.

4. Clean-home test: no `~/.cache/rustudio`, no `~/.local/share/rustudio`, no
   Studio install.
5. Fixture place: texture, mesh, sound, in-place KeyframeSequence animation,
   GUI, and a persisted DataStore value (phase 2: write, exit, relaunch,
   read back). The animation is played both ways: the
   existing register-then-`LoadAnimation` form and, after F7, the direct
   `humanoid:LoadAnimation(kfs)` form. Assert the track has a real `Length`
   and the rig's `Motor6D.Transform` changes; repeat on R15 and R6.
6. Screenshot review with the project harness:

   ```sh
   rustudio-player place.rbxl --play --shots 2,5,10 --screenshot shot.png
   ```

   Look at the screenshots. A passing exit code is not a rendering test.
7. Asset audit with `RUSTUDIO_ASSET_REPORT` plus the cook report.
8. Cross-platform build and launch: Linux, Windows, macOS.
9. Performance smoke: `--bench` and compare frame-time regressions.
10. Two-machine test: cook on the dev machine, run on a fresh VM with no
    network and no development tools.
11. Edit-mode preview: a selected KeyframeSequence plays on the selected rig
    without entering Play, via a small downstream action. It must not require
    the animation editor.
12. Signed-in capture/export: play a fixture place signed in with an image, a
    mesh, a sound, an animation asset, and a model. Confirm the cache and the
    ledger, export the assets, then run on a clean home with networking off;
    assert no downloads and the same visuals.
13. Content pack resolution: (a) bundled pack → used; (b) no pack → launches
    with the blocky fallback and logs what is missing; (c) download mode
    against a local test server → fetch, hash check, install, reuse, no
    second fetch (`scripts/pack-download-smoke.sh` covers install, reuse
    with the archive removed, and bad-hash fallback through `file://`);
    (d) bad hash or incompatible engine → rejected, falls back; (e) a newer
    pack version → used on the next launch.
14. `gamepack` build/update: deterministic output, version bump, hash
    written; the runtime accepts the built pack and rejects a tampered one.

---

## 15. Release and versioning

- Downstream version: `game-vX.Y.Z+upstream.A.B.C`.
- `game.json` records the engine version and upstream base.
- Bundle name: `<game>-<version>-<os>-<arch>.zip` (or `.tar.gz`).
- Include: launcher, player, manifest, place, `cache/assets`, and content.
  Never ship `cache/pipelines/` (GPU-specific) or `cache/capture-ledger.jsonl`
  (the developer's place paths), saves or a home folder.
- Publish checksums in `sha256sum -c` format.
- `scripts/release-bundle.sh` builds the folder, `release.json` (per-file
  sha256) and the archive; `scripts/release-smoke.sh` proves the Linux
  release unpacks, verifies, uses the bundled pack and runs sealed.
- Keep releases immutable; sync work happens on `game`, releases are tags.
- Optional updater only after the offline path is stable; it must be signed
  and opt-in.

---

## 16. Current base

Update this table on every sync.

| Date | Upstream base | Downstream | Patch stack | Notes |
|---|---|---|---|---|
| 2026-11-26 | `3f1ada8` (main; `Cargo.toml` 0.5.2) | plan | — | Initial plan |
| 2026-11-26 | `3f1ada8` (main; `Cargo.toml` 0.5.2) | Phase 0 | F0, U1 | Phase 0 spike on Linux; U1 is upstream PR #46 |
| 2026-11-26 | `7c2ce14` (main; `Cargo.toml` 0.6.0) | Phase 1 | F0, U1 | Synced 0.5.2 → 0.6.0 (Animation Editor); rebase + range-diff identical; workspace tests and Phase 0 smoke pass. U1 still open as PR #46. |
| 2026-11-26 | `ade1ff5` (main; `Cargo.toml` 0.6.0) | Phase 1 | F0, F2, F4, F7, U2 | U1 (PR #46) merged upstream; local U1 dropped by rebase. F2 PR #47 open upstream. F4 ledger + `rustudio-cook export` done. U2 (stale dock tab blanks the viewport) found and fixed. Rebased; gate rerun. |

---

## 17. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Upstream churn in `assets.rs`, `default_scripts.rs`, `player_app.rs` | Patch budget, small patches, tests, weekly rebase |
| Local store is a fallback and can stall offline runs | Seed cache for MVP; F2 for local-first |
| Asset cache is cleared by the OS/user | Per-place ledger + export; optional durable library mirror |
| Signed-in capture misses runtime-created ids | Full playthrough + ledger + static scan; fail on unresolved |
| Default content pack drifts from upstream expectations | Keep filenames/layout in one documented place; F3 makes it explicit |
| First-launch download breaks the offline promise | Bundled is the default; download mode is explicit and tested |
| Pack download fails, is corrupted, or is incompatible | Hash + engine range check; fallback; retry next launch |
| Pack updates break older games | Versioned packs; each game pins its pack/engine; updates are opt-in |
| User's saved rustudio settings change game behavior | Env overrides for critical keys; F9 game-scoped home |
| Network calls from a game or a service | Offline smoke with a real network namespace; F2; log blocked calls |
| Cook misses a dynamically built asset ID | Script regex pass + full playthrough capture + fail on unresolved |
| Own PlayerModule incompatible with games expecting the standard API | Implement `GetControls`/`GetCameras` and test with fixture places |
| Upstream animation API churn (F7 touches active code) | Keep F7 a thin wrapper; coordinate with upstream; re-run upstream animation tests |
| Direct clip replication uses local ids | Stable path/attribute ids in F7; split F8 only if needed |
| Edit-mode animation preview needs an Animator step outside Play | Small preview tick; keep the runtime path identical |
| Read-only install cannot write cache/DataStores/home | Decide portable vs. per-user runtime directory early |
| Graphics driver/backend differences | Test on all three platforms; keep wgpu backend defaults |
| Unsigned Windows/macOS binaries trigger warnings | Signing/notarization in Phase 2/3 |
| DataStore record corruption | Atomic writes, schema version; a corrupt or newer record is a loud error (backup later) |
| Bundle size | Report size per release; strip release binaries |

---

## 18. Open decisions

1. **Spawn vs in-process:** spawn is the zero-patch MVP; in-process needs F1.
2. **Workspace layout:** nested `game` workspace vs one root member line.
3. **Defaults:** engine content pack vs cook-time injection into each place.
4. **Install model:** portable writable folder vs read-only install with
   per-user cache/saves.
5. **Network policy:** hard-block by default vs opt-in manifest flag.
6. **Compatibility target:** own games only, or a best-effort subset of
   existing places.
7. **Local DataStores:** bundle-local vs per-user directory.
8. **Upstream PRs:** F2, F3, F4 first.
9. **Direct clip API:** `LoadAnimation(kfs)` (recommended) vs an `Animation`
   wrapper/child convention.
10. **Direct clip replication:** path/attribute ids vs a replicated clip
    reference (F8).
11. **Preview surface:** a downstream action/plugin (recommended, independent)
    vs sharing it with upstream animation tools later.
12. **Capture scope:** per place vs per game/experience; recommend a
    per-place manifest and a per-game export.
13. **Retention:** global cache + optional library mirror; no auto-eviction
    by default.
14. **Export trigger:** manual action vs automatic after Play/save.
15. **Content pack delivery default:** bundled vs download-once. Bundled is
    fully offline; download-once is smaller.
16. **Downloaded pack location:** per-user game data directory vs a portable
    folder beside the game.
17. **Pack update channel:** developer CLI first, Studio UI later; whether
    games check automatically or only on an explicit update.
18. **Multiplayer (Phase 3+):** the MVP is offline single-player. The player
    already has a local server/client split (`replicate.rs`), but a
    Minecraft-style host/join mode needs a headless game server, a socket
    transport for the replication ops and remotes, join/player/character
    management, and ownership. `rustudio-net`'s transport and replicator
    vocabulary are a starting point; it is a feature, not a patch.
19. **Asset uploader and This Place:** upstream 0.6's Asset Manager uploads
    to Roblox; that path is untouched. The downstream F4 patch adds a
    **This Place** scope and **Import Locally into This Place**: files are
    copied to `<place folder>/assets/`, `.rustudio-local-content` marks the
    folder as local content, and the reference to paste is the relative path
    (`assets/<name>`) — no upload and no synthetic asset id. The cook
    already bundles relative files and the marker, so a shipped game keeps
    them. The patch is small and lives in `src/asset_manager/`; publishing
    can still reuse `rustudio_account`'s upload call.

---

## Appendix A — Zero-patch smoke test

This is the fastest way to validate the whole approach before writing any
fork code.

```sh
# 1. Build the unmodified player.
cargo build --release -p rustudio-player

# 2. Make a game folder.
mkdir -p /tmp/mygame/{engine,cache/assets,content,logs}
cp target/release/rustudio-player /tmp/mygame/engine/
cp my_place.rbxl /tmp/mygame/

# 3. Put content in the content pack (own PlayerModule/Animate/rig) or omit
#    it to test the blocky fallback.

# 4. Run as a shipped game would. If the content pack is omitted, leave the
#    two content variables unset instead of pointing them at an empty folder.
cd /tmp/mygame
RUSTUDIO_NO_SESSION=1 \
RUSTUDIO_NO_STUDIO_FETCH=1 \
RUSTUDIO_CORE_SCRIPTS=0 \
RUSTUDIO_LIVE_DATA=0 \
RUSTUDIO_CACHE_DIR=/tmp/mygame/cache \
RUSTUDIO_CONTENT_DIR=/tmp/mygame/content/content \
RUSTUDIO_STUDIO_DIR=/tmp/mygame/content \
./engine/rustudio-player my_place.rbxl --window 1600x900

# 5. Repeat with networking off and a clean HOME.
unshare -n env HOME=/tmp/clean-home ...
```

Checklist:

- window opens at the requested size;
- the place runs, not an empty scene;
- with a bundled content pack: character moves, camera follows, and default
  animation works;
- with no content pack: the game still launches with the blocky fallback and
  logs what is missing;
- animation plays from a KeyframeSequence in the place, with no asset id
  (see §7.2; before F7 the same clip uses `RegisterKeyframeSequence`);
- no asset failures in Output/log;
- no outbound network;
- process exits cleanly on Leave/close.

The passive cache is `$RUSTUDIO_CACHE_DIR/assets`. A signed-in playthrough
fills it; copying that folder into the bundle's `cache/assets` is the first
offline test. §8.4 automates this with a per-place ledger and export.

## Appendix B — Environment reference

| Variable | Read in | Notes |
|---|---|---|
| `RUSTUDIO_NO_SESSION` | `player_app.rs` | Any value disables restore. |
| `RUSTUDIO_NO_STUDIO_FETCH` | `studio_content.rs` / settings | Stops classic oof fetch. |
| `RUSTUDIO_CORE_SCRIPTS` | `rustudio_settings.rs` | `0`/`false`/`off` disables. |
| `RUSTUDIO_LIVE_DATA` | `rustudio_settings.rs` | Only `1` enables live cloud data. |
| `RUSTUDIO_CACHE_DIR` | `assets::cache_dir` | Assets + pipeline cache. |
| `RUSTUDIO_DATASTORE_DIR` | `domains::data::datastore_root` | F5: local DataStore records; also enables local DataStores for a local place. |
| `RUSTUDIO_HOME` | `paths::scoped_home` | F9: scopes all per-user paths to a game folder; empty means the person's. |
| `RUSTUDIO_LOCAL_ASSETS` | `rustudio_settings.rs` | `<id>` / `<id>.<ext>` store. |
| `RUSTUDIO_ENGINE_CONTENT_DIR` | `assets::content_dir`, `default_scripts::studio_dir` | F3: the pack root; validated per half. |
| `RUSTUDIO_CONTENT_DIR` | `assets::content_dir` | `rbxasset://` root (development). |
| `RUSTUDIO_STUDIO_DIR` | `default_scripts::studio_dir` | Must contain `ExtraContent/scripts/PlayerScripts` (development). |
| `RUSTUDIO_ALLOW_LOCAL_CONTENT` | `assets::local_content_opt_in` | Only `1` enables; prefer the marker. |
| `RUSTUDIO_ASSET_REPORT` | `asset_report.rs` | Test/audit only. |
| `RUSTUDIO_GAPS` | `telemetry.rs` | Local JSON; tests only. |
| `RUSTUDIO_FIXED_DT` / `--fixed-dt` | `sim_clock` | Deterministic tests. |
| `RUSTUDIO_OFFLINE` | F2 (new) | Offline asset behaviour. |
| `RUSTUDIO_WINDOW_TITLE` | `player_app.rs` | Player window title (F1). |
