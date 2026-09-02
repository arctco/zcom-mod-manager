use crate::{
    database,
    error::{AppError, Result},
    load_order,
    models::{ModFile, ModSummary, StagedMod},
    ue4ss,
};
use chrono::Utc;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub fn sha256(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(hex::encode(hash.finalize()))
}

/// Where a mod type is deployed, relative to the game installation.
///
/// A game-folder mod carries its own path inside the installation, because it
/// replaces or adds files the engine reads directly, so its base is the game
/// root itself.
fn destination_base(game: &Path, kind: &str) -> PathBuf {
    match kind {
        "ue4ss" => game.join("SWZeroCompany/Binaries/Win64/ue4ss/Mods"),
        "gamedir" => game.to_path_buf(),
        _ => game.join("SWZeroCompany/Content/Paks/~mods"),
    }
}

/// Removes the directories a mod's own payload leaves standing.
///
/// A UE4SS mod deploys into a tree of its own — `ue4ss/Mods/<Name>/Scripts` —
/// and removing the files left that tree in place, so the runtime and the user
/// both still saw a mod that was no longer installed. Only empty directories
/// strictly below the deployment base are removed, so a directory holding
/// anything else survives and the base itself is never touched.
///
/// A game-folder mod is deliberately left alone: its base is the game
/// installation, and the directories under it belong to the game rather than to
/// any mod.
fn prune_empty_dirs(game: &Path, kind: &str, removed: &[PathBuf]) {
    if kind == "gamedir" {
        return;
    }
    let base = destination_base(game, kind);
    for path in removed {
        let mut current = path.parent().map(Path::to_path_buf);
        while let Some(directory) = current {
            if directory == base || !directory.starts_with(&base) {
                break;
            }
            let empty = fs::read_dir(&directory).is_ok_and(|mut entries| entries.next().is_none());
            if !empty || fs::remove_dir(&directory).is_err() {
                break;
            }
            current = directory.parent().map(Path::to_path_buf);
        }
    }
}

fn copy_atomic(source: &Path, destination: &Path) -> Result<()> {
    if destination.exists() {
        return Err(AppError::DeploymentConflict(destination.to_path_buf()));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| AppError::Other("invalid deployment path".into()))?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".zcom-stage-{}", Uuid::new_v4()));
    fs::copy(source, &temp)?;
    if let Err(e) = fs::rename(&temp, destination) {
        let _ = fs::remove_file(temp);
        return Err(e.into());
    }
    Ok(())
}

/// Moves a file, falling back to copy-and-remove across filesystems.
///
/// The managed library lives in the application data directory while the game
/// commonly sits on another drive entirely - a second SSD on Windows, or a
/// separate Steam library on Linux. `rename` cannot cross that boundary and
/// fails without moving anything, so an upgrade of a mod installed on another
/// drive would abort before it started.
fn move_file(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    fs::copy(from, to)?;
    fs::remove_file(from)?;
    Ok(())
}

fn backup_root(library: &Path, id: &str) -> PathBuf {
    library.join(id).join("replaced")
}

/// Moves a file the mod is about to replace into the managed library.
///
/// A game-folder mod overwrites files the game shipped — a movie, a shader, a
/// configuration file — so the original is kept and restored when the mod is
/// disabled or removed. Nothing else in the manager overwrites anything.
fn take_backup(
    library: &Path,
    id: &str,
    relative: &Path,
    destination: &Path,
) -> Result<(String, String)> {
    let hash = sha256(destination)?;
    let target = backup_root(library, id).join(relative);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(destination, &target)?;
    fs::remove_file(destination)?;
    Ok((relative.display().to_string(), hash))
}

fn restore_backups(conn: &Connection, library: &Path, id: &str) -> Result<()> {
    for (destination, relative, _) in database::backups(conn, id)? {
        let source = backup_root(library, id).join(&relative);
        let destination = PathBuf::from(destination);
        if !source.is_file() || destination.exists() {
            continue;
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&source, &destination)?;
    }
    Ok(())
}

pub fn install(
    conn: &mut Connection,
    library: &Path,
    game: &Path,
    staged: &StagedMod,
    build: Option<String>,
) -> Result<ModSummary> {
    install_over(conn, library, game, staged, build, None)
}

