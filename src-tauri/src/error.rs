use serde::Serialize;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Star Wars: Zero Company was not found. Locate the game in Settings.")]
    GameNotFound,
    #[error("That folder is not a valid Zero Company installation: {0}")]
    InvalidGamePath(String),
    #[error("The Steam manifest could not be read: {0}")]
    SteamManifestInvalid(String),
    #[error("The selected mod payload is not recognized.")]
    ModNotRecognized,
    #[error("The archive is unsafe: {0}")]
    UnsafeArchive(String),
    #[error("The mod is incomplete. Missing: {0}")]
    MissingIoStoreComponent(String),
    #[error("This archive contains overlapping IoStore containers that appear to be alternative variants: {0}. Extract it and install only one variant.")]
    AlternativeIoStoreVariants(String),
    #[error("IoStore validation failed: {0}")]
    RetocVerificationFailed(String),
    #[error("retoc 0.1.5 is required to validate IoStore mods. Configure it in Settings.")]
    RetocNotFound,
    #[error("UE4SS is not installed or its layout is incomplete.")]
    Ue4ssNotFound,
    #[error("That archive does not contain a UE4SS runtime. Expected dwmapi.dll next to a ue4ss folder.")]
    Ue4ssPackageNotRecognized,
    #[error("A different file already exists at {0}. It was not overwritten.")]
    DeploymentConflict(PathBuf),
    #[error("A managed file changed outside ZCOM Mod Manager: {0}")]
    ChecksumMismatch(PathBuf),
    #[error("The installation preview expired. Inspect the mod again.")]
    PreviewExpired,
    #[error("The proposed load order is invalid: {0}")]
    InvalidLoadOrder(String),
    #[error("Archive support requires the 7z command-line tool on this system.")]
    SevenZipNotFound,
    #[error("That is not a usable Nexus Mods link: {0}")]
    NexusLinkInvalid(String),
    #[error("That download link is for another game ({0}), so it was ignored.")]
    NexusLinkForAnotherGame(String),
    #[error("Nexus Mods rejected the API key. Check it in Settings.")]
    NexusUnauthorized,
    #[error("Nexus Mods rate limit reached. Try again later.")]
    NexusRateLimited,
    #[error("A Nexus Mods API key is required. Add one in Settings.")]
    NexusKeyMissing,
    #[error("Nexus Mods returned no download link. Non-premium downloads must start from the Mod Manager Download button on the website.")]
    NexusNoDownloadLink,
    #[error("Network request failed: {0}")]
    Network(String),
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("File operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("ZIP archive error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("{0}")]
    Other(String),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
