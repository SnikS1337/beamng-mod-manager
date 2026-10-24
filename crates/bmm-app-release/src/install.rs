//! Файловая часть обновления: распаковка exe из архива, подготовка рядом с
//! программой, подмена и откат.
//!
//! Работающий exe нельзя перезаписать на Windows, поэтому есть два пути:
//! старый — [`stage`] + [`commit`] (переименования `*.old` / `*.new`), и
//! текущий — [`write_in_place`] из новой версии, когда прежняя уже вышла:
//! файл остаётся тем же, прежняя версия лежит в копии `previous.exe`.
//! Оба пути проверяются по отдельности, процессы в тестах не запускаются.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

use serde::Serialize;

use crate::manifest::{MAX_SIZE, MIN_SIZE, UpdateManifest};
use crate::paths::with_suffix;
use crate::version::AppVersion;

/// Имя файла программы в архиве обновления.
pub const EXE_NAME: &str = "BeamModManager.exe";
/// Прежняя версия, отодвинутая при подмене.
pub const OLD_SUFFIX: &str = ".old";
/// Новая версия, подготовленная рядом с программой.
pub const NEW_SUFFIX: &str = ".new";
/// Новая версия, которая не запустилась и уступила место прежней.
pub const BAD_SUFFIX: &str = ".bad";
/// Понятный совет, когда в папку программы нельзя писать.
pub const NO_RIGHTS: &str =
    "нет прав менять файлы в папке программы. Перенесите её, например, на рабочий стол";

const RETRY_ATTEMPTS: usize = 4;
const COPY_ATTEMPTS: usize = 10;

/// Ошибка распаковки, подготовки или подмены.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InstallError {
    #[error("в архиве обновления нет программы")]
    ProgramMissing,
    #[error("программа в архиве неправдоподобного размера")]
    ImplausibleSize,
    #[error("в архиве не программа Windows")]
    NotWindowsExe,
    #[error("программа в архиве зашифрована")]
    ArchiveEncrypted,
    #[error("версия в архиве ({found}) не та, что обещана ({expected})")]
    VersionMismatch { found: String, expected: String },
    #[error("архив обновления не распаковался: {0}")]
    Archive(String),
    #[error("архив обновления не найден")]
    ArchiveMissing,
    #[error("подготовленное обновление пропало")]
    PreparedMissing,
    #[error("программа не найдена на прежнем месте")]
    CurrentMissing,
    #[error("{}", NO_RIGHTS)]
    NoRights,
    #[error("не удалось положить новую версию рядом с программой: {0}")]
    StageFailed(String),
    #[error("новую версию держит другая программа (антивирус?): {0}")]
    StagedBusy(String),
    #[error("не удалось убрать старую копию: {0}")]
    OldCleanupFailed(String),
    #[error("не удалось отодвинуть текущую версию: {0}")]
    MoveCurrentFailed(String),
    #[error(
        "обновление не установилось, и прежняя версия не вернулась: переименуйте {old} обратно в {current}"
    )]
    RollbackFailed { old: String, current: String },
    #[error("новая версия не установилась: {0}")]
    CommitFailed(String),
    #[error("не удалось сохранить прежнюю версию: {0}")]
    BackupFailed(String),
    #[error("новая версия не записалась: {0}")]
    OverwriteFailed(String),
    #[error(
        "новая версия не записалась, и прежняя не вернулась: скопируйте {backup} на место {target}"
    )]
    RestoreFailed { backup: String, target: String },
    #[error("обновление запрещено: программа запущена из временной папки")]
    TemporaryLocation,
    #[error("версия в описании ({candidate}) не новее установленной ({current})")]
    NotNewer { candidate: String, current: String },
}

