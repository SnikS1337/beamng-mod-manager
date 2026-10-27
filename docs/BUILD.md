# Сборка и проверки

Приложение: Rust + Tauri 2 + SvelteKit 3 (SPA) + Svelte 5 + Tailwind CSS 4.
Старый C#/WPF лежит в `legacy/` и в новой сборке и CI не участвует.

## Что нужно

- Windows 10/11 с Microsoft Edge WebView2 Runtime (обычно уже есть; при отсутствии
  установить отдельно — программа покажет ссылку при запуске);
- Node.js 24 (SvelteKit 3 требует минимум 22.17);
- Rust 1.95+ с target `x86_64-pc-windows-msvc`;
- Visual Studio Build Tools 2022: workload «C++ build tools» и Windows SDK.

TypeScript остаётся 6.0.x: svelte-check пока не поддерживает TypeScript 7.
`components.json` ссылается на старый алиас `$lib`; готовые shadcn-svelte-компоненты
лежат в `src/lib/components/ui` и собираются без CLI. Новые компоненты через CLI
добавлять после проверки, что он понимает SvelteKit 3 (конфиг в `vite.config.js`,
импорты `#lib/…`). Dependabot (`.github/dependabot.yml`) раз в неделю проверяет
npm, Cargo и GitHub Actions; PR идут в `refactor-rust-poc`.

## Обычная сборка (x64)

```bash
npm ci
npm run check
npm test
npm run build

cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets

npm run tauri dev
npm run tauri build -- --no-bundle
```

Отладочный бинарник — `target/debug/bmm-desktop.exe`; release-сборка Tauri кладёт
`target/release/BeamModManager.exe`. Установщик не используется: выпуск — portable
zip с exe и `latest.json`.

На 2026-10-27 проходят 655 тестов Rust (1 ignored) и 90 тестов фронтенда.

## Если хост ARM64

Если Rust установлен как `aarch64-pc-windows-msvc`, а MSVC ARM64-тулсета нет,
собирайте x64 — он же релизный:

```bash
rustup toolchain install stable-x86_64-pc-windows-msvc --force-non-host --profile minimal
rustup component add rustfmt clippy --toolchain stable-x86_64-pc-windows-msvc
```

Запускайте команды из окружения MSVC x64. Можно через временный `.bat`:

```bat
@echo off
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvarsall.bat" amd64 >nul
if errorlevel 1 exit /b 1
cargo +stable-x86_64-pc-windows-msvc %*
```

Для Tauri добавьте `set RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-msvc` и запускайте
`npm run tauri build -- --no-bundle --debug`, иначе Tauri возьмёт ARM64-хост.

## Почему имена крейтов без «update», «install», «setup»

Windows UAC считает exe с таким словом в имени установщиком и требует повышение
(ошибка 740). Поэтому крейты называются `bmm-app-release`, `bmm-packs`,
`bmm-checks`, а команды и тексты могут оставаться про обновления и установку.

## CI

`.github/workflows/ci.yml` — `windows-2025`, x64: `npm ci`, `svelte-check`, Vitest,
сборка фронтенда, `cargo fmt`, `clippy -D warnings`, `cargo test`. Полная сборка окна
запускается только вручную (`workflow_dispatch`), потому что release-сборка с LTO
долгая; выпуск всё равно проверяется отдельным workflow.

Старый C# workflow лежит в `legacy/.github/workflows/` и GitHub его не запускает.

## Релиз и обновление программы

`.github/workflows/release.yml` запускается на тег `v*`. Перед тегом версия должна
совпадать в `Cargo.toml`, `package.json` и `src-tauri/tauri.conf.json`; workflow
проверяет это и падает при расхождении. Он же требует непустой раздел `## <версия>`
в `CHANGES.md`.

Сборка делает `BeamModManager-<версия>.zip`, считает SHA-256, пишет `latest.json`
и публикует GitHub Release. Приложение читает `releases/latest/download/latest.json`,
скачивает архив, проверяет размер и хеш и после кнопки «Перезапустить» заменяет exe
через отдельный процесс `--install-update` с откатом.

В манифестах **1.0.4**, в `CHANGES.md` описан этот выпуск. Тег `v1.0.4` собирается
workflow `release.yml` из ветки `work/2026-10-24`; `main` и `refactor-rust-poc` не трогаются.

## Значок

Исходники лежат в `design/icon`: `python3 tools/icon/make_icon.py` перерисовывает SVG и
PNG-размеры (нужны Playwright с Chromium и Pillow). Набор для приложения в
`src-tauri/icons` (ICO, ICNS, PNG и значки Store) собирается из крупного исходника:

```bash
npm run tauri icon -- design/icon/app-large.svg
```

Файлы в `src-tauri/icons` вручную не правим — при следующей сборке значка они заменятся.

## Данные при проверках

Приложение использует `%LOCALAPPDATA%\BeamModManager` (тот же каталог, что старая
версия). Тесты домена и архивов пишут только во временные каталоги. Реальная папка
модов и игра для ручной проверки: `.dev\mods`/`.dev\mds` в Desktop-worktree и
`C:\Users\Admin\Desktop\BeamNG.drive`.

Для сбора сведений о BeamNG на другой машине есть `tools/windows/collect-beamng-info.ps1`:
он только читает файлы игры и складывает zip на рабочем столе.