/// Installs a payload, optionally taking over the files of the mod it replaces.
/// `replacing` is excluded from every ownership check: an upgrade lands on the
/// same names by definition, and `replace` has already moved those files aside.
fn install_over(
    conn: &mut Connection,
    library: &Path,
    game: &Path,
    staged: &StagedMod,
    build: Option<String>,
    replacing: Option<&str>,
) -> Result<ModSummary> {
    let id = Uuid::new_v4().to_string();
    let load_priority = match staged.mod_type.as_str() {
        "pak" | "iostore" => Some(database::next_load_priority(conn)?),
        "ue4ss" => Some(database::next_ue4ss_priority(conn)?),
        _ => None,
    };
    let orderable = staged.mod_type == "iostore"
        && staged.files.iter().any(|file| {
            file.destination_relative
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("pak"))
        });
    if matches!(staged.mod_type.as_str(), "pak" | "iostore") {
        let base = destination_base(game, &staged.mod_type);
        for file in &staged.files {
            let logical_name = file.library_relative.display().to_string();
            if database::packaged_source_name_owner(conn, &logical_name, replacing)?.is_some() {
                return Err(AppError::DeploymentConflict(
                    base.join(&file.destination_relative),
                ));
            }
        }
    }
    let mod_library = library.join(&id);
    let payload_root = mod_library.join("payload");
    fs::create_dir_all(&payload_root)?;
    for file in &staged.files {
        let target = payload_root.join(&file.library_relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?
        }
        fs::copy(&file.source, &target)?;
    }
    let base = destination_base(game, &staged.mod_type);
    fs::create_dir_all(&base)?;
    let mut deployed = Vec::new();
    let mut rows = Vec::new();
    let mut backups: Vec<(String, String, String)> = Vec::new();
    let deploy_result = (|| -> Result<()> {
        for file in &staged.files {
            let source = payload_root.join(&file.library_relative);
            let destination_relative = if orderable {
                let filename = file
                    .destination_relative
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                PathBuf::from(load_order::managed_filename(
                    &filename,
                    load_priority.expect("packaged mods have a priority"),
                )?)
            } else {
                file.destination_relative.clone()
            };
            let destination = base.join(destination_relative);
            if staged.mod_type == "gamedir" && destination.exists() {
                if database::destination_owner(conn, &destination.display().to_string(), replacing)?
                    .is_some()
                {
                    return Err(AppError::DeploymentConflict(destination));
                }
                let (relative, hash) =
                    take_backup(library, &id, &file.library_relative, &destination)?;
                backups.push((destination.display().to_string(), relative, hash));
            }
            copy_atomic(&source, &destination)?;
            let size = fs::metadata(&destination)?.len();
            let hash = sha256(&destination)?;
            rows.push((
                file.library_relative.display().to_string(),
                destination.display().to_string(),
                size,
                hash,
            ));
            deployed.push(destination);
        }
        Ok(())
    })();
    let unwind = |deployed: &[PathBuf], backups: &[(String, String, String)]| {
        for path in deployed {
            let _ = fs::remove_file(path);
        }
        for (destination, relative, _) in backups {
            let source = backup_root(library, &id).join(relative);
            let destination = PathBuf::from(destination);
            if source.is_file() && !destination.exists() {
                let _ = fs::copy(&source, &destination);
            }
        }
        let _ = fs::remove_dir_all(library.join(&id));
    };
    if let Err(error) = deploy_result {
        unwind(&deployed, &backups);
        return Err(error);
    }
    let summary = ModSummary {
        id: id.clone(),
        name: staged.name.clone(),
        version: staged.version.clone(),
        mod_type: staged.mod_type.clone(),
        enabled: true,
        installed_at: Utc::now().to_rfc3339(),
        installed_build: build,
        package_count: staged.packages.len(),
        conflict_count: 0,
        potential_conflict_count: 0,
        load_priority,
        // Attached after installation, when the archive is known to Nexus.
        nexus_mod_id: None,
        nexus_url: None,
        nexus_ignored: false,
        hidden: false,
        files: rows
            .iter()
            .map(|(_, d, s, h)| ModFile {
                name: Path::new(d)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                destination: d.clone(),
                size: *s,
                sha256: h.clone(),
            })
            .collect(),
    };
    let tx = conn.transaction()?;
    if let Err(error) = database::insert_mod(
        &tx,
        &summary,
        &staged.deployment_keys.join("\n"),
        Some(&staged.source_archive),
        &rows,
        &staged.packages,
    )
    .and_then(|_| {
        for (destination, relative, hash) in &backups {
            database::record_backup(&tx, &id, destination, relative, hash)?;
        }
        tx.commit()?;
        Ok(())
    }) {
        unwind(&deployed, &backups);
        return Err(error);
    }
    if staged.mod_type == "ue4ss" {
        for key in &staged.deployment_keys {
            if let Err(error) = ue4ss::update_mods_txt(game, key, true) {
                let _ = uninstall(conn, library, &id, true, Some(game));
                return Err(error);
            }
        }
    }
    Ok(summary)
}