/// Достать exe из скачанного архива в папку подготовки. В архиве нужен ровно
/// один `BeamModManager.exe` в корне; пути с папками и `..` не принимаются.
/// `version_of` читает версию exe (в Tauri — из ресурсов файла Windows).
pub fn extract_exe(
    zip_path: &Path,
    staging_dir: &Path,
    expected: AppVersion,
    version_of: &dyn Fn(&Path) -> Option<AppVersion>,
) -> Result<PathBuf, InstallError> {
    fs::create_dir_all(staging_dir).map_err(archive_error)?;
    let destination = staging_dir.join(EXE_NAME);

    let file = File::open(zip_path).map_err(archive_error)?;
    let mut archive = zip::ZipArchive::new(file).map_err(archive_error)?;

    let mut matched = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(archive_error)?;
        let name = entry.name().map_err(archive_error)?;
        if name.eq_ignore_ascii_case(EXE_NAME) {
            matched.push((index, entry.size(), entry.is_symlink(), entry.encrypted()));
        }
    }
    if matched.len() != 1 {
        return Err(InstallError::ProgramMissing);
    }
    let (index, declared_size, symlink, encrypted) = matched[0];
    if symlink {
        return Err(InstallError::NotWindowsExe);
    }
    if encrypted {
        return Err(InstallError::ArchiveEncrypted);
    }
    if !(MIN_SIZE..=MAX_SIZE).contains(&declared_size) {
        return Err(InstallError::ImplausibleSize);
    }

    let mut entry = archive.by_index(index).map_err(archive_error)?;
    let mut output = File::create(&destination).map_err(archive_error)?;
    let written = copy_limited(&mut entry, &mut output);
    drop(output);
    match written {
        Ok(written) if written == declared_size => {}
        Ok(_) => {
            let _ = fs::remove_file(&destination);
            return Err(InstallError::Archive(
                "размер программы в архиве не совпал с заявленным".to_string(),
            ));
        }
        Err(error) => {
            let _ = fs::remove_file(&destination);
            return Err(error);
        }
    }

    if !looks_like_exe(&destination) {
        let _ = fs::remove_file(&destination);
        return Err(InstallError::NotWindowsExe);
    }
    let actual = version_of(&destination);
    if actual != Some(expected) {
        let _ = fs::remove_file(&destination);
        return Err(InstallError::VersionMismatch {
            found: actual
                .map(AppVersion::display)
                .unwrap_or_else(|| "?".to_string()),
            expected: expected.display(),
        });
    }
    Ok(destination)
}

fn copy_limited(input: &mut dyn Read, output: &mut File) -> Result<u64, InstallError> {
    let mut written = 0u64;
    let mut buffer = vec![0u8; 80 * 1024];
    loop {
        let read = input.read(&mut buffer).map_err(archive_error)?;
        if read == 0 {
            return Ok(written);
        }
        written = written.saturating_add(read as u64);
        if written > MAX_SIZE {
            return Err(InstallError::ImplausibleSize);
        }
        output.write_all(&buffer[..read]).map_err(archive_error)?;
    }
}

fn archive_error(error: impl std::fmt::Display) -> InstallError {
    InstallError::Archive(error.to_string())
}

/// Файл начинается с «MZ» — заголовок программы Windows.
pub fn looks_like_exe(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut header = [0u8; 2];
    file.read_exact(&mut header).is_ok() && &header == b"MZ"
}

/// Подготовка: новый exe копируется рядом с текущим (`*.new`). Текущий exe не
/// трогается. Копирование заранее важно, когда папка обновления на другом диске.
pub fn stage(current_exe: &Path, new_exe: &Path) -> Result<(), InstallError> {
    if !new_exe.is_file() {
        return Err(InstallError::PreparedMissing);
    }
    if !current_exe.is_file() {
        return Err(InstallError::CurrentMissing);
    }
    let staged = with_suffix(current_exe, NEW_SUFFIX);

    let copied = retry(
        || {
            if staged.exists() {
                fs::remove_file(&staged)?;
            }
            fs::copy(new_exe, &staged).map(|_| ())
        },
        RETRY_ATTEMPTS,
    );
    if let Err(error) = copied {
        let _ = fs::remove_file(&staged);
        return Err(if is_no_rights(&error) {
            InstallError::NoRights
        } else {
            InstallError::StageFailed(error.to_string())
        });
    }

    // Антивирус проверяет свежий файл и держит его открытым: ждём, пока отпустит,
    // до того как отодвигать текущий.
    if let Err(error) = retry(|| exclusive_probe(&staged), COPY_ATTEMPTS) {
        let _ = fs::remove_file(&staged);
        return Err(InstallError::StagedBusy(error.to_string()));
    }
    Ok(())
}

