mod naming;

use crate::{
    archives,
    error::{AppError, Result},
    models::{ManifestGame, ModManifest, ModPreview, PayloadFile, StagedMod, ToolInfo},
    retoc, ue4ss,
};
use naming::display_name;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;
use walkdir::WalkDir;

/// Loader shims a game-folder mod such as ReShade ships. The file replaces a
/// system library next to the executable, so it belongs in `Binaries/Win64`
/// rather than in the mod folders.
const INJECTOR_NAMES: [&str; 8] = [
    "dxgi.dll",
    "d3d9.dll",
    "d3d11.dll",
    "d3d12.dll",
    "opengl32.dll",
    "dinput8.dll",
    "winmm.dll",
    "version.dll",
];

/// Where a game-folder mod's files are anchored inside the installation.
const GAME_CONTENT_ROOT: &str = "SWZeroCompany";
const WIN64: &str = "SWZeroCompany/Binaries/Win64";
const LOGIC_MODS: &str = "SWZeroCompany/Content/Paks/LogicMods";

fn lowercase_ext(path: &Path) -> String {
    path.extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn named(path: &Path, expected: &str) -> bool {
    path.file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case(expected))
}

fn parent_named(path: &Path, expected: &str) -> bool {
    path.parent().is_some_and(|parent| named(parent, expected))
}

/// Index of a path component matching `expected`, case-insensitively.
fn component_index(path: &Path, expected: &str) -> Option<usize> {
    path.components().position(|component| match component {
        Component::Normal(value) => value
            .to_str()
            .is_some_and(|value| value.eq_ignore_ascii_case(expected)),
        _ => false,
    })
}

fn suffix_from(path: &Path, index: usize) -> PathBuf {
    path.components().skip(index).collect()
}

/// The readable part of a source name. Only a known archive extension is
/// stripped, because a mod folder is regularly named with dots in it and
/// `file_stem` would cut the name at the first one.
fn source_stem(source: &Path) -> String {
    let name = file_name(source);
    match lowercase_ext(source).as_str() {
        "zip" | "7z" | "rar" | "pak" | "utoc" | "ucas" => source
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        _ => name,
    }
}

fn rel(root: &Path, path: &Path) -> Result<PathBuf> {
    Ok(path
        .strip_prefix(root)
        .map_err(|e| AppError::Other(e.to_string()))?
        .to_path_buf())
}

fn register_package_owners(
    owners: &mut BTreeMap<String, String>,
    stem: &str,
    packages: &[String],
) -> Result<()> {
    for package in packages {
        if let Some(previous) = owners.insert(package.clone(), stem.to_string()) {
            if previous != stem {
                return Err(AppError::AlternativeIoStoreVariants(format!(
                    "{previous} and {stem}"
                )));
            }
        }
    }
    Ok(())
}

/// A UE4SS mod folder: the directory UE4SS itself loads by name.
///
/// UE4SS accepts two payloads inside that folder, and a mod may ship either or
/// both: `Scripts/main.lua` for a Lua mod and `dlls/main.dll` for a native one.
/// Recognizing only the Lua form left every DLL mod uninstallable, and stopping
/// at the first match dropped every mod after the first in an archive that
/// ships several.
fn ue4ss_folders(root: &Path, files: &[PathBuf]) -> Vec<PathBuf> {
    let mut folders = BTreeSet::new();
    for file in files {
        let is_lua = named(file, "main.lua") && parent_named(file, "scripts");
        let is_dll = lowercase_ext(file) == "dll" && parent_named(file, "dlls");
        if !(is_lua || is_dll) {
            continue;
        }
        let Some(folder) = file.parent().and_then(Path::parent) else {
            continue;
        };
        // `shared` holds libraries the runtime provides to every mod, and
        // `Mods` would swallow the whole tree if an archive nested oddly.
        if named(folder, "shared") || named(folder, "Mods") {
            continue;
        }
        folders.insert(folder.to_path_buf());
    }
    // A folder nested inside another candidate is part of that mod, not a mod.
    let selected: Vec<PathBuf> = folders
        .iter()
        .filter(|folder| {
            !folders
                .iter()
                .any(|other| other != *folder && folder.starts_with(other))
        })
        .cloned()
        .collect();
    // An archive whose payload sits loose at the top level (`Scripts/main.lua`
    // with no wrapping folder) still describes exactly one mod.
    if selected.is_empty() {
        return Vec::new();
    }
    if selected.len() == 1 && selected[0] == root {
        return vec![root.to_path_buf()];
    }
    selected
        .into_iter()
        .filter(|folder| folder != root)
        .collect()
}

