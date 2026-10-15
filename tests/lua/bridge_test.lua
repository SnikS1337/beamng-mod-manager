-- Проверка помощника внутри игры (src/BeamModManager.Core/Game/Bridge) на подделке API BeamNG.
-- Запуск: lua5.1 tests/lua/bridge_test.lua src/BeamModManager.Core/Game/Bridge
local files = {}
local now = 1760000000
os.time = function() return now end
FS = {
  fileExists = function(_, p) return files[p] ~= nil end,
  directoryExists = function(_, p) return true end,
  directoryCreate = function() end,
}
jsonReadFile = function(p) return files[p] end
jsonWriteFile = function(p, d) files[p] = d end
log = function(...) print(...) end
local mods = { bmw = { active = true }, map = { active = false }, bmm_bridge = { active = true } }
local calls = {}
core_modmanager = {
  isReady = function() return true end,
  getModDB = function(n) return mods[n] end,
  activateMod = function(n) table.insert(calls, '+' .. n); mods[n].active = true end,
  deactivateMod = function(n) table.insert(calls, '-' .. n); mods[n].active = false end,
}
local loaded = {}
extensions = { load = function(n) loaded[n] = true end }
setExtensionUnloadMode = function() end
package.path = arg[1] .. '/?.lua;' .. package.path

local function check(c, msg) if not c then error('FAIL: ' .. msg, 2) end print('ok  ' .. msg) end

-- старая команда с прошлого раза
files['/bmm_bridge/commands.json'] = { id = 'old', time = now - 100, set = { bmw = false } }
local B = dofile(arg[1] .. '/bmmBridge.lua')
B.onExtensionLoaded()
check(files['/bmm_bridge/state.json'].alive == now, 'признак жизни при загрузке')
B.onUpdate(0.6)
check(#calls == 0, 'команда с прошлого запуска не выполняется')

-- новая команда
files['/bmm_bridge/commands.json'] = { id = 'c1', time = now, set = { BMW = false, map = true, bmm_bridge = false, ghost = true } }
B.onUpdate(0.6)
check(table.concat(calls, ',') == '-bmw,+map' or table.concat(calls, ',') == '+map,-bmw', 'выключил bmw, включил map: ' .. table.concat(calls, ','))
local st = files['/bmm_bridge/state.json']
check(st.ackId == 'c1', 'ответ на команду')
check(st.results.bmm_bridge == 'skipped' and st.results.ghost == 'missing' and st.results.bmw == 'ok', 'себя не трогает, неизвестный мод отмечен')
check(mods.bmm_bridge.active == true, 'помощник себя не выключил')

-- та же команда повторно — ничего
B.onUpdate(0.6)
check(#calls == 2, 'повтор той же команды не выполняется')

-- устаревшая
files['/bmm_bridge/commands.json'] = { id = 'c2', time = now - 60, set = { bmw = true } }
B.onUpdate(0.6)
check(#calls == 2 and files['/bmm_bridge/state.json'].error == 'stale', 'устаревшая команда отклонена')

-- игра ещё не готова
core_modmanager.isReady = function() return false end
files['/bmm_bridge/commands.json'] = { id = 'c3', time = now, set = { bmw = true } }
B.onUpdate(0.6)
check(#calls == 2, 'пока модменеджер не готов — ждём')
core_modmanager.isReady = function() return true end
B.onUpdate(0.6)
check(calls[3] == '+bmw', 'дождался и выполнил')

-- признак жизни каждые 2 с
now = now + 5
B.onUpdate(2.1)
check(files['/bmm_bridge/state.json'].alive == now, 'признак жизни обновляется')

-- сбой в модменеджере не роняет помощник
core_modmanager.deactivateMod = function() error('boom') end
files['/bmm_bridge/commands.json'] = { id = 'c4', time = now, set = { bmw = false } }
B.onUpdate(0.6)
check(files['/bmm_bridge/state.json'].error ~= nil, 'ошибка записана в ответ')

-- скрипт мода
dofile(arg[1] .. '/modScript.lua')
check(loaded.bmmBridge, 'modScript загружает расширение')
print('всё')
