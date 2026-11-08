//! Where the launcher keeps its files, and its settings (one JSON file in the app data folder).
//!
//! Layout under the app data dir (e.g. `~/Library/Application Support/io.github.peterpeet.koboldlauncher`):
//! `settings.json`, `bin/<tag>/<koboldcpp asset>`, `models/`, `presets/*.kcpps`, `runs/last-run.kcpps`,
//! `logs/` (`koboldcpp.log`, `launcher.log`), `cache/`.

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
    /// Esobold lives in its own folder, so both programs can stay installed.
    pub fn bin_esobold(&self) -> PathBuf { self.root.join("bin-esobold") }
    pub fn bin_of(&self, flavor: crate::binary::Flavor) -> PathBuf {
        match flavor {
            crate::binary::Flavor::KoboldCpp => self.bin(),
            crate::binary::Flavor::Esobold => self.bin_esobold(),
        }
    }
    pub fn default_models(&self) -> PathBuf { self.root.join("models") }
    pub fn presets(&self) -> PathBuf { self.root.join("presets") }
    pub fn runs(&self) -> PathBuf { self.root.join("runs") }
    pub fn cache(&self) -> PathBuf { self.root.join("cache") }
    pub fn logs(&self) -> PathBuf { self.root.join("logs") }

    pub fn ensure(&self) -> std::io::Result<()> {
        for d in [self.root.clone(), self.bin(), self.presets(), self.runs(), self.cache(), self.logs()] {
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

/// A folder in the library. `recursive`: also search all subfolders.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(from = "FolderRepr")]
pub struct LibraryFolder {
    pub path: PathBuf,
    pub recursive: bool,
}

/// Older settings stored plain paths; those count as recursive.
#[derive(Deserialize)]
#[serde(untagged)]
enum FolderRepr {
    Path(PathBuf),
    Full { path: PathBuf, recursive: bool },
}

impl From<FolderRepr> for LibraryFolder {
    fn from(r: FolderRepr) -> Self {
        match r {
            FolderRepr::Path(path) => Self { path, recursive: true },
            FolderRepr::Full { path, recursive } => Self { path, recursive },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub binary: Option<InstalledBinary>,
    /// Esobold mode (Advanced setup): run Esobold instead of KoboldCpp.
    pub esobold: bool,
    /// The installed Esobold (kept while in KoboldCpp mode, and the other way round).
    pub esobold_binary: Option<InstalledBinary>,
    pub check_updates_on_start: bool,
    pub include_prereleases: bool,
    /// Where downloads go; `None` = app data `models/`.
    pub models_dir: Option<PathBuf>,
    /// Where KoboldCpp is installed; `None` = app data `bin/`.
    pub koboldcpp_dir: Option<PathBuf>,
    /// Where Esobold is installed; `None` = app data `bin-esobold/`.
    pub esobold_dir: Option<PathBuf>,
    /// Where presets (.kcpps) are kept; `None` = app data `presets/`.
    pub presets_dir: Option<PathBuf>,
    /// Extra folders the library scans for .gguf files.
    pub library_folders: Vec<LibraryFolder>,
    /// Hugging Face sources, highest priority first.
    pub sources: Vec<crate::hf::Source>,
    /// Version of the default source list these settings have seen (older files: 0).
    #[serde(default)]
    pub sources_version: u32,
    /// Model path → last start (unix seconds).
    pub last_used: std::collections::HashMap<String, u64>,
    /// Model path → generation speed KoboldCpp reported (tokens/s), from the last answers.
    pub speeds: std::collections::HashMap<String, f64>,
    /// How much RAM models may use (see `fit::RAM_SHARE_*`).
    pub ram_mode: RamMode,
}

/// "stable": models use at most 60 % of RAM, the system stays responsive. "unstable": up to
/// 98 %, for the biggest models; the system may slow down or run out of memory.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RamMode {
    #[default]
    Stable,
    Unstable,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            binary: None,
            esobold: false,
            esobold_binary: None,
            check_updates_on_start: true,
            include_prereleases: false,
            models_dir: None,
            koboldcpp_dir: None,
            esobold_dir: None,
            presets_dir: None,
            library_folders: vec![],
            sources: crate::hf::default_sources(),
            sources_version: crate::hf::SOURCES_VERSION,
            last_used: Default::default(),
            speeds: Default::default(),
            ram_mode: RamMode::Stable,
        }
    }
}

impl Settings {
    pub fn flavor(&self) -> crate::binary::Flavor {
        crate::binary::Flavor::of(self.esobold)
    }
    /// The program for the current mode (KoboldCpp or Esobold).
    pub fn active_binary(&self) -> Option<&InstalledBinary> {
        if self.esobold { self.esobold_binary.as_ref() } else { self.binary.as_ref() }
    }
    pub fn set_active_binary(&mut self, b: Option<InstalledBinary>) {
        if self.esobold { self.esobold_binary = b } else { self.binary = b }
    }

    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path).ok().and_then(|t| Self::parse(&t).ok()).unwrap_or_else(|| Self::parse("{}").expect("empty settings"))
    }

    /// Settings from JSON (a settings file or a backup), with older source lists brought up to date.
    pub fn parse(text: &str) -> Result<Self, serde_json::Error> {
        let mut s: Self = serde_json::from_str(text)?;
        s.sources = crate::hf::migrate_sources(std::mem::take(&mut s.sources));
        crate::hf::add_new_defaults(&mut s.sources, s.sources_version);
        s.sources_version = crate::hf::SOURCES_VERSION;
        Ok(s)
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        write_atomic(path, serde_json::to_string_pretty(self).expect("settings serialize").as_bytes())
    }

    pub fn models_dir(&self, dirs: &Dirs) -> PathBuf {
        self.models_dir.clone().unwrap_or_else(|| dirs.default_models())
    }
    /// Install folder of KoboldCpp or Esobold.
    pub fn bin_dir(&self, dirs: &Dirs, flavor: crate::binary::Flavor) -> PathBuf {
        let own = match flavor {
            crate::binary::Flavor::KoboldCpp => &self.koboldcpp_dir,
            crate::binary::Flavor::Esobold => &self.esobold_dir,
        };
        own.clone().unwrap_or_else(|| dirs.bin_of(flavor))
    }
    /// Every folder a KoboldCpp or Esobold of ours may run from: both install folders and the
    /// folders of the recorded programs (an install from before a folder change stays usable).
    pub fn all_bin_dirs(&self, dirs: &Dirs) -> Vec<PathBuf> {
        let mut out = vec![self.bin_dir(dirs, crate::binary::Flavor::KoboldCpp), self.bin_dir(dirs, crate::binary::Flavor::Esobold), dirs.bin(), dirs.bin_esobold()];
        for b in [&self.binary, &self.esobold_binary].into_iter().flatten() {
            // `<bin dir>/<tag>/<program>`: the bin dir is two levels up.
            if let Some(d) = b.path.parent().and_then(Path::parent) {
                out.push(d.to_path_buf());
            }
        }
        out.sort();
        out.dedup();
        out
    }
    pub fn presets_dir(&self, dirs: &Dirs) -> PathBuf {
        self.presets_dir.clone().unwrap_or_else(|| dirs.presets())
    }
}

/// Name of the file in the default data folder that points to a data folder elsewhere
/// (Advanced setup › Launcher Setup). Absent: the default folder is used.
pub const LOCATION_FILE: &str = "data-location.txt";

/// The launcher's data folder: the default one, or where `data-location.txt` in it points
/// (if that folder exists).
pub fn resolve_root(default_root: &Path) -> PathBuf {
    std::fs::read_to_string(default_root.join(LOCATION_FILE))
        .ok()
        .map(|t| PathBuf::from(t.trim()))
        .filter(|p| p.is_absolute() && p.is_dir())
        .unwrap_or_else(|| default_root.to_path_buf())
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

    #[test]
    fn old_plain_folder_paths_load_as_recursive() {
        let s: Settings = serde_json::from_str(r#"{"libraryFolders":["/a",{"path":"/b","recursive":false}]}"#).unwrap();
        assert_eq!(s.library_folders, [LibraryFolder { path: "/a".into(), recursive: true }, LibraryFolder { path: "/b".into(), recursive: false }]);
    }
}
