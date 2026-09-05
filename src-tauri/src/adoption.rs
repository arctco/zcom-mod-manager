use crate::{
    database,
    deployment::sha256,
    error::{AppError, Result},
    models::{
        AdoptionGroup, AdoptionOutcome, AdoptionReport, ExistingModCandidate, ExistingModScan,
        ModFile, ModManifest, ModSummary, ToolInfo,
    },
    mods::naming::display_name,
    retoc,
};
use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};
use uuid::Uuid;
use walkdir::WalkDir;

const PACKAGED_ROOT: &str = "SWZeroCompany/Content/Paks/~mods";
const LOGIC_ROOT: &str = "SWZeroCompany/Content/Paks/LogicMods";
const UE4SS_ROOT: &str = "SWZeroCompany/Binaries/Win64/ue4ss/Mods";
const JOURNAL_NAME: &str = "adoption-operation.json";
const RUNTIME_COMPONENTS: [&str; 2] = ["bpmodloadermod", "consolecommandsmod"];
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

#[derive(Debug, Clone)]
pub struct FileSnapshot {
    pub source: PathBuf,
    pub library_relative: PathBuf,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

#[derive(Debug, Clone)]
pub struct CandidateSnapshot {
    pub public: ExistingModCandidate,
    pub files: Vec<FileSnapshot>,
    pub deployment_keys: Vec<String>,
    pub packages: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ScanSnapshot {
    pub game: PathBuf,
    pub candidates: HashMap<String, CandidateSnapshot>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AdoptionJournal {
    id: String,
    temporary: PathBuf,
    final_path: PathBuf,
}

fn lower_extension(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn regular_files(directory: &Path) -> Vec<(PathBuf, bool)> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut result = entries
        .filter_map(std::result::Result::ok)
        .map(|entry| {
            let path = entry.path();
            let regular = entry.file_type().is_ok_and(|kind| kind.is_file());
            (path, regular)
        })
        .collect::<Vec<_>>();
    result.sort_by(|left, right| {
        left.0
            .to_string_lossy()
            .to_ascii_lowercase()
            .cmp(&right.0.to_string_lossy().to_ascii_lowercase())
    });
    result
}

fn metadata_snapshot(path: &Path, library_relative: PathBuf) -> Result<FileSnapshot> {
    let metadata = fs::symlink_metadata(path)?;
    Ok(FileSnapshot {
        source: path.to_path_buf(),
        library_relative,
        size: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

fn normalized(path: &Path) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        value.to_ascii_lowercase()
    } else {
        value
    }
}

fn owned_destinations(conn: &Connection) -> Result<HashSet<String>> {
    Ok(database::list_mods(conn)?
        .into_iter()
        .flat_map(|item| item.files)
        .map(|file| normalized(Path::new(&file.destination)))
        .collect())
}

struct CandidateSpec<'a> {
    name: String,
    version: Option<String>,
    mod_type: &'a str,
    files: Vec<FileSnapshot>,
    enabled: bool,
    packages: Vec<String>,
    deployment_keys: Vec<String>,
    warnings: Vec<String>,
    blocked_reason: Option<String>,
    likely_runtime_component: bool,
    inferred_priority: Option<i64>,
}

fn candidate(spec: CandidateSpec<'_>) -> CandidateSnapshot {
    let id = Uuid::new_v4().to_string();
    let adoptable = spec.blocked_reason.is_none();
    let displayed = spec
        .files
        .iter()
        .map(|file| {
            file.library_relative
                .display()
                .to_string()
                .replace('\\', "/")
        })
        .collect();
    CandidateSnapshot {
        public: ExistingModCandidate {
            id,
            name: spec.name,
            version: spec.version,
            mod_type: spec.mod_type.into(),
            files: displayed,
            enabled: spec.enabled,
            package_count: spec.packages.len(),
            warnings: spec.warnings,
            adoptable,
            blocked_reason: spec.blocked_reason,
            selected_by_default: adoptable && !spec.likely_runtime_component,
            likely_runtime_component: spec.likely_runtime_component,
            inferred_priority: spec.inferred_priority,
        },
        files: spec.files,
        deployment_keys: spec.deployment_keys,
        packages: spec.packages,
    }
}

fn logical_packaged_name(path: &Path) -> PathBuf {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let Some(dot) = name.rfind('.') else {
        return PathBuf::from(name.as_ref());
    };
    let (stem, extension) = name.split_at(dot);
    let bytes = stem.as_bytes();
    if bytes.len() > 7
        && stem.ends_with("_P")
        && bytes[bytes.len() - 7] == b'_'
        && bytes[bytes.len() - 6..bytes.len() - 2]
            .iter()
            .all(u8::is_ascii_digit)
    {
        return PathBuf::from(format!("{}_P{}", &stem[..stem.len() - 7], extension));
    }
    PathBuf::from(name.as_ref())
}

fn scan_packaged(
    conn: &Connection,
    game: &Path,
    tool: &ToolInfo,
    owned: &HashSet<String>,
) -> Result<Vec<CandidateSnapshot>> {
    let root = game.join(PACKAGED_ROOT);
    let mut groups: BTreeMap<String, Vec<(PathBuf, bool)>> = BTreeMap::new();
    for (path, regular) in regular_files(&root) {
        if !matches!(lower_extension(&path).as_str(), "pak" | "utoc" | "ucas") {
            continue;
        }
        let stem = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase();
        groups.entry(stem).or_default().push((path, regular));
    }
    let all_pak_names = regular_files(&root)
        .into_iter()
        .filter(|(path, regular)| *regular && lower_extension(path) == "pak")
        .map(|(path, _)| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_ascii_lowercase()
        })
        .collect::<Vec<_>>();
    let mut result = Vec::new();
    for (_stem, paths) in groups {
        let owned_count = paths
            .iter()
            .filter(|(path, _)| owned.contains(&normalized(path)))
            .count();
        if owned_count == paths.len() {
            continue;
        }
        let extensions = paths
            .iter()
            .map(|(path, _)| lower_extension(path))
            .collect::<BTreeSet<_>>();
        let duplicate_extensions = extensions.len() != paths.len();
        let iostore = extensions.contains("utoc") || extensions.contains("ucas");
        let mut blocked = if owned_count > 0 {
            Some("Some files in this container family are already managed by ZCOM.".into())
        } else if paths.iter().any(|(_, regular)| !regular) {
            Some("Symbolic links and other non-regular files cannot be adopted.".into())
        } else if duplicate_extensions {
            Some("This family contains duplicate extensions with different casing.".into())
        } else if iostore && !(extensions.contains("utoc") && extensions.contains("ucas")) {
            Some(
                "The IoStore container is incomplete; both UTOC and UCAS files are required."
                    .into(),
            )
        } else {
            None
        };
        let mut packages = Vec::new();
        let warnings = Vec::new();
        if iostore && blocked.is_none() {
            let utoc = paths
                .iter()
                .find(|(path, _)| lower_extension(path) == "utoc")
                .map(|(path, _)| path.as_path())
                .expect("complete IoStore candidate has a UTOC");
            match retoc::inspect(tool, utoc) {
                Ok(inspection) => packages = inspection.package_ids,
                Err(error) => blocked = Some(error.to_string()),
            }
        }
        let mut snapshots = Vec::new();
        for (path, _) in &paths {
            snapshots.push(metadata_snapshot(path, logical_packaged_name(path))?);
        }
        let pak_name = paths
            .iter()
            .find(|(path, _)| lower_extension(path) == "pak")
            .map(|(path, _)| {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_ascii_lowercase()
            });
        let priority = pak_name
            .as_ref()
            .and_then(|name| all_pak_names.iter().position(|current| current == name))
            .map(|index| index as i64 + 1)
            .or_else(|| Some(result.len() as i64 + 1));
        result.push(candidate(CandidateSpec {
            name: display_name(&paths[0].0.file_stem().unwrap_or_default().to_string_lossy()),
            version: None,
            mod_type: if iostore { "iostore" } else { "pak" },
            files: snapshots,
            enabled: true,
            packages,
            deployment_keys: Vec::new(),
            warnings,
            blocked_reason: blocked,
            likely_runtime_component: false,
            inferred_priority: priority,
        }));
    }
    // Keep the unused parameter explicit: discovery ownership is evaluated from
    // summaries so Windows path comparisons can be case-insensitive.
    let _ = conn;
    Ok(result)
}

fn parse_mods_txt(path: &Path) -> HashMap<String, (bool, i64)> {
    let Ok(text) = fs::read_to_string(path) else {
        return HashMap::new();
    };
    text.lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let (name, value) = line.split_once(':')?;
            let name = name.trim();
            if name.is_empty() || name.starts_with(';') || name.starts_with('#') {
                return None;
            }
            let enabled = matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true");
            Some((name.to_ascii_lowercase(), (enabled, index as i64 + 1)))
        })
        .collect()
}

