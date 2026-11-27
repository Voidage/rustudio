//! Legacy in-memory DataStores, isolated for explicit local testing.
pub const SOURCE: &str = r###"local local_mode, create, set_props, datastores, json = ...
local M = __rs_lua_methods
---------------------------------------------------------- DataStoreService
-- Explicit local test mode only; never used as a successful live backend.
-- With `datastores` (RUSTUDIO_DATASTORE_DIR) records persist on disk: one JSON file
-- per key under <root>/<store>/<scope>, written atomically by the native
-- bridge. Versions, key info and the method semantics stay the same.
local stores = {}
local warned_store = false
local persist = datastores ~= nil
local SCHEMA = 1
local loaded = setmetatable({}, { __mode = "k" }) -- store -> key -> true
local function copy(v, depth)
    if type(v) ~= "table" or (depth or 0) > 32 then return v end
    local t = {}
    for k, x in pairs(v) do t[k] = copy(x, (depth or 0) + 1) end
    return t
end
local DataStore = {}
DataStore.__index = DataStore
-- Standard stores keep Studio's per-key record (value, version, times, user
-- ids, metadata) and answer with a DataStoreKeyInfo, as the cloud backend
-- (data_store.rs) does: ProfileStore's session start needs `key_info` from
-- UpdateAsync, and without it a game never spawned the player
-- (2026-11-21). Ordered stores return values only, as in Studio.
local entries = setmetatable({}, { __mode = "k" }) -- store -> key -> record
local infos = setmetatable({}, { __mode = "k" }) -- DataStoreKeyInfo -> {metadata, ids}
local version_n = 0
local function now_ms() if DateTime then return DateTime.now().UnixTimestampMillis end return math.floor(os.time() * 1000) end
local function ordered(self) return string.sub(self.name, 1, 8) == "ordered:" end
local function record(self, key)
    entries[self] = entries[self] or {}
    return entries[self][tostring(key)]
end
local function key_info(rec)
    if not rec or not create then return nil end
    local o = create("DataStoreKeyInfo")
    if set_props then set_props(o, { Version = rec.version, CreatedTime = rec.created, UpdatedTime = rec.updated }) end
    infos[o] = { metadata = copy(rec.metadata or {}), ids = copy(rec.ids or {}) }
    return o
end
local function mark_loaded(self, key)
    loaded[self] = loaded[self] or {}
    loaded[self][key] = true
end
local function is_loaded(self, key)
    local l = loaded[self]
    return l ~= nil and l[key] == true
end
local function decode_saved(text)
    local ok, rec = pcall(function() return game:GetService("HttpService"):JSONDecode(text) end)
    if not ok or type(rec) ~= "table" then return nil end
    if rec.schema ~= SCHEMA then return false end
    return rec
end
-- Load one key from disk on first use. A missing key stays missing; a
-- corrupt or newer record is loud instead of being silently dropped.
local function ensure_loaded(self, key)
    key = tostring(key)
    if not persist or is_loaded(self, key) then return end
    local text = datastores.read(self.name, self.scope, key)
    if text == nil then mark_loaded(self, key); return end
    local rec = decode_saved(text)
    if rec == false then
        error('Saved data for key "' .. key .. '" uses a newer schema', 0)
    elseif rec == nil then
        error('Saved data for key "' .. key .. '" is corrupt', 0)
    end
    entries[self] = entries[self] or {}
    entries[self][key] = {
        version = rec.version,
        created = rec.created,
        updated = rec.updated,
        ids = copy(rec.ids or {}),
        metadata = copy(rec.metadata or {}),
    }
    self.data[key] = copy(rec.value)
    mark_loaded(self, key)
end
local function ensure_all(self)
    if not persist then return end
    for _, key in ipairs(datastores.keys(self.name, self.scope)) do ensure_loaded(self, key) end
end
-- Persist before changing memory: a failed write leaves the store as it was.
local function persist_write(self, key, value, rec)
    if not persist then return end
    datastores.write(self.name, self.scope, key, json({
        schema = SCHEMA,
        key = key,
        value = value,
        version = rec and rec.version,
        created = rec and rec.created,
        updated = rec and rec.updated,
        ids = rec and rec.ids or {},
        metadata = rec and rec.metadata or {},
    }))
end
local function write(self, key, value, ids, metadata)
    key = tostring(key)
    entries[self] = entries[self] or {}
    local old = entries[self][key]
    local t = now_ms()
    version_n += 1
    local version = string.format("%016X.%010d.%016X.01", t, version_n, t)
    local rec = {
        version = version,
        created = old and old.created or t,
        updated = t,
        ids = copy(ids or {}),
        metadata = copy(metadata or {}),
    }
    persist_write(self, key, value, rec)
    entries[self][key] = rec
    self.data[key] = copy(value)
    mark_loaded(self, key)
    return rec
end
local function options_metadata(options)
    if typeof(options) == "Instance" then
        local ok, m = pcall(function() return options:GetMetadata() end)
        if ok then return m end
    end
    return nil
end
local priorMetadata, priorUserIds = M.GetMetadata, M.GetUserIds
M.GetMetadata = function(self, ...)
    if infos[self] then return copy(infos[self].metadata) end
    if priorMetadata then return priorMetadata(self, ...) end
    error("DataStoreKeyInfo has no metadata", 0)
