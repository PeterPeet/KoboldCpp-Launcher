//! End-to-end: real KoboldCpp release + a small real model. Downloads ~0.5 GB, so it is ignored
//! by default. Run with:
//!
//!     KL_E2E_DIR=/some/folder cargo test --test e2e -- --ignored --nocapture
//!
//! Files stay in KL_E2E_DIR so a second run skips the downloads.

use kobold_launcher_lib::config::{Backend, Kcpps};
use kobold_launcher_lib::server::{ServerManager, ServerStatus, local_get};
use kobold_launcher_lib::{binary, download, hardware};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

const MODEL_URL: &str = "https://huggingface.co/unsloth/Qwen3-0.6B-GGUF/resolve/main/Qwen3-0.6B-Q4_K_M.gguf";

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn real_koboldcpp_starts_and_answers() {
    let root = PathBuf::from(std::env::var("KL_E2E_DIR").expect("set KL_E2E_DIR"));
    std::fs::create_dir_all(&root).unwrap();
    let client = download::client();

    // 1. Binary for this machine, verified against GitHub's sha256.
    let hw = tokio::task::spawn_blocking(hardware::detect).await.unwrap();
    println!("hardware: {} GPUs, {} threads, backend {:?}", hw.gpus.len(), hw.default_threads, hw.suggested_backend);
    let asset = binary::asset_name(std::env::consts::OS, std::env::consts::ARCH, Some(&hw)).expect("platform has a build");
    let releases = binary::fetch_releases(&client, binary::Flavor::KoboldCpp).await.unwrap();
    let rel = binary::latest(&releases, false).unwrap();
    println!("installing {} {asset}", rel.tag_name);
    let mut last = 0;
    let bin = binary::install(&client, rel, asset, &root.join("bin"), &download::Cancel::default(), |p| {
        let pct = p.done * 100 / p.total.unwrap_or(1).max(1);
        if pct >= last + 20 { last = pct; println!("  binary {pct}%"); }
    }).await.unwrap();

    // 2. Small model.
    let model = root.join("models").join(download::file_name_from_url(MODEL_URL));
    download::download(&client, download::Request { url: MODEL_URL, dest: &model, size: None, sha256: None }, &download::Cancel::default(), |_| {}).await.unwrap();
    let info = kobold_launcher_lib::gguf::read_file(&model).unwrap();
    println!("model: {:?} {:?} layers={:?}", info.architecture, info.file_type, info.block_count);

    // 3. Config and start.
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let mut cfg = Kcpps::new();
    cfg.set_model(&model.to_string_lossy());
    cfg.set_port(port);
    cfg.set("contextsize", 2048.into());
    cfg.set("host", "127.0.0.1".into());
    if std::env::var("KL_E2E_CPU").is_ok() {
        cfg.set_backend(&Backend::Cpu);
    }
    let cfg_path = root.join("e2e.kcpps");
    cfg.save(&cfg_path).unwrap();
    let server = ServerManager::default();
    server.start(&bin.path, &cfg_path, &root, port, Arc::new(|ev| {
        if let kobold_launcher_lib::server::ServerEvent::Log { line } = ev { println!("  | {line}"); }
    })).unwrap();

    let end = Instant::now() + Duration::from_secs(300);
    let status = loop {
        let s = server.status();
        if !matches!(s, ServerStatus::Starting { .. }) || Instant::now() > end { break s; }
        tokio::time::sleep(Duration::from_millis(500)).await;
    };
    assert_eq!(status, ServerStatus::Running { port }, "logs:\n{}", server.logs(40).join("\n"));
    let version = local_get(port, "/api/extra/version", Duration::from_secs(5)).unwrap();
    println!("version: {version}");
    let perf = local_get(port, "/api/extra/perf", Duration::from_secs(5)).unwrap();
    println!("perf: {perf}");
    assert!(perf.contains("uptime"));

    let s2 = server.clone();
    tokio::task::spawn_blocking(move || s2.stop(Duration::from_secs(10))).await.unwrap();
    assert_eq!(server.status(), ServerStatus::Stopped);
}
