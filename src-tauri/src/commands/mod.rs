use crate::{
    database, deployment, diagnostics,
    error::{AppError, Result},
    models::{
        AppSettings, Dashboard, DiagnosticReport, GameInfo, ModPreview, ModSummary, StagedMod,
    },
    mods, retoc, steam, ue4ss, AppContext,
};
use std::{
    path::{Path, PathBuf},
    sync::MutexGuard,
};
use tauri::{Manager, State};

fn connection(ctx: &AppContext) -> Result<rusqlite::Connection> {
    database::open(&ctx.db_path)
}
fn game(ctx: &AppContext) -> Result<GameInfo> {
    let conn = connection(ctx)?;
    let settings = database::settings(&conn)?;
    if let Some(path) = settings.game_path.filter(|p| !p.is_empty()) {
        steam::from_manual(Path::new(&path))
    } else {
        Ok(steam::discover()?.unwrap_or_default())
    }
}
fn require_game(ctx: &AppContext) -> Result<(GameInfo, PathBuf)> {
    let info = game(ctx)?;
    let path = info
        .path
        .as_ref()
        .map(PathBuf::from)
        .ok_or(AppError::GameNotFound)?;
    Ok((info, path))
}
fn tool(ctx: &AppContext) -> Result<crate::models::ToolInfo> {
    let conn = connection(ctx)?;
    let settings = database::settings(&conn)?;
    Ok(retoc::find(settings.retoc_path.as_deref()))
}
fn previews(
    ctx: &AppContext,
) -> Result<MutexGuard<'_, std::collections::HashMap<String, StagedMod>>> {
    ctx.previews
        .lock()
        .map_err(|_| AppError::Other("preview state lock was poisoned".into()))
}

#[tauri::command]
pub fn get_dashboard(ctx: State<'_, AppContext>) -> Result<Dashboard> {
    let conn = connection(&ctx)?;
    let game = game(&ctx)?;
    let (installed_mods, enabled_mods) = database::counts(&conn)?;
    let conflict_count = database::conflict_count(&conn)?;
    let game_path = game.path.as_deref().map(Path::new);
    let compat = game.compat_data_path.as_deref().map(Path::new);
    let ue4ss = ue4ss::detect(game_path, compat);
    let retoc = tool(&ctx)?;
    Ok(Dashboard {
        game,
        installed_mods,
        enabled_mods,
        conflict_count,
        ue4ss,
        previous_build_id: ctx.previous_build_id.clone(),
        data_directory: ctx.data_dir.display().to_string(),
        retoc,
    })
}
#[tauri::command]
pub fn list_mods(ctx: State<'_, AppContext>) -> Result<Vec<ModSummary>> {
    database::list_mods(&connection(&ctx)?)
}

#[tauri::command]
pub fn inspect_mod(path: String, ctx: State<'_, AppContext>) -> Result<ModPreview> {
    let conn = connection(&ctx)?;
    let settings = database::settings(&conn)?;
    let game = game(&ctx)?;
    let ue = ue4ss::detect(
        game.path.as_deref().map(Path::new),
        game.compat_data_path.as_deref().map(Path::new),
    );
    let tool = retoc::find(settings.retoc_path.as_deref());
    let (staged, preview) = mods::scan(
        Path::new(&path),
        &ctx.cache_dir,
        &tool,
        game.steam_build_id.as_deref(),
        ue.healthy,
        settings.advanced_package_names,
    )?;
    log(
        &ctx,
        "info",
        "mod_inspected",
        &format!("type={} files={}", preview.mod_type, preview.files.len()),
    );
    previews(&ctx)?.insert(staged.staging_id.clone(), staged);
    Ok(preview)
}

#[tauri::command]
pub fn install_mod(staging_id: String, ctx: State<'_, AppContext>) -> Result<ModSummary> {
    let staged = previews(&ctx)?
        .remove(&staging_id)
        .ok_or(AppError::PreviewExpired)?;
    if staged.mod_type == "iostore" && staged.verification != "passed" {
        return Err(AppError::RetocVerificationFailed(
            staged
                .verification_details
                .unwrap_or_else(|| "verification did not pass".into()),
        ));
    }
    let (game_info, game_path) = require_game(&ctx)?;
    let mut conn = connection(&ctx)?;
    let result = deployment::install(
        &mut conn,
        &ctx.mods_dir,
        &game_path,
        &staged,
        game_info.steam_build_id,
    );
    let _ = std::fs::remove_dir_all(&staged.staging_root);
    let summary = result?;
    log(
        &ctx,
        "info",
        "mod_installed",
        &format!("mod_id={} type={}", summary.id, summary.mod_type),
    );
    Ok(summary)
}