end
M.GetUserIds = function(self, ...)
    if infos[self] then return copy(infos[self].ids) end
    if priorUserIds then return priorUserIds(self, ...) end
    error("DataStoreKeyInfo has no user ids", 0)
end
function DataStore:GetAsync(key)
    key = tostring(key)
    ensure_loaded(self, key)
    local value = copy(self.data[key])
    if ordered(self) or value == nil then return value end
    return value, key_info(record(self, key))
end
local function notify(self,key,value)
    local event=self.events and self.events[tostring(key)];if event then event:Fire(copy(value))end
end
function DataStore:SetAsync(key, value, ids, options)
    key = tostring(key)
    if ordered(self) then
        persist_write(self, key, value, nil)
        self.data[key] = copy(value)
        mark_loaded(self, key)
        notify(self, key, value)
        return
    end
    local rec = write(self, key, value, ids, options_metadata(options))
    notify(self, key, value)
    return rec.version
end
function DataStore:OnUpdate(key,callback)
    self.events=self.events or{};key=tostring(key)
    ensure_loaded(self, key)
    if not self.events[key]then self.events[key]=Instance.new('BindableEvent')end
    local connection=self.events[key].Event:Connect(function(v)callback(copy(v))end)
    if self.data[key]~=nil then local value=copy(self.data[key]);task.defer(function()if connection.Connected then callback(value)end end)end
    return connection
end
function DataStore:BatchGetAsync(keys)
    if string.sub(self.name,1,8)~='ordered:'then error('{"errorType":"BatchGetUnsupportedOperation","errorMessage":"BatchGetAsync is not supported.","callbackMessage":"BatchGetAsync is not supported."}',0)end
    if #keys==0 then error('OrderedDataStore BatchGetAsync keys array cannot be empty.',0)end
    if #keys>100 then error('OrderedDataStore BatchGetAsync exceeded the 100 key limit.',0)end
    local out={};for _,key in ipairs(keys)do if type(key)~='string'then error('513: Attribute keys format is invalid',0)end;if self.data[key]~=nil then out[key]={value=copy(self.data[key])}end end;return out
end
function DataStore:UpdateAsync(key, fn)
    key = tostring(key)
    ensure_loaded(self, key)
    local current = copy(self.data[key])
    if ordered(self) then
        local new = fn(current)
        if new ~= nil then
            persist_write(self, key, new, nil)
            self.data[key] = copy(new)
            mark_loaded(self, key)
            notify(self, key, new)
        end
        return new
    end
    local rec = record(self, key)
    local new, ids, metadata = fn(current, current ~= nil and key_info(rec) or nil)
    -- nil cancels the update: nothing is written, nothing comes back.
    if new == nil then return nil end
    rec = write(self, key, new, ids, metadata)
    notify(self,key,new)
    return copy(new), key_info(rec)
end
function DataStore:IncrementAsync(key, delta)
    key = tostring(key)
    ensure_loaded(self, key)
    local v = (tonumber(self.data[key]) or 0) + (tonumber(delta) or 1)
    if ordered(self) then
        persist_write(self, key, v, nil)
        self.data[key] = v
        mark_loaded(self, key)
    else
        write(self, key, v)
    end
    notify(self,key,v)
    return v
end
function DataStore:RemoveAsync(key)
    key = tostring(key)
    ensure_loaded(self, key)
    local old = self.data[key]
    local rec = not ordered(self) and record(self, key) or nil
    if persist then datastores.remove(self.name, self.scope, key) end
    self.data[key] = nil
    if entries[self] then entries[self][key] = nil end
    mark_loaded(self, key)
    notify(self,key,nil)
    if ordered(self) or old == nil then return old end
    return old, key_info(rec)
end
function DataStore:GetSortedAsync(ascending, page_size)
    ensure_all(self)
    local rows = {}
    for k, v in pairs(self.data) do table.insert(rows, { key = k, value = v }) end
    table.sort(rows, function(a, b)
        local x, y = tonumber(a.value) or 0, tonumber(b.value) or 0
        if ascending then return x < y end
        return x > y
    end)
    -- A real DataStorePages through the shared Pages (src/domains/pages.rs).
    local size, pages = tonumber(page_size) or 50, {}
    for i = 1, #rows, size do
        local page = {}
        for j = i, math.min(i + size - 1, #rows) do table.insert(page, rows[j]) end
        table.insert(pages, page)
    end
    return __rs_pages_list("DataStorePages", pages)
end
local function get_store(name, scope)
    if not persist and not warned_store then
        warned_store = true
        warn("DataStores in rustudio keep data in memory for this play session only")
    end
    scope = scope or "global"
    local key = tostring(name) .. "/" .. tostring(scope)
    stores[key] = stores[key] or setmetatable({ name = name, scope = scope, data = {} }, DataStore)
    return stores[key]
end
-- Like Studio: an unpublished place (PlaceId 0) can't use DataStores.
-- The message has no position, as an engine error (Studio: "DataStore
-- unavailable: You must publish …" from a pcall).
local function published()
    if local_mode then return end
    if (tonumber(game.PlaceId) or 0) == 0 then
        error("You must publish this place to the web to access DataStore.", 0)
    end
    error("rustudio: session DataStore backend is not implemented", 0)
end
function M.GetDataStore(self, name, scope) published() return get_store(name, scope) end
function M.GetGlobalDataStore(self) published() return get_store("__global") end
function M.GetOrderedDataStore(self, name, scope) published() return get_store("ordered:" .. tostring(name), scope) end

"###;
