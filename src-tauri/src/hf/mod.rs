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
    #[serde(default)]
    pub trending_score: Option<f64>,
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

/// Order of model lists (user's choice on the start screen).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Sort {
    Trending,
    #[default]
    Downloads,
    Likes,
    Newest,
}

impl Sort {
    /// Value for the Hugging Face API `sort=` parameter.
    pub fn api(self) -> &'static str {
        match self {
            Sort::Trending => "trendingScore",
            Sort::Downloads => "downloads",
            Sort::Likes => "likes",
            Sort::Newest => "createdAt",
        }
    }

    /// Bigger is better.
    pub fn key(self, e: &ModelEntry) -> f64 {
        match self {
            Sort::Trending => e.trending_score.unwrap_or(0.0),
            Sort::Downloads => e.downloads as f64,
            Sort::Likes => e.likes as f64,
            Sort::Newest => created_day(e.created_at.as_deref()).unwrap_or(0) as f64,
        }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Uploaded by the model maker's own account (e.g. `google/…`, `Qwen/…`).
    Official,
    /// Someone else's faithful quantization of an original model (unsloth, bartowski, …).
    Quant,
    /// A changed model (finetune, merge, distill).
    Finetune,
}

/// Hugging Face accounts of model makers. "Official" means uploaded by one of these, for its
/// own model. Anyone can upload a finetune of their own model, so "author = base author" alone
/// is not enough.
pub const MAKERS: &[&str] = &[
    "google", "Qwen", "meta-llama", "mistralai", "deepseek-ai", "zai-org", "THUDM", "openai", "nvidia",
    "LiquidAI", "microsoft", "ibm-granite", "allenai", "HuggingFaceTB", "CohereLabs", "CohereForAI",
    "ai21labs", "tencent", "baidu", "moonshotai", "MiniMaxAI", "stepfun-ai", "inclusionAI",
    "ByteDance-Seed", "swiss-ai", "apple", "xai-org", "internlm", "OpenGVLab", "upstage", "tiiuae",
    "arcee-ai", "NousResearch", "ServiceNow-AI", "Motif-Technologies", "kakaocorp", "LGAI-EXAONE",
];

pub fn is_maker(author: &str) -> bool {
    MAKERS.iter().any(|m| m.eq_ignore_ascii_case(author))
}

/// A maker's own upload of its own model, e.g. `Qwen/Qwen3-8B-GGUF` from `Qwen/Qwen3-8B`.
pub fn is_manufacturer(e: &ModelEntry) -> bool {
    is_maker(e.author()) && e.base_model().and_then(|b| b.split('/').next()).is_none_or(|a| a.eq_ignore_ascii_case(e.author()))
}

pub fn kind_of(e: &ModelEntry) -> Kind {
    if is_manufacturer(e) && !is_uncensored(e) {
        Kind::Official
    } else if is_faithful_quant(e) {
        Kind::Quant
    } else {
        Kind::Finetune
    }
}

/// Faithful quantization: HF says "quantized" (self-declared, so not enough on its own) **and**
/// the repo name is the base model's name plus a GGUF suffix (design notes).
pub fn is_faithful_quant(e: &ModelEntry) -> bool {
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
    const OTHER_PIPES: &[&str] = &["audio-classification", "voice-activity-detection", "audio-to-audio", "text-ranking"];
    // Repos without a pipeline tag still say what they are in the name.
    const OTHER_NAMES: &[&str] = &["diarization", "-asr", "asr-", "parakeet", "whisper", "-tts", "tts-", "speech", "embed", "rerank", "vad-"];
    let pipe_ok = e.pipeline_tag.as_deref().is_none_or(|p| !OTHER.contains(&p) && !OTHER_PIPES.contains(&p));
    let name = e.repo_name().to_ascii_lowercase();
    pipe_ok && !e.tags.iter().any(|t| OTHER.contains(&t.as_str())) && !OTHER_NAMES.iter().any(|k| name.contains(k))
}

pub fn is_uncensored(e: &ModelEntry) -> bool {
    let id = e.id.to_ascii_lowercase();
    UNCENSORED_WORDS.iter().any(|w| id.contains(w)) || e.tags.iter().any(|t| t == "uncensored" || t == "not-for-all-audiences")
}

