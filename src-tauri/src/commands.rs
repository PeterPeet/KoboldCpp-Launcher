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
    /// Releases from the last update check, for the program they belong to.
    pub releases: Mutex<Option<(binary::Flavor, Vec<Release>)>>,
    pub binary_cancel: Mutex<Option<Cancel>>,
    /// Config of the current/last run, to restart after an update.
    pub last_run: Mutex<Option<Kcpps>>,
}

impl AppState {
    pub fn new(root: PathBuf) -> Self {
        let dirs = Dirs::new(root);
        let _ = dirs.ensure();
        let mut settings = Settings::load(&dirs.settings_file());
        // A KoboldCpp in bin/ that settings don't know about (e.g. lost record): use the newest.
        if settings.binary.as_ref().is_none_or(|b| !b.path.is_file())
            && let Some(found) = binary::find_installed(&dirs.bin())
        {
            settings.binary = Some(found);
            let _ = settings.save(&dirs.settings_file());
        }
        if settings.esobold_binary.as_ref().is_none_or(|b| !b.path.is_file())
            && let Some(found) = binary::find_installed(&dirs.bin_esobold())
        {
            settings.esobold_binary = Some(found);
            let _ = settings.save(&dirs.settings_file());
        }
        crate::applog::init(&dirs.logs().join("launcher.log"));
        crate::applog!("launcher {} started, data folder {}", env!("CARGO_PKG_VERSION"), dirs.root.display());
        let server = ServerManager::default();
        server.set_log_path(dirs.logs().join("koboldcpp.log"));
        Self {
            dirs,
            settings: Mutex::new(settings),
            server,
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
pub fn save_settings(state: State<'_, AppState>, patch: SettingsPatch) -> CmdResult<Settings> {
    state.update_settings(|s| patch.apply(s))
}

/// The settings the UI may change. The install record, library folders and usage data are only
/// changed by their own commands, so a stale copy in the UI can never overwrite them.
#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub check_updates_on_start: Option<bool>,
    pub include_prereleases: Option<bool>,
    pub sources: Option<Vec<crate::hf::Source>>,
}

impl SettingsPatch {
    pub fn apply(self, s: &mut Settings) {
        if let Some(v) = self.check_updates_on_start {
            s.check_updates_on_start = v;
        }
        if let Some(v) = self.include_prereleases {
            s.include_prereleases = v;
        }
        if let Some(mut v) = self.sources {
            v.retain(|x| x.builtin || crate::hf::valid_source_name(&x.name));
            s.sources = v;
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Paths {
    pub data: PathBuf,
    pub models: PathBuf,
    pub presets: PathBuf,
    pub logs: PathBuf,
}

#[tauri::command]
pub fn app_paths(state: State<'_, AppState>) -> Paths {
    Paths { data: state.dirs.root.clone(), models: state.settings().models_dir(&state.dirs), presets: state.dirs.presets(), logs: state.dirs.logs() }
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
    let s = state.settings();
    let installed = s.active_binary().cloned();
    Ok(BinaryStatus {
        present: installed.as_ref().is_some_and(|b| b.path.is_file()),
        installed,
        asset: binary::asset_for(s.flavor(), std::env::consts::OS, std::env::consts::ARCH, hw.as_ref()).map(str::to_string),
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
    let s = state.settings();
    let flavor = s.flavor();
    let releases = binary::fetch_releases(&state.http, flavor).await.map_err(err)?;
    let hw = state.hardware().await.ok();
    let asset = binary::asset_for(flavor, std::env::consts::OS, std::env::consts::ARCH, hw.as_ref());
    let latest = binary::latest(&releases, s.include_prereleases).map(|r| binary::info(r, asset));
    let current = s.active_binary().and_then(|b| b.version.clone());
    let update_available = match (&latest, &current) {
        (Some(l), Some(c)) => binary::is_newer(&l.tag, c),
        (Some(_), None) => s.active_binary().is_none(),
        _ => false,
    };
    *state.releases.lock().map_err(err)? = Some((flavor, releases));
    Ok(UpdateInfo { current, latest, update_available })
}

/// Newest release of this launcher on GitHub (it doesn't update itself; the UI links to the page).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherUpdate {
    pub current: String,
    /// `None` when the repository has no published release yet.
    pub latest: Option<String>,
    pub url: Option<String>,
    pub update_available: bool,
}

#[tauri::command]
pub async fn launcher_update(state: State<'_, AppState>) -> CmdResult<LauncherUpdate> {
    let current = format!("v{}", env!("CARGO_PKG_VERSION"));
    let repo = env!("CARGO_PKG_REPOSITORY").trim_start_matches("https://github.com/");
    let resp = state
        .http
        .get(format!("https://api.github.com/repos/{repo}/releases/latest"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(err)?;
    if resp.status().as_u16() == 404 {
        return Ok(LauncherUpdate { current, latest: None, url: None, update_available: false });
    }
    if !resp.status().is_success() {
        return Err(format!("GitHub answered {}", resp.status()));
    }
    let r: Release = resp.json().await.map_err(err)?;
    let update_available = binary::is_newer(&r.tag_name, &current);
    Ok(LauncherUpdate { current, latest: Some(r.tag_name), url: r.html_url, update_available })
}

/// Download and install a release (newest by default). If KoboldCpp is running it is stopped
/// for the swap and started again with the same settings.
#[tauri::command]
pub async fn install_binary(app: AppHandle, state: State<'_, AppState>, tag: Option<String>) -> CmdResult<InstalledBinary> {
    let s = state.settings();
    let flavor = s.flavor();
    let cached = state.releases.lock().map_err(err)?.clone().filter(|(f, _)| *f == flavor);
    let releases = match cached {
        Some((_, r)) => r,
        None => binary::fetch_releases(&state.http, flavor).await.map_err(err)?,
    };
    let release = match &tag {
        Some(t) => releases.iter().find(|r| &r.tag_name == t),
        None => binary::latest(&releases, s.include_prereleases),
    }
    .ok_or_else(|| format!("No matching {} release found", flavor.name()))?
    .clone();
    let hw = state.hardware().await.ok();
    let asset = binary::asset_for(flavor, std::env::consts::OS, std::env::consts::ARCH, hw.as_ref()).ok_or_else(|| format!("{} has no build for this platform", flavor.name()))?;
    if !release.assets.iter().any(|a| a.name == asset) {
        return Err(format!("Release {} has no {asset} download", release.tag_name));
    }
    let cancel = Cancel::default();
    *state.binary_cancel.lock().map_err(err)? = Some(cancel.clone());
    let app2 = app.clone();
    crate::applog!("installing {} {} ({asset})", flavor.name(), release.tag_name);
    let installed = binary::install(&state.http, &release, asset, &state.dirs.bin_of(flavor), &cancel, move |p| {
        let _ = app2.emit("binary-progress", p);
    })
    .await
    .map_err(|e| {
        crate::applog!("install failed: {e}");
        err(e)
    })?;
    crate::applog!("installed {}", installed.path.display());
    *state.binary_cancel.lock().map_err(err)? = None;

    let was_running = state.server.is_active();
    if was_running {
        let server = state.server.clone();
        tauri::async_runtime::spawn_blocking(move || server.stop(Duration::from_secs(10))).await.map_err(err)?;
    }
    // Recorded for the program it belongs to, even if the mode was switched meanwhile.
    state.update_settings(|s| match flavor {
        binary::Flavor::KoboldCpp => s.binary = Some(installed.clone()),
        binary::Flavor::Esobold => s.esobold_binary = Some(installed.clone()),
    })?;
    binary::prune_old(&state.dirs.bin_of(flavor), &installed.path);
    if was_running && let Some(cfg) = state.last_run.lock().map_err(err)?.clone() {
        start(&app, &state, cfg)?;
    }
    Ok(installed)
}

/// Switch between KoboldCpp and Esobold (Advanced setup › Esobold). Not while a model runs:
/// the running program would no longer match the mode.
#[tauri::command]
pub fn set_esobold(state: State<'_, AppState>, on: bool) -> CmdResult<Settings> {
    if state.server.is_active() {
        return Err("Stop the running model first".into());
    }
    crate::applog!("Esobold mode {}", if on { "on" } else { "off" });
    // Releases fetched for the other program don't count any more.
    *state.releases.lock().map_err(err)? = None;
    state.update_settings(|s| s.esobold = on)
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
    // The launcher runs this file: only something that is called KoboldCpp (or Esobold).
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase();
    if !name.contains("koboldcpp") && !name.contains("esobold") {
        return Err("That doesn't look like a KoboldCpp program (its name should contain \"koboldcpp\")".into());
    }
    let b = InstalledBinary { path, version: None, asset: None };
    state.update_settings(|s| s.set_active_binary(Some(b.clone())))?;
    Ok(b)
}

/* ---------- server ---------- */

fn start(app: &AppHandle, state: &AppState, mut cfg: Kcpps) -> CmdResult<u16> {
    let s = state.settings();
    let bin = s.active_binary().filter(|b| b.path.is_file()).ok_or_else(|| format!("{} is not installed yet", s.flavor().name()))?;
    let models = s.models_dir(&state.dirs);
    std::fs::create_dir_all(&models).map_err(err)?;
    cfg.migrate();
    // Last line of defence, whatever the config came from (see config::safety).
    let removed = crate::config::safety::strip_never(&mut cfg);
    if !removed.is_empty() {
        crate::applog!("options removed before start: {}", removed.join(", "));
    }
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
    if let Some(m) = cfg.model().filter(|m| !config::kcpps::is_url(m)) {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let _ = state.update_settings(|s| {
            s.last_used.insert(m, now);
        });
    }
    let path = state.dirs.runs().join("last-run.kcpps");
    cfg.save(&path).map_err(err)?;
    crate::applog!("start {} with {} (model {}, port {port})", bin.path.display(), path.display(), cfg.model().unwrap_or_else(|| "-".into()));
    if !crate::server::port_free(port) {
        for f in [binary::Flavor::KoboldCpp, binary::Flavor::Esobold] {
            crate::server::reclaim_port(port, &state.dirs.bin_of(f));
        }
    }
    let app2 = app.clone();
    state
        .server
        .start(&bin.path, &path, &models, port, Arc::new(move |ev| {
            if let crate::server::ServerEvent::Status { status } = &ev {
                crate::applog!("KoboldCpp: {status:?}");
            }
            let _ = app2.emit("server", ev);
        }))
        .map_err(|e| {
            crate::applog!("start failed: {e}");
            err(e)
        })?;
    *state.last_run.lock().map_err(err)? = Some(cfg);
    Ok(port)
}

#[tauri::command]
pub fn start_server(app: AppHandle, state: State<'_, AppState>, config: Map<String, Value>) -> CmdResult<u16> {
    start(&app, &state, Kcpps::from_map(config))
}

#[tauri::command]
pub async fn stop_server(state: State<'_, AppState>) -> CmdResult<()> {
    crate::applog!("stop requested");
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
    let perf: Value = r.json().await.map_err(err)?;
    // Remember how fast this model really is here (answers of at least 16 tokens).
    let speed = perf.get("last_eval_speed").and_then(Value::as_f64).filter(|s| *s > 0.0);
    let count = perf.get("last_token_count").and_then(Value::as_f64).unwrap_or(0.0);
    let model = state.last_run.lock().map_err(err)?.as_ref().and_then(|c| c.model()).filter(|m| !config::kcpps::is_url(m));
    if let (Some(speed), Some(model)) = (speed, model)
        && count >= 16.0
        && state.settings().speeds.get(&model).is_none_or(|old| (old - speed).abs() > 0.05)
    {
        let rounded = (speed * 10.0).round() / 10.0;
        state.update_settings(|s| {
            s.speeds.insert(model, rounded);
        })?;
    }
    Ok(perf)
}

/* ---------- Hugging Face, templates, downloads ---------- */

fn hf(state: &AppState) -> crate::hf::client::Hf<'_> {
    crate::hf::client::Hf { http: &state.http, cache: state.dirs.cache() }
}

#[tauri::command]
pub async fn templates(state: State<'_, AppState>) -> CmdResult<Vec<crate::hf::templates::Template>> {
    let models = state.settings().models_dir(&state.dirs);
    let mut list = crate::hf::templates::list(&hf(&state)).await;
    for t in &mut list {
        t.downloaded = !t.files.is_empty() && t.files.iter().all(|(_, url, size)| local_file(&models, url, *size).is_some());
    }
    Ok(list)
}

/// Where a URL's file is (or would be) on disk if it is already complete: the per-repo folder,
/// or the models folder itself for files downloaded by older versions.
pub fn local_file(models: &std::path::Path, url: &str, size: Option<u64>) -> Option<PathBuf> {
    let name = download::file_name_from_url(url);
    [models.join(download::repo_subdir(url)).join(&name), models.join(&name)]
        .into_iter()
        .find(|p| std::fs::metadata(p).is_ok_and(|m| m.is_file() && size.is_none_or(|s| m.len() == s)))
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
    /// Bandwidths for the speed estimate.
    #[serde(default)]
    pub speed: fit::SpeedHw,
}

/// Share of the model on the GPU for the speed estimate.
fn gpu_share(fit: fit::Fit, size: u64, vram: u64) -> f64 {
    match fit {
        fit::Fit::Full => 1.0,
        fit::Fit::Cpu | fit::Fit::None => 0.0,
        fit::Fit::Partial => (vram as f64 * 0.85 / size.max(1) as f64).clamp(0.0, 1.0),
    }
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
    /// All files are already on this computer: the button says Start.
    pub downloaded: bool,
    /// The model file(s) are on this computer.
    pub model_downloaded: bool,
    /// The vision file is on this computer (false when there is none).
    pub mmproj_downloaded: bool,
    /// How this model can use MTP (faster answers); `None`: not possible or unknown.
    pub mtp: Option<crate::hf::Mtp>,
    /// The MTP head file is on this computer (false when there is none to download).
    pub mtp_downloaded: bool,
    /// Estimated generation speed on this PC (tokens/s); `None` when a bandwidth it needs is unknown.
    pub tps: Option<f64>,
    /// Speed KoboldCpp reported for this file here, if it has run before.
    pub measured: Option<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PopularResult {
    pub items: Vec<PopularItem>,
    pub too_big: usize,
    pub dupes: usize,
    /// Sources that could not be reached.
    pub errors: Vec<String>,
    /// Search only: matches left out because their uploader's source is off (uploader, count).
    pub hidden: Vec<(String, usize)>,
}

fn now_unix() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Candidates from all enabled sources in the chosen order; with a family, searched by its terms.
async fn gather(h: &crate::hf::client::Hf<'_>, sources: &[crate::hf::Source], terms: &[&str], sort: crate::hf::Sort) -> (Vec<crate::hf::ModelEntry>, Vec<String>) {
    let terms: Vec<Option<&str>> = if terms.is_empty() { vec![None] } else { terms.iter().map(|t| Some(*t)).collect() };
    let pairs: Vec<(&crate::hf::Source, Option<&str>)> = sources.iter().filter(|s| s.on && s.comps.is_some()).flat_map(|s| terms.iter().map(move |t| (s, *t))).collect();
    let jobs = pairs.into_iter().map(|(s, term)| async move {
        let r = match term {
            None => h.by_author(&s.name, sort, 40).await,
            Some(t) => h.by_author_search(&s.name, t, sort, 30).await,
        };
        (s.name.clone(), r)
    });
    let mut entries = Vec::new();
    let mut errors = Vec::new();
    for (name, r) in futures_util::future::join_all(jobs).await {
        match r {
            Ok(mut l) => entries.append(&mut l),
            Err(e) => errors.push(format!("{name}: {e}")),
        }
    }
    (entries, errors)
}

/// MTP for a repo that ships MTP files. KoboldCpp uses MTP layers inside the model file with
/// `--usemtp`; a model without them can use a separate MTP head as `--draftmodel` (it detects the
/// head and switches to MTP decoding). Both are checked from the files' first megabyte.
async fn mtp_for(h: &crate::hf::client::Hf<'_>, f: &crate::hf::RepoFiles, repo: &str, model_url: &str) -> Option<crate::hf::Mtp> {
    let (path, size) = f.best_mtp()?;
    if h.mtp_layers(model_url).await? > 0 {
        return Some(crate::hf::Mtp { built_in: true, url: None, size: None });
    }
    let url = crate::hf::resolve_url(repo, path);
    (h.mtp_layers(&url).await? > 0).then_some(crate::hf::Mtp { built_in: false, url: Some(url), size: Some(*size) })
}

/// Pick a quant for each candidate (in order) until `want` fit; counts the ones too big.
async fn with_picks(h: &crate::hf::client::Hf<'_>, models: &std::path::Path, speeds: &std::collections::HashMap<String, f64>, cands: Vec<crate::hf::Ranked>, machine: &Machine, want: usize) -> (Vec<PopularItem>, usize) {
    let files = futures_util::future::join_all(cands.iter().map(|m| h.files(&m.id))).await;
    let mut items: Vec<(PopularItem, crate::hf::RepoFiles)> = Vec::new();
    let mut too_big = 0;
    for (m, f) in cands.into_iter().zip(files) {
        let Ok(f) = f else { continue };
        let Some(pick) = fit::pick_quant(&f.sizes(), machine.context, machine.vram, machine.ram, machine.unified) else {
            too_big += 1;
            continue;
        };
        let Some(q) = f.quant(&pick.quant) else { continue };
        let urls: Vec<String> = q.files.iter().map(|p| crate::hf::resolve_url(&m.id, p)).collect();
        let mmproj = f.best_mmproj().map(|(p, s)| (crate::hf::resolve_url(&m.id, p), *s));
        let single = urls.len() == 1;
        let model_downloaded = urls.iter().all(|u| local_file(models, u, single.then_some(pick.size)).is_some());
        let mmproj_downloaded = mmproj.as_ref().is_some_and(|(u, s)| local_file(models, u, Some(*s)).is_some());
        let downloaded = model_downloaded && (mmproj.is_none() || mmproj_downloaded);
        let tps = fit::estimate_tps(pick.size, fit::active_ratio(&m.id), gpu_share(pick.fit, pick.size, machine.vram), &machine.speed);
        let measured = local_file(models, &urls[0], single.then_some(pick.size)).and_then(|p| speeds.get(&p.to_string_lossy().to_string()).copied());
        items.push((PopularItem { model: m, pick, urls, mmproj, downloaded, model_downloaded, mmproj_downloaded, mtp: None, mtp_downloaded: false, tps, measured }, f));
    }
    // Keep ranking order, but a normal quant beats a squeezed fallback.
    items.sort_by_key(|i| i.0.pick.fallback);
    items.truncate(want);
    // MTP only for the rows shown (it reads the start of model files).
    let mtps = futures_util::future::join_all(items.iter().map(|(i, f)| mtp_for(h, f, &i.model.id, &i.urls[0]))).await;
    let items = items
        .into_iter()
        .zip(mtps)
        .map(|((mut i, _), mtp)| {
            i.mtp_downloaded = mtp.as_ref().and_then(|x| x.url.as_ref().zip(x.size)).is_some_and(|(u, s)| local_file(models, u, Some(s)).is_some());
            i.mtp = mtp;
            i
        })
        .collect();
    (items, too_big)
}

/// `family`: None = "Top": for each maker whose source is on, its best model that fits this PC
/// (from the maker's own account, or a quant of its model). Else that family's whole list.
#[tauri::command]
pub async fn popular_models(state: State<'_, AppState>, options: crate::hf::RankOptions, machine: Machine, family: Option<String>) -> CmdResult<PopularResult> {
    let h = hf(&state);
    let sources = state.settings().sources;
    let models = state.settings().models_dir(&state.dirs);
    let speeds = state.settings().speeds;
    let now = now_unix();
    if let Some(fam) = family {
        let f = crate::hf::FAMILIES.iter().find(|f| f.id == fam).ok_or("Unknown model family")?;
        let (entries, errors) = gather(&h, &sources, f.terms, options.sort).await;
        let ranked = crate::hf::rank(&entries, &sources, options, now);
        let cands: Vec<_> = ranked.items.into_iter().filter(|m| m.family.as_deref() == Some(fam.as_str())).take(30).collect();
        let (items, too_big) = with_picks(&h, &models, &speeds, cands, &machine, 20).await;
        return Ok(PopularResult { items, too_big, dupes: ranked.dupes, errors, hidden: vec![] });
    }
    let maker_on = |m: &str| sources.iter().any(|s| s.on && s.name.eq_ignore_ascii_case(m));
    let per_family = crate::hf::FAMILIES.iter().filter(|f| maker_on(f.maker)).map(|f| {
        let (h, sources, machine, models, speeds) = (&h, &sources, &machine, &models, &speeds);
        async move {
            let (entries, errors) = gather(h, sources, f.terms, options.sort).await;
            let ranked = crate::hf::rank(&entries, sources, options, now);
            let is_maker = |m: &crate::hf::Ranked| {
                m.author.eq_ignore_ascii_case(f.maker) || m.base_model.as_deref().and_then(|b| b.split('/').next()).is_some_and(|a| a.eq_ignore_ascii_case(f.maker))
            };
            let mut cands: Vec<_> = ranked.items.into_iter().filter(|m| m.family.as_deref() == Some(f.id) && is_maker(m)).collect();
            // The maker's own uploads first (in the chosen order), quants of its models after.
            cands.sort_by_key(|m| m.kind != crate::hf::Kind::Official);
            cands.truncate(12);
            let (items, _) = with_picks(h, models, speeds, cands, machine, 1).await;
            (items, errors)
        }
    });
    let mut items = Vec::new();
    let mut errors = Vec::new();
    for (mut i, mut e) in futures_util::future::join_all(per_family).await {
        items.append(&mut i);
        errors.append(&mut e);
    }
    errors.sort();
    errors.dedup();
    Ok(PopularResult { items, too_big: 0, dupes: 0, errors, hidden: vec![] })
}

/// Hugging Face search, answered like the popular list: best fitting quant, speed, options.
#[tauri::command]
pub async fn search_models(state: State<'_, AppState>, query: String, machine: Machine) -> CmdResult<PopularResult> {
    let h = hf(&state);
    let models = state.settings().models_dir(&state.dirs);
    let speeds = state.settings().speeds;
    let sources = state.settings().sources;
    // More matches than shown: some belong to uploaders whose source is off.
    let entries = h.search(&query, 100).await.map_err(err)?;
    let now = now_unix();
    // Search follows the sources: only uploaders whose source is on (the makers count as sources).
    let on = |author: &str| sources.iter().any(|s| s.on && s.comps.is_some() && s.name.eq_ignore_ascii_case(author));
    let text: Vec<&crate::hf::ModelEntry> = entries.iter().filter(|e| crate::hf::is_text_model(e)).collect();
    let mut hidden: Vec<(String, usize)> = Vec::new();
    for e in text.iter().filter(|e| !on(e.author())) {
        match hidden.iter_mut().find(|(a, _)| a == e.author()) {
            Some(h) => h.1 += 1,
            None => hidden.push((e.author().to_string(), 1)),
        }
    }
    hidden.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    // Most downloaded first.
    let cands: Vec<crate::hf::Ranked> = text
        .into_iter()
        .filter(|e| on(e.author()))
        .map(|e| {
            let kind = crate::hf::kind_of(e);
            crate::hf::Ranked {
                id: e.id.clone(),
                name: crate::hf::display_name(e.repo_name()),
                author: e.author().into(),
                params: crate::hf::params_from_name(e.repo_name()),
                kind,
                official: kind == crate::hf::Kind::Official,
                family: crate::hf::family_of(&e.id).map(str::to_string),
                uncensored: crate::hf::is_uncensored(e),
                likes: e.likes,
                downloads: e.downloads,
                age_days: crate::hf::age_days(e.created_at.as_deref(), now),
                score: crate::hf::score(e, now),
                source: 0,
                base_model: e.base_model().map(str::to_string),
                tags: e.tags.clone(),
            }
        })
        .collect();
    let (mut items, too_big) = with_picks(&h, &models, &speeds, cands, &machine, 30).await;
    // with_picks prefers normal quants over fallbacks; keep the search order (downloads) otherwise.
    items.sort_by(|a, b| a.pick.fallback.cmp(&b.pick.fallback).then(b.model.downloads.cmp(&a.model.downloads)));
    Ok(PopularResult { items, too_big, dupes: 0, errors: vec![], hidden })
}

/// Model families for the start screen filter row: (id, label, maker account).
#[tauri::command]
pub fn model_families() -> Vec<(String, String, String)> {
    crate::hf::FAMILIES.iter().map(|f| (f.id.to_string(), f.label.to_string(), f.maker.to_string())).collect()
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
    crate::applog!("download {id}: {} file(s), {} bytes", jobs.len(), total.map_or("unknown".into(), |t| t.to_string()));
    let mut base = 0u64;
    let mut out = Vec::new();
    let result: CmdResult<()> = async {
        for (req, url, size) in &jobs {
            let name = download::file_name_from_url(url);
            let dest = local_file(&models, url, *size).unwrap_or_else(|| models.join(download::repo_subdir(url)).join(&name));
            let (app2, id2, name2) = (app.clone(), id.clone(), name.clone());
            let start = base;
            let path = download::download_retry(&state.http, download::Request { url, dest: &dest, size: *size, sha256: None }, &cancel, move |p| {
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
    match &result {
        Ok(_) => crate::applog!("download {id}: done"),
        Err(e) => crate::applog!("download {id}: {e}"),
    }
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
    if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) {
        return Err("Save the sources as a .json file".into());
    }
    let json = serde_json::to_string_pretty(&state.settings().sources).map_err(err)?;
    crate::store::write_atomic(&path, json.as_bytes()).map_err(err)
}

#[tauri::command]
pub fn import_sources(state: State<'_, AppState>, path: PathBuf) -> CmdResult<Vec<crate::hf::Source>> {
    if std::fs::metadata(&path).map_err(err)?.len() > 1 << 20 {
        return Err("That file is too big for a sources file".into());
    }
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

/* ---------- library ---------- */

/// Folders the library scans: our downloads folder (always recursive, `<uploader>/<repo>/`)
/// and the user's folders with their own setting.
fn library_roots(state: &AppState) -> Vec<(PathBuf, bool)> {
    let s = state.settings();
    let mut roots = vec![(s.models_dir(&state.dirs), true)];
    roots.extend(s.library_folders.iter().map(|f| (f.path.clone(), f.recursive)));
    let mut seen = std::collections::HashSet::new();
    roots.retain(|r| seen.insert(r.0.clone()));
    roots
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryView {
    pub models: Vec<crate::library::LocalModel>,
    pub drives: Vec<crate::library::Drive>,
    pub folders: Vec<crate::store::LibraryFolder>,
    pub models_dir: PathBuf,
}

#[tauri::command]
pub async fn library_scan(state: State<'_, AppState>) -> CmdResult<LibraryView> {
    let roots = library_roots(&state);
    let used = state.settings().last_used;
    // The architecture check compares against KoboldCpp's version; Esobold numbers its own.
    let s = state.settings();
    let installed = if s.esobold { None } else { s.binary.and_then(|b| b.version) };
    let models_dir = state.settings().models_dir(&state.dirs);
    let folders = roots.iter().map(|(path, recursive)| crate::store::LibraryFolder { path: path.clone(), recursive: *recursive }).collect();
    let (models, drives) = tauri::async_runtime::spawn_blocking(move || crate::library::scan(&roots, &used, installed.as_deref())).await.map_err(err)?;
    Ok(LibraryView { models, drives, folders, models_dir })
}

#[tauri::command]
pub fn add_library_folder(state: State<'_, AppState>, path: PathBuf, recursive: Option<bool>) -> CmdResult<Settings> {
    if !path.is_dir() {
        return Err("That folder does not exist".into());
    }
    let recursive = recursive.unwrap_or(true);
    crate::applog!("library folder added: {} (recursive: {recursive})", path.display());
    state.update_settings(|s| match s.library_folders.iter_mut().find(|f| f.path == path) {
        Some(f) => f.recursive = recursive,
        None => s.library_folders.push(crate::store::LibraryFolder { path, recursive }),
    })
}

/// How many models a folder would add (for the add-folder dialog).
#[tauri::command]
pub async fn preview_folder(path: PathBuf, recursive: bool) -> CmdResult<crate::library::FolderPreview> {
    tauri::async_runtime::spawn_blocking(move || crate::library::preview(&path, recursive)).await.map_err(err)
}

#[tauri::command]
pub fn remove_library_folder(state: State<'_, AppState>, path: PathBuf) -> CmdResult<Settings> {
    state.update_settings(|s| s.library_folders.retain(|f| f.path != path))
}

#[tauri::command]
pub async fn move_model(state: State<'_, AppState>, path: PathBuf, dest: PathBuf) -> CmdResult<PathBuf> {
    if !crate::library::is_managed(&path, &library_roots(&state).into_iter().map(|r| r.0).collect::<Vec<_>>()) {
        return Err("Only models in your library folders can be moved".into());
    }
    tauri::async_runtime::spawn_blocking(move || crate::library::move_model(&path, &dest)).await.map_err(err)?.map_err(err)
}

/// Move a model (all parts) to the Trash.
#[tauri::command]
pub fn trash_model(state: State<'_, AppState>, path: PathBuf) -> CmdResult<()> {
    if !crate::library::is_managed(&path, &library_roots(&state).into_iter().map(|r| r.0).collect::<Vec<_>>()) {
        return Err("Only models in your library folders can be removed".into());
    }
    crate::library::trash_model(&path).map_err(err)
}

#[tauri::command]
pub fn split_usage(size: u64, context: u32, shares: Vec<f64>, main_gpu: usize) -> Vec<u64> {
    fit::split_usage(&fit::ModelShape { size, ..Default::default() }, context, &shares, main_gpu)
}

/* ---------- presets and configs ---------- */

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetInfo {
    pub name: String,
    pub path: PathBuf,
    pub modified: Option<u64>,
}

fn preset_path(state: &AppState, name: &str) -> CmdResult<PathBuf> {
    let clean: String = name.chars().filter(|c| c.is_alphanumeric() || " -_.()".contains(*c)).collect::<String>().trim().to_string();
    if clean.is_empty() || clean.starts_with('.') {
        return Err("Give the preset a name".into());
    }
    Ok(state.dirs.presets().join(format!("{clean}.kcpps")))
}

#[tauri::command]
pub fn list_presets(state: State<'_, AppState>) -> Vec<PresetInfo> {
    let mut out: Vec<PresetInfo> = std::fs::read_dir(state.dirs.presets())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "kcpps" || x == "kcppt"))
        .map(|e| PresetInfo {
            name: e.path().file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            modified: e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()),
            path: e.path(),
        })
        .collect();
    out.sort_by_key(|p| std::cmp::Reverse(p.modified));
    out
}

#[tauri::command]
pub fn save_preset(state: State<'_, AppState>, name: String, config: Map<String, Value>) -> CmdResult<PathBuf> {
    let path = preset_path(&state, &name)?;
    let mut k = Kcpps::from_map(config);
    k.set("istemplate", Value::Bool(false));
    k.save(&path).map_err(err)?;
    Ok(path)
}

#[tauri::command]
pub fn delete_preset(state: State<'_, AppState>, name: String) -> CmdResult<()> {
    std::fs::remove_file(preset_path(&state, &name)?).map_err(err)
}

/// Load one of the user's presets (only files in the presets folder).
#[tauri::command]
pub fn load_config(state: State<'_, AppState>, path: PathBuf) -> CmdResult<Map<String, Value>> {
    let dir = state.dirs.presets().canonicalize().map_err(err)?;
    let p = path.canonicalize().map_err(err)?;
    if !p.starts_with(&dir) {
        return Err("Not a preset".into());
    }
    let mut k = Kcpps::load(&p).map_err(err)?;
    k.migrate();
    crate::config::safety::strip_never(&mut k);
    Ok(k.map().clone())
}

#[derive(Serialize)]
pub struct Imported {
    pub config: Map<String, Value>,
    /// Options left out because they are unsafe in a file from elsewhere (see config::safety).
    pub removed: Vec<String>,
}

/// Import a .kcpps/.kcppt from anywhere: legacy keys migrated, unsafe options removed.
#[tauri::command]
pub fn import_config(path: PathBuf) -> CmdResult<Imported> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if !matches!(ext.as_str(), "kcpps" | "kcppt" | "json") {
        return Err("Choose a .kcpps or .kcppt file".into());
    }
    if std::fs::metadata(&path).map_err(err)?.len() > 4 << 20 {
        return Err("That file is too big for a settings file".into());
    }
    let mut k = Kcpps::load(&path).map_err(err)?;
    k.migrate();
    let removed = crate::config::safety::strip_untrusted(&mut k);
    if !removed.is_empty() {
        crate::applog!("import {}: removed {}", path.display(), removed.join(", "));
    }
    Ok(Imported { config: k.map().clone(), removed })
}

/// Settings of the last run, to fill the advanced page.
#[tauri::command]
pub fn last_config(state: State<'_, AppState>) -> Option<Map<String, Value>> {
    Kcpps::load(&state.dirs.runs().join("last-run.kcpps")).ok().map(|k| k.map().clone())
}

/* ---------- logs ---------- */

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Logs {
    pub dir: PathBuf,
    pub koboldcpp: String,
    pub launcher: String,
}

/// The last part of both logs (KoboldCpp's output and the launcher's own).
#[tauri::command]
pub fn read_logs(state: State<'_, AppState>) -> Logs {
    let dir = state.dirs.logs();
    Logs {
        koboldcpp: crate::applog::tail(&dir.join("koboldcpp.log"), 256 * 1024),
        launcher: crate::applog::tail(&dir.join("launcher.log"), 256 * 1024),
        dir,
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalFitInput {
    pub size: u64,
    pub layers: Option<u32>,
    pub head_count_kv: Option<u32>,
    pub kv_len: Option<u32>,
    /// Size of the vision file loaded with it, if any.
    pub mmproj: Option<u64>,
    /// File path (for measured speeds) and name (MoE detection).
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalFit {
    pub fit: fit::Fit,
    /// Layers KoboldCpp would put on the GPU (0 when unknown or CPU only).
    pub gpu_layers: u32,
    /// Estimated memory while running (bytes) at the given context.
    pub memory: u64,
    /// Estimated generation speed (tokens/s); `None` when a bandwidth it needs is unknown.
    pub tps: Option<f64>,
    /// Speed KoboldCpp reported for this file, if it has run here.
    pub measured: Option<f64>,
}

/// Fit of local models using their GGUF layout (KoboldCpp's layer estimate).
#[tauri::command]
pub fn fit_local(state: State<'_, AppState>, items: Vec<LocalFitInput>, machine: Machine) -> Vec<LocalFit> {
    let s = fit::Settings { context: machine.context, ..Default::default() };
    let speeds = state.settings().speeds;
    items
        .into_iter()
        .map(|i| {
            let shape = fit::ModelShape { size: i.size, layers: i.layers, head_count_kv: i.head_count_kv, kv_len: i.kv_len };
            let extras = fit::Extras { mmproj: i.mmproj.unwrap_or(0), ..Default::default() };
            let (fit, gpu_layers) = fit::fit_by_layers(&shape, &extras, &s, machine.vram, 0, machine.ram, machine.unified);
            let share = match (fit, i.layers) {
                (fit::Fit::Partial, Some(l)) if l > 0 => (gpu_layers as f64 / (l + 1) as f64).min(1.0),
                _ => gpu_share(fit, i.size, machine.vram),
            };
            let name = i.name.as_deref().unwrap_or("");
            LocalFit {
                fit,
                gpu_layers,
                memory: fit::runtime_memory(&shape, &extras, &s),
                tps: fit::estimate_tps(i.size, fit::active_ratio(name), share, &machine.speed),
                measured: i.path.as_ref().and_then(|p| speeds.get(p).copied()),
            }
        })
        .collect()
}
