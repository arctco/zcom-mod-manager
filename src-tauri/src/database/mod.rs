use crate::{
    error::Result,
    models::{AppSettings, ModFile, ModSummary, PreviewConflict},
};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::{collections::BTreeMap, path::Path};

pub fn open(path: &Path) -> Result<Connection> {
    let mut connection = Connection::open(path)?;
    connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;
      CREATE TABLE IF NOT EXISTS schema_migrations(version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS mods(id TEXT PRIMARY KEY, name TEXT NOT NULL, version TEXT, mod_type TEXT NOT NULL, deployment_key TEXT NOT NULL DEFAULT '', source_archive TEXT, installed_at TEXT NOT NULL, enabled INTEGER NOT NULL, installed_build TEXT);
      CREATE TABLE IF NOT EXISTS mod_files(id INTEGER PRIMARY KEY, mod_id TEXT NOT NULL REFERENCES mods(id) ON DELETE CASCADE, library_relative TEXT NOT NULL, destination TEXT NOT NULL, size INTEGER NOT NULL, sha256 TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS mod_packages(mod_id TEXT NOT NULL REFERENCES mods(id) ON DELETE CASCADE, package_id TEXT NOT NULL, PRIMARY KEY(mod_id, package_id));
      CREATE TABLE IF NOT EXISTS mod_backups(mod_id TEXT NOT NULL REFERENCES mods(id) ON DELETE CASCADE, destination TEXT NOT NULL, backup_relative TEXT NOT NULL, sha256 TEXT NOT NULL, PRIMARY KEY(mod_id, destination));
      INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES(1, datetime('now'));"
    )?;
    migrate_v2(&mut connection)?;
    Ok(connection)
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(names.iter().any(|name| name == column))
}

fn migrate_v2(conn: &mut Connection) -> Result<()> {
    let applied: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=2)",
        [],
        |row| row.get(0),
    )?;
    if applied {
        return Ok(());
    }
    let transaction = conn.transaction()?;
    if !column_exists(&transaction, "mods", "load_priority")? {
        transaction.execute("ALTER TABLE mods ADD COLUMN load_priority INTEGER", [])?;
    }
    let ids = {
        let mut statement = transaction.prepare(
            "SELECT id FROM mods WHERE mod_type IN ('pak','iostore') ORDER BY installed_at ASC,id ASC",
        )?;
        let result = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        result
    };
    for (index, id) in ids.iter().enumerate() {
        transaction.execute(
            "UPDATE mods SET load_priority=?2 WHERE id=?1",
            params![id, (index + 1) as i64],
        )?;
    }
    transaction.execute(
        "INSERT INTO schema_migrations(version,applied_at) VALUES(2,datetime('now'))",
        [],
    )?;
    transaction.commit()?;
    Ok(())
}

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM settings WHERE key=?1", [key], |r| {
            r.get(0)
        })
        .optional()?)
}

pub fn settings(conn: &Connection) -> Result<AppSettings> {
    let bool_value = |key: &str| get_setting(conn, key).map(|v| v.as_deref() == Some("true"));
    Ok(AppSettings {
        game_path: get_setting(conn, "game_path")?,
        retoc_path: get_setting(conn, "retoc_path")?,
        log_level: get_setting(conn, "log_level")?.unwrap_or_else(|| "normal".into()),
        advanced_package_names: bool_value("advanced_package_names")?,
        reduced_motion: bool_value("reduced_motion")?,
    })
}

pub fn save_settings(conn: &Connection, value: &AppSettings) -> Result<()> {
    let values = [
        ("game_path", value.game_path.clone().unwrap_or_default()),
        ("retoc_path", value.retoc_path.clone().unwrap_or_default()),
        ("log_level", value.log_level.clone()),
        (
            "advanced_package_names",
            value.advanced_package_names.to_string(),
        ),
        ("reduced_motion", value.reduced_motion.to_string()),
    ];
    for (key, val) in values {
        conn.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key,val])?;
    }
    Ok(())
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key,value])?;
    Ok(())
}

