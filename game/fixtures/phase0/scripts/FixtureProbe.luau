-- Phase 0 fixture (FORK-PLAN §12, Phase 0): a KeyframeSequence saved in the
-- place plays on the spawned character through the current register-then-
-- LoadAnimation form. (The direct humanoid:LoadAnimation(kfs) form is F7,
-- Phase 1.) It also checks that the bundle's cached texture and sound load
-- with no download, and that the shoulder's Motor6D.Transform really moves.
--
-- Runs as a client LocalScript in StarterPlayerScripts, the way Roblox's own
-- Animate script plays clips. Prints "PROBE ..." lines that
-- scripts/phase0-smoke.sh asserts on; a failed check errors into the Output,
-- which `--fail-on-error` turns into a nonzero exit.

local Players = game:GetService("Players")
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local ContentProvider = game:GetService("ContentProvider")

-- The ids seeded into <bundle>/cache/assets by scripts/phase0-bundle.sh.
local TEXTURE = "rbxassetid://900000001"
local SOUND = "rbxassetid://900000002"

local function wait_for(predicate, seconds, what)
	local deadline = os.clock() + seconds
	repeat
		local value = predicate()
		if value then
			return value
		end
		task.wait(0.1)
	until os.clock() > deadline
	error(what .. " did not appear", 0)
end

local player = Players.LocalPlayer
local character = player.Character
if not character then
	character = player.CharacterAdded:Wait()
end
local humanoid = character:WaitForChild("Humanoid")

-- Both ids are already in the bundle's cache, which the engine checks first:
-- these must report Success without the network.
ContentProvider:PreloadAsync({ TEXTURE, SOUND })
local texture_status = ContentProvider:GetAssetFetchStatus(TEXTURE).Name
local sound_status = ContentProvider:GetAssetFetchStatus(SOUND).Name
print(("PROBE texture=%s sound=%s"):format(texture_status, sound_status))
assert(texture_status == "Success", "the bundled texture was not available")
assert(sound_status == "Success", "the bundled sound was not available")

-- Roblox-compatible playback: register the in-place KeyframeSequence, wrap
-- the returned rbxtemp id in an Animation, load it on the Humanoid.
local animations = ReplicatedStorage:WaitForChild("Animations")
local sequence = animations:WaitForChild("Wave")
local animation = Instance.new("Animation")
animation.AnimationId = game:GetService("KeyframeSequenceProvider"):RegisterKeyframeSequence(sequence)
local track = humanoid:LoadAnimation(animation)
track.Looped = true
track.Priority = Enum.AnimationPriority.Action
track:Play(0)
print(("PROBE track length=%.3f playing=%s"):format(track.Length, tostring(track.IsPlaying)))
assert(track.Length > 0.5, "the KeyframeSequence gave the track no length")
assert(track.IsPlaying, "the track did not start")

-- The joint differs by rig: R15 names it RightUpperArm, R6 "Right Arm".
local limb = wait_for(function()
	return character:FindFirstChild("RightUpperArm") or character:FindFirstChild("Right Arm")
end, 10, "the right arm")
local joint = wait_for(function()
	for _, descendant in character:GetDescendants() do
		if descendant:IsA("Motor6D") and descendant.Part1 == limb then
			return descendant
		end
	end
end, 10, "the shoulder Motor6D")

local first = joint.Transform
local moved, poses = 0, {}
-- A cold first frame (shader/pipeline warm-up, a busy machine) can delay the
-- Animator's first steps: sample until the joint has actually moved instead
-- of assuming 2.5 s of wall clock is enough.
for _ = 1, 60 do
	task.wait(0.1)
	local pose = joint.Transform
	if pose ~= first then
		moved += 1
	end
	poses[tostring(pose)] = true
	if moved >= 5 then
		break
	end
end
local distinct = 0
for _ in pairs(poses) do
	distinct += 1
end
print(("PROBE joint=%s moved=%d distinct=%d time=%.2f"):format(joint.Name, moved, distinct, track.TimePosition))
assert(moved > 0, "Motor6D.Transform never changed: the rig is not animating")
assert(distinct > 2, "the pose never varied: the track is static")

-- The Sound has Playing = true in the place; its asset must decode.
local speaker = wait_for(function()
	return workspace:FindFirstChild("Speaker")
end, 10, "the Speaker part")
local sound = wait_for(function()
	return speaker:FindFirstChild("Theme")
end, 10, "the Theme sound")
task.wait(1)
print(("PROBE sound playing=%s length=%.2f"):format(tostring(sound.IsPlaying), sound.TimeLength))
assert(sound.TimeLength > 0, "the bundled sound never decoded")
assert(sound.IsPlaying, "the Sound is not playing")

print("PROBE done")
