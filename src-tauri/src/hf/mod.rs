//! Hugging Face: model lists, ranking across the user's sources, repo files and quants.
//! This file is pure logic (tested on recorded API responses); `client.rs` does the requests.

pub mod client;
pub mod templates;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// One entry of `GET /api/models?...&expand[]=likes&expand[]=downloads&expand[]=createdAt&expand[]=baseModels&expand[]=tags`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelEntry {
    pub id: String,
    #[serde(default)]
    pub likes: u64,
    /// Downloads over the last 30 days.
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub base_models: Option<BaseModels>,
    #[serde(default)]
    pub pipeline_tag: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BaseModels {
    #[serde(default)]
    pub relation: Option<String>,
    #[serde(default)]
    pub models: Vec<BaseModelRef>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BaseModelRef {
    pub id: String,
}

impl ModelEntry {
    pub fn author(&self) -> &str {
        self.id.split('/').next().unwrap_or("")
    }
    pub fn repo_name(&self) -> &str {
        self.id.split('/').nth(1).unwrap_or(&self.id)
    }
    pub fn base_model(&self) -> Option<&str> {
        self.base_models.as_ref().and_then(|b| b.models.first()).map(|m| m.id.as_str())
    }
    pub fn relation(&self) -> Option<&str> {
        self.base_models.as_ref().and_then(|b| b.relation.as_deref())
    }
}

/// Repo name without author prefix (`bartowski/Qwen_Qwen3-8B-GGUF` → `Qwen3-8B`) and GGUF suffix.
pub fn clean_name(repo_name: &str) -> String {
    let mut n = repo_name.to_string();
    for suf in ["-GGUF", "_GGUF", "-gguf", ".gguf", "-GGUFs"] {
        if let Some(s) = n.strip_suffix(suf) {
            n = s.to_string();
        }
    }
    // bartowski style "Org_Model": drop "Org_" when it looks like an organisation name.
    if let Some((org, rest)) = n.split_once('_')
        && !org.is_empty()
        && !org.contains(['-', '.'])
        && !rest.is_empty()
    {
        n = rest.to_string();
    }
    n
}

/// Display name: "Qwen3.8-27B-Instruct" → "Qwen3.8 27B Instruct".
pub fn display_name(repo_name: &str) -> String {
    clean_name(repo_name).replace(['-', '_'], " ")
}

/// Parameter size from the name, e.g. "27B", "1.7B", "30B-A3B" → "30B".
pub fn params_from_name(name: &str) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'.') {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            if i < bytes.len() && (bytes[i] == b'b' || bytes[i] == b'm') && (i + 1 == bytes.len() || !bytes[i + 1].is_ascii_alphanumeric()) {
                return Some(format!("{}{}", &name[start..i], (bytes[i] as char).to_ascii_uppercase()));
            }
        }
        i += 1;
    }
    None
}

/// Official upload: HF says "quantized" (self-declared, so not enough on its own) **and** the
/// repo name is the base model's name plus a GGUF suffix (design notes).
pub fn is_official(e: &ModelEntry) -> bool {
    let Some(base) = e.base_model() else { return false };
    if e.relation() != Some("quantized") {
        return false;
    }
    let base_name = base.split('/').nth(1).unwrap_or(base);
    let norm = |s: &str| s.to_ascii_lowercase().replace('_', "-");
    norm(&clean_name(e.repo_name())) == norm(base_name)
}

const UNCENSORED_WORDS: &[&str] = &["uncensored", "abliterated", "unfiltered", "decensored", "orthogonalized", "heretic"];

/// Chat/text models (also vision chat). Image, video, speech and embedding models are left out
/// of the popular list; KoboldCpp's templates cover those.
pub fn is_text_model(e: &ModelEntry) -> bool {
    const OTHER: &[&str] = &["text-to-image", "image-to-image", "text-to-video", "image-to-video", "text-to-speech", "text-to-audio", "automatic-speech-recognition", "feature-extraction", "sentence-similarity"];
    let pipe_ok = e.pipeline_tag.as_deref().is_none_or(|p| !OTHER.contains(&p));
    pipe_ok && !e.tags.iter().any(|t| OTHER.contains(&t.as_str()))
}

pub fn is_uncensored(e: &ModelEntry) -> bool {
    let id = e.id.to_ascii_lowercase();
    UNCENSORED_WORDS.iter().any(|w| id.contains(w)) || e.tags.iter().any(|t| t == "uncensored" || t == "not-for-all-audiences")
}

