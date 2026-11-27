-- rustudio PlayerModule (content pack v1).
--
-- The public surface games use: GetCameras() and GetControls(). Requiring
-- this module starts both, as Roblox's PlayerScriptsLoader does. The camera
-- drives Workspace.CurrentCamera; the controls write Humanoid.MoveDirection
-- through Humanoid:Move.
local Players = game:GetService("Players")
local player = Players.LocalPlayer

local CameraModule = require(script:WaitForChild("CameraModule"))
local ControlModule = require(script:WaitForChild("ControlModule"))

local cameras = CameraModule.new(player)
local controls = ControlModule.new(player)
cameras:Enable(true)
controls:Enable(true)

local PlayerModule = {}
function PlayerModule:GetCameras()
	return cameras
end
function PlayerModule:GetControls()
	return controls
end
function PlayerModule:GetPlayerModule()
	return script
end
return PlayerModule
