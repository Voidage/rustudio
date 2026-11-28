# The content pack (FORK-PLAN §7.1)

The pack is the engine's defaults for a shipped game: the PlayerModule
(camera and controls), Animate, the R6/R15 bodies, and a content root for
`rbxasset://` files. It lives in `game/content/` and is built into a
versioned, hashed folder by `gamepack`.

**For game developers: use bundled mode and ignore everything else.** The
editor itself never asks you to manage a pack. For a release, run
`gamepack build` once and ship the game folder with the pack inside it —
no downloads and no network on first launch. The download mode described
below is an optional distribution choice (one pack shared by many games);
it changes nothing unless `game.json` explicitly says `"mode": "download"`.

## Roblox's own client scripts (packaged games)

The committed pack has rustudio's small PlayerModule (camera and controls)
so the engine works from a fresh checkout with no Studio. A **package**
built by File > Package Game… or `release-bundle.sh` stages a copy of the
pack and, when Studio is installed, adds:

```text
ExtraContent/models/  DataModelPatch.rbxm, AvatarContextMenu …
ExtraContent/scripts/PlayerScripts/  Roblox's full PlayerModule
ExtraContent/LuaPackages/…/BuilderIcons/  the icon font (tab/top-bar icons)
```

That is ~20 MB (18 MB patch + 1.7 MB scripts). The shipped game then starts
Roblox's own CoreScripts and CorePackages — real CoreGui, SettingsHub,
player list, chat — and uses the real PlayerModule, whose MouseLockController
provides shift lock.
`scripts/fetch-studio-avatar.sh --roblox-runtime --into DIR` does the copy;
the source pack is not modified. Without Studio
the package keeps the minimal module and the native Escape menu, and the
launcher sets `RUSTUDIO_CORE_SCRIPTS=0`; with the patch it sets `1`.

These files are Roblox's (not committed). A public release should confirm
redistribution terms first; local/personal builds are the current use.

### CoreGui

`CoreGui` is a real service in the place and always listed in the Explorer.
It starts empty, or with whatever the place itself saved. Roblox's
CoreScripts build the live CoreGui — chat, the top bar, the player list,
notifications, the settings menu — when Play starts, exactly as Roblox does.

Studio Play steps the client in the editor process, so the Explorer shows
that live tree while the game runs: select a chat frame and change it live,
as in Roblox's Play Solo. Stop brings the place back; runtime UI is not
saved into it. Copy what you want to keep into CoreGui in Edit.

Game scripts can get `game.CoreGui` and modify the live instances. Packaged
games run the same CoreScripts, so chat, settings and shift lock work there
too.

Roblox's scripts are in that live tree: the top-level CoreScripts under
`CoreGui.RobloxGui.CoreScripts` (chat, notifications, player list, …) and the
modules under `CoreGui.RobloxGui.Modules` (BackpackScript, PlayerList, …).
Double-click one while Play runs to open its Lua source in the script editor.
The engine runs Roblox's compiled bytecode; a place can also carry its own
`CoreScript` (Insert Object → CoreScript, under CoreGui). That source runs at
CoreScript identity on the client when Play starts, before game LocalScripts.

### The CoreScripts service

Window → CoreScripts lists Roblox's built-in CoreScripts and the
CoreGui module tree *before* Play, with each one's Lua source. Every place
also has a `CoreScripts` service next to CoreGui (empty until used):

- **Copy into this place** on a script puts a `CoreScript` named exactly as
  Roblox names it (`CoreScripts/ExperienceChatMain`, `StarterScript`, …)
  under `CoreScripts` with that Lua. Play runs the place's source at
  CoreScript identity instead of the patch's compiled bytecode. Edit it in
  the script editor like any script; delete it to go back to the built-in.
- **Copy into this place** on a module creates
  `CoreScripts.Modules.<path>` (the same tree as
  `CoreGui.RobloxGui.Modules`). Play compiles that source for the built-in
  module every script requires, so the override reaches chat, the backpack,
  the player list, and so on.
- The **Enabled** switch writes `Disabled` on the service entry. An entry
  left off never starts, so removing the chat GUI and switching chat off
  means no chat script running behind it. During Play, set `Disabled` on the
  live CoreScript (or delete it) to stop it; ticking it again restarts it
  from its Source.

