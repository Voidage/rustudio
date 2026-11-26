# F7 acceptance — direct KeyframeSequence playback

`acceptance.luau` proves the fork feature on a real place: a
`KeyframeSequence` saved in the DataModel plays through
`humanoid:LoadAnimation(kfs)`, with no upload and no asset id.

```sh
cargo build --release -p rustudio-player
target/release/rustudio-player PLACE.rbxl --headless --seconds 25 \
    --luau client game/fixtures/f7/acceptance.luau@5 --log f7.log
grep F7PROBE f7.log
```

The probe prefers `Workspace.ANIMATION`, otherwise the first
`KeyframeSequence` under `ReplicatedStorage` or `Workspace`. It needs a
spawnable R15/R6 character and prints:

- `F7PROBE clip=…` for each clip (full name, length, loop);
- `F7PROBE track length=… loop=… id=rbxtemp://kfs/…` — the direct track;
- `F7PROBE joints=… AnimationConstraint=…` (or `Motor6D=…`) — the rig's
  joints;
- `F7PROBE distinct=… moved=…` — how many poses and joints changed while
  the clip played.

A pass is a real `track length` and `moved > 0` (and `distinct > 2` for
a moving clip). A studio-authored R15 clip made by the user on 2026-11-26
(`Workspace.ANIMATION`, length 1, loop) gave 15 `AnimationConstraint`
joints, `distinct=20 moved=12`.

The place itself is not committed; point the command at any place with a
saved KeyframeSequence.
