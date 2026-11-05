//! Hugging Face requests with a small on-disk cache (24 h by default).

use super::{ModelEntry, RepoFiles, TreeEntry};
use crate::download::DownloadError;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub const API: &str = "https://huggingface.co/api";
pub const DAY: Duration = Duration::from_secs(24 * 3600);
const EXPAND: &str = "expand[]=likes&expand[]=downloads&expand[]=createdAt&expand[]=baseModels&expand[]=tags&expand[]=pipeline_tag";

pub struct Hf<'a> {
    pub http: &'a reqwest::Client,
    pub cache: PathBuf,
}

fn cache_file(dir: &Path, key: &str) -> PathBuf {
    let h = Sha256::digest(key.as_bytes());
    dir.join(format!("hf-{}.json", h.iter().take(12).map(|b| format!("{b:02x}")).collect::<String>()))
}

impl Hf<'_> {
    /// GET text with cache. On network failure an expired cache entry is still used.
    pub async fn get_text(&self, url: &str, ttl: Duration) -> Result<String, DownloadError> {
        let file = cache_file(&self.cache, url);
        let fresh = std::fs::metadata(&file).and_then(|m| m.modified()).ok().and_then(|t| SystemTime::now().duration_since(t).ok()).is_some_and(|age| age < ttl);
        if fresh && let Ok(t) = std::fs::read_to_string(&file) {
            return Ok(t);
        }
        let result = async {
            let r = self.http.get(url).timeout(Duration::from_secs(30)).send().await?;
            if !r.status().is_success() {
                return Err(DownloadError::Status(r.status().as_u16()));
            }
            Ok(r.text().await?)
        }
        .await;
        match result {
            Ok(text) => {
                let _ = crate::store::write_atomic(&file, text.as_bytes());
                Ok(text)
            }
            Err(e) => std::fs::read_to_string(&file).map_err(|_| e),
        }
    }

    pub async fn get_json<T: DeserializeOwned>(&self, url: &str, ttl: Duration) -> Result<T, DownloadError> {
        let t = self.get_text(url, ttl).await?;
        serde_json::from_str(&t).map_err(|e| DownloadError::Io(std::io::Error::other(e)))
    }

    /// GGUF repos of one uploader, trending first.
    pub async fn by_author(&self, author: &str, limit: u32) -> Result<Vec<ModelEntry>, DownloadError> {
        self.get_json(&format!("{API}/models?author={author}&filter=gguf&sort=trendingScore&limit={limit}&{EXPAND}"), DAY).await
    }

    /// One uploader's GGUF repos matching a search term (family lists).
    pub async fn by_author_search(&self, author: &str, term: &str, limit: u32) -> Result<Vec<ModelEntry>, DownloadError> {
        self.get_json(&format!("{API}/models?author={author}&search={term}&filter=gguf&sort=trendingScore&limit={limit}&{EXPAND}"), DAY).await
    }

    /// Trending GGUF repos from everyone matching a search term.
    pub async fn trending_search(&self, term: &str, limit: u32) -> Result<Vec<ModelEntry>, DownloadError> {
        self.get_json(&format!("{API}/models?search={term}&filter=gguf&sort=trendingScore&limit={limit}&{EXPAND}"), DAY).await
    }

    /// Trending GGUF repos from everyone (used for the "labs" source).
    pub async fn trending(&self, limit: u32) -> Result<Vec<ModelEntry>, DownloadError> {
        self.get_json(&format!("{API}/models?filter=gguf&sort=trendingScore&limit={limit}&{EXPAND}"), DAY).await
    }

    /// Free-text search over GGUF repos, most downloaded first.
    pub async fn search(&self, query: &str, limit: u32) -> Result<Vec<ModelEntry>, DownloadError> {
        let q: String = query.chars().filter(|c| c.is_alphanumeric() || " -_.".contains(*c)).collect::<String>().trim().replace(' ', "+");
        self.get_json(&format!("{API}/models?search={q}&filter=gguf&sort=downloads&limit={limit}&{EXPAND}"), Duration::from_secs(3600)).await
    }

    pub async fn files(&self, repo: &str) -> Result<RepoFiles, DownloadError> {
        let tree: Vec<TreeEntry> = self.get_json(&format!("{API}/models/{repo}/tree/main?recursive=true"), DAY).await?;
        Ok(super::parse_tree(&tree))
    }

    /// Size of a file behind a URL (HEAD, following redirects). Cached for a week.
    pub async fn size_of(&self, url: &str) -> Option<u64> {
        let key = format!("size:{url}");
        let file = cache_file(&self.cache, &key);
        if let Some(n) = std::fs::read_to_string(&file).ok().and_then(|t| t.trim().parse().ok()) {
            return Some(n);
        }
        let r = self.http.head(crate::download::normalize_url(url)).timeout(Duration::from_secs(20)).send().await.ok()?;
        let n = r.headers().get("x-linked-size").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok()).or_else(|| r.content_length())?;
        let _ = crate::store::write_atomic(&file, n.to_string().as_bytes());
        Some(n)
    }
}
