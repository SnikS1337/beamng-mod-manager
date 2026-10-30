# M0: каркас Rust + Tauri 2 + Svelte 5 — план реализации

> **Исторический документ.** Описывает старую C#/WPF-версию или ранний этап переезда. Не является текущей инструкцией. Актуальные сведения — в `README.md`, `docs/STATUS.md` и `docs/BUILD.md`.


> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Цель:** перестроить репозиторий и получить собираемое приложение `BeamModManager.exe` на Rust + Tauri 2 + Svelte 5 с четырьмя маршрутами, строгим CSP, общим мьютексом со старой версией и зелёными проверками.

**Архитектура:** новое приложение занимает корень, старый C#/WPF уезжает в `legacy/` как эталон. Rust-ядро делится на крейты, Tauri остаётся тонким слоем команд и окон, Svelte — представлением. На этом этапе создаются только каркас, один реальный доменный модуль (`AppPaths`) и одна IPC-команда (`app_bootstrap`); остальные крейты пока пустые.

**Стек:** Rust 1.99 MSVC, Tauri 2, SvelteKit 2 (SPA), Svelte 5 (runes), TypeScript 6, Tailwind CSS 4, shadcn-svelte, Vite 8, Node 24.

**Спека:** [docs/superpowers/specs/2026-10-21-rust-tauri-refactor-design.md](../specs/2026-10-21-rust-tauri-refactor-design.md)

## Общие ограничения

- Целевая платформа — Windows; релиз x64, локальная машина ARM64 (Windows on ARM).
- Полный паритет с 0.3.12 — только к релизу v1; в M0 функции ещё не переносятся.
- Каталог данных — `%LOCALAPPDATA%\BeamModManager`; форматы старых файлов не ломаем.
- Имя exe — `BeamModManager.exe`, имя архива выпуска — `ModManagerBeamNG.zip`.
- Идентификатор приложения — `com.sniks1337.beammodmanager`; имя продукта — `ModManager BeamNG`.
- Версия на этапе переезда — `0.4.0`.
- Фронтенду не выдаются права `fs`, `shell`, `http`; только конкретные команды.
- Строгий CSP без `unsafe-eval`; ослабления — только с записью в спеке.
- Весь текст для пользователя, документы и коммиты — по-русски; идентификаторы кода — английские.
- Проверки перед коммитом: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `npm run check`, `npm run build`.
- Push в GitHub и релизы — только после отдельного подтверждения пользователя.

## Файловая структура M0

```
Cargo.toml                         workspace и общие версии
src-tauri/
  Cargo.toml                       bmm-desktop, зависимости от крейтов
  tauri.conf.json                  окно, CSP, имя exe, версия
  capabilities/default.json        права главного окна
  src/main.rs                      точка входа
  src/lib.rs                       сборка Tauri, команды, мьютекс
  src/error.rs                     AppError
  src/state.rs                     AppState
  src/named_mutex.rs               общий мьютекс со старой версией
  src/commands/mod.rs
  src/commands/app.rs              app_bootstrap
  src/commands/dev.rs              dev_library, dev_environment, open_modland_probe
crates/
  bmm-domain/src/paths.rs          AppPaths + тесты
  bmm-domain/src/lib.rs
  bmm-game/src/lib.rs
  bmm-packs/src/lib.rs
  bmm-sources/src/lib.rs
  bmm-checks/src/lib.rs
  bmm-app-release/src/lib.rs
src/
  routes/+layout.svelte            общий импорт CSS и children
  routes/+layout.ts                ssr = false
  routes/(app)/+layout.svelte      фон, титульная строка, навигация
  routes/(app)/+page.svelte        Моды, проверка app_bootstrap
  routes/(app)/updates/+page.svelte
  routes/(app)/search/+page.svelte
  routes/(app)/settings/+page.svelte
  routes/dev/+page.svelte          старый PoC-стенд, вне навигации
  lib/shell/Titlebar.svelte        перенесён из PoC
  lib/shell/Background.svelte      перенесён из PoC
  lib/shell/NavRail.svelte
  lib/shell/StagePlaceholder.svelte
  lib/ipc/client.ts                bootstrap + mock
  lib/ipc/types.ts                 BootstrapDto, AppError
  lib/ipc/window.ts                windowAction
  lib/poc/                         ModGrid, ModCard, api — до M2
fixtures/                          переехали из tests/fixtures
legacy/                            старый C# и старый CI
.github/workflows/ci.yml           новый CI
```

---

### Задача 1: Перестроить репозиторий

**Файлы:**
- Переместить: `src/`, `tests/`, `BeamModManager.sln`, `.github/workflows/windows.yml`, `tools/release/`, `poc/*`.
- Создать: `legacy/`, `legacy/.github/workflows/`, `fixtures/`.

**Интерфейсы:**
- Потребляет: текущий PoC и старый проект.
- Производит: корень нового приложения, `legacy/` и `fixtures/`, на которые опираются все следующие задачи.

- [ ] **Шаг 1: Убедиться, что рабочее дерево чистое**

```bash
git status --short
```

Ожидается пустой вывод (спека уже закоммичена).

- [ ] **Шаг 2: Перенести старый проект в `legacy/`**

```bash
mkdir -p legacy/.github/workflows legacy/tools
git mv src legacy/src
git mv tests legacy/tests
git mv BeamModManager.sln legacy/
git mv .github/workflows/windows.yml legacy/.github/workflows/windows.yml
git mv legacy/tests/fixtures fixtures
git mv tools/release legacy/tools/release
```

- [ ] **Шаг 3: Перенести PoC в корень**