pub fn delete_setting(conn: &Connection, key: &str) -> Result<()> {
    conn.execute("DELETE FROM settings WHERE key=?1", params![key])?;
    Ok(())
}

pub fn counts(conn: &Connection) -> Result<(usize, usize)> {
    let total: i64 = conn.query_row("SELECT count(*) FROM mods", [], |r| r.get(0))?;
    let enabled: i64 = conn.query_row("SELECT count(*) FROM mods WHERE enabled=1", [], |r| {
        r.get(0)
    })?;
    Ok((total as usize, enabled as usize))
}

pub fn list_mods(conn: &Connection) -> Result<Vec<ModSummary>> {
    let mut stmt = conn.prepare("SELECT id,name,version,mod_type,enabled,installed_at,installed_build,load_priority FROM mods ORDER BY installed_at DESC")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, bool>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, Option<i64>>(7)?,
        ))
    })?;
    let mut result = Vec::new();
    for row in rows {
        let (id, name, version, mod_type, enabled, installed_at, installed_build, load_priority) =
            row?;
        let mut fs = conn.prepare(
            "SELECT destination,size,sha256 FROM mod_files WHERE mod_id=?1 ORDER BY destination",
        )?;
        let files = fs
            .query_map([&id], |r| {
                let d: String = r.get(0)?;
                Ok(ModFile {
                    name: Path::new(&d)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    destination: d,
                    size: r.get::<_, i64>(1)? as u64,
                    sha256: r.get(2)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let package_count: i64 = conn.query_row(
            "SELECT count(*) FROM mod_packages WHERE mod_id=?1",
            [&id],
            |r| r.get(0),
        )?;
        let potential_conflict_count: i64=conn.query_row("SELECT count(DISTINCT other.mod_id) FROM mod_packages mine JOIN mod_packages other ON mine.package_id=other.package_id AND mine.mod_id<>other.mod_id WHERE mine.mod_id=?1",[&id],|r|r.get(0))?;
        let conflict_count: i64 = if enabled {
            conn.query_row("SELECT count(DISTINCT other.mod_id) FROM mod_packages mine JOIN mod_packages other ON mine.package_id=other.package_id AND mine.mod_id<>other.mod_id JOIN mods other_mod ON other_mod.id=other.mod_id WHERE mine.mod_id=?1 AND other_mod.enabled=1",[&id],|r|r.get(0))?
        } else {
            0
        };
        result.push(ModSummary {
            id,
            name,
            version,
            mod_type,
            enabled,
            installed_at,
            installed_build,
            package_count: package_count as usize,
            conflict_count: conflict_count as usize,
            potential_conflict_count: potential_conflict_count as usize,
            load_priority,
            files,
        });
    }
    Ok(result)
}

pub fn insert_mod(
    tx: &Transaction<'_>,
    summary: &ModSummary,
    deployment_key: &str,
    source: Option<&str>,
    file_rows: &[(String, String, u64, String)],
    packages: &[String],
) -> Result<()> {
    tx.execute("INSERT INTO mods(id,name,version,mod_type,deployment_key,source_archive,installed_at,enabled,installed_build,load_priority) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![summary.id,summary.name,summary.version,summary.mod_type,deployment_key,source,summary.installed_at,summary.enabled,summary.installed_build,summary.load_priority])?;
    for (library, destination, size, hash) in file_rows {
        tx.execute("INSERT INTO mod_files(mod_id,library_relative,destination,size,sha256) VALUES(?1,?2,?3,?4,?5)",params![summary.id,library,destination,*size as i64,hash])?;
    }
    for package in packages {
        tx.execute(
            "INSERT INTO mod_packages(mod_id,package_id) VALUES(?1,?2)",
            params![summary.id, package],
        )?;
    }
    Ok(())
}

pub fn set_enabled(conn: &Connection, id: &str, enabled: bool) -> Result<()> {
    conn.execute(
        "UPDATE mods SET enabled=?2 WHERE id=?1",
        params![id, enabled],
    )?;
    Ok(())
}
pub fn remove_mod(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM mods WHERE id=?1", [id])?;
    Ok(())
}
pub fn file_records(conn: &Connection, id: &str) -> Result<Vec<(String, String, u64, String)>> {
    let mut s = conn.prepare(
        "SELECT library_relative,destination,size,sha256 FROM mod_files WHERE mod_id=?1",
    )?;
    let rows = s
        .query_map([id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? as u64, r.get(3)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}
pub fn next_load_priority(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT coalesce(max(load_priority),0)+1 FROM mods WHERE mod_type IN ('pak','iostore')",
        [],
        |row| row.get(0),
    )?)
}

/// UE4SS start order is a separate sequence in the same column: the packaged
/// list is ranked highest-wins, while `mods.txt` is read first-to-last.
pub fn next_ue4ss_priority(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT coalesce(max(load_priority),0)+1 FROM mods WHERE mod_type='ue4ss'",
        [],
        |row| row.get(0),
    )?)
}
/// The packaged mod that already owns a payload file name, if any. `exclude`
/// skips the mod an installation is replacing, whose files are being taken over
/// rather than collided with.
pub fn packaged_source_name_owner(
    conn: &Connection,
    library_relative: &str,
    exclude: Option<&str>,
) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT m.id FROM mod_files f JOIN mods m ON m.id=f.mod_id WHERE m.mod_type IN ('pak','iostore') AND lower(f.library_relative)=lower(?1) AND m.id IS NOT ?2 LIMIT 1",
            params![library_relative, exclude],
            |row| row.get(0),
        )
        .optional()?)
}

