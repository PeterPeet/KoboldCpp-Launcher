//! Tauri commands: the only surface the web UI talks to.
//! Errors are returned as plain strings so the UI can show them directly.

use crate::{config, fit, gguf, hardware};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;

pub type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Cached hardware info; detection runs external tools, so do it once and on request.
#[derive(Default)]
pub struct HardwareCache(pub Mutex<Option<hardware::HardwareInfo>>);

#[tauri::command]
pub async fn hardware_info(cache: State<'_, HardwareCache>, refresh: Option<bool>) -> CmdResult<hardware::HardwareInfo> {
    if !refresh.unwrap_or(false)
        && let Some(h) = cache.0.lock().map_err(err)?.clone()
    {
        return Ok(h);
    }
    let info = tauri::async_runtime::spawn_blocking(hardware::detect).await.map_err(err)?;
    *cache.0.lock().map_err(err)? = Some(info.clone());
    Ok(info)
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
pub fn command_line(config: serde_json::Map<String, serde_json::Value>, program: Option<String>) -> String {
    config::cli::to_command(&config::Kcpps::from_map(config), program.as_deref().unwrap_or("koboldcpp"))
}