The browser only lists what the data model patch has; without a Studio
install it says so, and a place that already carries copies still runs
them. The service is a fork class, not Roblox's: an empty one is not written
to the place (it comes back when the place is opened), so a place that never
uses it saves as an ordinary Roblox place.

## Layout

```text
game/content/
  content/                     RUSTUDIO_CONTENT_DIR (avatar/, fonts/, ...)
    avatar/
      character.rbxm             R6 body (classic)
      characterR15DynamicHeadV2.rbxm  R15 body (the engine's R15 file name)
      defaultDynamicHead.rbxm    public fallback head
      scripts/humanoidAnimateR15_DEPRECATED_20260813.rbxm
      scripts/humanoidAnimatePlayEmote.rbxm
      scripts/humanoidHealthRegenScript.rbxmx
  ExtraContent/scripts/PlayerScripts/StarterPlayerScripts/
    PlayerScriptsLoader.lua
    PlayerModule.module.lua
    PlayerModule.module/
      CameraModule.lua
      ControlModule.lua
```

The engine reads these names exactly (`crates/rustudio-core/src/default_scripts.rs`,
`crates/rustudio-engine/src/rig.rs`). `RUSTUDIO_STUDIO_DIR` is the pack
root; `RUSTUDIO_CONTENT_DIR` is its `content/`.

A shipped game has no Studio install to fall back on, so the pack also
carries the files the engine reads at Play time:

```text
PlatformContent/pc/textures/   legacy surfaces (studs), sky, water, brdfLUT
PlatformContent/pc/terrain/    terrain materials and textures
PlatformContent/pc/fonts/      Noto CJK fallbacks
content/avatar/heads/          the R6 head mesh (head.mesh) and head shapes
content/avatar/meshes/         classic R6 limb meshes
content/textures/ content/sky/ content/fonts/ content/sounds/
ExtraContent/scripts/PlayerScripts/StarterPlayerScriptsCommon/  RbxCharacterSounds
```

`scripts/fetch-studio-avatar.sh` copies all of it from the developer's
Studio install into `game/content` (git-ignored, Roblox content). Without
it the packaged game still runs, but parts draw with no studs, the sky and
some fonts are missing, and the R15 head's cloud mesh/face texture is not
in the cooked cache. The cook captures with no saved session, as a
shipped game runs, so the local rig's body, head and Animate clips (R15 or
R6, whichever the place uses) land in `cache/assets`.

## The avatar and Animate files

These are Roblox's default bodies and Animate scripts, copied from the
developer's own Studio install by:

```sh
scripts/fetch-studio-avatar.sh [STUDIO_DIR]
```

They are **not committed** (`game/content/content/avatar/` is git-ignored):
they belong to Roblox, and a public release should replace them with your
own rigs/Animate script or confirm the asset terms. Without them the
engine uses its blocky fallback and the same PlayerModule/camera work.

The built-in Animate script is Studio's, which names cloud animation ids;
a release must run the cook once online with the pack in place so those
clips (and the avatar meshes) are captured into the bundle's `cache/`.
That is the "download once" step; afterwards the game is offline. The
plan's longer-term v1.1 replaces it with our own Animate that embeds
in-place KeyframeSequences (F7), so no cloud ids remain.

**v1.1 is deferred until distribution (decided 2026-11-26).** The cache
already gives the download-once behaviour: warm the bundle once (the cook,
or `rustudio-cook export` after a play session), and every later run is
offline — `scripts/pack-smoke.sh` proves the flow. v1.1 buys three things
that only matter when handing the game to someone else:

1. **No Roblox session at cook time.** The warm needs a signed-in account
   when Roblox refuses an anonymous fetch; embedded clips never do.
2. **No redistribution of Roblox's animation bytes.** Shipping the warmed
   cache publishes Roblox content, the same licensing question as the
   avatar bodies above; our own KeyframeSequences are ours.
3. **Roblox cannot change or remove the ids.** A cooked bundle is safe, but
   a cook on a fresh machine is not.

The cost is authoring the Animate state machine's clips (idle, walk, run,
jump, fall, climb, swim, tool states) as in-place KeyframeSequences. The
intermediate option, if a smaller step is wanted first, is embedding only
idle/walk/run/jump and keeping Studio's Animate as the fallback.

