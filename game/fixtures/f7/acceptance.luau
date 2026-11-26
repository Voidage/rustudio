-- F7 acceptance probe (FORK-PLAN §7.2) for a real, Studio-authored place.
--   rustudio-player PLACE.rbxl --headless --seconds 25 \
--       --luau client game/fixtures/f7/acceptance.luau@5 --log f7.log
-- Prefers Workspace.ANIMATION, else the first KeyframeSequence found under
-- ReplicatedStorage or Workspace. Plays it on the local character through
-- the DIRECT form (humanoid:LoadAnimation(kfs), no asset id) and samples the
-- joints it moves (Motor6D or AnimationConstraint). Success:
-- "F7PROBE track length=..." > 0 and distinct poses > 1 / moved > 0.

local Players = game:GetService("Players")
local RS = game:GetService("ReplicatedStorage")

local clips = {}
for _, root in { RS, workspace } do
    for _, d in root:GetDescendants() do
        if d:IsA("KeyframeSequence") then
            table.insert(clips, d)
        end
    end
end
print("F7PROBE clips=" .. #clips)
for _, kfs in clips do
    local ok, len = pcall(function() return kfs.Length end)
    print(string.format("F7PROBE clip=%s length=%s loop=%s",
        kfs:GetFullName(), tostring(len), tostring(kfs.Loop)))
end

local player = Players.LocalPlayer
if not player then print("F7PROBE no-local-player") return end
local char = player.Character
if not char then
    char = player.CharacterAdded:Wait()
end
local humanoid = char:WaitForChild("Humanoid", 10)
if not humanoid then print("F7PROBE no-humanoid") return end
print("F7PROBE rig=" .. tostring(humanoid.RigType) .. " character=" .. char.Name)

local named = workspace:FindFirstChild("ANIMATION")
local kfs = (named and named:IsA("KeyframeSequence") and named) or clips[1]
if not kfs then print("F7PROBE no-clip") return end
print("F7PROBE using=" .. kfs:GetFullName())

local ok, track = pcall(function() return humanoid:LoadAnimation(kfs) end)
if not ok then
    print("F7PROBE load-failed " .. tostring(track))
    return
end
local anim = track.Animation
print(string.format("F7PROBE track length=%.3f loop=%s id=%s",
    track.Length, tostring(track.Looped), tostring(anim and anim.AnimationId)))

local joints, kinds = {}, {}
for _, d in char:GetDescendants() do
    if d:IsA("Motor6D") or d:IsA("AnimationConstraint") then
        table.insert(joints, d)
        kinds[d.ClassName] = (kinds[d.ClassName] or 0) + 1
    end
end
local kind_list = {}
for k, n in kinds do table.insert(kind_list, k .. "=" .. n) end
print("F7PROBE joints=" .. #joints .. " " .. table.concat(kind_list, ","))

local function snapshot()
    local out = {}
    for _, j in joints do
        local t = j.Transform
        table.insert(out, string.format("%.3f,%.3f,%.3f", t.Position.X, t.Position.Y, t.Position.Z))
    end
    return out, table.concat(out, "|")
end

track:Play(0)
local first, last_s = snapshot()
local distinct = 1
for _ = 1, 30 do
    task.wait(0.05)
    local _, now = snapshot()
    if now ~= last_s then
        distinct += 1
        last_s = now
    end
end
local after, after_s = snapshot()
local moved = 0
for i = 1, #joints do
    if first[i] ~= after[i] then moved += 1 end
end
print(string.format("F7PROBE distinct=%d moved=%d", distinct, moved))
print("F7PROBE done")
