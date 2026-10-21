# M2: игра, скан модов и экран «Моды» — план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Цель:** приложение показывает настоящие моды из папки игры, читает `db.json`, включает и выключает их как игра, строит превью и отдаёт рабочий экран «Моды» с виртуальной сеткой и правой панелью.

**Архитектура:** `bmm-game` отвечает за папки игры, `db.json`, процесс и мост; `bmm-domain` — за `DiskMod`, сверку и догадку названия; `bmm-packs` — за обход папки, превью и обзор архива; Tauri собирает снимок и команды; Svelte рисует сетку.

**Стек:** Rust + zip 9 + sha2 + image; Tauri 2; Svelte 5; существующие `library.json`, `settings.json`.

**Спека:** [2026-10-21-rust-tauri-refactor-design.md](../specs/2026-10-21-rust-tauri-refactor-design.md)

## Общие ограничения

- Windows x64; локально ARM64-хост, x64-сборка через vcvars.
- **FPS не критерий:** мы на виртуалке. Проверяем число карточек в DOM, отсутствие зависаний, загрузку 5000 модов с запасом.
- Данные пользователя не трогаем в тестах: только временные каталоги и фикстуры.
- `db.json` пишется только при закрытой игре, только поле `active`, с `db.json.bmm-bak`, атомарно.
- Удаление модов, «Удалённые» и прошлые версии **переносятся в M3** вместе с копиями и установкой:
  в M2 их нет, чтобы не делать полурабочий откат.
- Все изменения — этапами с зелёными тестами; UI проверяется в браузере и в Tauri.

## Файловая структура

```
crates/bmm-game/src/
  folders.rs      Steam, startup.ini, поиск mods, Resolve, write-check
  info.rs         версия игры из INI/лога, GameCompat
  process.rs      BeamNG.drive.x64 / BeamNG.tech.x64
  db.rs           GameModDb: KeyOf, Parse/ReadStable, Find, SetActive, backup
  bridge.rs       bmm_bridge.zip, commands.json/state.json, alive/send/answer
  sync.rs         GameDbSync: Overlay, ApplyRepoInfo
crates/bmm-domain/src/library/
  disk.rs         DiskMod
  sync.rs         Reconcile, RecordInstalled, NewMod, MigrateMod
crates/bmm-domain/src/text.rs
  filename.rs     FromFileName (порт BindingSuggester)
crates/bmm-packs/src/
  scan.rs         ScanDetailed, ModLayout
  zip_index.rs    нормализованный TOC
  preview.rs      PreviewPicker, ImageHeader, Extract
  cache.rs        PreviewCache: ключи, .jpg/.none, titles.json, prune
  overview.rs     vehicles/levels, info.json, TitleOf
src-tauri/src/
  state.rs        настройки, библиотека, игра, кэш, pending, lock
  commands/library.rs  library_load, library_refresh, mod_details, mod_toggle
  preview.rs      протокол bmm-preview://
src/lib/features/garage/
  GaragePage.svelte, ModGrid.svelte, ModCard.svelte, Inspector.svelte,
  Island.svelte, EmptyState.svelte, garage.svelte.ts
src/lib/ipc/library.ts
```

---

### Задача 1: Папки игры и версия

**Файлы:** `bmm-game/folders.rs`, `bmm-game/info.rs`, `bmm-game/process.rs`, тесты в модулях.

**Интерфейсы:**
- `ModsFolderStatus { None, Empty, NotFound, NotADirectory, NoModsFolder, NotWritable }`
- `resolve_mods_folder(settings, env) -> ResolvedFolder { path: Option<PathBuf>, status, warning: Option<String> }`
- `find_mods_folders(root) -> Vec<PathBuf>`; `default_user_roots()`
- `GameInfo::detect_version(user_folder) -> Option<String>`
- `GameCompat::{parse, compare, short, describe}`
- `GameProcess::is_running() -> bool` (имена `BeamNG.drive.x64`, `BeamNG.tech.x64`)

