//! Where the launcher keeps its files, and its settings (one JSON file in the app data folder).
//!
//! Layout under the app data dir (e.g. `~/Library/Application Support/io.github.peterpeet.koboldlauncher`):
//! `settings.json`, `bin/<tag>/<koboldcpp asset>`, `models/`, `presets/*.kcpps`, `runs/last-run.kcpps`,
//! `cache/`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Dirs {
    pub root: PathBuf,
}

impl Dirs {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
    pub fn settings_file(&self) -> PathBuf { self.root.join("settings.json") }
    pub fn bin(&self) -> PathBuf { self.root.join("bin") }
    pub fn default_models(&self) -> PathBuf { self.root.join("models") }
    pub fn presets(&self) -> PathBuf { self.root.join("presets") }
    pub fn runs(&self) -> PathBuf { self.root.join("runs") }
    pub fn cache(&self) -> PathBuf { self.root.join("cache") }

    pub fn ensure(&self) -> std::io::Result<()> {
        for d in [self.root.clone(), self.bin(), self.presets(), self.runs(), self.cache()] {
            std::fs::create_dir_all(d)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InstalledBinary {
    pub path: PathBuf,
    /// Release tag, e.g. "v1.124.2"; `None` for a file the user picked whose version we don't know.
    pub version: Option<String>,
    pub asset: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub binary: Option<InstalledBinary>,
    pub check_updates_on_start: bool,
    pub include_prereleases: bool,
    /// Where downloads go; `None` = app data `models/`.
    pub models_dir: Option<PathBuf>,
    /// Extra folders the library scans for .gguf files.
    pub library_folders: Vec<PathBuf>,
    /// Hugging Face sources, highest priority first.
    pub sources: Vec<crate::hf::Source>,
    /// Model path → last start (unix seconds).
    pub last_used: std::collections::HashMap<String, u64>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            binary: None,
            check_updates_on_start: true,
            include_prereleases: false,
            models_dir: None,
            library_folders: vec![],
            sources: crate::hf::default_sources(),
            last_used: Default::default(),
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        let mut s: Self = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        s.sources = crate::hf::migrate_sources(std::mem::take(&mut s.sources));
        s
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        write_atomic(path, serde_json::to_string_pretty(self).expect("settings serialize").as_bytes())
    }

    pub fn models_dir(&self, dirs: &Dirs) -> PathBuf {
        self.models_dir.clone().unwrap_or_else(|| dirs.default_models())
    }
}

/// Write via a temp file and rename, so readers never see half a file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp-write");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_tolerate_missing_fields() {
        let dir = tempfile::tempdir().unwrap();
        let dirs = Dirs::new(dir.path().to_path_buf());
        dirs.ensure().unwrap();
        assert_eq!(Settings::load(&dirs.settings_file()), Settings::default());
        std::fs::write(dirs.settings_file(), r#"{"includePrereleases":true,"futureField":1}"#).unwrap();
        let s = Settings::load(&dirs.settings_file());
        assert!(s.include_prereleases && s.check_updates_on_start);
        s.save(&dirs.settings_file()).unwrap();
        assert_eq!(Settings::load(&dirs.settings_file()), s);
        assert_eq!(s.models_dir(&dirs), dir.path().join("models"));
    }
}
