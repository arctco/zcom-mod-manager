use crate::{
    error::{AppError, Result},
    models::GameInfo,
};
use regex::Regex;
#[cfg(target_os = "linux")]
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

pub const APP_ID: &str = "2075800";

pub fn launch_url() -> String {
    format!("steam://run/{APP_ID}")
}

#[cfg(target_os = "linux")]
pub fn running_from_appimage() -> bool {
    std::env::var_os("APPIMAGE").is_some()
}

#[cfg(target_os = "linux")]
fn without_appdir_entries(value: &OsStr, appdir: &Path) -> Option<OsString> {
    let paths = std::env::split_paths(value)
        .filter(|entry| !entry.starts_with(appdir))
        .collect::<Vec<_>>();
    if paths.is_empty() {
        None
    } else {
        std::env::join_paths(paths).ok()
    }
}

/// Rebuilds the environment inherited from linuxdeploy's AppRun without the
/// mounted image's libraries, Python runtime, or toolkit plugins. Passing that
/// environment to xdg-open can otherwise start Steam against libraries below
/// `/tmp/.mount_*`, which breaks Steam even after the manager exits.
#[cfg(target_os = "linux")]
fn sanitized_appimage_environment<I>(appdir: &Path, environment: I) -> BTreeMap<OsString, OsString>
where
    I: IntoIterator<Item = (OsString, OsString)>,
{
    let mut clean = environment.into_iter().collect::<BTreeMap<_, _>>();
    for key in [
        "APPDIR",
        "APPIMAGE",
        "APPIMAGE_EXTRACT_AND_RUN",
        "APPIMAGE_SILENT_INSTALL",
        "ARGV0",
        "OWD",
        "PYTHONHOME",
        "PYTHONDONTWRITEBYTECODE",
        "GTK_DATA_PREFIX",
        "GTK_THEME",
        "GDK_BACKEND",
        "GTK_EXE_PREFIX",
        "GTK_IM_MODULE_FILE",
        "GDK_PIXBUF_MODULE_FILE",
    ] {
        clean.remove(OsStr::new(key));
    }
    for key in [
        "PATH",
        "LD_LIBRARY_PATH",
        "PYTHONPATH",
        "XDG_DATA_DIRS",
        "PERLLIB",
        "GSETTINGS_SCHEMA_DIR",
        "QT_PLUGIN_PATH",
        "GST_PLUGIN_SYSTEM_PATH",
        "GST_PLUGIN_SYSTEM_PATH_1_0",
        "GTK_PATH",
        "GIO_EXTRA_MODULES",
        "GI_TYPELIB_PATH",
    ] {
        let key = OsStr::new(key);
        if let Some(value) = clean.get(key).cloned() {
            match without_appdir_entries(&value, appdir) {
                Some(value) => {
                    clean.insert(key.to_os_string(), value);
                }
                None => {
                    clean.remove(key);
                }
            }
        }
    }
    // These accept formats other than a colon-separated path list. Removing
    // them only when AppRun pointed them into the mount avoids propagating a
    // bundled loader without discarding an unrelated user override.
    for key in ["LD_PRELOAD", "LD_AUDIT", "SSL_CERT_FILE", "SSL_CERT_DIR"] {
        let key = OsStr::new(key);
        if clean
            .get(key)
            .is_some_and(|value| Path::new(value).starts_with(appdir))
        {
            clean.remove(key);
        }
    }
    clean
}

#[cfg(target_os = "linux")]
fn executable_in_path(name: &str, environment: &BTreeMap<OsString, OsString>) -> Option<PathBuf> {
    environment
        .get(OsStr::new("PATH"))
        .into_iter()
        .flat_map(|value| std::env::split_paths(value))
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
        .or_else(|| {
            ["/usr/bin", "/bin", "/usr/local/bin"]
                .into_iter()
                .map(|directory| Path::new(directory).join(name))
                .find(|candidate| candidate.is_file())
        })
}

#[cfg(target_os = "linux")]
pub fn launch_from_appimage() -> Result<()> {
    let appdir = std::env::var_os("APPDIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| {
            AppError::Other(
                "The AppImage mount directory is unavailable, so Steam was not started with an unsafe environment. Start Steam manually and try again."
                    .into(),
            )
        })?;
    let environment = sanitized_appimage_environment(&appdir, std::env::vars_os());
    let opener = executable_in_path("xdg-open", &environment).ok_or_else(|| {
        AppError::Other(
            "The system URL opener (xdg-open) was not found. Start Steam manually and launch the game there."
                .into(),
        )
    })?;
    std::process::Command::new(opener)
        .arg(launch_url())
        .env_clear()
        .envs(environment)
        .spawn()
        .map_err(|error| AppError::Other(format!("Steam could not launch the game: {error}")))?;
    Ok(())
}

