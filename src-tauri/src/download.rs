//! Resumable HTTP downloads with progress and cancel.
//!
//! Data goes to `<file>.part`; an interrupted download continues with an HTTP Range request.
//! Only a complete file is renamed to its final name, so a half file is never mistaken for a model.

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("server answered {0}")]
    Status(u16),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("download cancelled")]
    Cancelled,
    #[error("file is incomplete ({got} of {want} bytes)")]
    Incomplete { got: u64, want: u64 },
    #[error("checksum does not match")]
    Checksum,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub done: u64,
    pub total: Option<u64>,
    /// Bytes per second over the last few seconds.
    pub speed: u64,
}

/// Shared cancel flag.
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn cancel(&self) { self.0.store(true, Ordering::SeqCst) }
    pub fn is_cancelled(&self) -> bool { self.0.load(Ordering::SeqCst) }
}

/// Hugging Face page links (`/blob/`) to file links (`/resolve/`), and drop `?download=true`.
pub fn normalize_url(url: &str) -> String {
    let u = url.replace("/blob/main/", "/resolve/main/");
    match u.split_once('?') {
        Some((base, "download=true")) => base.to_string(),
        _ => u,
    }
}

/// File name from a URL (last path segment, without query or fragment).
pub fn file_name_from_url(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    path.rsplit('/').next().filter(|s| !s.is_empty()).unwrap_or("download").to_string()
}

/// Folder for a download inside the models folder: `<owner>/<repo>` for Hugging Face links, so a
/// model and its vision file stay together and same-named files from different repos can't clash.
pub fn repo_subdir(url: &str) -> PathBuf {
    let rest = url.split_once("huggingface.co/").map(|(_, r)| r).unwrap_or("");
    let mut parts = rest.split('/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(owner), Some(repo), Some("resolve" | "blob")) if !owner.is_empty() && !repo.is_empty() && !owner.contains("..") && !repo.contains("..") => {
            PathBuf::from(owner).join(repo)
        }
        _ => PathBuf::new(),
    }
}

