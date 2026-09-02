//! Nexus Mods handoff.
//!
//! The manager never browses or searches Nexus. It reacts to an `nxm://` link
//! that the website produces when the user presses **Mod Manager Download**,
//! and it resolves that link with the user's own API key. A non-premium
//! account cannot obtain a download link any other way: the `key`/`expires`
//! pair is minted by the website and is the only authorisation the API accepts.

use crate::error::{AppError, Result};
use std::path::Path;
use std::sync::LazyLock;

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

/// Like `get_json`, but a miss is an answer rather than a failure: Nexus
/// replies 404 to an MD5 it does not know.
async fn get_json_optional(api_key: &str, url: &str) -> Result<Option<serde_json::Value>> {
    match get_json(api_key, url).await {
        Ok(body) => Ok(Some(body)),
        Err(AppError::Network(message)) if message.contains("404") => Ok(None),
        Err(error) => Err(error),
    }
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

/// A file Nexus currently offers for a mod.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteFile {
    pub file_id: u64,
    pub file_name: String,
    pub name: String,
    pub version: Option<String>,
    pub category_id: Option<u64>,
    pub category_name: Option<String>,
    pub uploaded_timestamp: u64,
}

impl RemoteFile {
    /// An author moves a superseded file to `OLD_VERSION`, and Nexus reports an
    /// archived or removed file with a null category. None of those is on offer.
    fn offered(&self) -> bool {
        let by_id = self.category_id.is_none_or(|id| !matches!(id, 4 | 6 | 7));
        let by_name = self.category_name.as_deref().is_some_and(|name| {
            !name.eq_ignore_ascii_case("OLD_VERSION")
                && !name.eq_ignore_ascii_case("ARCHIVED")
                && !name.eq_ignore_ascii_case("DELETED")
        });
        by_id && by_name
    }

    /// `MAIN` and `UPDATE` are the categories that carry a new version of the
    /// mod itself. An optional extra is a separate download, never an upgrade.
    fn primary(&self) -> bool {
        match self.category_id {
            Some(id) => matches!(id, 1 | 2),
            None => self.category_name.as_deref().is_some_and(|name| {
                name.eq_ignore_ascii_case("MAIN") || name.eq_ignore_ascii_case("UPDATE")
            }),
        }
    }
}

fn parse_files(body: &serde_json::Value) -> Vec<RemoteFile> {
    body["files"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|entry| {
            let file_name = entry["file_name"].as_str()?.to_string();
            Some(RemoteFile {
                file_id: entry["file_id"].as_u64()?,
                name: entry["name"].as_str().unwrap_or(&file_name).to_string(),
                version: entry["version"].as_str().map(str::to_string),
                category_id: entry["category_id"].as_u64(),
                category_name: entry["category_name"].as_str().map(str::to_string),
                uploaded_timestamp: entry["uploaded_timestamp"].as_u64().unwrap_or(0),
                file_name,
            })
        })
        .collect()
}

/// The file an update would come from: the newest upload among the categories
/// that carry the mod itself, falling back to whatever else is still offered
/// for a mod that files everything under another category. File ids increase
/// with every upload, so they settle a tie between equal timestamps.
pub fn newest_offered(files: &[RemoteFile]) -> Option<&RemoteFile> {
    let offered: Vec<&RemoteFile> = files.iter().filter(|file| file.offered()).collect();
    let mut candidates: Vec<&RemoteFile> = offered
        .iter()
        .copied()
        .filter(|file| file.primary())
        .collect();
    if candidates.is_empty() {
        candidates = offered;
    }
    candidates
        .into_iter()
        .max_by_key(|file| (file.uploaded_timestamp, file.file_id))
}

static DISPLAY_VERSION: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?i)\bv?\d+(?:\.\d+)+\b").expect("static version regex is valid")
});