/// Day number since 1970-01-01 of an ISO 8601 date ("2026-08-13T08:28:40.000Z").
/// Parsed by hand (Howard Hinnant's days-from-civil) to avoid a date crate.
pub fn created_day(created: Option<&str>) -> Option<i64> {
    let c = created?;
    let p = |a: usize, b: usize| c.get(a..b).and_then(|s| s.parse::<i64>().ok());
    let (y, m, d) = (p(0, 4)?, p(5, 7)?, p(8, 10)?);
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146097 + doe - 719468)
}

/// Days since `createdAt`; unknown dates count as a year old.
pub fn age_days(created: Option<&str>, now_unix: i64) -> f64 {
    match created_day(created) {
        Some(day) => ((now_unix / 86400 - day) as f64).max(0.0),
        None => 365.0,
    }
}

/// Popularity: `(log10(downloads_30d) + 0.6·log10(likes)) × max(0.35, 0.5^(age_days/180))`.
pub fn score(e: &ModelEntry, now_unix: i64) -> f64 {
    let decay = 0.5f64.powf(age_days(e.created_at.as_deref(), now_unix) / 180.0).max(0.35);
    (((e.downloads + 1) as f64).log10() + 0.6 * ((e.likes + 1) as f64).log10()) * decay
}

/// Key that merges uploads of the same model by different uploaders.
pub fn canonical_key(e: &ModelEntry) -> String {
    if kind_of(e) != Kind::Finetune
        && let Some(b) = e.base_model()
    {
        return b.to_ascii_lowercase();
    }
    clean_name(e.repo_name()).to_ascii_lowercase().replace('_', "-")
}

/* ---------- families ---------- */

/// A model family on the start screen and the Hugging Face account of its maker.
pub struct Family {
    pub id: &'static str,
    pub label: &'static str,
    /// HF search matches words, so sub-brands need their own term.
    pub terms: &'static [&'static str],
    pub maker: &'static str,
    pub maker_label: &'static str,
}

pub const FAMILIES: &[Family] = &[
    Family { id: "gemma", label: "Gemma", terms: &["gemma"], maker: "google", maker_label: "Google" },
    Family { id: "qwen", label: "Qwen", terms: &["qwen"], maker: "Qwen", maker_label: "Qwen (Alibaba)" },
    Family { id: "glm", label: "GLM", terms: &["glm"], maker: "zai-org", maker_label: "Z.ai" },
    Family { id: "deepseek", label: "DeepSeek", terms: &["deepseek"], maker: "deepseek-ai", maker_label: "DeepSeek" },
    Family { id: "mistral", label: "Mistral", terms: &["mistral", "ministral", "devstral", "magistral"], maker: "mistralai", maker_label: "Mistral AI" },
    Family { id: "llama", label: "Llama", terms: &["llama"], maker: "meta-llama", maker_label: "Meta" },
    Family { id: "gpt-oss", label: "GPT-oss", terms: &["gpt-oss"], maker: "openai", maker_label: "OpenAI" },
    Family { id: "nemotron", label: "Nemotron", terms: &["nemotron"], maker: "nvidia", maker_label: "NVIDIA" },
];

/// Author of the base model, e.g. "meta-llama" for a quant of `meta-llama/Llama-3.1-8B`.
pub fn base_author(e: &ModelEntry) -> Option<&str> {
    e.base_model().and_then(|b| b.split('/').next())
}

/// The model is from `maker`: uploaded by it, or a quant/finetune of its model.
pub fn from_maker(e: &ModelEntry, maker: &str) -> bool {
    e.author().eq_ignore_ascii_case(maker) || base_author(e).is_some_and(|a| a.eq_ignore_ascii_case(maker))
}