fn quoted_value(text: &str, key: &str) -> Option<String> {
    let pattern = format!(r#""{}"\s+"([^"]*)""#, regex::escape(key));
    Regex::new(&pattern)
        .ok()?
        .captures(text)?
        .get(1)
        .map(|m| m.as_str().to_string())
}

pub fn parse_manifest(text: &str) -> Result<(String, String, String)> {
    if !text.contains("AppState") {
        return Err(AppError::SteamManifestInvalid(
            "missing AppState section".into(),
        ));
    }
    let install_dir = quoted_value(text, "installdir")
        .ok_or_else(|| AppError::SteamManifestInvalid("missing installdir".into()))?;
    let build = quoted_value(text, "buildid")
        .ok_or_else(|| AppError::SteamManifestInvalid("missing buildid".into()))?;
    let state = quoted_value(text, "StateFlags").unwrap_or_else(|| "unknown".into());
    Ok((install_dir, build, state))
}

pub fn parse_library_folders(text: &str) -> Vec<PathBuf> {
    let mut result = BTreeSet::new();
    let re = Regex::new(r#""path"\s+"([^"]+)""#).expect("valid regex");
    for cap in re.captures_iter(text) {
        result.insert(PathBuf::from(cap[1].replace("\\\\", "\\")));
    }
    result.into_iter().collect()
}

pub fn valid_game(path: &Path) -> bool {
    path.join("SWZeroCompany/Binaries/Win64/SWZeroCompany.exe")
        .is_file()
        && path.join("SWZeroCompany/Content/Paks").is_dir()
}

pub fn from_manual(path: &Path) -> Result<GameInfo> {
    if !valid_game(path) {
        return Err(AppError::InvalidGamePath(path.display().to_string()));
    }
    let manifest = find_manifest_near_game(path)
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|t| parse_manifest(&t).ok());
    let compat = find_steam_root_for_game(path)
        .map(|r| r.join("steamapps/compatdata").join(APP_ID))
        .filter(|p| p.exists());
    Ok(GameInfo {
        detected: true,
        path: Some(path.display().to_string()),
        steam_build_id: manifest.as_ref().map(|m| m.1.clone()),
        install_state: manifest.map(|m| m.2),
        engine: "UE 5.6.1".into(),
        compat_data_path: compat.map(|p| p.display().to_string()),
        source: "manual".into(),
    })
}

fn find_steam_root_for_game(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|p| p.file_name().is_some_and(|n| n == "steamapps"))
        .and_then(|p| p.parent())
        .map(Path::to_path_buf)
}
fn find_manifest_near_game(path: &Path) -> Option<PathBuf> {
    find_steam_root_for_game(path)
        .map(|r| {
            r.join("steamapps")
                .join(format!("appmanifest_{APP_ID}.acf"))
        })
        .filter(|p| p.is_file())
}

pub fn discover_from_roots(roots: &[PathBuf]) -> Result<Option<GameInfo>> {
    let mut libraries = BTreeSet::new();
    for root in roots {
        libraries.insert(root.clone());
        if let Ok(text) = fs::read_to_string(root.join("steamapps/libraryfolders.vdf")) {
            libraries.extend(parse_library_folders(&text));
        }
    }
    for library in libraries {
        let manifest_path = library
            .join("steamapps")
            .join(format!("appmanifest_{APP_ID}.acf"));
        if !manifest_path.is_file() {
            continue;
        }
        let text = fs::read_to_string(&manifest_path)?;
        let (install_dir, build, state) = parse_manifest(&text)?;
        let game = library.join("steamapps/common").join(install_dir);
        if valid_game(&game) {
            let compat = library.join("steamapps/compatdata").join(APP_ID);
            return Ok(Some(GameInfo {
                detected: true,
                path: Some(game.display().to_string()),
                steam_build_id: Some(build),
                install_state: Some(state),
                engine: "UE 5.6.1".into(),
                compat_data_path: compat.exists().then(|| compat.display().to_string()),
                source: "automatic".into(),
            }));
        }
    }
    Ok(None)
}