fn direct_manifest(folder: &Path) -> Option<ModManifest> {
    fs::read_dir(folder)
        .ok()?
        .filter_map(std::result::Result::ok)
        .find(|entry| entry.file_name().eq_ignore_ascii_case("zcom-mod.json"))
        .and_then(|entry| fs::read_to_string(entry.path()).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
}

fn scan_ue4ss(game: &Path, owned: &HashSet<String>) -> Result<Vec<CandidateSnapshot>> {
    let root = game.join(UE4SS_ROOT);
    let order = parse_mods_txt(&root.join("mods.txt"));
    let Ok(entries) = fs::read_dir(&root) else {
        return Ok(Vec::new());
    };
    let mut folders = entries
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .collect::<Vec<_>>();
    folders.sort_by_key(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase());
    let mut result = Vec::new();
    for entry in folders {
        let key = entry.file_name().to_string_lossy().to_string();
        if key.eq_ignore_ascii_case("shared") {
            continue;
        }
        let folder = entry.path();
        let mut recognized = false;
        let mut unsafe_entry = false;
        let mut files = Vec::new();
        for item in WalkDir::new(&folder).follow_links(false) {
            let Ok(item) = item else {
                unsafe_entry = true;
                continue;
            };
            if item.file_type().is_symlink() {
                unsafe_entry = true;
                continue;
            }
            if !item.file_type().is_file() {
                continue;
            }
            let path = item.path();
            let parent = path
                .parent()
                .and_then(Path::file_name)
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            let lua = parent.eq_ignore_ascii_case("scripts")
                && path
                    .file_name()
                    .is_some_and(|name| name.eq_ignore_ascii_case("main.lua"));
            let dll = parent.eq_ignore_ascii_case("dlls") && lower_extension(path) == "dll";
            recognized |= lua || dll;
            let relative = path
                .strip_prefix(&folder)
                .map_err(|error| AppError::Other(error.to_string()))?;
            files.push(metadata_snapshot(path, PathBuf::from(&key).join(relative))?);
        }
        if !recognized || files.is_empty() {
            continue;
        }
        let owned_count = files
            .iter()
            .filter(|file| owned.contains(&normalized(&file.source)))
            .count();
        if owned_count == files.len() {
            continue;
        }
        let blocked = if owned_count > 0 {
            Some("Some files in this UE4SS folder are already managed by ZCOM.".into())
        } else if unsafe_entry {
            Some("This folder contains a symbolic link or unreadable entry.".into())
        } else {
            None
        };
        let manifest = direct_manifest(&folder);
        let likely = RUNTIME_COMPONENTS
            .iter()
            .any(|name| key.eq_ignore_ascii_case(name));
        let mut warnings = Vec::new();
        if likely {
            warnings.push(
                "This folder commonly ships with UE4SS. Adopting and uninstalling it may damage the runtime."
                    .into(),
            );
        }
        let (enabled, priority) = order
            .get(&key.to_ascii_lowercase())
            .copied()
            .unwrap_or((false, result.len() as i64 + 1));
        result.push(candidate(CandidateSpec {
            name: manifest
                .as_ref()
                .map(|value| value.name.clone())
                .unwrap_or_else(|| display_name(&key)),
            version: manifest.and_then(|value| value.version),
            mod_type: "ue4ss",
            files,
            enabled,
            packages: Vec::new(),
            deployment_keys: vec![key],
            warnings,
            blocked_reason: blocked,
            likely_runtime_component: likely,
            inferred_priority: Some(priority),
        }));
    }
    Ok(result)
}

fn scan_logic_mods(game: &Path, owned: &HashSet<String>) -> Result<Vec<CandidateSnapshot>> {
    let root = game.join(LOGIC_ROOT);
    let mut result = Vec::new();
    for (path, regular) in regular_files(&root) {
        if lower_extension(&path) != "pak" || owned.contains(&normalized(&path)) {
            continue;
        }
        let name = path.file_stem().unwrap_or_default().to_string_lossy();
        let relative = PathBuf::from(LOGIC_ROOT).join(path.file_name().unwrap_or_default());
        result.push(candidate(CandidateSpec {
            name: display_name(&name),
            version: None,
            mod_type: "gamedir",
            files: vec![metadata_snapshot(&path, relative)?],
            enabled: true,
            packages: Vec::new(),
            deployment_keys: Vec::new(),
            warnings: Vec::new(),
            blocked_reason: (!regular)
                .then(|| "Symbolic links and other non-regular files cannot be adopted.".into()),
            likely_runtime_component: false,
            inferred_priority: None,
        }));
    }
    Ok(result)
}

fn unsupported_replacements(game: &Path) -> Vec<String> {
    let win64 = game.join("SWZeroCompany/Binaries/Win64");
    let mut result = Vec::new();
    for (path, regular) in regular_files(&win64) {
        if !regular {
            continue;
        }
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if INJECTOR_NAMES
            .iter()
            .any(|expected| name.eq_ignore_ascii_case(expected))
        {
            result.push(format!(
                "{name} looks like a ReShade or injector file. ZCOM cannot adopt it safely without the original game file."
            ));
        }
    }
    result
}

pub fn discover(
    conn: &Connection,
    game: &Path,
    tool: &ToolInfo,
) -> Result<(ExistingModScan, ScanSnapshot)> {
    let owned = owned_destinations(conn)?;
    let mut candidates = scan_packaged(conn, game, tool, &owned)?;
    candidates.extend(scan_ue4ss(game, &owned)?);
    candidates.extend(scan_logic_mods(game, &owned)?);
    let scan_id = Uuid::new_v4().to_string();
    let public = candidates.iter().map(|item| item.public.clone()).collect();
    let held = candidates
        .into_iter()
        .map(|item| (item.public.id.clone(), item))
        .collect();
    Ok((
        ExistingModScan {
            scan_id,
            candidates: public,
            unsupported: unsupported_replacements(game),
            warnings: vec![
                "Replacement movies, audio, and other shipped game files cannot be distinguished from originals and are intentionally not offered for adoption."
                    .into(),
            ],
        },
        ScanSnapshot {
            game: game.to_path_buf(),
            candidates: held,
        },
    ))
}

fn journal_path(data: &Path) -> PathBuf {
    data.join(JOURNAL_NAME)
}

fn write_journal(data: &Path, journal: &AdoptionJournal) -> Result<()> {
    fs::write(journal_path(data), serde_json::to_vec_pretty(journal)?)?;
    Ok(())
}

fn clear_journal(data: &Path) {
    let path = journal_path(data);
    if path.is_file() {
        let _ = fs::remove_file(path);
    }
}

pub fn recover(conn: &Connection, data: &Path) -> Result<()> {
    let path = journal_path(data);
    if !path.is_file() {
        return Ok(());
    }
    let journal: AdoptionJournal = serde_json::from_slice(&fs::read(&path)?)?;
    let recorded: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM mods WHERE id=?1)",
        [&journal.id],
        |row| row.get(0),
    )?;
    if recorded {
        if journal.final_path.exists() {
            if journal.temporary.exists() {
                fs::remove_dir_all(&journal.temporary)?;
            }
        } else if journal.temporary.exists() {
            fs::rename(&journal.temporary, &journal.final_path)?;
        } else {
            return Err(AppError::Other(format!(
                "Adopted mod {} is recorded but its managed library copy is missing.",
                journal.id
            )));
        }
    } else {
        if journal.temporary.exists() {
            fs::remove_dir_all(&journal.temporary)?;
        }
        if journal.final_path.exists() {
            fs::remove_dir_all(&journal.final_path)?;
        }
    }
    clear_journal(data);
    Ok(())
}

