use std::path::PathBuf;

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

pub fn is_game_process_name(name: &str) -> bool {
    let name = name.strip_suffix(".exe").unwrap_or(name);
    [
        "BeamNG.drive.x64",
        "BeamNG.tech.x64",
        "BeamNG.drive.arm64",
        "BeamNG.tech.arm64",
    ]
    .iter()
    .any(|known| name.eq_ignore_ascii_case(known))
}

/// Только список процессов: `System::new_all` заодно опрашивал диски, память и
/// процессор — заметная задержка на каждом обновлении списка модов.
fn processes(with_exe: bool) -> System {
    let mut system = System::new();
    let kind = if with_exe {
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet)
    } else {
        ProcessRefreshKind::nothing()
    };
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
    system
}

pub fn is_running() -> bool {
    processes(false)
        .processes()
        .values()
        .any(|process| is_game_process_name(&process.name().to_string_lossy()))
}

/// Папка запущенной игры (`…\BeamNG.drive`), если игра сейчас запущена:
/// по её `startup.ini` находим папку пользователя, даже если она перенесена.
pub fn running_game_dir() -> Option<PathBuf> {
    let system = processes(true);
    system
        .processes()
        .values()
        .filter(|process| is_game_process_name(&process.name().to_string_lossy()))
        .filter_map(|process| process.exe())
        .filter_map(|exe| {
            // Bin64\BeamNG.drive.x64.exe — игра двумя уровнями выше.
            let bin = exe.parent()?;
            let dir = if bin
                .file_name()
                .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("Bin64"))
            {
                bin.parent()?
            } else {
                bin
            };
            Some(dir.to_path_buf())
        })
        .next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_process_names() {
        assert!(is_game_process_name("BeamNG.drive.x64"));
        assert!(is_game_process_name("BeamNG.drive.x64.exe"));
        assert!(is_game_process_name("beamng.tech.x64"));
        assert!(is_game_process_name("BeamNG.drive.arm64.exe"));
        assert!(!is_game_process_name("BeamNG.drive.exe"));
        assert!(!is_game_process_name("BeamNG.drive.x86"));
        assert!(!is_game_process_name("notepad.exe"));
    }
}
