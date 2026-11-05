//! The KoboldCpp executable: which release asset fits this PC, download and verify it, updates.

use crate::download::{self, Cancel, DownloadError, Progress, Request};
use crate::hardware::{GpuApi, HardwareInfo};
use crate::store::InstalledBinary;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const REPO: &str = "LostRuins/koboldcpp";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub name: String,
    pub size: u64,
    pub browser_download_url: String,
    /// "sha256:<hex>" (GitHub adds this for release assets).
    #[serde(default)]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub html_url: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    pub assets: Vec<Asset>,
}

/// What the UI shows in the update dialog.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseInfo {
    pub tag: String,
    pub published_at: Option<String>,
    pub url: Option<String>,
    pub notes: String,
    pub asset: Option<String>,
    pub size: Option<u64>,
}

/// Release asset name for this machine, following the KoboldCpp README:
/// NVIDIA → full build with CUDA; other GPUs → `nocuda` (Vulkan); no AVX2 → `oldpc`.
pub fn asset_name(os: &str, arch: &str, hw: Option<&HardwareInfo>) -> Option<&'static str> {
    let nvidia = hw.is_some_and(|h| h.gpus.iter().any(|g| g.api == GpuApi::Cuda));
    let old_cpu = hw.and_then(|h| h.avx2) == Some(false);
    match (os, arch) {
        ("macos", "aarch64") => Some("koboldcpp-mac-arm64"),
        ("windows", "x86_64") if old_cpu => Some("koboldcpp-oldpc.exe"),
        ("windows", "x86_64") if nvidia => Some("koboldcpp.exe"),
        ("windows", "x86_64") => Some("koboldcpp-nocuda.exe"),
        ("linux", "x86_64") if old_cpu => Some("koboldcpp-linux-x64-oldpc"),
        ("linux", "x86_64") if nvidia => Some("koboldcpp-linux-x64"),
        ("linux", "x86_64") => Some("koboldcpp-linux-x64-nocuda"),
        ("linux", "aarch64") => Some("koboldcpp-linux-arm64"),
        _ => None,
    }
}

/// Newest non-draft release; pre-releases only if asked.
pub fn latest(releases: &[Release], include_prereleases: bool) -> Option<&Release> {
    releases.iter().find(|r| !r.draft && (include_prereleases || !r.prerelease))
}

pub fn info(r: &Release, asset: Option<&str>) -> ReleaseInfo {
    let a = asset.and_then(|n| r.assets.iter().find(|a| a.name == n));
    ReleaseInfo {
        tag: r.tag_name.clone(),
        published_at: r.published_at.clone(),
        url: r.html_url.clone(),
        notes: r.body.clone().unwrap_or_default(),
        asset: a.map(|a| a.name.clone()),
        size: a.map(|a| a.size),
    }
}

/// Compare tags like "v1.124.2" numerically; `true` if `candidate` is newer than `current`.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    let nums = |s: &str| -> Vec<u64> { s.trim_start_matches(['v', 'V']).split(['.', '-']).map_while(|p| p.parse().ok()).collect() };
    nums(candidate) > nums(current)
}

pub async fn fetch_releases(client: &reqwest::Client) -> Result<Vec<Release>, DownloadError> {
    let resp = client
        .get(format!("https://api.github.com/repos/{REPO}/releases?per_page=15"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(DownloadError::Status(resp.status().as_u16()));
    }
    Ok(resp.json().await?)
}

/// Download `asset` of `release` into `bin_dir/<tag>/`, verify its checksum and make it executable.
pub async fn install(client: &reqwest::Client, release: &Release, asset: &str, bin_dir: &Path, cancel: &Cancel, progress: impl FnMut(Progress)) -> Result<InstalledBinary, DownloadError> {
    let a = release.assets.iter().find(|a| a.name == asset).ok_or(DownloadError::Status(404))?;
    let dest = bin_dir.join(&release.tag_name).join(&a.name);
    let sha = a.digest.as_deref().and_then(|d| d.strip_prefix("sha256:"));
    let path = download::download_retry(client, Request { url: &a.browser_download_url, dest: &dest, size: Some(a.size), sha256: sha }, cancel, progress).await?;
    make_executable(&path)?;
    Ok(InstalledBinary { path, version: Some(release.tag_name.clone()), asset: Some(a.name.clone()) })
}

pub fn make_executable(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = std::fs::metadata(path)?.permissions();
        p.set_mode(0o755);
        std::fs::set_permissions(path, p)?;
    }
    #[cfg(target_os = "macos")]
    {
        // Files we download ourselves normally carry no quarantine flag; remove it if present.
        let _ = std::process::Command::new("xattr").args(["-d", "com.apple.quarantine"]).arg(path).output();
    }
    Ok(())
}

