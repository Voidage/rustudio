# Updating the fork from upstream

`game` is a downstream fork of [`rippelz/rustudio`](https://github.com/rippelz/rustudio).
Upstream is actively developed. Keep `game` a **linear stack of `[fork]`
commits on top of `upstream/main`**: rebase, never merge upstream into it.

Read this with:

- `AGENTS.md` — build/test commands and change rules.
- `docs/FORK-PLAN.md` §2 — maintenance rules and the patch budget.
- `docs/FORK-INVARIANTS.md` — what each fork feature must keep doing. Check it
  before resolving a conflict.

## State when this was written (2026-11-27)

- `fork` → `https://github.com/Voidage/rustudio.git` (where `game` is pushed)
- `upstream` → `https://github.com/rippelz/rustudio.git`
- branch `game`, upstream head `ade1ff5`, **0 commits behind**
- **48** `[fork]` commits on top of that upstream head

These numbers go stale. Recompute before trusting them:

```sh
git fetch upstream --prune
git log --oneline -1 upstream/main
git rev-list --count upstream/main..game    # fork commits
git rev-list --count game..upstream/main    # upstream commits missing
git log --oneline upstream/main..game       # the patch stack
```

## When to update

- Before starting new work, and regularly (for example weekly).
- At a clean boundary: no uncommitted changes and no Play session running.
- Small, frequent rebases are far cheaper than one large rebase after months.

## Pre-flight

1. `git status --short` must be clean apart from known untracked local files.
   Commit or stash work first. Do not commit screenshots, caches, or `pic.png`.
2. `git switch game`
3. Make a backup: `git branch backup/game-$(date +%Y%m%d)`
4. `git fetch upstream --prune`
5. Inspect the upstream commits you are about to take:
   `git log --oneline game..upstream/main`
6. Confirm the stack is still linear:
   `git rev-list --merges upstream/main..game` (nothing should print).

## Rebase

```sh
git rebase upstream/main
```

Rules:

- Resolve every conflict by reading both sides. Keep upstream's new behavior
  and reapply the fork change on top. Never choose `--ours` or `--theirs`
  wholesale for a whole file.
- Check `docs/FORK-INVARIANTS.md` for that file's fork intent, and read the
  `[fork]` commits that touched it (`git log --oneline upstream/main..HEAD --
  <file>`).
- Do not mass-reformat, move, or delete upstream code to make a conflict go
  away.
- If upstream landed the same feature, drop the now-empty fork commit
  (`git rebase --skip`), update `docs/FORK-PLAN.md`, and remove the local
  duplicate.
- After editing: `git add <files>`, then `git rebase --continue`.
- To stop and recover: `git rebase --abort` (the backup branch and
  `git reflog` are the safety nets).
- Keep the result linear. Do not `git merge upstream/main` into `game`.

## Where conflicts are likely

Fork changes live in fork-only paths and in upstream files. Expect most
conflicts in:

- `crates/rustudio-core/src/doc.rs`, `core_scripts.rs`
- `crates/rustudio-engine/src/runtime.rs`, `security.rs`
- `crates/rustudio-player/src/play.rs`, `play_game.rs`, `play_worker.rs`,
  `game.rs`
- `src/app.rs`, `src/panels.rs`, `src/layout_store.rs`, `src/studio_ui.rs`,
  `src/studio_actions.rs`

CoreGui/CoreScripts is the newest and largest fork feature: it touches
serialization, Play startup, replication, and Studio UI, so upstream changes
in those areas deserve extra care. Fork-only paths (`game/`, `scripts/`,
`src/core_scripts_panel.rs`, this document) are much less likely to conflict.

The patch budget in `FORK-PLAN.md` targets roughly six small upstream-touching
patches with a hard ceiling of eight. The stack is currently much larger than
that (48 commits, including a large CoreGui/CoreScripts commit), so treat every
new upstream-touching change as debt. Prefer an upstream PR for anything
generally useful, and keep fork patches small and separate.

## Test gate

Before pushing the rebased branch:

```sh
cargo test --workspace
scripts/offline-smoke.sh
```

Also run what the change area needs:

- packaging, launcher, or bundle changes: `scripts/pack-smoke.sh` and/or
  `scripts/release-smoke.sh`
- engine/Play changes: open a real place, Play it, and exercise CoreGui and the
  CoreScripts browser
- Studio UI changes: launch the editor and check the affected panels

A semantic conflict resolved by guesswork is not finished until these pass.

## Publishing the rebased branch

`game` is pushed to `fork` and tracks `fork/game`. Rebasing rewrites history,
so the update needs a lease-protected force push — never plain `--force`:

```sh
git push --force-with-lease fork game
```

Do not push to `upstream`. Do not open a pull request upstream unless asked;
generally useful changes should be proposed upstream first (FORK-PLAN rule 1).

## Rollback

- During a rebase: `git rebase --abort`.
- After a rebase, before pushing: `git reset --hard backup/game-YYYYMMDD` is
  destructive; use it only with the user's agreement, or use `git reflog` to
  find the previous head.
- After a bad push: force-push with `--force-with-lease` from the backup or the
  reflog head.

## If upstream lands a fork feature

Drop the local patch instead of keeping both. Update the patch inventory in
`docs/FORK-PLAN.md`. If upstream's version is close but not identical, port the
remaining fork delta onto it and remove the rest.

## Agent checklist

- [ ] tree clean, backup branch made
- [ ] `git fetch upstream --prune`, divergence inspected
- [ ] `git rebase upstream/main`, conflicts resolved semantically
- [ ] no merge commits; stack still linear
- [ ] `cargo test --workspace` and `scripts/offline-smoke.sh` pass
- [ ] CoreGui/CoreScripts and any touched packaging still work
- [ ] `git push --force-with-lease fork game`
- [ ] report what moved, what conflicted, and what was dropped
