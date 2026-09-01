use crate::{
    adoption, database, deployment, diagnostics,
    error::{AppError, Result},
    load_order,
    models::{
        AdoptionGroup, AdoptionReport, AppSettings, Dashboard, DiagnosticReport, ExistingModScan,
        GameInfo, LaunchReport, LoadOrderPreview, LoadOrderState, ModPreview, ModSummary,
        ModUpdate, ModUpdateReport, ReplacedMod, StagedMod,
    },
    mods, retoc, steam, ue4ss, AppContext,
};
use std::{
    path::{Path, PathBuf},
    sync::MutexGuard,
    time::Duration,
};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

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

fn discoveries(
    ctx: &AppContext,
) -> Result<MutexGuard<'_, std::collections::HashMap<String, adoption::ScanSnapshot>>> {
    ctx.discoveries
        .lock()
        .map_err(|_| AppError::Other("existing-mod discovery lock was poisoned".into()))
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
    let existing_mod_scan_pending =
        database::get_setting(&conn, "existing_mod_prompt_acknowledged")?.as_deref()
            != Some("true");
    Ok(Dashboard {
        game,
        installed_mods,
        enabled_mods,
        conflict_count,
        ue4ss,
        previous_build_id: ctx.previous_build_id.clone(),
        data_directory: ctx.data_dir.display().to_string(),
        retoc,
        existing_mod_scan_pending,
    })
}
#[tauri::command]
pub fn list_mods(ctx: State<'_, AppContext>) -> Result<Vec<ModSummary>> {
    database::list_mods(&connection(&ctx)?)
}

#[tauri::command]
pub fn discover_existing_mods(ctx: State<'_, AppContext>) -> Result<ExistingModScan> {
    let (game_info, game_path) = require_game(&ctx)?;
    let conn = connection(&ctx)?;
    let settings = database::settings(&conn)?;
    let retoc = retoc::find(settings.retoc_path.as_deref());
    let (scan, snapshot) = adoption::discover(&conn, &game_path, &retoc)?;
    let mut held = discoveries(&ctx)?;
    // Discovery snapshots point into the live game folder and are useful only
    // to the currently visible review. Dropping older scans also bounds memory.
    held.clear();
    held.insert(scan.scan_id.clone(), snapshot);
    drop(held);
    log(
        &ctx,
        "info",
        "existing_mods_discovered",
        &format!(
            "candidates={} unsupported={} build={}",
            scan.candidates.len(),
            scan.unsupported.len(),
            game_info.steam_build_id.as_deref().unwrap_or("unknown")
        ),
    );
    Ok(scan)
}

#[tauri::command]
pub fn acknowledge_existing_mod_prompt(ctx: State<'_, AppContext>) -> Result<()> {
    database::set_setting(
        &connection(&ctx)?,
        "existing_mod_prompt_acknowledged",
        "true",
    )
}

#[tauri::command]
pub fn adopt_existing_mods(
    scan_id: String,
    groups: Vec<AdoptionGroup>,
    ctx: State<'_, AppContext>,
) -> Result<AdoptionReport> {
    let (game_info, _) = require_game(&ctx)?;
    let snapshot = discoveries(&ctx)?.get(&scan_id).cloned().ok_or_else(|| {
        AppError::Other("That discovery expired. Scan the game folders again.".into())
    })?;
    let mut conn = connection(&ctx)?;
    let report = adoption::adopt(
        &mut conn,
        &ctx.mods_dir,
        &ctx.data_dir,
        &snapshot,
        &groups,
        game_info.steam_build_id,
    );
    let successful = report
        .outcomes
        .iter()
        .filter(|outcome| outcome.mod_summary.is_some())
        .flat_map(|outcome| outcome.candidate_ids.iter().cloned())
        .collect::<std::collections::HashSet<_>>();
    if !successful.is_empty() {
        let mut held = discoveries(&ctx)?;
        if let Some(snapshot) = held.get_mut(&scan_id) {
            snapshot
                .candidates
                .retain(|candidate, _| !successful.contains(candidate));
            if snapshot.candidates.is_empty() {
                held.remove(&scan_id);
            }
        }
    }
    log(
        &ctx,
        "info",
        "existing_mods_adopted",
        &format!(
            "succeeded={} failed={}",
            report
                .outcomes
                .iter()
                .filter(|outcome| outcome.mod_summary.is_some())
                .count(),
            report
                .outcomes
                .iter()
                .filter(|outcome| outcome.error.is_some())
                .count()
        ),
    );
    Ok(report)
}

#[tauri::command]
pub fn get_load_order_state(ctx: State<'_, AppContext>) -> Result<LoadOrderState> {
    load_order::state(&connection(&ctx)?)
}

#[tauri::command]
pub fn preview_load_order(
    ordered_mod_ids: Vec<String>,
    ctx: State<'_, AppContext>,
) -> Result<LoadOrderPreview> {
    load_order::preview(&connection(&ctx)?, &ordered_mod_ids)
}

/// Writes the UE4SS start order. Unlike the packaged order, this renames
/// nothing: the runtime reads `mods.txt` top to bottom, so there is no preview
/// step and nothing to roll back.
#[tauri::command]
pub fn apply_ue4ss_order(
    ordered_mod_ids: Vec<String>,
    ctx: State<'_, AppContext>,
) -> Result<LoadOrderState> {
    let (_, game_path) = require_game(&ctx)?;
    let mut conn = connection(&ctx)?;
    let state = load_order::apply_ue4ss_order(&mut conn, &game_path, &ordered_mod_ids)?;
    log(
        &ctx,
        "info",
        "ue4ss_order_applied",
        &format!("ordered_mods={}", ordered_mod_ids.len()),
    );
    Ok(state)
}

#[tauri::command]
pub fn apply_load_order(
    ordered_mod_ids: Vec<String>,
    ctx: State<'_, AppContext>,
) -> Result<LoadOrderState> {
    let mut conn = connection(&ctx)?;
    let state = load_order::apply(
        &mut conn,
        &ordered_mod_ids,
        &ctx.data_dir.join("load-order-operation.json"),
    )?;
    log(
        &ctx,
        "info",
        "load_order_applied",
        &format!("ordered_mods={}", ordered_mod_ids.len()),
    );
    Ok(state)
}

