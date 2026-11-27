# The content pack (FORK-PLAN §7.1)

The pack is the engine's defaults for a shipped game: the PlayerModule
(camera and controls), Animate, the R6/R15 bodies, and a content root for
`rbxasset://` files. It lives in `game/content/` and is built into a
versioned, hashed folder by `gamepack`.

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
`MoveDirection`) every `RenderStepped`, and jumps on a Space press edge.
`GetMoveVector()` and `IsMoveDirectionFromKeyCode()` are provided.

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
```

The pack root gets `content-pack.json`:

```json
{ "manifest_version": 1, "version": "0.1.0", "sha256": "...",
  "files": { "ExtraContent/...": "<sha256>", "content/...": "<sha256>" } }
```

`sha256` is over the sorted `path\0file-sha256\n` lines, so the build is
deterministic. `build` refuses a non-empty output directory and a source
without `ExtraContent/scripts/PlayerScripts`; `verify` exits 1 on a
missing, changed or new file; `publish` copies to `<target>/<version>/`
and never overwrites a published version.

## Verification

```sh
scripts/pack-smoke.sh          # R15: pack, warm once, offline, probe
scripts/pack-smoke.sh --r6     # the same with the R6 place
scripts/pack-smoke.sh --no-warm
```

The smoke builds the pack with gamepack, assembles a bundle around the
Phase 0 place, warms the cache once online (avatar meshes and Animate
clips), then runs headless and offline with a held W. `game/fixtures/pack/probe.luau`
checks `PlayerModule`, `GetControls()`, `GetCameras()`, a Head, movement
> 2 studs, and a camera at a third-person distance. Both rigs pass.

The real 2026-11-26 playtest (the user at the window, offline) confirmed
walking, walking animation, right-drag rotation, wheel zoom, jump, and a
visible head on the R15 body.
