use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub version: u32,
    pub menu_bar_enabled: bool,
    pub widget_enabled: bool,
    pub widget_mode: usize,
    pub close_to_menu_bar: bool,
    /// 0 follows the system language, 1 English, 2 Thai.
    pub language: usize,
    /// 0 follows the system, 1 light (the SSD Scope look), 2 dark.
    pub appearance: usize,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            menu_bar_enabled: true,
            widget_enabled: true,
            widget_mode: 0,
            close_to_menu_bar: true,
            language: 0,
            appearance: 0,
        }
    }
}
impl Settings {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let mut file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)?;
        ensure!(
            file.metadata()?.is_file() && file.metadata()?.len() <= 16 * 1024,
            "Invalid settings file"
        );
        use std::io::Read;
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(16 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 16 * 1024, "Settings file too large");
        let value: Self = serde_json::from_slice(&bytes)?;
        ensure!(
            value.version == 1
                && value.widget_mode <= 2
                && value.language <= 2
                && value.appearance <= 2,
            "Unsupported settings"
        );
        Ok(value)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        ensure!(
            self.version == 1
                && self.widget_mode <= 2
                && self.language <= 2
                && self.appearance <= 2,
            "Invalid settings"
        );
        let parent = path.parent().context("Missing settings directory")?;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(&serde_json::to_vec_pretty(self)?)?;
        file.as_file().sync_all()?;
        file.persist(path)?;
        Ok(())
    }
}
pub fn default_path() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Library/Application Support/SSD Scope/settings.json")
}