/// Installs a payload over the mod it supersedes.
///
/// The upgrade is reversible up to the moment it succeeds: the previous
/// version's deployed files are moved aside rather than deleted, so a failure
/// anywhere in the new installation puts them back and leaves the old mod
/// installed and recorded exactly as it was. Only once the new payload is fully
/// deployed is the old library entry dropped.
///
/// Originals a game-folder mod displaced are carried over to the replacement,
/// so removing the new version still restores what the game shipped.
pub fn replace(
    conn: &mut Connection,
    library: &Path,
    game: &Path,
    old_id: &str,
    staged: &StagedMod,
    build: Option<String>,
    force: bool,
) -> Result<ModSummary> {
    let old = database::mod_record(conn, old_id)?;
    let old_files = database::file_records(conn, old_id)?;
    let old_backups = database::backups(conn, old_id)?;
    if old.enabled && !force {
        for (_, destination, _, expected) in &old_files {
            let path = PathBuf::from(destination);
            if path.exists() && sha256(&path)? != *expected {
                return Err(AppError::ChecksumMismatch(path));
            }
        }
    }
    let aside = library.join(format!(".replacing-{old_id}"));
    let _ = fs::remove_dir_all(&aside);
    fs::create_dir_all(&aside)?;
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    let restore = |moved: &[(PathBuf, PathBuf)]| {
        for (original, held) in moved.iter().rev() {
            if held.is_file() && !original.exists() {
                let _ = move_file(held, original);
            }
        }
    };
    for (index, (_, destination, _, _)) in old_files.iter().enumerate() {
        let original = PathBuf::from(destination);
        if !original.exists() {
            continue;
        }
        let held = aside.join(index.to_string());
        if let Err(error) = move_file(&original, &held) {
            restore(&moved);
            let _ = fs::remove_dir_all(&aside);
            return Err(error);
        }
        moved.push((original, held));
    }
    let installed = install_over(conn, library, game, staged, build, Some(old_id));
    let summary = match installed {
        Ok(summary) => summary,
        Err(error) => {
            restore(&moved);
            let _ = fs::remove_dir_all(&aside);
            return Err(error);
        }
    };
    // From here the replacement owns the destinations, so the old entry goes.
    let carried = (|| -> Result<()> {
        let tx = conn.transaction()?;
        for (destination, relative, hash) in &old_backups {
            let source = backup_root(library, old_id).join(relative);
            let target = backup_root(library, &summary.id).join(relative);
            if source.is_file() {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&source, &target)?;
                database::record_backup(&tx, &summary.id, destination, relative, hash)?;
            }
        }
        database::remove_mod(&tx, old_id)?;
        tx.commit()?;
        Ok(())
    })();
    if let Err(error) = carried {
        // The new payload is deployed and recorded; only the bookkeeping for
        // the old entry failed, so report it rather than tearing anything down.
        let _ = fs::remove_dir_all(&aside);
        return Err(error);
    }
    // The replacement takes the slot its predecessor held, so an upgrade never
    // silently changes what loads first.
    if let Some(priority) = old.load_priority {
        let tx = conn.transaction()?;
        database::set_load_priority(&tx, &summary.id, priority)?;
        tx.commit()?;
    }
    let _ = fs::remove_dir_all(library.join(old_id));
    let _ = fs::remove_dir_all(&aside);
    // A runtime folder the old version registered and the new one does not ship
    // would otherwise linger in mods.txt pointing at nothing.
    for key in old
        .keys
        .iter()
        .filter(|key| !staged.deployment_keys.iter().any(|kept| kept == *key))
    {
        let _ = ue4ss::update_mods_txt(game, key, false);
    }
    Ok(summary)
}

pub fn set_enabled(
    conn: &Connection,
    library: &Path,
    game: &Path,
    id: &str,
    enabled: bool,
) -> Result<()> {
    let record = database::mod_record(conn, id)?;
    if record.enabled == enabled {
        return Ok(());
    }
    let records = database::file_records(conn, id)?;
    if enabled {
        let mut deployed = Vec::new();
        for (lib, destination, _, _) in &records {
            let source = library.join(id).join("payload").join(lib);
            let target = PathBuf::from(destination);
            // An externally installed UE4SS mod may be disabled only through
            // mods.txt while its payload remains deployed. Adoption records
            // that disabled state without touching the game folder; enabling
            // it later can safely reuse an identical live copy.
            if record.mod_type != "gamedir"
                && target.is_file()
                && sha256(&target)? == sha256(&source)?
            {
                continue;
            }
            // A game-folder mod replaces what the game shipped. Whatever sits
            // at the destination now is either the original this mod put back
            // when it was disabled, or a newer file a game update wrote. The
            // first is already in the library and is simply cleared; the second
            // replaces the stored original, so a later removal restores what
            // the game actually has rather than a stale copy.
            if record.mod_type == "gamedir" && target.exists() {
                let recorded = database::backup_for(conn, id, destination)?;
                let current = sha256(&target)?;
                match recorded {
                    Some((_, stored)) if stored == current => fs::remove_file(&target)?,
                    _ => {
                        let (relative, hash) = take_backup(library, id, Path::new(lib), &target)?;
                        database::record_backup(conn, id, destination, &relative, &hash)?;
                    }
                }
            }
            if let Err(error) = copy_atomic(&source, &target) {
                for path in deployed {
                    let _ = fs::remove_file(path);
                }
                let _ = restore_backups(conn, library, id);
                return Err(error);
            }
            deployed.push(target);
        }
        for key in &record.keys {
            ue4ss::update_mods_txt(game, key, true)?
        }
    } else {
        for (_, destination, _, expected) in &records {
            let path = PathBuf::from(destination);
            if path.exists() && sha256(&path)? != *expected {
                return Err(AppError::ChecksumMismatch(path));
            }
        }
        let mut removed = Vec::new();
        for (_, destination, _, _) in &records {
            let path = PathBuf::from(destination);
            if path.exists() {
                fs::remove_file(&path)?;
                removed.push(path);
            }
        }
        restore_backups(conn, library, id)?;
        prune_empty_dirs(game, &record.mod_type, &removed);
        for key in &record.keys {
            ue4ss::update_mods_txt(game, key, false)?
        }
    }
    database::set_enabled(conn, id, enabled)
}

