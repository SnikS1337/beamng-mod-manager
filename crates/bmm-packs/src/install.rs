use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};
use sha2::{Digest, Sha256};

use bmm_domain::clock::Clock;

use crate::inspect::{ArchiveLimits, InspectionResult, inspect_zip};
use crate::safe_name::safe_file_name;
use crate::scan::ModLayout;

#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub allow_warnings: bool,
    pub replace_existing: bool,
    pub limits: ArchiveLimits,
    pub max_archive_bytes: u64,
}

impl Default for InstallOptions {
    fn default() -> Self {
        Self {
            allow_warnings: false,
            replace_existing: false,
            limits: ArchiveLimits::default(),
            max_archive_bytes: 8 * 1024 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallError {
    BadName,
    Blocked,
    HasWarnings,
    AlreadyExists,
    Io,
    InUse,
    TooLarge,
}

#[derive(Debug)]
pub struct InstallResult {
    pub success: bool,
    pub error: Option<InstallError>,
    pub message: Option<String>,
    pub path: Option<PathBuf>,
    pub sha256: Option<String>,
    pub inspection: Option<InspectionResult>,
}

impl InstallResult {
    fn fail(error: InstallError, message: impl Into<String>) -> Self {
        Self {
            success: false,
            error: Some(error),
            message: Some(message.into()),
            path: None,
            sha256: None,
            inspection: None,
        }
    }

    fn fail_with_inspection(
        error: InstallError,
        message: impl Into<String>,
        inspection: InspectionResult,
    ) -> Self {
        Self {
            success: false,
            error: Some(error),
            message: Some(message.into()),
            path: None,
            sha256: None,
            inspection: Some(inspection),
        }
    }
}

#[derive(Debug)]
pub struct UpdateResult {
    pub install: InstallResult,
    pub old_removed: bool,
    pub was_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpResult {
    pub success: bool,
    pub message: String,
    pub path: Option<PathBuf>,
}

impl OpResult {
    pub(crate) fn ok(message: impl Into<String>, path: Option<PathBuf>) -> Self {
        Self {
            success: true,
            message: message.into(),
            path,
        }
    }

    pub(crate) fn fail(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            path: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BackupEntry {
    pub id: String,
    pub file_name: String,
    pub sequence: i64,
    pub saved_at: Option<DateTime<FixedOffset>>,
    pub size_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct DeletedMod {
    pub file_name: String,
    pub copies: usize,
    pub last_saved_at: Option<DateTime<FixedOffset>>,
    pub last_size_bytes: u64,
}

pub struct ModInstaller {
    layout: ModLayout,
    now: Clock,
    keep_backups: Arc<dyn Fn() -> usize + Send + Sync>,
    gate: Mutex<()>,
}

impl ModInstaller {
    pub fn new(
        layout: ModLayout,
        now: Clock,
        keep_backups: Arc<dyn Fn() -> usize + Send + Sync>,
    ) -> Self {
        Self {
            layout,
            now,
            keep_backups,
            gate: Mutex::new(()),
        }
    }

    pub fn layout(&self) -> &ModLayout {
        &self.layout
    }

    pub fn mods_folder_available(&self) -> bool {
        self.layout.mods_dir.is_dir()
    }

    pub fn install_from_path(
        &self,
        source: &Path,
        file_name: &str,
        options: InstallOptions,
    ) -> InstallResult {
        let Some(name) = safe_file_name(Some(file_name)) else {
            return InstallResult::fail(
                InstallError::BadName,
                "Недопустимое имя файла (нужен .zip без путей).",
            );
        };
        let temp = self
            .layout
            .mods_dir
            .join(format!(".{}.part", unique_suffix()));
        let result = self.install_inner(source, &name, &temp, &options);
        let _ = fs::remove_file(&temp);
        result
    }

    fn install_inner(
        &self,
        source: &Path,
        name: &str,
        temp: &Path,
        options: &InstallOptions,
    ) -> InstallResult {
        if let Err(message) = fs::create_dir_all(&self.layout.mods_dir) {
            return InstallResult::fail(InstallError::Io, message.to_string());
        }
        if let Some(existing) = self.check_target(name, options) {
            return existing;
        }

        let copy = copy_with_hash(source, temp, options.max_archive_bytes);
        let hash = match copy {
            Ok(hash) => hash,
            Err(error) => return InstallResult::fail(error.0, error.1),
        };

        let inspection = inspect_zip(temp, &options.limits);
        if inspection.is_blocked() {
            return InstallResult::fail_with_inspection(
                InstallError::Blocked,
                "Архив не прошёл проверку безопасности.",
                inspection,
            );
        }
        if inspection.has_warnings() && !options.allow_warnings {
            return InstallResult::fail_with_inspection(
                InstallError::HasWarnings,
                "В архиве есть предупреждения: нужно ваше решение.",
                inspection,
            );
        }

        let _guard = self.gate.lock().unwrap();
        if let Some(existing) = self.check_target(name, options) {
            return existing;
        }
        let target = self.target_path(name);
        let mut moved: Option<PathBuf> = None;
        if target.exists() {
            match self.backup_file(&target, name) {
                Ok(path) => moved = Some(path),
                Err(error) => return InstallResult::fail_from_io(error),
            }
        }
        if let Some(parent) = target.parent()
            && let Err(error) = fs::create_dir_all(parent)
        {
            self.restore_moved(&moved, &target);
            return InstallResult::fail(InstallError::Io, error.to_string());
        }
        if let Err(error) = fs::rename(temp, &target) {
            self.restore_moved(&moved, &target);
            return InstallResult::fail_from_io(error);
        }
        self.rotate(name);

        InstallResult {
            success: true,
            error: None,
            message: None,
            path: Some(target),
            sha256: Some(hash),
            inspection: Some(inspection),
        }
    }

    pub fn update_from_path(
        &self,
        old_file_name: &str,
        source: &Path,
        new_file_name: &str,
        options: InstallOptions,
    ) -> UpdateResult {
        let Some(old_name) = safe_file_name(Some(old_file_name)) else {
            return UpdateResult {
                install: InstallResult::fail(
                    InstallError::BadName,
                    "Недопустимое имя файла (нужен .zip без путей).",
                ),
                old_removed: false,
                was_enabled: true,
            };
        };
        let Some(new_name) = safe_file_name(Some(new_file_name)) else {
            return UpdateResult {
                install: InstallResult::fail(
                    InstallError::BadName,
                    "Недопустимое имя файла (нужен .zip без путей).",
                ),
                old_removed: false,
                was_enabled: true,
            };
        };

        let (old_exists, was_enabled) = {
            let in_mods = self.layout.mods_dir.join(&old_name).is_file();
            let in_disabled = self.layout.disabled_dir.join(&old_name).is_file();
            (in_mods || in_disabled, !in_disabled || in_mods)
        };
        let same_name = old_name.eq_ignore_ascii_case(&new_name);
        if !same_name
            && (self.layout.mods_dir.join(&new_name).is_file()
                || self.layout.disabled_dir.join(&new_name).is_file())
        {
            return UpdateResult {
                install: InstallResult::fail(
                    InstallError::AlreadyExists,
                    "Файл с именем новой версии уже есть в папке.",
                ),
                old_removed: false,
                was_enabled,
            };
        }

        let options = InstallOptions {
            allow_warnings: options.allow_warnings,
            limits: options.limits,
            max_archive_bytes: options.max_archive_bytes,
            replace_existing: true,
        };
        let install = self.install_from_path(source, &new_name, options);
        if !install.success || same_name || !old_exists {
            return UpdateResult {
                install,
                old_removed: same_name,
                was_enabled,
            };
        }

        let removed = self.remove(&old_name);
        if !removed.success {
            let _ = fs::remove_file(self.layout.mods_dir.join(&new_name));
            return UpdateResult {
                install: InstallResult::fail(InstallError::InUse, removed.message),
                old_removed: false,
                was_enabled,
            };
        }
        if !was_enabled {
            let off = self.disable(&new_name);
            if !off.success {
                return UpdateResult {
                    install: InstallResult {
                        message: Some(format!(
                            "Обновлено, но мод остался включённым: {}",
                            off.message
                        )),
                        ..install
                    },
                    old_removed: true,
                    was_enabled,
                };
            }
        }
        UpdateResult {
            install,
            old_removed: true,
            was_enabled,
        }
    }

    pub fn disable(&self, file_name: &str) -> OpResult {
        self.move_file(
            file_name,
            &self.layout.mods_dir,
            &self.layout.disabled_dir,
            "выключен",
        )
    }

    pub fn enable(&self, file_name: &str) -> OpResult {
        self.move_file(
            file_name,
            &self.layout.disabled_dir,
            &self.layout.mods_dir,
            "включён",
        )
    }

    fn move_file(&self, file_name: &str, from: &Path, to: &Path, verb: &str) -> OpResult {
        let Some(name) = safe_file_name(Some(file_name)) else {
            return OpResult::fail("Недопустимое имя файла.");
        };
        let _guard = self.gate.lock().unwrap();
        let src = from.join(&name);
        let dst = to.join(&name);
        if !src.is_file() {
            return OpResult::fail("Файл не найден.");
        }
        if dst.exists() {
            return OpResult::fail("В папке уже есть файл с таким именем.");
        }
        if let Err(error) = fs::create_dir_all(to) {
            return OpResult::fail(io_message(&error));
        }
        match fs::rename(&src, &dst) {
            Ok(()) => OpResult::ok(format!("Мод {verb}."), Some(dst)),
            Err(error) => OpResult::fail(io_message(&error)),
        }
    }

    pub fn remove(&self, file_name: &str) -> OpResult {
        let Some(name) = safe_file_name(Some(file_name)) else {
            return OpResult::fail("Недопустимое имя файла.");
        };
        let _guard = self.gate.lock().unwrap();
        for dir in [&self.layout.mods_dir, &self.layout.disabled_dir] {
            let path = dir.join(&name);
            if !path.is_file() {
                continue;
            }
            match self.backup_file(&path, &name) {
                Ok(backup) => {
                    self.rotate(&name);
                    return OpResult::ok("Удалён, копия сохранена.", Some(backup));
                }
                Err(error) => return OpResult::fail(io_message(&error)),
            }
        }
        OpResult::fail("Файл не найден.")
    }

    pub fn unremove(&self, file_name: &str, backup_id: &str, into_disabled: bool) -> OpResult {
        let Some(name) = safe_file_name(Some(file_name)) else {
            return OpResult::fail("Недопустимое имя файла.");
        };
        let _guard = self.gate.lock().unwrap();
        let Some(source) = self.find_backup(&name, backup_id) else {
            return OpResult::fail("Копия удалённого мода пропала.");
        };
        if self.layout.mods_dir.join(&name).exists()
            || self.layout.disabled_dir.join(&name).exists()
        {
            return OpResult::fail("Файл с таким именем уже есть в папке.");
        }
        let dir = if into_disabled {
            &self.layout.disabled_dir
        } else {
            &self.layout.mods_dir
        };
        let target = dir.join(&name);
        if let Err(error) = fs::create_dir_all(dir) {
            return OpResult::fail(io_message(&error));
        }
        match fs::rename(&source, &target) {
            Ok(()) => OpResult::ok("Возвращено.", Some(target)),
            Err(error) => OpResult::fail(io_message(&error)),
        }
    }

    pub fn backups(&self, file_name: &str) -> Vec<PathBuf> {
        let Some(name) = safe_file_name(Some(file_name)) else {
            return Vec::new();
        };
        let dir = self.layout.backup_dir.join(&name);
        let Ok(entries) = fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .map(|file_name| is_backup_of(&file_name.to_string_lossy(), &name))
                    .unwrap_or(false)
            })
            .collect();
        files.sort();
        files
    }

    pub fn backed_up_file_names(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(&self.layout.backup_dir) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .filter(|entry| entry.path().is_dir())
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                safe_file_name(Some(&name)).filter(|safe| !self.backups(safe).is_empty())
            })
            .collect();
        names.sort_by_key(|name| name.to_lowercase());
        names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        names
    }

    pub fn list_backups(&self, file_name: &str) -> Vec<BackupEntry> {
        let Some(name) = safe_file_name(Some(file_name)) else {
            return Vec::new();
        };
        let mut list: Vec<BackupEntry> = self
            .backups(&name)
            .into_iter()
            .filter_map(|path| parse_backup(&path, &name))
            .collect();
        list.sort_by_key(|entry| std::cmp::Reverse(entry.sequence));
        list
    }

    pub fn restore_latest_backup(&self, file_name: &str) -> OpResult {
        let Some(name) = safe_file_name(Some(file_name)) else {
            return OpResult::fail("Недопустимое имя файла.");
        };
        let _guard = self.gate.lock().unwrap();
        let Some(last) = self.backups(&name).last().cloned() else {
            return OpResult::fail("Копий нет.");
        };
        self.restore_from(&name, &last)
    }

    pub fn restore_backup(&self, file_name: &str, backup_id: &str) -> OpResult {
        let Some(name) = safe_file_name(Some(file_name)) else {
            return OpResult::fail("Недопустимое имя файла.");
        };
        let _guard = self.gate.lock().unwrap();
        match self.find_backup(&name, backup_id) {
            Some(path) => self.restore_from(&name, &path),
            None => OpResult::fail("Такой копии нет."),
        }
    }

    pub fn restore_version(
        &self,
        current_file_name: &str,
        backup_file_name: &str,
        backup_id: &str,
    ) -> OpResult {
        let (Some(current), Some(old)) = (
            safe_file_name(Some(current_file_name)),
            safe_file_name(Some(backup_file_name)),
        ) else {
            return OpResult::fail("Недопустимое имя файла.");
        };
        if current.eq_ignore_ascii_case(&old) {
            return self.restore_backup(&old, backup_id);
        }

        let _guard = self.gate.lock().unwrap();
        let Some(source) = self.find_backup(&old, backup_id) else {
            return OpResult::fail("Такой копии нет.");
        };
        let current_enabled = self.layout.mods_dir.join(&current).is_file();
        let current_disabled = self.layout.disabled_dir.join(&current).is_file();
        if current_enabled && current_disabled {
            return OpResult::fail(
                "Файл с таким именем есть и среди включённых, и среди выключенных: уберите лишнюю копию.",
            );
        }
        let dir = if current_disabled {
            &self.layout.disabled_dir
        } else {
            &self.layout.mods_dir
        };
        let target = dir.join(&old);
        if self.layout.mods_dir.join(&old).exists() || self.layout.disabled_dir.join(&old).exists()
        {
            return OpResult::fail("Файл с именем той версии уже есть в папке.");
        }

        let mut moved: Option<PathBuf> = None;
        if current_enabled || current_disabled {
            match self.backup_file(&dir.join(&current), &current) {
                Ok(path) => moved = Some(path),
                Err(error) => return OpResult::fail(io_message(&error)),
            }
        }
        if let Err(error) = fs::create_dir_all(dir) {
            self.restore_moved(&moved, &dir.join(&current));
            return OpResult::fail(io_message(&error));
        }
        if let Err(error) = fs::rename(&source, &target) {
            self.restore_moved(&moved, &dir.join(&current));
            return OpResult::fail(io_message(&error));
        }
        self.rotate(&current);
        self.rotate(&old);
        OpResult::ok("Возвращено.", Some(target))
    }

    pub fn deleted_mods(&self) -> Vec<DeletedMod> {
        let Ok(entries) = fs::read_dir(&self.layout.backup_dir) else {
            return Vec::new();
        };
        let mut result = Vec::new();
        for entry in entries.filter_map(Result::ok) {
            if !entry.path().is_dir() {
                continue;
            }
            let raw = entry.file_name().to_string_lossy().into_owned();
            let Some(name) = safe_file_name(Some(&raw)) else {
                continue;
            };
            if self.layout.mods_dir.join(&name).exists()
                || self.layout.disabled_dir.join(&name).exists()
            {
                continue;
            }
            let list = self.list_backups(&name);
            let Some(latest) = list.first() else {
                continue;
            };
            result.push(DeletedMod {
                file_name: name,
                copies: list.len(),
                last_saved_at: latest.saved_at,
                last_size_bytes: latest.size_bytes,
            });
        }
        result.sort_by(|a, b| {
            b.last_saved_at
                .cmp(&a.last_saved_at)
                .then_with(|| a.file_name.to_lowercase().cmp(&b.file_name.to_lowercase()))
        });
        result
    }

    pub fn cleanup_stale_temp(&self, older_than: std::time::Duration) -> usize {
        let Ok(entries) = fs::read_dir(&self.layout.mods_dir) else {
            return 0;
        };
        let now = SystemTime::now();
        let mut removed = 0;
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with('.') || !name.ends_with(".part") {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            let Ok(modified) = metadata.modified() else {
                continue;
            };
            if now
                .duration_since(modified)
                .map(|age| age >= older_than)
                .unwrap_or(false)
                && fs::remove_file(entry.path()).is_ok()
            {
                removed += 1;
            }
        }
        removed
    }

    fn check_target(&self, name: &str, options: &InstallOptions) -> Option<InstallResult> {
        let in_disabled = self.layout.disabled_dir.join(name).is_file();
        let in_mods = self.layout.mods_dir.join(name).is_file();
        if in_disabled && in_mods {
            return Some(InstallResult::fail(
                InstallError::AlreadyExists,
                "Файл с таким именем есть и среди включённых, и среди выключенных: уберите лишнюю копию.",
            ));
        }
        if (in_disabled || in_mods) && !options.replace_existing {
            return Some(InstallResult::fail(
                InstallError::AlreadyExists,
                "Такой файл уже установлен.",
            ));
        }
        None
    }

    fn target_path(&self, name: &str) -> PathBuf {
        if self.layout.disabled_dir.join(name).exists() {
            self.layout.disabled_dir.join(name)
        } else {
            self.layout.mods_dir.join(name)
        }
    }

    fn backup_file(&self, path: &Path, name: &str) -> io::Result<PathBuf> {
        let dir = self.layout.backup_dir.join(name);
        fs::create_dir_all(&dir)?;
        let sequence = next_sequence(&dir);
        let stamp = (self.now)().format("%Y%m%d-%H%M%S-%3f");
        let destination = dir.join(format!("{sequence:010}_{stamp}_{name}"));
        fs::rename(path, &destination)?;
        Ok(destination)
    }

    fn restore_moved(&self, moved: &Option<PathBuf>, target: &Path) {
        if let Some(moved) = moved
            && !target.exists()
        {
            let _ = fs::rename(moved, target);
        }
    }

    fn restore_from(&self, name: &str, source: &Path) -> OpResult {
        let target = self.target_path(name);
        let mut moved = None;
        if target.exists() {
            match self.backup_file(&target, name) {
                Ok(path) => moved = Some(path),
                Err(error) => return OpResult::fail(io_message(&error)),
            }
        }
        if let Err(error) = fs::create_dir_all(target.parent().unwrap_or(Path::new("."))) {
            self.restore_moved(&moved, &target);
            return OpResult::fail(io_message(&error));
        }
        if let Err(error) = fs::rename(source, &target) {
            self.restore_moved(&moved, &target);
            return OpResult::fail(io_message(&error));
        }
        self.rotate(name);
        OpResult::ok("Возвращено.", Some(target))
    }

    fn find_backup(&self, name: &str, backup_id: &str) -> Option<PathBuf> {
        if backup_id.is_empty() {
            return None;
        }
        self.backups(name).into_iter().find(|path| {
            path.file_name()
                .map(|file| file.to_string_lossy() == backup_id)
                .unwrap_or(false)
        })
    }

    fn rotate(&self, name: &str) {
        let keep = (self.keep_backups)().max(1);
        let all = self.backups(name);
        if all.len() <= keep {
            return;
        }
        for old in all.iter().take(all.len() - keep) {
            let _ = fs::remove_file(old);
        }
    }
}

fn copy_with_hash(
    source: &Path,
    destination: &Path,
    max_bytes: u64,
) -> Result<String, (InstallError, String)> {
    let mut input = File::open(source).map_err(|error| (InstallError::Io, error.to_string()))?;
    let mut output =
        File::create(destination).map_err(|error| (InstallError::Io, error.to_string()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 81920];
    let mut total = 0u64;
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(|error| (InstallError::Io, error.to_string()))?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > max_bytes {
            return Err((
                InstallError::TooLarge,
                "Файл больше допустимого размера, загрузка прервана.".to_string(),
            ));
        }
        output
            .write_all(&buffer[..read])
            .map_err(|error| (InstallError::Io, error.to_string()))?;
        hasher.update(&buffer[..read]);
    }
    output
        .sync_all()
        .map_err(|error| (InstallError::Io, error.to_string()))?;
    Ok(hex_lower(&hasher.finalize()))
}

impl InstallResult {
    fn fail_from_io(error: io::Error) -> Self {
        if is_sharing_violation(&error) {
            Self::fail(
                InstallError::InUse,
                "Файл занят другой программой (возможно, запущена игра). Закройте игру и повторите.",
            )
        } else {
            Self::fail(InstallError::Io, error.to_string())
        }
    }
}

fn io_message(error: &io::Error) -> String {
    if is_sharing_violation(error) {
        "Файл занят другой программой (возможно, запущена игра). Закройте игру и повторите."
            .to_string()
    } else {
        format!("Не получилось изменить файл: {error}")
    }
}

fn is_sharing_violation(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(32) | Some(33))
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{}-{nanos}", std::process::id())
}

fn next_sequence(dir: &Path) -> i64 {
    let mut max = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            let digits = name.chars().take_while(char::is_ascii_digit).count();
            if (1..=15).contains(&digits)
                && let Ok(value) = name[..digits].parse::<i64>()
            {
                max = max.max(value);
            }
        }
    }
    max + 1
}

