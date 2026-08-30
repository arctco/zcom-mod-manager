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
fn destination_base(game: &Path, kind: &str) -> PathBuf {
    if kind == "ue4ss" {
        game.join("SWZeroCompany/Binaries/Win64/ue4ss/Mods")
    } else {
        game.join("SWZeroCompany/Content/Paks/~mods")
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

pub fn install(
    conn: &mut Connection,
    library: &Path,
    game: &Path,
    staged: &StagedMod,
    build: Option<String>,
) -> Result<ModSummary> {
    let id = Uuid::new_v4().to_string();
    let load_priority = matches!(staged.mod_type.as_str(), "pak" | "iostore")
        .then(|| database::next_load_priority(conn))
        .transpose()?;
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
            if database::packaged_source_name_exists(conn, &logical_name)? {
                return Err(AppError::DeploymentConflict(
                    base.join(&file.destination_relative),
                ));
            }
        }
    }
    let mod_library = library.join(&id);
    let payload_root = mod_library.join("payload");
    fs::create_dir_all(&payload_root)?;
    let mut library_copied = Vec::new();
    for file in &staged.files {
        let target = payload_root.join(&file.library_relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?
        }
        fs::copy(&file.source, &target)?;
        library_copied.push(target);
    }
    let base = destination_base(game, &staged.mod_type);
    fs::create_dir_all(&base)?;
    let mut deployed = Vec::new();
    let mut rows = Vec::new();
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
    if let Err(error) = deploy_result {
        for path in deployed {
            let _ = fs::remove_file(path);
        }
        let _ = fs::remove_dir_all(&mod_library);
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
        &staged.deployment_key,
        Some(&staged.source_archive),
        &rows,
        &staged.packages,
    )
    .and_then(|_| {
        tx.commit()?;
        Ok(())
    }) {
        for path in deployed {
            let _ = fs::remove_file(path);
        }
        let _ = fs::remove_dir_all(&mod_library);
        return Err(error);
    }
    if staged.mod_type == "ue4ss" {
        if let Err(error) = ue4ss::update_mods_txt(game, &staged.deployment_key, true) {
            let _ = uninstall(conn, library, &id, true, Some(game));
            return Err(error);
        }
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
    let (name, kind, current) = database::mod_kind(conn, id)?;
    if current == enabled {
        return Ok(());
    }
    let records = database::file_records(conn, id)?;
    if enabled {
        let mut deployed = Vec::new();
        for (lib, destination, _, _) in &records {
            let source = library.join(id).join("payload").join(lib);
            let target = PathBuf::from(destination);
            if let Err(error) = copy_atomic(&source, &target) {
                for path in deployed {
                    let _ = fs::remove_file(path);
                }
                return Err(error);
            }
            deployed.push(target);
        }
        if kind == "ue4ss" {
            ue4ss::update_mods_txt(game, &name, true)?
        }
    } else {
        for (_, destination, _, expected) in &records {
            let path = PathBuf::from(destination);
            if path.exists() && sha256(&path)? != *expected {
                return Err(AppError::ChecksumMismatch(path));
            }
        }
        for (_, destination, _, _) in &records {
            let path = PathBuf::from(destination);
            if path.exists() {
                fs::remove_file(path)?
            }
        }
        if kind == "ue4ss" {
            ue4ss::update_mods_txt(game, &name, false)?
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
    let (name, kind, enabled) = database::mod_kind(conn, id)?;
    let records = database::file_records(conn, id)?;
    if enabled {
        for (_, destination, _, expected) in &records {
            let path = PathBuf::from(destination);
            if path.exists() && !force && sha256(&path)? != *expected {
                return Err(AppError::ChecksumMismatch(path));
            }
        }
        for (_, destination, _, _) in &records {
            let path = PathBuf::from(destination);
            if path.exists() {
                fs::remove_file(path)?
            }
        }
    }
    if kind == "ue4ss" {
        if let Some(game) = game {
            let _ = ue4ss::update_mods_txt(game, &name, false);
        }
    }
    database::remove_mod(conn, id)?;
    let dir = library.join(id);
    if dir.exists() {
        fs::remove_dir_all(dir)?
    }
    Ok(())
}

pub fn verify(conn: &Connection, id: &str) -> Result<String> {
    let (name, _, enabled) = database::mod_kind(conn, id)?;
    let records = database::file_records(conn, id)?;
    for (_, destination, size, expected) in records {
        let path = PathBuf::from(&destination);
        if enabled && !path.exists() {
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
        "{name}: all present managed files match their recorded SHA-256 checksums."
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
            deployment_key: String::new(),
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
