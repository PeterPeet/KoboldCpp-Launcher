pub mod applog;
pub mod binary;
pub mod commands;
pub mod config;
pub mod download;
pub mod fit;
pub mod gguf;
pub mod hardware;
pub mod hf;
pub mod library;
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
            let state = AppState::new(dir);
            // A KoboldCpp or Esobold left from a session that was killed instead of closed.
            let bins = [state.dirs.bin(), state.dirs.bin_esobold()];
            std::thread::spawn(move || {
                let n: usize = bins.iter().map(|b| server::kill_orphans(b)).sum();
                if n > 0 {
                    applog!("ended {n} leftover KoboldCpp process(es)");
                }
            });
            app.manage(state);
            app.manage(commands::Downloads::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            app_paths,
            hardware_info,
            read_gguf,
            read_logs,
            pick_quant,
            command_line,
            binary_status,
            check_update,
            launcher_update,
            install_binary,
            cancel_binary_install,
            set_esobold,
            set_binary_path,
            start_server,
            stop_server,
            server_status,
            server_logs,
            server_perf,
            templates,
            fits,
            fit_local,
            popular_models,
            search_models,
            model_families,
            repo_files,
            download_files,
            cancel_download,
            export_sources,
            import_sources,
            library_scan,
            add_library_folder,
            preview_folder,
            remove_library_folder,
            move_model,
            trash_model,
            split_usage,
            list_presets,
            save_preset,
            delete_preset,
            load_config,
            import_config,
            last_config,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Never leave KoboldCpp running when the launcher closes (window closed, Cmd+Q, quit).
            if let tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit = event {
                let state = app.state::<AppState>();
                if state.server.is_active() {
                    applog!("launcher closing: stopping KoboldCpp");
                }
                state.server.stop(std::time::Duration::from_secs(5));
            }
        });
}
