//! Local model library: scan folders for GGUF files, classify them, drives, move and remove.

use crate::gguf;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelType {
    Text,
    Vision,
    Image,
    Speech,
    Embedding,
    /// Vision projector (`mmproj`), loaded next to a text model.
    Projector,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalModel {
    pub path: PathBuf,
    pub file: String,
    pub name: String,
    pub kind: ModelType,
    pub architecture: Option<String>,
    pub quant: Option<String>,
    pub size_label: Option<String>,
    /// All split parts together.
    pub size: u64,
    pub parts: u32,
    pub layers: Option<u32>,
    pub context_length: Option<u64>,
    pub head_count_kv: Option<u32>,
    pub kv_len: Option<u32>,
    /// Matching vision projector in the same folder, if any.
    pub mmproj: Option<PathBuf>,
    pub drive: Option<String>,
    pub modified: Option<u64>,
    pub last_used: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Drive {
    pub mount: String,
    pub name: String,
    pub total: u64,
    pub free: u64,
    pub removable: bool,
}

/// Type from architecture and file name.
pub fn classify(file: &str, arch: Option<&str>) -> ModelType {
    let f = file.to_ascii_lowercase();
    let a = arch.unwrap_or("").to_ascii_lowercase();
    if f.starts_with("mmproj") || a == "clip" {
        ModelType::Projector
    } else if a.contains("whisper") || f.contains("whisper") || f.contains("tts") || f.contains("kokoro") || a.contains("outetts") || a.contains("wavtokenizer") {
        ModelType::Speech
    } else if a.contains("bert") || f.contains("embed") || f.contains("bge-") || a == "xlm-roberta" {
        ModelType::Embedding
    } else if ["flux", "stable-diffusion", "sdxl", "sd3", "sd_", "z-image", "qwen-image", "wan2", "wan_2", "chroma"].iter().any(|k| f.contains(k)) || (a.is_empty() && f.contains("diffusion")) {
        ModelType::Image
    } else {
        ModelType::Text
    }
}

/// Folder of a path's drive: the longest mount point that contains it.
pub fn drive_of<'a>(path: &Path, drives: &'a [Drive]) -> Option<&'a Drive> {
    drives.iter().filter(|d| path.starts_with(&d.mount)).max_by_key(|d| d.mount.len())
}

pub fn drives() -> Vec<Drive> {
    let Ok(disks) = sysinfo::Disks::new_with_refreshed_list() else { return vec![] };
    let mut out: Vec<Drive> = disks
        .iter()
        .map(|d| Drive {
            mount: d.mount_point().to_string_lossy().into_owned(),
            name: d.name().to_string_lossy().into_owned(),
            total: d.total_space(),
            free: d.available_space(),
            removable: d.is_removable(),
        })
        .filter(|d| d.total > 0)
        .collect();
    out.sort_by(|a, b| a.mount.cmp(&b.mount));
    out.dedup_by(|a, b| a.mount == b.mount);
    out
}

fn walk(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() && depth > 0 && !e.file_name().to_string_lossy().starts_with('.') {
            walk(&p, depth - 1, out);
        } else if ft.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("gguf")) {
            out.push(p);
        }
    }
}

