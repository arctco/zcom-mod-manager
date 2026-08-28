use crate::{
    archives,
    error::{AppError, Result},
    models::{ManifestGame, ModManifest, ModPreview, PayloadFile, StagedMod, ToolInfo},
    retoc,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;
use walkdir::WalkDir;

fn lowercase_ext(path: &Path) -> String {
    path.extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}
fn display_name(stem: &str) -> String {
    let trimmed = stem
        .trim_end_matches("_P")
        .trim_start_matches("pakchunk99-");
    let mut result = String::new();
    for (i, c) in trimmed.chars().enumerate() {
        if i > 0
            && c.is_uppercase()
            && !trimmed
                .chars()
                .nth(i.saturating_sub(1))
                .is_some_and(char::is_whitespace)
        {
            result.push(' ')
        }
        result.push(if c == '_' { ' ' } else { c });
    }
    if result.trim().is_empty() {
        "Unnamed Mod".into()
    } else {
        result.trim().into()
    }
}
fn rel(root: &Path, path: &Path) -> Result<PathBuf> {
    Ok(path
        .strip_prefix(root)
        .map_err(|e| AppError::Other(e.to_string()))?
        .to_path_buf())
}

pub fn scan(
    source: &Path,
    cache: &Path,
    tool: &ToolInfo,
    game_build: Option<&str>,
    ue4ss_ready: bool,
    show_packages: bool,
) -> Result<(StagedMod, ModPreview)> {
    let archives::Staging { root, warnings } = archives::stage(source, cache)?;
    let mut files = Vec::new();
    for entry in WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        files.push(entry.path().to_path_buf())
    }
    let manifest = files
        .iter()
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case("zcom-mod.json"))
        })
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<ModManifest>(&t).ok());
    if let Some(manifest) = &manifest {
        if manifest.schema_version != 1 {
            return Err(AppError::Other(format!(
                "Unsupported zcom-mod.json schema version {}.",
                manifest.schema_version
            )));
        }
        if manifest
            .game
            .as_ref()
            .is_some_and(|game| game.app_id != 2075800)
        {
            return Err(AppError::Other(
                "This manifest targets a different Steam game.".into(),
            ));
        }
    }
    let mut groups: BTreeMap<(PathBuf, String), BTreeMap<String, PathBuf>> = BTreeMap::new();
    for file in &files {
        let ext = lowercase_ext(file);
        if matches!(ext.as_str(), "pak" | "utoc" | "ucas") {
            let parent = file.parent().unwrap_or(&root).to_path_buf();
            let stem = file
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            groups
                .entry((parent, stem))
                .or_default()
                .insert(ext, file.clone());
        }
    }
    let mut payload = Vec::new();
    let kind: String;
    let mut deployment_key = String::new();
    let mut packages = Vec::new();
    let mut package_paths = Vec::new();
    let mut verification = "not-required".to_string();
    let mut details = None;
    let has_iostore = groups
        .values()
        .any(|g| g.contains_key("utoc") || g.contains_key("ucas"));
    if has_iostore {
        kind = "iostore".into();
        verification = "passed".into();
        for ((_parent, stem), group) in groups
            .iter()
            .filter(|(_, g)| g.contains_key("utoc") || g.contains_key("ucas"))
        {
            let mut missing = Vec::new();
            if !group.contains_key("utoc") {
                missing.push(format!("{stem}.utoc"))
            }
            if !group.contains_key("ucas") {
                missing.push(format!("{stem}.ucas"))
            }
            if !missing.is_empty() {
                let _ = fs::remove_dir_all(&root);
                return Err(AppError::MissingIoStoreComponent(missing.join(", ")));
            }
            for ext in ["pak", "utoc", "ucas"] {
                if let Some(path) = group.get(ext) {
                    payload.push(PayloadFile {
                        source: path.clone(),
                        library_relative: PathBuf::from(path.file_name().unwrap()),
                        destination_relative: PathBuf::from(path.file_name().unwrap()),
                    });
                }
            }
            match retoc::inspect(tool, group.get("utoc").unwrap()) {
                Ok(info) => {
                    packages.extend(info.package_ids);
                    package_paths.extend(info.package_paths);
                    details = Some(match details {
                        Some(previous) => format!("{previous}\n{}", info.details),
                        None => info.details,
                    })
                }
                Err(AppError::RetocNotFound) => {
                    if verification != "failed" {
                        verification = "unavailable".into();
                    }
                    details = Some(AppError::RetocNotFound.to_string())
                }
                Err(error) => {
                    verification = "failed".into();
                    details = Some(error.to_string())
                }
            }
        }
    } else if !groups.is_empty() {
        kind = "pak".into();
        for ((_parent, _stem), group) in &groups {
            if let Some(path) = group.get("pak") {
                payload.push(PayloadFile {
                    source: path.clone(),
                    library_relative: PathBuf::from(path.file_name().unwrap()),
                    destination_relative: PathBuf::from(path.file_name().unwrap()),
                })
            }
        }
    } else {
        let main = files
            .iter()
            .find(|p| {
                p.file_name()
                    .is_some_and(|n| n.eq_ignore_ascii_case("main.lua"))
                    && p.parent()
                        .and_then(Path::file_name)
                        .is_some_and(|n| n.eq_ignore_ascii_case("scripts"))
            })
            .cloned()
            .ok_or(AppError::ModNotRecognized)?;
        kind = "ue4ss".into();
        let mod_root = main
            .parent()
            .and_then(Path::parent)
            .ok_or(AppError::ModNotRecognized)?;
        let mod_folder = mod_root
            .file_name()
            .ok_or(AppError::ModNotRecognized)?
            .to_os_string();
        deployment_key = mod_folder.to_string_lossy().into_owned();
        for file in files.iter().filter(|p| p.starts_with(mod_root)) {
            let inside = rel(mod_root, file)?;
            payload.push(PayloadFile {
                source: file.clone(),
                library_relative: PathBuf::from(&mod_folder).join(&inside),
                destination_relative: PathBuf::from(&mod_folder).join(&inside),
            });
        }
        if !ue4ss_ready {
            details = Some(AppError::Ue4ssNotFound.to_string());
        }
    }
    if payload.is_empty() {
        let _ = fs::remove_dir_all(&root);
        return Err(AppError::ModNotRecognized);
    }
    let fallback_stem = payload[0]
        .source
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let name = manifest
        .as_ref()
        .map(|m| m.name.clone())
        .unwrap_or_else(|| display_name(&fallback_stem));
    let tested = manifest
        .as_ref()
        .and_then(|m| m.game.as_ref())
        .map(|g| g.tested_builds.clone())
        .unwrap_or_default();
    let (compatibility, compatibility_message) = compatibility(
        game_build,
        &tested,
        manifest.as_ref().and_then(|m| m.game.as_ref()),
    );
    let valid = match kind.as_str() {
        "iostore" => verification == "passed",
        "ue4ss" => ue4ss_ready,
        _ => true,
    };
    let staging_id = Uuid::new_v4().to_string();
    packages.sort();
    packages.dedup();
    package_paths.sort();
    package_paths.dedup();
    let staged = StagedMod {
        staging_id: staging_id.clone(),
        staging_root: root,
        source_archive: source.display().to_string(),
        name: name.clone(),
        version: manifest.as_ref().and_then(|m| m.version.clone()),
        author: manifest.as_ref().and_then(|m| m.author.clone()),
        description: manifest.as_ref().and_then(|m| m.description.clone()),
        mod_type: kind.clone(),
        deployment_key,
        files: payload.clone(),
        packages: packages.clone(),
        verification: verification.clone(),
        verification_details: details.clone(),
    };
    let preview = ModPreview {
        staging_id,
        name,
        version: staged.version.clone(),
        author: staged.author.clone(),
        description: staged.description.clone(),
        mod_type: kind,
        files: payload
            .iter()
            .map(|p| p.library_relative.display().to_string())
            .collect(),
        warnings,
        valid,
        verification,
        verification_details: details,
        package_count: packages.len(),
        package_names: if show_packages {
            package_paths
        } else {
            Vec::new()
        },
        compatibility,
        compatibility_message,
        tested_builds: tested,
    };
    Ok((staged, preview))
}

