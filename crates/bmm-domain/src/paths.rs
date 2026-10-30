use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum PathsError {
    #[error("не задан каталог LOCALAPPDATA")]
    NoLocalAppData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    root: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> Result<Self, PathsError> {
        let base = std::env::var_os("LOCALAPPDATA").ok_or(PathsError::NoLocalAppData)?;
        Ok(Self::at(PathBuf::from(base).join("BeamModManager")))
    }

    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn settings(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    pub fn library(&self) -> PathBuf {
        self.root.join("library.json")
    }

    pub fn matches(&self) -> PathBuf {
        self.root.join("matches.json")
    }

    pub fn previews(&self) -> PathBuf {
        self.root.join("previews")
    }

    pub fn search_previews(&self) -> PathBuf {
        self.root.join("search-previews")
    }

    pub fn update_dir(&self) -> PathBuf {
        self.root.join("update")
    }

    /// Пустой `WebView2\` прежние версии создавали зря: профиль браузера WebView2
    /// хранит в своей папке (`%LOCALAPPDATA%\com.sniks1337.beammodmanager`).
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        for path in [
            self.root(),
            self.previews().as_path(),
            self.search_previews().as_path(),
            self.update_dir().as_path(),
        ] {
            std::fs::create_dir_all(path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_live_in_expected_places() {
        let paths = AppPaths::at(r"C:\Users\Test\AppData\Local\BeamModManager");
        assert_eq!(paths.settings(), paths.root().join("settings.json"));
        assert_eq!(paths.library(), paths.root().join("library.json"));
        assert_eq!(paths.matches(), paths.root().join("matches.json"));
        assert_eq!(paths.previews(), paths.root().join("previews"));
        assert_eq!(
            paths.search_previews(),
            paths.root().join("search-previews")
        );
        assert_eq!(paths.update_dir(), paths.root().join("update"));
    }

    #[test]
    fn ensure_dirs_creates_all_directories() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(temp.path());
        paths.ensure_dirs().unwrap();
        for path in [
            paths.root(),
            paths.previews().as_path(),
            paths.search_previews().as_path(),
            paths.update_dir().as_path(),
        ] {
            assert!(path.is_dir(), "нет каталога {path:?}");
        }
        assert!(
            !paths.root().join("WebView2").exists(),
            "пустую папку не создаём"
        );
    }
}