fn allowed_source(game: &Path, source: &Path) -> bool {
    [PACKAGED_ROOT, LOGIC_ROOT, UE4SS_ROOT].iter().any(|root| {
        let allowed = game.join(root);
        let Ok(allowed) = allowed.canonicalize() else {
            return false;
        };
        source
            .canonicalize()
            .is_ok_and(|path| path.starts_with(allowed))
    })
}

fn current_snapshot(file: &FileSnapshot) -> Result<()> {
    let metadata = fs::symlink_metadata(&file.source)?;
    if !metadata.file_type().is_file()
        || metadata.len() != file.size
        || metadata.modified().ok() != file.modified
    {
        return Err(AppError::Other(format!(
            "{} changed after discovery. Scan again before adopting it.",
            file.source.display()
        )));
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::Other("An adopted mod needs a name.".into()));
    }
    if name.chars().count() > 120 {
        return Err(AppError::Other(
            "That name is too long. Use 120 characters or fewer.".into(),
        ));
    }
    Ok(name.into())
}

fn adopt_group(
    conn: &mut Connection,
    library: &Path,
    data: &Path,
    scan: &ScanSnapshot,
    group: &AdoptionGroup,
    build: Option<String>,
) -> Result<ModSummary> {
    let name = validate_name(&group.name)?;
    if group.candidate_ids.is_empty() {
        return Err(AppError::Other(
            "Choose at least one candidate to adopt.".into(),
        ));
    }
    let mut seen = HashSet::new();
    let candidates = group
        .candidate_ids
        .iter()
        .map(|id| {
            if !seen.insert(id) {
                return Err(AppError::Other(
                    "An adoption group contains a duplicate candidate.".into(),
                ));
            }
            scan.candidates.get(id).ok_or_else(|| {
                AppError::Other("That discovery candidate expired. Scan again.".into())
            })
        })
        .collect::<Result<Vec<_>>>()?;
    if let Some(blocked) = candidates.iter().find(|item| !item.public.adoptable) {
        return Err(AppError::Other(
            blocked
                .public
                .blocked_reason
                .clone()
                .unwrap_or_else(|| "That candidate cannot be adopted safely.".into()),
        ));
    }
    let packaged = candidates
        .iter()
        .all(|item| matches!(item.public.mod_type.as_str(), "pak" | "iostore"));
    if candidates.len() > 1 && !packaged {
        return Err(AppError::Other(
            "Only packaged PAK/IoStore candidates can be merged.".into(),
        ));
    }
    let mut relative_names = HashSet::new();
    for candidate in &candidates {
        for file in &candidate.files {
            if !relative_names.insert(normalized(&file.library_relative)) {
                return Err(AppError::Other(
                    "Merged candidates contain the same managed filename.".into(),
                ));
            }
            if !allowed_source(&scan.game, &file.source) {
                return Err(AppError::Other(
                    "A discovered file is no longer inside an allowed mod folder.".into(),
                ));
            }
            current_snapshot(file)?;
            if database::destination_owner(conn, &file.source.display().to_string(), None)?
                .is_some()
            {
                return Err(AppError::DeploymentConflict(file.source.clone()));
            }
            if packaged
                && database::packaged_source_name_owner(
                    conn,
                    &file.library_relative.display().to_string(),
                    None,
                )?
                .is_some()
            {
                return Err(AppError::Other(format!(
                    "Another managed package already owns the logical filename {}.",
                    file.library_relative.display()
                )));
            }
        }
    }

    let id = Uuid::new_v4().to_string();
    let temporary = library.join(format!(".adopting-{id}"));
    let final_path = library.join(&id);
    let journal = AdoptionJournal {
        id: id.clone(),
        temporary: temporary.clone(),
        final_path: final_path.clone(),
    };
    write_journal(data, &journal)?;
    if let Err(error) = fs::create_dir_all(temporary.join("payload")) {
        clear_journal(data);
        return Err(error.into());
    }
    let copied = (|| -> Result<Vec<(String, String, u64, String)>> {
        let mut rows = Vec::new();
        for candidate in &candidates {
            for file in &candidate.files {
                current_snapshot(file)?;
                let target = temporary.join("payload").join(&file.library_relative);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&file.source, &target)?;
                let source_hash = sha256(&file.source)?;
                let copy_hash = sha256(&target)?;
                if source_hash != copy_hash {
                    return Err(AppError::ChecksumMismatch(file.source.clone()));
                }
                current_snapshot(file)?;
                rows.push((
                    file.library_relative.display().to_string(),
                    file.source.display().to_string(),
                    file.size,
                    source_hash,
                ));
            }
        }
        Ok(rows)
    })();
    let rows = match copied {
        Ok(rows) => rows,
        Err(error) => {
            let _ = fs::remove_dir_all(&temporary);
            clear_journal(data);
            return Err(error);
        }
    };
    if let Err(error) = fs::rename(&temporary, &final_path) {
        let _ = fs::remove_dir_all(&temporary);
        clear_journal(data);
        return Err(error.into());
    }

    let mod_type = if candidates
        .iter()
        .any(|item| item.public.mod_type == "iostore")
    {
        "iostore"
    } else {
        &candidates[0].public.mod_type
    };
    let enabled = candidates.iter().all(|item| item.public.enabled);
    let load_priority = candidates
        .iter()
        .filter_map(|item| item.public.inferred_priority)
        .max();
    let mut packages = candidates
        .iter()
        .flat_map(|item| item.packages.clone())
        .collect::<Vec<_>>();
    packages.sort();
    packages.dedup();
    let keys = candidates
        .iter()
        .flat_map(|item| item.deployment_keys.clone())
        .collect::<Vec<_>>();
    let summary = ModSummary {
        id: id.clone(),
        name,
        version: candidates
            .iter()
            .find_map(|item| item.public.version.clone()),
        mod_type: mod_type.into(),
        enabled,
        installed_at: Utc::now().to_rfc3339(),
        installed_build: build,
        package_count: packages.len(),
        conflict_count: 0,
        potential_conflict_count: 0,
        load_priority,
        // Attached after installation, when the archive is known to Nexus.
        nexus_mod_id: None,
        nexus_url: None,
        nexus_ignored: false,
        hidden: false,
        fomod: false,
        files: rows
            .iter()
            .map(|(_, destination, size, hash)| ModFile {
                name: Path::new(destination)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                destination: destination.clone(),
                size: *size,
                sha256: hash.clone(),
            })
            .collect(),
    };
    let result = (|| -> Result<()> {
        let tx = conn.transaction()?;
        database::insert_mod(&tx, &summary, &keys.join("\n"), None, &rows, &packages)?;
        tx.commit()?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&final_path);
        clear_journal(data);
        return Err(error);
    }
    clear_journal(data);
    Ok(summary)
}

