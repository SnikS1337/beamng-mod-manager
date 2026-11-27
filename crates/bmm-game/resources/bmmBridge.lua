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
local chunkSize = 50

local lastId = nil
local off = false -- помощник выключили в меню модов игры: код уже в памяти, но он молчит и ничего не делает
local lastAnswer = nil -- ответ на последнюю команду: повторяется в каждой записи, чтобы признак жизни его не затёр
local active = nil -- текущая пачка: между кадрами игра успевает отрисоваться и ответить heartbeat
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

local function startApply(cmd)
  local keys = {}
  for name in pairs(cmd.set) do keys[#keys + 1] = name end
  table.sort(keys)
  active = {
    id = cmd.id,
    set = cmd.set,
    keys = keys,
    index = 1,
    results = {},
  }
end

local function stepApply()
  if not active then return end
  local job = active
  local start = job.index
  local finish = math.min(start + chunkSize - 1, #job.keys)
  for index = start, finish do
    local name = job.keys[index]
    local on = job.set[name]
    local key = string.lower(tostring(name))
    if key == selfName then
      job.results[key] = 'skipped'
    else
      local mod = core_modmanager.getModDB(key)
      if not mod then
        job.results[key] = 'missing'
      else
        local enabled = mod.active ~= false
        if on == true and not enabled then
          core_modmanager.activateMod(key)
        elseif on ~= true and enabled then
          core_modmanager.deactivateMod(key)
        end
        job.results[key] = 'ok'
      end
    end
  end
  job.index = finish + 1
  if finish < #job.keys then
    lastAnswer = { busy = true, busyId = job.id, done = finish, total = #job.keys }
    writeState()
  else
    local results = job.results
    active = nil
    answer({ ackId = job.id, results = results })
  end
end

local function poll()
  if active then return end
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
  startApply(cmd)
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
  if not off and active then
    local id = active.id
    local ok, err = pcall(stepApply)
    if not ok then
      active = nil
      answer({ ackId = id, error = tostring(err) })
    end
  end
  -- выключенный помощник не подаёт признаков жизни: через 6 с менеджер перестаёт им пользоваться
  if beatTimer >= 2 then
    beatTimer = 0
    if not off then pcall(writeState) end
  end
end

return M