/// The mod that already deploys to a path, if any.
pub fn destination_owner(
    conn: &Connection,
    destination: &str,
    exclude: Option<&str>,
) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT mod_id FROM mod_files WHERE destination=?1 AND mod_id IS NOT ?2 LIMIT 1",
            params![destination, exclude],
            |row| row.get(0),
        )
        .optional()?)
}

/// The UE4SS mod occupying a runtime folder name, if any.
pub fn ue4ss_folder_owner(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT id FROM mods WHERE mod_type='ue4ss' AND lower(deployment_key)=lower(?1) LIMIT 1",
            [key],
            |row| row.get(0),
        )
        .optional()?)
}

pub fn summary_of(conn: &Connection, id: &str) -> Result<(String, Option<String>)> {
    Ok(
        conn.query_row("SELECT name,version FROM mods WHERE id=?1", [id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?,
    )
}
pub fn set_load_priority(tx: &Transaction<'_>, id: &str, priority: i64) -> Result<()> {
    tx.execute(
        "UPDATE mods SET load_priority=?2 WHERE id=?1",
        params![id, priority],
    )?;
    Ok(())
}
pub fn update_file_destination(
    tx: &Transaction<'_>,
    mod_id: &str,
    library_relative: &str,
    destination: &str,
) -> Result<()> {
    tx.execute(
        "UPDATE mod_files SET destination=?3 WHERE mod_id=?1 AND library_relative=?2",
        params![mod_id, library_relative, destination],
    )?;
    Ok(())
}
pub fn recorded_destination(
    conn: &Connection,
    mod_id: &str,
    library_relative: &str,
) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT destination FROM mod_files WHERE mod_id=?1 AND library_relative=?2",
            params![mod_id, library_relative],
            |row| row.get(0),
        )
        .optional()?)
}
pub fn conflicts_for_packages(
    conn: &Connection,
    packages: &[String],
) -> Result<Vec<PreviewConflict>> {
    let mut counts: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut statement = conn.prepare(
        "SELECT m.id,m.name FROM mod_packages p JOIN mods m ON m.id=p.mod_id WHERE p.package_id=?1",
    )?;
    for package in packages {
        for row in statement.query_map([package], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })? {
            *counts.entry(row?).or_default() += 1;
        }
    }
    Ok(counts
        .into_iter()
        .map(|((mod_id, name), package_count)| PreviewConflict {
            mod_id,
            name,
            package_count,
        })
        .collect())
}
pub fn package_members(conn: &Connection) -> Result<Vec<(String, String)>> {
    let mut statement =
        conn.prepare("SELECT package_id,mod_id FROM mod_packages ORDER BY package_id,mod_id")?;
    let result = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(result)
}
/// What the rest of the application needs to know about an installed mod
/// without loading its file list.
pub struct ModRecord {
    pub name: String,
    pub load_priority: Option<i64>,
    /// UE4SS mod folder names, one per line in the stored column. Rows written
    /// before an archive could hold several mods carry a single key, and rows
    /// that predate the column fall back to the mod name.
    pub keys: Vec<String>,
    pub mod_type: String,
    pub enabled: bool,
}

