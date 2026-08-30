use crate::{
    archives,
    error::{AppError, Result},
    models::{Ue4ssInfo, Ue4ssInstallReport},
};
use std::{
    fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

/// The community UE4SS build that is tested against Star Wars: Zero Company.
pub const DOWNLOAD_URL: &str = "https://www.nexusmods.com/starwarszerocompany/mods/9";

/// Files under `Binaries/Win64` that the user edits and that a UE4SS package
/// also ships, so they must survive an upgrade: the runtime configuration and
/// the load-order lists.
///
/// This deliberately does not cover all of `ue4ss/Mods/`. A package ships its
/// own Lua mods (BPModLoaderMod, ConsoleCommandsMod, and friends) that belong
/// to the runtime and have to move with it, or an upgraded `UE4SS.dll` ends up
/// paired with stale scripts. Lua mods the user installed are safe without a
/// rule: they are not in the package, and files that are not in the package are
/// never touched.
fn is_user_owned(relative: &Path) -> bool {
    let normalized = relative
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "ue4ss/ue4ss-settings.ini" | "ue4ss/mods/mods.txt" | "ue4ss/mods/mods.json"
    ) || (normalized.starts_with("ue4ss/mods/") && normalized.ends_with("/load_order.txt"))
}

pub fn base(game: &Path) -> PathBuf {
    game.join("SWZeroCompany/Binaries/Win64")
}
pub fn detect(game: Option<&Path>, compat_data: Option<&Path>) -> Ue4ssInfo {
    let Some(game) = game else {
        return Ue4ssInfo::default();
    };
    let win64 = base(game);
    let root = win64.join("ue4ss");
    let dll = win64.join("dwmapi.dll").is_file();
    let core = root.join("UE4SS.dll").is_file();
    let mods = root.join("Mods");
    let installed = dll || core || root.exists();
    let healthy = dll && core && mods.is_dir();
    let mod_count = if mods.is_dir() {
        installed_mod_folders(&mods)
    } else {
        0
    };
    let proton_override = compat_data.map(|_| detect_proton_override(game));
    let message = if installed && !healthy {
        Some("The UE4SS layout is incomplete (dwmapi.dll, UE4SS.dll, or Mods is missing).".into())
    } else if installed && proton_override == Some(false) {
        Some("UE4SS may not load under Proton. Add WINEDLLOVERRIDES=\"dwmapi=n,b\" %command% to Steam launch options.".into())
    } else {
        None
    };
    Ue4ssInfo {
        installed,
        healthy,
        mod_count,
        log_found: root.join("UE4SS.log").is_file(),
        proton_override,
        message,
    }
}

/// Counts the mod folders UE4SS will load. A mod ships Lua scripts, a native
/// DLL, or both, so counting `main.lua` alone missed every DLL mod.
fn installed_mod_folders(mods: &Path) -> usize {
    WalkDir::new(mods)
        .max_depth(4)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| {
            let path = entry.path();
            let parent = path.parent()?;
            let holder = parent.file_name()?.to_str()?.to_ascii_lowercase();
            let payload = match holder.as_str() {
                "scripts" => entry.file_name().eq_ignore_ascii_case("main.lua"),
                "dlls" => path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("dll")),
                _ => false,
            };
            payload.then(|| parent.parent().map(Path::to_path_buf))?
        })
        .collect::<std::collections::BTreeSet<_>>()
        .len()
}

fn detect_proton_override(game: &Path) -> bool {
    let steamapps = game
        .ancestors()
        .find(|p| p.file_name().is_some_and(|n| n == "steamapps"));
    let Some(steam_root) = steamapps.and_then(Path::parent) else {
        return false;
    };
    let userdata = steam_root.join("userdata");
    WalkDir::new(userdata)
        .max_depth(4)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_name() == "localconfig.vdf")
        .filter_map(|e| fs::read_to_string(e.path()).ok())
        .any(|text| text.contains("2075800") && text.to_ascii_lowercase().contains("dwmapi=n,b"))
}

/// Case-insensitive lookup of a direct child, because archive casing varies
/// between `ue4ss/` and `UE4SS/`.
fn child(directory: &Path, name: &str) -> Option<PathBuf> {
    fs::read_dir(directory)
        .ok()?
        .filter_map(|e| e.ok())
        .find_map(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|found| found.eq_ignore_ascii_case(name))
                .then(|| entry.path())
        })
}

