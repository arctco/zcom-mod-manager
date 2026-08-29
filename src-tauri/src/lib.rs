mod archives;
mod commands;
mod database;
mod deployment;
mod diagnostics;
mod error;
mod models;
mod mods;
mod retoc;
mod steam;
mod ue4ss;

use std::{collections::HashMap, path::PathBuf, sync::Mutex};
use tauri::Manager;

pub struct AppContext {
    data_dir: PathBuf,
    cache_dir: PathBuf,
    mods_dir: PathBuf,
    logs_dir: PathBuf,
    db_path: PathBuf,
    previews: Mutex<HashMap<String, models::StagedMod>>,
    previous_build_id: Option<String>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
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
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_dashboard,
            commands::list_mods,
            commands::inspect_mod,
            commands::install_mod,
            commands::set_mod_enabled,
            commands::uninstall_mod,
            commands::verify_mod,
            commands::install_ue4ss,
            commands::get_links,
            commands::run_diagnostics,
            commands::diagnostic_report,
            commands::get_settings,
            commands::save_settings,
            commands::set_game_path,
            commands::managed_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running ZCOM Mod Manager");
}