/// Scan folders (4 levels deep) for GGUF models. Split models show once (their first part).
pub fn scan(folders: &[PathBuf], last_used: &HashMap<String, u64>) -> (Vec<LocalModel>, Vec<Drive>) {
    let mut files = Vec::new();
    for f in folders {
        walk(f, 4, &mut files);
    }
    files.sort();
    files.dedup();
    let all_drives = drives();
    let mut models: Vec<LocalModel> = files
        .iter()
        .filter(|p| {
            let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            gguf::split_part(n).is_none_or(|(part, _)| part == 1)
        })
        .map(|p| {
            let file = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
            let info = gguf::read_file(p).ok();
            let size = info.as_ref().map(|i| i.total_size).filter(|&s| s > 0).unwrap_or_else(|| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0));
            let arch = info.as_ref().and_then(|i| i.architecture.clone());
            let stem = file.trim_end_matches(".gguf");
            LocalModel {
                kind: classify(&file, arch.as_deref()),
                name: info.as_ref().and_then(|i| i.name.clone()).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| stem.to_string()),
                quant: info.as_ref().and_then(|i| i.file_type.clone()).or_else(|| crate::hf::quant_of(&file).map(|q| q.0)),
                size_label: info.as_ref().and_then(|i| i.size_label.clone()),
                size,
                parts: info.as_ref().map(|i| i.parts).unwrap_or(1),
                layers: info.as_ref().and_then(|i| i.block_count),
                context_length: info.as_ref().and_then(|i| i.context_length),
                head_count_kv: info.as_ref().and_then(|i| i.head_count_kv),
                kv_len: info.as_ref().and_then(|i| i.key_length.max(i.value_length)),
                architecture: arch,
                mmproj: None,
                drive: drive_of(p, &all_drives).map(|d| d.mount.clone()),
                modified: std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()),
                last_used: last_used.get(&p.to_string_lossy().to_string()).copied(),
                path: p.clone(),
                file,
            }
        })
        .collect();
    // Pair text models with a projector in the same folder → vision model.
    let projectors: Vec<(PathBuf, String)> = models.iter().filter(|m| m.kind == ModelType::Projector).map(|m| (m.path.clone(), m.file.to_ascii_lowercase())).collect();
    for m in models.iter_mut().filter(|m| m.kind == ModelType::Text) {
        let dir = m.path.parent();
        let in_dir: Vec<&(PathBuf, String)> = projectors.iter().filter(|(p, _)| p.parent() == dir).collect();
        // Prefer a projector whose name shares the model's base name; a lone projector in a model folder also counts.
        let base = m.file.to_ascii_lowercase();
        let base = base.split(['-', '_', '.']).next().unwrap_or("");
        let is_root = folders.iter().any(|f| Some(f.as_path()) == dir);
        let found = in_dir.iter().find(|(_, f)| !base.is_empty() && f.contains(base)).or(if in_dir.len() == 1 && !is_root { in_dir.first() } else { None });
        if let Some((p, _)) = found {
            m.mmproj = Some(p.clone());
            m.kind = ModelType::Vision;
        }
    }
    let used: std::collections::HashSet<String> = models.iter().filter_map(|m| m.drive.clone()).collect();
    let shown = all_drives.into_iter().filter(|d| used.contains(&d.mount)).collect();
    (models, shown)
}

/// Every file belonging to a model (all split parts).
pub fn model_files(path: &Path) -> Vec<PathBuf> {
    gguf::split_paths(path).into_iter().filter(|p| p.exists()).collect()
}

/// Is `path` a GGUF inside one of `roots`? Guards move/remove against arbitrary paths.
pub fn is_managed(path: &Path, roots: &[PathBuf]) -> bool {
    let Ok(p) = path.canonicalize() else { return false };
    p.extension().is_some_and(|x| x.eq_ignore_ascii_case("gguf")) && roots.iter().filter_map(|r| r.canonicalize().ok()).any(|r| p.starts_with(r))
}

/// Move all parts to `dest_dir`: rename when possible, else copy, check size, delete original.
pub fn move_model(path: &Path, dest_dir: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dest_dir)?;
    let mut first = None;
    for src in model_files(path) {
        let dst = dest_dir.join(src.file_name().unwrap());
        if dst.exists() {
            return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, format!("{} already exists there", dst.display())));
        }
        if std::fs::rename(&src, &dst).is_err() {
            let tmp = dst.with_extension("gguf.part");
            std::fs::copy(&src, &tmp)?;
            if std::fs::metadata(&tmp)?.len() != std::fs::metadata(&src)?.len() {
                let _ = std::fs::remove_file(&tmp);
                return Err(std::io::Error::other("copy is incomplete"));
            }
            std::fs::rename(&tmp, &dst)?;
            std::fs::remove_file(&src)?;
        }
        first.get_or_insert(dst);
    }
    first.ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "model file not found"))
}

