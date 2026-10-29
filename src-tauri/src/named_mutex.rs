//! Общий именованный мьютекс со старой версией `Local\BeamModManager.Single`
//! и файловая блокировка каталога данных для разных сессий Windows.

use std::fs::{File, TryLockError};
use std::io;
use std::path::Path;

pub enum DataLock {
    Acquired(File),
    Busy,
    Unavailable(io::Error),
}

/// Защита от второго экземпляра в другой сессии Windows (RDP, переключение
/// пользователя): `Local\`-мьютекс живёт только в своей сессии, а файл — на диске.
pub fn acquire_data_lock(path: &Path) -> DataLock {
    let file = match File::options()
        .create(true)
        .write(true)
        .truncate(false)
        .open(path)
    {
        Ok(file) => file,
        Err(error) => return DataLock::Unavailable(error),
    };
    match file.try_lock() {
        Ok(()) => DataLock::Acquired(file),
        Err(TryLockError::WouldBlock) => DataLock::Busy,
        Err(TryLockError::Error(error)) => DataLock::Unavailable(error),
    }
}

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
    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
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
        use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONINFORMATION, MB_OK, MessageBoxW};

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

    #[test]
    fn data_folder_lock_is_exclusive() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("instance.lock");
        let first = acquire_data_lock(&path);
        assert!(matches!(first, DataLock::Acquired(_)));
        assert!(matches!(acquire_data_lock(&path), DataLock::Busy));
        drop(first);
        assert!(matches!(acquire_data_lock(&path), DataLock::Acquired(_)));
    }
}