/// Days since `createdAt` (ISO 8601). Parsed by hand to avoid a date crate.
pub fn age_days(created: Option<&str>, now_unix: i64) -> f64 {
    let Some(c) = created else { return 365.0 };
    let p = |a: usize, b: usize| c.get(a..b).and_then(|s| s.parse::<i64>().ok());
    let (Some(y), Some(m), Some(d)) = (p(0, 4), p(5, 7), p(8, 10)) else { return 365.0 };
    // days from civil (Howard Hinnant's algorithm)
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    ((now_unix / 86400 - days) as f64).max(0.0)
}

/// Popularity: `(log10(downloads_30d) + 0.6·log10(likes)) × max(0.35, 0.5^(age_days/180))`.
pub fn score(e: &ModelEntry, now_unix: i64) -> f64 {
    let decay = 0.5f64.powf(age_days(e.created_at.as_deref(), now_unix) / 180.0).max(0.35);
    (((e.downloads + 1) as f64).log10() + 0.6 * ((e.likes + 1) as f64).log10()) * decay
}

/// Key that merges uploads of the same model by different uploaders.
pub fn canonical_key(e: &ModelEntry) -> String {
    if is_official(e)
        && let Some(b) = e.base_model()
    {
        return b.to_ascii_lowercase();
    }
    clean_name(e.repo_name()).to_ascii_lowercase().replace('_', "-")
}

/* ---------- sources ---------- */

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Components {
    pub official: bool,
    pub finetune: bool,
    pub unc: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    /// "kcppt" (KoboldCpp templates), "labs" (uploads by the base model's own author), or an HF user name.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub builtin: bool,
    pub on: bool,
    /// `None` for the template source.
    pub comps: Option<Components>,
    #[serde(default)]
    pub desc: String,
}

pub fn default_sources() -> Vec<Source> {
    let s = |id: &str, name: &str, builtin, on, comps: Option<(bool, bool, bool)>, desc: &str| Source {
        id: id.into(), name: name.into(), builtin, on, desc: desc.into(),
        comps: comps.map(|(official, finetune, unc)| Components { official, finetune, unc }),
    };
    vec![
        s("kcppt", "KoboldCpp templates", true, true, None, "Ready-made setups from hf.co/koboldcpp"),
        s("labs", "Labs' own uploads", true, true, Some((true, false, false)), "Repo author is the base model author"),
        s("unsloth", "unsloth", false, true, Some((true, false, false)), "Official quants of new models"),
        s("bartowski", "bartowski", false, false, Some((true, true, false)), "Official quants and community finetunes"),
        s("TheDrummer", "TheDrummer", false, false, Some((false, true, false)), "Roleplay and story finetunes"),
    ]
}

/// Valid Hugging Face user or organisation name.
pub fn valid_source_name(n: &str) -> bool {
    let b = n.as_bytes();
    (2..=96).contains(&b.len()) && b[0].is_ascii_alphanumeric() && b.iter().all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(c))
}