/// Reads an archive or folder and reports every mod it contains.
///
/// One download regularly holds more than one installable mod: a UE4SS archive
/// with several script folders, or a package that ships both a `.pak` and a
/// loader mod. Each is previewed separately so the person can name and install
/// them individually.
#[tauri::command]
pub fn inspect_mod(path: String, ctx: State<'_, AppContext>) -> Result<Vec<ModPreview>> {
    let conn = connection(&ctx)?;
    let settings = database::settings(&conn)?;
    let game = game(&ctx)?;
    let ue = ue4ss::detect(
        game.path.as_deref().map(Path::new),
        game.compat_data_path.as_deref().map(Path::new),
    );
    let tool = retoc::find(settings.retoc_path.as_deref());
    let found = mods::scan(
        Path::new(&path),
        &ctx.cache_dir,
        &tool,
        game.steam_build_id.as_deref(),
        ue.healthy,
        settings.advanced_package_names,
    )?;
    let mut result = Vec::new();
    let mut held = previews(&ctx)?;
    for (staged, mut preview) in found {
        preview.conflicts = database::conflicts_for_packages(&conn, &staged.packages)?;
        preview.replaces = replaced_by(&conn, &staged)?;
        if preview.load_order_supported {
            preview.recommended_priority = Some(database::next_load_priority(&conn)?);
        }
        held.insert(staged.staging_id.clone(), staged);
        result.push(preview);
    }
    drop(held);
    log(
        &ctx,
        "info",
        "mod_inspected",
        &format!(
            "mods={} types={}",
            result.len(),
            result
                .iter()
                .map(|preview| preview.mod_type.clone())
                .collect::<Vec<_>>()
                .join(",")
        ),
    );
    Ok(result)
}

/// Every orderable mod, highest priority first.
fn ordered_supported(conn: &rusqlite::Connection) -> Result<Vec<String>> {
    Ok(load_order::state(conn)?
        .entries
        .into_iter()
        .filter(|entry| entry.supported)
        .map(|entry| entry.id)
        .collect())
}

/// Puts a replacement in the slot its predecessor held. Anything newly
/// orderable, or orderable for the first time, falls in at the end.
fn keep_position(
    conn: &rusqlite::Connection,
    previous: &[String],
    old_id: &str,
    new_id: &str,
) -> Result<Vec<String>> {
    let supported = ordered_supported(conn)?;
    let mut ordered: Vec<String> = previous
        .iter()
        .map(|id| {
            if id == old_id {
                new_id.to_string()
            } else {
                id.clone()
            }
        })
        .filter(|id| supported.contains(id))
        .collect();
    let appended: Vec<String> = supported
        .into_iter()
        .filter(|id| !ordered.contains(id))
        .collect();
    ordered.extend(appended);
    Ok(ordered)
}

/// The installed mod a candidate would land on top of.
///
/// A newer build of a mod occupies exactly the same runtime folder or payload
/// file names as the one already installed. Reporting that as a deployment
/// conflict made updating a mod a two-step chore, so it is surfaced as the
/// upgrade it is.
fn replaced_by(conn: &rusqlite::Connection, staged: &StagedMod) -> Result<Option<ReplacedMod>> {
    let found = match staged.mod_type.as_str() {
        "ue4ss" => staged
            .deployment_keys
            .iter()
            .find_map(|key| database::ue4ss_folder_owner(conn, key).transpose())
            .transpose()?
            .map(|id| (id, "It uses the same UE4SS mod folder.".to_string())),
        "pak" | "iostore" => staged
            .files
            .iter()
            .find_map(|file| {
                database::packaged_source_name_owner(
                    conn,
                    &file.library_relative.display().to_string(),
                    None,
                )
                .transpose()
            })
            .transpose()?
            .map(|id| (id, "It ships the same container files.".to_string())),
        "gamedir" => {
            let game = match game_path(conn)? {
                Some(path) => path,
                None => return Ok(None),
            };
            staged
                .files
                .iter()
                .find_map(|file| {
                    database::destination_owner(
                        conn,
                        &game.join(&file.destination_relative).display().to_string(),
                        None,
                    )
                    .transpose()
                })
                .transpose()?
                .map(|id| (id, "It writes to the same place in the game folder.".into()))
        }
        _ => None,
    };
    found
        .map(|(mod_id, reason)| {
            let (name, version) = database::summary_of(conn, &mod_id)?;
            Ok(ReplacedMod {
                mod_id,
                name,
                version,
                reason,
            })
        })
        .transpose()
}

/// The game folder as the database and Steam currently resolve it, without
/// failing when no game is connected: an inspection is still useful then.
fn game_path(conn: &rusqlite::Connection) -> Result<Option<PathBuf>> {
    let settings = database::settings(conn)?;
    let info = if let Some(path) = settings.game_path.filter(|p| !p.is_empty()) {
        steam::from_manual(Path::new(&path)).ok()
    } else {
        steam::discover().ok().flatten()
    };
    Ok(info.and_then(|info| info.path).map(PathBuf::from))
}

/// Drops the staged copy of an archive once no preview still refers to it.
/// Several previews share one extraction, so the sandbox outlives the first
/// installation and is cleaned up after the last.
fn release_staging(ctx: &AppContext, root: &Path) {
    let still_needed = previews(ctx)
        .map(|held| held.values().any(|staged| staged.staging_root == root))
        .unwrap_or(true);
    if !still_needed {
        let _ = std::fs::remove_dir_all(root);
    }
}

/// Forgets every preview taken from one archive, and removes its sandbox.
#[tauri::command]
pub fn discard_previews(staging_ids: Vec<String>, ctx: State<'_, AppContext>) -> Result<()> {
    let mut roots = Vec::new();
    {
        let mut held = previews(&ctx)?;
        for id in &staging_ids {
            if let Some(staged) = held.remove(id) {
                roots.push(staged.staging_root);
            }
        }
    }
    roots.sort();
    roots.dedup();
    for root in roots {
        release_staging(&ctx, &root);
    }
    Ok(())
}

/// Renames an installed mod. Only the label changes: deployed file names, the
/// UE4SS folder names, and every recorded checksum stay exactly as they are.
#[tauri::command]
pub fn rename_mod(id: String, name: String, ctx: State<'_, AppContext>) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::Other("A mod needs a name.".into()));
    }
    if name.chars().count() > 120 {
        return Err(AppError::Other(
            "That name is too long. Use 120 characters or fewer.".into(),
        ));
    }
    database::rename_mod(&connection(&ctx)?, &id, name)?;
    log(&ctx, "info", "mod_renamed", &format!("mod_id={id}"));
    Ok(())
}