**Шаги:**
1. Тесты на временных каталогах: `startup.ini` UserPath (абсолютный/относительный/пустой/комментарий), порядок `mods`, `current\mods`, версии 0.39/0.100, `mods` без регистра, кавычки/`%VAR%`, отсутствие и файл вместо папки, проверка записи.
2. Тесты версии: INI приоритетнее лога, `0.39.4.0`, первые 4096 байт лога, мусор → None.
3. Тесты `GameCompat`: 0.38 vs 0.39, 0.38.3 vs 0.38.5 → Same, диапазон 0.36–0.38 → 0.38, строки описания.
4. Реализовать; `sysinfo` для процесса, но чистую проверку имён вынести в функцию и тестировать без системы.
5. Коммит.

---

### Задача 2: `db.json`

**Файлы:** `bmm-game/db.rs`, фикстуры `fixtures/game/*.json`.

**Интерфейсы:**
- `GameModDb { path }`, `read_stable() -> GameDbReadResult`
- `GameModEntry`, `RepoInfo`, `GameDbReadResult::{find, unreadable}`
- `KeyOf(file_name) -> String` (ASCII-lower, без `.zip`)
- `SetActiveOutcome` (все варианты старого enum)
- `set_active(changes, game_running: impl Fn() -> bool) -> SetActiveResult`
- `backup_path` = `db.json.bmm-bak`.

**Шаги:**
1. Тесты на фикстурах: 5 модов, bmw-off, repo-инфо, пустой/битый/массив, мусорная запись, дата/размер, `KeyOf` с кириллицей и repo-путём.
2. Тесты записи: Done, Unchanged, GameRunning, NoDb, NotInDb, Unreadable, ChangedMeanwhile; backup; только active; BOM нет; `.bmm-tmp` нет; batch + отсутствующий файл в NotInDb.
3. Реализовать терпимый `serde_json::Value`-разбор с сохранением неизвестных полей; запись через `serde_json::to_string_pretty`, UTF-8 без BOM, `db.json.bmm-tmp`, `db.json.bmm-bak`, re-check mtime/length.
4. Коммит.

---

### Задача 3: `DiskMod`, догадка названия и сверка

**Файлы:** `bmm-domain/src/library/disk.rs`, `sync.rs`, `text/filename.rs`.

**Интерфейсы:**
- `DiskMod { file_name, enabled, size_bytes, sub_folder: Option<String>, modified_utc: Option<DateTime> }`
- `filename::from_file_name(&str) -> GuessedName { name, author, version }` — порт `BindingSuggester.FromFileName` (site tags, `by … for beamng`, `_`/`-`, версии с точкой/пробелом/v, V6/V8/V12 не версия).
- `sync::reconcile(store, disk, folder_available) -> SyncReport { added, removed, changed, renamed, warning }`
  - правила: включённая копия важнее; пустая папка при непустой библиотеке → skip; unavailable → skip;
  - совпавшие: enabled/size, версия после внешней смены;
  - исчезли/новые + однозначное переименование по `NameKey`;
  - дубли; case-only не изменение.
- `sync::record_installed` (для M3) можно не делать сейчас.

**Шаги:**
1. Тесты `from_file_name` на именах из старых тестов, V8, версии, мусор.
2. Тесты сверки: новый мод, повтор без изменений, удалённый, переименование, неоднозначная пара, пустая папка, регистр, дубли.
3. Реализовать; использовать `TitleNormalizer` и `SizeComparer`.
4. Коммит.

---

### Задача 4: Скан, превью и обзор архива

**Файлы:** `bmm-packs/src/scan.rs`, `zip_index.rs`, `preview.rs`, `cache.rs`, `overview.rs`.

**Интерфейсы:**
- `ModLayout { mods_dir, disabled_dir, backup_dir }` — `InAppFolder` с SHA-256 ключом пути (первые 10 hex), `disabled`/`backups` в `%LOCALAPPDATA%\BeamModManager\<key>`.
- `scan_mods(layout) -> ScanResult { mods: Vec<DiskMod>, folder_available, warnings }`
  - корень, `disabled`, подпапки ≤8, кроме `unpacked`, без reparse, только `.zip`, `bmm_bridge.zip` скрыт; корень важнее подпапки; ошибки пропускаются; сортировка по имени.