#[tauri::command]
pub fn set_mod_enabled(id: String, enabled: bool, ctx: State<'_, AppContext>) -> Result<()> {
    let (_, game_path) = require_game(&ctx)?;
    let conn = connection(&ctx)?;
    deployment::set_enabled(&conn, &ctx.mods_dir, &game_path, &id, enabled)?;
    log(
        &ctx,
        "info",
        if enabled {
            "mod_enabled"
        } else {
            "mod_disabled"
        },
        &format!("mod_id={id}"),
    );
    Ok(())
}

#[tauri::command]
pub fn uninstall_mod(id: String, force: bool, ctx: State<'_, AppContext>) -> Result<()> {
    let game_path = game(&ctx)?.path.map(PathBuf::from);
    let conn = connection(&ctx)?;
    deployment::uninstall(&conn, &ctx.mods_dir, &id, force, game_path.as_deref())?;
    log(&ctx, "info", "mod_uninstalled", &format!("mod_id={id}"));
    Ok(())
}

#[tauri::command]
pub fn verify_mod(id: String, ctx: State<'_, AppContext>) -> Result<String> {
    deployment::verify(&connection(&ctx)?, &id)
}

/// Installs a user-downloaded UE4SS package. Nothing is fetched from the
/// network: the user supplies the archive, and it is staged in the same
/// sandbox that mod archives use before any file reaches the game folder.
#[tauri::command]
pub fn install_ue4ss(
    path: String,
    ctx: State<'_, AppContext>,
) -> Result<crate::models::Ue4ssInstallReport> {
    let (_, game_path) = require_game(&ctx)?;
    let report = ue4ss::install_from(Path::new(&path), &game_path, &ctx.cache_dir)?;
    log(
        &ctx,
        "info",
        "ue4ss_installed",
        &format!(
            "files={} preserved={}",
            report.installed,
            report.preserved.len()
        ),
    );
    Ok(report)
}

