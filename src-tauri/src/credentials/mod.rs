//! Storage for the user's Nexus Mods API key.
//!
//! The key can download on the user's behalf and is rate-limited against their
//! account, so it belongs in the operating system's secret store rather than
//! beside the settings. Linux only has a secret store when a Secret Service
//! provider is running; on a machine without one the key falls back to the
//! application database and the interface says so plainly instead of pretending
//! the key is protected.

use crate::error::{AppError, Result};
use rusqlite::Connection;

const SERVICE: &str = "org.zcommodmanager.desktop";
const ACCOUNT: &str = "nexus-api-key";
const FALLBACK_SETTING: &str = "nexus_api_key";

/// Where the key actually ended up, so the interface can be honest about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Storage {
    /// The operating system secret store.
    Keyring,
    /// The application database, in plain text.
    Database,
}

fn entry() -> Option<keyring::Entry> {
    keyring::Entry::new(SERVICE, ACCOUNT).ok()
}

/// Stores the key, preferring the OS secret store. Returns where it landed.
pub fn store(conn: &Connection, key: &str) -> Result<Storage> {
    let key = key.trim();
    if key.is_empty() {
        return Err(AppError::Other("The API key is empty.".into()));
    }
    if let Some(entry) = entry() {
        if entry.set_password(key).is_ok() {
            // Never leave a plaintext copy behind once the keyring accepts it.
            let _ = crate::database::delete_setting(conn, FALLBACK_SETTING);
            return Ok(Storage::Keyring);
        }
    }
    crate::database::set_setting(conn, FALLBACK_SETTING, key)?;
    Ok(Storage::Database)
}

/// Reads the key, checking the secret store first.
pub fn load(conn: &Connection) -> Option<String> {
    if let Some(entry) = entry() {
        if let Ok(password) = entry.get_password() {
            if !password.trim().is_empty() {
                return Some(password);
            }
        }
    }
    crate::database::get_setting(conn, FALLBACK_SETTING)
        .ok()
        .flatten()
}

/// Reports where a stored key lives without revealing it.
pub fn location(conn: &Connection) -> Option<Storage> {
    if entry().and_then(|e| e.get_password().ok()).is_some() {
        return Some(Storage::Keyring);
    }
    crate::database::get_setting(conn, FALLBACK_SETTING)
        .ok()
        .flatten()
        .map(|_| Storage::Database)
}

/// Removes the key from both locations.
pub fn clear(conn: &Connection) -> Result<()> {
    if let Some(entry) = entry() {
        let _ = entry.delete_credential();
    }
    crate::database::delete_setting(conn, FALLBACK_SETTING)?;
    Ok(())
}
