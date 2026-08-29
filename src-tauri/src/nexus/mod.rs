//! Nexus Mods handoff.
//!
//! The manager never browses or searches Nexus. It reacts to an `nxm://` link
//! that the website produces when the user presses **Mod Manager Download**,
//! and it resolves that link with the user's own API key. A non-premium
//! account cannot obtain a download link any other way: the `key`/`expires`
//! pair is minted by the website and is the only authorisation the API accepts.

use crate::error::{AppError, Result};
use std::path::Path;

/// Zero Company's domain on Nexus Mods. A link for any other game is refused
/// rather than downloaded, so a stray association cannot make this manager
/// fetch content for a title it knows nothing about.
pub const GAME_DOMAIN: &str = "starwarszerocompany";
const API_ROOT: &str = "https://api.nexusmods.com/v1";

/// An `nxm://` link: `nxm://<game>/mods/<mod>/files/<file>?key=..&expires=..`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NxmLink {
    pub game_domain: String,
    pub mod_id: u64,
    pub file_id: u64,
    /// Minted by the website for non-premium downloads. Absent for a premium
    /// account, which may request a link without one.
    pub key: Option<String>,
    pub expires: Option<String>,
}

fn decode_component(value: &str) -> String {
    // Percent-decoding limited to what a Nexus key or expiry can contain.
    let bytes = value.as_bytes();
    let mut out = String::with_capacity(value.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&value[i + 1..i + 3], 16) {
                out.push(byte as char);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Parses an `nxm://` link and rejects anything that is not a Zero Company
/// mod file. Called before any network request is made.
pub fn parse_nxm(url: &str) -> Result<NxmLink> {
    let rest = url
        .strip_prefix("nxm://")
        .or_else(|| url.strip_prefix("NXM://"))
        .ok_or_else(|| AppError::NexusLinkInvalid("not an nxm:// link".into()))?;
    let (path, query) = match rest.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (rest, None),
    };
    let parts: Vec<&str> = path.trim_end_matches('/').split('/').collect();
    let [game_domain, "mods", mod_id, "files", file_id] = parts.as_slice() else {
        return Err(AppError::NexusLinkInvalid(
            "expected nxm://<game>/mods/<id>/files/<id>".into(),
        ));
    };
    if !game_domain.eq_ignore_ascii_case(GAME_DOMAIN) {
        return Err(AppError::NexusLinkForAnotherGame(
            (*game_domain).to_string(),
        ));
    }
    let parse_id = |value: &str, what: &str| {
        value
            .parse::<u64>()
            .map_err(|_| AppError::NexusLinkInvalid(format!("{what} is not a number")))
    };
    let mut key = None;
    let mut expires = None;
    for pair in query
        .unwrap_or_default()
        .split('&')
        .filter(|p| !p.is_empty())
    {
        match pair.split_once('=') {
            Some(("key", value)) => key = Some(decode_component(value)),
            Some(("expires", value)) => expires = Some(decode_component(value)),
            _ => {}
        }
    }
    Ok(NxmLink {
        game_domain: game_domain.to_ascii_lowercase(),
        mod_id: parse_id(mod_id, "mod id")?,
        file_id: parse_id(file_id, "file id")?,
        key,
        expires,
    })
}

/// Identifies this application to Nexus, which their terms require.
fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(format!(
            "ZCOM Mod Manager/{} ({}; {})",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH
        ))
        .build()
        .map_err(|e| AppError::Network(e.to_string()))
}

async fn get_json(api_key: &str, url: &str) -> Result<serde_json::Value> {
    let response = client()?
        .get(url)
        .header("apikey", api_key)
        .header("Application-Name", "ZCOM Mod Manager")
        .header("Application-Version", env!("CARGO_PKG_VERSION"))
        .send()
        .await
        .map_err(|e| AppError::Network(e.to_string()))?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(AppError::NexusUnauthorized);
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(AppError::NexusRateLimited);
    }
    if !status.is_success() {
        return Err(AppError::Network(format!(
            "Nexus Mods replied {}",
            status.as_u16()
        )));
    }
    response
        .json()
        .await
        .map_err(|e| AppError::Network(e.to_string()))
}

/// Who the stored key belongs to, shown in Settings so the user can confirm
/// the key took effect without leaving the application.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub name: String,
    pub premium: bool,
}

pub async fn validate(api_key: &str) -> Result<Account> {
    let body = get_json(api_key, &format!("{API_ROOT}/users/validate.json")).await?;
    Ok(Account {
        name: body["name"].as_str().unwrap_or("Nexus user").to_string(),
        premium: body["is_premium"].as_bool().unwrap_or(false),
    })
}

