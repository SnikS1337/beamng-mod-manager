-- ModManager BeamNG: помощник внутри игры.
-- Включает и выключает моды, пока игра запущена, тем же способом, что и меню модов игры
-- (core_modmanager.activateMod / deactivateMod; игра сама обновляет mods/db.json).
-- Команды приходят от менеджера в файле /bmm_bridge/commands.json папки пользователя игры,
-- ответ и признак жизни — в /bmm_bridge/state.json. Ничего не удаляет и не скачивает.

local M = {}

local dir = '/bmm_bridge/'
local cmdFile = dir .. 'commands.json'
local stateFile = dir .. 'state.json'
local selfName = 'bmm_bridge'
local version = 1
local maxAge = 15 -- секунд: команда старше не выполняется (например, оставшаяся с прошлого запуска)

local lastId = nil
local lastAnswer = nil -- ответ на последнюю команду: повторяется в каждой записи, чтобы признак жизни его не затёр
local pollTimer = 0
local beatTimer = 0

local function writeState()
  if not FS:directoryExists(dir) then FS:directoryCreate(dir, true) end
  local s = { alive = os.time(), version = version, lastId = lastId }
  if lastAnswer then for k, v in pairs(lastAnswer) do s[k] = v end end
  jsonWriteFile(stateFile, s, true)
end

local function answer(a)
  lastAnswer = a
  writeState()
end

local function apply(set)
  local results = {}
  for name, on in pairs(set) do
    local key = string.lower(tostring(name))
    if key == selfName then
      results[key] = 'skipped'
    else
      local mod = core_modmanager.getModDB(key)
      if not mod then
        results[key] = 'missing'
      else
        local active = mod.active ~= false
        if on == true and not active then
          core_modmanager.activateMod(key)
        elseif on ~= true and active then
          core_modmanager.deactivateMod(key)
        end
        results[key] = 'ok'
      end
    end
  end
  return results
end

local function poll()
  if not FS:fileExists(cmdFile) then return end
  local cmd = jsonReadFile(cmdFile)
  if type(cmd) ~= 'table' or cmd.id == nil or cmd.id == lastId then return end
  if not (core_modmanager and core_modmanager.isReady and core_modmanager.isReady()) then return end -- игра ещё грузит моды
  lastId = cmd.id
  if type(cmd.time) ~= 'number' or os.time() - cmd.time > maxAge or type(cmd.set) ~= 'table' then
    answer({ ackId = cmd.id, error = 'stale' })
    return
  end
  local ok, res = pcall(apply, cmd.set)
  if ok then
    answer({ ackId = cmd.id, results = res })
  else
    answer({ ackId = cmd.id, error = tostring(res) })
  end
end

M.onExtensionLoaded = function()
  -- команду, оставшуюся с прошлого раза, не выполняем
  if FS:fileExists(cmdFile) then
    local cmd = jsonReadFile(cmdFile)
    if type(cmd) == 'table' then lastId = cmd.id end
  end
  pcall(writeState)
end

M.onUpdate = function(dtReal)
  local dt = tonumber(dtReal) or 0
  pollTimer = pollTimer + dt
  beatTimer = beatTimer + dt
  if pollTimer >= 0.5 then
    pollTimer = 0
    local ok, err = pcall(poll)
    if not ok then log('E', 'bmmBridge', tostring(err)) end
  end
  if beatTimer >= 2 then
    beatTimer = 0
    pcall(writeState)
  end
end

return M
