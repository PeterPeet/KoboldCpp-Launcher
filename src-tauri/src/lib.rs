pub mod binary;
pub mod commands;
pub mod config;
pub mod download;
pub mod fit;
pub mod gguf;
pub mod hardware;
pub mod hf;
pub mod server;
pub mod store;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use commands::*;
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            app.manage(AppState::new(dir));
            app.manage(commands::Downloads::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            app_paths,
            hardware_info,
            read_gguf,
            pick_quant,
            command_line,
            binary_status,
            check_update,
            install_binary,
            cancel_binary_install,
            set_binary_path,
            start_server,
            stop_server,
            server_status,
            server_logs,
            server_perf,
            templates,
            fits,
            popular_models,
            search_models,
            repo_files,
            download_files,
            cancel_download,
            export_sources,
            import_sources,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Never leave KoboldCpp running in the background when the launcher quits.
            if let tauri::RunEvent::Exit = event {
                app.state::<AppState>().server.stop(std::time::Duration::from_secs(5));
            }
        });
}
