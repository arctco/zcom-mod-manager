use crate::{
    error::Result,
    models::{AppSettings, ModFile, ModSummary},
};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::path::Path;

pub fn open(path: &Path) -> Result<Connection> {
    let connection = Connection::open(path)?;
    connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;
      CREATE TABLE IF NOT EXISTS schema_migrations(version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS mods(id TEXT PRIMARY KEY, name TEXT NOT NULL, version TEXT, mod_type TEXT NOT NULL, deployment_key TEXT NOT NULL DEFAULT '', source_archive TEXT, installed_at TEXT NOT NULL, enabled INTEGER NOT NULL, installed_build TEXT);
      CREATE TABLE IF NOT EXISTS mod_files(id INTEGER PRIMARY KEY, mod_id TEXT NOT NULL REFERENCES mods(id) ON DELETE CASCADE, library_relative TEXT NOT NULL, destination TEXT NOT NULL, size INTEGER NOT NULL, sha256 TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS mod_packages(mod_id TEXT NOT NULL REFERENCES mods(id) ON DELETE CASCADE, package_id TEXT NOT NULL, PRIMARY KEY(mod_id, package_id));
      INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES(1, datetime('now'));"
    )?;
    Ok(connection)
}

fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
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

pub fn counts(conn: &Connection) -> Result<(usize, usize)> {
    let total: i64 = conn.query_row("SELECT count(*) FROM mods", [], |r| r.get(0))?;
    let enabled: i64 = conn.query_row("SELECT count(*) FROM mods WHERE enabled=1", [], |r| {
        r.get(0)
    })?;
    Ok((total as usize, enabled as usize))
}

pub fn list_mods(conn: &Connection) -> Result<Vec<ModSummary>> {
    let mut stmt = conn.prepare("SELECT id,name,version,mod_type,enabled,installed_at,installed_build FROM mods ORDER BY installed_at DESC")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, bool>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, Option<String>>(6)?,
        ))
    })?;
    let mut result = Vec::new();
    for row in rows {
        let (id, name, version, mod_type, enabled, installed_at, installed_build) = row?;
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
        let conflict_count: i64=conn.query_row("SELECT count(DISTINCT other.mod_id) FROM mod_packages mine JOIN mod_packages other ON mine.package_id=other.package_id AND mine.mod_id<>other.mod_id WHERE mine.mod_id=?1",[&id],|r|r.get(0))?;
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
    tx.execute("INSERT INTO mods(id,name,version,mod_type,deployment_key,source_archive,installed_at,enabled,installed_build) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![summary.id,summary.name,summary.version,summary.mod_type,deployment_key,source,summary.installed_at,summary.enabled,summary.installed_build])?;
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
pub fn mod_kind(conn: &Connection, id: &str) -> Result<(String, String, bool)> {
    Ok(conn.query_row(
        "SELECT CASE WHEN deployment_key='' THEN name ELSE deployment_key END,mod_type,enabled FROM mods WHERE id=?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?)
}
pub fn conflict_count(conn: &Connection) -> Result<usize> {
    let package:i64=conn.query_row("SELECT count(*) FROM (SELECT package_id FROM mod_packages GROUP BY package_id HAVING count(*)>1)",[],|r|r.get(0))?;
    let files:i64=conn.query_row("SELECT count(*) FROM (SELECT destination FROM mod_files GROUP BY destination HAVING count(*)>1)",[],|r|r.get(0))?;
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
}