/// Installs a staged mod, optionally over the one it supersedes.
///
/// `replace` carries the id the preview reported in `replaces`. Passing it
/// upgrades in place; leaving it out installs alongside, which fails with the
/// usual deployment conflict when the two really do collide.
#[tauri::command]
pub fn install_mod(
    staging_id: String,
    name: Option<String>,
    replace: Option<String>,
    ctx: State<'_, AppContext>,
) -> Result<ModSummary> {
    let mut staged = previews(&ctx)?
        .get(&staging_id)
        .cloned()
        .ok_or(AppError::PreviewExpired)?;
    if let Some(name) = name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
    {
        staged.name = name.chars().take(120).collect();
    }
    if staged.mod_type == "ue4ss-runtime" {
        return Err(AppError::Other(
            "That archive is the UE4SS runtime. Install it with the UE4SS button instead.".into(),
        ));
    }
    if staged.mod_type == "iostore" && staged.verification != "passed" {
        return Err(AppError::RetocVerificationFailed(
            staged
                .verification_details
                .clone()
                .unwrap_or_else(|| "verification did not pass".into()),
        ));
    }
    let (game_info, game_path) = require_game(&ctx)?;
    let mut conn = connection(&ctx)?;
    // Position the replacement where the mod it supersedes sat, rather than at
    // the top, so an upgrade does not silently change which mod wins.
    let previous_order = replace
        .as_ref()
        .map(|_| ordered_supported(&conn))
        .transpose()?;
    let result = match replace.as_deref() {
        Some(old_id) => deployment::replace(
            &mut conn,
            &ctx.mods_dir,
            &game_path,
            old_id,
            &staged,
            game_info.steam_build_id,
            false,
        ),
        None => deployment::install(
            &mut conn,
            &ctx.mods_dir,
            &game_path,
            &staged,
            game_info.steam_build_id,
        ),
    };
    let summary = result?;
    if summary.mod_type == "ue4ss" {
        // The recorded start order is the source of truth, so mods.txt is
        // rewritten from it. For a fresh install that only confirms the entry
        // just appended; for an upgrade it restores the slot it inherited.
        let ordered = load_order::state(&conn)?
            .ue4ss_entries
            .into_iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>();
        if let Err(error) = load_order::apply_ue4ss_order(&mut conn, &game_path, &ordered) {
            log(&ctx, "warn", "ue4ss_order_not_written", &error.to_string());
        }
    }
    if matches!(summary.mod_type.as_str(), "pak" | "iostore") {
        let ordered = match (previous_order, replace.as_deref()) {
            (Some(previous), Some(old_id)) => keep_position(&conn, &previous, old_id, &summary.id)?,
            _ => ordered_supported(&conn)?,
        };
        if let Err(error) = load_order::apply(
            &mut conn,
            &ordered,
            &ctx.data_dir.join("load-order-operation.json"),
        ) {
            let _ =
                deployment::uninstall(&conn, &ctx.mods_dir, &summary.id, true, Some(&game_path));
            return Err(error);
        }
    }
    // Keep a failed preview available for retry. Only a fully successful
    // deployment consumes its staging id and may release the shared bundle
    // sandbox.
    previews(&ctx)?.remove(&staging_id);
    release_staging(&ctx, &staged.staging_root);
    // A payload that arrived through the nxm:// handoff carries the mod and
    // file it came from. Recording it here is what later lets the manager ask
    // Nexus whether a newer file exists; a hand-picked archive records nothing
    // and is simply never checked.
    match database::link_nexus_source(&conn, &summary.id, &staged.source_archive) {
        Ok(true) => log(
            &ctx,
            "info",
            "nexus_source_linked",
            &format!("mod_id={}", summary.id),
        ),
        Ok(false) => {}
        Err(error) => log(&ctx, "warn", "nexus_source_not_linked", &error.to_string()),
    }
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

/// Keeps a mod out of the library list without touching what is deployed.
/// Existing-mod discovery adopts the UE4SS runtime's own bundled mods, which
/// have to stay installed and ordered but do not need to be looked at.
#[tauri::command]
pub fn set_mod_hidden(id: String, hidden: bool, ctx: State<'_, AppContext>) -> Result<()> {
    database::set_hidden(&connection(&ctx)?, &id, hidden)?;
    log(
        &ctx,
        "info",
        if hidden { "mod_hidden" } else { "mod_shown" },
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
        // The manager's own Nexus page. A release reaches Nexus and GitHub
        // alike, and someone who found the manager on Nexus expects to update
        // it there.
        nexus_manager: "https://www.nexusmods.com/starwarszerocompany/mods/29".into(),
        project: "https://github.com/arctco/zcom-mod-manager".into(),
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Links {
    pub ue4ss_download: String,
    pub nexus_game: String,
    pub nexus_manager: String,
    pub project: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub release_url: String,
    pub update_available: bool,
}

fn version_parts(version: &str) -> Option<Vec<u64>> {
    version
        .trim_start_matches(['v', 'V'])
        .split('.')
        .map(|part| {
            part.split_once('-')
                .map_or(part, |(number, _)| number)
                .parse()
                .ok()
        })
        .collect()
}

fn version_is_newer(latest: &str, current: &str) -> bool {
    let (Some(mut latest), Some(mut current)) = (version_parts(latest), version_parts(current))
    else {
        return latest.trim_start_matches(['v', 'V']) != current.trim_start_matches(['v', 'V']);
    };
    let width = latest.len().max(current.len());
    latest.resize(width, 0);
    current.resize(width, 0);
    latest > current
}

/// Queries the latest published GitHub release. The interface calls this once
/// at startup and also exposes an explicit retry on the About page.
#[tauri::command]
pub async fn check_for_updates() -> Result<UpdateInfo> {
    const RELEASE_API: &str =
        "https://api.github.com/repos/arctco/zcom-mod-manager/releases/latest";
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let response = reqwest::Client::builder()
        .user_agent(format!("zcom-mod-manager/{current_version}"))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|error| AppError::Network(error.to_string()))?
        .get(RELEASE_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| AppError::Network(error.to_string()))?;
    let release: serde_json::Value = response
        .json()
        .await
        .map_err(|error| AppError::Network(error.to_string()))?;
    let tag = release["tag_name"]
        .as_str()
        .ok_or_else(|| AppError::Other("GitHub returned a release without a version.".into()))?;
    let release_url = release["html_url"]
        .as_str()
        .ok_or_else(|| AppError::Other("GitHub returned a release without a link.".into()))?;
    let latest_version = tag.trim_start_matches(['v', 'V']).to_string();
    Ok(UpdateInfo {
        update_available: version_is_newer(&latest_version, &current_version),
        current_version,
        latest_version,
        release_url: release_url.to_string(),
    })
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
    if settings
        .custom_executable_path
        .as_deref()
        .is_some_and(|path| path.trim().is_empty())
    {
        settings.custom_executable_path = None
    }
    database::save_settings(&connection(&ctx)?, &settings)
}
#[tauri::command]
pub fn set_game_path(path: String, ctx: State<'_, AppContext>) -> Result<GameInfo> {
    let info = steam::from_manual(Path::new(&path))?;
    database::set_setting(&connection(&ctx)?, "game_path", &path)?;
    Ok(info)
}

fn managed_path_for(kind: &str, ctx: &AppContext) -> Result<PathBuf> {
    let path = if let Some(id) = kind.strip_prefix("mod:") {
        let conn = connection(ctx)?;
        database::mod_record(&conn, id)?;
        let path = ctx.mods_dir.join(id);
        if !path.is_dir() {
            return Err(AppError::Other(
                "Managed mod source folder was not found.".into(),
            ));
        }
        path
    } else if let Some(id) = kind.strip_prefix("installed:") {
        let conn = connection(ctx)?;
        database::mod_record(&conn, id)?;
        let destination = database::file_records(&conn, id)?
            .into_iter()
            .next()
            .map(|(_, destination, _, _)| PathBuf::from(destination))
            .ok_or_else(|| AppError::Other("This mod has no managed files.".into()))?;
        destination
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| AppError::Other("The installed mod folder was not found.".into()))?
    } else {
        match kind {
            "logs" => ctx.logs_dir.clone(),
            "data" => ctx.data_dir.clone(),
            "mods" => game(ctx)?
                .path
                .map(PathBuf::from)
                .map(|p| p.join("SWZeroCompany/Content/Paks/~mods"))
                .unwrap_or_else(|| ctx.mods_dir.clone()),
            "game" => game(ctx)?
                .path
                .map(PathBuf::from)
                .ok_or(AppError::GameNotFound)?,
            _ => return Err(AppError::Other("unknown managed path".into())),
        }
    };
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

#[tauri::command]
pub fn open_managed_path(kind: String, app: AppHandle, ctx: State<'_, AppContext>) -> Result<()> {
    let path = managed_path_for(&kind, &ctx)?;
    app.opener()
        .open_path(path.display().to_string(), None::<String>)
        .map_err(|error| AppError::Other(format!("The folder could not be opened: {error}")))?;
    Ok(())
}

fn configured_executable(settings: &AppSettings) -> Result<Option<(PathBuf, PathBuf)>> {
    let Some(path) = settings
        .custom_executable_path
        .as_deref()
        .filter(|path| !path.trim().is_empty())
    else {
        return Ok(None);
    };
    let executable = PathBuf::from(path);
    if !executable.is_file() {
        return Err(AppError::Other(
            "The custom game executable no longer exists. Choose it again in Settings or clear it to use Steam."
                .into(),
        ));
    }
    let working_directory = executable
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| {
            AppError::Other("The custom game executable has no containing folder.".into())
        })?
        .to_path_buf();
    Ok(Some((executable, working_directory)))
}

#[tauri::command]
pub fn launch_game(app: AppHandle, ctx: State<'_, AppContext>) -> Result<LaunchReport> {
    let settings = database::settings(&connection(&ctx)?)?;
    if let Some((executable, working_directory)) = configured_executable(&settings)? {
        std::process::Command::new(&executable)
            .current_dir(&working_directory)
            .spawn()
            .map_err(|error| {
                AppError::Other(format!(
                    "The custom game executable could not be launched: {error}. Choose a compatible executable or launcher in Settings."
                ))
            })?;
        log(
            &ctx,
            "info",
            "game_launch_requested",
            &format!("source=custom_executable path={}", executable.display()),
        );
        return Ok(LaunchReport {
            method: "custom-executable".into(),
        });
    }
    require_game(&ctx)?;
    app.opener()
        .open_url(steam::launch_url(), None::<String>)
        .map_err(|error| AppError::Other(format!("Steam could not launch the game: {error}")))?;
    log(&ctx, "info", "game_launch_requested", "source=steam_uri");
    Ok(LaunchReport {
        method: "steam".into(),
    })
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
    /// Who the stored key belongs to, remembered from the moment it was
    /// verified so Settings can say so again after a restart without asking
    /// Nexus on every launch.
    pub account_name: Option<String>,
    /// A premium account can resolve a download link without the key the
    /// website mints, which is the only reason a direct update download is
    /// offered at all.
    pub premium: bool,
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
    database::set_setting(&conn, "nexus_account_name", &account.name)?;
    database::set_setting(&conn, "nexus_premium", &account.premium.to_string())?;
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
    for key in ["nexus_account_name", "nexus_premium"] {
        database::delete_setting(&conn, key)?;
    }
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
    let missing: Vec<&str> = ["xdg-mime", "update-desktop-database"]
        .into_iter()
        .filter(|tool| {
            std::env::var_os("PATH").is_none_or(|path| {
                !std::env::split_paths(&path).any(|dir| dir.join(tool).is_file())
            })
        })
        .collect();
    (!missing.is_empty()).then(|| {
        format!(
            "{} is not installed, so nxm:// links cannot be registered. Install xdg-utils and \
             desktop-file-utils.",
            missing.join(" and ")
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
        account_name: storage
            .is_some()
            .then(|| {
                database::get_setting(conn, "nexus_account_name")
                    .ok()
                    .flatten()
            })
            .flatten(),
        premium: storage.is_some()
            && database::get_setting(conn, "nexus_premium").ok().flatten() == Some("true".into()),
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
    if enabled {
        // The plugin quotes Exec, which xdg-mime can never resolve, so the
        // Linux entry is written by crate::protocol instead. Windows registers
        // through the registry and is unaffected.
        #[cfg(target_os = "linux")]
        crate::protocol::register(
            app.config()
                .product_name
                .as_deref()
                .unwrap_or("ZCOM Mod Manager"),
            &ctx.data_dir,
        )?;
        #[cfg(not(target_os = "linux"))]
        app.deep_link().register("nxm").map_err(|e| {
            AppError::Other(format!("The nxm:// association could not change: {e}"))
        })?;
    } else {
        app.deep_link().unregister("nxm").map_err(|e| {
            AppError::Other(format!("The nxm:// association could not change: {e}"))
        })?;
        // The plugin only clears the generic list.
        #[cfg(target_os = "linux")]
        crate::protocol::unregister()?;
    }
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
    // Announce the file before the transfer starts. Resolving the link takes a
    // moment, and until this arrives the interface has nothing to show but a
    // spinner.
    let announced_total = (info.size_bytes > 0).then_some(info.size_bytes);
    let _ = app.emit(
        "zcom://download-progress",
        serde_json::json!({"name": info.name, "done": 0, "total": announced_total}),
    );
    let source = crate::nexus::download_link(&api_key, &link).await?;
    let destination = cache_dir.join("downloads").join(&info.file_name);
    let emitter = app.clone();
    let name = info.name.clone();
    // One event per chunk floods the webview on a large file and makes the
    // window it is meant to keep responsive stutter instead.
    let mut last = std::time::Instant::now();
    crate::nexus::download_to(&source, &destination, move |done, total| {
        let complete = total.is_some_and(|total| done >= total);
        if !complete && last.elapsed() < Duration::from_millis(100) {
            return;
        }
        last = std::time::Instant::now();
        let _ = emitter.emit(
            "zcom://download-progress",
            serde_json::json!({"name": name, "done": done, "total": total.or(announced_total)}),
        );
    })
    .await?;
    let ctx = app.state::<AppContext>();
    // Remember what this archive is, so the mod installed from it can be
    // checked against Nexus later.
    let path = destination.display().to_string();
    if let Err(error) = database::open(&ctx.db_path).and_then(|conn| {
        database::record_nexus_source(
            &conn,
            &path,
            link.mod_id,
            link.file_id,
            info.version.as_deref(),
            &info.file_name,
        )
    }) {
        log(
            &ctx,
            "warn",
            "nexus_source_not_recorded",
            &error.to_string(),
        );
    }
    log(
        &ctx,
        "info",
        "nexus_download",
        &format!("mod_id={} file_id={}", link.mod_id, link.file_id),
    );
    Ok(path)
}

/// How long a stored result stands before an unforced check goes back to the
/// network. Nexus rate-limits by the hour and mod files change rarely, so a
/// start-up check that finds a recent result stays offline.
const UPDATE_CHECK_INTERVAL_HOURS: i64 = 6;
const LAST_CHECK_KEY: &str = "nexus_update_checked_at";

/// Whether the stored result is recent enough to stand in for a fresh check.
/// A timestamp that cannot be read is treated as no check at all.
fn checked_recently(recorded: Option<String>) -> bool {
    recorded
        .and_then(|last| chrono::DateTime::parse_from_rfc3339(&last).ok())
        .is_some_and(|last| {
            chrono::Utc::now().signed_duration_since(last)
                < chrono::Duration::hours(UPDATE_CHECK_INTERVAL_HOURS)
        })
}

/// Builds the report from what is already recorded. No network access: an
/// update is a stored newest-file id that is later than the installed one.
fn update_report(
    conn: &rusqlite::Connection,
    from_cache: bool,
    identified: usize,
    problem: Option<String>,
) -> Result<ModUpdateReport> {
    let installs = database::nexus_installs(conn)?;
    let latest = database::nexus_latest(conn)?;
    let mut updates = Vec::new();
    for install in &installs {
        let Some(known) = latest.get(&install.nexus_mod_id) else {
            continue;
        };
        if !crate::nexus::is_newer(known.latest_file_id, install.nexus_file_id) {
            continue;
        }
        updates.push(ModUpdate {
            mod_id: install.id.clone(),
            name: install.name.clone(),
            installed_version: install.version.clone(),
            installed_file_id: install.nexus_file_id,
            nexus_mod_id: install.nexus_mod_id,
            latest_file_id: known.latest_file_id,
            latest_version: known.latest_version.clone(),
            latest_file_name: known.latest_file_name.clone(),
            page_url: crate::nexus::mod_files_url(install.nexus_mod_id),
            nxm_url: crate::nexus::nxm_url(install.nexus_mod_id, known.latest_file_id),
            checked_at: known.checked_at.clone(),
        });
    }
    Ok(ModUpdateReport {
        updates,
        tracked: installs.len(),
        identified,
        unmatched: database::untracked_installs(conn)?.len(),
        ignored: database::ignored_count(conn)?,
        checked_at: database::get_setting(conn, LAST_CHECK_KEY)?,
        from_cache,
        problem,
    })
}

/// Matches installed mods to their Nexus page by the MD5 of the archive they
/// were installed from.
///
/// This is what covers a library that existed before the manager recorded
/// provenance, and anything installed from an archive downloaded in a browser.
/// Only the uploaded archive is indexed by Nexus, so a mod whose archive has
/// been deleted, or that never came from Nexus, stays unmatched. An archive
/// Nexus does not recognise is remembered as such and is retried only when the
/// user asks for a check themselves.
async fn identify_untracked(
    app: &AppHandle,
    api_key: &str,
    retry: bool,
) -> Result<(usize, Option<String>)> {
    let candidates: Vec<(String, PathBuf)> = {
        let ctx = app.state::<AppContext>();
        let conn = database::open(&ctx.db_path)?;
        database::untracked_installs(&conn)?
            .into_iter()
            .filter(|install| retry || install.attempt.is_none())
            .filter_map(|install| {
                let archive = PathBuf::from(install.source_archive?);
                archive.is_file().then_some((install.id, archive))
            })
            .collect()
    };
    let mut identified = 0;
    for (mod_id, archive) in candidates {
        // An unreadable archive is no worse than a missing one.
        let Ok(md5) = md5_of(&archive) else { continue };
        // A rate limit or a rejected key ends the pass; what was matched before
        // it stands, and the caller reports why the rest was not attempted.
        let found = match crate::nexus::md5_search(api_key, &md5).await {
            Ok(found) => found,
            Err(error) => return Ok((identified, Some(error.to_string()))),
        };
        let ctx = app.state::<AppContext>();
        let conn = database::open(&ctx.db_path)?;
        if let Some((nexus_mod_id, nexus_file_id)) = found {
            database::set_nexus_ids(&conn, &mod_id, nexus_mod_id, nexus_file_id)?;
            identified += 1;
        }
        database::record_identification(&conn, &mod_id, &md5, found.is_some())?;
    }
    Ok((identified, None))
}

/// Nexus indexes uploaded files by MD5, so that is the digest this needs. It is
/// never used here to decide that a file is unchanged or trustworthy.
fn md5_of(path: &Path) -> Result<String> {
    use md5::{Digest, Md5};
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hash = Md5::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(hex::encode(hash.finalize()))
}

/// Points an installed mod at a Nexus mod the user names, for a mod whose
/// archive is gone or was never a Nexus download. The file recorded as
/// installed is the one carrying the installed version, so an update is only
/// reported when Nexus really has moved past it.
#[tauri::command]
pub async fn link_mod_to_nexus(
    mod_id: String,
    reference: String,
    app: AppHandle,
) -> Result<ModUpdateReport> {
    let nexus_mod_id = crate::nexus::parse_mod_reference(&reference)?;
    let (api_key, installed_version) = {
        let ctx = app.state::<AppContext>();
        let conn = database::open(&ctx.db_path)?;
        let version = rusqlite::OptionalExtension::optional(conn.query_row(
            "SELECT version FROM mods WHERE id=?1",
            [&mod_id],
            |row| row.get::<_, Option<String>>(0),
        ))?
        .ok_or_else(|| AppError::Other("That mod is no longer installed.".into()))?;
        let key = crate::credentials::load(&conn).ok_or(AppError::NexusKeyMissing)?;
        (key, version)
    };
    let files = crate::nexus::files(&api_key, nexus_mod_id).await?;
    let file = crate::nexus::file_for_version(&files, installed_version.as_deref())
        .ok_or_else(|| AppError::Other("That Nexus mod offers no files to match.".into()))?;
    let latest = crate::nexus::newest_offered(&files).map(|newest| database::NexusLatest {
        latest_file_id: newest.file_id,
        latest_version: newest.version.clone(),
        latest_file_name: newest.file_name.clone(),
        checked_at: chrono::Utc::now().to_rfc3339(),
    });
    let ctx = app.state::<AppContext>();
    let conn = database::open(&ctx.db_path)?;
    // Naming a page is an instruction to check it, whatever was decided before.
    database::set_nexus_checked(&conn, &mod_id, true)?;
    database::set_nexus_ids(&conn, &mod_id, nexus_mod_id, file.file_id)?;
    if let Some(latest) = latest {
        database::record_nexus_latest(&conn, nexus_mod_id, &latest)?;
    }
    log(
        &ctx,
        "info",
        "nexus_mod_linked",
        &format!(
            "mod_id={mod_id} nexus_mod_id={nexus_mod_id} file_id={}",
            file.file_id
        ),
    );
    update_report(&conn, false, 0, None)
}

/// Takes a mod out of update checking, or puts it back.
///
/// This covers both halves of the same decision: a mod linked to the wrong page
/// and a mod that never came from Nexus at all. Turning checking off drops the
/// link and keeps the mod out of the identification lookup, so a check the user
/// asks for does not quietly match and link it again.
#[tauri::command]
pub fn set_mod_checked(
    mod_id: String,
    checked: bool,
    ctx: State<'_, AppContext>,
) -> Result<ModUpdateReport> {
    let conn = connection(&ctx)?;
    database::set_nexus_checked(&conn, &mod_id, checked)?;
    log(
        &ctx,
        "info",
        if checked {
            "nexus_checks_resumed"
        } else {
            "nexus_checks_stopped"
        },
        &format!("mod_id={mod_id}"),
    );
    update_report(&conn, true, 0, None)
}

/// Turns the start-up check on or off, on its own.
///
/// Every other control in the Nexus panel takes effect the moment it is used,
/// so a checkbox that quietly needed **Save settings** as well read as one that
/// did not work — and any refresh overwrote the pending toggle before it could
/// be saved. Writing the single setting also avoids committing whatever else is
/// half-edited on the Settings page.
#[tauri::command]
pub fn set_nexus_auto_check(enabled: bool, ctx: State<'_, AppContext>) -> Result<()> {
    let conn = connection(&ctx)?;
    database::set_setting(&conn, "nexus_auto_update_check", &enabled.to_string())?;
    log(
        &ctx,
        "info",
        "nexus_auto_check_changed",
        &format!("enabled={enabled}"),
    );
    Ok(())
}

/// What the last check found. Read on every refresh so the library can show
/// known updates without reaching Nexus.
#[tauri::command]
pub fn mod_updates(ctx: State<'_, AppContext>) -> Result<ModUpdateReport> {
    update_report(&connection(&ctx)?, true, 0, None)
}

/// Asks Nexus which files each tracked mod now offers.
///
/// `force` is the Mods page button. Without it the stored result stands for
/// `UPDATE_CHECK_INTERVAL`, which is what the opt-in start-up check relies on
/// so a manager opened repeatedly does not spend the hourly API allowance.
#[tauri::command]
pub async fn check_mod_updates(force: bool, app: AppHandle) -> Result<ModUpdateReport> {
    let api_key = {
        let ctx = app.state::<AppContext>();
        let conn = database::open(&ctx.db_path)?;
        // Nothing installed at all, so there is neither anything to check nor
        // anything to identify.
        if database::nexus_installs(&conn)?.is_empty()
            && database::untracked_installs(&conn)?.is_empty()
        {
            return update_report(&conn, true, 0, None);
        }
        if !force && checked_recently(database::get_setting(&conn, LAST_CHECK_KEY)?) {
            return update_report(&conn, true, 0, None);
        }
        let Some(api_key) = crate::credentials::load(&conn) else {
            if force {
                return Err(AppError::NexusKeyMissing);
            }
            return update_report(
                &conn,
                true,
                0,
                Some("No Nexus Mods API key is stored, so installed mods were not checked.".into()),
            );
        };
        api_key
    };

    // A forced check is also the moment to confirm the stored key still works
    // and whether the account is premium, which is what decides if a direct
    // download can be offered instead of a trip to the website.
    let account = match force {
        true => crate::nexus::validate(&api_key).await.ok(),
        false => None,
    };
    // Mods with no provenance are matched by their archive first, so anything
    // recognised here is checked in this same pass. An archive Nexus has
    // already refused is offered again only when the user asked for the check.
    let (identified, mut problem) = identify_untracked(&app, &api_key, force).await?;
    let targets = {
        let ctx = app.state::<AppContext>();
        let conn = database::open(&ctx.db_path)?;
        let mut targets: Vec<u64> = database::nexus_installs(&conn)?
            .iter()
            .map(|install| install.nexus_mod_id)
            .collect();
        targets.sort_unstable();
        // One request per Nexus mod, however many local mods came out of it.
        targets.dedup();
        targets
    };
    let mut found: Vec<(u64, database::NexusLatest)> = Vec::new();
    let mut failed = 0usize;
    for nexus_mod_id in targets.iter().take_while(|_| problem.is_none()) {
        match crate::nexus::files(&api_key, *nexus_mod_id).await {
            Ok(files) => {
                if let Some(file) = crate::nexus::newest_offered(&files) {
                    found.push((
                        *nexus_mod_id,
                        database::NexusLatest {
                            latest_file_id: file.file_id,
                            latest_version: file.version.clone(),
                            latest_file_name: file.file_name.clone(),
                            checked_at: String::new(),
                        },
                    ));
                }
            }
            // Stopping on a rate limit or a rejected key keeps the remaining
            // requests from making either worse; what was already read stands.
            Err(error @ (AppError::NexusRateLimited | AppError::NexusUnauthorized)) => {
                problem = Some(error.to_string());
                break;
            }
            // A single mod can be hidden, deleted, or moderated. That is not a
            // reason to abandon the rest of the library.
            Err(_) => failed += 1,
        }
    }
    if problem.is_none() && failed > 0 {
        problem = Some(format!(
            "{failed} of {} mods could not be read on Nexus Mods. They may have been hidden or removed.",
            targets.len()
        ));
    }

    let ctx = app.state::<AppContext>();
    let conn = database::open(&ctx.db_path)?;
    let now = chrono::Utc::now().to_rfc3339();
    if let Some(account) = account {
        database::set_setting(&conn, "nexus_account_name", &account.name)?;
        database::set_setting(&conn, "nexus_premium", &account.premium.to_string())?;
    }
    for (nexus_mod_id, latest) in &found {
        database::record_nexus_latest(
            &conn,
            *nexus_mod_id,
            &database::NexusLatest {
                checked_at: now.clone(),
                ..latest.clone()
            },
        )?;
    }
    // Only a complete pass advances the throttle, so a run cut short by a rate
    // limit is retried on the next opportunity rather than waiting it out.
    if problem.is_none() {
        database::set_setting(&conn, LAST_CHECK_KEY, &now)?;
    }
    let report = update_report(&conn, false, identified, problem)?;
    log(
        &ctx,
        "info",
        "nexus_update_check",
        &format!(
            "checked={} identified={} tracked={} unmatched={} updates={}",
            found.len(),
            identified,
            report.tracked,
            report.unmatched,
            report.updates.len()
        ),
    );
    Ok(report)
}

#[cfg(test)]
mod update_tests {
    use super::{
        checked_recently, configured_executable, replaced_by, update_report, version_is_newer,
    };
    use crate::{
        database,
        models::{AppSettings, PayloadFile, StagedMod},
    };
    use std::{fs, path::PathBuf};
    use tempfile::tempdir;

    #[test]
    fn compares_release_versions_numerically() {
        assert!(version_is_newer("v0.2.0", "0.1.4"));
        assert!(version_is_newer("0.1.10", "0.1.9"));
        assert!(!version_is_newer("v0.1.4", "0.1.4"));
        assert!(!version_is_newer("0.1.3", "0.1.4"));
    }

    /// An installed mod that arrived through the handoff, recorded the way the
    /// download and installation commands record one between them.
    fn installed_from_nexus(
        conn: &rusqlite::Connection,
        id: &str,
        archive: &str,
        nexus_mod_id: u64,
        file_id: u64,
    ) {
        conn.execute(
            "INSERT INTO mods(id,name,version,mod_type,deployment_key,source_archive,installed_at,enabled,load_priority) \
             VALUES(?1,?1,'1.0','iostore','',?2,'now',1,1)",
            rusqlite::params![id, archive],
        )
        .unwrap();
        database::record_nexus_source(conn, archive, nexus_mod_id, file_id, Some("1.0"), "mod.zip")
            .unwrap();
        assert!(database::link_nexus_source(conn, id, archive).unwrap());
    }

    fn latest(file_id: u64, version: &str) -> database::NexusLatest {
        database::NexusLatest {
            latest_file_id: file_id,
            latest_version: Some(version.into()),
            latest_file_name: format!("mod-{version}.zip"),
            checked_at: "2026-09-01T00:00:00+00:00".into(),
        }
    }

    #[test]
    fn an_archive_the_user_picked_by_hand_carries_no_provenance() {
        let directory = tempdir().unwrap();
        let conn = database::open(&directory.path().join("db.sqlite3")).unwrap();
        conn.execute(
            "INSERT INTO mods(id,name,version,mod_type,deployment_key,source_archive,installed_at,enabled,load_priority) \
             VALUES('local','Local','1.0','iostore','','/home/user/mod.zip','now',1,1)",
            [],
        )
        .unwrap();
        assert!(!database::link_nexus_source(&conn, "local", "/home/user/mod.zip").unwrap());
        assert!(database::nexus_installs(&conn).unwrap().is_empty());
        let report = update_report(&conn, true, 0, None).unwrap();
        assert_eq!((report.tracked, report.updates.len()), (0, 0));
    }

    #[test]
    fn only_a_later_file_than_the_installed_one_is_an_update() {
        let directory = tempdir().unwrap();
        let conn = database::open(&directory.path().join("db.sqlite3")).unwrap();
        installed_from_nexus(&conn, "unlocked", "/cache/unlocked.zip", 34, 200);
        // The same file that is installed, and an older one, are not updates.
        for known in [latest(200, "1.3"), latest(180, "1.2")] {
            database::record_nexus_latest(&conn, 34, &known).unwrap();
            assert!(update_report(&conn, true, 0, None)
                .unwrap()
                .updates
                .is_empty());
        }
        database::record_nexus_latest(&conn, 34, &latest(260, "1.4")).unwrap();
        let report = update_report(&conn, true, 0, None).unwrap();
        assert_eq!(report.tracked, 1);
        let update = &report.updates[0];
        assert_eq!(update.mod_id, "unlocked");
        assert_eq!(update.latest_version.as_deref(), Some("1.4"));
        // The link has to be one this manager would accept back from a browser.
        assert_eq!(
            crate::nexus::parse_nxm(&update.nxm_url).unwrap().file_id,
            260
        );
    }

    #[test]
    fn every_mod_from_one_nexus_page_is_reported() {
        let directory = tempdir().unwrap();
        let conn = database::open(&directory.path().join("db.sqlite3")).unwrap();
        // A single archive can install several mods, and each keeps the
        // provenance of the file it came from.
        installed_from_nexus(&conn, "core", "/cache/suite.zip", 9, 100);
        installed_from_nexus(&conn, "extras", "/cache/suite.zip", 9, 100);
        database::record_nexus_latest(&conn, 9, &latest(150, "2.0")).unwrap();
        let report = update_report(&conn, true, 0, None).unwrap();
        assert_eq!(report.updates.len(), 2);
        assert_eq!(report.tracked, 2);
    }

    #[test]
    fn a_library_installed_before_provenance_existed_is_offered_for_identification() {
        let directory = tempdir().unwrap();
        let conn = database::open(&directory.path().join("db.sqlite3")).unwrap();
        // Installed from an archive the user still has, from one that is gone,
        // and adopted from disk with no archive at all.
        for (id, archive) in [
            ("kept", "/downloads/kept.zip"),
            ("gone", "/downloads/gone.zip"),
            ("adopted", ""),
        ] {
            conn.execute(
                "INSERT INTO mods(id,name,version,mod_type,deployment_key,source_archive,installed_at,enabled,load_priority) \
                 VALUES(?1,?1,'1.0','iostore','',?2,'now',1,1)",
                rusqlite::params![id, archive],
            )
            .unwrap();
        }
        let untracked = database::untracked_installs(&conn).unwrap();
        assert_eq!(untracked.len(), 3);
        // An adopted mod has no archive to offer, so it can only be linked by
        // hand; the other two are candidates for an MD5 lookup.
        let adopted = untracked.iter().find(|i| i.id == "adopted").unwrap();
        assert!(adopted.source_archive.is_none());
        assert!(untracked.iter().all(|install| install.attempt.is_none()));

        // A refusal is remembered so an automatic check does not ask again.
        database::record_identification(&conn, "gone", "d41d8cd98f00b204e9800998ecf8427e", false)
            .unwrap();
        let untracked = database::untracked_installs(&conn).unwrap();
        let gone = untracked.iter().find(|i| i.id == "gone").unwrap();
        assert!(!gone.attempt.as_ref().unwrap().1);

        // A match takes the mod out of the untracked list and into the checked
        // one, without any download having happened.
        database::set_nexus_ids(&conn, "kept", 34, 260).unwrap();
        database::record_identification(&conn, "kept", "0123456789abcdef0123456789abcdef", true)
            .unwrap();
        assert_eq!(database::untracked_installs(&conn).unwrap().len(), 2);
        let installs = database::nexus_installs(&conn).unwrap();
        assert_eq!(installs.len(), 1);
        assert_eq!(installs[0].nexus_file_id, 260);
        let report = update_report(&conn, true, 1, None).unwrap();
        assert_eq!(
            (
                report.tracked,
                report.identified,
                report.unmatched,
                report.ignored
            ),
            (1, 1, 2, 0)
        );
    }

    #[test]
    fn stopping_checks_on_a_linked_mod_keeps_it_from_being_relinked() {
        let directory = tempdir().unwrap();
        let conn = database::open(&directory.path().join("db.sqlite3")).unwrap();
        installed_from_nexus(&conn, "unlocked", "/cache/unlocked.zip", 34, 200);
        database::record_identification(
            &conn,
            "unlocked",
            "0123456789abcdef0123456789abcdef",
            true,
        )
        .unwrap();
        database::record_nexus_latest(&conn, 34, &latest(260, "1.4")).unwrap();
        assert_eq!(
            update_report(&conn, true, 0, None).unwrap().updates.len(),
            1
        );

        database::set_nexus_checked(&conn, "unlocked", false).unwrap();
        let report = update_report(&conn, true, 0, None).unwrap();
        assert_eq!((report.tracked, report.updates.len()), (0, 0));
        // Out of checking is also out of identification: the archive is still
        // on disk, and a check the user asks for must not match it and link it
        // again behind their back.
        assert!(database::untracked_installs(&conn).unwrap().is_empty());
        assert_eq!((report.unmatched, report.ignored), (0, 1));

        // Turning it back on offers the archive to Nexus once more.
        database::set_nexus_checked(&conn, "unlocked", true).unwrap();
        let untracked = database::untracked_installs(&conn).unwrap();
        assert_eq!(untracked.len(), 1);
        assert!(untracked[0].attempt.is_none());
        assert_eq!(update_report(&conn, true, 0, None).unwrap().ignored, 0);
    }

    #[test]
    fn a_mod_that_never_came_from_nexus_can_be_left_out_for_good() {
        let directory = tempdir().unwrap();
        let conn = database::open(&directory.path().join("db.sqlite3")).unwrap();
        conn.execute(
            "INSERT INTO mods(id,name,version,mod_type,deployment_key,source_archive,installed_at,enabled,load_priority) \
             VALUES('mine','My own mod','1.0','iostore','','/home/user/dist/mine.zip','now',1,1)",
            [],
        )
        .unwrap();
        // Until it is excluded it is a candidate on every check the user asks
        // for, which is a request per check for a mod Nexus will never know.
        assert_eq!(database::untracked_installs(&conn).unwrap().len(), 1);

        database::set_nexus_checked(&conn, "mine", false).unwrap();
        assert!(database::untracked_installs(&conn).unwrap().is_empty());
        let report = update_report(&conn, true, 0, None).unwrap();
        assert_eq!(
            (report.tracked, report.unmatched, report.ignored),
            (0, 0, 1)
        );
    }

    #[test]
    fn a_recent_result_stands_and_an_unreadable_one_does_not() {
        let now = chrono::Utc::now();
        assert!(checked_recently(Some(now.to_rfc3339())));
        assert!(checked_recently(Some(
            (now - chrono::Duration::hours(5)).to_rfc3339()
        )));
        assert!(!checked_recently(Some(
            (now - chrono::Duration::hours(7)).to_rfc3339()
        )));
        assert!(!checked_recently(Some("not a timestamp".into())));
        assert!(!checked_recently(None));
    }

    #[test]
    fn validates_a_configured_launch_executable_and_uses_its_folder() {
        let directory = tempdir().unwrap();
        let executable = directory.path().join("ZeroCompany.exe");
        fs::write(&executable, b"test executable").unwrap();
        let settings = AppSettings {
            custom_executable_path: Some(executable.display().to_string()),
            ..AppSettings::default()
        };

        let (selected, working_directory) = configured_executable(&settings).unwrap().unwrap();
        assert_eq!(selected, executable);
        assert_eq!(working_directory, directory.path());
    }

    #[test]
    fn reports_a_missing_custom_launch_executable() {
        let settings = AppSettings {
            custom_executable_path: Some("/missing/ZeroCompany.exe".into()),
            ..AppSettings::default()
        };
        let error = configured_executable(&settings).unwrap_err().to_string();
        assert!(error.contains("no longer exists"));
        assert!(error.contains("clear it to use Steam"));
    }

    #[test]
    fn one_bundle_matches_both_entries_from_an_old_split_install() {
        let directory = tempdir().unwrap();
        let connection = database::open(&directory.path().join("mods.sqlite3")).unwrap();
        connection.execute(
            "INSERT INTO mods(id,name,version,mod_type,deployment_key,source_archive,installed_at,enabled,load_priority) VALUES('old-core','Squad Six - Core','1.0.1','iostore','','core.zip','now',1,1)",
            [],
        ).unwrap();
        connection.execute(
            "INSERT INTO mod_files(mod_id,library_relative,destination,size,sha256) VALUES('old-core','pakchunk99-ZCOMSquadSix_P.pak','/game/~mods/pakchunk99-ZCOMSquadSix_P.pak',1,'hash')",
            [],
        ).unwrap();
        connection.execute(
            "INSERT INTO mods(id,name,version,mod_type,deployment_key,source_archive,installed_at,enabled,load_priority) VALUES('old-runtime','Squad Six - Runtime','1.0.1','ue4ss','ZCOMSquadSix','runtime.zip','now',1,1)",
            [],
        ).unwrap();

        let component = |kind: &str, keys: Vec<String>, files: Vec<PayloadFile>| StagedMod {
            staging_id: kind.into(),
            staging_root: PathBuf::from("/staging"),
            source_archive: "Squad-Six-ZCOM-Manager.zip".into(),
            name: format!("Squad Six - {kind}"),
            version: Some("1.1.1".into()),
            author: None,
            description: None,
            mod_type: kind.to_ascii_lowercase(),
            deployment_keys: keys,
            files,
            packages: Vec::new(),
            verification: "passed".into(),
            verification_details: None,
        };
        let core = component(
            "IoStore",
            Vec::new(),
            vec![PayloadFile {
                source: PathBuf::from("/staging/Core/pakchunk99-ZCOMSquadSix_P.pak"),
                library_relative: PathBuf::from("pakchunk99-ZCOMSquadSix_P.pak"),
                destination_relative: PathBuf::from("pakchunk99-ZCOMSquadSix_P.pak"),
            }],
        );
        let runtime = component("UE4SS", vec!["ZCOMSquadSix".into()], Vec::new());

        assert_eq!(
            replaced_by(&connection, &core).unwrap().unwrap().mod_id,
            "old-core"
        );
        assert_eq!(
            replaced_by(&connection, &runtime).unwrap().unwrap().mod_id,
            "old-runtime"
        );
    }
}