/// A stable label for one selectable file variant. Authors commonly keep the
/// display name and change only a `v1.2.3` token between uploads. Numeric
/// choices such as `150`, `200`, `1x`, and `2x` deliberately remain, as do
/// `Manual` and `Mod Manager`, because those distinguish independent files.
fn variant_key(name: &str) -> String {
    DISPLAY_VERSION
        .replace_all(name, " ")
        .to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Finds the newest currently offered upload in the same selectable file
/// variant as the exact Nexus file that was installed.
///
/// A Nexus page can contain several mutually exclusive `MAIN` files and can
/// also keep versioned choices under `OPTIONAL`. Comparing every install to one
/// page-wide newest id therefore creates both false positives and missed
/// updates. The installed file's display name supplies the lineage; if Nexus no
/// longer returns that file, no relationship is invented.
pub fn newest_for_installed(files: &[RemoteFile], installed_file_id: u64) -> Option<&RemoteFile> {
    let installed = files
        .iter()
        .find(|file| file.file_id == installed_file_id)?;
    let installed_key = variant_key(&installed.name);
    files
        .iter()
        .filter(|file| file.offered() && variant_key(&file.name) == installed_key)
        .max_by_key(|file| (file.uploaded_timestamp, file.file_id))
}

/// Whether the offered file is something the installed one does not already
/// have. Nexus issues file ids in upload order, so a larger one is always later.
pub fn is_newer(latest_file_id: u64, installed_file_id: u64) -> bool {
    latest_file_id > installed_file_id
}

pub async fn files(api_key: &str, mod_id: u64) -> Result<Vec<RemoteFile>> {
    let url = format!("{API_ROOT}/games/{GAME_DOMAIN}/mods/{mod_id}/files.json");
    Ok(parse_files(&get_json(api_key, &url).await?))
}

/// The mod and file an archive was uploaded as, if Nexus recognises its MD5.
///
/// This is how a mod installed before its provenance was recorded, or from an
/// archive the user downloaded in a browser, is matched to its Nexus page. Only
/// the uploaded archive is indexed, so it has to be the file as downloaded.
pub async fn md5_search(api_key: &str, md5: &str) -> Result<Option<(u64, u64)>> {
    let url = format!("{API_ROOT}/games/{GAME_DOMAIN}/mods/md5_search/{md5}.json");
    let Some(body) = get_json_optional(api_key, &url).await? else {
        return Ok(None);
    };
    Ok(first_md5_match(&body))
}

/// The first `{ mod, file_details }` pair in an MD5 search response. An archive
/// can in principle be uploaded to several mods; the first is the match.
fn first_md5_match(body: &serde_json::Value) -> Option<(u64, u64)> {
    body.as_array()?.iter().find_map(|entry| {
        Some((
            entry["mod"]["mod_id"].as_u64()?,
            entry["file_details"]["file_id"].as_u64()?,
        ))
    })
}

/// The mod id in whatever a user pastes: a mod page address, an `nxm://` link,
/// or the number on its own. A link for another game is refused here too.
pub fn parse_mod_reference(input: &str) -> Result<u64> {
    let text = input.trim();
    if text.is_empty() {
        return Err(AppError::NexusLinkInvalid("nothing was entered".into()));
    }
    if let Ok(id) = text.parse::<u64>() {
        return Ok(id);
    }
    if let Ok(link) = parse_nxm(text) {
        return Ok(link.mod_id);
    }
    // https://www.nexusmods.com/games/<game>/mods/<id>, and the older
    // https://www.nexusmods.com/<game>/mods/<id>, with anything after it.
    let rest = text
        .split_once("nexusmods.com/")
        .map(|(_, rest)| rest)
        .ok_or_else(|| {
            AppError::NexusLinkInvalid("expected a Nexus Mods address or a mod id".into())
        })?;
    let parts: Vec<&str> = rest.split(['/', '?', '#']).collect();
    let position = parts
        .iter()
        .position(|part| *part == "mods")
        .ok_or_else(|| AppError::NexusLinkInvalid("that address names no mod".into()))?;
    let game = parts[..position]
        .iter()
        .rev()
        .find(|part| !part.is_empty() && **part != "games");
    if let Some(game) = game {
        if !game.eq_ignore_ascii_case(GAME_DOMAIN) {
            return Err(AppError::NexusLinkForAnotherGame((*game).to_string()));
        }
    }
    parts
        .get(position + 1)
        .and_then(|id| id.parse::<u64>().ok())
        .ok_or_else(|| AppError::NexusLinkInvalid("that address names no mod".into()))
}

/// The file an installed version most likely came from: the one Nexus lists
/// under that exact version, and otherwise the newest it still offers.
pub fn file_for_version<'a>(
    files: &'a [RemoteFile],
    installed_version: Option<&str>,
) -> Option<&'a RemoteFile> {
    installed_version
        .map(str::trim)
        .filter(|version| !version.is_empty())
        .and_then(|version| {
            files
                .iter()
                .filter(|file| {
                    file.version
                        .as_deref()
                        .is_some_and(|candidate| candidate.trim().eq_ignore_ascii_case(version))
                })
                // Several files can share a version; the earliest is the one an
                // installed copy is least likely to be ahead of.
                .min_by_key(|file| file.file_id)
        })
        .or_else(|| newest_offered(files))
}