```bash
git mv poc/package.json package.json
git mv poc/package-lock.json package-lock.json
git mv poc/README.md docs/POC.md
git mv poc/components.json components.json
git mv poc/svelte.config.js svelte.config.js
git mv poc/tsconfig.json tsconfig.json
git mv poc/vite.config.js vite.config.js
git mv poc/src src
git mv poc/src-tauri src-tauri
git mv poc/static static
```

- [ ] **Шаг 4: Слить `.gitignore` и убрать остатки PoC**

Прочитать `poc/.gitignore`, добавить в корневой `.gitignore` секции:

```gitignore
# SvelteKit / Vite
node_modules
/build
/.svelte-kit
vite.config.js.timestamp-*
vite.config.ts.timestamp-*
.env
.env.*
!.env.example

# Rust / Tauri
/target
/src-tauri/target
```

Затем:

```bash
git rm poc/.gitignore
git status --short
```

Если `poc/` остался пустым — каталог просто не отслеживается; удалить его командой `rmdir poc` (не `rm -rf`).

- [ ] **Шаг 5: Проверить раскладку**

```bash
ls
ls .github/workflows
ls legacy
ls fixtures | head
```

Ожидается: в корне `src/`, `src-tauri/`, `crates/` ещё нет, но есть `package.json`, `svelte.config.js`; в `.github/workflows` пусто; в `legacy` — `src`, `tests`, `BeamModManager.sln`; в `fixtures` — HTML и `db.json`.

- [ ] **Шаг 6: Коммит**

```bash
git add -A
git -c user.name="Lev" -c user.email="mrvandal94@mail.ru" commit -m "Перестроить репозиторий под Rust-переезд"
```

---

### Задача 2: Переименовать приложение и настроить Tauri

**Файлы:**
- Изменить: `package.json`, `src-tauri/Cargo.toml`, `src-tauri/src/main.rs`, `src-tauri/tauri.conf.json`, `src/app.html`, `src-tauri/capabilities/default.json`.
- Создать: нет.

**Интерфейсы:**
- Потребляет: корневую раскладку из задачи 1.
- Производит: пакет `bmm-desktop` с библиотекой `bmm_desktop_lib`, exe `BeamModManager.exe`, окно с CSP и правами; на это опираются задачи 3–6.

- [ ] **Шаг 1: Привести `package.json` к новому имени**

```json
{
  "name": "beamng-mod-manager",
  "version": "0.4.0",
  "description": "ModManager BeamNG: Rust + Tauri 2 + Svelte 5",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite dev",
    "build": "vite build",
    "preview": "vite preview",
    "prepare": "svelte-kit sync || echo ''",
    "check": "svelte-kit sync && svelte-check --tsconfig ./tsconfig.json",
    "check:watch": "svelte-kit sync && svelte-check --tsconfig ./tsconfig.json --watch",
    "tauri": "tauri"
  },
  "license": "UNLICENSED",
  "dependencies": {
    "@lucide/svelte": "^1.60.0",
    "@tauri-apps/api": "^2",
    "@tauri-apps/plugin-opener": "^2",
    "bits-ui": "^2.20.0"
  },
  "devDependencies": {
    "@internationalized/date": "^3.12.4",
    "@sveltejs/adapter-static": "^3.0.10",
    "@sveltejs/kit": "^2.65.1",
    "@sveltejs/vite-plugin-svelte": "^7.1.2",
    "@tailwindcss/vite": "^4.3.3",
    "@tauri-apps/cli": "^2",
    "clsx": "^2.1.1",
    "svelte": "^5.56.3",
    "svelte-check": "^4.6.0",
    "tailwind-merge": "^3.7.0",
    "tailwind-variants": "^3.3.1",
    "tailwindcss": "^4.3.3",
    "tw-animate-css": "^1.4.0",
    "typescript": "~6.0.3",
    "vite": "^8.0.16"
  }
}
```

- [ ] **Шаг 2: Обновить lock-файл**

```bash
npm install --package-lock-only
```

Ожидается обновление имени/версии в `package-lock.json` без установки пакетов.

- [ ] **Шаг 3: Переименовать Rust-пакет**

`src-tauri/Cargo.toml`:

```toml
[package]
name = "bmm-desktop"
version = "0.4.0"
description = "ModManager BeamNG desktop shell"
authors = ["SnikS1337"]
edition = "2024"

[lib]
name = "bmm_desktop_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = [] }
tauri-plugin-opener = "2"
tauri-plugin-window-state = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[profile.release]
codegen-units = 1
lto = true
opt-level = 3
panic = "abort"
strip = true
```

`src-tauri/src/main.rs`:

```rust
// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    bmm_desktop_lib::run()
}
```

- [ ] **Шаг 4: Настроить окно и CSP**

