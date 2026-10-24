# Сборка и проверки

Новое приложение: Rust + Tauri 2 + SvelteKit 2 (SPA) + Svelte 5 + Tailwind CSS 4.
Старый C#/WPF лежит в `legacy/` и не собирается в новом CI.

## Что нужно

- Windows 10/11 с WebView2 (на целевой машине уже есть);
- Node.js 24;
- Rust stable (минимум 1.95 — его требует sysinfo) с target `x86_64-pc-windows-msvc`;
- Visual Studio Build Tools 2022: workload «C++ build tools» и Windows SDK.

TypeScript остаётся 6.0.x: svelte-check пока не поддерживает TypeScript 7.
Зависимости раз в неделю проверяет Dependabot (`.github/dependabot.yml`), PR идут в `refactor-rust-poc`.

## Обычная x64-машина

```bash
npm ci
npm run check
npm run build

cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets

npm run tauri dev
npm run tauri build -- --no-bundle
```

Готовый exe — `target/release/BeamModManager.exe` (в CI), отладочный — `target/debug/BeamModManager.exe`.
Портативная сборка выпуска — zip с этим exe и `latest.json`; установщик не используется.

## Windows on ARM (эта машина)

Здесь Rust по умолчанию поставился как `aarch64-pc-windows-msvc`, но MSVC ARM64-тулсета нет.
Собираем x64 — он же релизный:

```bash
rustup toolchain install stable-x86_64-pc-windows-msvc --force-non-host --profile minimal
rustup component add rustfmt clippy --toolchain stable-x86_64-pc-windows-msvc
```

Команды запускаются из окружения MSVC x64. Проще через временный `.bat`:

```bat
@echo off
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvarsall.bat" amd64 >nul
if errorlevel 1 exit /b 1
cargo +stable-x86_64-pc-windows-msvc %*
```

Для Tauri-сборки добавьте `set RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-msvc` и запускайте
`npm run tauri build -- --no-bundle --debug`. Иначе Tauri возьмёт ARM64-хост и линковка упадёт
на несовпадении объектов и библиотек.

## Почему имена крейтов без «update», «install», «setup»

Windows UAC считает exe с таким словом в имени установщиком и требует повышение (ошибка 740).
Тестовый exe `bmm_appupdate-*.exe` из-за этого не запускается. Поэтому крейты называются
`bmm-app-release`, `bmm-packs`, `bmm-checks`, а команды и тексты могут оставаться про обновления
и установку.

## CI

`.github/workflows/ci.yml` — windows-2025, x64: форматирование, clippy, тесты Rust, проверка
типов и сборка фронтенда, сборка окна. Старый workflow лежит в `legacy/.github/workflows/` и
GitHub его не запускает.

## Релиз и обновление программы

`.github/workflows/release.yml` запускается на тег `v*`. Перед тегом версия должна совпадать в
`Cargo.toml`, `package.json` и `src-tauri/tauri.conf.json` — workflow это проверяет и падает,
если версии разошлись. Сборка делает портативный `BeamModManager-<версия>.zip`, считает
SHA-256, пишет `latest.json` и публикует GitHub Release. Установленное приложение проверяет
`releases/latest/download/latest.json`, скачивает архив, проверяет хеш/размер и после
подтверждения перезапускается через отдельный процесс `--install-update` с откатом.

## Данные при проверках

Приложение использует `%LOCALAPPDATA%\BeamModManager` (тот же каталог, что старая версия).
Тесты домена пишут только во временные каталоги.