/// Priority index of the first enabled source that offers this entry with an allowed component.
pub fn source_index(e: &ModelEntry, sources: &[Source], official: bool, uncensored: bool) -> Option<usize> {
    sources.iter().filter(|s| s.on && s.comps.is_some()).position(|s| {
        let matches = if s.id == "labs" {
            e.base_model().is_some_and(|b| b.split('/').next() == Some(e.author()))
        } else {
            s.name.eq_ignore_ascii_case(e.author())
        };
        let c = s.comps.unwrap();
        matches && if uncensored { c.unc } else if official { c.official } else { c.finetune }
    })
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RankOptions {
    pub official_only: bool,
    pub show_uncensored: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ranked {
    pub id: String,
    pub name: String,
    pub author: String,
    pub params: Option<String>,
    pub official: bool,
    pub uncensored: bool,
    pub likes: u64,
    pub downloads: u64,
    pub age_days: f64,
    pub score: f64,
    pub source: usize,
    pub base_model: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankResult {
    pub items: Vec<Ranked>,
    /// Uploads hidden because a higher-priority source has the same model.
    pub dupes: usize,
}

/// Filter by switches and sources, merge duplicates (higher source wins, then higher score),
/// sort by score.
pub fn rank(entries: &[ModelEntry], sources: &[Source], opts: RankOptions, now_unix: i64) -> RankResult {
    let mut best: HashMap<String, Ranked> = HashMap::new();
    let mut kept = 0usize;
    let mut seen = std::collections::HashSet::new();
    for e in entries {
        if !seen.insert(e.id.clone()) {
            continue;
        }
        if !is_text_model(e) {
            continue;
        }
        let official = is_official(e);
        let uncensored = is_uncensored(e);
        if opts.official_only && !official || uncensored && !opts.show_uncensored {
            continue;
        }
        let Some(si) = source_index(e, sources, official, uncensored) else { continue };
        kept += 1;
        let r = Ranked {
            id: e.id.clone(),
            name: display_name(e.repo_name()),
            author: e.author().to_string(),
            params: params_from_name(e.repo_name()),
            official,
            uncensored,
            likes: e.likes,
            downloads: e.downloads,
            age_days: age_days(e.created_at.as_deref(), now_unix),
            score: score(e, now_unix),
            source: si,
            base_model: e.base_model().map(str::to_string),
            tags: e.tags.clone(),
        };
        let key = canonical_key(e);
        match best.get(&key) {
            Some(cur) if (cur.source, -cur.score) <= (r.source, -r.score) => {}
            _ => {
                best.insert(key, r);
            }
        }
    }
    let mut items: Vec<Ranked> = best.into_values().collect();
    items.sort_by(|a, b| b.score.total_cmp(&a.score));
    RankResult { dupes: kept - items.len(), items }
}

/* ---------- repo files ---------- */

#[derive(Debug, Clone, Deserialize)]
pub struct TreeEntry {
    #[serde(rename = "type")]
    pub kind: String,
    pub path: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuantFile {
    /// Normalised quant, e.g. "Q4_K_M" (also for "UD-Q4_K_M").
    pub quant: String,
    /// Uploader-specific flavour, e.g. "UD" (unsloth dynamic) or "XL".
    pub label: String,
    /// Repo paths; several for split files, in order.
    pub files: Vec<String>,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RepoFiles {
    pub quants: Vec<QuantFile>,
    /// Vision projector files (`mmproj-*.gguf`) with sizes.
    pub mmproj: Vec<(String, u64)>,
}

impl RepoFiles {
    /// Best vision projector: F16, then BF16, then Q8_0, then any.
    pub fn best_mmproj(&self) -> Option<&(String, u64)> {
        ["F16", "BF16", "Q8_0"].iter().find_map(|q| self.mmproj.iter().find(|(p, _)| p.to_ascii_uppercase().contains(&format!("-{q}.")))).or(self.mmproj.first())
    }

    /// Sizes per normalised quant (smallest file of each quant), for `fit::pick_quant`.
    pub fn sizes(&self) -> Vec<(String, u64)> {
        let mut m: HashMap<&str, u64> = HashMap::new();
        for q in &self.quants {
            let e = m.entry(&q.quant).or_insert(q.size);
            *e = (*e).min(q.size);
        }
        m.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
    }

    /// The file set for a quant: plain upload preferred over special flavours.
    pub fn quant(&self, quant: &str) -> Option<&QuantFile> {
        let mut c: Vec<&QuantFile> = self.quants.iter().filter(|q| q.quant.eq_ignore_ascii_case(quant)).collect();
        c.sort_by_key(|q| (q.label != q.quant, q.size));
        c.first().copied()
    }
}

/// Quant name in a file name: "Qwen3-8B-UD-Q4_K_XL.gguf" → ("Q4_K_XL", "UD-Q4_K_XL").
pub fn quant_of(file_name: &str) -> Option<(String, String)> {
    let stem = file_name.rsplit('/').next()?.trim_end_matches(".gguf");
    let stem = match crate::gguf::split_part(&format!("{stem}.gguf")) {
        Some(_) => &stem[..stem.len() - "-00001-of-00002".len()],
        None => stem,
    };
    let upper = stem.to_ascii_uppercase();
    let tokens: Vec<&str> = upper.split(['-', '.']).collect();
    for (i, t) in tokens.iter().enumerate().rev() {
        let t = *t;
        let is_q = (t.starts_with('Q') || t.starts_with("IQ") || t.starts_with("TQ")) && t.chars().nth(if t.starts_with('Q') { 1 } else { 2 }).is_some_and(|c| c.is_ascii_digit())
            || matches!(t, "F16" | "BF16" | "F32")
            || t.starts_with("MXFP4");
        if is_q {
            let label = if i > 0 && tokens[i - 1] == "UD" { format!("UD-{t}") } else { t.to_string() };
            return Some((t.to_string(), label));
        }
    }
    None
}

/// Path without the split tag: "a/m-00002-of-00003.gguf" → "a/m.gguf".
fn split_base(path: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or(path);
    match crate::gguf::split_part(file) {
        Some((p, n)) => path.replace(&format!("-{p:05}-of-{n:05}"), ""),
        None => path.to_string(),
    }
}

/// Group a repo tree into quants (split parts summed) and vision projectors.
pub fn parse_tree(entries: &[TreeEntry]) -> RepoFiles {
    let mut groups: Vec<QuantFile> = Vec::new();
    let mut keys: Vec<String> = Vec::new();
    let mut mmproj = Vec::new();
    let mut paths: Vec<&TreeEntry> = entries.iter().filter(|e| e.kind == "file" && e.path.to_ascii_lowercase().ends_with(".gguf")).collect();
    paths.sort_by(|a, b| a.path.cmp(&b.path));
    for e in paths {
        let file = e.path.rsplit('/').next().unwrap_or(&e.path);
        let lower = file.to_ascii_lowercase();
        if lower.starts_with("mmproj") {
            mmproj.push((e.path.clone(), e.size));
            continue;
        }
        if lower.contains("imatrix") || lower.starts_with("mtp-") {
            continue;
        }
        // Subfolders hold split quants ("UD-Q4_K_M/", "BF16/"); others ("MTP/", "dspark/") are helpers.
        if let Some((folder, _)) = e.path.rsplit_once('/')
            && quant_of(&format!("{}.gguf", folder.rsplit('/').next().unwrap_or(folder))).is_none()
        {
            continue;
        }
        let Some((quant, label)) = quant_of(file) else { continue };
        // Split parts (-00001-of-00003) share one entry.
        let key = split_base(&e.path);
        if let Some(i) = keys.iter().position(|k| *k == key) {
            groups[i].files.push(e.path.clone());
            groups[i].size += e.size;
        } else {
            keys.push(key);
            groups.push(QuantFile { quant, label, files: vec![e.path.clone()], size: e.size });
        }
    }
    RepoFiles { quants: groups, mmproj }
}

/// Download URL of a repo file.
pub fn resolve_url(repo: &str, path: &str) -> String {
    format!("https://huggingface.co/{repo}/resolve/main/{path}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(name: &str) -> Vec<ModelEntry> {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
        serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
    }

    // 2026-11-05
    const NOW: i64 = 1_793_836_800;

    #[test]
    fn names() {
        assert_eq!(clean_name("Qwen_Qwen3.8-27B-GGUF"), "Qwen3.8-27B");
        assert_eq!(clean_name("TheDrummer_Cydonia-24B-v4.2.0-GGUF"), "Cydonia-24B-v4.2.0");
        assert_eq!(clean_name("Mistral-Small-3.2-24B-Instruct-2506-GGUF"), "Mistral-Small-3.2-24B-Instruct-2506");
        assert_eq!(params_from_name("Qwen3.8-27B-GGUF").as_deref(), Some("27B"));
        assert_eq!(params_from_name("LFM3-1.7B-GGUF").as_deref(), Some("1.7B"));
        assert_eq!(params_from_name("Qwen3-30B-A3B-GGUF").as_deref(), Some("30B"));
        assert_eq!(params_from_name("gemma-4-E4B-it"), None);
    }

    #[test]
    fn age_and_score() {
        assert_eq!(age_days(Some("2026-11-05T00:00:00.000Z"), NOW), 0.0);
        assert_eq!(age_days(Some("2026-08-13T08:28:40.000Z"), NOW), 84.0);
        let e = ModelEntry { id: "a/b".into(), likes: 999, downloads: 999_999, created_at: Some("2026-11-05".into()), tags: vec![], base_models: None, pipeline_tag: None };
        assert!((score(&e, NOW) - (6.0 + 0.6 * 3.0)).abs() < 1e-6);
    }

    #[test]
    fn official_rule_on_real_listing() {
        let l = list("hf-list-unsloth.json");
        let qwen = l.iter().find(|e| e.id == "unsloth/Qwen3.8-27B-GGUF").unwrap();
        assert!(is_official(qwen));
        assert_eq!(canonical_key(qwen), "qwen/qwen3.8-27b");
        let drummer = list("hf-list-TheDrummer.json");
        assert!(drummer.iter().all(|e| !is_official(e)), "TheDrummer uploads are finetunes");
    }

    #[test]
    fn ranking_respects_sources_and_switches() {
        let mut all = list("hf-list-unsloth.json");
        all.extend(list("hf-list-bartowski.json"));
        all.extend(list("hf-list-TheDrummer.json"));
        all.extend(list("hf-list-trending.json"));
        let mut sources = default_sources();
        let r = rank(&all, &sources, RankOptions { official_only: true, show_uncensored: false }, NOW);
        assert!(!r.items.is_empty());
        assert!(r.items.iter().all(|i| i.official && !i.uncensored));
        assert!(r.items.iter().all(|i| i.author != "bartowski" && i.author != "TheDrummer"), "disabled sources");
        assert!(r.items.windows(2).all(|w| w[0].score >= w[1].score));
        // Enable TheDrummer finetunes and show all models
        for s in &mut sources { if s.id == "TheDrummer" { s.on = true; } }
        let r2 = rank(&all, &sources, RankOptions { official_only: false, show_uncensored: false }, NOW);
        assert!(r2.items.iter().any(|i| i.author == "TheDrummer"));
    }

    #[test]
    fn duplicate_uploads_merge_to_higher_source() {
        let mk = |id: &str, base: &str, dl| ModelEntry {
            id: id.into(), likes: 10, downloads: dl, created_at: Some("2026-10-01".into()), tags: vec![],
            base_models: Some(BaseModels { relation: Some("quantized".into()), models: vec![BaseModelRef { id: base.into() }] }),
            pipeline_tag: None,
        };
        let entries = vec![mk("bartowski/Qwen_Qwen3-8B-GGUF", "Qwen/Qwen3-8B", 900_000), mk("unsloth/Qwen3-8B-GGUF", "Qwen/Qwen3-8B", 100)];
        let mut sources = default_sources();
        for s in &mut sources { if s.id == "bartowski" { s.on = true; } }
        let r = rank(&entries, &sources, RankOptions { official_only: true, show_uncensored: false }, NOW);
        assert_eq!(r.items.len(), 1);
        assert_eq!(r.items[0].author, "unsloth", "unsloth is above bartowski in the list");
        assert_eq!(r.dupes, 1);
    }

    #[test]
    fn quants_from_real_tree() {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf-tree-unsloth-qwen.json");
        let tree: Vec<TreeEntry> = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        let f = parse_tree(&tree);
        let q4 = f.quant("Q4_K_M").unwrap();
        assert_eq!(q4.label, "UD-Q4_K_M");
        assert_eq!(q4.size, 16464440224);
        let bf16 = f.quant("BF16").unwrap();
        assert_eq!(bf16.files.len(), 2);
        assert_eq!(bf16.size, 49986159616 + 4671576000);
        assert!(f.quants.iter().all(|q| !q.files[0].contains("imatrix") && !q.files[0].starts_with("MTP/")));
        assert_eq!(f.best_mmproj().unwrap().0, "mmproj-F16.gguf");
        assert!(f.sizes().iter().any(|(q, _)| q == "Q6_K"));
    }

    #[test]
    fn helper_folders_and_non_text_models_are_skipped() {
        let te = |p: &str, s| TreeEntry { kind: "file".into(), path: p.into(), size: s };
        let f = parse_tree(&[
            te("dspark/dspark-DeepSeek-BF16.gguf", 8),
            te("MTP/mtp-Qwen-Q4_0.gguf", 1),
            te("UD-IQ1_M/DeepSeek-UD-IQ1_M-00001-of-00002.gguf", 10),
            te("UD-IQ1_M/DeepSeek-UD-IQ1_M-00002-of-00002.gguf", 20),
        ]);
        assert_eq!(f.quants.len(), 1);
        assert_eq!((f.quants[0].quant.as_str(), f.quants[0].size, f.quants[0].files.len()), ("IQ1_M", 30, 2));
        let mut e = ModelEntry { id: "unsloth/Qwen-Image-GGUF".into(), likes: 1, downloads: 1, created_at: None, tags: vec!["text-to-image".into()], base_models: None, pipeline_tag: None };
        assert!(!is_text_model(&e));
        e.tags.clear();
        e.pipeline_tag = Some("image-text-to-text".into());
        assert!(is_text_model(&e));
    }

    #[test]
    fn quant_names() {
        assert_eq!(quant_of("Qwen3-8B-Q4_K_M.gguf").unwrap().0, "Q4_K_M");
        assert_eq!(quant_of("Qwen3-8B-UD-IQ2_XXS.gguf").unwrap(), ("IQ2_XXS".into(), "UD-IQ2_XXS".into()));
        assert_eq!(quant_of("BF16/Qwen3.8-27B-BF16-00001-of-00002.gguf").unwrap().0, "BF16");
        assert_eq!(quant_of("model.Q8_0.gguf").unwrap().0, "Q8_0");
        assert_eq!(quant_of("readme.gguf"), None);
    }

    #[test]
    fn source_names() {
        assert!(valid_source_name("bartowski") && valid_source_name("TheDrummer") && valid_source_name("lmstudio-community"));
        assert!(!valid_source_name("a") && !valid_source_name("-bad") && !valid_source_name("a b"));
    }
}