pub fn mod_record(conn: &Connection, id: &str) -> Result<ModRecord> {
    let (name, stored, mod_type, enabled, load_priority) = conn.query_row(
        "SELECT name,deployment_key,mod_type,enabled,load_priority FROM mods WHERE id=?1",
        [id],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, bool>(3)?,
                r.get::<_, Option<i64>>(4)?,
            ))
        },
    )?;
    let mut keys: Vec<String> = stored
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    if keys.is_empty() && mod_type == "ue4ss" {
        keys.push(name.clone());
    }
    Ok(ModRecord {
        name,
        load_priority,
        keys,
        mod_type,
        enabled,
    })
}

pub fn rename_mod(conn: &Connection, id: &str, name: &str) -> Result<()> {
    let changed = conn.execute("UPDATE mods SET name=?2 WHERE id=?1", params![id, name])?;
    if changed == 0 {
        return Err(crate::error::AppError::Other(
            "That mod is no longer installed.".into(),
        ));
    }
    Ok(())
}

pub fn record_backup(
    tx: &Connection,
    mod_id: &str,
    destination: &str,
    backup_relative: &str,
    sha256: &str,
) -> Result<()> {
    tx.execute(
        "INSERT INTO mod_backups(mod_id,destination,backup_relative,sha256) VALUES(?1,?2,?3,?4) ON CONFLICT(mod_id,destination) DO UPDATE SET backup_relative=excluded.backup_relative,sha256=excluded.sha256",
        params![mod_id, destination, backup_relative, sha256],
    )?;
    Ok(())
}