pub fn uninstall(
    conn: &Connection,
    library: &Path,
    id: &str,
    force: bool,
    game: Option<&Path>,
) -> Result<()> {
    let record = database::mod_record(conn, id)?;
    let records = database::file_records(conn, id)?;
    // A mod adopted while disabled in mods.txt can still have its payload on
    // disk. Remove an exact managed copy regardless of the enabled flag, but
    // keep anything else (such as a restored original game file) when the mod
    // is disabled.
    let mut removed = Vec::new();
    for (_, destination, _, expected) in &records {
        let path = PathBuf::from(destination);
        if !path.exists() {
            continue;
        }
        let matches = sha256(&path)? == *expected;
        if record.enabled && !matches && !force {
            return Err(AppError::ChecksumMismatch(path));
        }
        if matches || (record.enabled && force) {
            fs::remove_file(&path)?;
            removed.push(path);
        }
    }
    if record.enabled {
        restore_backups(conn, library, id)?;
    }
    if let Some(game) = game {
        for key in &record.keys {
            let _ = ue4ss::update_mods_txt(game, key, false);
        }
        // The payload is gone; the folders it created would otherwise stay.
        prune_empty_dirs(game, &record.mod_type, &removed);
    }
    database::remove_mod(conn, id)?;
    let dir = library.join(id);
    if dir.exists() {
        fs::remove_dir_all(dir)?
    }
    Ok(())
}