pub fn part_path(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

/// HTTP client for all downloads and API calls. Uses the operating system's certificate store.
/// `KL_CA_BUNDLE=/path/to/bundle.pem` trusts only that bundle instead (corporate proxies,
/// sandboxes without keychain access).
pub fn client() -> reqwest::Client {
    let mut b = reqwest::Client::builder()
        .user_agent(concat!("KoboldCpp-Launcher/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(60));
    if let Some(certs) = std::env::var_os("KL_CA_BUNDLE").and_then(|p| std::fs::read(p).ok()).and_then(|pem| reqwest::Certificate::from_pem_bundle(&pem).ok()) {
        b = b.tls_certs_only(certs);
    }
    b.build().expect("http client builds")
}

#[derive(Clone, Copy)]
pub struct Request<'a> {
    pub url: &'a str,
    pub dest: &'a Path,
    /// Expected size, if known (checked at the end).
    pub size: Option<u64>,
    /// Expected SHA-256 as lowercase hex, if known.
    pub sha256: Option<&'a str>,
}

/// Download `req.url` to `req.dest`. Skips the download if `dest` already exists with the
/// expected size. Calls `progress` at most ~5 times a second.
pub async fn download(client: &reqwest::Client, req: Request<'_>, cancel: &Cancel, mut progress: impl FnMut(Progress)) -> Result<PathBuf, DownloadError> {
    let dest = req.dest;
    if let (Ok(meta), Some(want)) = (std::fs::metadata(dest), req.size)
        && meta.len() == want
    {
        return Ok(dest.to_path_buf());
    }
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let part = part_path(dest);
    let mut have = tokio::fs::metadata(&part).await.map(|m| m.len()).unwrap_or(0);
    if req.size.is_some_and(|want| have > want) {
        have = 0;
    }

    let url = normalize_url(req.url);
    let mut rq = client.get(&url);
    if have > 0 {
        rq = rq.header(reqwest::header::RANGE, format!("bytes={have}-"));
    }
    let resp = rq.send().await?;
    let status = resp.status();
    let resumed = status == reqwest::StatusCode::PARTIAL_CONTENT;
    if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE && req.size == Some(have) {
        // .part already complete
    } else if !status.is_success() {
        return Err(DownloadError::Status(status.as_u16()));
    }
    if !resumed {
        have = 0;
    }
    let total = req.size.or_else(|| resp.content_length().map(|l| l + have));

    let mut file = tokio::fs::OpenOptions::new().create(true).write(true).append(resumed).truncate(!resumed).open(&part).await?;
    let mut done = have;
    let mut stream = resp.bytes_stream();
    let mut last_report = Instant::now() - Duration::from_secs(1);
    let mut window: (Instant, u64) = (Instant::now(), done);
    let mut speed = 0u64;
    while let Some(chunk) = stream.next().await {
        if cancel.is_cancelled() {
            file.flush().await?;
            return Err(DownloadError::Cancelled);
        }
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        done += chunk.len() as u64;
        let now = Instant::now();
        if now.duration_since(window.0) >= Duration::from_secs(2) {
            speed = ((done - window.1) as f64 / now.duration_since(window.0).as_secs_f64()) as u64;
            window = (now, done);
        }
        if now.duration_since(last_report) >= Duration::from_millis(200) {
            last_report = now;
            progress(Progress { done, total, speed });
        }
    }
    file.flush().await?;
    drop(file);
    if let Some(want) = total
        && done != want
    {
        return Err(DownloadError::Incomplete { got: done, want });
    }
    if let Some(expected) = req.sha256 {
        let part2 = part.clone();
        let actual = tokio::task::spawn_blocking(move || sha256_file(&part2)).await.map_err(std::io::Error::other)??;
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(DownloadError::Checksum);
        }
    }
    progress(Progress { done, total, speed });
    tokio::fs::rename(&part, dest).await?;
    Ok(dest.to_path_buf())
}

/// `download` with up to 4 attempts for network trouble (each resumes where the last stopped).
pub async fn download_retry(client: &reqwest::Client, req: Request<'_>, cancel: &Cancel, mut progress: impl FnMut(Progress)) -> Result<PathBuf, DownloadError> {
    let mut attempt = 0u32;
    loop {
        match download(client, req, cancel, &mut progress).await {
            Err(e @ (DownloadError::Http(_) | DownloadError::Incomplete { .. } | DownloadError::Status(500..))) if attempt < 3 && !cancel.is_cancelled() => {
                attempt += 1;
                eprintln!("download of {} interrupted ({e}), retry {attempt}", req.url);
                tokio::time::sleep(Duration::from_secs(2u64 << attempt)).await;
            }
            other => return other,
        }
    }
}

/// URLs of all parts of a split model, given the first part's URL (`-00001-of-00003.gguf`).
pub fn split_urls(url: &str) -> Vec<String> {
    let name = file_name_from_url(url);
    match crate::gguf::split_part(&name) {
        Some((1, total)) if (2..=999).contains(&total) => {
            let tag = format!("-00001-of-{total:05}");
            (1..=total).map(|i| url.replace(&tag, &format!("-{i:05}-of-{total:05}"))).collect()
        }
        _ => vec![url.to_string()],
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut f = std::io::BufReader::with_capacity(1 << 20, std::fs::File::open(path)?);
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    #[test]
    fn sha256_of_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("abc");
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(sha256_file(&p).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn repo_folders() {
        assert_eq!(repo_subdir("https://huggingface.co/unsloth/Qwen3.5-0.8B-GGUF/resolve/main/mmproj-F16.gguf"), PathBuf::from("unsloth/Qwen3.5-0.8B-GGUF"));
        assert_eq!(repo_subdir("https://huggingface.co/koboldcpp/tts/resolve/main/Kokoro.gguf"), PathBuf::from("koboldcpp/tts"));
        assert_eq!(repo_subdir("https://example.com/x.gguf"), PathBuf::new());
        assert_eq!(repo_subdir("https://huggingface.co/../x/resolve/main/a.gguf"), PathBuf::new());
    }

    #[test]
    fn url_helpers() {
        assert_eq!(normalize_url("https://huggingface.co/a/b/blob/main/x.gguf?download=true"), "https://huggingface.co/a/b/resolve/main/x.gguf");
        assert_eq!(file_name_from_url("https://x/y/model.gguf?download=true#z"), "model.gguf");
        let parts = split_urls("https://h/r/resolve/main/m-00001-of-00003.gguf");
        assert_eq!(parts.len(), 3);
        assert!(parts[2].ends_with("m-00003-of-00003.gguf"));
        assert_eq!(split_urls("https://h/m.gguf").len(), 1);
    }

    /// Tiny HTTP server that serves `body`, honours Range, and can cut the first response short.
    fn serve(body: Vec<u8>, cut_first_at: Option<usize>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for (n, stream) in listener.incoming().enumerate() {
                let mut s = stream.unwrap();
                let mut reader = BufReader::new(s.try_clone().unwrap());
                let mut start = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" { break; }
                    if let Some(r) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                        start = r.trim().trim_end_matches('-').parse().unwrap();
                    }
                }
                let slice = &body[start..];
                let head = if start > 0 {
                    format!("HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n", slice.len(), start, body.len() - 1, body.len())
                } else {
                    format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", slice.len())
                };
                let _ = s.write_all(head.as_bytes());
                let send = if n == 0 { cut_first_at.map_or(slice, |c| &slice[..c]) } else { slice };
                let _ = s.write_all(send);
            }
        });
        format!("http://{addr}/file.bin")
    }

    #[tokio::test]
    async fn downloads_and_resumes_after_interruption() {
        let body: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let url = serve(body.clone(), Some(100_000));
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("file.bin");
        let c = client();
        let first = download(&c, Request { url: &url, dest: &dest, size: Some(body.len() as u64), sha256: None }, &Cancel::default(), |_| {}).await;
        assert!(first.is_err(), "first attempt is cut short");
        assert!(!dest.exists());
        let have = std::fs::metadata(part_path(&dest)).unwrap().len();
        assert!(have > 0 && have < body.len() as u64);
        let expected = hex(&Sha256::digest(&body));
        let mut reports = 0;
        download(&c, Request { url: &url, dest: &dest, size: Some(body.len() as u64), sha256: Some(&expected) }, &Cancel::default(), |_| reports += 1).await.unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), body);
        assert!(!part_path(&dest).exists());
        assert!(reports >= 1);
        // Already there: no request needed.
        download(&c, Request { url: "http://127.0.0.1:9/nothing", dest: &dest, size: Some(body.len() as u64), sha256: None }, &Cancel::default(), |_| {}).await.unwrap();
    }

    #[tokio::test]
    async fn retry_resumes_automatically() {
        let body: Vec<u8> = (0..200_000u32).map(|i| (i % 13) as u8).collect();
        let url = serve(body.clone(), Some(50_000));
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("r.bin");
        download_retry(&client(), Request { url: &url, dest: &dest, size: Some(body.len() as u64), sha256: None }, &Cancel::default(), |_| {}).await.unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), body);
    }

    #[tokio::test]
    async fn cancel_stops_download() {
        let body = vec![7u8; 5_000_000];
        let url = serve(body, None);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("big.bin");
        let cancel = Cancel::default();
        cancel.cancel();
        let r = download(&client(), Request { url: &url, dest: &dest, size: None, sha256: None }, &cancel, |_| {}).await;
        assert!(matches!(r, Err(DownloadError::Cancelled)));
        assert!(!dest.exists());
    }
}
