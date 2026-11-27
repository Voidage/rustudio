-- rustudio content pack: the client's PlayerScripts loader.
-- The engine puts this beside PlayerModule in Player.PlayerScripts; requiring
-- the module once starts its camera and controls (Roblox's loader does the
-- same, in its own words).

-- The engine's R15 fallback head (content/avatar/defaultDynamicHead.rbxm)
-- carries a face texture that is transparent everywhere except the face.
-- Drawn as the MeshPart's texture, it hides the head; clear it so the head
-- reads, and keep the visible face the engine draws separately. Only that
-- texture is touched: a custom or avatar head keeps its own.
local FALLBACK_FACE = "11445807951"

local function fixHead(character)
	local head = character:FindFirstChild("Head")
	if head and head:IsA("MeshPart") then
		local texture = tostring(head.TextureContent)
		if string.find(texture, FALLBACK_FACE, 1, true) then
			head.TextureContent = ""
		end
	end
end

local player = game:GetService("Players").LocalPlayer
if player.Character then
	fixHead(player.Character)
end
player.CharacterAdded:Connect(function(character)
	character:WaitForChild("Head", 10)
	fixHead(character)
end)

local playerModule = script.Parent:WaitForChild("PlayerModule")
require(playerModule)