`src-tauri/tauri.conf.json`:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "ModManager BeamNG",
  "version": "0.4.0",
  "identifier": "com.sniks1337.beammodmanager",
  "mainBinaryName": "BeamModManager",
  "build": {
    "beforeDevCommand": "npm run dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "npm run build",
    "frontendDist": "../build"
  },
  "app": {
    "windows": [
      {
        "label": "main",
        "title": "ModManager BeamNG",
        "width": 1280,
        "height": 820,
        "minWidth": 1000,
        "minHeight": 680,
        "center": true,
        "decorations": false,
        "resizable": true,
        "theme": "Dark"
      }
    ],
    "security": {
      "csp": "default-src 'self'; img-src 'self' asset: http://asset.localhost data: blob:; style-src 'self' 'unsafe-inline'; script-src 'self'; connect-src ipc: http://ipc.localhost; font-src 'self' data:; object-src 'none'; base-uri 'none'; frame-ancestors 'none'",
      "devCsp": "default-src 'self'; img-src 'self' asset: http://asset.localhost data: blob:; style-src 'self' 'unsafe-inline'; script-src 'self'; connect-src ipc: http://ipc.localhost ws://localhost:1420 ws://127.0.0.1:1420; font-src 'self' data:; object-src 'none'; base-uri 'none'; frame-ancestors 'none'"
    }
  },
  "bundle": {
    "active": false,
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  }
}
```

- [ ] **Шаг 5: Обновить заголовок HTML**

В `src/app.html` заменить `<title>ModManager BeamNG — PoC</title>` на `<title>ModManager BeamNG</title>`.

- [ ] **Шаг 6: Проверить права окна**

`src-tauri/capabilities/default.json` оставить как у PoC, но убедиться, что `windows` содержит `"main"` и есть только `core:default`, оконные разрешения и `opener:default`. Не добавлять `fs`, `shell`, `http`.

- [ ] **Шаг 7: Проверить метаданные**

```bash
npm run check
cd src-tauri && cargo metadata --no-deps --format-version 1 > /dev/null
```

Ожидается: `svelte-check` без ошибок, `cargo metadata` видит пакет `bmm-desktop`.

- [ ] **Шаг 8: Коммит**

```bash
git add package.json package-lock.json src-tauri/Cargo.toml src-tauri/src/main.rs src-tauri/tauri.conf.json src/app.html src-tauri/capabilities/default.json
git -c user.name="Lev" -c user.email="mrvandal94@mail.ru" commit -m "Переименовать приложение и настроить окно Tauri"
```

---

### Задача 3: Cargo workspace, крейты и `AppPaths`

**Файлы:**
- Создать: `Cargo.toml`, `crates/bmm-domain/Cargo.toml`, `crates/bmm-domain/src/lib.rs`, `crates/bmm-domain/src/paths.rs`, `crates/bmm-game/…`, `crates/bmm-packs/…`, `crates/bmm-sources/…`, `crates/bmm-checks/…`, `crates/bmm-app-release/…`.
- Изменить: `src-tauri/Cargo.toml`.

**Интерфейсы:**
- Потребляет: переименованный `bmm-desktop`.
- Производит: `bmm_domain::paths::AppPaths` с методами `resolve`, `at`, `root`, `settings`, `library`, `matches`, `previews`, `search_previews`, `webview2`, `update_dir`, `errors_log`, `ensure_dirs`; на них опираются задачи 4 и следующие этапы.

- [ ] **Шаг 1: Создать workspace и пустые крейты**

Корневой `Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = [
  "src-tauri",
  "crates/bmm-domain",
  "crates/bmm-game",
  "crates/bmm-packs",
  "crates/bmm-sources",
  "crates/bmm-checks",
  "crates/bmm-app-release",
]