pub fn adopt(
    conn: &mut Connection,
    library: &Path,
    data: &Path,
    scan: &ScanSnapshot,
    groups: &[AdoptionGroup],
    build: Option<String>,
) -> AdoptionReport {
    let mut submitted = HashSet::new();
    let mut outcomes = Vec::new();
    for group in groups {
        let duplicate = group
            .candidate_ids
            .iter()
            .any(|id| !submitted.insert(id.clone()));
        let result = if duplicate {
            Err(AppError::Other(
                "A candidate was submitted in more than one adoption group.".into(),
            ))
        } else {
            adopt_group(conn, library, data, scan, group, build.clone())
        };
        outcomes.push(match result {
            Ok(summary) => AdoptionOutcome {
                candidate_ids: group.candidate_ids.clone(),
                name: group.name.clone(),
                mod_summary: Some(summary),
                error: None,
            },
            Err(error) => AdoptionOutcome {
                candidate_ids: group.candidate_ids.clone(),
                name: group.name.clone(),
                mod_summary: None,
                error: Some(error.to_string()),
            },
        });
    }
    AdoptionReport { outcomes }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deployment;
    use tempfile::tempdir;

    fn write(path: &Path, body: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    fn record_owned(conn: &mut Connection, path: &Path) {
        let hash = sha256(path).unwrap();
        let summary = ModSummary {
            id: Uuid::new_v4().to_string(),
            name: "Owned".into(),
            version: None,
            mod_type: "pak".into(),
            enabled: true,
            installed_at: "2026-08-31T00:00:00Z".into(),
            installed_build: None,
            package_count: 0,
            conflict_count: 0,
            potential_conflict_count: 0,
            load_priority: Some(1),
            nexus_mod_id: None,
            nexus_url: None,
            nexus_ignored: false,
            hidden: false,
            fomod: false,
            files: Vec::new(),
        };
        let tx = conn.transaction().unwrap();
        database::insert_mod(
            &tx,
            &summary,
            "",
            None,
            &[(
                path.file_name().unwrap().to_string_lossy().to_string(),
                path.display().to_string(),
                fs::metadata(path).unwrap().len(),
                hash,
            )],
            &[],
        )
        .unwrap();
        tx.commit().unwrap();
    }

    #[test]
    fn discovers_pak_ue4ss_and_logic_mods_without_touching_them() {
        let root = tempdir().unwrap();
        let game = root.path().join("game");
        let pak = game.join(PACKAGED_ROOT).join("Example_P.pak");
        let lua = game.join(UE4SS_ROOT).join("MyMod/Scripts/main.lua");
        let logic = game.join(LOGIC_ROOT).join("Blueprint_P.pak");
        write(&pak, b"pak");
        write(&lua, b"lua");
        write(&logic, b"logic");
        write(&game.join(UE4SS_ROOT).join("mods.txt"), b"MyMod : 0\n");
        let conn = database::open(&root.path().join("db")).unwrap();
        let (scan, _) = discover(&conn, &game, &ToolInfo::default()).unwrap();
        assert_eq!(scan.candidates.len(), 3);
        assert!(scan.candidates.iter().any(|item| item.mod_type == "pak"));
        let runtime = scan
            .candidates
            .iter()
            .find(|item| item.mod_type == "ue4ss")
            .unwrap();
        assert!(!runtime.enabled);
        assert_eq!(fs::read(pak).unwrap(), b"pak");
        assert_eq!(fs::read(lua).unwrap(), b"lua");
    }

    #[test]
    fn incomplete_iostore_is_visible_but_blocked() {
        let root = tempdir().unwrap();
        let game = root.path().join("game");
        write(&game.join(PACKAGED_ROOT).join("Broken.utoc"), b"utoc");
        let conn = database::open(&root.path().join("db")).unwrap();
        let (scan, _) = discover(&conn, &game, &ToolInfo::default()).unwrap();
        assert_eq!(scan.candidates.len(), 1);
        assert!(!scan.candidates[0].adoptable);
        assert!(scan.candidates[0]
            .blocked_reason
            .as_deref()
            .unwrap()
            .contains("incomplete"));
    }

    #[test]
    fn runtime_components_are_unchecked_but_adoptable() {
        let root = tempdir().unwrap();
        let game = root.path().join("game");
        write(
            &game
                .join(UE4SS_ROOT)
                .join("ConsoleCommandsMod/Scripts/main.lua"),
            b"lua",
        );
        let conn = database::open(&root.path().join("db")).unwrap();
        let (scan, _) = discover(&conn, &game, &ToolInfo::default()).unwrap();
        assert!(scan.candidates[0].likely_runtime_component);
        assert!(!scan.candidates[0].selected_by_default);
        assert!(scan.candidates[0].adoptable);
    }

    #[test]
    fn adopts_each_group_independently_and_keeps_live_files() {
        let root = tempdir().unwrap();
        let game = root.path().join("game");
        let data = root.path().join("data");
        let library = data.join("mods");
        fs::create_dir_all(&library).unwrap();
        let first = game.join(PACKAGED_ROOT).join("First_P.pak");
        let second = game.join(PACKAGED_ROOT).join("Second_P.pak");
        write(&first, b"first");
        write(&second, b"second");
        let mut conn = database::open(&data.join("db")).unwrap();
        let (public, held) = discover(&conn, &game, &ToolInfo::default()).unwrap();
        let groups = public
            .candidates
            .iter()
            .map(|item| AdoptionGroup {
                candidate_ids: vec![item.id.clone()],
                name: item.name.clone(),
            })
            .collect::<Vec<_>>();
        fs::write(&second, b"changed").unwrap();
        let report = adopt(&mut conn, &library, &data, &held, &groups, None);
        assert_eq!(
            report
                .outcomes
                .iter()
                .filter(|item| item.error.is_none())
                .count(),
            1
        );
        assert_eq!(
            report
                .outcomes
                .iter()
                .filter(|item| item.error.is_some())
                .count(),
            1
        );
        assert_eq!(fs::read(first).unwrap(), b"first");
        assert_eq!(fs::read(second).unwrap(), b"changed");
        assert_eq!(database::counts(&conn).unwrap().0, 1);
    }

    #[test]
    fn owned_families_are_excluded_and_partial_ownership_is_blocked() {
        let root = tempdir().unwrap();
        let game = root.path().join("game");
        let owned = game.join(PACKAGED_ROOT).join("Owned_P.pak");
        let partial_pak = game.join(PACKAGED_ROOT).join("Partial_P.pak");
        write(&owned, b"owned");
        write(&partial_pak, b"pak");
        write(&game.join(PACKAGED_ROOT).join("Partial_P.utoc"), b"utoc");
        write(&game.join(PACKAGED_ROOT).join("Partial_P.ucas"), b"ucas");
        let mut conn = database::open(&root.path().join("db")).unwrap();
        record_owned(&mut conn, &owned);
        record_owned(&mut conn, &partial_pak);
        let (scan, _) = discover(&conn, &game, &ToolInfo::default()).unwrap();
        assert_eq!(scan.candidates.len(), 1);
        assert_eq!(scan.candidates[0].name, "Partial");
        assert!(!scan.candidates[0].adoptable);
        assert!(scan.candidates[0]
            .blocked_reason
            .as_deref()
            .unwrap()
            .contains("already managed"));
    }

    #[test]
    fn merges_packaged_families_into_one_library_entry() {
        let root = tempdir().unwrap();
        let game = root.path().join("game");
        let data = root.path().join("data");
        let library = data.join("mods");
        fs::create_dir_all(&library).unwrap();
        write(&game.join(PACKAGED_ROOT).join("One_P.pak"), b"one");
        write(&game.join(PACKAGED_ROOT).join("Two_P.pak"), b"two");
        let mut conn = database::open(&data.join("db")).unwrap();
        let (public, held) = discover(&conn, &game, &ToolInfo::default()).unwrap();
        let report = adopt(
            &mut conn,
            &library,
            &data,
            &held,
            &[AdoptionGroup {
                candidate_ids: public
                    .candidates
                    .iter()
                    .map(|item| item.id.clone())
                    .collect(),
                name: "Together".into(),
            }],
            None,
        );
        assert!(report.outcomes[0].error.is_none());
        let mods = database::list_mods(&conn).unwrap();
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].files.len(), 2);
        assert_eq!(
            fs::read(game.join(PACKAGED_ROOT).join("One_P.pak")).unwrap(),
            b"one"
        );
    }

    #[test]
    fn an_adopted_disabled_ue4ss_mod_keeps_state_and_has_a_full_lifecycle() {
        let root = tempdir().unwrap();
        let game = root.path().join("game");
        let data = root.path().join("data");
        let library = data.join("mods");
        fs::create_dir_all(&library).unwrap();
        let lua = game.join(UE4SS_ROOT).join("Quiet/Scripts/main.lua");
        let order = game.join(UE4SS_ROOT).join("mods.txt");
        write(&lua, b"lua");
        write(&order, b"; keep me\nQuiet : 0\n");
        let before = fs::read(&order).unwrap();
        let mut conn = database::open(&data.join("db")).unwrap();
        let (public, held) = discover(&conn, &game, &ToolInfo::default()).unwrap();
        let report = adopt(
            &mut conn,
            &library,
            &data,
            &held,
            &[AdoptionGroup {
                candidate_ids: vec![public.candidates[0].id.clone()],
                name: "Quiet".into(),
            }],
            None,
        );
        let adopted = report.outcomes[0].mod_summary.as_ref().unwrap();
        assert!(!adopted.enabled);
        assert_eq!(fs::read(&order).unwrap(), before);
        assert!(lua.is_file());

        deployment::set_enabled(&conn, &library, &game, &adopted.id, true).unwrap();
        assert!(lua.is_file());
        assert!(fs::read_to_string(&order).unwrap().contains("Quiet : 1"));
        deployment::set_enabled(&conn, &library, &game, &adopted.id, false).unwrap();
        assert!(!lua.exists());
        deployment::set_enabled(&conn, &library, &game, &adopted.id, true).unwrap();
        deployment::uninstall(&conn, &library, &adopted.id, false, Some(&game)).unwrap();
        assert!(!lua.exists());
        assert_eq!(database::counts(&conn).unwrap().0, 0);
    }

    #[test]
    fn recovery_removes_only_the_unrecorded_adoption_paths() {
        let root = tempdir().unwrap();
        let data = root.path().join("data");
        let temporary = data.join("mods/.adopting-id");
        let final_path = data.join("mods/id");
        fs::create_dir_all(&temporary).unwrap();
        fs::create_dir_all(&final_path).unwrap();
        write_journal(
            &data,
            &AdoptionJournal {
                id: "id".into(),
                temporary: temporary.clone(),
                final_path: final_path.clone(),
            },
        )
        .unwrap();
        let conn = database::open(&data.join("db")).unwrap();
        recover(&conn, &data).unwrap();
        assert!(!temporary.exists());
        assert!(!final_path.exists());
        assert!(!journal_path(&data).exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_payloads_are_visible_but_blocked() {
        use std::os::unix::fs::symlink;
        let root = tempdir().unwrap();
        let game = root.path().join("game");
        let real = root.path().join("outside.pak");
        write(&real, b"outside");
        let linked = game.join(PACKAGED_ROOT).join("Linked_P.pak");
        fs::create_dir_all(linked.parent().unwrap()).unwrap();
        symlink(real, linked).unwrap();
        let conn = database::open(&root.path().join("db")).unwrap();
        let (scan, _) = discover(&conn, &game, &ToolInfo::default()).unwrap();
        assert_eq!(scan.candidates.len(), 1);
        assert!(!scan.candidates[0].adoptable);
    }
}