fn exclusive_probe(path: &Path) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    let _file = options.open(path)?;
    Ok(())
}

/// Подмена: два быстрых переименования на одном диске — текущий → `*.old`,
/// `*.new` → на его место; если второе не удалось, текущий возвращается.
pub fn swap(current_exe: &Path, new_exe: &Path) -> Result<(), InstallError> {
    stage(current_exe, new_exe)?;
    commit(current_exe)
}

/// [`swap`] с подстановкой переименования (для проверки сбоев).
pub fn swap_with(
    current_exe: &Path,
    new_exe: &Path,
    move_file: &mut dyn FnMut(&Path, &Path) -> io::Result<()>,
) -> Result<(), InstallError> {
    stage(current_exe, new_exe)?;
    commit_with(current_exe, move_file)
}

/// Подмена уже подготовленного `*.new`.
pub fn commit(current_exe: &Path) -> Result<(), InstallError> {
    let mut move_file = |from: &Path, to: &Path| fs::rename(from, to);
    commit_with(current_exe, &mut move_file)
}

/// [`commit`] с подстановкой переименования (для проверки отката).
pub fn commit_with(
    current_exe: &Path,
    move_file: &mut dyn FnMut(&Path, &Path) -> io::Result<()>,
) -> Result<(), InstallError> {
    let old = with_suffix(current_exe, OLD_SUFFIX);
    let staged = with_suffix(current_exe, NEW_SUFFIX);
    if !staged.is_file() {
        return Err(InstallError::PreparedMissing);
    }

    if old.exists()
        && let Err(error) = fs::remove_file(&old)
    {
        let _ = fs::remove_file(&staged);
        return Err(InstallError::OldCleanupFailed(error.to_string()));
    }

    if let Err(error) = move_file(current_exe, &old) {
        let _ = fs::remove_file(&staged);
        return Err(if is_no_rights(&error) {
            InstallError::NoRights
        } else {
            InstallError::MoveCurrentFailed(error.to_string())
        });
    }

    match retry(|| move_file(&staged, current_exe), RETRY_ATTEMPTS) {
        Ok(()) => Ok(()),
        Err(failed) => {
            let current_missing = !current_exe.exists();
            if current_missing && move_file(&old, current_exe).is_err() {
                return Err(InstallError::RollbackFailed {
                    old: file_name(&old),
                    current: file_name(current_exe),
                });
            }
            let _ = fs::remove_file(&staged);
            Err(InstallError::CommitFailed(failed.to_string()))
        }
    }
}

/// Вернуть прежнюю версию из `*.old` (новая не запустилась и уже закрылась).
pub fn rollback(current_exe: &Path) -> bool {
    let old = with_suffix(current_exe, OLD_SUFFIX);
    if !old.is_file() {
        return false;
    }
    retry(
        || {
            if current_exe.exists() {
                fs::remove_file(current_exe)?;
            }
            fs::rename(&old, current_exe)
        },
        6,
    )
    .is_ok()
}

