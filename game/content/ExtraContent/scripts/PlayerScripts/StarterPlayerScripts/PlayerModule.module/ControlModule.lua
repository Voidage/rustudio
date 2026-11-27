-- rustudio ControlModule (content pack v1): WASD / arrows move the
-- character relative to the camera, Space jumps. The engine's Humanoid:Move
-- sets MoveDirection, which its client-server character step reads.
local UserInputService = game:GetService("UserInputService")
local RunService = game:GetService("RunService")

local ControlModule = {}
ControlModule.__index = ControlModule

local MOVE = {
	[Enum.KeyCode.W] = Vector3.new(0, 0, -1),
	[Enum.KeyCode.S] = Vector3.new(0, 0, 1),
	[Enum.KeyCode.A] = Vector3.new(-1, 0, 0),
	[Enum.KeyCode.D] = Vector3.new(1, 0, 0),
	[Enum.KeyCode.Up] = Vector3.new(0, 0, -1),
	[Enum.KeyCode.Down] = Vector3.new(0, 0, 1),
	[Enum.KeyCode.Left] = Vector3.new(-1, 0, 0),
	[Enum.KeyCode.Right] = Vector3.new(1, 0, 0),
}

local function humanoidOf(player)
	local character = player.Character
	return character and character:FindFirstChildOfClass("Humanoid")
end

function ControlModule.new(player)
	return setmetatable({ player = player, enabled = false }, ControlModule)
end

function ControlModule:GetMoveVector()
	local move = Vector3.zero
	for keyCode, direction in MOVE do
		if UserInputService:IsKeyDown(keyCode) then
			move += direction
		end
	end
	if move.Magnitude > 1 then
		move = move.Unit
	end
	return move
end

function ControlModule:IsMoveDirectionFromKeyCode(keyCode)
	return MOVE[keyCode] ~= nil
end

function ControlModule:Enable(enable)
	self.enabled = enable ~= false
	if self.enabled and not self.stepConnection then
		self.stepConnection = RunService.RenderStepped:Connect(function()
			local humanoid = humanoidOf(self.player)
			if not humanoid then
				return
			end
			humanoid:Move(self:GetMoveVector(), true)
			-- Holding Space keeps asking to jump, as Roblox does: the
			-- character hops again the moment it lands.
			if UserInputService:IsKeyDown(Enum.KeyCode.Space) then
				humanoid.Jump = true
			end
		end)
	elseif not self.enabled then
		if self.stepConnection then
			self.stepConnection:Disconnect()
			self.stepConnection = nil
		end
	end
end

function ControlModule:Disable()
	self:Enable(false)
end

return ControlModule
