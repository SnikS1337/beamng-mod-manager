use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, FixedOffset};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use crate::atomic;
use crate::clock::Clock;

use super::{
    CheckState, Inner, InstalledMod, LibraryError, LibraryStore, ModBinding, PageSearch,
    SourceStatus,
};

const MAX_BYTES: u64 = 64 * 1024 * 1024;
const SCHEMA: i32 = 1;
/// Внутри пакета изменений файл пишется не чаще этого: проверка обновлений после долгого
/// перерыва идёт до часа и меняет сотни записей, а при сбое теряется не больше 20 секунд.
const BATCH_FLUSH_EVERY: Duration = Duration::from_secs(20);

struct State {
    inner: Inner,
    /// Поля, которых эта версия программы не знает: их записала более новая.
    unknown: Unknown,
    batch: usize,
    dirty: bool,
    save_blocked: bool,
    save_error: Option<String>,
    /// Когда последний раз писали файл (или начали пакет).
    last_save: Option<Instant>,
}

pub struct JsonLibraryStore {
    path: PathBuf,
    state: Mutex<State>,
    load_warnings: Vec<String>,
}

impl JsonLibraryStore {
    pub fn new(path: impl Into<PathBuf>, now: Clock) -> Self {
        let path = path.into();
        atomic::cleanup_stale_temps(&path);
        let (inner, warnings, save_blocked, unknown) = load(&path, &now);
        Self {
            path,
            state: Mutex::new(State {
                inner,
                unknown,
                batch: 0,
                dirty: false,
                save_blocked,
                save_error: None,
                last_save: None,
            }),
            load_warnings: warnings,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load_warnings(&self) -> &[String] {
        &self.load_warnings
    }

    pub fn save_error(&self) -> Option<String> {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .save_error
            .clone()
    }

    pub fn batch(&self) -> BatchGuard<'_> {
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.batch == 0 {
                state.last_save = Some(Instant::now());
            }
            state.batch += 1;
        }
        BatchGuard { store: self }
    }

    pub fn flush(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.dirty {
            self.save_locked(&mut state);
        }
        !state.dirty
    }

    fn changed(&self, state: &mut State, changed: bool) {
        if !changed {
            return;
        }
        state.dirty = true;
        let overdue = state
            .last_save
            .is_none_or(|saved| saved.elapsed() >= BATCH_FLUSH_EVERY);
        if state.batch == 0 || overdue {
            self.save_locked(state);
        }
    }

    fn save_locked(&self, state: &mut State) {
        if state.save_blocked {
            state.save_error = Some(
                "файл библиотеки был занят при запуске; чтобы не потерять его, изменения не пишутся до перезапуска программы"
                    .to_string(),
            );
            return;
        }

        let model = model_of(&state.inner);
        let json = match serde_json::to_value(&model)
            .map(|value| state.unknown.restore(value))
            .and_then(|value| serde_json::to_string_pretty(&value))
        {
            Ok(json) => json,
            Err(error) => {
                state.save_error = Some(error.to_string());
                return;
            }
        };
        match atomic::write_atomic(&self.path, json.as_bytes()) {
            Ok(()) => {
                state.dirty = false;
                state.save_error = None;
                state.last_save = Some(Instant::now());
            }
            Err(error) => {
                state.save_error = Some(error.to_string());
            }
        }
    }
}

pub struct BatchGuard<'a> {
    store: &'a JsonLibraryStore,
}

impl Drop for BatchGuard<'_> {
    fn drop(&mut self) {
        let mut state = self
            .store
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if state.batch > 0 {
            state.batch -= 1;
        }
        if state.batch == 0 && state.dirty {
            self.store.save_locked(&mut state);
        }
    }
}

impl LibraryStore for JsonLibraryStore {
    fn mods(&self) -> Vec<InstalledMod> {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .inner
            .sorted_mods()
    }

    fn get_mod(&self, id: &str) -> Option<InstalledMod> {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .inner
            .mods
            .get(id)
            .cloned()
    }