pub fn verify(conn: &Connection, id: &str) -> Result<String> {
    let record = database::mod_record(conn, id)?;
    let records = database::file_records(conn, id)?;
    for (_, destination, size, expected) in records {
        let path = PathBuf::from(&destination);
        if record.enabled && !path.exists() {
            return Err(AppError::Other(format!(
                "A deployed file is missing: {}",
                path.display()
            )));
        }
        if path.exists() && (fs::metadata(&path)?.len() != size || sha256(&path)? != expected) {
            return Err(AppError::ChecksumMismatch(path));
        }
    }
    Ok(format!(
        "{}: all present managed files match their recorded SHA-256 checksums.",
        record.name
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{database, models::PayloadFile};
    use tempfile::tempdir;
    fn staged(root: &Path) -> StagedMod {
        let src = root.join("Test_P.pak");
        fs::write(&src, b"original").unwrap();
        StagedMod {
            staging_id: "s".into(),
            staging_root: root.into(),
            source_archive: root.display().to_string(),
            name: "Test".into(),
            version: None,
            author: None,
            description: None,
            mod_type: "pak".into(),
            deployment_keys: Vec::new(),
            files: vec![PayloadFile {
                source: src,
                library_relative: "Test_P.pak".into(),
                destination_relative: "Test_P.pak".into(),
            }],
            packages: vec![],
            verification: "not-required".into(),
            verification_details: None,
        }
    }
    fn game(root: &Path) {
        fs::create_dir_all(root.join("SWZeroCompany/Content/Paks")).unwrap();
        fs::create_dir_all(root.join("SWZeroCompany/Binaries/Win64")).unwrap();
        fs::write(
            root.join("SWZeroCompany/Binaries/Win64/SWZeroCompany.exe"),
            b"",
        )
        .unwrap();
    }
    #[test]
    fn install_disable_enable_uninstall() {
        let d = tempdir().unwrap();
        let g = d.path().join("game");
        let l = d.path().join("library");
        game(&g);
        fs::create_dir_all(&l).unwrap();
        let mut c = database::open(&d.path().join("db")).unwrap();
        let m = install(&mut c, &l, &g, &staged(d.path()), None).unwrap();
        let deployed = g.join("SWZeroCompany/Content/Paks/~mods/Test_P.pak");
        assert!(deployed.exists());
        set_enabled(&c, &l, &g, &m.id, false).unwrap();
        assert!(!deployed.exists());
        set_enabled(&c, &l, &g, &m.id, true).unwrap();
        uninstall(&c, &l, &m.id, false, Some(&g)).unwrap();
        assert!(!deployed.exists());
    }
    /// A UE4SS mod deploys into a folder tree of its own, the way Squad Six's
    /// Runtime component does.
    fn staged_ue4ss(root: &Path) -> StagedMod {
        let src = root.join("main.lua");
        fs::write(&src, b"return {}").unwrap();
        StagedMod {
            staging_id: "u".into(),
            staging_root: root.into(),
            source_archive: root.display().to_string(),
            name: "Squad Six - Runtime".into(),
            version: None,
            author: None,
            description: None,
            mod_type: "ue4ss".into(),
            deployment_keys: vec!["ZCOMSquadSix".into()],
            files: vec![PayloadFile {
                source: src,
                library_relative: "ZCOMSquadSix/Scripts/main.lua".into(),
                destination_relative: "ZCOMSquadSix/Scripts/main.lua".into(),
            }],
            packages: vec![],
            verification: "not-required".into(),
            verification_details: None,
        }
    }

    #[test]
    fn removing_a_ue4ss_mod_takes_its_folders_with_it() {
        let d = tempdir().unwrap();
        let g = d.path().join("game");
        let l = d.path().join("library");
        game(&g);
        fs::create_dir_all(&l).unwrap();
        let mut c = database::open(&d.path().join("db")).unwrap();
        let m = install(&mut c, &l, &g, &staged_ue4ss(d.path()), None).unwrap();
        let mods_root = g.join("SWZeroCompany/Binaries/Win64/ue4ss/Mods");
        let folder = mods_root.join("ZCOMSquadSix");
        assert!(folder.join("Scripts/main.lua").exists());

        // Disabling clears the tree as well, so a disabled mod does not look
        // installed to the runtime.
        set_enabled(&c, &l, &g, &m.id, false).unwrap();
        assert!(!folder.exists(), "the mod folder outlived its payload");
        set_enabled(&c, &l, &g, &m.id, true).unwrap();
        assert!(folder.join("Scripts/main.lua").exists());

        uninstall(&c, &l, &m.id, false, Some(&g)).unwrap();
        assert!(!folder.exists(), "the mod folder outlived the mod");
        // The base every UE4SS mod shares is never removed with one of them.
        assert!(mods_root.exists());
    }

    #[test]
    fn a_folder_holding_anything_else_is_left_alone() {
        let d = tempdir().unwrap();
        let g = d.path().join("game");
        let l = d.path().join("library");
        game(&g);
        fs::create_dir_all(&l).unwrap();
        let mut c = database::open(&d.path().join("db")).unwrap();
        let m = install(&mut c, &l, &g, &staged_ue4ss(d.path()), None).unwrap();
        let folder = g.join("SWZeroCompany/Binaries/Win64/ue4ss/Mods/ZCOMSquadSix");
        // Settings the user wrote, or anything the manager does not own.
        fs::write(folder.join("Scripts/settings.lua"), b"local x = 1").unwrap();

        uninstall(&c, &l, &m.id, false, Some(&g)).unwrap();
        assert!(folder.join("Scripts/settings.lua").exists());
        assert!(!folder.join("Scripts/main.lua").exists());
    }

    #[test]
    fn checksum_mismatch_is_kept() {
        let d = tempdir().unwrap();
        let g = d.path().join("game");
        let l = d.path().join("library");
        game(&g);
        fs::create_dir_all(&l).unwrap();
        let mut c = database::open(&d.path().join("db")).unwrap();
        let m = install(&mut c, &l, &g, &staged(d.path()), None).unwrap();
        let deployed = g.join("SWZeroCompany/Content/Paks/~mods/Test_P.pak");
        fs::write(&deployed, b"changed").unwrap();
        assert!(matches!(
            uninstall(&c, &l, &m.id, false, Some(&g)),
            Err(AppError::ChecksumMismatch(_))
        ));
        assert!(deployed.exists());
    }
    #[test]
    fn filename_collision_is_refused() {
        let d = tempdir().unwrap();
        let g = d.path().join("game");
        let l = d.path().join("library");
        game(&g);
        fs::create_dir_all(g.join("SWZeroCompany/Content/Paks/~mods")).unwrap();
        fs::write(
            g.join("SWZeroCompany/Content/Paks/~mods/Test_P.pak"),
            b"unknown",
        )
        .unwrap();
        fs::create_dir_all(&l).unwrap();
        let mut c = database::open(&d.path().join("db")).unwrap();
        assert!(matches!(
            install(&mut c, &l, &g, &staged(d.path()), None),
            Err(AppError::DeploymentConflict(_))
        ));
    }

    fn gamedir(root: &Path, relative: &str, body: &[u8]) -> StagedMod {
        let src = root.join("payload.bin");
        fs::write(&src, body).unwrap();
        StagedMod {
            staging_id: "s".into(),
            staging_root: root.into(),
            source_archive: root.display().to_string(),
            name: "No Intro".into(),
            version: None,
            author: None,
            description: None,
            mod_type: "gamedir".into(),
            deployment_keys: Vec::new(),
            files: vec![PayloadFile {
                source: src,
                library_relative: PathBuf::from(relative),
                destination_relative: PathBuf::from(relative),
            }],
            packages: vec![],
            verification: "not-required".into(),
            verification_details: None,
        }
    }

    #[test]
    fn a_game_folder_mod_keeps_and_restores_the_file_it_replaced() {
        let d = tempdir().unwrap();
        let g = d.path().join("game");
        let l = d.path().join("library");
        game(&g);
        fs::create_dir_all(&l).unwrap();
        let original = g.join("SWZeroCompany/Content/Movies/Logo.mp4");
        fs::create_dir_all(original.parent().unwrap()).unwrap();
        fs::write(&original, b"shipped").unwrap();
        let mut c = database::open(&d.path().join("db")).unwrap();
        let staged = gamedir(d.path(), "SWZeroCompany/Content/Movies/Logo.mp4", b"silent");

        let installed = install(&mut c, &l, &g, &staged, None).unwrap();
        assert_eq!(fs::read(&original).unwrap(), b"silent");

        set_enabled(&c, &l, &g, &installed.id, false).unwrap();
        assert_eq!(
            fs::read(&original).unwrap(),
            b"shipped",
            "disabling puts back what the game shipped"
        );

        set_enabled(&c, &l, &g, &installed.id, true).unwrap();
        assert_eq!(fs::read(&original).unwrap(), b"silent");

        uninstall(&c, &l, &installed.id, false, Some(&g)).unwrap();
        assert_eq!(
            fs::read(&original).unwrap(),
            b"shipped",
            "removal puts back what the game shipped"
        );
    }

    #[test]
    fn a_game_update_while_disabled_replaces_the_stored_original() {
        let d = tempdir().unwrap();
        let g = d.path().join("game");
        let l = d.path().join("library");
        game(&g);
        fs::create_dir_all(&l).unwrap();
        let original = g.join("SWZeroCompany/Content/Movies/Logo.mp4");
        fs::create_dir_all(original.parent().unwrap()).unwrap();
        fs::write(&original, b"shipped").unwrap();
        let mut c = database::open(&d.path().join("db")).unwrap();
        let staged = gamedir(d.path(), "SWZeroCompany/Content/Movies/Logo.mp4", b"silent");
        let installed = install(&mut c, &l, &g, &staged, None).unwrap();

        set_enabled(&c, &l, &g, &installed.id, false).unwrap();
        fs::write(&original, b"patched").unwrap();
        set_enabled(&c, &l, &g, &installed.id, true).unwrap();
        uninstall(&c, &l, &installed.id, false, Some(&g)).unwrap();
        assert_eq!(
            fs::read(&original).unwrap(),
            b"patched",
            "removal restores what the game has now, not a stale copy"
        );
    }

    #[test]
    fn a_game_folder_mod_never_overwrites_another_mod() {
        let d = tempdir().unwrap();
        let g = d.path().join("game");
        let l = d.path().join("library");
        game(&g);
        fs::create_dir_all(&l).unwrap();
        let mut c = database::open(&d.path().join("db")).unwrap();
        fs::create_dir_all(d.path().join("a")).unwrap();
        fs::create_dir_all(d.path().join("b")).unwrap();
        let first = gamedir(
            &d.path().join("a"),
            "SWZeroCompany/Content/Movies/Logo.mp4",
            b"one",
        );
        install(&mut c, &l, &g, &first, None).unwrap();
        let second = gamedir(
            &d.path().join("b"),
            "SWZeroCompany/Content/Movies/Logo.mp4",
            b"two",
        );
        assert!(matches!(
            install(&mut c, &l, &g, &second, None),
            Err(AppError::DeploymentConflict(_))
        ));
        assert_eq!(
            fs::read(g.join("SWZeroCompany/Content/Movies/Logo.mp4")).unwrap(),
            b"one"
        );
    }

    #[test]
    fn every_ue4ss_folder_in_one_archive_is_registered() {
        let d = tempdir().unwrap();
        let g = d.path().join("game");
        let l = d.path().join("library");
        game(&g);
        fs::create_dir_all(g.join("SWZeroCompany/Binaries/Win64/ue4ss/Mods")).unwrap();
        fs::create_dir_all(&l).unwrap();
        let lua = d.path().join("main.lua");
        fs::write(&lua, b"return {}").unwrap();
        let staged = StagedMod {
            staging_id: "s".into(),
            staging_root: d.path().into(),
            source_archive: d.path().display().to_string(),
            name: "TrueLight Shadows".into(),
            version: None,
            author: None,
            description: None,
            mod_type: "ue4ss".into(),
            deployment_keys: vec!["ShadowsCore".into(), "ShadowsTweaks".into()],
            files: ["ShadowsCore", "ShadowsTweaks"]
                .into_iter()
                .map(|folder| PayloadFile {
                    source: lua.clone(),
                    library_relative: PathBuf::from(folder).join("Scripts/main.lua"),
                    destination_relative: PathBuf::from(folder).join("Scripts/main.lua"),
                })
                .collect(),
            packages: vec![],
            verification: "not-required".into(),
            verification_details: None,
        };
        let mut c = database::open(&d.path().join("db")).unwrap();
        let installed = install(&mut c, &l, &g, &staged, None).unwrap();
        let mods_txt = g.join("SWZeroCompany/Binaries/Win64/ue4ss/Mods/mods.txt");
        let text = fs::read_to_string(&mods_txt).unwrap();
        assert!(text.contains("ShadowsCore : 1"), "{text}");
        assert!(text.contains("ShadowsTweaks : 1"), "{text}");
        assert!(g
            .join("SWZeroCompany/Binaries/Win64/ue4ss/Mods/ShadowsTweaks/Scripts/main.lua")
            .is_file());

        set_enabled(&c, &l, &g, &installed.id, false).unwrap();
        let text = fs::read_to_string(&mods_txt).unwrap();
        assert!(text.contains("ShadowsCore : 0"), "{text}");
        assert!(text.contains("ShadowsTweaks : 0"), "{text}");
    }

    fn lua_mod(root: &Path, folder: &str, body: &[u8]) -> StagedMod {
        let src = root.join(format!("{folder}.lua"));
        fs::write(&src, body).unwrap();
        let relative = PathBuf::from(folder).join("Scripts/main.lua");
        StagedMod {
            staging_id: "s".into(),
            staging_root: root.into(),
            source_archive: root.display().to_string(),
            name: folder.into(),
            version: None,
            author: None,
            description: None,
            mod_type: "ue4ss".into(),
            deployment_keys: vec![folder.into()],
            files: vec![PayloadFile {
                source: src,
                library_relative: relative.clone(),
                destination_relative: relative,
            }],
            packages: vec![],
            verification: "not-required".into(),
            verification_details: None,
        }
    }

    fn ue4ss_game(root: &Path) {
        game(root);
        fs::create_dir_all(root.join("SWZeroCompany/Binaries/Win64/ue4ss/Mods")).unwrap();
    }

    #[test]
    fn moving_a_file_creates_its_destination_and_clears_the_source() {
        let d = tempdir().unwrap();
        let from = d.path().join("original.pak");
        fs::write(&from, b"payload").unwrap();
        let to = d.path().join("held/deep/0");

        move_file(&from, &to).unwrap();

        assert_eq!(fs::read(&to).unwrap(), b"payload");
        assert!(!from.exists(), "the source is gone either way it moved");
    }

    #[test]
    fn an_upgrade_takes_the_place_of_the_version_it_replaces() {
        let d = tempdir().unwrap();
        let g = d.path().join("game");
        let l = d.path().join("library");
        ue4ss_game(&g);
        fs::create_dir_all(&l).unwrap();
        let mut c = database::open(&d.path().join("db")).unwrap();
        let first = install(&mut c, &l, &g, &lua_mod(d.path(), "Talents", b"v1"), None).unwrap();
        install(&mut c, &l, &g, &lua_mod(d.path(), "Other", b"other"), None).unwrap();
        let slot = database::mod_record(&c, &first.id).unwrap().load_priority;

        fs::create_dir_all(d.path().join("v2")).unwrap();
        let newer = lua_mod(&d.path().join("v2"), "Talents", b"v2");
        let upgraded = replace(&mut c, &l, &g, &first.id, &newer, None, false).unwrap();

        let deployed = g.join("SWZeroCompany/Binaries/Win64/ue4ss/Mods/Talents/Scripts/main.lua");
        assert_eq!(fs::read(&deployed).unwrap(), b"v2");
        assert!(
            database::mod_record(&c, &first.id).is_err(),
            "the old entry is gone"
        );
        assert_eq!(
            database::mod_record(&c, &upgraded.id)
                .unwrap()
                .load_priority,
            slot,
            "the replacement keeps the slot its predecessor held"
        );
        assert!(
            !l.join(&first.id).exists(),
            "the old library copy is removed"
        );
        assert!(!l.join(format!(".replacing-{}", first.id)).exists());
    }

    #[test]
    fn a_failed_upgrade_leaves_the_previous_version_installed() {
        let d = tempdir().unwrap();
        let g = d.path().join("game");
        let l = d.path().join("library");
        ue4ss_game(&g);
        fs::create_dir_all(&l).unwrap();
        let mut c = database::open(&d.path().join("db")).unwrap();
        let first = install(&mut c, &l, &g, &lua_mod(d.path(), "Talents", b"v1"), None).unwrap();
        let deployed = g.join("SWZeroCompany/Binaries/Win64/ue4ss/Mods/Talents/Scripts/main.lua");

        // A payload whose source has gone missing fails once the replacement is
        // already under way, which is exactly when a rollback has to work.
        let mut broken = lua_mod(d.path(), "Talents", b"v2");
        broken.files[0].source = d.path().join("does-not-exist.lua");
        assert!(replace(&mut c, &l, &g, &first.id, &broken, None, false).is_err());

        assert_eq!(
            fs::read(&deployed).unwrap(),
            b"v1",
            "the previous version is put back"
        );
        assert!(
            database::mod_record(&c, &first.id).is_ok(),
            "and stays installed"
        );
        assert!(!l.join(format!(".replacing-{}", first.id)).exists());
        verify(&c, &first.id).unwrap();
    }

    #[test]
    fn an_upgrade_carries_over_the_original_it_displaced() {
        let d = tempdir().unwrap();
        let g = d.path().join("game");
        let l = d.path().join("library");
        game(&g);
        fs::create_dir_all(&l).unwrap();
        let original = g.join("SWZeroCompany/Content/Movies/Logo.mp4");
        fs::create_dir_all(original.parent().unwrap()).unwrap();
        fs::write(&original, b"shipped").unwrap();
        let mut c = database::open(&d.path().join("db")).unwrap();
        let first = install(
            &mut c,
            &l,
            &g,
            &gamedir(d.path(), "SWZeroCompany/Content/Movies/Logo.mp4", b"silent"),
            None,
        )
        .unwrap();

        fs::create_dir_all(d.path().join("v2")).unwrap();
        let newer = gamedir(
            &d.path().join("v2"),
            "SWZeroCompany/Content/Movies/Logo.mp4",
            b"quieter",
        );
        let upgraded = replace(&mut c, &l, &g, &first.id, &newer, None, false).unwrap();
        assert_eq!(fs::read(&original).unwrap(), b"quieter");

        uninstall(&c, &l, &upgraded.id, false, Some(&g)).unwrap();
        assert_eq!(
            fs::read(&original).unwrap(),
            b"shipped",
            "the file the first version displaced is still the one restored"
        );
    }

    #[test]
    fn ue4ss_start_order_is_written_to_mods_txt() {
        let d = tempdir().unwrap();
        let g = d.path().join("game");
        let l = d.path().join("library");
        ue4ss_game(&g);
        fs::create_dir_all(&l).unwrap();
        let mods_txt = g.join("SWZeroCompany/Binaries/Win64/ue4ss/Mods/mods.txt");
        fs::write(&mods_txt, "Keybinds : 1\n").unwrap();
        let mut c = database::open(&d.path().join("db")).unwrap();
        let alpha = install(&mut c, &l, &g, &lua_mod(d.path(), "Alpha", b"a"), None).unwrap();
        let bravo = install(&mut c, &l, &g, &lua_mod(d.path(), "Bravo", b"b"), None).unwrap();
        assert_eq!(
            fs::read_to_string(&mods_txt).unwrap(),
            "Alpha : 1\nBravo : 1\nKeybinds : 1\n",
            "a new mod starts last"
        );

        let state =
            crate::load_order::apply_ue4ss_order(&mut c, &g, &[bravo.id.clone(), alpha.id.clone()])
                .unwrap();
        assert_eq!(
            fs::read_to_string(&mods_txt).unwrap(),
            "Bravo : 1\nAlpha : 1\nKeybinds : 1\n"
        );
        assert_eq!(
            state
                .ue4ss_entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Bravo", "Alpha"],
            "the recorded order matches the file"
        );
    }

    #[test]
    fn mods_installed_before_ordering_keep_the_order_mods_txt_already_has() {
        let d = tempdir().unwrap();
        let g = d.path().join("game");
        let l = d.path().join("library");
        ue4ss_game(&g);
        fs::create_dir_all(&l).unwrap();
        let mut c = database::open(&d.path().join("db")).unwrap();
        install(&mut c, &l, &g, &lua_mod(d.path(), "First", b"1"), None).unwrap();
        install(&mut c, &l, &g, &lua_mod(d.path(), "Alpha", b"2"), None).unwrap();
        // An install from before this release recorded no slot at all.
        c.execute("UPDATE mods SET load_priority=NULL", []).unwrap();

        let names: Vec<String> = crate::load_order::state(&c)
            .unwrap()
            .ue4ss_entries
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        assert_eq!(
            names,
            vec!["First".to_string(), "Alpha".to_string()],
            "installation order, not alphabetical: it is the order mods.txt already has"
        );
    }

    fn dll_mod(root: &Path, folder: &str) -> StagedMod {
        let mut staged = lua_mod(root, folder, b"native");
        staged.files[0].library_relative = PathBuf::from(folder).join("dlls/main.dll");
        staged.files[0].destination_relative = staged.files[0].library_relative.clone();
        staged
    }

    #[test]
    fn dll_mods_are_ordered_ahead_of_lua_mods() {
        let d = tempdir().unwrap();
        let g = d.path().join("game");
        let l = d.path().join("library");
        ue4ss_game(&g);
        fs::create_dir_all(&l).unwrap();
        let mods_txt = g.join("SWZeroCompany/Binaries/Win64/ue4ss/Mods/mods.txt");
        let mut c = database::open(&d.path().join("db")).unwrap();
        let script = install(&mut c, &l, &g, &lua_mod(d.path(), "Script", b"lua"), None).unwrap();
        let native = install(&mut c, &l, &g, &dll_mod(d.path(), "Native"), None).unwrap();

        let state = crate::load_order::state(&c).unwrap();
        assert_eq!(
            state
                .ue4ss_entries
                .iter()
                .map(|entry| (entry.name.as_str(), entry.runtime_kind.as_deref()))
                .collect::<Vec<_>>(),
            vec![("Native", Some("native")), ("Script", Some("script"))],
            "UE4SS starts every DLL mod before any Lua mod, whatever mods.txt says"
        );

        // Asking for the impossible interleaving records the achievable order.
        crate::load_order::apply_ue4ss_order(&mut c, &g, &[script.id.clone(), native.id.clone()])
            .unwrap();
        assert_eq!(
            fs::read_to_string(&mods_txt).unwrap(),
            "Native : 1\nScript : 1\n"
        );
    }

    #[test]
    fn ue4ss_order_must_list_every_mod_once() {
        let d = tempdir().unwrap();
        let g = d.path().join("game");
        let l = d.path().join("library");
        ue4ss_game(&g);
        fs::create_dir_all(&l).unwrap();
        let mut c = database::open(&d.path().join("db")).unwrap();
        let alpha = install(&mut c, &l, &g, &lua_mod(d.path(), "Alpha", b"a"), None).unwrap();
        install(&mut c, &l, &g, &lua_mod(d.path(), "Bravo", b"b"), None).unwrap();
        assert!(matches!(
            crate::load_order::apply_ue4ss_order(&mut c, &g, std::slice::from_ref(&alpha.id)),
            Err(AppError::InvalidLoadOrder(_))
        ));
        assert!(matches!(
            crate::load_order::apply_ue4ss_order(&mut c, &g, &[alpha.id.clone(), alpha.id]),
            Err(AppError::InvalidLoadOrder(_))
        ));
    }

    #[test]
    fn logical_source_filename_collision_is_refused_across_priorities() {
        let d = tempdir().unwrap();
        let g = d.path().join("game");
        let l = d.path().join("library");
        game(&g);
        fs::create_dir_all(&l).unwrap();
        let mut c = database::open(&d.path().join("db")).unwrap();
        install(&mut c, &l, &g, &staged(d.path()), None).unwrap();
        assert!(matches!(
            install(&mut c, &l, &g, &staged(d.path()), None),
            Err(AppError::DeploymentConflict(_))
        ));
    }
}
