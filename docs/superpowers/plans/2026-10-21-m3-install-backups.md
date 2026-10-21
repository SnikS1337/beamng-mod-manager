# M3: установка, копии, удаление, версии и входящие — план

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans.

**Цель:** довести до паритета работу с файлами модов: установка ZIP, обновление, копии, удаление с «Отменить», «Удалённые», прошлые версии и установка из ZIP/RAR/7z через окно входящих.

**Архитектура:** файловые операции — `bmm-packs` (`safe_name`, `inspect`, `install`, `versions`, `incoming`); Tauri — команды и состояние; Svelte — тост с commit/undo, фильтр «Удалённые», блок версий, окно входящих и drag-drop.

**Спека:** [2026-10-21-rust-tauri-refactor-design.md](../specs/2026-10-21-rust-tauri-refactor-design.md)

## Общие ограничения

- Windows x64, но чистые правила имён и путей проверяются и на других ОС.
- Моды — обычные ZIP; удаление/замена всегда оставляет копию; `keep >= 1`.
- Все мутации под общим мьютексом установщика; при ошибке текущий файл не теряется.
- `db.json` не трогается установщиком; включение/выключение остаётся в `bmm-game`.
- Archive safety — эвристика, не антивирус; блокирующие находки не устанавливаются.
- FPS/виртуалка — не критерий; тесты, целостность и краевые случаи важнее.
- Каждое изменение — с тестом; старые C#-тесты — эталон.

## Уже сделано в M3a

- `safe_name.rs`: точный порт `SafeFileName` (пути, `.zip`, запрещённые символы, управляющие,
  U+202E/200E/200F, зарезервированные имена, длина, точка/пробел в конце).
- `inspect.rs` (порт `ArchiveInspector`): лимиты 50 000/20 ГиБ/200×/100 МиБ, zip-slip, ADS,
  управляющие символы, исполняемые, бомба, структура, known roots.
- `install.rs`: `ModInstaller` — install/update/disable/enable/remove/unremove, `.part`,
  потоковый SHA-256, лимит 8 ГиБ, backup `seq_stamp_name`, ротация, restore backup/version.
- `versions.rs`: `PastVersion`, `versions_for`, `deleted`, `restore_version`, `restore_deleted`.
- Тесты на install/backups/rotation/update/remove/unremove/versions.

## Задача 1: завершить backend и прогнать

- Исправить компиляцию после `inspect` (Debug/Clone, точные имена).
- Прогнать `cargo test -p bmm-packs`, добавить недостающие тесты:
  - конфликт файла в mods и disabled; replace; repo-папка не участвует в install;
  - `.part` очищается при ошибке, старые temp по возрасту;
  - backup `desktop.ini`/`Thumbs.db` не считается копией;
  - restore при занятом имени; restore в disabled; откат при сбое;
  - `versions_for` с историей имён и другими текущими модами;
  - `deleted` исключает имена текущих модов и их историю.
- Коммит.

## Задача 2: Tauri

Новые команды:
- `mod_delete(id) -> DeletedTicket { id, file_name, backup_id, was_in_disabled, name }`
- `mod_undelete(id, file_name, backup_id, into_disabled) -> ToggleResult`
- `mod_forget_deleted(id, file_name)` — удалить запись, если файла снова нет; при появлении файла запись остаётся.
- `mod_restore_deleted(file_name, backup_id?)` — последняя копия; новая запись без привязок.
- `mod_versions(id) -> Vec<VersionRow { backup_id, file_name, version, saved_at, size_bytes }>`
- `mod_restore_version(id, backup_id) -> ToggleResult`
- `deleted_list()`/снимок: добавить `deleted` карточки и `counts.deleted`; карточка `fromGameRepo`, `deleted`.
- Guard: repo/подпапка (`sub_folder.is_some()`) удалять/обновлять нельзя; для repo — отказ с текстом.
- Toggle при запущенной игре не меняется; установка/удаление — только при закрытой игре (как старый installer).
- После мутаций — пересборка снимка, чтобы «Удалённые» и счётчики обновлялись сразу.

## Задача 3: UI удаления и версий

- `ToastHost.svelte` + `toast.svelte.ts`: один тост; 3/5/6 с; «Отменить»; пауза под мышью;
  Ctrl+Z; commit при замене/истечении/закрытии окна; имя обрезается до 34.
- Кнопка «Удалить» и Delete в инспекторе/карточке; после удаления выбор сбрасывается и не переходит
  к соседу; тост `Удалён «Имя»` с undo; commit → запись библиотеки удаляется, появляется «Удалённые».
- Фильтр «Удалённые» пятым при `count > 0`; карточка `Удалён`, инспектор «Вернуть», без переключателя.
- «Вернуть» удалённый: тост `Возвращён «Имя»` без undo, новая запись; если список опустел — фильтр «Все».
- `VersionsBlock.svelte` в инспекторе: строки `Версия X`/`Прошлая копия`, дата/размер, «Вернуть»;
  тост `Возвращена версия X` с undo; repo — блока нет.
- Клавиши: Delete, Ctrl+Z, Esc; меню карточки «Удалить»/«Вернуть».

## Задача 4: входящие (M3b)

- `extract.rs`: ZIP безопасно, RAR/7z через `%SystemRoot%\System32\tar.exe` с `-xf/-C`, таймаутом 10 мин,
  контролем размера, kill дерева процессов; пост-проверка symlink/путей.
- `incoming.rs`: `Analyze` одиночного архива и пака; лимиты 6 ГиБ/64 мода; вложенные ZIP до 64;
  `ShapeOf` (мод/обёртка/не мод); перепаковка обёртки; `Match` (SHA-256, имя, состав ≥0.5, NameKey);
  `CleanName`/`UniqueName`; временные папки и очистка; RAR/7z → FromFolder.
- Tauri: `incoming_inspect(paths)`, `incoming_install(items)`, `incoming_cancel`, `incoming_undo(tickets)`,
  `incoming_offer` (скачанные) — по возможности; лимит 8 файлов drag-drop.
- UI: `DropOverlay`, `IncomingDialog`, статусы «Новый/Обновление/Уже установлен/Не мод», проверки,
  «Установить N», прогресс/очередь, тосты и откат всех квитанций.
- Не устанавливать при блокировке/предупреждениях без явного согласия; repo/подпапку не обновлять.

## Задача 5: проверки и статус

- `cargo test --workspace`, `clippy -D warnings`, `npm run check/build`, `tauri build`.
- Ручные сценарии: установка, обновление, удаление/undo/commit, «Удалённые», версии, RAR/7z, занятый файл,
  repo, длинные/русские имена.
- `docs/M3_STATUS.md`: что перенесено, какие тесты, что осталось (мост/сеть).
- CI, коммит, пуш.

## Самопроверка

- Все старые коды/тексты сохранены; копии и удаление обратимы.
- Ни один путь не выходит за mods/disabled/backups; архивные пути проверяются.
- Удаление и версии не теряют привязки до commit; после commit запись удаляется.
- FPS не критерий; проверяем целостность, отсутствие `.part`, количество копий, повторяемость.