/// Вернуть прежнюю версию из самой новой, которая не смогла запуститься.
/// Работающий exe удалить нельзя, но можно переименовать: он уходит в `*.bad`.
pub fn rollback_from_running(current_exe: &Path) -> bool {
    let old = with_suffix(current_exe, OLD_SUFFIX);
    let bad = with_suffix(current_exe, BAD_SUFFIX);
    if !old.is_file() || !current_exe.is_file() {
        return false;
    }

    let moved = (|| {
        if bad.exists() {
            fs::remove_file(&bad)?;
        }
        fs::rename(current_exe, &bad)
    })();
    if moved.is_err() {
        return false;
    }

    match fs::rename(&old, current_exe) {
        Ok(()) => true,
        Err(_) => {
            let _ = fs::rename(&bad, current_exe);
            false
        }
    }
}

/// Убрать `*.old`, `*.new`, `*.bad` и папку подготовки. Зовётся, когда версия
/// уже показала окно: до этого `*.old` нужен для отката.
pub fn cleanup(current_exe: &Path, update_dir: &Path) {
    for suffix in [OLD_SUFFIX, NEW_SUFFIX, BAD_SUFFIX] {
        let leftover = with_suffix(current_exe, suffix);
        let mut attempts = 0;
        while leftover.exists() && attempts < 10 {
            match fs::remove_file(&leftover) {
                Ok(()) => break,
                Err(_) => sleep(Duration::from_millis(300)),
            }
            attempts += 1;
        }
    }

    let mut attempts = 0;
    while update_dir.is_dir() && attempts < 10 {
        match fs::remove_dir_all(update_dir) {
            Ok(()) => break,
            Err(_) => sleep(Duration::from_millis(300)),
        }
        attempts += 1;
    }
}

/// Версия, которая не запустилась на этом компьютере (её больше не предлагаем).
pub fn read_skipped(path: &Path) -> Option<AppVersion> {
    let text = fs::read_to_string(path).ok()?;
    AppVersion::parse(&text)
}

/// Запомнить версию, которая не запустилась. Ошибки записи не критичны для
/// обновления, но возвращаются вызывающему.
pub fn write_skipped(path: &Path, version: AppVersion) -> io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, version.display())
}

/// Можно ли будет записать новую версию в файл программы. Работающий exe
/// открыть на запись нельзя («занят») — это нормально: запись будет, когда
/// программа выйдет. Нет прав — видно сразу.
pub fn check_writable(current_exe: &Path) -> Result<(), InstallError> {
    if !current_exe.is_file() {
        return Err(InstallError::CurrentMissing);
    }
    match OpenOptions::new().write(true).open(current_exe) {
        Ok(_) => Ok(()),
        Err(error) if is_no_rights(&error) => Err(InstallError::NoRights),
        // Файл занят самой программой или антивирусом: права есть.
        Err(_) => Ok(()),
    }
}

/// Новая версия — в тот же файл программы. Прежняя сперва копируется в
/// `backup`; запись оборвалась — она возвращается оттуда. Файл остаётся тем же,
/// поэтому значок на рабочем столе не переезжает.
pub fn write_in_place(new_exe: &Path, target: &Path, backup: &Path) -> Result<(), InstallError> {
    let mut overwrite = |from: &Path, to: &Path| overwrite_file(from, to);
    write_in_place_with(new_exe, target, backup, &mut overwrite)
}

