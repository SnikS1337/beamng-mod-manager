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
local maxAge = 60 -- секунд: команда старше не выполняется (например, оставшаяся с прошлого запуска)

local lastId = nil
local off = false -- помощник выключили в меню модов игры: код уже в памяти, но он молчит и ничего не делает
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
  -- Активация большого мода блокирует onUpdate дольше признака жизни:
  -- менеджер должен видеть, что команда выполняется, а не что мост умер.
  lastAnswer = { busy = true, busyId = cmd.id }
  writeState()
  local ok, res = pcall(apply, cmd.set)
  if ok then
    answer({ ackId = cmd.id, results = res })
  else
    answer({ ackId = cmd.id, error = tostring(res) })
  end
end

-- выключили в меню модов игры (архив отключён, а расширение осталось загруженным до перезапуска)
local function switchedOff()
  if not (core_modmanager and core_modmanager.getModDB) then return false end
  local me = core_modmanager.getModDB(selfName)
  return type(me) == 'table' and me.active == false
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
    local okOff, isOff = pcall(switchedOff)
    off = okOff and isOff
    if not off then
      local ok, err = pcall(poll)
      if not ok then log('E', 'bmmBridge', tostring(err)) end
    end
  end
  -- выключенный помощник не подаёт признаков жизни: через 6 с менеджер перестаёт им пользоваться
  if beatTimer >= 2 then
    beatTimer = 0
    if not off then pcall(writeState) end
  end
end

return M
