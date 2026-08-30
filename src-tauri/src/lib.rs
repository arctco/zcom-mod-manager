mod archives;
mod commands;
mod credentials;
mod database;
mod deployment;
mod diagnostics;
mod error;
mod load_order;
mod models;
mod mods;
mod nexus;
#[cfg(target_os = "linux")]
mod protocol;
mod retoc;
mod steam;
mod ue4ss;

use std::{collections::HashMap, path::PathBuf, sync::Mutex};
use tauri::{Emitter, Manager};
use tauri_plugin_deep_link::DeepLinkExt;

pub struct AppContext {
    data_dir: PathBuf,
    cache_dir: PathBuf,
    mods_dir: PathBuf,
    logs_dir: PathBuf,
    db_path: PathBuf,
    previews: Mutex<HashMap<String, models::StagedMod>>,
    previous_build_id: Option<String>,
    /// An `nxm://` link that launched this process. Emitting it during setup
    /// would be lost, because the interface has not subscribed yet, so it is
    /// held here until the interface collects it on mount.
    pending_nxm: Mutex<Option<String>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // A second launch is almost always the browser handing over an
        // nxm:// link. Forward it to the running window instead of opening
        // another copy of the manager.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
            if let Some(url) = argv.iter().find(|arg| arg.starts_with("nxm://")) {
                let _ = app.emit("zcom://nxm", url.clone());
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Links delivered while the application is already running, and on
            // Linux the link that started this process.
            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    if url.scheme() == "nxm" {
                        let _ = handle.emit("zcom://nxm", url.to_string());
                    }
                }
            });
            let data_dir = app.path().app_data_dir()?;
            let cache_dir = app.path().app_cache_dir()?;
            let mods_dir = data_dir.join("mods");
            let logs_dir = app.path().app_log_dir()?;
            for dir in [&data_dir, &cache_dir, &mods_dir, &logs_dir] {
                std::fs::create_dir_all(dir)?
            }
            let db_path = data_dir.join("zcom-mod-manager.sqlite3");
            let conn = database::open(&db_path)
                .map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))?;
            load_order::recover(&conn, &data_dir.join("load-order-operation.json"))
                .map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))?;
            let settings = database::settings(&conn)
                .map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))?;
            let detected = if let Some(path) = settings.game_path.filter(|p| !p.is_empty()) {
                steam::from_manual(std::path::Path::new(&path)).ok()
            } else {
                steam::discover().ok().flatten()
            };
            let current = detected.and_then(|g| g.steam_build_id);
            let stored = conn
                .query_row(
                    "SELECT value FROM settings WHERE key='last_game_build'",
                    [],
                    |r| r.get::<_, String>(0),
                )
                .ok();
            let previous_build_id = match (&stored, &current) {
                (Some(old), Some(now)) if old != now => Some(old.clone()),
                _ => None,
            };
            if let Some(current) = current {
                let _ = database::set_setting(&conn, "last_game_build", &current);
            }
            app.manage(AppContext {
                data_dir,
                cache_dir,
                mods_dir,
                logs_dir,
                db_path,
                previews: Mutex::new(HashMap::new()),
                previous_build_id,
                pending_nxm: Mutex::new(std::env::args().find(|arg| arg.starts_with("nxm://"))),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_dashboard,
            commands::list_mods,
            commands::get_load_order_state,
            commands::preview_load_order,
            commands::apply_load_order,
            commands::inspect_mod,
            commands::install_mod,
            commands::set_mod_enabled,
            commands::uninstall_mod,
            commands::verify_mod,
            commands::install_ue4ss,
            commands::get_links,
            commands::check_for_updates,
            commands::nexus_status,
            commands::set_nexus_key,
            commands::clear_nexus_key,
            commands::set_nxm_handler,
            commands::nexus_download,
            commands::take_pending_nxm,
            commands::run_diagnostics,
            commands::diagnostic_report,
            commands::get_settings,
            commands::save_settings,
            commands::set_game_path,
            commands::open_managed_path,
            commands::launch_game
        ])
        .run(tauri::generate_context!())
        .expect("error while running ZCOM Mod Manager");
}