/// [`write_in_place`] с подстановкой записи (для проверки сбоя на середине).
pub fn write_in_place_with(
    new_exe: &Path,
    target: &Path,
    backup: &Path,
    overwrite: &mut dyn FnMut(&Path, &Path) -> io::Result<()>,
) -> Result<(), InstallError> {
    if !new_exe.is_file() {
        return Err(InstallError::PreparedMissing);
    }
    if !target.is_file() {
        return Err(InstallError::CurrentMissing);
    }

    let saved = retry(
        || {
            if let Some(parent) = backup.parent()
                && !parent.as_os_str().is_empty()
            {
                fs::create_dir_all(parent)?;
            }
            fs::copy(target, backup).map(|_| ())
        },
        RETRY_ATTEMPTS,
    );
    if let Err(error) = saved {
        return Err(InstallError::BackupFailed(error.to_string()));
    }

    let failed = retry(|| overwrite(new_exe, target), COPY_ATTEMPTS);
    if failed.is_ok() {
        forget_download_mark(target);
        return Ok(());
    }
    let failed = failed.expect_err("проверено выше");

    let why = if is_no_rights(&failed) {
        InstallError::NoRights
    } else {
        InstallError::OverwriteFailed(failed.to_string())
    };
    if same_content(backup, target) {
        // Файл не открылся на запись — он не тронут.
        return Err(why);
    }
    // Откат — только настоящей записью, без подставленной сбойной функции:
    // иначе проверка сбоя не вернула бы прежнюю версию.
    if retry(|| overwrite_file(backup, target), COPY_ATTEMPTS).is_err() {
        return Err(InstallError::RestoreFailed {
            backup: backup.display().to_string(),
            target: file_name(target),
        });
    }
    Err(why)
}

/// Вернуть прежнюю версию в файл программы из копии.
pub fn restore_in_place(backup: &Path, target: &Path) -> bool {
    if !backup.is_file() {
        return false;
    }
    if retry(|| overwrite_file(backup, target), COPY_ATTEMPTS).is_err() {
        return false;
    }
    forget_download_mark(target);
    true
}

/// Содержимое `src` — в существующий файл `dst`, тот же файл, без переименований.
pub fn overwrite_file(src: &Path, dst: &Path) -> io::Result<()> {
    let mut from = File::open(src)?;
    let length = from.metadata()?.len();
    let mut to = OpenOptions::new().write(true).open(dst)?;
    io::copy(&mut from, &mut to)?;
    to.set_len(length)?;
    to.flush()?;
    to.sync_all()
}

fn same_content(left: &Path, right: &Path) -> bool {
    let (Ok(left_meta), Ok(right_meta)) = (fs::metadata(left), fs::metadata(right)) else {
        return false;
    };
    if left_meta.len() != right_meta.len() {
        return false;
    }
    let (Ok(mut left_file), Ok(mut right_file)) = (File::open(left), File::open(right)) else {
        return false;
    };
    let mut left_buffer = [0u8; 8192];
    let mut right_buffer = [0u8; 8192];
    loop {
        let Ok(read) = left_file.read(&mut left_buffer) else {
            return false;
        };
        if read == 0 {
            return true;
        }
        if right_file.read_exact(&mut right_buffer[..read]).is_err() {
            return false;
        }
        if left_buffer[..read] != right_buffer[..read] {
            return false;
        }
    }
}

/// Пометка «скачано из интернета» (поток `Zone.Identifier`) остаётся у файла от
/// первой распаковки. Новая версия в том же файле иначе спрашивала бы SmartScreen.
#[cfg(windows)]
fn forget_download_mark(target: &Path) {
    use std::ffi::OsString;
    let mut name: OsString = target.as_os_str().to_os_string();
    name.push(":Zone.Identifier");
    let _ = fs::remove_file(PathBuf::from(name));
}

#[cfg(not(windows))]
fn forget_download_mark(_target: &Path) {}

/// Программа запущена из временной папки? Такую сборку не обновляем: подмена
/// файла в чужой временной папке ничего не даёт и может испортить чужой запуск.
pub fn is_temporary_location(path: &Path) -> bool {
    let Some(path) = absolute_existing(path) else {
        return false;
    };
    let temp = fs::canonicalize(std::env::temp_dir()).unwrap_or_else(|_| std::env::temp_dir());
    let temp = normalize_path(&temp);
    let path = normalize_path(&path);
    let temp = temp.trim_end_matches('/');
    path == temp || path.starts_with(&format!("{temp}/"))
}