/// Metadata for the file behind an `nxm://` link, used to name the download
/// and to show the user what is about to be fetched.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub file_name: String,
    pub name: String,
    pub version: Option<String>,
    pub size_bytes: u64,
}

pub async fn file_info(api_key: &str, link: &NxmLink) -> Result<FileInfo> {
    let url = format!(
        "{API_ROOT}/games/{}/mods/{}/files/{}.json",
        link.game_domain, link.mod_id, link.file_id
    );
    let body = get_json(api_key, &url).await?;
    let file_name = body["file_name"]
        .as_str()
        .ok_or_else(|| AppError::Network("Nexus Mods returned no file name".into()))?
        .to_string();
    Ok(FileInfo {
        name: body["name"].as_str().unwrap_or(&file_name).to_string(),
        version: body["version"].as_str().map(str::to_string),
        // `size` is in kilobytes; `size_in_bytes` is present but nullable.
        size_bytes: body["size_in_bytes"]
            .as_u64()
            .or_else(|| body["size"].as_u64().map(|kb| kb * 1024))
            .unwrap_or(0),
        file_name,
    })
}

/// Exchanges the link for a time-limited CDN URL. Without `key`/`expires` this
/// only succeeds for a premium account, which is why the handoff exists.
pub async fn download_link(api_key: &str, link: &NxmLink) -> Result<String> {
    let mut url = format!(
        "{API_ROOT}/games/{}/mods/{}/files/{}/download_link.json",
        link.game_domain, link.mod_id, link.file_id
    );
    if let (Some(key), Some(expires)) = (&link.key, &link.expires) {
        url.push_str(&format!("?key={key}&expires={expires}"));
    }
    let body = get_json(api_key, &url).await?;
    body.as_array()
        .and_then(|list| list.first())
        .and_then(|entry| entry["URI"].as_str())
        .map(str::to_string)
        .ok_or(AppError::NexusNoDownloadLink)
}

/// Streams the file to `destination`, reporting progress so a large download
/// does not look like a frozen window. Returns the number of bytes written.
pub async fn download_to<F>(url: &str, destination: &Path, mut progress: F) -> Result<u64>
where
    F: FnMut(u64, Option<u64>),
{
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(AppError::Io)?;
    }
    let response = client()?
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::Network(e.to_string()))?;
    if !response.status().is_success() {
        return Err(AppError::Network(format!(
            "the download server replied {}",
            response.status().as_u16()
        )));
    }
    let total = response.content_length();
    let mut file = tokio::fs::File::create(destination)
        .await
        .map_err(AppError::Io)?;
    let mut written = 0u64;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| AppError::Network(e.to_string()))?;
        file.write_all(&chunk).await.map_err(AppError::Io)?;
        written += chunk.len() as u64;
        progress(written, total);
    }
    file.flush().await.map_err(AppError::Io)?;
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_website_link() {
        let link = parse_nxm(
            "nxm://starwarszerocompany/mods/9/files/42?key=abc123&expires=1799999999&user_id=7",
        )
        .unwrap();
        assert_eq!(link.mod_id, 9);
        assert_eq!(link.file_id, 42);
        assert_eq!(link.key.as_deref(), Some("abc123"));
        assert_eq!(link.expires.as_deref(), Some("1799999999"));
    }

    #[test]
    fn parses_a_premium_link_without_a_key() {
        let link = parse_nxm("nxm://starwarszerocompany/mods/9/files/42").unwrap();
        assert!(link.key.is_none() && link.expires.is_none());
    }

    #[test]
    fn refuses_a_link_for_another_game() {
        assert!(matches!(
            parse_nxm("nxm://skyrimspecialedition/mods/1/files/2"),
            Err(AppError::NexusLinkForAnotherGame(_))
        ));
    }

    #[test]
    fn refuses_malformed_links() {
        for bad in [
            "https://www.nexusmods.com/starwarszerocompany/mods/9",
            "nxm://starwarszerocompany/mods/9",
            "nxm://starwarszerocompany/collections/9/files/2",
            "nxm://starwarszerocompany/mods/nine/files/2",
        ] {
            assert!(parse_nxm(bad).is_err(), "{bad} should be refused");
        }
    }

    #[test]
    fn percent_decodes_key_values() {
        let link =
            parse_nxm("nxm://starwarszerocompany/mods/9/files/42?key=a%2Bb%3Dc&expires=1").unwrap();
        assert_eq!(link.key.as_deref(), Some("a+b=c"));
    }
}
