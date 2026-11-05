//! KoboldCpp's ready-made templates (`hf.co/koboldcpp/newbie-templates`, `popular-templates`).

use super::client::{DAY, Hf, API};
use crate::config::Kcpps;
use crate::hardware::Tier;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const COLLECTIONS: &[&str] = &["newbie-templates", "popular-templates"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    /// File name without extension, e.g. "MidSpec-Chatbot".
    pub id: String,
    pub collection: String,
    pub url: String,
    pub tier: Option<Tier>,
    /// "Chatbot", "Coding agent", "Images", …
    pub title: String,
    pub parts: Vec<String>,
    /// Total download size of all files in it (bytes), when known.
    pub size: Option<u64>,
    pub context: Option<i64>,
    pub config: Map<String, Value>,
    /// Every file it downloads: (config key, URL, size if known).
    pub files: Vec<(String, String, Option<u64>)>,
}

#[derive(Deserialize)]
struct RepoInfo {
    #[serde(default)]
    siblings: Vec<Sibling>,
}

#[derive(Deserialize)]
struct Sibling {
    rfilename: String,
}

pub fn tier_of(id: &str) -> Option<Tier> {
    if id.starts_with("LowSpec") {
        Some(Tier::Low)
    } else if id.starts_with("MidSpec") {
        Some(Tier::Mid)
    } else if id.starts_with("HighSpec") {
        Some(Tier::High)
    } else {
        None
    }
}

pub fn title_of(id: &str) -> String {
    let use_part = id.split_once("Spec-").map(|(_, u)| u).unwrap_or(id);
    match use_part {
        "Chatbot" => "Chatbot".into(),
        "CodingAgent" => "Coding agent".into(),
        "ImageGen" => "Images".into(),
        "MusicGen" => "Music".into(),
        "VideoGen" => "Video".into(),
        other => other.replace(['-', '_'], " "),
    }
}

/// Model name from a URL's file name: ".../Qwen3VL-8B-Instruct-Q4_K_M.gguf" → "Qwen3VL-8B-Instruct".
fn model_label(url: &str) -> String {
    let f = crate::download::file_name_from_url(url);
    let stem = f.trim_end_matches(".gguf").trim_end_matches(".safetensors");
    match super::quant_of(&f) {
        Some((_, label)) => stem.trim_end_matches(&label).trim_end_matches(&label.to_ascii_lowercase()).trim_end_matches(['-', '_', '.']).to_string(),
        None => stem.to_string(),
    }
}

/// What a template brings, in plain words.
pub fn parts_of(cfg: &Kcpps) -> Vec<String> {
    let mut p = Vec::new();
    if let Some(m) = cfg.model() {
        p.push(model_label(&m));
    }
    let has = |k: &str| cfg.get_str(k).is_some();
    if has("mmproj") { p.push("Sees images".into()); }
    if has("sdmodel") { p.push("Image model".into()); }
    if has("whispermodel") { p.push("Voice in".into()); }
    if has("ttsmodel") { p.push("Voice out".into()); }
    if has("embeddingsmodel") { p.push("Search and memory".into()); }
    if has("musicllm") || has("musicdiffusion") { p.push("Music model".into()); }
    if has("draftmodel") { p.push("Draft model".into()); }
    if cfg.get_bool("websearch") { p.push("Web search".into()); }
    p
}

pub async fn list(hf: &Hf<'_>) -> Vec<Template> {
    let mut out = Vec::new();
    for c in COLLECTIONS {
        let Ok(info) = hf.get_json::<RepoInfo>(&format!("{API}/models/koboldcpp/{c}"), DAY).await else { continue };
        for s in info.siblings.iter().filter(|s| s.rfilename.ends_with(".kcppt")) {
            let url = format!("https://huggingface.co/koboldcpp/{c}/resolve/main/{}", s.rfilename);
            let Ok(text) = hf.get_text(&url, DAY).await else { continue };
            let Ok(cfg) = Kcpps::parse(&text) else { continue };
            let id = s.rfilename.trim_end_matches(".kcppt").to_string();
            let mut files = Vec::new();
            for (k, u) in cfg.urls() {
                let size = hf.size_of(&u).await;
                files.push((k, u, size));
            }
            let size = files.iter().map(|f| f.2).sum::<Option<u64>>();
            out.push(Template {
                tier: tier_of(&id),
                title: title_of(&id),
                parts: parts_of(&cfg),
                context: cfg.get_i64("contextsize"),
                collection: c.to_string(),
                url,
                size,
                config: cfg.map().clone(),
                files,
                id,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_parts() {
        assert_eq!(tier_of("MidSpec-Chatbot"), Some(Tier::Mid));
        assert_eq!(tier_of("gemma4-e4b"), None);
        assert_eq!(title_of("HighSpec-CodingAgent"), "Coding agent");
        assert_eq!(title_of("flux2-klein-4b"), "flux2 klein 4b");
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/MidSpec-Chatbot.kcppt");
        let cfg = Kcpps::load(&p).unwrap();
        assert_eq!(parts_of(&cfg), ["Qwen3VL-8B-Instruct", "Sees images", "Voice in", "Voice out", "Search and memory", "Web search"]);
    }
}
