//! Tauri commands: the only surface the web UI talks to.
//! Errors are returned as plain strings so the UI can show them directly.

use crate::binary::{self, Release, ReleaseInfo};
use crate::config::{self, Kcpps};
use crate::download::Cancel;
use crate::hardware::{self, HardwareInfo};
use crate::server::{ServerManager, ServerStatus};
use crate::store::{Dirs, InstalledBinary, Settings};
use crate::{download, fit, gguf};
use serde::Serialize;
use serde_json::{Map, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

pub type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

pub struct AppState {
    pub dirs: Dirs,
    pub settings: Mutex<Settings>,
    pub server: ServerManager,
    pub http: reqwest::Client,
    pub hardware: Mutex<Option<HardwareInfo>>,
    pub releases: Mutex<Option<Vec<Release>>>,
    pub binary_cancel: Mutex<Option<Cancel>>,
    /// Config of the current/last run, to restart after an update.
    pub last_run: Mutex<Option<Kcpps>>,
}

impl AppState {
    pub fn new(root: PathBuf) -> Self {
        let dirs = Dirs::new(root);
        let _ = dirs.ensure();
        let settings = Settings::load(&dirs.settings_file());
        Self {
            dirs,
            settings: Mutex::new(settings),
            server: ServerManager::default(),
            http: download::client(),
            hardware: Mutex::new(None),
            releases: Mutex::new(None),
            binary_cancel: Mutex::new(None),
            last_run: Mutex::new(None),
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    pub fn update_settings(&self, f: impl FnOnce(&mut Settings)) -> CmdResult<Settings> {
        let mut s = self.settings.lock().map_err(err)?;
        f(&mut s);
        s.save(&self.dirs.settings_file()).map_err(err)?;
        Ok(s.clone())
    }

    async fn hardware(&self) -> CmdResult<HardwareInfo> {
        if let Some(h) = self.hardware.lock().map_err(err)?.clone() {
            return Ok(h);
        }
        let info = tauri::async_runtime::spawn_blocking(hardware::detect).await.map_err(err)?;
        *self.hardware.lock().map_err(err)? = Some(info.clone());
        Ok(info)
    }
}

/* ---------- settings, hardware, files ---------- */

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings()
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: Settings) -> CmdResult<Settings> {
    state.update_settings(|s| *s = settings)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Paths {
    pub data: PathBuf,
    pub models: PathBuf,
    pub presets: PathBuf,
}

#[tauri::command]
pub fn app_paths(state: State<'_, AppState>) -> Paths {
    Paths { data: state.dirs.root.clone(), models: state.settings().models_dir(&state.dirs), presets: state.dirs.presets() }
}

#[tauri::command]
pub async fn hardware_info(state: State<'_, AppState>, refresh: Option<bool>) -> CmdResult<HardwareInfo> {
    if refresh.unwrap_or(false) {
        *state.hardware.lock().map_err(err)? = None;
    }
    state.hardware().await
}

#[tauri::command]
pub async fn read_gguf(path: PathBuf) -> CmdResult<gguf::GgufInfo> {
    tauri::async_runtime::spawn_blocking(move || gguf::read_file(&path)).await.map_err(err)?.map_err(err)
}

#[tauri::command]
pub fn pick_quant(quants: Vec<(String, u64)>, context: u32, vram: u64, ram: u64, unified: bool) -> Option<fit::QuantPick> {
    fit::pick_quant(&quants, context, vram, ram, unified)
}

#[tauri::command]
pub fn command_line(config: Map<String, Value>, program: Option<String>) -> String {
    config::cli::to_command(&Kcpps::from_map(config), program.as_deref().unwrap_or("koboldcpp"))
}

/* ---------- KoboldCpp binary and updates ---------- */

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinaryStatus {
    pub installed: Option<InstalledBinary>,
    /// The installed file is still there.
    pub present: bool,
    /// Release asset we would download for this PC (`None`: no official build for this platform).
    pub asset: Option<String>,
}

#[tauri::command]
pub async fn binary_status(state: State<'_, AppState>) -> CmdResult<BinaryStatus> {
    let hw = state.hardware().await.ok();
    let installed = state.settings().binary;
    Ok(BinaryStatus {
        present: installed.as_ref().is_some_and(|b| b.path.is_file()),
        installed,
        asset: binary::asset_name(std::env::consts::OS, std::env::consts::ARCH, hw.as_ref()).map(str::to_string),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: Option<String>,
    pub latest: Option<ReleaseInfo>,
    pub update_available: bool,
}

#[tauri::command]
pub async fn check_update(state: State<'_, AppState>) -> CmdResult<UpdateInfo> {
    let releases = binary::fetch_releases(&state.http).await.map_err(err)?;
    let hw = state.hardware().await.ok();
    let asset = binary::asset_name(std::env::consts::OS, std::env::consts::ARCH, hw.as_ref());
    let s = state.settings();
    let latest = binary::latest(&releases, s.include_prereleases).map(|r| binary::info(r, asset));
    let current = s.binary.as_ref().and_then(|b| b.version.clone());
    let update_available = match (&latest, &current) {
        (Some(l), Some(c)) => binary::is_newer(&l.tag, c),
        (Some(_), None) => s.binary.is_none(),
        _ => false,
    };
    *state.releases.lock().map_err(err)? = Some(releases);
    Ok(UpdateInfo { current, latest, update_available })
}

/// Download and install a release (newest by default). If KoboldCpp is running it is stopped
/// for the swap and started again with the same settings.
#[tauri::command]
pub async fn install_binary(app: AppHandle, state: State<'_, AppState>, tag: Option<String>) -> CmdResult<InstalledBinary> {
    let cached = state.releases.lock().map_err(err)?.clone();
    let releases = match cached {
        Some(r) => r,
        None => binary::fetch_releases(&state.http).await.map_err(err)?,
    };
    let s = state.settings();
    let release = match &tag {
        Some(t) => releases.iter().find(|r| &r.tag_name == t),
        None => binary::latest(&releases, s.include_prereleases),
    }
    .ok_or("No matching KoboldCpp release found")?
    .clone();
    let hw = state.hardware().await.ok();
    let asset = binary::asset_name(std::env::consts::OS, std::env::consts::ARCH, hw.as_ref()).ok_or("KoboldCpp has no official build for this platform")?;
    if !release.assets.iter().any(|a| a.name == asset) {
        return Err(format!("Release {} has no {asset} download", release.tag_name));
    }
    let cancel = Cancel::default();
    *state.binary_cancel.lock().map_err(err)? = Some(cancel.clone());
    let app2 = app.clone();
    let installed = binary::install(&state.http, &release, asset, &state.dirs.bin(), &cancel, move |p| {
        let _ = app2.emit("binary-progress", p);
    })
    .await
    .map_err(err)?;
    *state.binary_cancel.lock().map_err(err)? = None;

    let was_running = state.server.is_active();
    if was_running {
        let server = state.server.clone();
        tauri::async_runtime::spawn_blocking(move || server.stop(Duration::from_secs(10))).await.map_err(err)?;
    }
    state.update_settings(|s| s.binary = Some(installed.clone()))?;
    binary::prune_old(&state.dirs.bin(), &installed.path);
    if was_running && let Some(cfg) = state.last_run.lock().map_err(err)?.clone() {
        start(&app, &state, cfg)?;
    }
    Ok(installed)
}

#[tauri::command]
pub fn cancel_binary_install(state: State<'_, AppState>) {
    if let Some(c) = state.binary_cancel.lock().unwrap().as_ref() {
        c.cancel();
    }
}

/// Use a KoboldCpp file the user already has.
#[tauri::command]
pub fn set_binary_path(state: State<'_, AppState>, path: PathBuf) -> CmdResult<InstalledBinary> {
    if !path.is_file() {
        return Err("That file does not exist".into());
    }
    let b = InstalledBinary { path, version: None, asset: None };
    state.update_settings(|s| s.binary = Some(b.clone()))?;
    Ok(b)
}

/* ---------- server ---------- */

fn start(app: &AppHandle, state: &AppState, mut cfg: Kcpps) -> CmdResult<u16> {
    let s = state.settings();
    let bin = s.binary.as_ref().filter(|b| b.path.is_file()).ok_or("KoboldCpp is not installed yet")?;
    let models = s.models_dir(&state.dirs);
    std::fs::create_dir_all(&models).map_err(err)?;
    cfg.migrate();
    // Without any model KoboldCpp opens its own Tk launcher instead of serving.
    const MODEL_KEYS: &[&str] = &["model_param", "sdmodel", "whispermodel", "ttsmodel", "embeddingsmodel", "musicdiffusion", "musicllm", "mcpfile"];
    if !MODEL_KEYS.iter().any(|k| cfg.get_str(k).is_some()) && !cfg.get_bool("nomodel") {
        return Err("No model selected".into());
    }
    // Template downloads land in our models folder, not in the working directory.
    if cfg.get_str("downloaddir").is_none() {
        cfg.set("downloaddir", Value::String(models.to_string_lossy().into_owned()));
    }
    cfg.set("istemplate", Value::Bool(false));
    let port = cfg.port();
    let path = state.dirs.runs().join("last-run.kcpps");
    cfg.save(&path).map_err(err)?;
    let app2 = app.clone();
    state
        .server
        .start(&bin.path, &path, &models, port, Arc::new(move |ev| {
            let _ = app2.emit("server", ev);
        }))
        .map_err(err)?;
    *state.last_run.lock().map_err(err)? = Some(cfg);
    Ok(port)
}

#[tauri::command]
pub fn start_server(app: AppHandle, state: State<'_, AppState>, config: Map<String, Value>) -> CmdResult<u16> {
    start(&app, &state, Kcpps::from_map(config))
}

#[tauri::command]
pub async fn stop_server(state: State<'_, AppState>) -> CmdResult<()> {
    let server = state.server.clone();
    tauri::async_runtime::spawn_blocking(move || server.stop(Duration::from_secs(8))).await.map_err(err)
}

#[tauri::command]
pub fn server_status(state: State<'_, AppState>) -> ServerStatus {
    state.server.status()
}

#[tauri::command]
pub fn server_logs(state: State<'_, AppState>, last: Option<usize>) -> Vec<String> {
    state.server.logs(last.unwrap_or(500))
}

/// `/api/extra/perf` of the running server (speeds, queue, uptime).
#[tauri::command]
pub async fn server_perf(state: State<'_, AppState>) -> CmdResult<Value> {
    let ServerStatus::Running { port } = state.server.status() else { return Err("KoboldCpp is not running".into()) };
    let r = state.http.get(format!("http://127.0.0.1:{port}/api/extra/perf")).timeout(Duration::from_secs(3)).send().await.map_err(err)?;
    r.json().await.map_err(err)
}

/* ---------- Hugging Face, templates, downloads ---------- */

fn hf(state: &AppState) -> crate::hf::client::Hf<'_> {
    crate::hf::client::Hf { http: &state.http, cache: state.dirs.cache() }
}

#[tauri::command]
pub async fn templates(state: State<'_, AppState>) -> CmdResult<Vec<crate::hf::templates::Template>> {
    Ok(crate::hf::templates::list(&hf(&state)).await)
}

/// Fit for many (size, context) pairs at once.
#[tauri::command]
pub fn fits(items: Vec<(u64, u32)>, vram: u64, ram: u64, unified: bool) -> Vec<fit::Fit> {
    items.into_iter().map(|(size, ctx)| fit::fit_by_size(size, ctx, vram, ram, unified)).collect()
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Machine {
    pub vram: u64,
    pub ram: u64,
    pub unified: bool,
    pub context: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PopularItem {
    #[serde(flatten)]
    pub model: crate::hf::Ranked,
    pub pick: fit::QuantPick,
    /// Download URLs for the picked quant (several for split files).
    pub urls: Vec<String>,
    pub mmproj: Option<(String, u64)>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PopularResult {
    pub items: Vec<PopularItem>,
    pub too_big: usize,
    pub dupes: usize,
    /// Sources that could not be reached.
    pub errors: Vec<String>,
}

fn now_unix() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

#[tauri::command]
pub async fn popular_models(state: State<'_, AppState>, options: crate::hf::RankOptions, machine: Machine) -> CmdResult<PopularResult> {
    let h = hf(&state);
    let sources = state.settings().sources;
    let mut entries = Vec::new();
    let mut errors = Vec::new();
    for s in sources.iter().filter(|s| s.on && s.comps.is_some()) {
        let r = if s.id == "labs" { h.trending(100).await } else { h.by_author(&s.name, 40).await };
        match r {
            Ok(mut l) => entries.append(&mut l),
            Err(e) => errors.push(format!("{}: {e}", s.name)),
        }
    }
    let ranked = crate::hf::rank(&entries, &sources, options, now_unix());
    let top: Vec<crate::hf::Ranked> = ranked.items.into_iter().take(24).collect();
    let files = futures_util::future::join_all(top.iter().map(|m| h.files(&m.id))).await;
    let mut items = Vec::new();
    let mut too_big = 0;
    for (m, f) in top.into_iter().zip(files) {
        let Ok(f) = f else { continue };
        let Some(pick) = fit::pick_quant(&f.sizes(), machine.context, machine.vram, machine.ram, machine.unified) else {
            too_big += 1;
            continue;
        };
        let Some(q) = f.quant(&pick.quant) else { continue };
        let urls = q.files.iter().map(|p| crate::hf::resolve_url(&m.id, p)).collect();
        let mmproj = f.best_mmproj().map(|(p, s)| (crate::hf::resolve_url(&m.id, p), *s));
        items.push(PopularItem { model: m, pick, urls, mmproj });
    }
    Ok(PopularResult { items, too_big, dupes: ranked.dupes, errors })
}

#[tauri::command]
pub async fn search_models(state: State<'_, AppState>, query: String) -> CmdResult<Vec<crate::hf::Ranked>> {
    let entries = hf(&state).search(&query, 50).await.map_err(err)?;
    // Search shows everything that matches, from any uploader (no source filter).
    let now = now_unix();
    Ok(entries
        .iter()
        .map(|e| crate::hf::Ranked {
            id: e.id.clone(),
            name: crate::hf::display_name(e.repo_name()),
            author: e.author().into(),
            params: crate::hf::params_from_name(e.repo_name()),
            official: crate::hf::is_official(e),
            uncensored: crate::hf::is_uncensored(e),
            likes: e.likes,
            downloads: e.downloads,
            age_days: crate::hf::age_days(e.created_at.as_deref(), now),
            score: crate::hf::score(e, now),
            source: 0,
            base_model: e.base_model().map(str::to_string),
            tags: e.tags.clone(),
        })
        .collect())
}

#[tauri::command]
pub async fn repo_files(state: State<'_, AppState>, repo: String) -> CmdResult<crate::hf::RepoFiles> {
    hf(&state).files(&repo).await.map_err(err)
}

#[derive(serde::Deserialize)]
pub struct DlFile {
    pub url: String,
    pub size: Option<u64>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DlProgress {
    pub id: String,
    pub done: u64,
    pub total: Option<u64>,
    pub speed: u64,
    pub file: String,
}

#[derive(Default)]
pub struct Downloads(pub Mutex<std::collections::HashMap<String, Cancel>>);

/// Download files into the models folder (split models: pass the first part, all parts follow).
/// Returns url → local path for each requested URL. Progress: event `download-progress`.
#[tauri::command]
pub async fn download_files(app: AppHandle, state: State<'_, AppState>, downloads: State<'_, Downloads>, id: String, files: Vec<DlFile>) -> CmdResult<Vec<(String, String)>> {
    let models = state.settings().models_dir(&state.dirs);
    let cancel = Cancel::default();
    downloads.0.lock().map_err(err)?.insert(id.clone(), cancel.clone());
    // Expand split parts; sizes for extra parts come from HEAD.
    let h = hf(&state);
    let mut jobs: Vec<(String, String, Option<u64>)> = Vec::new(); // (requested url, part url, size)
    for f in &files {
        let parts = download::split_urls(&f.url);
        let single = parts.len() == 1;
        for p in parts {
            let size = if single { f.size } else { h.size_of(&p).await };
            jobs.push((f.url.clone(), p, size));
        }
    }
    for j in &mut jobs {
        if j.2.is_none() {
            j.2 = h.size_of(&j.1).await;
        }
    }
    let total: Option<u64> = jobs.iter().map(|j| j.2).sum();
    let mut base = 0u64;
    let mut out = Vec::new();
    let result: CmdResult<()> = async {
        for (req, url, size) in &jobs {
            let name = download::file_name_from_url(url);
            let dest = models.join(&name);
            let (app2, id2, name2) = (app.clone(), id.clone(), name.clone());
            let start = base;
            let path = download::download(&state.http, download::Request { url, dest: &dest, size: *size, sha256: None }, &cancel, move |p| {
                let _ = app2.emit("download-progress", DlProgress { id: id2.clone(), done: start + p.done, total, speed: p.speed, file: name2.clone() });
            })
            .await
            .map_err(err)?;
            base += size.unwrap_or_else(|| std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0));
            if req == url || crate::gguf::split_part(&name).is_some_and(|(p, _)| p == 1) {
                out.push((req.clone(), path.to_string_lossy().into_owned()));
            }
        }
        Ok(())
    }
    .await;
    downloads.0.lock().map_err(err)?.remove(&id);
    result.map(|_| out)
}

#[tauri::command]
pub fn cancel_download(downloads: State<'_, Downloads>, id: String) {
    if let Some(c) = downloads.0.lock().unwrap().get(&id) {
        c.cancel();
    }
}

#[tauri::command]
pub fn export_sources(state: State<'_, AppState>, path: PathBuf) -> CmdResult<()> {
    let json = serde_json::to_string_pretty(&state.settings().sources).map_err(err)?;
    crate::store::write_atomic(&path, json.as_bytes()).map_err(err)
}

#[tauri::command]
pub fn import_sources(state: State<'_, AppState>, path: PathBuf) -> CmdResult<Vec<crate::hf::Source>> {
    let text = std::fs::read_to_string(&path).map_err(err)?;
    let mut list: Vec<crate::hf::Source> = serde_json::from_str(&text).map_err(|e| format!("Not a sources file: {e}"))?;
    list.retain(|s| s.builtin || crate::hf::valid_source_name(&s.name));
    // Built-in sources always exist exactly once.
    for d in crate::hf::default_sources().into_iter().filter(|d| d.builtin) {
        if !list.iter().any(|s| s.id == d.id) {
            list.insert(0, d);
        }
    }
    let saved = state.update_settings(|s| s.sources = list)?;
    Ok(saved.sources)
}