/// Family of a repo by name. Order matters: a "DeepSeek-R1-Distill-Qwen" is DeepSeek,
/// a "Llama-Nemotron" is Nemotron.
pub fn family_of(repo_id: &str) -> Option<&'static str> {
    let n = repo_id.rsplit('/').next().unwrap_or(repo_id).to_ascii_lowercase();
    let rules: &[(&str, &[&str])] = &[
        ("nemotron", &["nemotron"]),
        ("gpt-oss", &["gpt-oss"]),
        ("deepseek", &["deepseek"]),
        ("glm", &["glm"]),
        ("gemma", &["gemma"]),
        ("mistral", &["mistral", "magistral", "devstral", "ministral", "mixtral", "codestral"]),
        ("qwen", &["qwen", "qwq"]),
        ("llama", &["llama"]),
    ];
    rules.iter().find(|(_, keys)| keys.iter().any(|k| n.contains(k))).map(|(id, _)| *id)
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
    let mut v = vec![s("kcppt", "KoboldCpp templates", true, true, None, "Ready-made setups from hf.co/koboldcpp")];
    // Model makers' own accounts (hf.co/google, hf.co/Qwen, …).
    for f in FAMILIES {
        v.push(s(f.maker, f.maker, true, true, Some((true, false, false)), &format!("{}: official {} uploads (hf.co/{})", f.maker_label, f.label, f.maker)));
    }
    v.extend([
        s("unsloth", "unsloth", false, true, Some((true, false, false)), "Quants of new models, also for makers without GGUF uploads"),
        s("bartowski", "bartowski", false, false, Some((true, true, false)), "Official quants and community finetunes"),
        s("TheDrummer", "TheDrummer", false, false, Some((false, true, false)), "Roleplay and story finetunes"),
    ]);
    v
}

/// Bring a saved source list up to date: built-in sources that are missing are added (enabled,
/// after the ones already there), and the old combined "labs" entry is replaced by the makers.
pub fn migrate_sources(mut list: Vec<Source>) -> Vec<Source> {
    list.retain(|s| s.id != "labs");
    let defaults = default_sources();
    for (i, d) in defaults.iter().enumerate() {
        if d.builtin && !list.iter().any(|s| s.id == d.id) {
            // Insert after the previous default that exists, keeping the default order.
            let pos = defaults[..i].iter().rev().find_map(|p| list.iter().position(|s| s.id == p.id)).map(|p| p + 1).unwrap_or(0);
            list.insert(pos.min(list.len()), d.clone());
        }
    }
    list
}

/// Valid Hugging Face user or organisation name.
pub fn valid_source_name(n: &str) -> bool {
    let b = n.as_bytes();
    (2..=96).contains(&b.len()) && b[0].is_ascii_alphanumeric() && b.iter().all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(c))
}

