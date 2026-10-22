//! Имена рядом с программой: `BeamModManager.exe.new` и т. п. без потери
//! Unicode и нестандартных символов пути.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Добавить суффикс к полному пути, не трогая имя файла: `app.exe` + `.new`.
pub(crate) fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name: OsString = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}
