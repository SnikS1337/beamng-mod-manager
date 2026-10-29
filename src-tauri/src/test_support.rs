//! Общее для тестов: состояние программы во временной папке и сверка строковых
//! значений из Rust с `src/lib/ipc/unions.ts`.
//!
//! Формы полей DTO в TypeScript рождаются из Rust (`ts_export`), а вот строки
//! (`kind`, `status`, `phase`) ts-rs не видит: в Rust это `String`. Их и сверяем.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use bmm_domain::clock::system_clock;
use bmm_domain::library::JsonLibraryStore;
use bmm_domain::paths::AppPaths;
use bmm_domain::settings::AppSettings;
use bmm_domain::settings_store::SettingsStore;
use bmm_packs::cache::PreviewCache;

use crate::state::AppState;

/// Состояние программы с папкой данных во `temp` и папкой модов `mods`.
pub fn test_state(temp: &tempfile::TempDir, mods: &Path) -> AppState {
    let data = temp.path().join("data");
    let paths = AppPaths::at(&data);
    paths.ensure_dirs().unwrap();
    let settings = SettingsStore::with_system_clock(paths.settings());
    settings
        .save(&AppSettings {
            mods_folder: Some(mods.display().to_string()),
            ..AppSettings::default()
        })
        .unwrap();
    AppState {
        paths,
        settings,
        library: Arc::new(JsonLibraryStore::new(
            temp.path().join("data/library.json"),
            system_clock(),
        )),
        previews: PreviewCache::new(temp.path().join("data/previews")),
        load_lock: Mutex::new(()),
        previews_by_id: Mutex::new(HashMap::new()),
        pending: Mutex::new(HashMap::new()),
        preview_gate: Arc::new(tokio::sync::Semaphore::new(4)),
        deleted: Mutex::new(HashMap::new()),
        content_cache: Mutex::new(HashMap::new()),
        search: None,
        update: None,
        app_update: None,
        search_cancel: Mutex::new(None),
        last_search: Mutex::new(None),
        browse: Mutex::new(None),
    }
}

fn ipc_file(file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../src/lib/ipc")
        .join(file)
}

/// Значения строкового union-типа: `export type X = "a" | "b";`.
pub fn ts_union(file: &str, name: &str) -> BTreeSet<String> {
    let text = std::fs::read_to_string(ipc_file(file)).unwrap();
    let head = format!("export type {name} =");
    let start = text
        .find(&head)
        .unwrap_or_else(|| panic!("в {file} нет типа {name}"));
    let body = &text[start + head.len()..];
    let end = body.find(';').unwrap();
    let mut values = BTreeSet::new();
    for part in body[..end].split('|') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix('"').and_then(|p| p.strip_suffix('"')) {
            values.insert(value.to_string());
        } else if !part.is_empty() {
            // Ссылка на другой тип: SearchPrice | "mixed".
            values.extend(ts_union(file, part));
        }
    }
    values
}

// ── Команды: окно вызывает их по имени и передаёт аргументы по именам ─────────

fn sources_under(dir: &Path, extensions: &[&str], out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources_under(&path, extensions, out);
        } else if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| extensions.contains(&ext))
            && !path.to_string_lossy().contains(".test.")
        {
            out.push(path);
        }
    }
}

/// Текст от открывающей скобки до парной закрывающей (без них).
fn balanced(text: &str, open: char, close: char) -> Option<&str> {
    let mut depth = 0usize;
    for (index, c) in text.char_indices() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(&text[open.len_utf8()..index]);
            }
        }
    }
    None
}

/// Части через запятую верхнего уровня: `a, b: f(x, y), c` → три части.
fn top_level_parts(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    for c in text.chars() {
        match c {
            '(' | '{' | '[' | '<' => depth += 1,
            ')' | '}' | ']' | '>' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    parts.push(current);
    parts
        .into_iter()
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect()
}

fn identifier(text: &str) -> String {
    text.trim()
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect()
}

/// Вызовы `invoke("имя", { ключи })` в коде окна.
fn window_calls() -> Vec<(String, BTreeSet<String>, PathBuf)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src");
    let mut files = Vec::new();
    sources_under(&root, &["ts", "svelte"], &mut files);
    let mut calls = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        let mut rest = text.as_str();
        while let Some(found) = rest.find("invoke") {
            rest = &rest[found + "invoke".len()..];
            let mut after = rest;
            if after.starts_with('<') {
                let Some(generic) = balanced(after, '<', '>') else {
                    continue;
                };
                after = &after[generic.len() + 2..];
            }
            let Some(after) = after.strip_prefix('(') else {
                continue;
            };
            let after = after.trim_start();
            let Some(after) = after.strip_prefix('"') else {
                continue;
            };
            let Some(end) = after.find('"') else { continue };
            let name = after[..end].to_string();
            let tail = after[end + 1..].trim_start();
            let mut keys = BTreeSet::new();
            if let Some(tail) = tail.strip_prefix(',') {
                let tail = tail.trim_start();
                assert!(
                    tail.starts_with('{'),
                    "{}: аргументы «{name}» передаются не литералом — проверка их не видит",
                    file.display()
                );
                let body = balanced(tail, '{', '}').unwrap();
                for part in top_level_parts(body) {
                    keys.insert(identifier(part.split(':').next().unwrap()));
                }
            }
            calls.push((name, keys, file.clone()));
        }
    }
    calls
}