/// Priority index of the first enabled source that offers this entry with an allowed component.
/// Priority index of the first enabled Hugging Face source that uploaded this entry.
/// A source that is on shows all its models (official, quants, finetunes, uncensored).
pub fn source_index(e: &ModelEntry, sources: &[Source]) -> Option<usize> {
    sources.iter().filter(|s| s.on && s.comps.is_some()).position(|s| {
        if s.id == "labs" { is_manufacturer(e) } else { s.name.eq_ignore_ascii_case(e.author()) }
    })
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RankOptions {
    /// Only original models (official uploads and faithful quants), no finetunes.
    pub official_only: bool,
    pub show_uncensored: bool,
    #[serde(default)]
    pub sort: Sort,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ranked {
    pub id: String,
    pub name: String,
    pub author: String,
    pub params: Option<String>,
    pub kind: Kind,
    /// Uploaded by the model maker (kind == Official).
    pub official: bool,
    pub family: Option<String>,
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
    let mut best: HashMap<String, (Ranked, f64)> = HashMap::new();
    let mut kept = 0usize;
    let mut seen = std::collections::HashSet::new();
    for e in entries {
        if !seen.insert(e.id.clone()) {
            continue;
        }
        if !is_text_model(e) {
            continue;
        }
        let kind = kind_of(e);
        let original = kind != Kind::Finetune;
        let uncensored = is_uncensored(e);
        if opts.official_only && !original || uncensored && !opts.show_uncensored {
            continue;
        }
        let Some(si) = source_index(e, sources) else { continue };
        kept += 1;
        let r = Ranked {
            id: e.id.clone(),
            name: display_name(e.repo_name()),
            author: e.author().to_string(),
            params: params_from_name(e.repo_name()),
            kind,
            official: kind == Kind::Official,
            family: family_of(&e.id).map(str::to_string),
            uncensored,
            likes: e.likes,
            downloads: e.downloads,
            age_days: age_days(e.created_at.as_deref(), now_unix),
            score: score(e, now_unix),
            source: si,
            base_model: e.base_model().map(str::to_string),
            tags: e.tags.clone(),
        };
        let metric = opts.sort.key(e);
        let key = canonical_key(e);
        match best.get(&key) {
            Some((cur, m)) if (cur.source, -m) <= (r.source, -metric) => {}
            _ => {
                best.insert(key, (r, metric));
            }
        }
    }
    let mut ranked: Vec<(Ranked, f64)> = best.into_values().collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    let items: Vec<Ranked> = ranked.into_iter().map(|(r, _)| r).collect();
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
    /// The repo ships MTP files (`MTP/…`, `mtp-…`): its models have multi-token-prediction
    /// layers that KoboldCpp can use (`--usemtp`). unsloth publishes them this way.
    pub mtp: bool,
}

impl RepoFiles {
    /// Best vision projector: F16, then BF16, then Q8_0, then any.
    pub fn best_mmproj(&self) -> Option<&(String, u64)> {
        ["F16", "BF16", "Q8_0"].iter().find_map(|q| self.mmproj.iter().find(|(p, _)| p.to_ascii_uppercase().contains(&format!("-{q}.")))).or(self.mmproj.first())
    }

    /// Size per normalised quant (the file set `quant()` would download), for `fit::pick_quant`.
    pub fn sizes(&self) -> Vec<(String, u64)> {
        let mut names: Vec<&str> = self.quants.iter().map(|q| q.quant.as_str()).collect();
        names.sort();
        names.dedup();
        names.into_iter().filter_map(|n| self.quant(n).map(|q| (n.to_string(), q.size))).collect()
    }

    /// The file set for a quant: plain upload preferred over special flavours; if two claim the
    /// same name, the larger one is the model (a small one is a leftover helper).
    pub fn quant(&self, quant: &str) -> Option<&QuantFile> {
        let mut c: Vec<&QuantFile> = self.quants.iter().filter(|q| q.quant.eq_ignore_ascii_case(quant)).collect();
        c.sort_by_key(|q| (q.label != q.quant, std::cmp::Reverse(q.size)));
        c.first().copied()
    }
}

/// Quant name in a file name: "Qwen3-8B-UD-Q4_K_XL.gguf" → ("Q4_K_XL", "UD-Q4_K_XL"),
/// "gemma-4-E4B_q4_0-it.gguf" → ("Q4_0", "Q4_0"). The last quant-looking part wins.
pub fn quant_of(file_name: &str) -> Option<(String, String)> {
    let stem = file_name.rsplit('/').next()?.trim_end_matches(".gguf");
    let stem = match crate::gguf::split_part(&format!("{stem}.gguf")) {
        Some(_) => &stem[..stem.len() - "-00001-of-00002".len()],
        None => stem,
    };
    let u = stem.to_ascii_uppercase();
    let b = u.as_bytes();
    let mut found: Option<(usize, usize)> = None;
    for i in 0..b.len() {
        if i > 0 && b[i - 1].is_ascii_alphanumeric() {
            continue;
        }
        if let Some(len) = quant_at(&b[i..])
            && b.get(i + len).is_none_or(|c| !c.is_ascii_alphanumeric())
        {
            found = Some((i, len));
        }
    }
    let (i, len) = found?;
    let q = u[i..i + len].to_string();
    let label = if u[..i].ends_with("UD-") || u[..i].ends_with("UD_") { format!("UD-{q}") } else { q.clone() };
    Some((q, label))
}

/// Length of a quant name at the start of `b` (uppercase): Q4_K_M, Q8_0, IQ2_XXS, TQ1_0,
/// BF16, F16, F32, MXFP4(_MOE).
fn quant_at(b: &[u8]) -> Option<usize> {
    let s = std::str::from_utf8(b).ok()?;
    for fixed in ["MXFP4_MOE", "MXFP4", "BF16", "F16", "F32"] {
        if s.starts_with(fixed) {
            return Some(fixed.len());
        }
    }
    let (prefix, rest) = match (s.strip_prefix("IQ").or_else(|| s.strip_prefix("TQ")), s.strip_prefix('Q')) {
        (Some(r), _) => (2, r),
        (None, Some(r)) => (1, r),
        (None, None) => return None,
    };
    let rb = rest.as_bytes();
    if rb.len() < 3 || !rb[0].is_ascii_digit() || rb[1] != b'_' {
        return None;
    }
    // After "Qn_": a digit (Q8_0), or letters (K, K_M, K_XL, XXS, NL, …) joined by '_'.
    let mut n = 2;
    if rb[2].is_ascii_digit() {
        n = 3;
    } else {
        while n < rb.len() && (rb[n].is_ascii_uppercase() || (rb[n] == b'_' && rb.get(n + 1).is_some_and(|c| c.is_ascii_uppercase()))) {
            n += 1;
        }
        if n == 2 {
            return None;
        }
    }
    Some(prefix + n)
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
        if lower.contains("mmproj") {
            mmproj.push((e.path.clone(), e.size));
            continue;
        }
        // Helpers that sit next to the model: importance matrix, multi-token-prediction and
        // speculative-decoding drafts (unsloth "dspark", "MTP").
        if ["imatrix", "mtp-", "dspark", "draft", "eagle"].iter().any(|k| lower.contains(k)) {
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
    let mtp = entries.iter().any(|e| {
        let l = e.path.to_ascii_lowercase();
        l.starts_with("mtp/") || l.rsplit('/').next().is_some_and(|f| f.starts_with("mtp-"))
    });
    RepoFiles { quants: groups, mmproj, mtp }
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
        let e = ModelEntry { id: "a/b".into(), likes: 999, downloads: 999_999, created_at: Some("2026-11-05".into()), tags: vec![], base_models: None, pipeline_tag: None, trending_score: None };
        assert!((score(&e, NOW) - (6.0 + 0.6 * 3.0)).abs() < 1e-6);
    }

    #[test]
    fn kinds_on_real_listing() {
        let l = list("hf-list-unsloth.json");
        let qwen = l.iter().find(|e| e.id == "unsloth/Qwen3.8-27B-GGUF").unwrap();
        assert_eq!(kind_of(qwen), Kind::Quant, "unsloth re-uploads are quants, not official");
        assert_eq!(canonical_key(qwen), "qwen/qwen3.8-27b");
        let drummer = list("hf-list-TheDrummer.json");
        assert!(drummer.iter().all(|e| kind_of(e) == Kind::Finetune), "TheDrummer uploads are finetunes");
        let trending = list("hf-list-trending.json");
        let lfm = trending.iter().find(|e| e.id == "LiquidAI/LFM3-1.7B-GGUF").unwrap();
        assert_eq!(kind_of(lfm), Kind::Official, "maker's own account");
    }

    #[test]
    fn families() {
        assert_eq!(family_of("unsloth/gemma-4-E2B-it-GGUF"), Some("gemma"));
        assert_eq!(family_of("bartowski/deepseek-ai_DeepSeek-R1-Distill-Qwen-7B-GGUF"), Some("deepseek"));
        assert_eq!(family_of("unsloth/Devstral-Small-2-GGUF"), Some("mistral"));
        assert_eq!(family_of("nvidia/Llama-3.3-Nemotron-Super-49B-GGUF"), Some("nemotron"));
        assert_eq!(family_of("unsloth/gpt-oss-20b-GGUF"), Some("gpt-oss"));
        assert_eq!(family_of("LiquidAI/LFM3-1.7B-GGUF"), None);
    }

    #[test]
    fn ranking_respects_sources_and_switches() {
        let mut all = list("hf-list-unsloth.json");
        all.extend(list("hf-list-bartowski.json"));
        all.extend(list("hf-list-TheDrummer.json"));
        all.extend(list("hf-list-trending.json"));
        let mut sources = default_sources();
        let r = rank(&all, &sources, RankOptions { official_only: true, show_uncensored: false, sort: Sort::Downloads }, NOW);
        assert!(!r.items.is_empty());
        assert!(r.items.iter().all(|i| i.kind != Kind::Finetune && !i.uncensored));
        assert!(r.items.iter().all(|i| i.author != "bartowski" && i.author != "TheDrummer"), "disabled sources");
        assert!(r.items.windows(2).all(|w| w[0].downloads >= w[1].downloads), "sorted by downloads");
        // Enable TheDrummer finetunes and show all models
        for s in &mut sources { if s.id == "TheDrummer" { s.on = true; } }
        let r2 = rank(&all, &sources, RankOptions { official_only: false, show_uncensored: false, sort: Sort::Downloads }, NOW);
        assert!(r2.items.iter().any(|i| i.author == "TheDrummer"));
    }

    #[test]
    fn duplicate_uploads_merge_to_higher_source() {
        let mk = |id: &str, base: &str, dl| ModelEntry {
            id: id.into(), likes: 10, downloads: dl, created_at: Some("2026-10-01".into()), tags: vec![],
            base_models: Some(BaseModels { relation: Some("quantized".into()), models: vec![BaseModelRef { id: base.into() }] }),
            pipeline_tag: None,
            trending_score: None,
        };
        let entries = vec![mk("bartowski/Qwen_Qwen3-8B-GGUF", "Qwen/Qwen3-8B", 900_000), mk("unsloth/Qwen3-8B-GGUF", "Qwen/Qwen3-8B", 100)];
        let mut sources = default_sources();
        for s in &mut sources { if s.id == "bartowski" { s.on = true; } }
        let r = rank(&entries, &sources, RankOptions { official_only: true, show_uncensored: false, sort: Sort::Downloads }, NOW);
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
        assert!(f.mtp, "unsloth's Qwen3.8 repo has an MTP/ folder");
        assert!(f.sizes().iter().any(|(q, _)| q == "Q6_K"));
    }

    #[test]
    fn mmproj_anywhere_in_name() {
        let te = |p: &str, s| TreeEntry { kind: "file".into(), path: p.into(), size: s };
        let f = parse_tree(&[te("gemma-4-E4B-it-mmproj.gguf", 9), te("gemma-4-E4B_q4_0-it.gguf", 48)]);
        assert_eq!(f.quants.len(), 1);
        assert_eq!(f.quants[0].quant, "Q4_0");
        assert_eq!(f.mmproj.len(), 1);
        assert!(!f.mtp);
    }

    #[test]
    fn helper_folders_and_non_text_models_are_skipped() {
        let te = |p: &str, s| TreeEntry { kind: "file".into(), path: p.into(), size: s };
        let f = parse_tree(&[
            te("dspark/dspark-DeepSeek-BF16.gguf", 8),
            te("dspark-DeepSeek-Q8_0.gguf", 2),
            te("MTP/mtp-Qwen-Q4_0.gguf", 1),
            te("UD-IQ1_M/DeepSeek-UD-IQ1_M-00001-of-00002.gguf", 10),
            te("UD-IQ1_M/DeepSeek-UD-IQ1_M-00002-of-00002.gguf", 20),
        ]);
        assert_eq!(f.quants.len(), 1);
        assert_eq!((f.quants[0].quant.as_str(), f.quants[0].size, f.quants[0].files.len()), ("IQ1_M", 30, 2));
        let mut e = ModelEntry { id: "unsloth/Qwen-Image-GGUF".into(), likes: 1, downloads: 1, created_at: None, tags: vec!["text-to-image".into()], base_models: None, pipeline_tag: None, trending_score: None };
        assert!(!is_text_model(&e));
        e.tags.clear();
        e.pipeline_tag = Some("image-text-to-text".into());
        assert!(is_text_model(&e));
        e.id = "nvidia/Nemotron-3-Diarization".into();
        e.pipeline_tag = None;
        assert!(!is_text_model(&e));
    }

    #[test]
    fn quant_names() {
        assert_eq!(quant_of("Qwen3-8B-Q4_K_M.gguf").unwrap().0, "Q4_K_M");
        assert_eq!(quant_of("Qwen3-8B-UD-IQ2_XXS.gguf").unwrap(), ("IQ2_XXS".into(), "UD-IQ2_XXS".into()));
        assert_eq!(quant_of("BF16/Qwen3.8-27B-BF16-00001-of-00002.gguf").unwrap().0, "BF16");
        assert_eq!(quant_of("model.Q8_0.gguf").unwrap().0, "Q8_0");
        assert_eq!(quant_of("readme.gguf"), None);
        assert_eq!(quant_of("gemma-4-E4B_q4_0-it.gguf").unwrap().0, "Q4_0");
        assert_eq!(quant_of("gemma-4-12b-it-qat-q4_0.gguf").unwrap().0, "Q4_0");
        assert_eq!(quant_of("Qwen3-8B-UD-Q4_K_XL.gguf").unwrap(), ("Q4_K_XL".into(), "UD-Q4_K_XL".into()));
        assert_eq!(quant_of("gpt-oss-20b-MXFP4.gguf").unwrap().0, "MXFP4");
        assert_eq!(quant_of("Llama-3.1-8B-Instruct-Q4_K_M-00001-of-00002.gguf").unwrap().0, "Q4_K_M");
        assert_eq!(quant_of("Qwen3-30B-A3B-Q8_0.gguf").unwrap().0, "Q8_0");
    }

    #[test]
    fn sources_get_makers_and_lose_labs() {
        let old = vec![
            Source { id: "kcppt".into(), name: "KoboldCpp templates".into(), builtin: true, on: true, comps: None, desc: String::new() },
            Source { id: "labs".into(), name: "Labs' own uploads".into(), builtin: true, on: true, comps: Some(Components { official: true, finetune: false, unc: false }), desc: String::new() },
            Source { id: "bartowski".into(), name: "bartowski".into(), builtin: false, on: true, comps: Some(Components { official: true, finetune: true, unc: false }), desc: String::new() },
        ];
        let new = migrate_sources(old);
        let ids: Vec<&str> = new.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(&ids[..3], ["kcppt", "google", "Qwen"]);
        assert!(!ids.contains(&"labs") && ids.contains(&"nvidia") && ids.contains(&"bartowski"));
        assert!(new.iter().find(|s| s.id == "google").unwrap().on);
        assert_eq!(migrate_sources(new.clone()), new, "idempotent");
    }

    #[test]
    fn sort_orders() {
        let e = |id: &str, dl, likes, created: &str, tr| ModelEntry { id: id.into(), likes, downloads: dl, created_at: Some(created.into()), tags: vec![], base_models: None, pipeline_tag: None, trending_score: Some(tr) };
        let a = e("a/a", 10, 300, "2026-01-01", 5.0);
        let b = e("b/b", 900, 10, "2026-10-01", 1.0);
        assert!(Sort::Downloads.key(&b) > Sort::Downloads.key(&a));
        assert!(Sort::Likes.key(&a) > Sort::Likes.key(&b));
        assert!(Sort::Newest.key(&b) > Sort::Newest.key(&a));
        assert!(Sort::Trending.key(&a) > Sort::Trending.key(&b));
        let r = |id: &str| ModelEntry { id: id.into(), likes: 0, downloads: 0, created_at: None, tags: vec![], base_models: Some(BaseModels { relation: Some("quantized".into()), models: vec![BaseModelRef { id: "meta-llama/Llama-3.1-8B".into() }] }), pipeline_tag: None, trending_score: None };
        assert!(from_maker(&r("unsloth/Llama-3.1-8B-GGUF"), "meta-llama"));
        assert!(!from_maker(&r("unsloth/Llama-3.1-8B-GGUF"), "google"));
    }

    #[test]
    fn source_names() {
        assert!(valid_source_name("bartowski") && valid_source_name("TheDrummer") && valid_source_name("lmstudio-community"));
        assert!(!valid_source_name("a") && !valid_source_name("-bad") && !valid_source_name("a b"));
    }
}
