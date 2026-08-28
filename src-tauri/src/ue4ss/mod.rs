use crate::{
    error::{AppError, Result},
    models::Ue4ssInfo,
};
use std::{fs, path::Path};
use walkdir::WalkDir;

pub fn base(game: &Path) -> std::path::PathBuf {
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
    let lua_mods = if mods.is_dir() {
        WalkDir::new(&mods)
            .max_depth(4)
            .into_iter()
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_type().is_file() && e.file_name().eq_ignore_ascii_case("main.lua"))
            .count()
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
        lua_mods,
        log_found: root.join("UE4SS.log").is_file(),
        proton_override,
        message,
    }
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
        let trimmed = line.trim();
        let entry = trimmed.split([':', '=']).next().unwrap_or("").trim();
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
