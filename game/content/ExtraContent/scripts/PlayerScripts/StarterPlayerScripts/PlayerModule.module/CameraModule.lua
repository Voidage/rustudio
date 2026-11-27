-- rustudio CameraModule (content pack v1): a third-person orbit camera.
-- Right mouse turns it, the wheel zooms, and it follows the character's
-- HumanoidRootPart every frame through Workspace.CurrentCamera.
local RunService = game:GetService("RunService")
local UserInputService = game:GetService("UserInputService")

local CameraModule = {}
CameraModule.__index = CameraModule

local MIN_DISTANCE, MAX_DISTANCE = 4, 40
local DEFAULT_DISTANCE = 12.5
local SENSITIVITY = 0.005
local PITCH_LIMIT = 1.4

local function rootOf(player)
	local character = player.Character
	return character and character:FindFirstChild("HumanoidRootPart")
end

function CameraModule.new(player)
	return setmetatable({
		player = player,
		enabled = false,
		rotating = false,
		yaw = 0,
		pitch = -0.25,
		distance = DEFAULT_DISTANCE,
		cameraType = Enum.CameraType.Custom,
		state = "Custom",
	}, CameraModule)
end

function CameraModule:GetCameraType()
	return self.cameraType
end

function CameraModule:SetCameraType(cameraType)
	self.cameraType = cameraType
end

function CameraModule:GetCameraState()
	return self.state
end

function CameraModule:SetCameraState(state)
	self.state = state
end

function CameraModule:GetZoom()
	return self.distance
end

function CameraModule:SetZoom(distance)
	self.distance = math.clamp(distance, MIN_DISTANCE, MAX_DISTANCE)
end

function CameraModule:GetCameraPosition()
	local camera = workspace.CurrentCamera
	return camera and camera.CFrame.Position or Vector3.zero
end

function CameraModule:Enable(enable)
	self.enabled = enable ~= false
	if not self.enabled then
		if self.stepConnection then
			self.stepConnection:Disconnect()
			self.stepConnection = nil
		end
		for _, connection in { self.inputConnection, self.beginConnection, self.endConnection } do
			if connection then
				connection:Disconnect()
			end
		end
		self.inputConnection, self.beginConnection, self.endConnection = nil, nil, nil
		return
	end
	local root = rootOf(self.player)
	if root then
		local _, y = root.CFrame:ToOrientation()
		self.yaw = y
	end
	local camera = workspace.CurrentCamera
	if camera then
		camera.CameraType = Enum.CameraType.Scriptable
	end
	if not self.inputConnection then
		self.inputConnection = UserInputService.InputChanged:Connect(function(input)
			if input.UserInputType == Enum.UserInputType.MouseWheel then
				self:SetZoom(self.distance - input.Position.Z)
			end
		end)
		-- Track the right button from the events: this engine's
		-- UserInputService:IsMouseButtonPressed is a stub that answers false.
		self.beginConnection = UserInputService.InputBegan:Connect(function(input)
			if input.UserInputType == Enum.UserInputType.MouseButton2 then
				self.rotating = true
				-- The engine reports raw mouse deltas only while the cursor
				-- is held (MouseBehavior); Studio's camera does the same.
				UserInputService.MouseBehavior = Enum.MouseBehavior.LockCenter
			end
		end)
		self.endConnection = UserInputService.InputEnded:Connect(function(input)
			if input.UserInputType == Enum.UserInputType.MouseButton2 then
				self.rotating = false
				UserInputService.MouseBehavior = Enum.MouseBehavior.Default
			end
		end)
	end
	if not self.stepConnection then
		self.stepConnection = RunService.RenderStepped:Connect(function()
			self:Update()
		end)
	end
end

function CameraModule:Disable()
	self:Enable(false)
end

function CameraModule:Update()
	local camera = workspace.CurrentCamera
	local root = rootOf(self.player)
	if not camera or not root then
		return
	end
	if camera.CameraType ~= Enum.CameraType.Scriptable then
		camera.CameraType = Enum.CameraType.Scriptable
	end
	if self.rotating then
		local delta = UserInputService:GetMouseDelta()
		self.yaw -= delta.X * SENSITIVITY
		self.pitch = math.clamp(self.pitch - delta.Y * SENSITIVITY, -PITCH_LIMIT, PITCH_LIMIT)
	end
	local focus = root.Position + Vector3.new(0, 1.5, 0)
	local rotation = CFrame.Angles(0, self.yaw, 0) * CFrame.Angles(self.pitch, 0, 0)
	local look = rotation.LookVector
	camera.CFrame = CFrame.lookAt(focus - look * self.distance, focus)
	camera.Focus = CFrame.new(focus)
end

return CameraModule