/// Finds the directory inside a staged archive that maps onto `Binaries/Win64`.
/// A Zero Company UE4SS package contains `dwmapi.dll` next to a `ue4ss` folder,
/// but publishers frequently nest that pair one or two levels deep.
pub(crate) fn layout_root(staged: &Path) -> Option<PathBuf> {
    WalkDir::new(staged)
        .max_depth(4)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_dir())
        .map(|entry| entry.path().to_path_buf())
        .find(|directory| {
            child(directory, "dwmapi.dll").is_some_and(|p| p.is_file())
                && child(directory, "ue4ss").is_some_and(|p| p.is_dir())
        })
}

/// Installs a user-downloaded UE4SS package into the game's `Binaries/Win64`
/// folder. The archive is staged through the same sandbox used for mods, so
/// traversal paths and symbolic links are rejected before anything is copied.
pub fn install_from(archive: &Path, game: &Path, cache: &Path) -> Result<Ue4ssInstallReport> {
    let staging = archives::stage(archive, cache)?;
    let result = install_staged(&staging.root, game);
    let _ = fs::remove_dir_all(&staging.root);
    result
}

pub(crate) fn install_staged(staged: &Path, game: &Path) -> Result<Ue4ssInstallReport> {
    let source = layout_root(staged).ok_or(AppError::Ue4ssPackageNotRecognized)?;
    let win64 = base(game);
    if !win64.is_dir() {
        return Err(AppError::GameNotFound);
    }
    let mut installed = 0usize;
    let mut preserved = Vec::new();
    for entry in WalkDir::new(&source).follow_links(false) {
        let entry = entry.map_err(|e| AppError::Other(e.to_string()))?;
        let relative = entry
            .path()
            .strip_prefix(&source)
            .map_err(|e| AppError::Other(e.to_string()))?;
        if relative.as_os_str().is_empty() || entry.file_type().is_dir() {
            continue;
        }
        let target = win64.join(relative);
        if is_user_owned(relative) && target.exists() {
            preserved.push(relative.display().to_string());
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(entry.path(), &target)?;
        installed += 1;
    }
    let info = detect(Some(game), None);
    if !info.healthy {
        return Err(AppError::Ue4ssPackageNotRecognized);
    }
    Ok(Ue4ssInstallReport {
        installed,
        preserved,
        proton_hint: cfg!(unix),
    })
}

/// The mod name an entry line declares, if it declares one.
fn entry_name(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with(';') {
        return None;
    }
    let name = trimmed.split([':', '=']).next().unwrap_or("").trim();
    (!name.is_empty()).then_some(name)
}

/// Rewrites the managed block of `mods.txt` in the given order.
///
/// UE4SS starts mods in the order this file lists them, so the order is the
/// mechanism, not a label. Everything the manager does not own — the comments,
/// the blank lines, and the runtime's own entries, including the "do not move
/// up" keybind block — keeps its position and its relative order; the managed
/// entries are written after it, in the order given. Any managed name already
/// present elsewhere in the file is removed from that position first, so a mod
/// is never listed twice.
pub fn write_order(game: &Path, ordered: &[(String, bool)]) -> Result<()> {
    let mods = base(game).join("ue4ss/Mods");
    if !mods.is_dir() {
        return Err(AppError::Ue4ssNotFound);
    }
    let path = mods.join("mods.txt");
    let original = fs::read_to_string(&path).unwrap_or_default();
    let line_ending = if original.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let managed = |name: &str| {
        ordered
            .iter()
            .any(|(key, _)| key.eq_ignore_ascii_case(name))
    };
    let mut output: Vec<String> = original
        .lines()
        .filter(|line| !entry_name(line).is_some_and(managed))
        .map(str::to_string)
        .collect();
    while output.last().is_some_and(|line| line.trim().is_empty()) {
        output.pop();
    }
    for (key, enabled) in ordered {
        output.push(format!("{key} : {}", if *enabled { 1 } else { 0 }));
    }
    let mut value = output.join(line_ending);
    if !value.is_empty() {
        value.push_str(line_ending)
    }
    fs::write(path, value)?;
    Ok(())
}

pub fn update_mods_txt(game: &Path, name: &str, enabled: bool) -> Result<()> {
    let mods = base(game).join("ue4ss/Mods");
    if !mods.is_dir() {
        return Err(AppError::Ue4ssNotFound);
    }
    let path = mods.join("mods.txt");
    let original = fs::read_to_string(&path).unwrap_or_default();
    let line_ending = if original.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut found = false;
    let mut output = Vec::new();
    for line in original.lines() {
        let entry = entry_name(line).unwrap_or("");
        if entry.eq_ignore_ascii_case(name) {
            let indentation = &line[..line.len() - line.trim_start().len()];
            output.push(format!(
                "{indentation}{name} : {}",
                if enabled { 1 } else { 0 }
            ));
            found = true
        } else {
            output.push(line.to_string())
        }
    }
    if !found {
        output.push(format!("{name} : {}", if enabled { 1 } else { 0 }));
    }
    let mut value = output.join(line_ending);
    if !value.is_empty() {
        value.push_str(line_ending)
    }
    fs::write(path, value)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    fn write(path: &Path, body: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    #[test]
    fn installs_a_package_and_keeps_existing_lua_mods() {
        let d = tempdir().unwrap();
        let package = d.path().join("pkg/UE4SS-SWZC/Binaries/Win64");
        write(&package.join("dwmapi.dll"), "loader");
        write(&package.join("ue4ss/UE4SS.dll"), "core");
        write(&package.join("ue4ss/UE4SS-settings.ini"), "shipped");
        write(&package.join("ue4ss/Mods/mods.txt"), "Shipped : 1\n");
        write(
            &package.join("ue4ss/Mods/ShippedMod/Scripts/main.lua"),
            "new",
        );
        let game = d.path().join("game");
        let win64 = base(&game);
        write(&win64.join("ue4ss/Mods/mods.txt"), "MyMod : 1\n");
        write(&win64.join("ue4ss/UE4SS-settings.ini"), "mine");
        write(
            &win64.join("ue4ss/Mods/ShippedMod/Scripts/main.lua"),
            "stale",
        );
        write(&win64.join("ue4ss/Mods/MyMod/Scripts/main.lua"), "mine");

        let report = install_staged(d.path().join("pkg").as_path(), &game).unwrap();

        assert!(win64.join("dwmapi.dll").is_file());
        assert!(win64.join("ue4ss/UE4SS.dll").is_file());
        assert_eq!(
            fs::read_to_string(win64.join("ue4ss/Mods/mods.txt")).unwrap(),
            "MyMod : 1\n"
        );
        assert_eq!(
            fs::read_to_string(win64.join("ue4ss/UE4SS-settings.ini")).unwrap(),
            "mine"
        );
        assert_eq!(
            fs::read_to_string(win64.join("ue4ss/Mods/ShippedMod/Scripts/main.lua")).unwrap(),
            "new",
            "a Lua mod the package ships is part of the runtime"
        );
        assert_eq!(
            fs::read_to_string(win64.join("ue4ss/Mods/MyMod/Scripts/main.lua")).unwrap(),
            "mine",
            "a Lua mod the package does not ship is never touched"
        );
        assert_eq!(report.installed, 3);
        assert_eq!(report.preserved.len(), 2);
    }

    /// End-to-end check against a real published UE4SS package, which no CI
    /// runner may download. Point `ZCOM_UE4SS_ARCHIVE` at a package from
    /// <https://www.nexusmods.com/starwarszerocompany/mods/9> and run
    /// `cargo test -- --ignored` to exercise a fresh install followed by an
    /// upgrade over user content.
    #[test]
    #[ignore = "requires a locally downloaded UE4SS package"]
    fn installs_a_published_package_over_user_content() {
        let Some(archive) = std::env::var_os("ZCOM_UE4SS_ARCHIVE") else {
            panic!("set ZCOM_UE4SS_ARCHIVE to a downloaded UE4SS package")
        };
        let archive = PathBuf::from(archive);
        let d = tempdir().unwrap();
        let game = d.path().join("game");
        let win64 = base(&game);
        fs::create_dir_all(&win64).unwrap();

        let fresh = install_from(&archive, &game, d.path()).unwrap();
        assert!(win64.join("dwmapi.dll").is_file());
        assert!(win64.join("ue4ss/UE4SS.dll").is_file());
        assert!(win64.join("ue4ss/Mods/mods.txt").is_file());
        assert!(fresh.preserved.is_empty(), "nothing exists yet to preserve");
        assert!(fresh.installed > 10, "expected a full runtime payload");

        // Simulate a user who tuned the runtime and added their own Lua mod.
        write(&win64.join("ue4ss/UE4SS-settings.ini"), "; mine");
        write(&win64.join("ue4ss/Mods/mods.txt"), "MyMod : 1\n");
        write(&win64.join("ue4ss/Mods/MyMod/Scripts/main.lua"), "-- mine");
        write(
            &win64.join("ue4ss/Mods/ConsoleCommandsMod/Scripts/main.lua"),
            "-- stale",
        );

        let upgrade = install_from(&archive, &game, d.path()).unwrap();
        assert_eq!(
            fs::read_to_string(win64.join("ue4ss/UE4SS-settings.ini")).unwrap(),
            "; mine",
            "tuned configuration must survive"
        );
        assert_eq!(
            fs::read_to_string(win64.join("ue4ss/Mods/mods.txt")).unwrap(),
            "MyMod : 1\n",
            "load order must survive"
        );
        assert_eq!(
            fs::read_to_string(win64.join("ue4ss/Mods/MyMod/Scripts/main.lua")).unwrap(),
            "-- mine",
            "a Lua mod absent from the package must never be touched"
        );
        assert_ne!(
            fs::read_to_string(win64.join("ue4ss/Mods/ConsoleCommandsMod/Scripts/main.lua"))
                .unwrap(),
            "-- stale",
            "runtime-supplied Lua mods must move with the runtime"
        );
        // The published package ships every file the preserve rule covers, so
        // an upgrade over a configured install keeps exactly these four.
        let preserved: Vec<String> = upgrade
            .preserved
            .iter()
            .map(|path| path.replace('\\', "/"))
            .collect();
        for expected in [
            "ue4ss/UE4SS-settings.ini",
            "ue4ss/Mods/mods.txt",
            "ue4ss/Mods/mods.json",
        ] {
            assert!(preserved.iter().any(|p| p == expected), "{preserved:?}");
        }
        assert!(
            preserved.iter().any(|p| p.ends_with("/load_order.txt")),
            "{preserved:?}"
        );
        assert_eq!(preserved.len(), 4, "{preserved:?}");
    }

    #[test]
    fn rejects_an_archive_without_a_ue4ss_layout() {
        let d = tempdir().unwrap();
        write(&d.path().join("staged/SomeMod_P.pak"), "pak");
        let game = d.path().join("game");
        fs::create_dir_all(base(&game)).unwrap();
        assert!(matches!(
            install_staged(d.path().join("staged").as_path(), &game),
            Err(AppError::Ue4ssPackageNotRecognized)
        ));
    }

    #[test]
    fn rewrites_only_the_managed_block_of_mods_txt() {
        let d = tempdir().unwrap();
        let mods = d.path().join("SWZeroCompany/Binaries/Win64/ue4ss/Mods");
        fs::create_dir_all(&mods).unwrap();
        fs::write(
            mods.join("mods.txt"),
            "CheatManagerEnablerMod : 1\n\n; Built-in keybinds, do not move up!\nKeybinds : 1\nAlpha : 1\nBravo : 0\n",
        )
        .unwrap();

        write_order(
            d.path(),
            &[
                ("Bravo".into(), false),
                ("Charlie".into(), true),
                ("Alpha".into(), true),
            ],
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(mods.join("mods.txt")).unwrap(),
            "CheatManagerEnablerMod : 1\n\n; Built-in keybinds, do not move up!\nKeybinds : 1\nBravo : 0\nCharlie : 1\nAlpha : 1\n",
            "runtime entries and comments keep their place; managed mods follow in order"
        );
    }

    #[test]
    fn ordering_never_lists_a_mod_twice() {
        let d = tempdir().unwrap();
        let mods = d.path().join("SWZeroCompany/Binaries/Win64/ue4ss/Mods");
        fs::create_dir_all(&mods).unwrap();
        fs::write(
            mods.join("mods.txt"),
            "alpha : 1\nKeybinds : 1\nAlpha : 1\n",
        )
        .unwrap();
        write_order(d.path(), &[("Alpha".into(), true)]).unwrap();
        assert_eq!(
            fs::read_to_string(mods.join("mods.txt")).unwrap(),
            "Keybinds : 1\nAlpha : 1\n"
        );
    }

    #[test]
    fn preserves_unrelated_mod_entries() {
        let d = tempdir().unwrap();
        let p = d.path().join("SWZeroCompany/Binaries/Win64/ue4ss/Mods");
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join("mods.txt"), "; comment\r\nOther : 1\r\nMine : 0\r\n").unwrap();
        update_mods_txt(d.path(), "Mine", true).unwrap();
        let t = fs::read_to_string(p.join("mods.txt")).unwrap();
        assert!(t.contains("; comment\r\nOther : 1\r\nMine : 1"));
    }
}
