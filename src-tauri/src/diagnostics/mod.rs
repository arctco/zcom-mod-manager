use crate::{
    database,
    error::Result,
    models::{DiagnosticItem, DiagnosticReport, GameInfo, ToolInfo, Ue4ssInfo},
};
use rusqlite::Connection;
use std::path::Path;

fn item(
    label: &str,
    status: &str,
    value: impl Into<String>,
    action: Option<String>,
) -> DiagnosticItem {
    DiagnosticItem {
        label: label.into(),
        status: status.into(),
        value: value.into(),
        action,
    }
}
pub fn run(
    conn: &Connection,
    game: &GameInfo,
    ue4ss: &Ue4ssInfo,
    retoc: &ToolInfo,
) -> Result<DiagnosticReport> {
    let mods = database::list_mods(conn)?;
    let conflicts = database::conflict_count(conn)?;
    let mut items = Vec::new();
    items.push(item(
        "Game installation",
        if game.detected { "good" } else { "error" },
        if game.detected {
            "Valid Zero Company layout"
        } else {
            "Not detected"
        },
        (!game.detected).then(|| "Locate the game folder in Settings.".into()),
    ));
    items.push(item("Steam manifest",if game.steam_build_id.is_some(){"good"}else{"warning"},game.steam_build_id.as_deref().map(|id|format!("Build {id}")).unwrap_or_else(||"Build ID unavailable".into()),game.steam_build_id.is_none().then(||"Manual installations work, but build compatibility cannot be assessed without an app manifest.".into())));
    let mods_folder = game
        .path
        .as_deref()
        .map(Path::new)
        .map(|p| p.join("SWZeroCompany/Content/Paks/~mods"));
    items.push(item(
        "~mods folder",
        if mods_folder.as_ref().is_some_and(|p| p.is_dir()) {
            "good"
        } else if game.detected {
            "warning"
        } else {
            "unknown"
        },
        mods_folder
            .as_ref()
            .filter(|p| p.exists())
            .map(|_| "Present")
            .unwrap_or("Created on first packaged-mod install"),
        None,
    ));
    items.push(item(
        "Installed mods",
        "good",
        format!(
            "{} managed, {} enabled",
            mods.len(),
            mods.iter().filter(|m| m.enabled).count()
        ),
        None,
    ));
    items.push(item("Package conflicts",if conflicts==0{"good"}else{"warning"},format!("{conflicts} overlapping file/package group(s)"),(conflicts>0).then(||"Open Mods to review the affected managers. Raw package names remain hidden by default.".into())));
    items.push(item("retoc",if retoc.found{"good"}else{"warning"},retoc.version.clone().unwrap_or_else(||"Not configured".into()),(!retoc.found).then(||"Install retoc 0.1.5 or select its executable in Settings before installing IoStore mods.".into())));
    items.push(item(
        "UE4SS",
        if ue4ss.healthy {
            "good"
        } else if ue4ss.installed {
            "warning"
        } else {
            "unknown"
        },
        if ue4ss.healthy {
            format!("Healthy; {} Lua mod(s)", ue4ss.lua_mods)
        } else if ue4ss.installed {
            "Incomplete installation".into()
        } else {
            "Not installed (optional)".into()
        },
        ue4ss.message.clone(),
    ));
    if game.compat_data_path.is_some() {
        items.push(item("Proton compatibility prefix", "good", "Found", None));
        items.push(item(
            "DLL override",
            if ue4ss.proton_override == Some(true) {
                "good"
            } else if ue4ss.installed {
                "warning"
            } else {
                "unknown"
            },
            if ue4ss.proton_override == Some(true) {
                "dwmapi override detected"
            } else {
                "Not detected"
            },
            (ue4ss.installed && ue4ss.proton_override != Some(true)).then(|| {
                "Add to Steam launch options:\nWINEDLLOVERRIDES=\"dwmapi=n,b\" %command%".into()
            }),
        ));
    }
    let has_error = items.iter().any(|i| i.status == "error");
    let has_warning = items.iter().any(|i| i.status == "warning");
    let overall = if has_error {
        "BLOCKED"
    } else if has_warning {
        "NEEDS ATTENTION"
    } else {
        "GOOD"
    }
    .to_string();
    let mut text = format!("ZCOM Mod Doctor\nOverall: {overall}\n\n");
    for i in &items {
        text.push_str(&format!(
            "{:<30} {} — {}\n",
            i.label,
            status_symbol(&i.status),
            i.value
        ));
        if let Some(a) = &i.action {
            text.push_str(&format!("  Action: {a}\n"));
        }
    }
    Ok(DiagnosticReport {
        overall,
        items,
        text: sanitize(&text),
    })
}
fn status_symbol(status: &str) -> &'static str {
    match status {
        "good" => "OK",
        "warning" => "ATTENTION",
        "error" => "ERROR",
        _ => "INFO",
    }
}
fn sanitize(text: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        text.replace(&home.display().to_string(), "~")
    } else {
        text.into()
    }
}