/// The recorded original for one destination, as `(library path, checksum)`.
pub fn backup_for(
    conn: &Connection,
    mod_id: &str,
    destination: &str,
) -> Result<Option<(String, String)>> {
    Ok(conn
        .query_row(
            "SELECT backup_relative,sha256 FROM mod_backups WHERE mod_id=?1 AND destination=?2",
            params![mod_id, destination],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?)
}

pub fn backups(conn: &Connection, mod_id: &str) -> Result<Vec<(String, String, String)>> {
    let mut statement = conn.prepare(
        "SELECT destination,backup_relative,sha256 FROM mod_backups WHERE mod_id=?1 ORDER BY destination",
    )?;
    let rows = statement
        .query_map([mod_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn conflict_count(conn: &Connection) -> Result<usize> {
    let package:i64=conn.query_row("SELECT count(*) FROM (SELECT p.package_id FROM mod_packages p JOIN mods m ON m.id=p.mod_id WHERE m.enabled=1 GROUP BY p.package_id HAVING count(*)>1)",[],|r|r.get(0))?;
    let files:i64=conn.query_row("SELECT count(*) FROM (SELECT f.destination FROM mod_files f JOIN mods m ON m.id=f.mod_id WHERE m.enabled=1 GROUP BY f.destination HAVING count(*)>1)",[],|r|r.get(0))?;
    Ok((package + files) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ModSummary;
    use tempfile::tempdir;

    fn summary(id: &str) -> ModSummary {
        ModSummary {
            id: id.into(),
            name: id.into(),
            version: None,
            mod_type: "iostore".into(),
            enabled: true,
            installed_at: "2026-08-28T00:00:00Z".into(),
            installed_build: None,
            package_count: 0,
            conflict_count: 0,
            potential_conflict_count: 0,
            load_priority: None,
            files: vec![],
        }
    }

    fn add(conn: &mut Connection, id: &str, destination: &str, packages: &[String]) {
        let tx = conn.transaction().unwrap();
        insert_mod(
            &tx,
            &summary(id),
            "",
            None,
            &[(
                format!("{id}.pak"),
                destination.into(),
                1,
                id.repeat(64 / id.len().max(1)),
            )],
            packages,
        )
        .unwrap();
        tx.commit().unwrap();
    }

    #[test]
    fn overlapping_packages_are_counted_without_names() {
        let d = tempdir().unwrap();
        let mut conn = open(&d.path().join("db")).unwrap();
        add(&mut conn, "a", "/mods/a.pak", &["hashed-package".into()]);
        add(&mut conn, "b", "/mods/b.pak", &["hashed-package".into()]);
        assert_eq!(conflict_count(&conn).unwrap(), 1);
    }

    #[test]
    fn duplicate_deployment_filename_is_counted() {
        let d = tempdir().unwrap();
        let mut conn = open(&d.path().join("db")).unwrap();
        add(&mut conn, "a", "/mods/shared.pak", &[]);
        add(&mut conn, "b", "/mods/shared.pak", &[]);
        assert_eq!(conflict_count(&conn).unwrap(), 1);
    }

    #[test]
    fn unrelated_mods_have_no_conflict() {
        let d = tempdir().unwrap();
        let mut conn = open(&d.path().join("db")).unwrap();
        add(&mut conn, "a", "/mods/a.pak", &["one".into()]);
        add(&mut conn, "b", "/mods/b.pak", &["two".into()]);
        assert_eq!(conflict_count(&conn).unwrap(), 0);
    }

    #[test]
    fn migrates_v1_and_assigns_newer_packaged_mods_higher_priority() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("db");
        let legacy = Connection::open(&path).unwrap();
        legacy.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
          CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
          CREATE TABLE mods(id TEXT PRIMARY KEY, name TEXT NOT NULL, version TEXT, mod_type TEXT NOT NULL, deployment_key TEXT NOT NULL DEFAULT '', source_archive TEXT, installed_at TEXT NOT NULL, enabled INTEGER NOT NULL, installed_build TEXT);
          CREATE TABLE mod_files(id INTEGER PRIMARY KEY, mod_id TEXT NOT NULL REFERENCES mods(id) ON DELETE CASCADE, library_relative TEXT NOT NULL, destination TEXT NOT NULL, size INTEGER NOT NULL, sha256 TEXT NOT NULL);
          CREATE TABLE mod_packages(mod_id TEXT NOT NULL REFERENCES mods(id) ON DELETE CASCADE, package_id TEXT NOT NULL, PRIMARY KEY(mod_id, package_id));
          INSERT INTO schema_migrations VALUES(1,datetime('now'));
          INSERT INTO mods VALUES('old','Old',NULL,'pak','',NULL,'2026-01-01',1,NULL);
          INSERT INTO mods VALUES('new','New',NULL,'iostore','',NULL,'2026-02-01',1,NULL);
          INSERT INTO mods VALUES('lua','Lua',NULL,'ue4ss','',NULL,'2026-03-01',1,NULL);").unwrap();
        drop(legacy);
        let migrated = open(&path).unwrap();
        let priorities = migrated
            .prepare("SELECT id,load_priority FROM mods ORDER BY id")
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?))
            })
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            priorities,
            vec![
                ("lua".into(), None),
                ("new".into(), Some(2)),
                ("old".into(), Some(1))
            ]
        );
    }
}
