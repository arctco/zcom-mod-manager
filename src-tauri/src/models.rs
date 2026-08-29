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
    pub lua_mods: usize,
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
    pub files: Vec<ModFile>,
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
    pub deployment_key: String,
    pub files: Vec<PayloadFile>,
    pub packages: Vec<String>,
    pub verification: String,
    pub verification_details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModPreview {
    pub staging_id: String,
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
    pub retoc_path: Option<String>,
    pub log_level: String,
    pub advanced_package_names: bool,
    pub reduced_motion: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            game_path: None,
            retoc_path: None,
            log_level: "normal".into(),
            advanced_package_names: false,
            reduced_motion: false,
        }
    }
}