/// Move all parts to the system Trash (recoverable), never a hard delete.
pub fn trash_model(path: &Path) -> Result<(), trash::Error> {
    #[allow(unused_mut)]
    let mut ctx = trash::TrashContext::default();
    #[cfg(target_os = "macos")]
    {
        // The native call; the default scripts Finder and asks for Apple Events permission.
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        ctx.set_delete_method(DeleteMethod::NsFileManager);
    }
    ctx.delete_all(model_files(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_by_name_and_arch() {
        assert_eq!(classify("mmproj-F16.gguf", Some("clip")), ModelType::Projector);
        assert_eq!(classify("whisper-base.en-q5_1.gguf", None), ModelType::Speech);
        assert_eq!(classify("Kokoro_no_espeak_Q4.gguf", None), ModelType::Speech);
        assert_eq!(classify("bge-m3-q8_0.gguf", Some("bert")), ModelType::Embedding);
        assert_eq!(classify("flux-2-klein-4b-Q4_0.gguf", None), ModelType::Image);
        assert_eq!(classify("Qwen3-8B-Q4_K_M.gguf", Some("qwen3")), ModelType::Text);
    }

    #[test]
    fn scans_pairs_projectors_and_moves() {
        let root = tempfile::tempdir().unwrap();
        let d = root.path().join("Qwen3-VL-8B");
        std::fs::create_dir_all(&d).unwrap();
        let head = gguf::tests::llama_like();
        std::fs::write(d.join("Qwen3-VL-8B-Q4_K_M.gguf"), &head).unwrap();
        std::fs::write(d.join("mmproj-Qwen3-VL-8B-F16.gguf"), b"GGUF").unwrap();
        std::fs::write(root.path().join("big-00001-of-00002.gguf"), &head).unwrap();
        std::fs::write(root.path().join("big-00002-of-00002.gguf"), vec![0u8; 500]).unwrap();
        std::fs::write(root.path().join("notes.txt"), b"x").unwrap();
        let (models, _) = scan(&[root.path().to_path_buf()], &HashMap::new());
        assert_eq!(models.len(), 3, "{:?}", models.iter().map(|m| &m.file).collect::<Vec<_>>());
        let vl = models.iter().find(|m| m.file.starts_with("Qwen3-VL")).unwrap();
        assert_eq!(vl.kind, ModelType::Vision);
        assert!(vl.mmproj.as_ref().unwrap().ends_with("mmproj-Qwen3-VL-8B-F16.gguf"));
        assert_eq!(vl.layers, Some(32));
        let big = models.iter().find(|m| m.file.starts_with("big")).unwrap();
        assert_eq!((big.parts, big.size), (2, head.len() as u64 + 500));

        assert!(is_managed(&big.path, &[root.path().to_path_buf()]));
        assert!(!is_managed(Path::new("/etc/hosts"), &[root.path().to_path_buf()]));
        let dest = root.path().join("archive");
        let moved = move_model(&big.path, &dest).unwrap();
        assert!(moved.ends_with("archive/big-00001-of-00002.gguf"));
        assert!(dest.join("big-00002-of-00002.gguf").exists() && !big.path.exists());
    }
}

#[cfg(test)]
mod real_folder {
    /// `KL_MODELS=/path cargo test --lib real_folder -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn scan_real_folder() {
        let p = std::path::PathBuf::from(std::env::var("KL_MODELS").expect("set KL_MODELS"));
        let (models, drives) = super::scan(&[p], &Default::default());
        for m in &models {
            println!("{:?} {} {:?} {} layers={:?} mmproj={:?}", m.kind, m.file, m.quant, m.size, m.layers, m.mmproj);
        }
        println!("drives: {drives:?}");
        assert!(!models.is_empty());
    }
}
