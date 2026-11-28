# Fork invariants: what must survive an upstream sync

This is the semantic list of what this fork adds or changes, and what a rebase
must not quietly remove. It is deliberately **not** a raw diff: the diff is in
git and changes constantly; what future agents need is intent and a way to
check it.

Use this with:

- [UPDATING.md](UPDATING.md) — the rebase procedure.
- [FORK-PLAN.md](FORK-PLAN.md) §2 and §11 — maintenance rules and the patch
  inventory.

## Before resolving a conflict

For each conflicted file:

```sh
git log --oneline upstream/main..HEAD -- path/to/file
git show <fork-commit> -- path/to/file
git diff upstream/main...HEAD -- path/to/file
```

The `[fork]` commit body says why the line exists. Read it before choosing a
side. Then keep upstream's new behaviour and reapply the fork intent on top.
Never take a whole file from one side.

If a fork feature has moved or upstream has implemented it, update this file
and `FORK-PLAN.md`, and drop the local patch rather than keeping both.

## Invariants

### Bundle, launcher and packaging

**Must keep:** a game folder can be launched with no Roblox account, no Studio
install and no network; the launcher sets the bundle's content/cache/data
roots; per-game window title and game-scoped home/settings/cache/crash; File >
Package Game cooks a place and assembles a playable folder.

**Where:** `game/`, `game/launcher/`, `game/cook/`, `src/package.rs`,
`crates/rustudio-core/src/paths.rs`, the release scripts.

**Check:** `scripts/phase0-smoke.sh`, `scripts/offline-smoke.sh`,
`scripts/release-smoke.sh`; package a small place and launch it.

**Upstreamable:** the player entry point and explicit content root are
candidates; the launcher and bundle format are intentionally fork-only.

### Content pack and offline assets

**Must keep:** bundled pack is the default; download mode is optional; pack
build/verify/publish/archive stays deterministic; offline mode uses the cache
and local asset store only; per-place capture/export writes assets and a
ledger beside the place.

**Where:** `game/pack/`, `scripts/fetch-studio-avatar.sh`,
`scripts/pack-smoke.sh`, `crates/rustudio-core/src/assets.rs`,
`crates/rustudio-engine/` asset capture, Asset Manager's This Place view.

**Check:** `scripts/pack-smoke.sh`, `scripts/pack-download-smoke.sh`; export
assets for a place and reopen it with the network off.

**Upstreamable:** offline asset mode and the export are candidates.

### Local persistence

**Must keep:** DataStores/MemoryStores/Messaging are local and file-backed by
default for a bundle (`<bundle>/datastores`), not dependent on the hosted
service.

**Where:** the engine's DataStore file backend and the launcher's
`RUSTUDIO_DATASTORE_DIR`.

**Check:** run a place that saves and loads a value, relaunch, read it back.

**Upstreamable:** candidate (local DataStore backend).

### Direct animation playback (F7)

**Must keep:** `LoadAnimation(KeyframeSequence)` plays a place's own clip with
no upload and no asset id; clip path ids resolve across the wire.

**Where:** `crates/rustudio-engine/src/rig.rs`, `domains/anim*`,
`library.luau`, the F7 probe/tests.

**Check:** `cargo test -p rustudio-engine` plus the F7 acceptance probe; Play a
place that loads a saved KeyframeSequence directly.

**Upstreamable:** likely; keep the local patch thin.

### Network

**Must keep:** network is allowed by default; a game opts out per game. Do not
reintroduce a forced-offline default.

**Where:** settings/config and the launcher environment contract.

**Check:** settings tests and a place that makes an allowed HTTP request.

**Upstreamable:** no (fork product policy).

### Studio fixes

**Must keep:**

- a dock leaf whose active tab was closed draws nothing (U2);
- attachments are selectable and get the attachment gizmo/property frames (U3);
- holding Space keeps hopping, including after landing.

**Where:** `vendor/egui_dock` (U2), `src/viewport.rs`/attachment frames (U3),
the engine's jump input.

**Check:** the UI tests and a quick interactive check; the jump test in the
engine suite.

**Upstreamable:** U2 and U3 were written to be upstreamable; drop them if
upstream lands equivalents.

### CoreGui and CoreScripts (fork product choice)

**Must keep:**

- `CoreGui` is a saved place service and is always in the Explorer;
- at Play, Roblox's CoreScripts build the live tree; Studio Play shows and
  edits that live tree; runtime UI is not saved back;
- every place has a top-level `CoreScripts` service; Window → CoreScripts
  lists the patch's scripts and CoreGui modules with source;
- a copied CoreScript runs at CoreScript identity in place of the compiled
  bytecode; a copied module replaces the built-in module's source; a Disabled
  entry never starts, and toggling during Play stops/starts the live script;
- an empty `CoreScripts` service is not written to the place.

**Where:** `crates/rustudio-core/src/doc.rs` (service class/serialization),
`crates/rustudio-core/src/core_scripts.rs` (patch catalog/source),
`crates/rustudio-engine/src/runtime.rs` (startup, overrides, Disabled),
`crates/rustudio-player/src/play.rs` (live client tree),
`src/core_scripts_panel.rs` (browser), plus the Explorer/menu wiring.

**Check:** `cargo test -p rustudio-core core_script`,
`cargo test -p rustudio-engine a_core_scripts`,
`cargo test -p rustudio-player place_core_scripts_service_overrides_and_disables_builtins`;
then Play a place, disable `ExperienceChatMain`, and confirm chat never
appears; copy it and confirm the edited Lua runs.

**Upstreamable:** mostly not (it is the fork's developer-power feature), but
generic parts such as a source-backed CoreScript run path could be proposed.

### Cook normalization

**Must keep:** legacy Roblox asset URLs in properties are normalized while
cooking, so a packaged place does not depend on old URL spellings.

**Where:** `game/cook/`.

**Check:** package a place that uses a legacy URL and launch it offline.

**Upstreamable:** probably.

## Adding to this list

When a new fork feature is added:

1. Add its invariant above with the files, a check, and whether it is
   upstreamable.
2. Add or update an automated test where practical. A test is the strongest
   protection against a rebase silently dropping the behaviour.
3. Update the patch inventory in `FORK-PLAN.md` §11.
4. If upstream later implements it, delete the invariant and the fork patch.