    fn upsert_mod(&self, installed: InstalledMod) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let changed = state.inner.upsert_mod(installed);
        self.changed(&mut state, changed);
    }

    fn remove_mod(&self, id: &str) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let changed = state.inner.remove_mod(id);
        self.changed(&mut state, changed);
    }

    fn bindings(&self) -> Vec<ModBinding> {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .inner
            .bindings
            .values()
            .cloned()
            .collect()
    }

    fn upsert_binding(&self, binding: ModBinding) -> Result<(), LibraryError> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        match state.inner.upsert_binding(binding) {
            Ok(changed) => {
                self.changed(&mut state, changed);
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    fn remove_binding(&self, mod_id: &str, source: &str) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let changed = state.inner.remove_binding(mod_id, source);
        self.changed(&mut state, changed);
    }

    fn get_check(&self, mod_id: &str, source: &str) -> Option<CheckState> {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .inner
            .checks
            .get(&(mod_id.to_string(), source.to_string()))
            .cloned()
    }

    fn save_check(&self, check: CheckState) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        // Без привязки проверка не сохраняется — и файл переписывать незачем.
        let changed = state.inner.save_check(check);
        self.changed(&mut state, changed);
    }

    fn checks(&self) -> Vec<CheckState> {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .inner
            .checks
            .values()
            .cloned()
            .collect()
    }

    fn source_status(&self, source: &str) -> SourceStatus {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .inner
            .statuses
            .get(source)
            .copied()
            .unwrap_or_default()
    }

    fn set_source_status(&self, source: &str, status: SourceStatus) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let changed = state.inner.set_source_status(source, status);
        self.changed(&mut state, changed);
    }

    fn source_statuses(&self) -> BTreeMap<String, SourceStatus> {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .inner
            .statuses
            .clone()
    }

    fn last_run(&self) -> Option<DateTime<FixedOffset>> {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .inner
            .last_run
    }

    fn set_last_run(&self, at: DateTime<FixedOffset>) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let changed = state.inner.last_run != Some(at);
        state.inner.last_run = Some(at);
        self.changed(&mut state, changed);
    }

    fn get_dismissed_update(&self, mod_id: &str) -> Option<String> {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .inner
            .dismissed
            .get(mod_id)
            .cloned()
    }

    fn set_dismissed_update(&self, mod_id: &str, key: Option<&str>) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let changed = state.inner.set_dismissed_update(mod_id, key);
        self.changed(&mut state, changed);
    }

    fn file_history(&self, mod_id: &str) -> Vec<String> {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .inner
            .history
            .get(mod_id)
            .cloned()
            .unwrap_or_default()
    }

    fn add_file_history(&self, mod_id: &str, file_name: &str) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let changed = state.inner.add_file_history(mod_id, file_name);
        self.changed(&mut state, changed);
    }

    fn is_checking_declined(&self, mod_id: &str) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .inner
            .declined
            .contains(mod_id)
    }

    fn set_checking_declined(&self, mod_id: &str, declined: bool) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.inner.set_checking_declined(mod_id, declined);
        self.changed(&mut state, true);
    }

    fn get_page_search(&self, mod_id: &str) -> Option<PageSearch> {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .inner
            .page_searches
            .get(mod_id)
            .cloned()
    }

    fn set_page_search(&self, mod_id: &str, search: Option<PageSearch>) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let changed = state.inner.set_page_search(mod_id, search);
        self.changed(&mut state, changed);
    }

    fn get_fallback_search(&self, mod_id: &str) -> Option<DateTime<FixedOffset>> {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .inner
            .fallback_searches
            .get(mod_id)
            .copied()
    }

    fn set_fallback_search(&self, mod_id: &str, at: Option<DateTime<FixedOffset>>) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let changed = state.inner.set_fallback_search(mod_id, at);
        self.changed(&mut state, changed);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct FileModel {
    schema: i32,
    mods: Option<Vec<InstalledMod>>,
    #[serde(default)]
    bindings: Option<Vec<ModBinding>>,
    #[serde(default)]
    checks: Option<Vec<CheckState>>,
    #[serde(default)]
    sources: Option<BTreeMap<String, SourceStatus>>,
    #[serde(default)]
    dismissed: Option<BTreeMap<String, String>>,
    #[serde(default)]
    history: Option<BTreeMap<String, Vec<String>>>,
    #[serde(default)]
    declined: Option<Vec<String>>,
    #[serde(default)]
    page_searches: Option<BTreeMap<String, PageSearch>>,
    #[serde(default)]
    fallback_searches: Option<BTreeMap<String, DateTime<FixedOffset>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_run_utc: Option<DateTime<FixedOffset>>,
}

/// Поля файла, которые знает эта версия.
const KNOWN_ROOT: &[&str] = &[
    "Schema",
    "Mods",
    "Bindings",
    "Checks",
    "Sources",
    "Dismissed",
    "History",
    "Declined",
    "PageSearches",
    "FallbackSearches",
    "LastRunUtc",
];

/// Поля, которых эта версия не знает, — чтобы не выбросить их при сохранении.
///
/// Было: отладочная сборка записала сведения о файле за ссылкой «Скачать», потом
/// запустилась прежняя версия (чтобы обновиться) и сохранила библиотеку без них —
/// у всех 44 модов. Теперь прежняя версия переписывает только то, что знает, а
/// чужие поля файла и записей модов, привязок и проверок оставляет как были.
#[derive(Debug, Default)]
struct Unknown {
    root: Map<String, Value>,
    /// (раздел, запись) → её незнакомые поля.
    records: HashMap<(&'static str, String), Map<String, Value>>,
}

impl Unknown {
    fn record_key(section: &str, value: &Map<String, Value>) -> Option<String> {
        let text = |key: &str| value.get(key).and_then(Value::as_str);
        match section {
            "Mods" => text("Id").map(str::to_string),
            "Bindings" | "Checks" => Some(format!("{}\u{0}{}", text("ModId")?, text("Source")?)),
            _ => None,
        }
    }

    /// Запомнить поля `raw`, которых нет в том, что эта версия сама пишет (`known`).
    fn remember(&mut self, section: &'static str, raw: &Value, known: &Value) {
        let (Some(raw), Some(known)) = (raw.as_object(), known.as_object()) else {
            return;
        };
        let extra: Map<String, Value> = raw
            .iter()
            .filter(|(key, _)| !known.contains_key(*key))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        if extra.is_empty() {
            return;
        }
        if let Some(key) = Self::record_key(section, known) {
            self.records.insert((section, key), extra);
        }
    }

