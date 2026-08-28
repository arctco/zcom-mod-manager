use crate::error::{AppError, Result};
use std::{
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    process::Command,
};
use uuid::Uuid;
use walkdir::WalkDir;

pub struct Staging {
    pub root: PathBuf,
    pub warnings: Vec<String>,
}

fn unsafe_name(path: &Path) -> bool {
    path.is_absolute()
        || path.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
}

fn suspicious(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("exe" | "bat" | "cmd" | "ps1" | "dll" | "sh")
    )
}

fn copy_tree(source: &Path, destination: &Path, warnings: &mut Vec<String>) -> Result<()> {
    for entry in WalkDir::new(source).follow_links(false) {
        let entry = entry.map_err(|e| AppError::Other(e.to_string()))?;
        let rel = entry
            .path()
            .strip_prefix(source)
            .map_err(|e| AppError::Other(e.to_string()))?;
        if rel.as_os_str().is_empty() {
            continue;
        }
        if unsafe_name(rel) {
            return Err(AppError::UnsafeArchive(rel.display().to_string()));
        }
        if entry.file_type().is_symlink() {
            return Err(AppError::UnsafeArchive(format!(
                "symbolic link {}",
                rel.display()
            )));
        }
        let target = destination.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else {
            if suspicious(rel) {
                warnings.push(format!("Ignored executable content: {}", rel.display()));
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn extract_zip(source: &Path, destination: &Path, warnings: &mut Vec<String>) -> Result<()> {
    let file = fs::File::open(source)?;
    let mut archive = zip::ZipArchive::new(file)?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| AppError::UnsafeArchive(entry.name().to_string()))?
            .to_path_buf();
        if unsafe_name(&enclosed) {
            return Err(AppError::UnsafeArchive(entry.name().to_string()));
        }
        #[cfg(unix)]
        if entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            return Err(AppError::UnsafeArchive(format!(
                "symbolic link {}",
                entry.name()
            )));
        }
        let target = destination.join(&enclosed);
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if suspicious(&enclosed) {
            warnings.push(format!(
                "Ignored executable content: {}",
                enclosed.display()
            ));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = fs::File::create(target)?;
        std::io::copy(&mut entry, &mut output)?;
        output.flush()?;
    }
    Ok(())
}

fn find_7z() -> Option<PathBuf> {
    let binary = if cfg!(windows) { "7z.exe" } else { "7z" };
    std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join(binary))
            .find(|p| p.is_file())
    })
}

fn extract_7z(source: &Path, destination: &Path, warnings: &mut Vec<String>) -> Result<()> {
    let seven = find_7z().ok_or(AppError::SevenZipNotFound)?;
    let listing = Command::new(&seven)
        .args(["l", "-slt", "--"])
        .arg(source)
        .output()?;
    if !listing.status.success() {
        return Err(AppError::Other("7z could not read the archive".into()));
    }
    let text = String::from_utf8_lossy(&listing.stdout);
    let mut entries = false;
    for line in text.lines() {
        if line.starts_with("----------") {
            entries = true;
            continue;
        }
        if !entries {
            continue;
        }
        if let Some(name) = line.strip_prefix("Path = ") {
            let path = Path::new(name);
            if unsafe_name(path) {
                return Err(AppError::UnsafeArchive(name.into()));
            }
            if suspicious(path) {
                warnings.push(format!("Ignored executable content: {name}"));
            }
        }
    }
    let output = Command::new(seven)
        .args(["x", "-y", "-snl", "-snh"])
        .arg(format!("-o{}", destination.display()))
        .arg("--")
        .arg(source)
        .output()?;
    if !output.status.success() {
        return Err(AppError::Other(format!(
            "7z extraction failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    for entry in WalkDir::new(destination).follow_links(false) {
        let entry = entry.map_err(|e| AppError::Other(e.to_string()))?;
        if entry.file_type().is_symlink() {
            return Err(AppError::UnsafeArchive(
                "archive created a symbolic link".into(),
            ));
        }
    }
    Ok(())
}

pub fn stage(source: &Path, cache: &Path) -> Result<Staging> {
    if !source.exists() {
        return Err(AppError::Other(
            "The selected source no longer exists.".into(),
        ));
    }
    let root = cache.join("staging").join(Uuid::new_v4().to_string());
    fs::create_dir_all(&root)?;
    let mut warnings = Vec::new();
    let result = if source.is_dir() {
        copy_tree(source, &root, &mut warnings)
    } else {
        match source
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref()
        {
            Some("zip") => extract_zip(source, &root, &mut warnings),
            Some("7z") => extract_7z(source, &root, &mut warnings),
            Some("pak" | "utoc" | "ucas") => {
                let stem = source.file_stem().unwrap_or_default();
                for ext in ["pak", "utoc", "ucas"] {
                    let candidate = source.with_extension(ext);
                    if candidate.is_file() {
                        fs::copy(
                            &candidate,
                            root.join(format!("{}.{}", stem.to_string_lossy(), ext)),
                        )?;
                    }
                }
                Ok(())
            }
            _ => Err(AppError::ModNotRecognized),
        }
    };
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&root);
        return Err(error);
    }
    Ok(Staging { root, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;
    #[test]
    fn rejects_zip_traversal() {
        let d = tempdir().unwrap();
        let path = d.path().join("bad.zip");
        let f = fs::File::create(&path).unwrap();
        let mut z = zip::ZipWriter::new(f);
        z.start_file("../../evil.pak", SimpleFileOptions::default())
            .unwrap();
        z.write_all(b"malicious").unwrap();
        z.finish().unwrap();
        assert!(matches!(
            stage(&path, d.path()),
            Err(AppError::UnsafeArchive(_))
        ));
    }
    #[test]
    fn absolute_path_is_unsafe() {
        assert!(unsafe_name(Path::new("/tmp/evil")));
        assert!(unsafe_name(Path::new("../evil")));
        assert!(!unsafe_name(Path::new("safe/mod.pak")));
    }
    #[test]
    fn directory_symlink_is_rejected() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let s = tempdir().unwrap();
            let c = tempdir().unwrap();
            fs::write(s.path().join("real"), b"x").unwrap();
            symlink(s.path().join("real"), s.path().join("link")).unwrap();
            assert!(stage(s.path(), c.path()).is_err());
        }
    }

    #[test]
    fn rejects_absolute_zip_path() {
        let d = tempdir().unwrap();
        let path = d.path().join("absolute.zip");
        let mut z = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        z.start_file("/tmp/evil.pak", SimpleFileOptions::default())
            .unwrap();
        z.write_all(b"malicious").unwrap();
        z.finish().unwrap();
        assert!(matches!(
            stage(&path, d.path()),
            Err(AppError::UnsafeArchive(_))
        ));
    }

    #[test]
    fn extracts_nested_7z_when_tool_is_available() {
        let Some(seven) = find_7z() else { return };
        let d = tempdir().unwrap();
        let input = d.path().join("input/SomeMod");
        fs::create_dir_all(&input).unwrap();
        fs::write(input.join("Nested_P.pak"), b"pak").unwrap();
        let archive = d.path().join("mod.7z");
        let status = Command::new(seven)
            .current_dir(d.path().join("input"))
            .args(["a", "-y"])
            .arg(&archive)
            .arg("SomeMod")
            .status()
            .unwrap();
        assert!(status.success());
        let staged = stage(&archive, d.path()).unwrap();
        assert!(staged.root.join("SomeMod/Nested_P.pak").is_file());
    }
}