/// External resources the interface links to. Kept on this side so the
/// download target has a single definition shared with diagnostics.
#[tauri::command]
pub fn get_links() -> Links {
    Links {
        ue4ss_download: ue4ss::DOWNLOAD_URL.into(),
        nexus_game: "https://www.nexusmods.com/games/starwarszerocompany/mods".into(),
        project: "https://github.com/zcom-modding/zcom-mod-manager".into(),
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Links {
    pub ue4ss_download: String,
    pub nexus_game: String,
    pub project: String,
}
#[tauri::command]
pub fn run_diagnostics(ctx: State<'_, AppContext>) -> Result<DiagnosticReport> {
    let conn = connection(&ctx)?;
    let game = game(&ctx)?;
    let ue = ue4ss::detect(
        game.path.as_deref().map(Path::new),
        game.compat_data_path.as_deref().map(Path::new),
    );
    diagnostics::run(&conn, &game, &ue, &tool(&ctx)?)
}
#[tauri::command]
pub fn diagnostic_report(ctx: State<'_, AppContext>) -> Result<String> {
    Ok(run_diagnostics(ctx)?.text)
}
#[tauri::command]
pub fn get_settings(ctx: State<'_, AppContext>) -> Result<AppSettings> {
    database::settings(&connection(&ctx)?)
}
#[tauri::command]
pub fn save_settings(mut settings: AppSettings, ctx: State<'_, AppContext>) -> Result<()> {
    if settings.game_path.as_deref() == Some("") {
        settings.game_path = None
    }
    if settings.retoc_path.as_deref() == Some("") {
        settings.retoc_path = None
    }
    database::save_settings(&connection(&ctx)?, &settings)
}
#[tauri::command]
pub fn set_game_path(path: String, ctx: State<'_, AppContext>) -> Result<GameInfo> {
    let info = steam::from_manual(Path::new(&path))?;
    database::set_setting(&connection(&ctx)?, "game_path", &path)?;
    Ok(info)
}
#[tauri::command]
pub fn managed_path(kind: String, ctx: State<'_, AppContext>) -> Result<String> {
    let path = if let Some(id) = kind.strip_prefix("mod:") {
        let path = ctx.mods_dir.join(id);
        if !path.is_dir() {
            return Err(AppError::Other(
                "Managed mod source folder was not found.".into(),
            ));
        }
        path
    } else {
        match kind.as_str() {
            "logs" => ctx.logs_dir.clone(),
            "data" => ctx.data_dir.clone(),
            "mods" => game(&ctx)?
                .path
                .map(PathBuf::from)
                .map(|p| p.join("SWZeroCompany/Content/Paks/~mods"))
                .unwrap_or_else(|| ctx.mods_dir.clone()),
            _ => return Err(AppError::Other("unknown managed path".into())),
        }
    };
    std::fs::create_dir_all(&path)?;
    Ok(path.display().to_string())
}

pub fn log(ctx: &AppContext, level: &str, event: &str, detail: &str) {
    use std::io::Write;
    let detail = dirs::home_dir()
        .map(|h| detail.replace(&h.display().to_string(), "~"))
        .unwrap_or_else(|| detail.into());
    let record = serde_json::json!({"timestamp":chrono::Utc::now().to_rfc3339(),"level":level,"event":event,"detail":detail});
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(ctx.logs_dir.join("application.jsonl"))
    {
        let _ = writeln!(file, "{record}");
    }
}

/// What Settings needs to describe the Nexus connection without revealing the
/// key itself.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusStatus {
    pub has_key: bool,
    pub storage: Option<crate::credentials::Storage>,
    pub handler_registered: bool,
    /// The application currently handling `nxm://`, when it is not this one.
    /// Claiming the protocol is a real conflict with whatever held it, so the
    /// interface names the other application rather than failing quietly.
    pub handler_owner: Option<String>,
    /// Why registration cannot take effect on this system, if it cannot.
    pub handler_problem: Option<String>,
}

#[tauri::command]
pub fn nexus_status(app: tauri::AppHandle, ctx: State<'_, AppContext>) -> Result<NexusStatus> {
    let conn = connection(&ctx)?;
    Ok(nexus_status_for(&app, &conn))
}

/// Checks the key against Nexus before storing it, so a typo is reported at
/// the moment it is entered rather than during a download.
#[tauri::command]
pub async fn set_nexus_key(key: String, app: tauri::AppHandle) -> Result<crate::nexus::Account> {
    let account = crate::nexus::validate(key.trim()).await?;
    let ctx = app.state::<AppContext>();
    let conn = database::open(&ctx.db_path)?;
    let storage = crate::credentials::store(&conn, &key)?;
    drop(conn);
    log(
        &ctx,
        "info",
        "nexus_key_stored",
        &format!("storage={storage:?} premium={}", account.premium),
    );
    Ok(account)
}

#[tauri::command]
pub fn clear_nexus_key(ctx: State<'_, AppContext>) -> Result<()> {
    let conn = connection(&ctx)?;
    crate::credentials::clear(&conn)?;
    log(&ctx, "info", "nexus_key_cleared", "");
    Ok(())
}

fn nxm_handler_registered(app: &tauri::AppHandle) -> bool {
    use tauri_plugin_deep_link::DeepLinkExt;
    app.deep_link().is_registered("nxm").unwrap_or(false)
}

/// Reads the desktop entry that currently owns `nxm://` and resolves it to a
/// human name, so the interface can say who holds the protocol.
#[cfg(target_os = "linux")]
fn nxm_handler_owner() -> Option<String> {
    let output = std::process::Command::new("xdg-mime")
        .args(["query", "default", "x-scheme-handler/nxm"])
        .output()
        .ok()?;
    let entry = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if entry.is_empty() {
        return None;
    }
    let mut roots = vec![dirs::data_dir()?];
    roots.extend(
        std::env::var("XDG_DATA_DIRS")
            .unwrap_or_else(|_| "/usr/local/share:/usr/share".into())
            .split(':')
            .map(PathBuf::from),
    );
    for root in roots {
        let candidate = root.join("applications").join(&entry);
        let Ok(text) = std::fs::read_to_string(&candidate) else {
            continue;
        };
        if let Some(name) = text
            .lines()
            .find_map(|line| line.strip_prefix("Name="))
            .filter(|name| !name.is_empty())
        {
            return Some(name.to_string());
        }
    }
    Some(entry)
}

#[cfg(not(target_os = "linux"))]
fn nxm_handler_owner() -> Option<String> {
    None
}

/// `xdg-mime` resolves a desktop entry by taking the first whitespace-separated
/// word of `Exec`, so a correctly quoted path containing a space never resolves
/// and the entry is skipped without any error. Detect that here rather than
/// letting the toggle appear to do nothing.
#[cfg(target_os = "linux")]
fn nxm_handler_problem() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let exe = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .unwrap_or(exe);
    exe.to_string_lossy().contains(' ').then(|| {
        format!(
            "The application path contains a space ({}). xdg-mime cannot resolve such a path, \
             so the association is ignored. Install the .deb, or move the application to a path \
             without spaces.",
            exe.display()
        )
    })
}

