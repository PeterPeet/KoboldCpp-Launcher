pub mod commands;
pub mod config;
pub mod fit;
pub mod gguf;
pub mod hardware;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(commands::HardwareCache::default())
        .invoke_handler(tauri::generate_handler![
            commands::hardware_info,
            commands::read_gguf,
            commands::pick_quant,
            commands::command_line,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
