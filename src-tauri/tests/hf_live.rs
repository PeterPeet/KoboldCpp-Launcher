//! Live Hugging Face checks (network). Run: cargo test --test hf_live -- --ignored --nocapture

use kobold_launcher_lib::hf::{self, client::Hf, templates, RankOptions};
use kobold_launcher_lib::{download, fit};

const GB: u64 = 1 << 30;

#[tokio::test]
#[ignore]
async fn templates_and_popular_list() {
    let http = download::client();
    let dir = tempfile::tempdir().unwrap();
    let h = Hf { http: &http, cache: dir.path().to_path_buf() };

    let t = templates::list(&h).await;
    println!("{} templates", t.len());
    for x in t.iter().take(6) {
        println!("  {:<22} {:?} {:<14} {:>6.1} GB  {:?}", x.id, x.tier, x.title, x.size.unwrap_or(0) as f64 / GB as f64, x.parts);
    }
    assert!(t.iter().any(|x| x.id == "MidSpec-Chatbot" && x.size.is_some_and(|s| s > 3 * GB)));

    let sources = hf::default_sources();
    let mut entries = h.by_author("unsloth", 40).await.unwrap();
    entries.extend(h.trending(100).await.unwrap());
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    let r = hf::rank(&entries, &sources, RankOptions { official_only: true, show_uncensored: false }, now);
    println!("{} ranked, {} dupes merged", r.items.len(), r.dupes);
    // Pretend: 16 GB Apple Silicon (Metal budget ~10.7 GB)
    for m in r.items.iter().take(8) {
        let files = h.files(&m.id).await.unwrap();
        let pick = fit::pick_quant(&files.sizes(), 16384, 10 * GB + GB * 7 / 10, 16 * GB, true);
        println!("  {:<40} {:>5} score {:.2}  pick {:?}", m.id, m.params.clone().unwrap_or_default(), m.score, pick.map(|p| (p.quant, p.size / (GB / 10))));
    }
    assert!(!r.items.is_empty());
}