- `zip_index::read_index(path) -> Index { entries: Vec<Entry> }` (нормализация `\`, ведущий `/`, отбросить `..`, папки).
- `preview::pick(index) -> Option<PreviewPick>` — порядок 1–6 из старого `PreviewPicker`; не более 40 заголовков; PNG/JPEG размеры без декодирования; текстуры отбрасываются.
- `preview::extract(path, pick) -> Vec<u8>`.
- `cache::PreviewCache::new(root)`, `key(file_name,size,modified)`, `get`, `put_image`, `put_no_image`, `titles`, `prune`.
- `overview::read(path) -> ArchiveOverview { vehicles, levels, has_scripts, lua_files, files, unpacked_bytes, warnings, unreadable }`, `title_of`.

**Шаги:**
1. Добавить `zip = "9"`, `sha2 = "0.10"`, `image = "0.25"` (jpeg/png) в `bmm-packs`; `zip`, `sysinfo` в `bmm-game`.
2. Тесты: скан временной папки с вложенными zip/`.zipx`/`unpacked`/битым; приоритет корня; bridge скрыт.
3. Тесты превью: собрать zip с default.jpg, textures, `(1972)`, 64px, обрезанным заголовком; порядок выбора; `\` пути; кэш ключ/mtime/.none/prune; titles.json.
4. Тесты обзора: одна машина/карта, обёртка, `vehicles/common`, `info.json` (Brand+Name/title), лимит 256 КБ, битый архив.
5. Реализовать; превью хранить как JPEG 616×356 (image resize) или исходник при неудаче; `.none` только при отсутствии картинки, не при ошибке чтения.
6. Коммит.

---

### Задача 5: Tauri — снимок библиотеки, toggle, детали, превью

**Файлы:** `src-tauri/src/state.rs`, `commands/library.rs`, `preview.rs`, `lib.rs`, `tauri.conf.json`, `src/lib/ipc/*`.

**Интерфейсы DTO (camelCase):**
```ts
type ModCard = {
  id: string; name: string; fileName: string; version: string | null;
  sizeBytes: number; enabled: boolean; subFolder: string | null;
  kind: "vehicle" | "level" | "sound" | "script" | "unknown";
  previewUrl: string | null;
  compat: { modFor: string | null; game: string | null; text: string | null; differs: boolean };
  update: "none" | "likely" | "possible"; pending: "none" | "enabling" | "disabling" | "queued";
};
type LibrarySnapshot = {
  mods: ModCard[]; counts: { all, enabled, updates, unchecked, deleted };
  folder: string | null; folderError: string | null; gameVersion: string | null;
  gameRunning: boolean; warning: string | null;
};
```
- `library_load(force: bool) -> LibrarySnapshot` — async, `spawn_blocking`; под общим mutex.
  1. настройки → папка; 2. scan; 3. `db.json` overlay; 4. `reconcile`; 5. repo info; 6. карточки.
- `library_refresh()` — то же с force.
- `mod_toggle(id, enabled) -> ToggleResult { outcome, enabled, message }`
  - игра закрыта: `GameDbSync`/`set_active`; обновить библиотеку; вернуть `done`/`unchanged`/`error`.
  - игра запущена: `queued` + сохранить `pending.json` (M2 fallback; мост — задача 7).
- `mod_details(id) -> ModDetails` — файл, размер, дата, версия, совместимость, overview, привязка, preview.
- превью: `bmm-preview://<key>`; регистрировать протокол в Tauri, отдавать файл из `previews`; в `tauri.conf` CSP `img-src` добавить `bmm-preview:` и `http://bmm-preview.localhost`.
- `app_bootstrap` дополнить `gameVersion`, `folder`, `gameRunning`.
- Capabilities: только core + opener; asset/протокол не даёт fs-доступа.

**Шаги:**
1. Тесты состояния/скана с временной папкой и фейковым `db.json`; Tauri-команды проверить смоук-тестом.
2. Реализовать; `library_load` возвращает до 5000 карточек (компактно, без превью base64).
3. Протокол превью; проверить в окне.
4. Коммит.

---

### Задача 6: Экран «Моды»

**Файлы:** `src/lib/features/garage/*`, `src/routes/(app)/+page.svelte`, `src/lib/ipc/library.ts`, mock.

**UI:**
- Заголовок «Моды» + «N модов · K ждут обновления»; фильтры «Все / Включённые / Нужно обновить / Не проверяются» с числами; поиск по названию и файлу (токены-префиксы, регистр/ё не важны); Ctrl+F, Esc.
- Виртуальная сетка 2–6 колонок; `<ModCard>`: превью/заглушка по типу, название, файл, «версия · размер», плашки «Есть обновление», «Выключен»; выключенная — приглушённая; выбранная — подъём/кромка; фокус с клавиатуры.
- Правая панель: фото, название, автор, размер, установлен, версия, «Для игры» (янтарным при различии), «Файл», «Внутри архива» (машины/карты/скрипты/файлы/размер), «Открыть папку», переключатель включения.
- Кнопки: «Включить/Выключить», «Открыть папку», «Обновить» (пока без загрузки — только уведомление), «Удалить» не показывать до M3.
- Пустые состояния: нет папки/модов/поиска; «Указать папку» (пока открывает системный диалог или объясняет, что настройки в M6).
- Тосты: одна строка, «Отменить» для toggle, 3/6 с; Ctrl+Z.
- Клавиши: стрелки, Space, Enter, Esc, Delete не активен до M3.
- Mock-режим для браузера: 5000 карточек с заглушками, чтобы проверять сетку без игры; в Tauri — реальные данные.
- Не гнаться за FPS; проверить число карточек в DOM (<80), переключение фильтра и поиск без фризов.

**Шаги:**
1. Типы/клиент/mock.
2. Сетка и карточка (адаптировать PoC `ModGrid`/`ModCard`).
3. Панель, фильтры, поиск, клавиатура, тосты.
4. Проверка в браузере на mock и в Tauri на временной папке; снимок.
5. Коммит.

---

### Задача 7: Мост и очередь при запущенной игре

**Файлы:** `bmm-game/bridge.rs`, assets Lua (скопировать из `legacy/src/BeamModManager.Core/Game/Bridge`), `commands/library.rs`, `garage.svelte.ts`.

- `BuildMod()` — детерминированный zip (фиксированная дата 2026-10-15), `EnsureInstalled`.
- `GameBridge::send(map)`, `is_alive`, `read_answer`; те же окна 6 с/2 с/15 с.
- `mod_toggle` при running+alive: оптимистичное состояние, pending, отправка; опрос 6 с, до 2 попыток; `missing`/`stale`/ошибка; события `library://changed`.
- При выходе игры применить pending через `db.json`; `pending.json` сохранять/восстанавливать.
- Тесты: alive 2/10 с, команда/ответ, детерминированный архив, состав, повторная установка, pending.

Если мост не успевает — M2 закрывается без него, но `queued` и `pending.json` работают; в отчёте отметить.

---

### Задача 8: Проверки и статус M2

- `cargo test --workspace`, `clippy -D warnings`, `npm run check`, `npm run build`, `npm run tauri build -- --no-bundle --debug`.
- Дым: временная папка mods с 3 zip и `db.json`; окно показывает карточки, toggle пишет active, второй экземпляр блокируется.
- 5000 моков: сетка, число DOM-карточек, отсутствие зависаний; FPS не смотреть.
- `docs/M2_STATUS.md`: что сделано, что перенесено в M3 (удаление, копии, версии), что осталось в M5 (мост, если не сделан).
- Коммит.

## Самопроверка

- M2 покрывает: папки/игру, db.json, скан, сверку, превью, обзор, снимок, toggle, UI.
- Удаление/версии/копии явно в M3; это осознанный срез, а не забытый пункт.
- FPS не в критериях; проверяем DOM/загрузку/зависания.
- Следующий план — M3: установка архивов, копии, удаление/возврат, версии.