fn is_backup_of(file_name: &str, name: &str) -> bool {
    let digits = file_name.chars().take_while(char::is_ascii_digit).count();
    digits > 0
        && digits <= 15
        && file_name.len() > digits + 1 + name.len()
        && file_name.as_bytes().get(digits) == Some(&b'_')
        && file_name
            .to_lowercase()
            .ends_with(&format!("_{}", name.to_lowercase()))
}

fn parse_backup(path: &Path, mod_name: &str) -> Option<BackupEntry> {
    let file_name = path.file_name()?.to_string_lossy().into_owned();
    if !is_backup_of(&file_name, mod_name) {
        return None;
    }
    let digits = file_name.chars().take_while(char::is_ascii_digit).count();
    let sequence = file_name[..digits].parse::<i64>().ok()?;
    const STAMP_LEN: usize = 19;
    let mut saved_at = None;
    if file_name.len() >= digits + 1 + STAMP_LEN
        && let Ok(naive) = NaiveDateTime::parse_from_str(
            &file_name[digits + 1..digits + 1 + STAMP_LEN],
            "%Y%m%d-%H%M%S-%3f",
        )
    {
        saved_at = Some(Utc.from_utc_datetime(&naive).fixed_offset());
    }
    let size_bytes = fs::metadata(path).ok()?.len();
    Some(BackupEntry {
        id: file_name,
        file_name: mod_name.to_string(),
        sequence,
        saved_at,
        size_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::ModLayout;
    use std::io::Write;

    fn layout(temp: &tempfile::TempDir) -> ModLayout {
        let root = temp.path();
        let mods = root.join("mods");
        let layout = ModLayout {
            mods_dir: mods.clone(),
            disabled_dir: root.join("disabled"),
            backup_dir: root.join("backups"),
        };
        fs::create_dir_all(&layout.mods_dir).unwrap();
        fs::create_dir_all(&layout.disabled_dir).unwrap();
        fs::create_dir_all(&layout.backup_dir).unwrap();
        layout
    }

    fn installer(temp: &tempfile::TempDir) -> ModInstaller {
        ModInstaller::new(
            layout(temp),
            Arc::new(|| Utc::now().fixed_offset()),
            Arc::new(|| 3),
        )
    }

    fn zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for (name, bytes) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }

    fn source_zip(temp: &tempfile::TempDir) -> PathBuf {
        let path = temp.path().join("source.zip");
        zip(
            &path,
            &[
                ("vehicles/car/info.json", b"{}"),
                ("vehicles/car/main.jbeam", b"x"),
            ],
        );
        path
    }

    #[test]
    fn installs_and_hashes() {
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        let source = source_zip(&temp);
        let result = installer.install_from_path(&source, "car.zip", InstallOptions::default());
        assert!(result.success, "{:?}", result.message);
        assert_eq!(
            result.path,
            Some(installer.layout().mods_dir.join("car.zip"))
        );
        assert_eq!(result.sha256.as_ref().unwrap().len(), 64);
        assert!(installer.layout().mods_dir.join("car.zip").is_file());
        assert!(
            !fs::read_dir(&installer.layout().mods_dir)
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| entry.file_name().to_string_lossy().ends_with(".part"))
        );
    }

    #[test]
    fn blocks_executable_and_bad_names() {
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        let bad = temp.path().join("bad.zip");
        zip(&bad, &[("setup.exe", b"x")]);
        let result = installer.install_from_path(&bad, "bad.zip", InstallOptions::default());
        assert_eq!(result.error, Some(InstallError::Blocked));
        assert!(
            fs::read_dir(&installer.layout().mods_dir)
                .unwrap()
                .next()
                .is_none()
        );
        let result = installer.install_from_path(&bad, "CON.zip", InstallOptions::default());
        assert_eq!(result.error, Some(InstallError::BadName));
    }

    #[test]
    fn refuses_duplicates_and_allows_replace() {
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        let source = source_zip(&temp);
        assert!(
            installer
                .install_from_path(&source, "car.zip", InstallOptions::default())
                .success
        );
        let again = installer.install_from_path(&source, "car.zip", InstallOptions::default());
        assert_eq!(again.error, Some(InstallError::AlreadyExists));
        let replace = installer.install_from_path(
            &source,
            "car.zip",
            InstallOptions {
                replace_existing: true,
                ..InstallOptions::default()
            },
        );
        assert!(replace.success, "{:?}", replace.message);
        assert_eq!(installer.backups("car.zip").len(), 1);
    }

    #[test]
    fn disable_enable_remove_and_unremove() {
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        let source = source_zip(&temp);
        installer.install_from_path(&source, "car.zip", InstallOptions::default());
        assert!(installer.disable("car.zip").success);
        assert!(installer.layout().disabled_dir.join("car.zip").is_file());
        assert!(installer.enable("car.zip").success);

        let removed = installer.remove("car.zip");
        assert!(removed.success);
        let backup_id = removed
            .path
            .as_ref()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert!(!installer.layout().mods_dir.join("car.zip").exists());
        assert!(installer.unremove("car.zip", &backup_id, false).success);
        assert!(installer.layout().mods_dir.join("car.zip").is_file());
    }

    #[test]
    fn backup_rotation_keeps_newest() {
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        let source = source_zip(&temp);
        for _ in 0..5 {
            installer.install_from_path(
                &source,
                "car.zip",
                InstallOptions {
                    replace_existing: true,
                    ..InstallOptions::default()
                },
            );
        }
        assert_eq!(installer.backups("car.zip").len(), 3);
        assert_eq!(installer.list_backups("car.zip").len(), 3);
    }

    #[test]
    fn update_removes_old_and_keeps_disabled_state() {
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        let source = source_zip(&temp);
        installer.install_from_path(&source, "car_v1.zip", InstallOptions::default());
        installer.disable("car_v1.zip");
        let update = installer.update_from_path(
            "car_v1.zip",
            &source,
            "car_v2.zip",
            InstallOptions::default(),
        );
        assert!(update.install.success, "{:?}", update.install.message);
        assert!(update.old_removed);
        assert!(!update.was_enabled);
        assert!(installer.layout().disabled_dir.join("car_v2.zip").is_file());
        assert!(!installer.layout().mods_dir.join("car_v1.zip").exists());
    }

    #[test]
    fn restores_specific_version_and_lists_deleted() {
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        let source = source_zip(&temp);
        installer.install_from_path(&source, "car_v1.zip", InstallOptions::default());
        installer.update_from_path(
            "car_v1.zip",
            &source,
            "car_v2.zip",
            InstallOptions::default(),
        );
        let versions = installer.list_backups("car_v1.zip");
        assert_eq!(versions.len(), 1);
        let restored = installer.restore_version("car_v2.zip", "car_v1.zip", &versions[0].id);
        assert!(restored.success, "{}", restored.message);
        assert!(installer.layout().mods_dir.join("car_v1.zip").is_file());
        assert!(!installer.layout().mods_dir.join("car_v2.zip").exists());

        let deleted = installer.deleted_mods();
        assert!(deleted.iter().any(|item| item.file_name == "car_v2.zip"));
    }
}
