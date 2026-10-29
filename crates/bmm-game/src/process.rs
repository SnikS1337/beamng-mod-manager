use std::path::PathBuf;

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

pub fn is_game_process_name(name: &str) -> bool {
    let name = strip_exe(name);
    [
        "BeamNG.drive.x64",
        "BeamNG.tech.x64",
        "BeamNG.drive.arm64",
        "BeamNG.tech.arm64",
    ]
    .iter()
    .any(|known| name.eq_ignore_ascii_case(known))
}

/// Расширение `.exe` в любом регистре: Windows не различает `EXE` и `exe`.
fn strip_exe(name: &str) -> &str {
    if name.len() >= 4
        && let Some(tail) = name.get(name.len() - 4..)
        && tail.eq_ignore_ascii_case(".exe")
    {
        return &name[..name.len() - 4];
    }
    name
}

/// Только список процессов: `System::new_all` заодно опрашивал диски, память и
/// процессор — заметная задержка на каждом обновлении списка модов.
fn processes() -> System {
    let mut system = System::new();
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    system
}

pub fn is_running() -> bool {
    processes()
        .processes()
        .values()
        .any(|process| is_game_process_name(&process.name().to_string_lossy()))
}

/// Папка запущенной игры (`…\BeamNG.drive`), если игра сейчас запущена:
/// по её `startup.ini` находим папку пользователя, даже если она перенесена.
pub fn running_game_dir() -> Option<PathBuf> {
    // Сначала дешёвый список имён, и только найденным процессам запрашиваем exe:
    // путь к файлу у всех процессов — дорогой запрос на первом обновлении.
    let names = processes();
    let pids: Vec<sysinfo::Pid> = names
        .processes()
        .iter()
        .filter(|(_, process)| is_game_process_name(&process.name().to_string_lossy()))
        .map(|(pid, _)| *pid)
        .collect();
    if pids.is_empty() {
        return None;
    }
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&pids),
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
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
        assert!(is_game_process_name("BeamNG.drive.x64.EXE"));
        assert!(is_game_process_name("BeamNG.drive.x64.Exe"));
        assert!(is_game_process_name("beamng.tech.x64"));
        assert!(is_game_process_name("BeamNG.drive.arm64.exe"));
        assert!(!is_game_process_name("BeamNG.drive.exe"));
        assert!(!is_game_process_name("BeamNG.drive.x86"));
        assert!(!is_game_process_name("notepad.exe"));
        assert!(!is_game_process_name("BeamNG.drive.x64.exe.malware"));
    }
}