/// A mod's page on Nexus Mods.
///
/// A mod lives at `/<game>/mods/<id>`. The `/games/<game>/...` form addresses
/// the game itself and does not resolve to a mod, so it is accepted from a user
/// who pastes it but is never produced here.
pub fn mod_url(mod_id: u64) -> String {
    format!("https://www.nexusmods.com/{GAME_DOMAIN}/mods/{mod_id}")
}

/// The mod's files tab. A free account has to start the download there, so this
/// is where an available update sends the user.
pub fn mod_files_url(mod_id: u64) -> String {
    format!("{}?tab=files", mod_url(mod_id))
}

/// The link the website's Mod Manager Download button would produce, without
/// the key it mints for a free account.
pub fn nxm_url(mod_id: u64, file_id: u64) -> String {
    format!("nxm://{GAME_DOMAIN}/mods/{mod_id}/files/{file_id}")
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

    fn remote(file_id: u64, category_id: u64, uploaded: u64) -> RemoteFile {
        RemoteFile {
            file_id,
            file_name: format!("mod-{file_id}.zip"),
            name: format!("Mod {file_id}"),
            version: Some("1.0".into()),
            category_id: Some(category_id),
            category_name: Some(match category_id {
                1 => "MAIN".into(),
                2 => "UPDATE".into(),
                3 => "OPTIONAL".into(),
                _ => "OLD_VERSION".to_string(),
            }),
            uploaded_timestamp: uploaded,
        }
    }

    #[test]
    fn the_newest_main_file_is_the_update() {
        let files = [remote(10, 1, 100), remote(20, 1, 200), remote(15, 1, 150)];
        assert_eq!(newest_offered(&files).unwrap().file_id, 20);
    }

    #[test]
    fn superseded_and_archived_files_are_never_offered() {
        let files = [remote(30, 4, 900), remote(20, 1, 200)];
        assert_eq!(newest_offered(&files).unwrap().file_id, 20);
        let archived = [RemoteFile {
            category_id: None,
            category_name: None,
            ..remote(40, 1, 999)
        }];
        assert!(newest_offered(&archived).is_none());
    }

    #[test]
    fn an_optional_extra_is_not_an_upgrade_of_the_mod() {
        let files = [remote(50, 3, 900), remote(20, 1, 200)];
        assert_eq!(newest_offered(&files).unwrap().file_id, 20);
    }

    #[test]
    fn update_follows_the_installed_variant_instead_of_another_main_file() {
        let mut old_150 = remote(10, 4, 100);
        old_150.name = "More Enemies 150 v0.1.0 - ZCOM Mod Manager".into();
        let mut new_150 = remote(20, 1, 200);
        new_150.name = "More Enemies 150 v0.2.0 - ZCOM Mod Manager".into();
        let mut new_200 = remote(30, 1, 300);
        new_200.name = "More Enemies 200 v0.2.0 - ZCOM Mod Manager".into();
        let files = [old_150, new_150, new_200];

        assert_eq!(newest_for_installed(&files, 10).unwrap().file_id, 20);
        assert_eq!(newest_for_installed(&files, 20).unwrap().file_id, 20);
    }

    #[test]
    fn optional_variants_can_receive_their_own_updates() {
        let mut old_2x = remote(40, 4, 100);
        old_2x.name = "Stronger with the Force - 2x".into();
        let mut new_2x = remote(60, 3, 300);
        new_2x.name = "Stronger with the Force - 2x".into();
        let mut main_15x = remote(50, 1, 200);
        main_15x.name = "Stronger with the Force - 1.5x".into();
        let files = [old_2x, main_15x, new_2x];

        assert_eq!(newest_for_installed(&files, 40).unwrap().file_id, 60);
    }

    #[test]
    fn a_missing_installed_file_does_not_invent_a_variant_match() {
        let files = [remote(20, 1, 200), remote(30, 1, 300)];
        assert!(newest_for_installed(&files, 10).is_none());
    }

    #[test]
    fn a_mod_filing_everything_elsewhere_still_reports_its_newest() {
        let files = [remote(50, 3, 900), remote(40, 5, 800)];
        assert_eq!(newest_offered(&files).unwrap().file_id, 50);
    }

    #[test]
    fn only_a_later_upload_counts_as_newer() {
        assert!(is_newer(21, 20));
        assert!(!is_newer(20, 20));
        assert!(!is_newer(19, 20));
    }

    #[test]
    fn reads_the_file_list_a_mod_page_returns() {
        let body = serde_json::json!({"files":[
            {"file_id":7,"file_name":"a.zip","name":"A","version":"1.2","category_id":1,"category_name":"MAIN","uploaded_timestamp":100},
            {"file_id":8,"name":"missing a file name"}
        ]});
        let files = parse_files(&body);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].version.as_deref(), Some("1.2"));
        assert_eq!(files[0].file_name, "a.zip");
    }

    #[test]
    fn reads_a_mod_reference_a_user_can_paste() {
        for text in [
            "34",
            " 34 ",
            "nxm://starwarszerocompany/mods/34/files/9",
            "https://www.nexusmods.com/games/starwarszerocompany/mods/34",
            "https://www.nexusmods.com/games/starwarszerocompany/mods/34?tab=files",
            "https://www.nexusmods.com/starwarszerocompany/mods/34/",
            "www.nexusmods.com/games/starwarszerocompany/mods/34#comments",
        ] {
            assert_eq!(parse_mod_reference(text).unwrap(), 34, "{text}");
        }
    }

    #[test]
    fn refuses_a_reference_to_another_game_or_to_nothing() {
        assert!(matches!(
            parse_mod_reference("https://www.nexusmods.com/games/skyrimspecialedition/mods/34"),
            Err(AppError::NexusLinkForAnotherGame(_))
        ));
        for bad in [
            "",
            "the mod with the red icon",
            "https://www.nexusmods.com/games/starwarszerocompany/mods",
            "https://example.com/mods/34",
        ] {
            assert!(parse_mod_reference(bad).is_err(), "{bad} should be refused");
        }
    }

    #[test]
    fn reads_the_mod_and_file_an_md5_belongs_to() {
        let body = serde_json::json!([
            {"mod": {"mod_id": 34, "name": "ZCUnlocked"}, "file_details": {"file_id": 260}}
        ]);
        assert_eq!(first_md5_match(&body), Some((34, 260)));
        assert_eq!(first_md5_match(&serde_json::json!([])), None);
        // A reply that is not the shape we expect is a miss, never a panic.
        assert_eq!(first_md5_match(&serde_json::json!({"code": 404})), None);
    }

    #[test]
    fn links_an_installed_version_to_the_file_that_carries_it() {
        let mut old = remote(10, 4, 100);
        old.version = Some("1.0".into());
        let mut current = remote(20, 1, 200);
        current.version = Some("1.3".into());
        let files = [old, current];
        assert_eq!(file_for_version(&files, Some("1.0")).unwrap().file_id, 10);
        assert_eq!(file_for_version(&files, Some(" 1.3 ")).unwrap().file_id, 20);
        // An unknown or absent version falls back to what is on offer now, so
        // linking never reports an update the user has not actually missed.
        assert_eq!(file_for_version(&files, Some("9.9")).unwrap().file_id, 20);
        assert_eq!(file_for_version(&files, None).unwrap().file_id, 20);
    }

    #[test]
    fn the_direct_link_is_one_this_manager_would_accept() {
        let link = parse_nxm(&nxm_url(9, 42)).unwrap();
        assert_eq!((link.mod_id, link.file_id), (9, 42));
        assert_eq!(
            mod_url(9),
            "https://www.nexusmods.com/starwarszerocompany/mods/9"
        );
        assert_eq!(
            mod_files_url(9),
            "https://www.nexusmods.com/starwarszerocompany/mods/9?tab=files"
        );
        // The other form is a game address, not a mod one. Reading it back is
        // fine; handing it to someone as a mod page is not.
        assert!(!mod_url(9).contains("/games/"));
    }

    #[test]
    fn percent_decodes_key_values() {
        let link =
            parse_nxm("nxm://starwarszerocompany/mods/9/files/42?key=a%2Bb%3Dc&expires=1").unwrap();
        assert_eq!(link.key.as_deref(), Some("a+b=c"));
    }
}