    /// Вернуть незнакомые поля в то, что сейчас сохраняется. Своё не перетирают.
    fn restore(&self, mut value: Value) -> Value {
        let Some(root) = value.as_object_mut() else {
            return value;
        };
        for (key, extra) in &self.root {
            root.entry(key.clone()).or_insert_with(|| extra.clone());
        }
        if self.records.is_empty() {
            return value;
        }
        for section in ["Mods", "Bindings", "Checks"] {
            let Some(Value::Array(items)) = root.get_mut(section) else {
                continue;
            };
            for item in items {
                let Some(object) = item.as_object_mut() else {
                    continue;
                };
                let Some(key) = Self::record_key(section, object) else {
                    continue;
                };
                if let Some(extra) = self.records.get(&(section, key)) {
                    for (name, field) in extra {
                        object.entry(name.clone()).or_insert_with(|| field.clone());
                    }
                }
            }
        }
        value
    }
}

fn model_of(inner: &Inner) -> FileModel {
    let mods = inner.sorted_mods();
    let existing: HashSet<String> = mods.iter().map(|item| item.id.clone()).collect();

    let dismissed = mods
        .iter()
        .filter_map(|item| {
            inner
                .dismissed
                .get(&item.id)
                .map(|key| (item.id.clone(), key.clone()))
        })
        .collect();
    let history = mods
        .iter()
        .filter_map(|item| {
            let names = inner.history.get(&item.id)?;
            if names.is_empty() {
                None
            } else {
                Some((item.id.clone(), names.clone()))
            }
        })
        .collect();
    let declined = mods
        .iter()
        .filter(|item| inner.declined.contains(&item.id))
        .map(|item| item.id.clone())
        .collect();
    let page_searches = mods
        .iter()
        .filter_map(|item| {
            inner
                .page_searches
                .get(&item.id)
                .map(|search| (item.id.clone(), search.clone()))
        })
        .collect();
    let fallback_searches = mods
        .iter()
        .filter_map(|item| {
            inner
                .fallback_searches
                .get(&item.id)
                .map(|at| (item.id.clone(), *at))
        })
        .collect();

    FileModel {
        schema: SCHEMA,
        mods: Some(mods),
        bindings: Some(
            inner
                .bindings
                .values()
                .filter(|binding| existing.contains(&binding.mod_id))
                .cloned()
                .collect(),
        ),
        checks: Some(
            inner
                .checks
                .values()
                .filter(|check| existing.contains(&check.mod_id))
                .cloned()
                .collect(),
        ),
        sources: Some(inner.statuses.clone()),
        dismissed: Some(dismissed),
        history: Some(history),
        declined: Some(declined),
        page_searches: Some(page_searches),
        fallback_searches: Some(fallback_searches),
        last_run_utc: inner.last_run,
    }
}

fn load(path: &Path, now: &Clock) -> (Inner, Vec<String>, bool, Unknown) {
    let mut unknown = Unknown::default();
    let (inner, warnings, blocked) = load_inner(path, now, &mut unknown);
    (inner, warnings, blocked, unknown)
}

fn load_inner(path: &Path, now: &Clock, unknown: &mut Unknown) -> (Inner, Vec<String>, bool) {
    let mut inner = Inner::default();
    let mut warnings = Vec::new();
    if !path.exists() {
        return (inner, warnings, false);
    }

    if let Ok(metadata) = fs::metadata(path)
        && metadata.len() > MAX_BYTES
    {
        let blocked = quarantine(path, now, "файл библиотеки слишком большой", &mut warnings);
        return (inner, warnings, blocked);
    }

    let mut text = None;
    for attempt in 0..4 {
        match fs::read_to_string(path) {
            Ok(value) => {
                text = Some(value);
                break;
            }
            Err(_error) if attempt < 3 => {
                std::thread::sleep(std::time::Duration::from_millis(150 * (attempt + 1)));
            }
            Err(error) => {
                warnings.push(format!(
                    "библиотека не прочиталась ({error}); изменения не сохранятся до перезапуска программы"
                ));
                return (inner, warnings, true);
            }
        }
    }
    let Some(text) = text else {
        return (inner, warnings, true);
    };

    let value: serde_json::Value = match crate::json5_safe::from_str(&text) {
        Ok(value) => value,
        Err(_) => {
            let blocked = quarantine(path, now, "файл библиотеки повреждён", &mut warnings);
            return (inner, warnings, blocked);
        }
    };

    let Some(root) = value.as_object() else {
        let blocked = quarantine(
            path,
            now,
            "файл библиотеки имеет неверный формат",
            &mut warnings,
        );
        return (inner, warnings, blocked);
    };
    if !matches!(root.get("Mods"), Some(serde_json::Value::Array(_))) {
        let blocked = quarantine(
            path,
            now,
            "файл библиотеки имеет неверный формат",
            &mut warnings,
        );
        return (inner, warnings, blocked);
    }

    let schema = match root.get("Schema") {
        None => SCHEMA,
        Some(Value::Number(number)) => number.as_i64().unwrap_or(SCHEMA as i64) as i32,
        Some(_) => {
            warnings.push(
                "поле Schema в библиотеке имеет неверный формат; считаем текущую схему".to_string(),
            );
            SCHEMA
        }
    };
    if schema > SCHEMA {
        warnings.push(format!(
            "библиотека создана более новой версией (схема {schema}); изменения не сохранятся"
        ));
        return (inner, warnings, true);
    }
    if schema < SCHEMA {
        warnings.push(format!(
            "библиотека старой схемы ({schema}); читаем как есть"
        ));
    }

    for (key, value) in root {
        if !KNOWN_ROOT.contains(&key.as_str()) {
            unknown.root.insert(key.clone(), value.clone());
        }
    }

    // Каждую запись разбираем отдельно: одна битая строка не должна обнулять
    // привязки, проверки и историю остальных модов.
    let mut skipped = 0usize;
    for installed in parse_records::<InstalledMod>(root, "Mods", &mut skipped, unknown) {
        if installed.id.trim().is_empty() || installed.file_name.trim().is_empty() {
            skipped += 1;
            continue;
        }
        inner.upsert_mod(installed);
    }

    for binding in parse_records::<ModBinding>(root, "Bindings", &mut skipped, unknown) {
        if inner.upsert_binding(binding).is_err() {
            skipped += 1;
        }
    }

    for check in parse_records::<CheckState>(root, "Checks", &mut skipped, unknown) {
        inner.save_check(check);
    }

    for (source, status) in parse_map::<SourceStatus>(root, "Sources", &mut skipped) {
        inner.statuses.insert(source, status);
    }

    for (mod_id, key) in parse_map::<String>(root, "Dismissed", &mut skipped) {
        inner.set_dismissed_update(&mod_id, Some(&key));
    }

    for (mod_id, names) in parse_map::<Vec<String>>(root, "History", &mut skipped) {
        for name in names {
            inner.add_file_history(&mod_id, &name);
        }
    }

    for mod_id in parse_array::<String>(root, "Declined", &mut skipped) {
        inner.set_checking_declined(&mod_id, true);
    }

    for (mod_id, search) in parse_map::<PageSearch>(root, "PageSearches", &mut skipped) {
        inner.set_page_search(&mod_id, Some(search));
    }

    for (mod_id, at) in parse_map::<DateTime<FixedOffset>>(root, "FallbackSearches", &mut skipped) {
        inner.set_fallback_search(&mod_id, Some(at));
    }

    inner.last_run = root
        .get("LastRunUtc")
        .and_then(|value| serde_json::from_value(value.clone()).ok());

    if skipped > 0 {
        // Следующее сохранение перепишет файл без этих записей. Копию исходника
        // кладём рядом заранее; не вышло — не пишем вовсе, чтобы ничего не потерять.
        match atomic::keep_copy(path, (now)()) {
            Some(copy) => {
                let name = copy
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                warnings.push(format!(
                    "записей библиотеки пропущено из-за неверных данных: {skipped}; исходный файл сохранён как {name}"
                ));
            }
            None => {
                warnings.push(format!(
                    "записей библиотеки пропущено из-за неверных данных: {skipped}; копию сохранить не удалось, изменения не пишутся до перезапуска программы"
                ));
                return (inner, warnings, true);
            }
        }
    }

    (inner, warnings, false)
}

fn parse_array<T: DeserializeOwned>(
    root: &Map<String, Value>,
    key: &str,
    skipped: &mut usize,
) -> Vec<T> {
    let Some(value) = root.get(key) else {
        return Vec::new();
    };
    if value.is_null() {
        return Vec::new();
    }
    let Some(items) = value.as_array() else {
        *skipped += 1;
        return Vec::new();
    };
    let mut result = Vec::new();
    for item in items {
        match serde_json::from_value(item.clone()) {
            Ok(item) => result.push(item),
            Err(_) => *skipped += 1,
        }
    }
    result
}

/// Как [`parse_array`], но запоминает незнакомые поля каждой записи.
fn parse_records<T: DeserializeOwned + Serialize>(
    root: &Map<String, Value>,
    key: &'static str,
    skipped: &mut usize,
    unknown: &mut Unknown,
) -> Vec<T> {
    let Some(Value::Array(items)) = root.get(key) else {
        return parse_array(root, key, skipped);
    };
    let mut result = Vec::new();
    for item in items {
        match serde_json::from_value::<T>(item.clone()) {
            Ok(parsed) => {
                if let Ok(known) = serde_json::to_value(&parsed) {
                    unknown.remember(key, item, &known);
                }
                result.push(parsed);
            }
            Err(_) => *skipped += 1,
        }
    }
    result
}

fn parse_map<T: DeserializeOwned>(
    root: &Map<String, Value>,
    key: &str,
    skipped: &mut usize,
) -> Vec<(String, T)> {
    let Some(value) = root.get(key) else {
        return Vec::new();
    };
    if value.is_null() {
        return Vec::new();
    }
    let Some(entries) = value.as_object() else {
        *skipped += 1;
        return Vec::new();
    };
    let mut result = Vec::new();
    for (name, item) in entries {
        match serde_json::from_value(item.clone()) {
            Ok(item) => result.push((name.clone(), item)),
            Err(_) => *skipped += 1,
        }
    }
    result
}

fn quarantine(path: &Path, now: &Clock, why: &str, warnings: &mut Vec<String>) -> bool {
    let quarantined = atomic::quarantine_file(path, (now)());
    let blocked = quarantined.is_none();
    let warning = match quarantined {
        Some(target) => {
            let name = target
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            format!("{why}: сохранён как {name}, библиотека начата заново")
        }
        None => format!(
            "{why}, файл убрать не удалось; библиотека начата заново, сохранение остановлено"
        ),
    };
    warnings.push(warning);
    blocked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::{Clock, system_clock};
    use chrono::{DateTime, TimeZone, Utc};
    use filetime::{FileTime, set_file_mtime};
    use url::Url;

    fn moment() -> DateTime<chrono::FixedOffset> {
        Utc.with_ymd_and_hms(2026, 10, 21, 12, 0, 0)
            .unwrap()
            .fixed_offset()
    }

    fn fixed_clock() -> Clock {
        let moment = moment();
        std::sync::Arc::new(move || moment)
    }

    fn temp_names(dir: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect()
    }

    fn installed(id: &str, name: &str, version: &str) -> InstalledMod {
        InstalledMod {
            id: id.to_string(),
            name: name.to_string(),
            file_name: format!("{id}.zip"),
            version: Some(version.to_string()),
            size_bytes: Some(100),
            installed_at_utc: moment(),
            enabled: true,
        }
    }

    fn binding(mod_id: &str) -> ModBinding {
        ModBinding {
            mod_id: mod_id.to_string(),
            source: "modland".to_string(),
            source_id: "x".to_string(),
            url: Url::parse("https://modland.net/x").unwrap(),
            confirmed: true,
        }
    }

    fn check(mod_id: &str) -> CheckState {
        CheckState {
            last_file: None,
            mod_id: mod_id.to_string(),
            source: "modland".to_string(),
            last_checked_utc: moment(),
            last_changed_utc: None,
            last_version: Some("1.0".to_string()),
            last_size: Some(100),
            miss_count: 0,
            last_game_version: Some("0.39".to_string()),
            last_title: Some("Some Car".to_string()),
        }
    }

    #[test]
    fn nothing_changed_means_no_write() {
        // Было: удаление несуществующего, проверка без привязки и повтор того же имени
        // в истории каждый раз переписывали весь файл библиотеки.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        let store = JsonLibraryStore::new(&path, system_clock());
        store.upsert_mod(installed("a", "A", "1.0"));
        store.add_file_history("a", "old.zip");
        fs::remove_file(&path).unwrap();

        store.remove_mod("нет");
        store.remove_binding("a", "modland");
        store.save_check(check("a"));
        store.add_file_history("a", "old.zip");
        store.add_file_history("a", "a.zip");
        assert!(!path.exists(), "ничего не поменялось — файл не пишется");

        store.add_file_history("a", "older.zip");
        assert!(path.exists(), "настоящее изменение пишется сразу");
        assert_eq!(store.file_history("a"), ["old.zip", "older.zip"]);
        fs::remove_file(&path).unwrap();
        store.remove_mod("a");
        assert!(path.exists());
        assert!(store.mods().is_empty());
    }

    #[test]
    fn fields_of_a_newer_version_survive_a_save_by_this_one() {
        // Было: сведения о файле за ссылкой, записанные новой версией, пропали у всех
        // модов, когда прежняя версия сохранила библиотеку.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        fs::write(
            &path,
            r#"{"Schema":1,"FutureSection":{"x":1},
            "Mods":[{"Id":"a","Name":"A","FileName":"a.zip","Version":"1.0","SizeBytes":100,
                     "InstalledAtUtc":"2026-10-21T12:00:00+00:00","Enabled":true,"FutureMod":"m"},
                    {"Id":"b","Name":"B","FileName":"b.zip","Version":"1.0","SizeBytes":100,
                     "InstalledAtUtc":"2026-10-21T12:00:00+00:00","Enabled":true,"FutureMod":"gone"}],
            "Bindings":[{"ModId":"a","Source":"modland","SourceId":"x","Url":"https://modland.net/x",
                         "Confirmed":true,"FutureBinding":[1,2]}],
            "Checks":[{"ModId":"a","Source":"modland","LastCheckedUtc":"2026-10-21T12:00:00+00:00",
                       "LastChangedUtc":null,"LastVersion":"1.0","LastSize":100,"MissCount":0,
                       "LastGameVersion":null,"LastTitle":null,"FutureCheck":{"Name":"a_v1.zip"}}]}"#,
        )
        .unwrap();
        {
            let store = JsonLibraryStore::new(&path, system_clock());
            assert!(
                store.load_warnings().is_empty(),
                "{:?}",
                store.load_warnings()
            );
            // Эта версия меняет запись проверки и удаляет мод b.
            let mut changed = check("a");
            changed.last_version = Some("1.1".to_string());
            store.save_check(changed);
            store.remove_mod("b");
        }
        let saved: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["FutureSection"]["x"], 1);
        assert_eq!(
            saved["Mods"].as_array().unwrap().len(),
            1,
            "удалённый мод не воскрес"
        );
        assert_eq!(saved["Mods"][0]["FutureMod"], "m");
        assert_eq!(saved["Bindings"][0]["FutureBinding"][1], 2);
        assert_eq!(saved["Checks"][0]["FutureCheck"]["Name"], "a_v1.zip");
        assert_eq!(
            saved["Checks"][0]["LastVersion"], "1.1",
            "своё поле записано заново, а не взято из старого файла"
        );
    }

    #[test]
    fn records_write_every_known_field_even_when_empty() {
        // На этом держится сохранение чужих полей: незнакомым считается то, чего нет
        // в собственной записи. Пропусти запись пустое поле — его старое значение
        // вернулось бы из файла.
        let mut empty = check("a");
        empty.last_version = None;
        empty.last_size = None;
        empty.last_game_version = None;
        empty.last_title = None;
        let value = serde_json::to_value(&empty).unwrap();
        for key in [
            "LastVersion",
            "LastSize",
            "LastGameVersion",
            "LastTitle",
            "LastFile",
        ] {
            assert!(value.get(key).is_some(), "{key}");
        }
        let mut quiet = installed("a", "A", "1.0");
        quiet.version = None;
        quiet.size_bytes = None;
        let value = serde_json::to_value(&quiet).unwrap();
        for key in ["Version", "SizeBytes"] {
            assert!(value.get(key).is_some(), "{key}");
        }
    }

    #[test]
    fn known_field_cleared_by_this_version_does_not_come_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        {
            let store = JsonLibraryStore::new(&path, system_clock());
            store.upsert_mod(installed("a", "A", "1.0"));
            store.save_check(check("a"));
        }
        {
            let store = JsonLibraryStore::new(&path, system_clock());
            let mut cleared = check("a");
            cleared.last_title = None;
            store.save_check(cleared);
        }
        let saved: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert!(saved["Checks"][0]["LastTitle"].is_null());
    }

    #[test]
    fn round_trip_everything() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        {
            let store = JsonLibraryStore::new(&path, system_clock());
            store.upsert_mod(installed("a", "A", "1.0"));
            store.upsert_binding(binding("a")).unwrap();
            store.save_check(check("a"));
            store.set_source_status("modland", SourceStatus::NeedsHumanCheck);
            store.set_dismissed_update("a", Some("1.0"));
            store.add_file_history("a", "a_old.zip");
            store.set_checking_declined("a", true);
            store.set_page_search(
                "a",
                Some(PageSearch {
                    at: moment(),
                    rejected: true,
                }),
            );
            store.set_fallback_search("a", Some(moment()));
            store.set_last_run(moment());
            assert!(temp_names(dir.path()).is_empty());
        }

        let loaded = JsonLibraryStore::new(&path, system_clock());
        assert!(
            loaded.load_warnings().is_empty(),
            "{:?}",
            loaded.load_warnings()
        );
        assert_eq!(loaded.mods().len(), 1);
        assert_eq!(loaded.mods()[0].name, "A");
        assert_eq!(loaded.bindings().len(), 1);
        assert!(loaded.get_check("a", "modland").is_some());
        assert_eq!(
            loaded.source_status("modland"),
            SourceStatus::NeedsHumanCheck
        );
        assert_eq!(loaded.get_dismissed_update("a").as_deref(), Some("1.0"));
        assert_eq!(loaded.file_history("a"), vec!["a_old.zip"]);
        assert!(loaded.is_checking_declined("a"));
        assert!(loaded.get_page_search("a").unwrap().rejected);
        assert_eq!(loaded.get_fallback_search("a"), Some(moment()));
        assert_eq!(loaded.last_run(), Some(moment()));
    }

    #[test]
    fn fallback_search_mark_goes_with_the_mod() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        let store = JsonLibraryStore::new(&path, system_clock());
        store.set_fallback_search("ghost", Some(moment()));
        assert_eq!(
            store.get_fallback_search("ghost"),
            None,
            "мода нет — отметки нет"
        );
        store.upsert_mod(installed("a", "A", "1.0"));
        store.set_fallback_search("a", Some(moment()));
        store.remove_mod("a");
        store.upsert_mod(installed("a", "A", "1.0"));
        assert_eq!(
            store.get_fallback_search("a"),
            None,
            "удалённый мод забыт целиком"
        );
        store.set_fallback_search("a", Some(moment()));
        store.set_fallback_search("a", None);
        assert_eq!(store.get_fallback_search("a"), None);
    }

    #[test]
    fn batch_writes_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        let store = JsonLibraryStore::new(&path, system_clock());
        {
            let _guard = store.batch();
            store.upsert_mod(installed("a", "A", "1.0"));
            store.upsert_mod(installed("b", "B", "1.0"));
            assert!(!path.exists());
        }
        assert!(path.exists());
        let loaded = JsonLibraryStore::new(&path, system_clock());
        assert_eq!(loaded.mods().len(), 2);
    }

    #[test]
    fn flush_during_batch_saves_what_is_pending() {
        // Программу закрыли посреди долгой проверки: пакет ещё открыт в другом потоке,
        // а найденное до этого момента должно лечь на диск.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        let store = JsonLibraryStore::new(&path, system_clock());
        let _guard = store.batch();
        store.upsert_mod(installed("a", "A", "1.0"));
        assert!(!path.exists());
        assert!(store.flush());
        let loaded = JsonLibraryStore::new(&path, system_clock());
        assert_eq!(loaded.mods().len(), 1);
    }

    #[test]
    fn nested_batch_writes_only_after_outer() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        let store = JsonLibraryStore::new(&path, system_clock());
        {
            let _outer = store.batch();
            store.upsert_mod(installed("a", "A", "1.0"));
            {
                let _inner = store.batch();
                store.upsert_mod(installed("b", "B", "1.0"));
                assert!(!path.exists());
            }
            store.upsert_mod(installed("c", "C", "1.0"));
            assert!(!path.exists());
        }
        assert!(path.exists());
        let loaded = JsonLibraryStore::new(&path, system_clock());
        assert_eq!(loaded.mods().len(), 3);
    }

    #[test]
    fn remove_mod_cleans_bindings_and_checks_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        {
            let store = JsonLibraryStore::new(&path, system_clock());
            store.upsert_mod(installed("a", "A", "1.0"));
            store.upsert_mod(installed("b", "B", "1.0"));
            store.upsert_binding(binding("a")).unwrap();
            store.upsert_binding(binding("b")).unwrap();
            store.save_check(check("a"));
            store.save_check(check("b"));
            store.remove_mod("a");
        }
        let loaded = JsonLibraryStore::new(&path, system_clock());
        assert_eq!(loaded.mods().len(), 1);
        assert_eq!(loaded.bindings().len(), 1);
        assert_eq!(loaded.checks().len(), 1);
        assert!(loaded.get_check("a", "modland").is_none());
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("\"ModId\": \"a\""), "{text}");
    }

    #[test]
    fn corrupted_file_is_quarantined() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        fs::write(&path, "{ это не json").unwrap();
        let store = JsonLibraryStore::new(&path, system_clock());
        assert_eq!(store.load_warnings().len(), 1);
        assert!(store.mods().is_empty());
        assert!(!path.exists());
        store.upsert_mod(installed("a", "A", "1.0"));
        assert!(path.exists());
    }

    #[test]
    fn partly_broken_library_keeps_a_copy_before_rewriting() {
        // Битые записи пропускаются, и следующее сохранение писало файл уже без них —
        // данные терялись насовсем. Теперь исходник остаётся рядом.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        let original = r#"{"Schema":1,"Mods":[{"Id":"a","Name":"A","FileName":"a.zip","InstalledAtUtc":"2026-10-01T00:00:00+00:00","Enabled":true},{"Id":42}]}"#;
        fs::write(&path, original).unwrap();
        let store = JsonLibraryStore::new(&path, fixed_clock());
        assert!(
            store
                .load_warnings()
                .iter()
                .any(|warning| warning.contains("сохранён как")),
            "{:?}",
            store.load_warnings()
        );
        store.upsert_mod(installed("b", "B", "1.0"));
        assert!(store.flush());
        let copies: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".bad-"))
            .map(|entry| fs::read_to_string(entry.path()).unwrap())
            .collect();
        assert_eq!(copies, vec![original.to_string()]);

        // Тот же битый файл при следующем запуске не плодит копий.
        fs::write(&path, original).unwrap();
        let _again = JsonLibraryStore::new(&path, fixed_clock());
        let count = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".bad-"))
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn too_deep_file_is_quarantined_not_a_crash() {
        // json5 без предела вложенности переполнял стек — программа закрывалась
        // ещё до окна.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        fs::write(&path, format!("{{\"Mods\":{}", "[".repeat(100_000))).unwrap();
        let store = JsonLibraryStore::new(&path, system_clock());
        assert_eq!(store.load_warnings().len(), 1);
        assert!(store.mods().is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn unremovable_bad_library_blocks_saving() {
        use std::os::windows::fs::OpenOptionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        fs::write(&path, "{ это не json").unwrap();
        let _lock = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        let store = JsonLibraryStore::new(&path, system_clock());
        assert!(store.mods().is_empty());
        assert!(
            store
                .load_warnings()
                .iter()
                .any(|warning| warning.contains("остановлено")),
            "{:?}",
            store.load_warnings()
        );
        store.upsert_mod(installed("a", "A", "1.0"));
        assert!(store.save_error().is_some());
        assert!(!store.flush());
    }

    #[test]
    fn second_quarantine_keeps_first_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        fs::write(&path, "{ первый").unwrap();
        let first = JsonLibraryStore::new(&path, fixed_clock());
        assert_eq!(first.load_warnings().len(), 1);
        fs::write(&path, "{ второй").unwrap();
        let second = JsonLibraryStore::new(&path, fixed_clock());
        assert_eq!(second.load_warnings().len(), 1);

        let backups: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".bad-"))
            .map(|entry| fs::read_to_string(entry.path()).unwrap())
            .collect();
        assert_eq!(backups.len(), 2, "{backups:?}");
        assert!(backups.contains(&"{ первый".to_string()), "{backups:?}");
        assert!(backups.contains(&"{ второй".to_string()), "{backups:?}");
    }

    #[test]
    fn null_collections_load_without_warnings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        fs::write(
            &path,
            r#"{
                "Schema": 1,
                "Mods": [{"Id":"a","Name":"A","FileName":"a.zip","InstalledAtUtc":"2026-10-15T12:00:00+00:00","Enabled":true}],
                "Bindings": null,
                "Checks": null,
                "Sources": null,
                "Dismissed": null,
                "History": null,
                "Declined": null,
                "PageSearches": null
            }"#,
        )
        .unwrap();
        let store = JsonLibraryStore::new(&path, system_clock());
        assert!(
            store.load_warnings().is_empty(),
            "{:?}",
            store.load_warnings()
        );
        assert_eq!(store.mods().len(), 1);
        assert!(store.bindings().is_empty());
        assert!(store.checks().is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn locked_library_at_start_is_not_overwritten() {
        use std::os::windows::fs::OpenOptionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        {
            let store = JsonLibraryStore::new(&path, system_clock());
            store.upsert_mod(installed("a", "A", "1.0"));
            store.upsert_binding(binding("a")).unwrap();
        }
        let before = fs::read_to_string(&path).unwrap();
        let store = {
            let _lock = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(0)
                .open(&path)
                .unwrap();
            JsonLibraryStore::new(&path, system_clock())
        };

        assert_eq!(store.load_warnings().len(), 1);
        assert!(store.mods().is_empty());
        store.upsert_mod(installed("b", "B", "1.0"));
        assert!(store.save_error().is_some());
        assert!(!store.flush());
        assert_eq!(fs::read_to_string(&path).unwrap(), before);
    }

    #[test]
    fn one_bad_mod_does_not_wipe_library() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        fs::write(
            &path,
            r#"{
                "Schema": 1,
                "Mods": [
                    {"Id":"a","Name":"A","FileName":"a.zip","InstalledAtUtc":"2026-10-15T12:00:00+00:00","Enabled":true},
                    {"Id":"b","Name":"B","FileName":"b.zip","InstalledAtUtc":"2026-10-15T12:00:00","Enabled":true}
                ],
                "Bindings": [
                    {"ModId":"a","Source":"stmods","SourceId":"x","Url":"https://stmods.org/x/","Confirmed":true}
                ],
                "Checks": [],
                "Sources": {},
                "Dismissed": {"a":"1.0"},
                "History": {"a":["a_old.zip"]},
                "Declined": []
            }"#,
        )
        .unwrap();
        let store = JsonLibraryStore::new(&path, system_clock());
        assert_eq!(store.mods().len(), 1, "живая запись осталась");
        assert_eq!(store.mods()[0].id, "a");
        assert_eq!(store.bindings().len(), 1);
        assert_eq!(store.get_dismissed_update("a").as_deref(), Some("1.0"));
        assert_eq!(store.file_history("a"), vec!["a_old.zip"]);
        assert!(
            store
                .load_warnings()
                .iter()
                .any(|warning| warning.contains("пропущено")),
            "{:?}",
            store.load_warnings()
        );
        assert!(path.exists(), "файл не уводится в карантин");
    }

    #[test]
    fn missing_schema_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        fs::write(
            &path,
            r#"{"Mods":[{"Id":"a","Name":"A","FileName":"a.zip","InstalledAtUtc":"2026-10-15T12:00:00+00:00","Enabled":true}]}"#,
        )
        .unwrap();
        let store = JsonLibraryStore::new(&path, system_clock());
        assert_eq!(store.mods().len(), 1);
        assert!(
            store.load_warnings().is_empty(),
            "{:?}",
            store.load_warnings()
        );
    }

    #[test]
    fn newer_schema_is_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        let text = r#"{"Schema":2,"Mods":[{"Id":"a","Name":"A","FileName":"a.zip","InstalledAtUtc":"2026-10-15T12:00:00+00:00","Enabled":true}]}"#;
        fs::write(&path, text).unwrap();
        let store = JsonLibraryStore::new(&path, system_clock());
        assert!(store.mods().is_empty());
        assert!(
            store
                .load_warnings()
                .iter()
                .any(|warning| warning.contains("более новой")),
            "{:?}",
            store.load_warnings()
        );
        store.upsert_mod(installed("b", "B", "1.0"));
        assert!(store.save_error().is_some());
        assert!(!store.flush());
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }

    #[test]
    fn bad_entries_are_skipped_with_one_warning() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        fs::write(
            &path,
            r#"{
                "Schema": 1,
                "Mods": [
                    {"Id":"a","Name":"A","FileName":"a.zip","InstalledAtUtc":"2026-10-15T12:00:00+00:00","Enabled":true},
                    {"Id":"","Name":"?","FileName":"b.zip","InstalledAtUtc":"2026-10-15T12:00:00+00:00"}
                ],
                "Bindings": [
                    {"ModId":"нет","Source":"stmods","SourceId":"x","Url":"https://stmods.org/x/","Confirmed":true}
                ],
                "Checks": [],
                "Sources": {},
                "Dismissed": {},
                "History": {},
                "Declined": []
            }"#,
        )
        .unwrap();
        let store = JsonLibraryStore::new(&path, system_clock());
        assert_eq!(store.mods().len(), 1);
        assert!(store.bindings().is_empty());
        assert_eq!(
            store.load_warnings().len(),
            1,
            "{:?}",
            store.load_warnings()
        );
        assert!(store.load_warnings()[0].contains("пропущено"));
    }

    #[test]
    fn invalid_roots_are_quarantined() {
        for text in ["[1,2,3]", "{}", "null"] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("library.json");
            fs::write(&path, text).unwrap();
            let store = JsonLibraryStore::new(&path, system_clock());
            assert!(store.mods().is_empty(), "{text}");
            assert_eq!(store.load_warnings().len(), 1, "{text}");
        }
    }

    #[test]
    fn save_failure_is_reported_and_recovers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        let store = JsonLibraryStore::new(&path, system_clock());
        fs::create_dir(&path).unwrap();
        store.upsert_mod(installed("a", "A", "1.0"));
        assert!(store.save_error().is_some());
        assert!(!store.flush());
        fs::remove_dir(&path).unwrap();
        assert!(store.flush());
        assert!(store.save_error().is_none());
        assert!(temp_names(dir.path()).is_empty());
    }

    #[test]
    fn same_data_does_not_rewrite_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        let store = JsonLibraryStore::new(&path, system_clock());
        let item = installed("a", "A", "1.0");
        store.upsert_mod(item.clone());
        let old = FileTime::from_unix_time(946_684_800, 0);
        set_file_mtime(&path, old).unwrap();

        store.upsert_mod(item);
        assert_eq!(
            FileTime::from_last_modification_time(&fs::metadata(&path).unwrap()),
            old
        );

        store.upsert_mod(installed("a", "A", "1.1"));
        assert_ne!(
            FileTime::from_last_modification_time(&fs::metadata(&path).unwrap()),
            old
        );
    }

    #[test]
    fn dates_and_enums_have_old_json_shape() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        let store = JsonLibraryStore::new(&path, system_clock());
        store.upsert_mod(installed("a", "A", "1.0"));
        store.set_source_status("stmods", SourceStatus::Unavailable);
        let text = fs::read_to_string(&path).unwrap();
        let date_line = text
            .lines()
            .find(|line| line.contains("\"InstalledAtUtc\""))
            .unwrap();
        assert!(date_line.contains("2026-10-21T12:00:00"), "{text}");
        assert!(
            date_line.contains('Z') || date_line.contains("+00:00"),
            "{text}"
        );
        assert!(text.contains("\"Unavailable\""), "{text}");
        assert!(text.contains("\"PageSearches\": {}"), "{text}");
    }
}
