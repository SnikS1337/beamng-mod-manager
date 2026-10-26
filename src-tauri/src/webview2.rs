//! Проверка WebView2 Runtime до создания окна Tauri.
//!
//! Без этой среды окно не может быть создано: Tauri на Windows всегда использует
//! WebView2. Проверяем реестр до запуска и, если среды нет, показываем причину
//! и ссылку на установку вместо молчаливого выхода.

#[cfg(any(windows, test))]
fn has_runtime(version: Option<&str>) -> bool {
    version.is_some_and(|version| {
        let version = version.trim();
        !version.is_empty() && version != "0.0.0.0"
    })
}

#[cfg(not(windows))]
pub fn ensure_available() -> bool {
    true
}

#[cfg(windows)]
pub fn ensure_available() -> bool {
    if has_runtime(installed_version().as_deref()) {
        return true;
    }
    show_missing();
    false
}

#[cfg(windows)]
fn installed_version() -> Option<String> {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    const CLIENT: &str = r"Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    let roots = [
        (
            HKEY_LOCAL_MACHINE,
            format!(r"SOFTWARE\WOW6432Node\{CLIENT}"),
        ),
        (HKEY_LOCAL_MACHINE, format!(r"SOFTWARE\{CLIENT}")),
        (HKEY_CURRENT_USER, format!(r"Software\{CLIENT}")),
    ];
    for (root, path) in roots {
        if let Some(version) = registry_value(root, &path) {
            return Some(version);
        }
    }
    None
}

#[cfg(windows)]
fn registry_value(root: windows_sys::Win32::System::Registry::HKEY, path: &str) -> Option<String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{RRF_RT_REG_SZ, RegGetValueW};

    let path: Vec<u16> = OsStr::new(path).encode_wide().chain(Some(0)).collect();
    let name: Vec<u16> = OsStr::new("pv").encode_wide().chain(Some(0)).collect();
    let mut buffer = [0u16; 128];
    let mut size = (buffer.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            root,
            path.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let length = (size as usize / 2).min(buffer.len());
    let text = String::from_utf16_lossy(&buffer[..length]);
    let text = text.trim_end_matches('\0').trim().to_string();
    (!text.is_empty()).then_some(text)
}

#[cfg(windows)]
fn show_missing() {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::WindowsAndMessaging::{IDYES, MB_ICONERROR, MB_YESNO, MessageBoxW};

    const TEXT: &str = "Не найдена среда Microsoft Edge WebView2.\n\n\
        Без неё программа не может показать окно.\n\n\
        Установите WebView2 Runtime и запустите программу снова.\n\n\
        Открыть страницу загрузки?";
    let text: Vec<u16> = OsStr::new(TEXT).encode_wide().chain(Some(0)).collect();
    let caption: Vec<u16> = OsStr::new("ModManager BeamNG")
        .encode_wide()
        .chain(Some(0))
        .collect();
    let answer = unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_YESNO | MB_ICONERROR,
        )
    };
    if answer == IDYES {
        open_download_page();
    }
}

#[cfg(windows)]
fn open_download_page() {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    const URL: &str = "https://developer.microsoft.com/microsoft-edge/webview2/";
    let operation: Vec<u16> = OsStr::new("open").encode_wide().chain(Some(0)).collect();
    let url: Vec<u16> = OsStr::new(URL).encode_wide().chain(Some(0)).collect();
    unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            url.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_empty_runtime_is_detected() {
        assert!(!has_runtime(None));
        assert!(!has_runtime(Some("")));
        assert!(!has_runtime(Some("0.0.0.0")));
        assert!(has_runtime(Some("120.0.2210.91")));
    }
}
