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
    /// Archive-relative paths that carry native code. Whether those matter is a
    /// question about the mod layout rather than about the archive, so the
    /// judgement is left to `crate::mods`.
    pub executables: Vec<String>,
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

/// Interprets an archive member name as a relative path.
///
/// ZIP mandates `/` as the separator, but Windows packaging tools regularly
/// write `\`. A non-Windows host then reads `ue4ss\Mods\X\main.dll` as a single
/// long file name, the mod folder never materializes, and detection fails on an
/// archive that installs correctly on Windows. Both separators are therefore
/// treated as directory boundaries on every platform.
fn archive_relative(name: &str) -> Option<PathBuf> {
    // An absolute member name is never legitimate, and rebasing it silently
    // would hide an archive that tried to write outside its own tree.
    if name.starts_with('/') || name.starts_with('\\') {
        return None;
    }
    let mut path = PathBuf::new();
    for part in name.split(['/', '\\']) {
        match part {
            "" | "." => continue,
            ".." => return None,
            // A Windows drive prefix survives as an ordinary component on other
            // platforms, so it is rejected by hand rather than by `unsafe_name`.
            _ if part.contains(':') || part.contains('\0') => return None,
            _ => path.push(part),
        }
    }
    (!path.as_os_str().is_empty() && !unsafe_name(&path)).then_some(path)
}

fn suspicious(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("exe" | "bat" | "cmd" | "ps1" | "dll" | "sh" | "msi" | "scr" | "vbs")
    )
}

fn note_executable(executables: &mut Vec<String>, relative: &Path) {
    if suspicious(relative) {
        executables.push(relative.display().to_string().replace('\\', "/"));
    }
}

fn copy_tree(source: &Path, destination: &Path, executables: &mut Vec<String>) -> Result<()> {
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
            note_executable(executables, rel);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn extract_zip(source: &Path, destination: &Path, executables: &mut Vec<String>) -> Result<()> {
    let file = fs::File::open(source)?;
    let mut archive = zip::ZipArchive::new(file)?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let enclosed = archive_relative(entry.name())
            .ok_or_else(|| AppError::UnsafeArchive(entry.name().to_string()))?;
        #[cfg(unix)]
        if entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            return Err(AppError::UnsafeArchive(format!(
                "symbolic link {}",
                entry.name()
            )));
        }
        let target = destination.join(&enclosed);
        // A directory entry may also be spelled with a trailing separator that
        // `archive_relative` has already dropped, so both forms are checked.
        if entry.is_dir() || entry.name().ends_with(['/', '\\']) {
            fs::create_dir_all(&target)?;
            continue;
        }
        note_executable(executables, &enclosed);
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

/// Rebuilds directories from member names that kept `\` as their separator.
/// `extract_zip` handles this while reading, but an external extractor writes
/// whatever the archive contained, so the staged tree is repaired afterwards.
fn split_backslash_names(root: &Path) -> Result<()> {
    loop {
        let offender = WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(std::result::Result::ok)
            .find(|entry| {
                entry.file_type().is_file() && entry.file_name().to_string_lossy().contains('\\')
            })
            .map(|entry| entry.path().to_path_buf());
        let Some(offender) = offender else {
            return Ok(());
        };
        let name = offender.file_name().unwrap_or_default().to_string_lossy();
        let rebuilt =
            archive_relative(&name).ok_or_else(|| AppError::UnsafeArchive(name.to_string()))?;
        let target = offender.parent().unwrap_or(root).join(rebuilt);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&offender, &target)?;
    }
}

fn extract_7z(source: &Path, destination: &Path, executables: &mut Vec<String>) -> Result<()> {
    let seven = find_7z().ok_or(AppError::SevenZipNotFound)?;
    let listing = Command::new(&seven)
        .args(["l", "-slt", "--"])
        .arg(source)
        .output()?;
    if !listing.status.success() {
        // RAR support is a separate, non-free codec that many 7-Zip builds omit,
        // and the failure otherwise looks like a corrupt download.
        return Err(AppError::Other(format!(
            "7z could not read this archive. {}",
            if source
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("rar"))
            {
                "RAR needs a 7-Zip build with the RAR codec; extract it yourself and install the \
                 folder instead."
            } else {
                "It may be corrupt or use an unsupported compression method."
            }
        )));
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
            let path =
                archive_relative(name).ok_or_else(|| AppError::UnsafeArchive(name.to_string()))?;
            note_executable(executables, &path);
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
    split_backslash_names(destination)
}

pub fn stage(source: &Path, cache: &Path) -> Result<Staging> {
    if !source.exists() {
        return Err(AppError::Other(
            "The selected source no longer exists.".into(),
        ));
    }
    let root = cache.join("staging").join(Uuid::new_v4().to_string());
    fs::create_dir_all(&root)?;
    let mut executables = Vec::new();
    let result = if source.is_dir() {
        copy_tree(source, &root, &mut executables)
    } else {
        match source
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref()
        {
            Some("zip") => extract_zip(source, &root, &mut executables),
            Some("7z" | "rar") => extract_7z(source, &root, &mut executables),
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
    Ok(Staging {
        root,
        warnings: Vec::new(),
        executables,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;

    fn zip_with(path: &Path, entries: &[(&str, &[u8])]) {
        let mut writer = zip::ZipWriter::new(fs::File::create(path).unwrap());
        for (name, body) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(body).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn rejects_zip_traversal() {
        let d = tempdir().unwrap();
        let path = d.path().join("bad.zip");
        zip_with(&path, &[("../../evil.pak", b"malicious")]);
        assert!(matches!(
            stage(&path, d.path()),
            Err(AppError::UnsafeArchive(_))
        ));
    }

    #[test]
    fn rejects_traversal_spelled_with_backslashes() {
        let d = tempdir().unwrap();
        let path = d.path().join("bad.zip");
        zip_with(&path, &[("..\\..\\evil.pak", b"malicious")]);
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
    fn windows_separators_become_directories() {
        let d = tempdir().unwrap();
        let path = d.path().join("windows.zip");
        zip_with(
            &path,
            &[
                ("ue4ss\\Mods\\ZCUnlocked\\enabled.txt", b"1"),
                ("ue4ss\\Mods\\ZCUnlocked\\dlls\\main.dll", b"MZ"),
            ],
        );
        let staged = stage(&path, d.path()).unwrap();
        assert!(staged
            .root
            .join("ue4ss/Mods/ZCUnlocked/dlls/main.dll")
            .is_file());
        assert_eq!(
            staged.executables,
            vec!["ue4ss/Mods/ZCUnlocked/dlls/main.dll"]
        );
    }

    #[test]
    fn drive_letters_are_rejected() {
        assert!(archive_relative("C:\\Windows\\evil.dll").is_none());
        assert!(archive_relative("mods/Good_P.pak").is_some());
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
        zip_with(&path, &[("/tmp/evil.pak", b"malicious")]);
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

    #[test]
    fn repairs_names_an_external_extractor_left_flat() {
        let d = tempdir().unwrap();
        let root = d.path().join("staged");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("ue4ss\\Mods\\Thing\\enabled.txt"), b"1").unwrap();
        split_backslash_names(&root).unwrap();
        assert!(root.join("ue4ss/Mods/Thing/enabled.txt").is_file());
    }
}