fn snake_to_camel(name: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for c in name.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Команды Rust: имя → (все аргументы, обязательные) в том виде, как их ждёт окно.
fn rust_commands() -> HashMap<String, (BTreeSet<String>, BTreeSet<String>)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands");
    let mut files = Vec::new();
    sources_under(&dir, &["rs"], &mut files);
    let mut commands = HashMap::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        for (index, _) in text.match_indices("#[tauri::command]") {
            let after = &text[index..];
            let start = after.find("fn ").unwrap() + 3;
            let name = identifier(&after[start..]);
            let open = after[start..].find('(').unwrap() + start;
            let params = balanced(&after[open..], '(', ')').unwrap();
            let mut all = BTreeSet::new();
            let mut required = BTreeSet::new();
            for param in top_level_parts(params) {
                let (pname, ptype) = param.split_once(':').unwrap();
                let ptype = ptype.trim();
                if ["State<", "AppHandle", "Window", "WebviewWindow"]
                    .iter()
                    .any(|skip| ptype.contains(skip))
                {
                    continue;
                }
                let key = snake_to_camel(pname.trim().trim_start_matches("mut "));
                if !ptype.starts_with("Option<") {
                    required.insert(key.clone());
                }
                all.insert(key);
            }
            commands.insert(name, (all, required));
        }
    }
    commands
}

/// Имена из `tauri::generate_handler![…]` — только они доступны окну.
fn registered_commands() -> BTreeSet<String> {
    let text =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs")).unwrap();
    let start = text.find("generate_handler![").unwrap();
    let body = balanced(&text[start + "generate_handler!".len()..], '[', ']').unwrap();
    top_level_parts(body)
        .iter()
        .map(|path| path.rsplit("::").next().unwrap().trim().to_string())
        .collect()
}

#[test]
fn every_window_call_reaches_a_command_with_the_same_arguments() {
    // Опечатка в имени команды или аргумента не ловится ни компилятором, ни
    // проверкой типов окна — только ошибкой у пользователя при нажатии.
    let calls = window_calls();
    assert!(calls.len() > 30, "вызовы окна не нашлись: {}", calls.len());
    let registered = registered_commands();
    let commands = rust_commands();
    let mut problems = Vec::new();
    for (name, keys, file) in &calls {
        if !registered.contains(name) {
            problems.push(format!(
                "{}: команды «{name}» нет в generate_handler",
                file.display()
            ));
            continue;
        }
        let Some((all, required)) = commands.get(name) else {
            problems.push(format!("команда «{name}» зарегистрирована, но не найдена"));
            continue;
        };
        let unknown: Vec<_> = keys.difference(all).collect();
        let missing: Vec<_> = required.difference(keys).collect();
        if !unknown.is_empty() || !missing.is_empty() {
            problems.push(format!(
                "{}: «{name}» — лишние аргументы {unknown:?}, не хватает {missing:?}",
                file.display()
            ));
        }
    }
    let called: BTreeSet<&String> = calls.iter().map(|(name, _, _)| name).collect();
    for name in &registered {
        if !called.contains(name) {
            problems.push(format!(
                "команда «{name}» зарегистрирована, но окно её не вызывает"
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn reads_arguments_from_object_literals() {
    assert_eq!(
        top_level_parts("modId: id, url, backupId: backupId ?? null, f: g(a, b)"),
        [
            "modId: id",
            "url",
            "backupId: backupId ?? null",
            "f: g(a, b)"
        ]
    );
    assert_eq!(snake_to_camel("search_again"), "searchAgain");
    assert_eq!(balanced("<A<B>>(x)", '<', '>'), Some("A<B>"));
}