### R15 head

The public R15 fallback head carries a face texture that is transparent
everywhere except the face; drawn as the MeshPart's texture it hides the
head. The pack loader clears that one texture (`11445807951`) on the
client when the head uses it, so the head reads; a custom or avatar head
keeps its own texture. The engine's renderer should handle this itself —
recorded as a gap.

## The PlayerModule

`PlayerModule.module.lua` returns the public surface games use:

- `PlayerModule:GetCameras()` — the CameraModule;
- `PlayerModule:GetControls()` — the ControlModule;
- `PlayerModule:GetPlayerModule()` — the script.

Requiring it starts both, as Roblox's `PlayerScriptsLoader` does (the
engine also has its own loader fallback if the file is missing).

**ControlModule** reads WASD/arrows from `UserInputService`, calls
`Humanoid:Move(vector, true)` (camera-relative; the engine sets
`MoveDirection`) every `RenderStepped`, and sets `Humanoid.Jump` while Space
is held (so holding Space keeps hopping, as Roblox does). `GetMoveVector()`
and `IsMoveDirectionFromKeyCode()` are provided.

**CameraModule** is a third-person orbit camera: it follows the
`HumanoidRootPart`, turns with a right-drag, and zooms with the wheel. Two
engine details matter:

- the right button is tracked from `InputBegan`/`InputEnded`, because
  `UserInputService:IsMouseButtonPressed` is currently a stub returning
  false;
- `UserInputService.MouseBehavior = LockCenter` is set while rotating,
  because raw mouse deltas are only reported in that mode.

`GetCameraType`/`SetCameraType`, `GetCameraState`/`SetCameraState`, and
`GetZoom`/`SetZoom` are provided.

## gamepack

```sh
cargo build --release --manifest-path game/Cargo.toml
game/target/release/rustudio-gamepack build game/content --out game/build/content-pack --version 0.1.0
game/target/release/rustudio-gamepack verify game/build/content-pack
game/target/release/rustudio-gamepack update game/build/content-pack     # bump patch
game/target/release/rustudio-gamepack publish game/build/content-pack dist/packs
game/target/release/rustudio-gamepack archive game/build/content-pack dist/packs/0.1.0.zip
```

The pack root gets `content-pack.json`:

```json
{ "manifest_version": 1, "version": "0.1.0", "sha256": "...",
  "files": { "ExtraContent/...": "<sha256>", "content/...": "<sha256>" } }
```

`sha256` is over the sorted `path\0file-sha256\n` lines, so the build is
deterministic. `build` refuses a non-empty output directory and a source
without `ExtraContent/scripts/PlayerScripts`; `verify` exits 1 on a
missing, changed or new file; `publish` copies to `<target>/<version>/`,
also writes the single-file `<target>/<version>.zip`, and never overwrites
a published version. `archive` writes that zip on its own.

A game in **download mode** points `content.url` at the zip and copies
`sha256` from `content-pack.json`; the launcher verifies and installs it
under the game's per-user data directory, so later launches need no
network. See `game/docs/BUNDLE.md` (`Content resolution`).

## Verification

```sh
scripts/pack-smoke.sh            # R15: pack, warm once, offline, probe
scripts/pack-smoke.sh --r6       # the same with the R6 place
scripts/pack-smoke.sh --no-warm
scripts/pack-download-smoke.sh   # download mode: install, reuse, bad hash
```

The smoke builds the pack with gamepack, assembles a bundle around the
Phase 0 place, warms the cache once online (avatar meshes and Animate
clips), then runs headless and offline with a held W. `game/fixtures/pack/probe.luau`
checks `PlayerModule`, `GetControls()`, `GetCameras()`, a Head, movement
> 2 studs, and a camera at a third-person distance. Both rigs pass.
`scripts/release-smoke.sh` exercises the packaged path: it stages Roblox's
CoreScripts and full PlayerScripts (when Studio is installed), builds a
release, unpacks it with a clean HOME, and runs the same probe offline.

The real 2026-11-26 playtest (the user at the window, offline) confirmed
walking, walking animation, right-drag rotation, wheel zoom, jump, and a
visible head on the R15 body.