fn absolute_existing(path: &Path) -> Option<PathBuf> {
    if let Ok(canonical) = fs::canonicalize(path) {
        return Some(canonical);
    }
    std::path::absolute(path).ok()
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

/// Правила подготовки: по умолчанию обновлять сборку из временной папки
/// запрещено и права на файл программы проверяются сразу.
#[derive(Debug, Clone)]
pub struct PrepareOptions {
    pub allow_temporary_location: bool,
    pub require_writable: bool,
}

impl Default for PrepareOptions {
    fn default() -> Self {
        Self {
            allow_temporary_location: false,
            require_writable: true,
        }
    }
}

/// Проверенное обновление в папке подготовки. Текущий exe ещё не изменён.
/// [`PreparedUpdate::apply`] зовётся после выхода прежнего процесса.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedUpdate {
    pub version: AppVersion,
    pub archive: PathBuf,
    pub new_exe: PathBuf,
    pub current_exe: PathBuf,
    pub backup: PathBuf,
}

impl PreparedUpdate {
    /// Записать новую версию в тот же файл программы с копией прежней.
    pub fn apply(&self) -> Result<(), InstallError> {
        write_in_place(&self.new_exe, &self.current_exe, &self.backup)
    }

    /// [`PreparedUpdate::apply`] с подстановкой записи (для проверок).
    pub fn apply_with(
        &self,
        overwrite: &mut dyn FnMut(&Path, &Path) -> io::Result<()>,
    ) -> Result<(), InstallError> {
        write_in_place_with(&self.new_exe, &self.current_exe, &self.backup, overwrite)
    }

    /// Вернуть прежнюю версию из копии, если новая не запустилась.
    pub fn restore(&self) -> bool {
        restore_in_place(&self.backup, &self.current_exe)
    }
}

/// Распаковать архив, сверить версию и права, вернуть готовое к перезапуску
/// обновление. Рабочий exe не перезаписывается.
#[allow(clippy::too_many_arguments)]
pub fn prepare_update(
    archive: &Path,
    update_dir: &Path,
    manifest: &UpdateManifest,
    local_version: &AppVersion,
    current_exe: &Path,
    version_of: &dyn Fn(&Path) -> Option<AppVersion>,
    options: &PrepareOptions,
) -> Result<PreparedUpdate, InstallError> {
    if !options.allow_temporary_location && is_temporary_location(current_exe) {
        return Err(InstallError::TemporaryLocation);
    }
    if !manifest.version.is_newer_than(local_version) {
        return Err(InstallError::NotNewer {
            candidate: manifest.version.display(),
            current: local_version.display(),
        });
    }
    if !current_exe.is_file() {
        return Err(InstallError::CurrentMissing);
    }
    if !archive.is_file() {
        return Err(InstallError::ArchiveMissing);
    }

    let staging = update_dir.join(manifest.version.display()).join("exe");
    let new_exe = extract_exe(archive, &staging, manifest.version, version_of)?;
    if options.require_writable {
        check_writable(current_exe)?;
    }

    Ok(PreparedUpdate {
        version: manifest.version,
        archive: archive.to_path_buf(),
        new_exe,
        current_exe: current_exe.to_path_buf(),
        backup: update_dir.join("previous.exe"),
    })
}

fn retry<T>(mut action: impl FnMut() -> io::Result<T>, attempts: usize) -> io::Result<T> {
    let mut index = 0usize;
    loop {
        match action() {
            Ok(value) => return Ok(value),
            Err(error) => {
                if index + 1 >= attempts {
                    return Err(error);
                }
                sleep(Duration::from_millis((150 * (index + 1)).min(1000) as u64));
                index += 1;
            }
        }
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn is_no_rights(error: &io::Error) -> bool {
    #[cfg(windows)]
    if let Some(code) = error.raw_os_error() {
        // ERROR_ACCESS_DENIED. ERROR_SHARING_VIOLATION (32) — файл занят, права есть.
        return code == 5;
    }
    error.kind() == io::ErrorKind::PermissionDenied
}