#[cfg(not(target_os = "linux"))]
fn nxm_handler_problem() -> Option<String> {
    None
}

fn nexus_status_for(app: &tauri::AppHandle, conn: &rusqlite::Connection) -> NexusStatus {
    let storage = crate::credentials::location(conn);
    let registered = nxm_handler_registered(app);
    NexusStatus {
        has_key: storage.is_some(),
        storage,
        handler_registered: registered,
        handler_owner: (!registered).then(nxm_handler_owner).flatten(),
        handler_problem: (!registered).then(nxm_handler_problem).flatten(),
    }
}

/// Claims or releases the `nxm://` association. Never called on start-up: the
/// user opts in from Settings so the manager does not quietly take the
/// protocol away from another mod manager.
#[tauri::command]
pub fn set_nxm_handler(
    enabled: bool,
    app: tauri::AppHandle,
    ctx: State<'_, AppContext>,
) -> Result<NexusStatus> {
    use tauri_plugin_deep_link::DeepLinkExt;
    let result = if enabled {
        app.deep_link().register("nxm")
    } else {
        app.deep_link().unregister("nxm")
    };
    result.map_err(|e| AppError::Other(format!("The nxm:// association could not change: {e}")))?;
    let conn = connection(&ctx)?;
    Ok(nexus_status_for(&app, &conn))
}

/// Collects a link that launched the application, exactly once.
#[tauri::command]
pub fn take_pending_nxm(ctx: State<'_, AppContext>) -> Result<Option<String>> {
    let mut pending = ctx
        .pending_nxm
        .lock()
        .map_err(|_| AppError::Other("pending link state lock was poisoned".into()))?;
    Ok(pending.take())
}

/// Resolves an `nxm://` link and downloads the file into the cache, returning
/// the local path. Inspection and installation then run through exactly the
/// same validation as a file the user picked by hand.
#[tauri::command]
pub async fn nexus_download(url: String, app: tauri::AppHandle) -> Result<String> {
    use tauri::Emitter;
    let link = crate::nexus::parse_nxm(&url)?;
    let (api_key, cache_dir) = {
        let ctx = app.state::<AppContext>();
        let conn = database::open(&ctx.db_path)?;
        let key = crate::credentials::load(&conn).ok_or(AppError::NexusKeyMissing)?;
        (key, ctx.cache_dir.clone())
    };
    let info = crate::nexus::file_info(&api_key, &link).await?;
    let source = crate::nexus::download_link(&api_key, &link).await?;
    let destination = cache_dir.join("downloads").join(&info.file_name);
    let emitter = app.clone();
    let name = info.name.clone();
    crate::nexus::download_to(&source, &destination, move |done, total| {
        let _ = emitter.emit(
            "zcom://download-progress",
            serde_json::json!({"name": name, "done": done, "total": total}),
        );
    })
    .await?;
    let ctx = app.state::<AppContext>();
    log(
        &ctx,
        "info",
        "nexus_download",
        &format!("mod_id={} file_id={}", link.mod_id, link.file_id),
    );
    Ok(destination.display().to_string())
}
