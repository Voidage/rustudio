# Handoff — start here (Phase 1)

Read this first, then `docs/FORK-PLAN.md` §2 (maintenance rules), §12
(Phase 1) and `game/docs/PHASE0.md`. The plan is the source of truth; this
page is the state of the world.

## What we're building

A downstream fork of rustudio (Roblox-style local-first engine/editor):
self-contained game bundles that run offline with no account and no Roblox
Studio install, plus direct KeyframeSequence playback. Upstream is under
active development, so every change must stay cheap to rebase.

## Current state (2026-11-26)

Phase 0 is complete and visually confirmed by the user:

- `game/launcher`: spawn-mode launcher, no rustudio deps (11 unit tests).
- `game/fixtures/phase0`: generated place (R15 + R6 variants) with a cached
  texture, a cached looping sound, and an in-place `KeyframeSequence`; the
  probe plays it with the current register-then-`LoadAnimation` form.
- `scripts/phase0-bundle.sh` assembles `game/build/phase0-game`.
- `scripts/phase0-smoke.sh` passes three headless runs: R15 via launcher,
  R6 via engine, clean-HOME/no-Studio blocky fallback. A clean run under
  `strace` made no AF_INET/AF_INET6 connections.
- Upstream PR **#46** (autoplay fix) is open:
  https://github.com/rippelz/rustudio/pull/46

## Git state

- Repo initialized here. Base: **`3f1ada8`** — upstream `main` at snapshot
  time, `Cargo.toml` 0.5.2. (Not the `v0.5.2` tag; #39/#40/#41 are included.)
  Upstream `main` has since moved to 0.6.0.
- `game` branch (checked out here) has 4 commits: U1 (autoplay fix + test),
  F0 launcher/bundle contract, F0 fixture/smoke, F0 PR record. Tree is clean.
- `u1-autoplay-sounds` branch lives in the `../upstream-u1` worktree, one
  commit on upstream `main`; it is PR #46's head.
- Remotes: `upstream` → rippelz/rustudio, `fork` → Voidage/rustudio.
- The downstream `game` branch is **local only** (not pushed).

## Verify before you change anything

```sh
cargo test --manifest-path game/Cargo.toml   # launcher
cargo test -p rustudio-player                # player suite incl. U1 test
scripts/phase0-bundle.sh                     # builds player + launcher
scripts/phase0-smoke.sh                      # 3 headless checks
game/build/phase0-game/phase0-game            # visual check — ask the user
```

## Next: Phase 1

Start with a sync (FORK-PLAN §13.2, §13.4): clean tree, tag `pre-sync-…`,
`git fetch upstream --tags`, rebase `game` onto current `upstream/main` (or
the latest release tag). If PR #46 has merged, drop the local U1 commit;
otherwise keep/re-derive it. Then run the test gate:
`cargo test --workspace` plus the smoke test. Only then start:

1. Cook tool: static scan + runtime capture + `assets.json` report.
2. Content pack v1: PlayerModule equivalent, Animate, blocky rig; `gamepack`
   build/update; launcher first-launch resolution (bundled first).
3. F2 offline mode (cache/local first, no network scheduling).
4. F4 capture ledger + “Save assets for offline use”.
5. F7 direct `humanoid:LoadAnimation(keyframeSequence)` (no upload/id).
6. Launcher logging/error UI, CI, release script and docs.

## Rules and user preferences (important)

- **No screenshots and no mouse/keyboard control.** The user is at the
  computer. Verify with Luau probes, Output logs, `--result`, strace; for
  visual checks, launch the game and ask the user what they see.
- Upstream-first: fixes and generic features go upstream as small PRs
  (FORK-PLAN §2). Downstream code lives in `game/`, `scripts/`.
- Patch budget: small, single-purpose commits tagged `[fork] F<n>:` (or
  `U<n>` for upstream fixes); §11 inventory must match the log.
- The user is new to git: explain branch/remote actions plainly, and confirm
  before any push or public change.
- Player tests can play the classic "oof" sound locally — that is normal.
- `game/build/` and `game/target/` are git-ignored build output.