fn compatibility(
    current: Option<&str>,
    tested: &[String],
    _game: Option<&ManifestGame>,
) -> (String, String) {
    if tested.is_empty() {
        return ("unknown".into(), "Unknown".into());
    }
    match current {
        Some(build) if tested.iter().any(|b| b == build) => ("good".into(), "Compatible".into()),
        Some(build) => (
            "warning".into(),
            format!("Tested on {}; game is {build}", tested.join(", ")),
        ),
        None => ("unknown".into(), "Game build unavailable".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    fn tool() -> ToolInfo {
        ToolInfo::default()
    }
    #[test]
    fn detects_pak_only() {
        let s = tempdir().unwrap();
        let c = tempdir().unwrap();
        fs::write(s.path().join("BetterActions_P.pak"), b"pak").unwrap();
        let (_, p) = scan(s.path(), c.path(), &tool(), None, false, false).unwrap();
        assert_eq!(p.mod_type, "pak");
        assert!(p.valid);
    }
    #[test]
    fn detects_complete_iostore_triplet_and_requires_verifier() {
        let s = tempdir().unwrap();
        let c = tempdir().unwrap();
        for ext in ["pak", "utoc", "ucas"] {
            fs::write(s.path().join(format!("Cool_P.{ext}")), b"synthetic").unwrap();
        }
        let (_, preview) = scan(s.path(), c.path(), &tool(), None, false, false).unwrap();
        assert_eq!(preview.mod_type, "iostore");
        assert_eq!(preview.files.len(), 3);
        assert_eq!(preview.verification, "unavailable");
        assert!(!preview.valid);
    }
    #[test]
    fn rejects_incomplete_triplet() {
        let s = tempdir().unwrap();
        let c = tempdir().unwrap();
        fs::write(s.path().join("Cool_P.utoc"), b"x").unwrap();
        fs::write(s.path().join("Cool_P.pak"), b"x").unwrap();
        assert!(matches!(
            scan(s.path(), c.path(), &tool(), None, false, false),
            Err(AppError::MissingIoStoreComponent(_))
        ));
    }
    #[test]
    fn nested_payload_is_found() {
        let s = tempdir().unwrap();
        let c = tempdir().unwrap();
        let p = s.path().join("SomeMod/SWZeroCompany/Content/Paks/~mods");
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join("Nested_P.pak"), b"x").unwrap();
        assert_eq!(
            scan(s.path(), c.path(), &tool(), None, false, false)
                .unwrap()
                .1
                .mod_type,
            "pak"
        );
    }
    #[test]
    fn detects_lua_case_insensitively() {
        let s = tempdir().unwrap();
        let c = tempdir().unwrap();
        let p = s.path().join("MyMod/scripts");
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join("main.lua"), b"return {}").unwrap();
        assert_eq!(
            scan(s.path(), c.path(), &tool(), None, true, false)
                .unwrap()
                .1
                .mod_type,
            "ue4ss"
        );
    }
    #[test]
    fn unrelated_folder_is_rejected() {
        let s = tempdir().unwrap();
        let c = tempdir().unwrap();
        fs::write(s.path().join("readme.txt"), b"hi").unwrap();
        assert!(matches!(
            scan(s.path(), c.path(), &tool(), None, false, false),
            Err(AppError::ModNotRecognized)
        ));
    }
}