/// A file name that is safe to use as a folder inside the game installation.
fn sanitize_folder(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches(['_', '.']).to_string();
    if trimmed.is_empty() {
        "Mod".into()
    } else {
        trimmed
    }
}

struct Bucket {
    kind: &'static str,
    files: Vec<PayloadFile>,
    keys: Vec<String>,
    /// A name derived from the payload itself, used when the archive name
    /// cannot stand in because it describes more than one mod.
    intrinsic_name: Option<String>,
}

#[allow(clippy::too_many_arguments)]
pub fn scan(
    source: &Path,
    cache: &Path,
    tool: &ToolInfo,
    game_build: Option<&str>,
    ue4ss_ready: bool,
    show_packages: bool,
) -> Result<Vec<(StagedMod, ModPreview)>> {
    let archives::Staging {
        root,
        mut warnings,
        executables,
    } = archives::stage(source, cache)?;
    let result = collect(
        source,
        &root,
        tool,
        game_build,
        ue4ss_ready,
        show_packages,
        &mut warnings,
        &executables,
    );
    if result.is_err() {
        let _ = fs::remove_dir_all(&root);
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn collect(
    source: &Path,
    root: &Path,
    tool: &ToolInfo,
    game_build: Option<&str>,
    ue4ss_ready: bool,
    show_packages: bool,
    warnings: &mut Vec<String>,
    executables: &[String],
) -> Result<Vec<(StagedMod, ModPreview)>> {
    let mut files: Vec<PathBuf> = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .collect();
    files.sort();

    let (source_name, source_version) = naming::from_source_name(&source_stem(source));

    // The UE4SS runtime is not a mod, and installing it as one would scatter
    // the loader across the mod folder. It is offered as a runtime install
    // instead of being rejected with a puzzle.
    if ue4ss::layout_root(root).is_some() {
        let listed: Vec<String> = files
            .iter()
            .filter_map(|file| rel(root, file).ok())
            .map(|path| path.display().to_string())
            .take(200)
            .collect();
        let preview = ModPreview {
            staging_id: Uuid::new_v4().to_string(),
            source_path: source.display().to_string(),
            name: source_name
                .clone()
                .unwrap_or_else(|| "UE4SS runtime".into()),
            version: source_version.clone(),
            author: None,
            description: Some(
                "This archive is the UE4SS runtime rather than a mod. Installing it sets up the \
                 loader that Lua and DLL mods need."
                    .into(),
            ),
            mod_type: "ue4ss-runtime".into(),
            files: listed,
            warnings: warnings.clone(),
            valid: true,
            verification: "not-required".into(),
            verification_details: None,
            package_count: 0,
            package_names: Vec::new(),
            compatibility: "unknown".into(),
            compatibility_message: "Runtime package".into(),
            tested_builds: Vec::new(),
            conflicts: Vec::new(),
            replaces: None,
            recommended_priority: None,
            load_order_supported: false,
            load_order_support_reason: None,
        };
        let staged = StagedMod {
            staging_id: preview.staging_id.clone(),
            staging_root: root.to_path_buf(),
            source_archive: source.display().to_string(),
            name: preview.name.clone(),
            version: preview.version.clone(),
            author: None,
            description: preview.description.clone(),
            mod_type: "ue4ss-runtime".into(),
            deployment_keys: Vec::new(),
            files: Vec::new(),
            packages: Vec::new(),
            verification: "not-required".into(),
            verification_details: None,
        };
        return Ok(vec![(staged, preview)]);
    }

    let manifest = files
        .iter()
        .find(|p| named(p, "zcom-mod.json"))
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

    let mut claimed: BTreeSet<PathBuf> = BTreeSet::new();
    let mut buckets: Vec<Bucket> = Vec::new();

    // UE4SS mods. Each folder the runtime loads by name is its own mod, even
    // when several arrive in one download: they are enabled, ordered, and
    // removed independently, so they cannot share a library entry.
    let folders = ue4ss_folders(root, &files);
    let mut folder_keys: Vec<String> = Vec::new();
    for folder in &folders {
        let key = if folder == root {
            sanitize_folder(source_name.as_deref().unwrap_or(&file_name(source)))
        } else {
            sanitize_folder(&file_name(folder))
        };
        if folder_keys
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&key))
        {
            return Err(AppError::Other(format!(
                "This archive contains two UE4SS mod folders named {key}. Extract it and \
                 install one at a time."
            )));
        }
        let mut payload = Vec::new();
        for file in files.iter().filter(|file| file.starts_with(folder)) {
            let inside = rel(folder, file)?;
            let relative = PathBuf::from(&key).join(&inside);
            payload.push(PayloadFile {
                source: file.clone(),
                library_relative: relative.clone(),
                destination_relative: relative,
            });
            claimed.insert(file.clone());
        }
        folder_keys.push(key.clone());
        buckets.push(Bucket {
            kind: "ue4ss",
            files: payload,
            keys: vec![key],
            intrinsic_name: (folder != root).then(|| display_name(&file_name(folder))),
        });
    }

    // Packaged content. Blueprint mods living under `LogicMods` are loaded by
    // the UE4SS blueprint loader from their own folder, so they are handled
    // with the game-folder payloads below instead of being renamed for
    // priority in `~mods`.
    let mut groups: BTreeMap<(PathBuf, String), BTreeMap<String, PathBuf>> = BTreeMap::new();
    for file in files
        .iter()
        .filter(|file| !claimed.contains(*file))
        .filter(|file| component_index(file, "LogicMods").is_none())
    {
        let ext = lowercase_ext(file);
        if matches!(ext.as_str(), "pak" | "utoc" | "ucas") {
            let parent = file.parent().unwrap_or(root).to_path_buf();
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
    let has_iostore = groups
        .values()
        .any(|g| g.contains_key("utoc") || g.contains_key("ucas"));
    let mut packages = Vec::new();
    let mut package_paths = Vec::new();
    let mut verification = "not-required".to_string();
    let mut details: Option<String> = None;
    if !groups.is_empty() {
        let mut payload = Vec::new();
        let mut package_owners: BTreeMap<String, String> = BTreeMap::new();
        if has_iostore {
            verification = "passed".into();
        }
        for ((_parent, stem), group) in groups
            .iter()
            .filter(|(_, g)| !has_iostore || g.contains_key("utoc") || g.contains_key("ucas"))
        {
            if has_iostore {
                let mut missing = Vec::new();
                if !group.contains_key("utoc") {
                    missing.push(format!("{stem}.utoc"))
                }
                if !group.contains_key("ucas") {
                    missing.push(format!("{stem}.ucas"))
                }
                if !missing.is_empty() {
                    return Err(AppError::MissingIoStoreComponent(missing.join(", ")));
                }
            }
            for ext in ["pak", "utoc", "ucas"] {
                if let Some(path) = group.get(ext) {
                    let name = PathBuf::from(file_name(path));
                    payload.push(PayloadFile {
                        source: path.clone(),
                        library_relative: name.clone(),
                        destination_relative: name,
                    });
                    claimed.insert(path.clone());
                }
            }
            let Some(utoc) = group.get("utoc") else {
                continue;
            };
            match retoc::inspect(tool, utoc) {
                Ok(info) => {
                    register_package_owners(&mut package_owners, stem, &info.package_ids)?;
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
        if !payload.is_empty() {
            let intrinsic = display_name(
                &payload[0]
                    .library_relative
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy(),
            );
            buckets.push(Bucket {
                kind: if has_iostore { "iostore" } else { "pak" },
                files: payload,
                keys: Vec::new(),
                intrinsic_name: Some(intrinsic),
            });
        }
    }

    // Everything the game reads straight from its own folders: ReShade and
    // other loader shims, replacement movies and audio, and blueprint mods.
    let injector_root = files
        .iter()
        .filter(|file| !claimed.contains(*file))
        .find(|file| INJECTOR_NAMES.iter().any(|name| named(file, name)))
        .and_then(|file| file.parent())
        .map(Path::to_path_buf);
    let mut gamedir = Vec::new();
    let mut ignored = Vec::new();
    for file in files.iter().filter(|file| !claimed.contains(*file)) {
        let relative = rel(root, file)?;
        let destination = if let Some(index) = component_index(&relative, GAME_CONTENT_ROOT) {
            Some(suffix_from(&relative, index))
        } else if component_index(&relative, "LogicMods").is_some() && lowercase_ext(file) == "pak"
        {
            Some(PathBuf::from(LOGIC_MODS).join(file_name(file)))
        } else if let Some(injector) = injector_root
            .as_ref()
            .filter(|injector| file.starts_with(injector))
        {
            Some(PathBuf::from(WIN64).join(rel(injector, file)?))
        } else {
            None
        };
        match destination {
            Some(destination) => gamedir.push(PayloadFile {
                source: file.clone(),
                library_relative: relative,
                destination_relative: destination,
            }),
            None => ignored.push(relative.display().to_string()),
        }
    }
    if !gamedir.is_empty() {
        for file in &gamedir {
            claimed.insert(file.source.clone());
        }
        buckets.push(Bucket {
            kind: "gamedir",
            files: gamedir,
            keys: Vec::new(),
            intrinsic_name: None,
        });
    }

    if buckets.is_empty() {
        return Err(AppError::ModNotRecognized);
    }
    if !ignored.is_empty() {
        warnings.push(format!(
            "{} file{} in this archive {} not part of a recognized mod layout and will not be \
             installed: {}",
            ignored.len(),
            if ignored.len() == 1 { "" } else { "s" },
            if ignored.len() == 1 { "is" } else { "are" },
            ignored
                .iter()
                .take(6)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

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
    let single = buckets.len() == 1;
    let mut result = Vec::new();
    for bucket in buckets {
        // The archive name is what the person downloaded and recognizes, so it
        // wins whenever it can only mean this one mod. A manifest still wins
        // over both, and every name can be edited before installing.
        let name = manifest
            .as_ref()
            .map(|m| m.name.clone())
            .or_else(|| {
                if single {
                    source_name.clone()
                } else {
                    bucket.intrinsic_name.clone()
                }
            })
            .or_else(|| bucket.intrinsic_name.clone())
            .or_else(|| source_name.clone())
            .unwrap_or_else(|| "Unnamed Mod".into());
        let mut bucket_warnings = warnings.clone();
        for executable in executables {
            let inside = bucket.files.iter().any(|file| {
                rel(root, &file.source)
                    .map(|relative| relative.display().to_string().replace('\\', "/"))
                    .is_ok_and(|relative| relative == *executable)
            });
            bucket_warnings.push(if inside {
                format!(
                    "{executable} is native code that runs inside the game. Install it only if \
                     you trust the author."
                )
            } else {
                format!("{executable} is an executable outside the mod layout and is ignored.")
            });
        }
        let (verification, details) = if bucket.kind == "iostore" || bucket.kind == "pak" {
            (verification.clone(), details.clone())
        } else {
            ("not-required".to_string(), None)
        };
        let orderable = bucket.kind == "iostore"
            && bucket.files.iter().any(|file| {
                file.destination_relative
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("pak"))
            });
        let valid = match bucket.kind {
            "iostore" => verification == "passed",
            "ue4ss" => ue4ss_ready,
            _ => true,
        };
        if bucket.kind == "ue4ss" && !ue4ss_ready {
            bucket_warnings.push(AppError::Ue4ssNotFound.to_string());
        }
        let staging_id = Uuid::new_v4().to_string();
        let mut bucket_packages = if bucket.kind == "iostore" {
            packages.clone()
        } else {
            Vec::new()
        };
        bucket_packages.sort();
        bucket_packages.dedup();
        let mut names = package_paths.clone();
        names.sort();
        names.dedup();
        let staged = StagedMod {
            staging_id: staging_id.clone(),
            staging_root: root.to_path_buf(),
            source_archive: source.display().to_string(),
            name: name.clone(),
            version: manifest
                .as_ref()
                .and_then(|m| m.version.clone())
                .or_else(|| source_version.clone()),
            author: manifest.as_ref().and_then(|m| m.author.clone()),
            description: manifest.as_ref().and_then(|m| m.description.clone()),
            mod_type: bucket.kind.into(),
            deployment_keys: bucket.keys.clone(),
            files: bucket.files.clone(),
            packages: bucket_packages.clone(),
            verification: verification.clone(),
            verification_details: details.clone(),
        };
        let preview = ModPreview {
            staging_id,
            source_path: source.display().to_string(),
            name,
            version: staged.version.clone(),
            author: staged.author.clone(),
            description: staged.description.clone(),
            mod_type: bucket.kind.into(),
            files: bucket
                .files
                .iter()
                // Displayed, never parsed. One spelling on every platform keeps
                // a Windows path from reading as a mix of both separators.
                .map(|p| {
                    p.destination_relative
                        .display()
                        .to_string()
                        .replace('\\', "/")
                })
                .collect(),
            warnings: bucket_warnings,
            valid,
            verification,
            verification_details: details,
            package_count: bucket_packages.len(),
            package_names: if show_packages && bucket.kind == "iostore" {
                names
            } else {
                Vec::new()
            },
            compatibility: compatibility.clone(),
            compatibility_message: compatibility_message.clone(),
            tested_builds: tested.clone(),
            conflicts: Vec::new(),
            replaces: None,
            recommended_priority: None,
            load_order_supported: orderable,
            load_order_support_reason: match bucket.kind {
                "pak" => Some(
                    "PAK-only ordering did not pass the Zero Company runtime capability test."
                        .into(),
                ),
                "iostore" if !orderable => Some(
                    "IoStore pairs without a companion PAK are not runtime-verified for ordering."
                        .into(),
                ),
                "ue4ss" => Some("UE4SS mods use their own runtime ordering.".into()),
                "gamedir" => {
                    Some("Game-folder mods are placed at fixed paths and are not ordered.".into())
                }
                _ => None,
            },
        };
        result.push((staged, preview));
    }
    Ok(result)
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

    fn write(path: &Path, body: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    fn one(source: &Path, cache: &Path, ue4ss_ready: bool) -> (StagedMod, ModPreview) {
        let mut found = scan(source, cache, &tool(), None, ue4ss_ready, false).unwrap();
        assert_eq!(found.len(), 1, "expected exactly one mod");
        found.remove(0)
    }

    #[test]
    fn detects_pak_only() {
        let s = tempdir().unwrap();
        let c = tempdir().unwrap();
        fs::write(s.path().join("BetterActions_P.pak"), b"pak").unwrap();
        let (_, p) = one(s.path(), c.path(), false);
        assert_eq!(p.mod_type, "pak");
        assert!(p.valid);
        assert!(!p.load_order_supported);
        assert!(p.load_order_support_reason.is_some());
    }

    #[test]
    fn rejects_overlapping_iostore_variants() {
        let mut owners = BTreeMap::new();
        register_package_owners(&mut owners, "FullPrice", &["package-a".into()]).unwrap();
        assert!(matches!(
            register_package_owners(&mut owners, "HalfPrice", &["package-a".into()]),
            Err(AppError::AlternativeIoStoreVariants(_))
        ));
    }

    #[test]
    fn detects_complete_iostore_triplet_and_requires_verifier() {
        let s = tempdir().unwrap();
        let c = tempdir().unwrap();
        for ext in ["pak", "utoc", "ucas"] {
            fs::write(s.path().join(format!("Cool_P.{ext}")), b"synthetic").unwrap();
        }
        let (_, preview) = one(s.path(), c.path(), false);
        assert_eq!(preview.mod_type, "iostore");
        assert_eq!(preview.files.len(), 3);
        assert_eq!(preview.verification, "unavailable");
        assert!(!preview.valid);
        assert!(preview.load_order_supported);
    }

    #[test]
    fn keeps_iostore_pairs_visible_but_not_orderable() {
        let s = tempdir().unwrap();
        let c = tempdir().unwrap();
        for ext in ["utoc", "ucas"] {
            fs::write(s.path().join(format!("Cool_P.{ext}")), b"synthetic").unwrap();
        }
        let (_, preview) = one(s.path(), c.path(), false);
        assert_eq!(preview.mod_type, "iostore");
        assert!(!preview.load_order_supported);
        assert!(preview
            .load_order_support_reason
            .as_deref()
            .unwrap()
            .contains("without a companion PAK"));
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
        assert_eq!(one(s.path(), c.path(), false).1.mod_type, "pak");
    }

    #[test]
    fn detects_lua_case_insensitively() {
        let s = tempdir().unwrap();
        let c = tempdir().unwrap();
        write(&s.path().join("MyMod/scripts/main.lua"), b"return {}");
        let (staged, preview) = one(s.path(), c.path(), true);
        assert_eq!(preview.mod_type, "ue4ss");
        assert_eq!(staged.deployment_keys, vec!["MyMod".to_string()]);
    }

    #[test]
    fn installs_a_dll_only_ue4ss_mod_and_names_it_after_its_folder() {
        let s = tempdir().unwrap();
        let c = tempdir().unwrap();
        write(
            &s.path()
                .join("ue4ss/Mods/UniqueTalentsForAll/dlls/main.dll"),
            b"MZ",
        );
        write(
            &s.path().join("ue4ss/Mods/UniqueTalentsForAll/enabled.txt"),
            b"1",
        );
        let (staged, preview) = one(s.path(), c.path(), true);
        assert_eq!(preview.mod_type, "ue4ss");
        assert_eq!(
            staged.deployment_keys,
            vec!["UniqueTalentsForAll".to_string()]
        );
        assert_eq!(staged.files.len(), 2);
        assert!(preview
            .files
            .contains(&"UniqueTalentsForAll/dlls/main.dll".to_string()));
    }

    #[test]
    fn every_lua_mod_an_archive_ships_becomes_its_own_entry() {
        let s = tempdir().unwrap();
        let c = tempdir().unwrap();
        write(
            &s.path().join("ue4ss/Mods/ShadowsCore/Scripts/main.lua"),
            b"a",
        );
        write(
            &s.path().join("ue4ss/Mods/ShadowsTweaks/Scripts/main.lua"),
            b"b",
        );
        write(
            &s.path().join("ue4ss/Mods/ShadowsTweaks/Scripts/helper.lua"),
            b"c",
        );
        let found = scan(s.path(), c.path(), &tool(), None, true, false).unwrap();
        assert_eq!(
            found.len(),
            2,
            "each folder is loaded and disabled on its own"
        );
        assert_eq!(found[0].0.deployment_keys, vec!["ShadowsCore".to_string()]);
        assert_eq!(found[0].1.name, "Shadows Core");
        assert_eq!(found[0].0.files.len(), 1);
        assert_eq!(
            found[1].0.deployment_keys,
            vec!["ShadowsTweaks".to_string()]
        );
        assert_eq!(found[1].0.files.len(), 2);
    }

    #[test]
    fn separates_a_packaged_mod_from_a_lua_mod_in_one_archive() {
        let s = tempdir().unwrap();
        let c = tempdir().unwrap();
        write(&s.path().join("Mods/Extra/Scripts/main.lua"), b"a");
        write(&s.path().join("Paks/Cool_P.pak"), b"pak");
        let found = scan(s.path(), c.path(), &tool(), None, true, false).unwrap();
        let kinds: Vec<&str> = found.iter().map(|(_, p)| p.mod_type.as_str()).collect();
        assert_eq!(kinds, vec!["ue4ss", "pak"]);
        assert_eq!(found[0].1.name, "Extra");
        assert_eq!(found[1].1.name, "Cool");
    }

    #[test]
    fn names_a_single_mod_after_the_archive_it_came_from() {
        let d = tempdir().unwrap();
        let c = tempdir().unwrap();
        let source = d
            .path()
            .join("ZCUnlocked 34 1.3 2026-08-30T07-32Z i9WZfkaQ7");
        write(&source.join("ue4ss/Mods/ZCUnlocked/dlls/main.dll"), b"MZ");
        let (_, preview) = one(&source, c.path(), true);
        assert_eq!(preview.name, "ZC Unlocked");
        assert_eq!(preview.version.as_deref(), Some("1.3"));
    }

    #[test]
    fn deploys_a_game_folder_mod_to_its_own_path() {
        let s = tempdir().unwrap();
        let c = tempdir().unwrap();
        write(
            &s.path()
                .join("NoIntro/SWZeroCompany/Content/Movies/Logo.mp4"),
            b"movie",
        );
        let (staged, preview) = one(s.path(), c.path(), false);
        assert_eq!(preview.mod_type, "gamedir");
        assert_eq!(
            staged.files[0].destination_relative,
            PathBuf::from("SWZeroCompany/Content/Movies/Logo.mp4")
        );
    }

    #[test]
    fn places_a_loader_shim_next_to_the_executable() {
        let s = tempdir().unwrap();
        let c = tempdir().unwrap();
        write(&s.path().join("ReShade/dxgi.dll"), b"MZ");
        write(&s.path().join("ReShade/ReShade.ini"), b"[GENERAL]");
        write(
            &s.path().join("ReShade/reshade-shaders/Shaders/Tone.fx"),
            b"fx",
        );
        let (staged, preview) = one(s.path(), c.path(), false);
        assert_eq!(preview.mod_type, "gamedir");
        let destinations: BTreeSet<String> = staged
            .files
            .iter()
            .map(|file| {
                file.destination_relative
                    .display()
                    .to_string()
                    .replace('\\', "/")
            })
            .collect();
        assert!(destinations.contains("SWZeroCompany/Binaries/Win64/dxgi.dll"));
        assert!(
            destinations.contains("SWZeroCompany/Binaries/Win64/reshade-shaders/Shaders/Tone.fx")
        );
        assert!(preview
            .warnings
            .iter()
            .any(|warning| warning.contains("native code")));
    }

    #[test]
    fn sends_a_blueprint_mod_to_the_logic_mods_folder() {
        let s = tempdir().unwrap();
        let c = tempdir().unwrap();
        write(&s.path().join("LogicMods/Blueprint_P.pak"), b"pak");
        let (staged, preview) = one(s.path(), c.path(), false);
        assert_eq!(preview.mod_type, "gamedir");
        assert_eq!(
            staged.files[0].destination_relative,
            PathBuf::from("SWZeroCompany/Content/Paks/LogicMods/Blueprint_P.pak")
        );
    }

    #[test]
    fn offers_the_runtime_package_as_a_runtime_install() {
        let s = tempdir().unwrap();
        let c = tempdir().unwrap();
        write(&s.path().join("dwmapi.dll"), b"MZ");
        write(&s.path().join("ue4ss/UE4SS.dll"), b"MZ");
        write(&s.path().join("ue4ss/Mods/mods.txt"), b"");
        let (_, preview) = one(s.path(), c.path(), false);
        assert_eq!(preview.mod_type, "ue4ss-runtime");
        assert!(preview.valid);
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

    /// Reads real published archives, which no CI runner may download. Point
    /// `ZCOM_MOD_ARCHIVES` at a folder of downloads and run
    /// `cargo test -- --ignored --nocapture` to see what each one resolves to.
    #[test]
    #[ignore = "requires locally downloaded mod archives"]
    fn describes_locally_downloaded_archives() {
        let Some(folder) = std::env::var_os("ZCOM_MOD_ARCHIVES") else {
            panic!("set ZCOM_MOD_ARCHIVES to a folder of downloaded mod archives")
        };
        let cache = tempdir().unwrap();
        let mut seen = 0;
        for entry in fs::read_dir(folder).unwrap().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            seen += 1;
            match scan(&path, cache.path(), &tool(), None, true, false) {
                Ok(found) => {
                    println!("{}", file_name(&path));
                    for (staged, preview) in found {
                        println!(
                            "  [{}] {} {} keys={:?} files={}",
                            preview.mod_type,
                            preview.name,
                            preview.version.clone().unwrap_or_default(),
                            staged.deployment_keys,
                            preview.files.len()
                        );
                        for file in preview.files.iter().take(4) {
                            println!("      {file}");
                        }
                    }
                }
                Err(error) => println!("{}\n  REJECTED: {error}", file_name(&path)),
            }
        }
        assert!(seen > 0, "the folder held no archives");
    }

    #[test]
    fn reports_files_it_will_not_install() {
        let s = tempdir().unwrap();
        let c = tempdir().unwrap();
        write(&s.path().join("MyMod/Scripts/main.lua"), b"return {}");
        write(&s.path().join("README.txt"), b"read me");
        let (_, preview) = one(s.path(), c.path(), true);
        assert!(preview
            .warnings
            .iter()
            .any(|warning| warning.contains("README.txt")));
    }
}
