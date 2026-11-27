-- Content-pack smoke probe: the PlayerModule surface, camera follow, and
-- movement while the harness holds W. Run on the client:
--   rustudio-player PLACE --headless --seconds 10 --key W@4:2 \
--       --luau client game/fixtures/pack/probe.luau@2 --log out.log
-- A pass is module=true, controls=true, cameras=true, moved > 2 and a
-- camera at a third-person distance.
local Players = game:GetService("Players")
local player = Players.LocalPlayer
local char = player.Character or player.CharacterAdded:Wait()
local humanoid = char:WaitForChild("Humanoid")
local root = char:WaitForChild("HumanoidRootPart")
local camera = workspace.CurrentCamera

local playerScripts = player:FindFirstChild("PlayerScripts")
local playerModuleInstance = playerScripts and playerScripts:FindFirstChild("PlayerModule")
local playerModule = nil
if playerModuleInstance then
    local ok, module = pcall(require, playerModuleInstance)
    if ok then
        playerModule = module
    end
end
local controls = playerModule and playerModule:GetControls()
local cameras = playerModule and playerModule:GetCameras()
local head = char:FindFirstChild("Head")
print(string.format("PACKPROBE module=%s controls=%s cameras=%s head=%s",
    tostring(playerModule ~= nil), tostring(controls ~= nil), tostring(cameras ~= nil),
    tostring(head and head.ClassName)))

local start = root.Position
local closest, farthest = math.huge, 0
for _ = 1, 20 do
    task.wait(0.25)
    local distance = (camera.CFrame.Position - root.Position).Magnitude
    closest = math.min(closest, distance)
    farthest = math.max(farthest, distance)
end
local moved = root.Position - start
print(string.format("PACKPROBE moved=%.2f cam=%.2f..%.2f", Vector3.new(moved.X, 0, moved.Z).Magnitude, closest, farthest))
print("PACKPROBE done")
