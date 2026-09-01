use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GameInfo {
    pub detected: bool,
    pub path: Option<String>,
    pub steam_build_id: Option<String>,
    pub install_state: Option<String>,
    pub engine: String,
    pub compat_data_path: Option<String>,
    pub source: String,
}

impl Default for GameInfo {
    fn default() -> Self {
        Self {
            detected: false,
            path: None,
            steam_build_id: None,
            install_state: None,
            engine: "UE 5.6.1".into(),
            compat_data_path: None,
            source: "none".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub found: bool,
    pub path: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Ue4ssInfo {
    pub installed: bool,
    pub healthy: bool,
    /// UE4SS mod folders present in the runtime, script and DLL mods alike.
    pub mod_count: usize,
    pub log_found: bool,
    pub proton_override: Option<bool>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Ue4ssInstallReport {
    /// Number of runtime files written into `Binaries/Win64`.
    pub installed: usize,
    /// Existing user-owned files that were kept instead of being overwritten.
    pub preserved: Vec<String>,
    /// Whether the Steam launch-option reminder applies to this platform.
    pub proton_hint: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub game: GameInfo,
    pub installed_mods: usize,
    pub enabled_mods: usize,
    pub conflict_count: usize,
    pub ue4ss: Ue4ssInfo,
    pub previous_build_id: Option<String>,
    pub data_directory: String,
    pub retoc: ToolInfo,
    /// Whether the one-time existing-mod discovery has not yet been shown.
    pub existing_mod_scan_pending: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModFile {
    pub name: String,
    pub destination: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModSummary {
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub mod_type: String,
    pub enabled: bool,
    pub installed_at: String,
    pub installed_build: Option<String>,
    pub package_count: usize,
    pub conflict_count: usize,
    pub potential_conflict_count: usize,
    pub load_priority: Option<i64>,
    /// The Nexus mod this was installed from, when it is known. Only a mod with
    /// one can be checked for updates.
    pub nexus_mod_id: Option<u64>,
    /// That mod's page, so the interface can offer to open it without knowing
    /// how a Nexus address is put together.
    pub nexus_url: Option<String>,
    /// Taken out of update checking by the user. Neither checked nor offered to
    /// the identification lookup again.
    pub nexus_ignored: bool,
    /// Kept out of the library list. Still installed, still deployed, and still
    /// ordered — only hidden from view.
    pub hidden: bool,
    pub files: Vec<ModFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PreviewConflict {
    pub mod_id: String,
    pub name: String,
    pub package_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadOrderEntry {
    pub id: String,
    pub name: String,
    pub mod_type: String,
    /// For a UE4SS mod, which of the runtime's start passes it belongs to:
    /// `native` for a DLL mod, `script` for a Lua mod, `mixed` when a mod
    /// ships both. `None` for packaged mods.
    pub runtime_kind: Option<String>,
    pub enabled: bool,
    pub priority: Option<i64>,
    pub supported: bool,
    pub support_reason: Option<String>,
    pub applied: bool,
    pub active_conflict_count: usize,
    pub potential_conflict_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConflictGroup {
    pub id: String,
    pub member_ids: Vec<String>,
    pub package_count: usize,
    pub active: bool,
    pub potential: bool,
    pub winner_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadOrderState {
    pub entries: Vec<LoadOrderEntry>,
    /// UE4SS mods in the order the runtime starts them, first to last. They are
    /// ordered by `mods.txt` rather than by deployed file name, so they are a
    /// separate list rather than another row in `entries`.
    pub ue4ss_entries: Vec<LoadOrderEntry>,
    pub active_conflicts: Vec<ConflictGroup>,
    pub potential_conflicts: Vec<ConflictGroup>,
    pub unapplied: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LoadOrderMove {
    pub mod_id: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WinnerChange {
    pub conflict_id: String,
    pub from_mod_id: Option<String>,
    pub to_mod_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadOrderPreview {
    pub ordered_mod_ids: Vec<String>,
    pub moves: Vec<LoadOrderMove>,
    pub active_conflicts: Vec<ConflictGroup>,
    pub potential_conflicts: Vec<ConflictGroup>,
    pub winner_changes: Vec<WinnerChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub game: Option<ManifestGame>,
    #[serde(rename = "type", default)]
    pub mod_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestGame {
    pub app_id: u32,
    #[serde(default)]
    pub tested_builds: Vec<String>,
    pub strict: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct PayloadFile {
    pub source: PathBuf,
    pub library_relative: PathBuf,
    pub destination_relative: PathBuf,
}

#[derive(Debug, Clone)]
pub struct StagedMod {
    pub staging_id: String,
    pub staging_root: PathBuf,
    pub source_archive: String,
    pub name: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub mod_type: String,
    /// UE4SS mod folder names this payload owns. An archive regularly ships
    /// several, and every one needs its own line in `mods.txt`.
    pub deployment_keys: Vec<String>,
    pub files: Vec<PayloadFile>,
    pub packages: Vec<String>,
    pub verification: String,
    pub verification_details: Option<String>,
}

/// An installed mod that a candidate would take the place of, so the interface
/// can offer an upgrade instead of a deployment conflict.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReplacedMod {
    pub mod_id: String,
    pub name: String,
    pub version: Option<String>,
    /// Why this candidate lands on the same files.
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModPreview {
    pub staging_id: String,
    /// The archive or folder this candidate was read from, so the interface can
    /// hand a runtime package back to the UE4SS installer.
    pub source_path: String,
    pub name: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub mod_type: String,
    pub files: Vec<String>,
    pub warnings: Vec<String>,
    pub valid: bool,
    pub verification: String,
    pub verification_details: Option<String>,
    pub package_count: usize,
    pub package_names: Vec<String>,
    pub compatibility: String,
    pub compatibility_message: String,
    pub tested_builds: Vec<String>,
    pub conflicts: Vec<PreviewConflict>,
    /// The installed mod this one would replace, when there is one.
    pub replaces: Option<ReplacedMod>,
    pub recommended_priority: Option<i64>,
    pub load_order_supported: bool,
    pub load_order_support_reason: Option<String>,
    /// A containing folder that represents one selectable packaged option in
    /// an archive with several sibling variants/components.
    pub option_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExistingModCandidate {
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub mod_type: String,
    /// Display-only paths relative to one of the controlled mod roots.
    pub files: Vec<String>,
    pub enabled: bool,
    pub package_count: usize,
    pub warnings: Vec<String>,
    pub adoptable: bool,
    pub blocked_reason: Option<String>,
    pub selected_by_default: bool,
    pub likely_runtime_component: bool,
    pub inferred_priority: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExistingModScan {
    pub scan_id: String,
    pub candidates: Vec<ExistingModCandidate>,
    pub unsupported: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptionGroup {
    pub candidate_ids: Vec<String>,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptionOutcome {
    pub candidate_ids: Vec<String>,
    pub name: String,
    pub mod_summary: Option<ModSummary>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptionReport {
    pub outcomes: Vec<AdoptionOutcome>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticItem {
    pub label: String,
    pub status: String,
    pub value: String,
    pub action: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticReport {
    pub overall: String,
    pub items: Vec<DiagnosticItem>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub game_path: Option<String>,
    pub custom_executable_path: Option<String>,
    pub retoc_path: Option<String>,
    pub log_level: String,
    pub advanced_package_names: bool,
    pub reduced_motion: bool,
    /// Whether one throttled update check may run on start-up. Off by default:
    /// the manager reaches Nexus only when the user has asked it to.
    pub nexus_auto_update_check: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            game_path: None,
            custom_executable_path: None,
            retoc_path: None,
            log_level: "normal".into(),
            advanced_package_names: false,
            reduced_motion: false,
            nexus_auto_update_check: false,
        }
    }
}

/// An installed mod that Nexus Mods now offers a newer file for.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModUpdate {
    /// The installed mod, not the Nexus mod.
    pub mod_id: String,
    pub name: String,
    pub installed_version: Option<String>,
    pub installed_file_id: u64,
    pub nexus_mod_id: u64,
    pub latest_file_id: u64,
    pub latest_version: Option<String>,
    pub latest_file_name: String,
    /// The mod's files tab, where the download has to start for a free account.
    pub page_url: String,
    /// The link the website would hand over. A premium key can resolve it
    /// without the website, so the interface offers that as a direct download.
    pub nxm_url: String,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModUpdateReport {
    pub updates: Vec<ModUpdate>,
    /// Installed mods that carry Nexus provenance and can be checked at all.
    pub tracked: usize,
    /// When the newest of the stored results was taken, if anything is stored.
    pub checked_at: Option<String>,
    /// True when nothing was fetched and the report is the stored result.
    pub from_cache: bool,
    /// Mods matched to a Nexus page by their archive during this check.
    pub identified: usize,
    /// Installed mods that could not be matched, and so are not checked. Either
    /// they did not come from Nexus or the archive they came from is gone.
    pub unmatched: usize,
    /// Mods the user has taken out of checking, which are never looked up.
    pub ignored: usize,
    /// Why the check was incomplete, when it was.
    pub problem: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchReport {
    pub method: String,
}
