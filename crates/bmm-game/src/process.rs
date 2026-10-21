use sysinfo::{ProcessesToUpdate, System};

pub fn is_game_process_name(name: &str) -> bool {
    let name = name.strip_suffix(".exe").unwrap_or(name);
    name.eq_ignore_ascii_case("BeamNG.drive.x64") || name.eq_ignore_ascii_case("BeamNG.tech.x64")
}

pub fn is_running() -> bool {
    let mut system = System::new_all();
    system.refresh_processes(ProcessesToUpdate::All, true);
    system
        .processes()
        .values()
        .any(|process| is_game_process_name(&process.name().to_string_lossy()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_process_names() {
        assert!(is_game_process_name("BeamNG.drive.x64"));
        assert!(is_game_process_name("BeamNG.drive.x64.exe"));
        assert!(is_game_process_name("beamng.tech.x64"));
        assert!(!is_game_process_name("BeamNG.drive.exe"));
        assert!(!is_game_process_name("BeamNG.drive.x86"));
        assert!(!is_game_process_name("notepad.exe"));
    }
}