pub fn discover() -> Result<Option<GameInfo>> {
    let mut roots = Vec::new();
    #[cfg(target_os = "linux")]
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".local/share/Steam"));
        roots.push(home.join(".steam/steam"));
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(program) = std::env::var("PROGRAMFILES(X86)") {
            roots.push(PathBuf::from(program).join("Steam"));
        }
        if let Ok(program) = std::env::var("PROGRAMFILES") {
            roots.push(PathBuf::from(program).join("Steam"));
        }
        for letter in b'C'..=b'Z' {
            roots.push(PathBuf::from(format!("{}:\\Steam", letter as char)));
            roots.push(PathBuf::from(format!("{}:\\SteamLibrary", letter as char)));
        }
    }
    discover_from_roots(&roots)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    const MANIFEST: &str = r#""AppState" { "appid" "2075800" "StateFlags" "4" "installdir" "Star Wars Zero Company" "buildid" "24874058" }"#;
    fn game(root: &Path) {
        fs::create_dir_all(
            root.join("steamapps/common/Star Wars Zero Company/SWZeroCompany/Binaries/Win64"),
        )
        .unwrap();
        fs::create_dir_all(
            root.join("steamapps/common/Star Wars Zero Company/SWZeroCompany/Content/Paks"),
        )
        .unwrap();
        fs::write(root.join("steamapps/common/Star Wars Zero Company/SWZeroCompany/Binaries/Win64/SWZeroCompany.exe"),b"").unwrap();
        fs::write(root.join("steamapps/appmanifest_2075800.acf"), MANIFEST).unwrap();
    }
    #[test]
    fn default_linux_library() {
        let d = tempdir().unwrap();
        game(d.path());
        let g = discover_from_roots(&[d.path().into()]).unwrap().unwrap();
        assert_eq!(g.steam_build_id.as_deref(), Some("24874058"));
    }
    #[test]
    fn additional_library() {
        let a = tempdir().unwrap();
        let b = tempdir().unwrap();
        game(b.path());
        fs::create_dir_all(a.path().join("steamapps")).unwrap();
        fs::write(
            a.path().join("steamapps/libraryfolders.vdf"),
            format!(
                r#""libraryfolders" {{ "0" {{ "path" "{}" }} }}"#,
                b.path().display()
            ),
        )
        .unwrap();
        assert!(discover_from_roots(&[a.path().into()]).unwrap().is_some());
    }
    #[test]
    fn missing_app() {
        let d = tempdir().unwrap();
        assert!(discover_from_roots(&[d.path().into()]).unwrap().is_none());
    }
    #[test]
    fn malformed_manifest() {
        assert!(parse_manifest("nope").is_err());
    }
    #[test]
    fn windows_paths_are_unescaped() {
        let v = r#""libraryfolders" { "1" { "path" "D:\\SteamLibrary" } }"#;
        assert_eq!(
            parse_library_folders(v),
            vec![PathBuf::from(r"D:\SteamLibrary")]
        );
    }

    #[test]
    fn default_windows_library_fixture() {
        let steam = tempdir().unwrap();
        game(steam.path());
        assert!(discover_from_roots(&[steam.path().to_path_buf()])
            .unwrap()
            .is_some());
    }

    #[test]
    fn additional_windows_library_fixture() {
        let steam = tempdir().unwrap();
        let additional = tempdir().unwrap();
        game(additional.path());
        fs::create_dir_all(steam.path().join("steamapps")).unwrap();
        fs::write(
            steam.path().join("steamapps/libraryfolders.vdf"),
            format!(
                r#""libraryfolders" {{ "1" {{ "path" "{}" }} }}"#,
                additional.path().display()
            ),
        )
        .unwrap();
        assert!(discover_from_roots(&[steam.path().to_path_buf()])
            .unwrap()
            .is_some());
    }

    #[test]
    fn launch_url_targets_zero_company_through_steam() {
        assert_eq!(launch_url(), "steam://run/2075800");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn appimage_environment_is_removed_without_losing_host_paths() {
        let appdir = Path::new("/tmp/.mount_ZCOM123");
        let environment = [
            ("APPIMAGE", "/downloads/ZCOM.AppImage"),
            ("APPDIR", "/tmp/.mount_ZCOM123"),
            ("PYTHONHOME", "/tmp/.mount_ZCOM123/usr"),
            (
                "PATH",
                "/tmp/.mount_ZCOM123/usr/bin:/usr/local/bin:/usr/bin",
            ),
            (
                "LD_LIBRARY_PATH",
                "/tmp/.mount_ZCOM123/usr/lib:/custom/lib:/usr/lib",
            ),
            (
                "XDG_DATA_DIRS",
                "/tmp/.mount_ZCOM123/usr/share:/usr/local/share:/usr/share",
            ),
            ("DISPLAY", ":0"),
        ]
        .into_iter()
        .map(|(key, value)| (OsString::from(key), OsString::from(value)));

        let clean = sanitized_appimage_environment(appdir, environment);

        assert!(!clean.contains_key(OsStr::new("APPIMAGE")));
        assert!(!clean.contains_key(OsStr::new("APPDIR")));
        assert!(!clean.contains_key(OsStr::new("PYTHONHOME")));
        assert_eq!(
            clean.get(OsStr::new("PATH")).unwrap(),
            OsStr::new("/usr/local/bin:/usr/bin")
        );
        assert_eq!(
            clean.get(OsStr::new("LD_LIBRARY_PATH")).unwrap(),
            OsStr::new("/custom/lib:/usr/lib")
        );
        assert_eq!(clean.get(OsStr::new("DISPLAY")).unwrap(), OsStr::new(":0"));
        assert!(clean
            .values()
            .all(|value| !value.to_string_lossy().contains("/tmp/.mount_ZCOM123")));
    }
}