[workspace.package]
version = "0.4.0"
edition = "2024"
rust-version = "1.85"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tempfile = "3"
```

Создать крейты:

```bash
mkdir -p crates
cargo new --lib crates/bmm-domain --vcs none
cargo new --lib crates/bmm-game --vcs none
cargo new --lib crates/bmm-packs --vcs none
cargo new --lib crates/bmm-sources --vcs none
cargo new --lib crates/bmm-checks --vcs none
cargo new --lib crates/bmm-app-release --vcs none
```

В каждом `crates/*/Cargo.toml` заменить версию/edition на workspace:

```toml
[package]
name = "bmm-domain"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
```

Для остальных — имя соответствующее (`bmm-game`, `bmm-packs`, `bmm-sources`, `bmm-checks`, `bmm-app-release`).

- [ ] **Шаг 2: Подключить крейты к Tauri**

В `src-tauri/Cargo.toml` заменить блок `[package]`:

```toml
[package]
name = "bmm-desktop"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
```

И добавить в `[dependencies]`:

```toml
serde.workspace = true
serde_json.workspace = true
bmm-domain = { path = "../crates/bmm-domain" }
bmm-game = { path = "../crates/bmm-game" }
bmm-packs = { path = "../crates/bmm-packs" }
bmm-sources = { path = "../crates/bmm-sources" }
bmm-checks = { path = "../crates/bmm-checks" }
bmm-app-release = { path = "../crates/bmm-app-release" }
```

- [ ] **Шаг 3: Написать падающий тест `AppPaths`**

`crates/bmm-domain/Cargo.toml`:

```toml
[package]
name = "bmm-domain"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
thiserror.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

`crates/bmm-domain/src/lib.rs`:

```rust
//! Доменные модели и файловые правила ModManager BeamNG.

pub mod paths;
```

`crates/bmm-domain/src/paths.rs`:

```rust
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum PathsError {
    #[error("не задан каталог LOCALAPPDATA")]
    NoLocalAppData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    root: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> Result<Self, PathsError> {
        let base = std::env::var_os("LOCALAPPDATA").ok_or(PathsError::NoLocalAppData)?;
        Ok(Self::at(PathBuf::from(base).join("BeamModManager")))
    }

    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn settings(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    pub fn library(&self) -> PathBuf {
        self.root.join("library.json")
    }

    pub fn matches(&self) -> PathBuf {
        self.root.join("matches.json")
    }

    pub fn previews(&self) -> PathBuf {
        self.root.join("previews")
    }

    pub fn search_previews(&self) -> PathBuf {
        self.root.join("search-previews")
    }

    pub fn webview2(&self) -> PathBuf {
        self.root.join("WebView2")
    }

    pub fn update_dir(&self) -> PathBuf {
        self.root.join("update")
    }

    pub fn errors_log(&self) -> PathBuf {
        self.root.join("errors.log")
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        for path in [
            self.root(),
            self.previews().as_path(),
            self.search_previews().as_path(),
            self.webview2().as_path(),
            self.update_dir().as_path(),
        ] {
            std::fs::create_dir_all(path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_live_in_expected_places() {
        let paths = AppPaths::at(r"C:\Users\Test\AppData\Local\BeamModManager");
        assert_eq!(paths.settings(), paths.root().join("settings.json"));
        assert_eq!(paths.library(), paths.root().join("library.json"));
        assert_eq!(paths.matches(), paths.root().join("matches.json"));
        assert_eq!(paths.previews(), paths.root().join("previews"));
        assert_eq!(paths.search_previews(), paths.root().join("search-previews"));
        assert_eq!(paths.webview2(), paths.root().join("WebView2"));
        assert_eq!(paths.update_dir(), paths.root().join("update"));
        assert_eq!(paths.errors_log(), paths.root().join("errors.log"));
    }

    #[test]
    fn ensure_dirs_creates_all_directories() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(temp.path());
        paths.ensure_dirs().unwrap();
        for path in [
            paths.root(),
            paths.previews().as_path(),
            paths.search_previews().as_path(),
            paths.webview2().as_path(),
            paths.update_dir().as_path(),
        ] {
            assert!(path.is_dir(), "нет каталога {path:?}");
        }
    }
}
```

- [ ] **Шаг 4: Убедиться, что тест сначала не проходит**

```bash
cargo test -p bmm-domain
```

Ожидается ошибка компиляции или паника: модуль/методы ещё не готовы (если файл уже записан целиком — временно удалить `pub mod paths;` и запустить снова).

- [ ] **Шаг 5: Вернуть реализацию и прогнать тест**

```bash
cargo test -p bmm-domain
```

Ожидается: `2 passed`.

- [ ] **Шаг 6: Проверить весь workspace**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
```

Ожидается: без предупреждений, все тесты зелёные.

- [ ] **Шаг 7: Коммит**

```bash
git add Cargo.toml Cargo.lock crates src-tauri/Cargo.toml
git -c user.name="Lev" -c user.email="mrvandal94@mail.ru" commit -m "Добавить cargo workspace и AppPaths"
```

---

### Задача 4: Команда `app_bootstrap` и IPC-клиент

**Файлы:**
- Создать: `src-tauri/src/error.rs`, `src-tauri/src/state.rs`, `src-tauri/src/commands/mod.rs`, `src-tauri/src/commands/app.rs`, `src-tauri/src/commands/dev.rs`, `src/lib/ipc/types.ts`, `src/lib/ipc/client.ts`, `src/lib/ipc/window.ts`.
- Изменить: `src-tauri/src/lib.rs`, `src/lib/poc/api.ts`, `src/lib/shell/Titlebar.svelte` (после задачи 5 путь), `src/lib/poc/Titlebar.svelte` (до переноса).

**Интерфейсы:**
- Потребляет: `AppPaths` из задачи 3.
- Производит: команду `app_bootstrap`, тип `BootstrapDto` в Rust и TS, функцию `bootstrap()`; на неё опирается экран «Моды» в задаче 5.

- [ ] **Шаг 1: Добавить `AppError`**

`src-tauri/src/error.rs`:

```rust
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: String,
    pub message: String,
}

impl AppError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl From<bmm_domain::paths::PathsError> for AppError {
    fn from(error: bmm_domain::paths::PathsError) -> Self {
        Self::new("paths", error.to_string())
    }
}
```

- [ ] **Шаг 2: Добавить состояние**

`src-tauri/src/state.rs`:

```rust
use bmm_domain::paths::AppPaths;

pub struct AppState {
    pub paths: AppPaths,
}
```

- [ ] **Шаг 3: Написать команду bootstrap**

`src-tauri/src/commands/mod.rs`:

```rust
pub mod app;
pub mod dev;
```

`src-tauri/src/commands/app.rs`:

```rust
use serde::Serialize;
use tauri::State;

use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapDto {
    pub app_version: String,
    pub os: &'static str,
    pub arch: &'static str,
    pub data_dir: String,
    pub settings_exists: bool,
    pub library_exists: bool,
}

#[tauri::command]
pub fn app_bootstrap(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<BootstrapDto, AppError> {
    let paths = &state.paths;
    Ok(BootstrapDto {
        app_version: app.package_info().version.to_string(),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        data_dir: paths.root().display().to_string(),
        settings_exists: paths.settings().exists(),
        library_exists: paths.library().exists(),
    })
}
```

- [ ] **Шаг 4: Перенести PoC-команды в `dev.rs`**

Перенести из `src-tauri/src/lib.rs` структуры `ModEntry`, `EnvInfo`, функции `poc_library`, `poc_environment`, `open_modland_probe` в `src-tauri/src/commands/dev.rs`. Переименовать команды:

- `poc_library` → `dev_library`
- `poc_environment` → `dev_environment`
- `open_modland_probe` оставить без изменений.

`DevEnvInfo`/`ModEntry` должны быть `pub` и использовать `serde::Serialize`. `dev_library` остаётся синхронной, `open_modland_probe` — `async`.

- [ ] **Шаг 5: Собрать `lib.rs`**

`src-tauri/src/lib.rs` должен:

1. Объявить модули `commands`, `error`, `state`.
2. В `run()` получить `AppPaths::resolve()`, вызвать `ensure_dirs()`, собрать `AppState`.
3. Подключить `tauri_plugin_opener` и `tauri_plugin_window_state`.
4. Зарегистрировать `app_bootstrap`, `dev_library`, `dev_environment`, `open_modland_probe`.
5. Оставить обработку `--probe` для `open_modland_probe`.

Каркас:

```rust
mod commands;
mod error;
mod state;

use bmm_domain::paths::AppPaths;
use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let paths = match AppPaths::resolve() {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("[bmm] не удалось определить каталог данных: {error}");
            return;
        }
    };

    if let Err(error) = paths.ensure_dirs() {
        eprintln!("[bmm] не удалось создать каталоги данных: {error}");
    }

    let state = AppState { paths };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .manage(state)
        .setup(|app| {
            if std::env::args().any(|arg| arg == "--probe") {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = commands::dev::open_modland_probe(handle).await {
                        eprintln!("[bmm] modland probe failed: {error}");
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::app_bootstrap,
            commands::dev::dev_library,
            commands::dev::dev_environment,
            commands::dev::open_modland_probe
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Шаг 6: Обновить фронтенд-вызовы PoC**

В `src/lib/poc/api.ts` заменить `invoke("poc_library")` на `invoke("dev_library")`, `invoke("poc_environment")` на `invoke("dev_environment")`; имена функций TS можно оставить прежними.

- [ ] **Шаг 7: Добавить типы и клиент IPC**

`src/lib/ipc/types.ts`:

```ts
export interface AppError {
  code: string;
  message: string;
}

export interface BootstrapDto {
  appVersion: string;
  os: string;
  arch: string;
  dataDir: string;
  settingsExists: boolean;
  libraryExists: boolean;
}
```

`src/lib/ipc/client.ts`:

```ts
import { invoke, isTauri } from "@tauri-apps/api/core";
import type { BootstrapDto } from "./types";

export async function bootstrap(): Promise<BootstrapDto> {
  if (isTauri()) {
    return invoke<BootstrapDto>("app_bootstrap");
  }

  return {
    appVersion: "web-preview",
    os: navigator.platform || "web",
    arch: "-",
    dataDir: "%LOCALAPPDATA%\\BeamModManager",
    settingsExists: false,
    libraryExists: false,
  };
}
```

`src/lib/ipc/window.ts`:

```ts
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauri } from "@tauri-apps/api/core";

export async function windowAction(
  action: "minimize" | "maximize" | "close",
): Promise<void> {
  if (!isTauri()) return;

  const win = getCurrentWindow();
  if (action === "minimize") await win.minimize();
  else if (action === "maximize") await win.toggleMaximize();
  else await win.close();
}
```

- [ ] **Шаг 8: Переключить титульную строку на новый клиент**

В `src/lib/poc/Titlebar.svelte` заменить импорт `windowAction` на `import { windowAction } from "$lib/ipc/window";`. Удалить `windowAction` из `src/lib/poc/api.ts`, если она там была.

- [ ] **Шаг 9: Проверить**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
npm run check
```

Ожидается: всё зелёное, `app_bootstrap` зарегистрирована.

- [ ] **Шаг 10: Коммит**

```bash
git add src-tauri/src src/lib/ipc src/lib/poc
git -c user.name="Lev" -c user.email="mrvandal94@mail.ru" commit -m "Добавить bootstrap-команду и IPC-клиент"
```

---

### Задача 5: Оболочка Svelte и четыре маршрута

**Файлы:**
- Переместить: `src/lib/poc/Titlebar.svelte` → `src/lib/shell/Titlebar.svelte`, `src/lib/poc/Background.svelte` → `src/lib/shell/Background.svelte`, `src/routes/+page.svelte` → `src/routes/dev/+page.svelte`.
- Создать: `src/routes/(app)/+layout.svelte`, `src/routes/(app)/+page.svelte`, `src/routes/(app)/updates/+page.svelte`, `src/routes/(app)/search/+page.svelte`, `src/routes/(app)/settings/+page.svelte`, `src/lib/shell/NavRail.svelte`, `src/lib/shell/StagePlaceholder.svelte`.
- Изменить: `src/routes/+layout.svelte`, `src/routes/dev/+page.svelte`.

**Интерфейсы:**
- Потребляет: `bootstrap()` и `BootstrapDto` из задачи 4.
- Производит: маршруты `/`, `/updates`, `/search`, `/settings` и `/dev`; на них опираются M1–M6.

- [ ] **Шаг 1: Перенести общие компоненты**

```bash
mkdir -p src/lib/shell
git mv src/lib/poc/Titlebar.svelte src/lib/shell/Titlebar.svelte
git mv src/lib/poc/Background.svelte src/lib/shell/Background.svelte
```

В `Titlebar.svelte` заменить тег `PoC` на `0.4` и оставить импорт `windowAction` из `$lib/ipc/window`.

- [ ] **Шаг 2: Настроить корневой layout**

`src/routes/+layout.svelte`:

```svelte
<script lang="ts">
  import "../app.css";

  let { children } = $props();
</script>

{@render children()}
```

`src/routes/+layout.ts` оставить с `export const ssr = false;`.

- [ ] **Шаг 3: Добавить навигацию**

`src/lib/shell/NavRail.svelte`:

```svelte
<script lang="ts">
  import { Download, Package, Search, Settings } from "@lucide/svelte";
  import { page } from "$app/state";

  const nav = [
    { href: "/", label: "Моды", icon: Package },
    { href: "/updates", label: "Обновления", icon: Download },
    { href: "/search", label: "Поиск", icon: Search },
    { href: "/settings", label: "Настройки", icon: Settings },
  ] as const;
</script>

<aside class="rail glass liquid-edge">
  <nav>
    {#each nav as item (item.href)}
      {@const Icon = item.icon}
      {@const active = page.url.pathname === item.href}
      <a class="nav" class:active href={item.href} aria-current={active ? "page" : undefined}>
        <Icon size={20} strokeWidth={1.6} />
        <span>{item.label}</span>
      </a>
    {/each}
  </nav>
</aside>

<style>
  .rail {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    border-radius: 20px;
    padding: 10px 8px;
  }

  nav {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .nav {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 9px 4px;
    border-radius: 14px;
    color: var(--muted-foreground);
    font-size: 9.5px;
    line-height: 1.15;
    text-align: center;
    text-decoration: none;
    transition: background 160ms ease, color 160ms ease;
  }

  .nav:hover,
  .nav.active {
    background: color-mix(in oklab, var(--foreground) 10%, transparent);
    color: var(--foreground);
  }

  .nav.active {
    font-weight: 600;
  }
</style>
```

Если `$app/state` недоступен в текущей версии SvelteKit, заменить на `$app/stores`:

```svelte
<script lang="ts">
  import { page } from "$app/stores";
  // ...
  {@const active = $page.url.pathname === item.href}
</script>
```

- [ ] **Шаг 4: Собрать каркас приложения**

`src/routes/(app)/+layout.svelte`:

```svelte
<script lang="ts">
  import Background from "$lib/shell/Background.svelte";
  import NavRail from "$lib/shell/NavRail.svelte";
  import Titlebar from "$lib/shell/Titlebar.svelte";

  let { children } = $props();
</script>

<div class="app">
  <Background />
  <div class="noise"></div>
  <Titlebar />

  <div class="workspace">
    <NavRail />
    <main class="content scroll-area">
      {@render children()}
    </main>
  </div>
</div>

<style>
  .app {
    position: relative;
    height: 100vh;
    overflow: hidden;
  }

  .workspace {
    position: relative;
    z-index: 10;
    display: grid;
    grid-template-columns: 96px minmax(0, 1fr);
    gap: 12px;
    height: calc(100vh - 44px);
    margin-top: 44px;
    padding: 0 12px 12px;
  }

  .content {
    min-width: 0;
    overflow: auto;
    border-radius: 20px;
  }

  @media (max-width: 1020px) {
    .workspace {
      grid-template-columns: 76px minmax(0, 1fr);
    }
  }
</style>
```

- [ ] **Шаг 5: Добавить заглушку раздела**

`src/lib/shell/StagePlaceholder.svelte`:

```svelte
<script lang="ts">
  let {
    title,
    stage = "M0",
    text,
  }: { title: string; stage?: string; text: string } = $props();
</script>

<section class="placeholder glass liquid-edge">
  <p class="eyebrow">{stage}</p>
  <h1>{title}</h1>
  <p>{text}</p>
</section>

<style>
  .placeholder {
    min-height: 320px;
    border-radius: 20px;
    padding: 32px;
  }

  .eyebrow {
    margin: 0 0 8px;
    color: var(--muted-foreground);
    font-size: 10px;
    font-weight: 650;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  h1 {
    margin: 0 0 10px;
    font-size: 26px;
    font-weight: 650;
    letter-spacing: -0.02em;
  }

  p {
    max-width: 640px;
    margin: 0;
    color: var(--muted-foreground);
    font-size: 13px;
    line-height: 1.5;
  }
</style>
```

- [ ] **Шаг 6: Создать четыре страницы**

`src/routes/(app)/+page.svelte`:

```svelte
<script lang="ts">
  import { onMount } from "svelte";
  import { bootstrap } from "$lib/ipc/client";
  import type { BootstrapDto } from "$lib/ipc/types";
  import StagePlaceholder from "$lib/shell/StagePlaceholder.svelte";

  let info = $state<BootstrapDto | null>(null);
  let error = $state<string | null>(null);

  onMount(() => {
    bootstrap()
      .then((value) => (info = value))
      .catch((reason: unknown) => (error = String(reason)));
  });
</script>

<StagePlaceholder
  title="Моды"
  text="Каркас M0. Здесь появится гараж: карточки, фильтры, включение и выключение, правая панель."
/>

{#if info}
  <section class="bootstrap glass liquid-edge">
    <p>Rust IPC отвечает: {info.appVersion} · {info.os}/{info.arch}</p>
    <p class="mono">{info.dataDir}</p>
  </section>
{:else if error}
  <section class="bootstrap glass liquid-edge error">{error}</section>
{/if}

<style>
  .bootstrap {
    margin-top: 12px;
    border-radius: 16px;
    padding: 14px 18px;
    color: var(--muted-foreground);
    font-size: 11px;
  }

  .bootstrap p {
    margin: 0;
  }

  .mono {
    margin-top: 4px !important;
    font-family: "SFMono-Regular", ui-monospace, Menlo, Consolas, monospace;
    font-size: 10px;
  }

  .error {
    color: oklch(0.72 0.18 25);
  }
</style>
```

`src/routes/(app)/updates/+page.svelte`:

```svelte
<script lang="ts">
  import StagePlaceholder from "$lib/shell/StagePlaceholder.svelte";
</script>

<StagePlaceholder
  title="Обновления"
  text="Каркас M0. Здесь появится проверка обновлений модов, панель сайтов и привязки страниц."
/>
```

`src/routes/(app)/search/+page.svelte`:

```svelte
<script lang="ts">
  import StagePlaceholder from "$lib/shell/StagePlaceholder.svelte";
</script>

<StagePlaceholder
  title="Поиск"
  text="Каркас M0. Здесь появится потоковый поиск по Modland, ModsGaming, STMods и репозиторию BeamNG."
/>
```

`src/routes/(app)/settings/+page.svelte`:

```svelte
<script lang="ts">
  import StagePlaceholder from "$lib/shell/StagePlaceholder.svelte";
</script>

<StagePlaceholder
  title="Настройки"
  text="Каркас M0. Здесь появятся внешний вид, загрузка модов, папки, обновления и сайты."
/>
```

- [ ] **Шаг 7: Перенести PoC-стенд на `/dev`**

```bash
mkdir -p src/routes/dev
git mv src/routes/+page.svelte src/routes/dev/+page.svelte
```

В `src/routes/dev/+page.svelte`:

- заменить импорты `$lib/poc/Titlebar.svelte` и `$lib/poc/Background.svelte` на `$lib/shell/Titlebar.svelte` и `$lib/shell/Background.svelte`;
- заменить `<title>ModManager BeamNG — PoC</title>` на `<title>ModManager BeamNG — стенд</title>`;
- заменить тег `PoC` рядом с логотипом на `стенд`.

- [ ] **Шаг 8: Проверить сборку и маршруты**

```bash
npm run check
npm run build
```

Ожидается: `svelte-check` без ошибок, сборка в `build/`.

- [ ] **Шаг 9: Проверить в браузере**

Запустить `npm run dev` в фоне. Открыть `http://localhost:1420/`, `/updates`, `/search`, `/settings`, `/dev`. Убедиться: навигация работает, фон и титульная строка на месте, на «Модах» виден ответ mock-IPC, консоль без ошибок. Остановить dev-сервер.

- [ ] **Шаг 10: Коммит**

```bash
git add src
git -c user.name="Lev" -c user.email="mrvandal94@mail.ru" commit -m "Собрать оболочку приложения и маршруты"
```

---

### Задача 6: Общий мьютекс и сохранение окна

**Файлы:**
- Создать: `src-tauri/src/named_mutex.rs`.
- Изменить: `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs`, `src-tauri/capabilities/default.json`.

**Интерфейсы:**
- Потребляет: `run()` из задачи 4.
- Производит: `named_mutex::acquire()` и `named_mutex::show_already_running()`; второе приложение и старая версия не работают одновременно.

- [ ] **Шаг 1: Добавить зависимость Windows API**

В `src-tauri/Cargo.toml`:

```toml
[target.'cfg(windows)'.dependencies]
windows-sys = { version = "0.59", features = [
  "Win32_Foundation",
  "Win32_System_Threading",
  "Win32_UI_WindowsAndMessaging",
] }
```

- [ ] **Шаг 2: Написать мьютекс и тест**

`src-tauri/src/named_mutex.rs`:

```rust
//! Общий именованный мьютекс со старой версией `Local\BeamModManager.Single`.

#[cfg(windows)]
pub struct NamedMutex(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
unsafe impl Send for NamedMutex {}

#[cfg(windows)]
impl Drop for NamedMutex {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
pub fn acquire() -> Option<NamedMutex> {
    acquire_named("Local\\BeamModManager.Single")
}

#[cfg(windows)]
fn acquire_named(name: &str) -> Option<NamedMutex> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::CreateMutexW;

    let wide: Vec<u16> = OsStr::new(name).encode_wide().chain(Some(0)).collect();
    unsafe {
        let handle = CreateMutexW(std::ptr::null(), 1, wide.as_ptr());
        if handle.is_null() {
            return None;
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            windows_sys::Win32::Foundation::CloseHandle(handle);
            return None;
        }
        Some(NamedMutex(handle))
    }
}

#[cfg(windows)]
pub fn show_already_running() {
    #[cfg(debug_assertions)]
    {
        eprintln!("[bmm] ModManager BeamNG уже запущен");
    }

    #[cfg(not(debug_assertions))]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            MessageBoxW, MB_ICONINFORMATION, MB_OK,
        };

        let text: Vec<u16> = OsStr::new("ModManager BeamNG уже запущен.")
            .encode_wide()
            .chain(Some(0))
            .collect();
        let caption: Vec<u16> = OsStr::new("ModManager BeamNG")
            .encode_wide()
            .chain(Some(0))
            .collect();
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                caption.as_ptr(),
                MB_OK | MB_ICONINFORMATION,
            );
        }
    }
}

#[cfg(not(windows))]
pub struct NamedMutex;

#[cfg(not(windows))]
pub fn acquire() -> Option<NamedMutex> {
    Some(NamedMutex)
}

#[cfg(not(windows))]
pub fn show_already_running() {}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn second_instance_is_rejected() {
        let name = format!(
            "Local\\BeamModManager.Test.{}.{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let first = acquire_named(&name);
        assert!(first.is_some(), "первый мьютекс не создан");
        let second = acquire_named(&name);
        assert!(second.is_none(), "второй мьютекс не должен создаваться");
        drop(first);
        let third = acquire_named(&name);
        assert!(third.is_some(), "мьютекс не освободился");
    }
}
```

- [ ] **Шаг 3: Подключить мьютекс в `run()`**

В начале `src-tauri/src/lib.rs` добавить `mod named_mutex;` и в `run()` перед `AppPaths::resolve()`:

```rust
let _instance = match named_mutex::acquire() {
    Some(guard) => guard,
    None => {
        named_mutex::show_already_running();
        return;
    }
};
```

`_instance` живёт до конца `run()`, то есть до закрытия приложения.

- [ ] **Шаг 4: Проверить права окна**

`src-tauri/capabilities/default.json` не должен содержать `window-state` и лишних прав; плагин работает из Rust. Оставить `core:default`, оконные разрешения и `opener:default`.

- [ ] **Шаг 5: Проверить**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
```

Ожидается: тест мьютекса проходит; на не-Windows тест пропускается.

- [ ] **Шаг 6: Коммит**

```bash
git add src-tauri
git -c user.name="Lev" -c user.email="mrvandal94@mail.ru" commit -m "Добавить общий мьютекс и сохранение окна"
```

---

### Задача 7: CI и корневые документы

**Файлы:**
- Создать: `.github/workflows/ci.yml`.
- Изменить: `.gitignore`, `README.md`.

**Интерфейсы:**
- Потребляет: собранный проект из задач 1–6.
- Производит: проверку на windows-2025 x64 на каждый push/PR; старый CI не запускается.

- [ ] **Шаг 1: Написать CI**

`.github/workflows/ci.yml`:

```yaml
name: ci

on:
  push:
  pull_request:
  workflow_dispatch:

permissions:
  contents: read

concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true

jobs:
  windows:
    runs-on: windows-2025
    timeout-minutes: 40
    steps:
      - uses: actions/checkout@v5

      - name: Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: x86_64-pc-windows-msvc
          components: rustfmt, clippy

      - name: Кэш Rust
        uses: Swatinem/rust-cache@v2

      - name: Node
        uses: actions/setup-node@v5
        with:
          node-version: 24
          cache: npm

      - name: Зависимости фронтенда
        run: npm ci

      - name: Проверка типов
        run: npm run check

      - name: Сборка фронтенда
        run: npm run build

      - name: Форматирование Rust
        run: cargo fmt --all --check

      - name: Clippy
        run: cargo clippy --workspace --all-targets -- -D warnings

      - name: Тесты Rust
        run: cargo test --workspace --all-targets

      - name: Сборка окна
        run: npm run tauri build -- --no-bundle
```

- [ ] **Шаг 2: Проверить `.gitignore`**

Корневой `.gitignore` должен содержать `node_modules`, `/build`, `/.svelte-kit`, `/src-tauri/target`, `/target`. Старые правила `bin/`, `obj/` можно оставить.

- [ ] **Шаг 3: Добавить пометку о переезде в README**

В начало `README.md` добавить:

```md
> **Ветка `refactor-rust-poc`.** Идёт переезд на Rust + Tauri 2 + Svelte 5. Спека —
> [docs/superpowers/specs/2026-10-21-rust-tauri-refactor-design.md](../specs/2026-10-21-rust-tauri-refactor-design.md).
> Старое приложение на C#/WPF лежит в `legacy/` как эталон до паритета.
```

- [ ] **Шаг 4: Убедиться, что старый workflow не в корне**

```bash
ls .github/workflows
```

Ожидается только `ci.yml`; `windows.yml` лежит в `legacy/.github/workflows/`.

- [ ] **Шаг 5: Коммит**

```bash
git add .github/workflows/ci.yml .gitignore README.md
git -c user.name="Lev" -c user.email="mrvandal94@mail.ru" commit -m "Добавить CI нового стека"
```

---

### Задача 8: Полная проверка M0

**Файлы:** только исправления, найденные проверками.

**Интерфейсы:**
- Потребляет: всё предыдущее.
- Производит: доказательство, что M0 собирается, запускается и маршруты работают.

- [ ] **Шаг 1: Установить зависимости фронтенда**

```bash
npm ci
```

Если `npm ci` ругается на lock-файл — `npm install`, затем повторить.

- [ ] **Шаг 2: Прогнать проверки**

```bash
npm run check
npm run build
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
```

Ожидается: 0 ошибок, 0 предупреждений.

- [ ] **Шаг 3: Собрать окно локально (ARM64)**

```bash
npm run tauri build -- --no-bundle --debug
```

Ожидается файл `src-tauri/target/debug/BeamModManager.exe`.

Если линковка падает из-за незавершённой установки VS Build Tools — остановиться и попросить пользователя перезагрузить компьютер; не обходить ошибку.

- [ ] **Шаг 4: Дымовой запуск и проверка мьютекса**

```powershell
$exe = "src-tauri/target/debug/BeamModManager.exe"
$p = Start-Process $exe -PassThru
Start-Sleep -Seconds 4
if (-not (Get-Process -Id $p.Id -ErrorAction SilentlyContinue)) { throw "окно не запустилось" }
$p2 = Start-Process $exe -PassThru
Start-Sleep -Seconds 2
$count = @(Get-Process BeamModManager -ErrorAction SilentlyContinue).Count
if ($count -ne 1) { throw "второй экземпляр не заблокирован: $count" }
Stop-Process -Name BeamModManager -Force
```

Ожидается: окно открывается, второй процесс завершается сам, остаётся один.

- [ ] **Шаг 5: Проверить окно**

Запустить `npm run tauri dev`, открыть окно и убедиться:

- заголовка Windows нет, кнопки окна работают;
- слева четыре раздела, они переключают маршруты;
- на «Модах» виден каталог данных `%LOCALAPPDATA%\BeamModManager`;
- тёмная тема и фон на месте;
- в консоли нет ошибок CSP;
- `/dev` открывает старый стенд с 5000 карточек.

- [ ] **Шаг 6: Проверить x64-сборку (по возможности)**

```bash
rustup target add x86_64-pc-windows-msvc
npm run tauri build -- --no-bundle --target x86_64-pc-windows-msvc
```

Если локальных x64-инструментов MSVC нет, это делает CI; зафиксировать результат в отчёте.

- [ ] **Шаг 7: Финальный коммит**

```bash
git status --short
git add -A
git -c user.name="Lev" -c user.email="mrvandal94@mail.ru" commit -m "Завершить каркас M0"
```

---

## Самопроверка плана

- Покрытие спеки M0: перенос старого и PoC — задача 1; workspace и крейты — задача 3; окно, CSP, имя exe — задача 2; данные/`AppPaths` — задача 3; IPC и mock — задача 4; оболочка и маршруты — задача 5; общий мьютекс и сохранение окна — задача 6; CI — задача 7; проверки — задача 8.
- Плейсхолдеров нет; все команды и ключевые файлы приведены.
- Имена согласованы: `AppPaths`, `AppState`, `BootstrapDto`, `bootstrap()`, `app_bootstrap`, `BeamModManager.exe`.
- Следующие этапы (M1–M7) получат отдельные планы; этот план останавливается на рабочем каркасе.