/// Newest KoboldCpp already in `bin_dir/<tag>/`, if any.
pub fn find_installed(bin_dir: &Path) -> Option<InstalledBinary> {
    let mut found: Vec<InstalledBinary> = std::fs::read_dir(bin_dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|dir| {
            let tag = dir.file_name().to_string_lossy().into_owned();
            let file = std::fs::read_dir(dir.path()).ok()?.flatten().map(|f| f.path()).find(|p| {
                let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                n.starts_with("koboldcpp") && !n.ends_with(".part") && p.is_file()
            })?;
            Some(InstalledBinary { asset: file.file_name().map(|n| n.to_string_lossy().into_owned()), path: file, version: Some(tag) })
        })
        .collect();
    found.sort_by(|a, b| {
        let (x, y) = (a.version.as_deref().unwrap_or(""), b.version.as_deref().unwrap_or(""));
        if is_newer(x, y) { std::cmp::Ordering::Greater } else if is_newer(y, x) { std::cmp::Ordering::Less } else { std::cmp::Ordering::Equal }
    });
    found.pop()
}

/// Remove older installed versions, keeping `keep` (the folder of the current binary).
pub fn prune_old(bin_dir: &Path, keep: &Path) {
    let keep_dir = keep.parent().map(Path::to_path_buf);
    if let Ok(entries) = std::fs::read_dir(bin_dir) {
        for e in entries.flatten() {
            let p: PathBuf = e.path();
            if p.is_dir() && Some(&p) != keep_dir.as_ref() {
                let _ = std::fs::remove_dir_all(&p);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Backend;
    use crate::hardware::{Gpu, Tier};

    fn hw(api: Option<GpuApi>, avx2: Option<bool>) -> HardwareInfo {
        HardwareInfo {
            os: "x".into(), arch: "x".into(), cpu_name: String::new(), logical_cpus: 8, default_threads: 3, avx2,
            ram_total: 0, ram_available: 0, unified_memory: false,
            gpus: api.map(|api| vec![Gpu { index: 0, name: "g".into(), api, vram: 8 << 30, vram_free: None, discrete: true, bandwidth: None }]).unwrap_or_default(),
            ram_bandwidth: 60.0,
            suggested_backend: Backend::Auto, tier: Tier::Low,
        }
    }

    #[test]
    fn picks_asset_per_platform() {
        assert_eq!(asset_name("macos", "aarch64", None), Some("koboldcpp-mac-arm64"));
        assert_eq!(asset_name("macos", "x86_64", None), None);
        assert_eq!(asset_name("windows", "x86_64", Some(&hw(Some(GpuApi::Cuda), Some(true)))), Some("koboldcpp.exe"));
        assert_eq!(asset_name("windows", "x86_64", Some(&hw(Some(GpuApi::Vulkan), Some(true)))), Some("koboldcpp-nocuda.exe"));
        assert_eq!(asset_name("windows", "x86_64", Some(&hw(Some(GpuApi::Cuda), Some(false)))), Some("koboldcpp-oldpc.exe"));
        assert_eq!(asset_name("linux", "x86_64", Some(&hw(Some(GpuApi::Rocm), Some(true)))), Some("koboldcpp-linux-x64-nocuda"));
        assert_eq!(asset_name("linux", "x86_64", Some(&hw(None, Some(true)))), Some("koboldcpp-linux-x64-nocuda"));
    }

    #[test]
    fn parses_github_releases() {
        let rels: Vec<Release> = serde_json::from_str(include_str!("../tests/fixtures/github-releases.json")).unwrap();
        let r = latest(&rels, false).unwrap();
        assert_eq!(r.tag_name, "v1.124.2");
        let mac = r.assets.iter().find(|a| a.name == "koboldcpp-mac-arm64").unwrap();
        assert!(mac.digest.as_deref().unwrap().starts_with("sha256:"));
        assert!(mac.size > 10_000_000);
        let i = info(r, Some("koboldcpp-mac-arm64"));
        assert_eq!(i.size, Some(mac.size));
    }

    #[test]
    fn finds_installed_binary() {
        let d = tempfile::tempdir().unwrap();
        for tag in ["v1.123.1", "v1.124.2"] {
            std::fs::create_dir_all(d.path().join(tag)).unwrap();
            std::fs::write(d.path().join(tag).join("koboldcpp-mac-arm64"), b"x").unwrap();
        }
        std::fs::create_dir_all(d.path().join("v1.125.0")).unwrap();
        std::fs::write(d.path().join("v1.125.0/koboldcpp-mac-arm64.part"), b"x").unwrap();
        let b = find_installed(d.path()).unwrap();
        assert_eq!(b.version.as_deref(), Some("v1.124.2"));
        assert!(b.path.ends_with("v1.124.2/koboldcpp-mac-arm64"));
    }

    #[test]
    fn version_compare() {
        assert!(is_newer("v1.124.2", "v1.123.1"));
        assert!(is_newer("v1.124.10", "v1.124.9"));
        assert!(!is_newer("v1.124.2", "v1.124.2"));
        assert!(!is_newer("v1.99", "v1.124.2"));
    }
}
