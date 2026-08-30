use crate::{
    database,
    deployment::sha256,
    error::{AppError, Result},
    models::{
        ConflictGroup, LoadOrderEntry, LoadOrderMove, LoadOrderPreview, LoadOrderState, ModSummary,
        WinnerChange,
    },
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

const MAX_PRIORITY: usize = 9_999;

#[derive(Debug, Clone)]
struct FileUpdate {
    mod_id: String,
    library_relative: String,
    old: PathBuf,
    new: PathBuf,
    expected: String,
    enabled: bool,
}

#[derive(Debug)]
struct PlannedOrder {
    ordered_ids: Vec<String>,
    priorities: HashMap<String, i64>,
    updates: Vec<FileUpdate>,
    conflicts: Vec<ConflictGroup>,
    winner_changes: Vec<WinnerChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JournalMove {
    mod_id: String,
    library_relative: String,
    old: PathBuf,
    temporary: PathBuf,
    new: PathBuf,
    expected: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Journal {
    moves: Vec<JournalMove>,
}

fn extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn support(summary: &ModSummary) -> (bool, Option<String>) {
    match summary.mod_type.as_str() {
        "pak" => (
            false,
            Some("PAK-only ordering did not pass the Zero Company runtime capability test.".into()),
        ),
        "iostore"
            if summary
                .files
                .iter()
                .any(|file| extension(&file.name) == "pak") =>
        {
            (true, None)
        }
        "iostore" => (
            false,
            Some(
                "IoStore pairs without a companion PAK are not runtime-verified for ordering."
                    .into(),
            ),
        ),
        "gamedir" => (
            false,
            Some("Game-folder mods are placed at fixed paths and are not ordered.".into()),
        ),
        _ => (
            false,
            Some("UE4SS mods use their own runtime ordering.".into()),
        ),
    }
}

pub fn managed_filename(original: &str, priority: i64) -> Result<String> {
    if !(1..=MAX_PRIORITY as i64).contains(&priority) {
        return Err(AppError::InvalidLoadOrder(format!(
            "priority must be between 1 and {MAX_PRIORITY}"
        )));
    }
    let path = Path::new(original);
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .ok_or_else(|| AppError::InvalidLoadOrder(format!("{original} has no extension")))?;
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| AppError::InvalidLoadOrder(format!("{original} has no usable name")))?;
    let base = if stem.to_ascii_lowercase().ends_with("_p") {
        &stem[..stem.len() - 2]
    } else {
        stem
    };
    Ok(format!("{base}_{priority:04}_P.{extension}"))
}

fn conflicts_with_priorities(
    conn: &Connection,
    mods: &[ModSummary],
    priorities: &HashMap<String, i64>,
) -> Result<Vec<ConflictGroup>> {
    let mut packages: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (package, mod_id) in database::package_members(conn)? {
        packages.entry(package).or_default().push(mod_id);
    }
    let enabled = mods
        .iter()
        .map(|item| (item.id.clone(), item.enabled))
        .collect::<HashMap<_, _>>();
    let orderable = mods
        .iter()
        .map(|item| (item.id.clone(), support(item).0))
        .collect::<HashMap<_, _>>();
    let mut grouped: BTreeMap<Vec<String>, usize> = BTreeMap::new();
    for mut members in packages.into_values() {
        members.sort();
        members.dedup();
        if members.len() > 1 {
            *grouped.entry(members).or_default() += 1;
        }
    }
    Ok(grouped
        .into_iter()
        .enumerate()
        .map(|(index, (member_ids, package_count))| {
            let enabled_members = member_ids
                .iter()
                .filter(|id| enabled.get(*id).copied().unwrap_or(false))
                .collect::<Vec<_>>();
            let winner_id = member_ids
                .iter()
                .all(|id| orderable.get(id).copied().unwrap_or(false))
                .then(|| {
                    enabled_members
                        .iter()
                        .max_by_key(|id| priorities.get(id.as_str()).copied().unwrap_or(0))
                        .map(|id| (*id).clone())
                })
                .flatten();
            let active = enabled_members.len() > 1;
            let potential = enabled_members.len() < member_ids.len();
            ConflictGroup {
                id: format!("overlap-{}", index + 1),
                member_ids,
                package_count,
                active,
                potential,
                winner_id,
            }
        })
        .collect())
}

/// UE4SS mods in runtime start order.
///
/// `mods.txt` is read top to bottom, so the first line starts first: the
/// opposite of the packaged list, where the highest priority wins. The two are
/// presented as separate lists for that reason.
/// Which of the runtime's start passes a UE4SS mod belongs to.
///
/// UE4SS does not start every mod in one sequence. It starts the DLL mods from
/// the `mods.txt` order first, and only once the Lua state exists does it walk
/// `mods.txt` again for the script mods. A DLL mod therefore always starts
/// before every Lua mod, whatever the file says, and the two are ordered
/// independently rather than interleaved.
fn runtime_kind(item: &ModSummary) -> &'static str {
    let holds = |folder: &str, extension: &str| {
        item.files.iter().any(|file| {
            let path = file.destination.to_ascii_lowercase().replace('\\', "/");
            path.contains(folder) && path.ends_with(extension)
        })
    };
    match (holds("/dlls/", ".dll"), holds("/scripts/", ".lua")) {
        (true, true) => "mixed",
        (true, false) => "native",
        _ => "script",
    }
}

/// DLL mods start before Lua mods, so they rank first. A mod shipping both
/// starts its native half in the first pass, so it ranks with the DLL mods.
fn kind_rank(kind: &str) -> u8 {
    match kind {
        "native" | "mixed" => 0,
        _ => 1,
    }
}

fn ue4ss_entries(mods: &[ModSummary]) -> Vec<LoadOrderEntry> {
    let mut items: Vec<(&ModSummary, &'static str)> = mods
        .iter()
        .filter(|item| item.mod_type == "ue4ss")
        .map(|item| (item, runtime_kind(item)))
        .collect();
    items.sort_by(|(left, left_kind), (right, right_kind)| {
        kind_rank(left_kind)
            .cmp(&kind_rank(right_kind))
            .then_with(|| match (left.load_priority, right.load_priority) {
                (Some(left), Some(right)) => left.cmp(&right),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                // A mod installed before start order was recorded has no slot.
                // Each install appended its entry to `mods.txt`, so
                // installation order is the order the file already has.
                (None, None) => left.installed_at.cmp(&right.installed_at),
            })
            .then_with(|| left.name.cmp(&right.name))
    });
    items
        .into_iter()
        .map(|(item, kind)| LoadOrderEntry {
            id: item.id.clone(),
            name: item.name.clone(),
            mod_type: item.mod_type.clone(),
            runtime_kind: Some(kind.into()),
            enabled: item.enabled,
            priority: item.load_priority,
            supported: true,
            support_reason: None,
            applied: item.load_priority.is_some(),
            active_conflict_count: 0,
            potential_conflict_count: 0,
        })
        .collect()
}

/// Records a UE4SS start order and writes it to `mods.txt`.
///
/// Nothing on disk moves: the runtime reads the order from that one file, so
/// applying it is a single text rewrite with no renames to roll back.
pub fn apply_ue4ss_order(
    conn: &mut Connection,
    game: &Path,
    ordered_ids: &[String],
) -> Result<LoadOrderState> {
    let mods = database::list_mods(conn)?;
    let known: HashMap<&str, &ModSummary> = mods
        .iter()
        .filter(|item| item.mod_type == "ue4ss")
        .map(|item| (item.id.as_str(), item))
        .collect();
    let proposed: BTreeSet<&String> = ordered_ids.iter().collect();
    if proposed.len() != ordered_ids.len() {
        return Err(AppError::InvalidLoadOrder(
            "the order contains a duplicate mod".into(),
        ));
    }
    if proposed.len() != known.len()
        || ordered_ids
            .iter()
            .any(|id| !known.contains_key(id.as_str()))
    {
        return Err(AppError::InvalidLoadOrder(
            "the order must list every installed UE4SS mod exactly once".into(),
        ));
    }
    // A caller may hand back a sequence that interleaves the two passes. The
    // runtime cannot honour that, so it is normalised into pass order here
    // rather than recorded as something that will not happen.
    let mut ordered_ids: Vec<String> = ordered_ids.to_vec();
    ordered_ids.sort_by_key(|id| {
        known
            .get(id.as_str())
            .map_or(1, |item| kind_rank(runtime_kind(item)))
    });
    let mut lines = Vec::new();
    for id in &ordered_ids {
        let record = database::mod_record(conn, id)?;
        for key in record.keys {
            lines.push((key, record.enabled));
        }
    }
    let tx = conn.transaction()?;
    for (index, id) in ordered_ids.iter().enumerate() {
        database::set_load_priority(&tx, id, index as i64 + 1)?;
    }
    tx.commit()?;
    crate::ue4ss::write_order(game, &lines)?;
    state(conn)
}

pub fn state(conn: &Connection) -> Result<LoadOrderState> {
    let mods = database::list_mods(conn)?;
    let priorities = mods
        .iter()
        .filter_map(|item| {
            item.load_priority
                .map(|priority| (item.id.clone(), priority))
        })
        .collect::<HashMap<_, _>>();
    let conflicts = conflicts_with_priorities(conn, &mods, &priorities)?;
    let mut entries = Vec::new();
    for item in mods
        .iter()
        .filter(|item| matches!(item.mod_type.as_str(), "pak" | "iostore"))
    {
        let (supported, support_reason) = support(item);
        let records = database::file_records(conn, &item.id)?;
        let applied = if supported {
            match item.load_priority {
                Some(priority) => item.files.iter().all(|file| {
                    let original = records
                        .iter()
                        .find(|(_, destination, _, _)| destination == &file.destination)
                        .map(|(library, _, _, _)| library);
                    original
                        .and_then(|name| managed_filename(name, priority).ok())
                        .is_some_and(|expected| {
                            Path::new(&file.destination)
                                .file_name()
                                .is_some_and(|actual| actual == expected.as_str())
                        })
                }),
                None => false,
            }
        } else {
            false
        };
        entries.push(LoadOrderEntry {
            id: item.id.clone(),
            name: item.name.clone(),
            mod_type: item.mod_type.clone(),
            runtime_kind: None,
            enabled: item.enabled,
            priority: item.load_priority,
            supported,
            support_reason,
            applied,
            active_conflict_count: item.conflict_count,
            potential_conflict_count: item.potential_conflict_count,
        });
    }
    entries.sort_by(|left, right| {
        right
            .supported
            .cmp(&left.supported)
            .then_with(|| right.priority.cmp(&left.priority))
            .then_with(|| left.name.cmp(&right.name))
    });
    let supported_count = entries.iter().filter(|entry| entry.supported).count() as i64;
    let priorities_contiguous = entries
        .iter()
        .filter(|entry| entry.supported)
        .enumerate()
        .all(|(index, entry)| entry.priority == Some(supported_count - index as i64));
    let active_conflicts = conflicts
        .iter()
        .filter(|group| group.active)
        .cloned()
        .collect();
    let potential_conflicts = conflicts
        .into_iter()
        .filter(|group| group.potential)
        .collect();
    Ok(LoadOrderState {
        ue4ss_entries: ue4ss_entries(&mods),
        unapplied: !priorities_contiguous
            || entries
                .iter()
                .any(|entry| entry.supported && !entry.applied),
        entries,
        active_conflicts,
        potential_conflicts,
    })
}

fn build_plan(conn: &Connection, ordered_ids: &[String]) -> Result<PlannedOrder> {
    if ordered_ids.len() > MAX_PRIORITY {
        return Err(AppError::InvalidLoadOrder(format!(
            "at most {MAX_PRIORITY} packaged mods can be ordered"
        )));
    }
    let mods = database::list_mods(conn)?;
    let supported = mods
        .iter()
        .filter(|item| matches!(item.mod_type.as_str(), "pak" | "iostore") && support(item).0)
        .map(|item| item.id.clone())
        .collect::<BTreeSet<_>>();
    let proposed = ordered_ids.iter().cloned().collect::<BTreeSet<_>>();
    if proposed.len() != ordered_ids.len() {
        return Err(AppError::InvalidLoadOrder(
            "the order contains a duplicate mod".into(),
        ));
    }
    if proposed != supported {
        return Err(AppError::InvalidLoadOrder(
            "the order must contain every supported packaged mod exactly once".into(),
        ));
    }
    let count = ordered_ids.len() as i64;
    let priorities = ordered_ids
        .iter()
        .enumerate()
        .map(|(index, id)| (id.clone(), count - index as i64))
        .collect::<HashMap<_, _>>();
    let current_priorities = mods
        .iter()
        .filter_map(|item| {
            item.load_priority
                .map(|priority| (item.id.clone(), priority))
        })
        .collect::<HashMap<_, _>>();
    let current_conflicts = conflicts_with_priorities(conn, &mods, &current_priorities)?;
    let conflicts = conflicts_with_priorities(conn, &mods, &priorities)?;
    let old_winners = current_conflicts
        .into_iter()
        .map(|group| (group.id, group.winner_id))
        .collect::<HashMap<_, _>>();
    let winner_changes = conflicts
        .iter()
        .filter_map(|group| {
            let from = old_winners.get(&group.id).cloned().flatten();
            (from != group.winner_id).then(|| WinnerChange {
                conflict_id: group.id.clone(),
                from_mod_id: from,
                to_mod_id: group.winner_id.clone(),
            })
        })
        .collect();
    let by_id = mods
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let mut updates = Vec::new();
    let mut targets = HashSet::new();
    for id in ordered_ids {
        let item = by_id[id.as_str()];
        let priority = priorities[id];
        for (library, destination, _, expected) in database::file_records(conn, id)? {
            let old = PathBuf::from(&destination);
            let parent = old.parent().ok_or_else(|| {
                AppError::InvalidLoadOrder("a deployment path has no parent".into())
            })?;
            let new = parent.join(managed_filename(&library, priority)?);
            if !targets.insert(new.clone()) {
                return Err(AppError::InvalidLoadOrder(format!(
                    "two managed files would become {}",
                    new.file_name().unwrap_or_default().to_string_lossy()
                )));
            }
            updates.push(FileUpdate {
                mod_id: id.clone(),
                library_relative: library,
                old,
                new,
                expected,
                enabled: item.enabled,
            });
        }
    }
    let current_paths = updates
        .iter()
        .filter(|update| update.enabled)
        .map(|update| update.old.clone())
        .collect::<HashSet<_>>();
    for update in &updates {
        if update.new != update.old && update.new.exists() && !current_paths.contains(&update.new) {
            return Err(AppError::DeploymentConflict(update.new.clone()));
        }
    }
    for update in updates.iter().filter(|update| update.enabled) {
        if !update.old.exists() {
            return Err(AppError::Other(format!(
                "A deployed file is missing: {}",
                update.old.display()
            )));
        }
        if sha256(&update.old)? != update.expected {
            return Err(AppError::ChecksumMismatch(update.old.clone()));
        }
    }
    Ok(PlannedOrder {
        ordered_ids: ordered_ids.to_vec(),
        priorities,
        updates,
        conflicts,
        winner_changes,
    })
}

pub fn preview(conn: &Connection, ordered_ids: &[String]) -> Result<LoadOrderPreview> {
    let plan = build_plan(conn, ordered_ids)?;
    let active_conflicts = plan
        .conflicts
        .iter()
        .filter(|group| group.active)
        .cloned()
        .collect();
    let potential_conflicts = plan
        .conflicts
        .into_iter()
        .filter(|group| group.potential)
        .collect();
    Ok(LoadOrderPreview {
        ordered_mod_ids: plan.ordered_ids,
        moves: plan
            .updates
            .iter()
            .filter(|update| update.enabled && update.old != update.new)
            .map(|update| LoadOrderMove {
                mod_id: update.mod_id.clone(),
                from: update
                    .old
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                to: update
                    .new
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            })
            .collect(),
        active_conflicts,
        potential_conflicts,
        winner_changes: plan.winner_changes,
    })
}

fn rollback_moves(moves: &[JournalMove]) -> bool {
    let mut complete = true;
    for item in moves.iter().rev() {
        let source = if item.temporary.exists() {
            Some(&item.temporary)
        } else if item.new.exists() {
            Some(&item.new)
        } else {
            None
        };
        if item.old.exists() {
            continue;
        }
        if let Some(source) = source {
            if fs::rename(source, &item.old).is_err() {
                complete = false;
            }
        } else {
            complete = false;
        }
    }
    complete
}

fn move_files(moves: &[JournalMove]) -> Result<()> {
    for item in moves {
        fs::rename(&item.old, &item.temporary)?;
    }
    for item in moves {
        fs::rename(&item.temporary, &item.new)?;
    }
    Ok(())
}

pub fn apply(
    conn: &mut Connection,
    ordered_ids: &[String],
    journal_path: &Path,
) -> Result<LoadOrderState> {
    let plan = build_plan(conn, ordered_ids)?;
    let operation = Uuid::new_v4();
    let moves = plan
        .updates
        .iter()
        .filter(|update| update.enabled && update.old != update.new)
        .enumerate()
        .map(|(index, update)| JournalMove {
            mod_id: update.mod_id.clone(),
            library_relative: update.library_relative.clone(),
            old: update.old.clone(),
            temporary: update
                .old
                .parent()
                .unwrap()
                .join(format!(".zcom-order-{operation}-{index}")),
            new: update.new.clone(),
            expected: update.expected.clone(),
        })
        .collect::<Vec<_>>();
    if !moves.is_empty() {
        fs::write(
            journal_path,
            serde_json::to_vec_pretty(&Journal {
                moves: moves.clone(),
            })?,
        )?;
    }
    let filesystem_result = move_files(&moves);
    if let Err(error) = filesystem_result {
        if rollback_moves(&moves) {
            let _ = fs::remove_file(journal_path);
        }
        return Err(error);
    }
    let database_result = (|| -> Result<()> {
        let transaction = conn.transaction()?;
        for id in &plan.ordered_ids {
            database::set_load_priority(&transaction, id, plan.priorities[id])?;
        }
        for update in &plan.updates {
            database::update_file_destination(
                &transaction,
                &update.mod_id,
                &update.library_relative,
                &update.new.display().to_string(),
            )?;
        }
        transaction.commit()?;
        Ok(())
    })();
    if let Err(error) = database_result {
        if rollback_moves(&moves) {
            let _ = fs::remove_file(journal_path);
        }
        return Err(error);
    }
    if journal_path.exists() {
        fs::remove_file(journal_path)?;
    }
    state(conn)
}

pub fn recover(conn: &Connection, journal_path: &Path) -> Result<()> {
    if !journal_path.exists() {
        return Ok(());
    }
    let journal: Journal = serde_json::from_slice(&fs::read(journal_path)?)?;
    for item in &journal.moves {
        let recorded = database::recorded_destination(conn, &item.mod_id, &item.library_relative)?;
        let committed = recorded.as_deref() == Some(item.new.to_string_lossy().as_ref());
        let (destination, candidates) = if committed {
            (&item.new, [&item.new, &item.temporary, &item.old])
        } else {
            (&item.old, [&item.old, &item.temporary, &item.new])
        };
        if destination.exists() {
            if sha256(destination)? != item.expected {
                return Err(AppError::ChecksumMismatch(destination.clone()));
            }
            continue;
        }
        let source = candidates
            .into_iter()
            .find(|path| path.exists() && sha256(path).ok().as_deref() == Some(&item.expected))
            .ok_or_else(|| {
                AppError::Other("A load-order operation could not be recovered safely.".into())
            })?;
        fs::rename(source, destination)?;
    }
    fs::remove_file(journal_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        deployment,
        models::{PayloadFile, StagedMod},
    };
    use tempfile::tempdir;

    fn game(root: &Path) {
        fs::create_dir_all(root.join("SWZeroCompany/Content/Paks")).unwrap();
        fs::create_dir_all(root.join("SWZeroCompany/Binaries/Win64")).unwrap();
        fs::write(
            root.join("SWZeroCompany/Binaries/Win64/SWZeroCompany.exe"),
            b"",
        )
        .unwrap();
    }

    fn staged(root: &Path, name: &str, packages: &[&str]) -> StagedMod {
        let files = ["pak", "utoc", "ucas"]
            .into_iter()
            .map(|extension| {
                let filename = format!("{name}_P.{extension}");
                let source = root.join(&filename);
                fs::write(&source, format!("{name}-{extension}")).unwrap();
                PayloadFile {
                    source,
                    library_relative: filename.clone().into(),
                    destination_relative: filename.into(),
                }
            })
            .collect();
        StagedMod {
            staging_id: name.into(),
            staging_root: root.into(),
            source_archive: root.display().to_string(),
            name: name.into(),
            version: None,
            author: None,
            description: None,
            mod_type: "iostore".into(),
            deployment_keys: Vec::new(),
            files,
            packages: packages.iter().map(|value| (*value).into()).collect(),
            verification: "not-required".into(),
            verification_details: None,
        }
    }

    #[test]
    fn appends_a_deterministic_patch_rank() {
        assert_eq!(
            managed_filename("Example_P.pak", 3).unwrap(),
            "Example_0003_P.pak"
        );
        assert_eq!(
            managed_filename("Example_12_P.utoc", 42).unwrap(),
            "Example_12_0042_P.utoc"
        );
        assert_eq!(
            managed_filename("Example.ucas", 1).unwrap(),
            "Example_0001_P.ucas"
        );
        let pair =
            ["Example_P.utoc", "Example_P.ucas"].map(|name| managed_filename(name, 7).unwrap());
        assert_eq!(pair, ["Example_0007_P.utoc", "Example_0007_P.ucas"]);
        let triplet = ["Example_P.pak", "Example_P.utoc", "Example_P.ucas"]
            .map(|name| managed_filename(name, 8).unwrap());
        assert_eq!(
            triplet,
            [
                "Example_0008_P.pak",
                "Example_0008_P.utoc",
                "Example_0008_P.ucas"
            ]
        );
        assert_eq!(
            managed_filename("SecondContainer_P.pak", 8).unwrap(),
            "SecondContainer_0008_P.pak"
        );
    }

    #[test]
    fn applies_order_and_updates_active_winner_and_lifecycle_paths() {
        let directory = tempdir().unwrap();
        let game_root = directory.path().join("game");
        let library = directory.path().join("library");
        let source_a = directory.path().join("source-a");
        let source_b = directory.path().join("source-b");
        fs::create_dir_all(&library).unwrap();
        fs::create_dir_all(&source_a).unwrap();
        fs::create_dir_all(&source_b).unwrap();
        game(&game_root);
        let mut conn = database::open(&directory.path().join("db")).unwrap();
        let a = deployment::install(
            &mut conn,
            &library,
            &game_root,
            &staged(&source_a, "Alpha", &["shared"]),
            None,
        )
        .unwrap();
        let b = deployment::install(
            &mut conn,
            &library,
            &game_root,
            &staged(&source_b, "Bravo", &["shared"]),
            None,
        )
        .unwrap();
        assert_eq!(
            state(&conn).unwrap().active_conflicts[0].winner_id,
            Some(b.id.clone())
        );
        assert_eq!(a.load_priority, Some(1));
        assert_eq!(b.load_priority, Some(2));

        let journal = directory.path().join("journal.json");
        let result = apply(&mut conn, &[a.id.clone(), b.id.clone()], &journal).unwrap();
        assert_eq!(result.active_conflicts[0].winner_id, Some(a.id.clone()));
        let mods_path = game_root.join("SWZeroCompany/Content/Paks/~mods");
        assert!(mods_path.join("Alpha_0002_P.pak").exists());
        assert!(mods_path.join("Bravo_0001_P.pak").exists());
        deployment::set_enabled(&conn, &library, &game_root, &a.id, false).unwrap();
        assert!(!mods_path.join("Alpha_0002_P.pak").exists());
        assert_eq!(state(&conn).unwrap().potential_conflicts.len(), 1);
        deployment::set_enabled(&conn, &library, &game_root, &a.id, true).unwrap();
        deployment::verify(&conn, &a.id).unwrap();
        assert!(mods_path.join("Alpha_0002_P.pak").exists());
        deployment::uninstall(&conn, &library, &a.id, false, Some(&game_root)).unwrap();
        assert!(!mods_path.join("Alpha_0002_P.pak").exists());
    }

    #[test]
    fn separates_active_and_potential_overlaps_and_recalculates_the_winner() {
        let directory = tempdir().unwrap();
        let game_root = directory.path().join("game");
        let library = directory.path().join("library");
        let source_a = directory.path().join("source-a");
        let source_b = directory.path().join("source-b");
        let source_c = directory.path().join("source-c");
        for path in [&library, &source_a, &source_b, &source_c] {
            fs::create_dir_all(path).unwrap();
        }
        game(&game_root);
        let mut conn = database::open(&directory.path().join("db")).unwrap();
        let a = deployment::install(
            &mut conn,
            &library,
            &game_root,
            &staged(&source_a, "Alpha", &["shared"]),
            None,
        )
        .unwrap();
        let b = deployment::install(
            &mut conn,
            &library,
            &game_root,
            &staged(&source_b, "Bravo", &["shared"]),
            None,
        )
        .unwrap();
        let c = deployment::install(
            &mut conn,
            &library,
            &game_root,
            &staged(&source_c, "Charlie", &["shared"]),
            None,
        )
        .unwrap();

        let active = state(&conn).unwrap();
        assert_eq!(active.active_conflicts.len(), 1);
        assert_eq!(active.potential_conflicts.len(), 0);
        assert_eq!(active.active_conflicts[0].winner_id, Some(c.id.clone()));

        deployment::set_enabled(&conn, &library, &game_root, &c.id, false).unwrap();
        let mixed = state(&conn).unwrap();
        assert_eq!(mixed.active_conflicts.len(), 1);
        assert_eq!(mixed.potential_conflicts.len(), 1);
        assert_eq!(
            mixed.active_conflicts[0].id,
            mixed.potential_conflicts[0].id
        );
        assert_eq!(mixed.active_conflicts[0].winner_id, Some(b.id.clone()));

        deployment::set_enabled(&conn, &library, &game_root, &b.id, false).unwrap();
        let potential = state(&conn).unwrap();
        assert_eq!(potential.potential_conflicts.len(), 1);
        assert_eq!(
            potential.potential_conflicts[0].winner_id,
            Some(a.id.clone())
        );
        assert!(potential
            .entries
            .iter()
            .all(|entry| entry.potential_conflict_count == 2));

        deployment::set_enabled(&conn, &library, &game_root, &a.id, false).unwrap();
        assert_eq!(state(&conn).unwrap().potential_conflicts[0].winner_id, None);
    }

    #[test]
    fn injected_rename_failure_can_be_completely_rolled_back() {
        let directory = tempdir().unwrap();
        let old_a = directory.path().join("a-old");
        let old_b = directory.path().join("b-old");
        fs::write(&old_a, b"a").unwrap();
        fs::write(&old_b, b"b").unwrap();
        let moves = [(&old_a, "a"), (&old_b, "b")]
            .into_iter()
            .map(|(old, name)| JournalMove {
                mod_id: name.into(),
                library_relative: name.into(),
                old: old.clone(),
                temporary: directory.path().join(format!("{name}-temporary")),
                new: directory.path().join(format!("{name}-new")),
                expected: String::new(),
            })
            .collect::<Vec<_>>();
        fs::create_dir(&moves[1].temporary).unwrap();

        assert!(move_files(&moves).is_err());
        assert!(rollback_moves(&moves));
        assert!(old_a.exists());
        assert!(old_b.exists());
        assert!(!moves[0].temporary.exists());
        assert!(!moves[0].new.exists());
    }

    #[test]
    fn database_failure_rolls_back_all_files_and_priorities() {
        let directory = tempdir().unwrap();
        let game_root = directory.path().join("game");
        let library = directory.path().join("library");
        let source_a = directory.path().join("source-a");
        let source_b = directory.path().join("source-b");
        for path in [&library, &source_a, &source_b] {
            fs::create_dir_all(path).unwrap();
        }
        game(&game_root);
        let mut conn = database::open(&directory.path().join("db")).unwrap();
        let a = deployment::install(
            &mut conn,
            &library,
            &game_root,
            &staged(&source_a, "Alpha", &[]),
            None,
        )
        .unwrap();
        let b = deployment::install(
            &mut conn,
            &library,
            &game_root,
            &staged(&source_b, "Bravo", &[]),
            None,
        )
        .unwrap();
        let old_paths = a
            .files
            .iter()
            .chain(&b.files)
            .map(|file| PathBuf::from(&file.destination))
            .collect::<Vec<_>>();
        conn.execute_batch(
            "CREATE TRIGGER reject_priority BEFORE UPDATE OF load_priority ON mods
             BEGIN SELECT RAISE(FAIL, 'injected database failure'); END;",
        )
        .unwrap();
        let journal = directory.path().join("journal.json");

        assert!(apply(&mut conn, &[a.id.clone(), b.id.clone()], &journal).is_err());
        assert!(old_paths.iter().all(|path| path.exists()));
        assert!(!journal.exists());
        let priorities = database::list_mods(&conn)
            .unwrap()
            .into_iter()
            .map(|item| (item.id, item.load_priority))
            .collect::<HashMap<_, _>>();
        assert_eq!(priorities[&a.id], Some(1));
        assert_eq!(priorities[&b.id], Some(2));
    }

    #[test]
    fn checksum_mismatch_blocks_reorder_before_a_journal_is_written() {
        let directory = tempdir().unwrap();
        let game_root = directory.path().join("game");
        let library = directory.path().join("library");
        let source_a = directory.path().join("source-a");
        let source_b = directory.path().join("source-b");
        for path in [&library, &source_a, &source_b] {
            fs::create_dir_all(path).unwrap();
        }
        game(&game_root);
        let mut conn = database::open(&directory.path().join("db")).unwrap();
        let a = deployment::install(
            &mut conn,
            &library,
            &game_root,
            &staged(&source_a, "Alpha", &[]),
            None,
        )
        .unwrap();
        let b = deployment::install(
            &mut conn,
            &library,
            &game_root,
            &staged(&source_b, "Bravo", &[]),
            None,
        )
        .unwrap();
        fs::write(&a.files[0].destination, b"changed").unwrap();
        let journal = directory.path().join("journal.json");
        assert!(matches!(
            apply(&mut conn, &[a.id, b.id], &journal),
            Err(AppError::ChecksumMismatch(_))
        ));
        assert!(!journal.exists());
        assert!(Path::new(&b.files[0].destination).exists());
    }

    #[test]
    fn startup_recovery_restores_or_finishes_a_recorded_move() {
        let directory = tempdir().unwrap();
        let game_root = directory.path().join("game");
        let library = directory.path().join("library");
        let source = directory.path().join("source");
        fs::create_dir_all(&library).unwrap();
        fs::create_dir_all(&source).unwrap();
        game(&game_root);
        let mut conn = database::open(&directory.path().join("db")).unwrap();
        let installed = deployment::install(
            &mut conn,
            &library,
            &game_root,
            &staged(&source, "Recover", &[]),
            None,
        )
        .unwrap();
        let old = PathBuf::from(&installed.files[0].destination);
        let temporary = old.with_file_name(".zcom-order-recovery");
        let new = old.with_file_name("Recover_0002_P.pak");
        let expected = installed.files[0].sha256.clone();
        let move_record = JournalMove {
            mod_id: installed.id.clone(),
            library_relative: "Recover_P.pak".into(),
            old: old.clone(),
            temporary: temporary.clone(),
            new: new.clone(),
            expected,
        };
        let journal_path = directory.path().join("journal.json");

        fs::rename(&old, &temporary).unwrap();
        fs::write(
            &journal_path,
            serde_json::to_vec(&Journal {
                moves: vec![move_record.clone()],
            })
            .unwrap(),
        )
        .unwrap();
        recover(&conn, &journal_path).unwrap();
        assert!(old.exists());

        fs::rename(&old, &temporary).unwrap();
        let tx = conn.transaction().unwrap();
        database::update_file_destination(
            &tx,
            &installed.id,
            "Recover_P.pak",
            &new.display().to_string(),
        )
        .unwrap();
        tx.commit().unwrap();
        fs::write(
            &journal_path,
            serde_json::to_vec(&Journal {
                moves: vec![move_record],
            })
            .unwrap(),
        )
        .unwrap();
        recover(&conn, &journal_path).unwrap();
        assert!(new.exists());
        assert!(!temporary.exists());
    }

    #[test]
    #[ignore = "requires a local, redistributable IoStore test triplet"]
    fn renamed_iostore_fixture_remains_verifiable() {
        use std::process::Command;
        let source = PathBuf::from(
            std::env::var("ZCOM_LOAD_ORDER_FIXTURE")
                .expect("set ZCOM_LOAD_ORDER_FIXTURE to a .utoc"),
        );
        let directory = tempdir().unwrap();
        let source_stem = source.file_stem().unwrap().to_string_lossy();
        let target_stem = managed_filename(&format!("{source_stem}.utoc"), 42).unwrap();
        let target_stem = Path::new(&target_stem)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        for extension in ["pak", "utoc", "ucas"] {
            let from = source.with_file_name(format!("{source_stem}.{extension}"));
            if from.exists() {
                fs::copy(
                    from,
                    directory.path().join(format!("{target_stem}.{extension}")),
                )
                .unwrap();
            }
        }
        let renamed = directory.path().join(format!("{target_stem}.utoc"));
        let status = Command::new(std::env::var("RETOC_SOURCE").unwrap_or_else(|_| "retoc".into()))
            .arg("verify")
            .arg(renamed)
            .status()
            .unwrap();
        assert!(status.success());
    }
}
