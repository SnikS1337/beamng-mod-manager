const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "CLOCK$", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
    "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

const INVALID: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

pub fn safe_file_name(raw: Option<&str>) -> Option<String> {
    let raw = raw?;
    if raw.trim().is_empty() {
        return None;
    }
    let normalized = raw.replace('\\', "/");
    let name = normalized
        .rsplit_once('/')
        .map(|(_, name)| name)
        .unwrap_or(&normalized)
        .trim()
        .to_string();

    let length = name.chars().count();
    if length == 0
        || length > 200
        || name.starts_with('.')
        || name.chars().any(|c| INVALID.contains(&c) || c == '\0')
        || name
            .chars()
            .any(|c| (c as u32) < 32 || matches!(c, '\u{202e}' | '\u{200e}' | '\u{200f}'))
    {
        return None;
    }

    let lower = name.to_lowercase();
    if !lower.ends_with(".zip") {
        return None;
    }
    let stem = &name[..name.len() - 4];
    if stem.is_empty() || stem.ends_with(' ') || stem.ends_with('.') {
        return None;
    }
    let device = stem
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_uppercase();
    if RESERVED.contains(&device.as_str()) {
        return None;
    }

    Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_names() {
        for name in [
            "my_car-1.2.zip",
            "Автомобиль.zip",
            "console.zip",
            "com10.zip",
            "a b.zip",
            "MOD.ZIP",
        ] {
            assert_eq!(safe_file_name(Some(name)).as_deref(), Some(name));
        }
    }

    #[test]
    fn rejects_dangerous_and_reserved_names() {
        for name in [
            "",
            "   ",
            "mod.exe",
            ".hidden.zip",
            "a/",
            "bad:name.zip",
            "CON.zip",
            "nul.zip",
            "Com1.zip",
            "LPT9.zip",
            "aux.v2.zip",
            "x .zip",
            "x..zip",
            ".zip",
            "readme.txt",
            "a.zip.exe",
            "a\u{1}b.zip",
            "a\u{202e}b.zip",
        ] {
            assert!(safe_file_name(Some(name)).is_none(), "{name:?}");
        }
        let long = format!("{}.zip", "a".repeat(200));
        assert!(safe_file_name(Some(&long)).is_none());
    }

    #[test]
    fn strips_path_but_keeps_name() {
        assert_eq!(
            safe_file_name(Some("../../up/evil.zip")).as_deref(),
            Some("evil.zip")
        );
        assert_eq!(
            safe_file_name(Some("C:\\mods\\car.zip")).as_deref(),
            Some("car.zip")
        );
    }
}
