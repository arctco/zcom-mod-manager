//! Linux `nxm://` registration.
//!
//! `xdg-mime` resolves a desktop entry by reading the first whitespace-separated
//! word of `Exec` and handing it to `command -v`. It never strips quotes. An
//! entry written as
//!
//! ```text
//! Exec="/home/user/.local/bin/app" %u
//! ```
//!
//! therefore resolves to the literal string `"/home/user/.local/bin/app`,
//! leading quote included, which is not a command. `xdg-mime` then skips the
//! entry without reporting anything and picks the next candidate — so the
//! association is written, is highest priority, and is silently ignored.
//!
//! `tauri-plugin-deep-link` always quotes, so its Linux registration cannot win
//! on any path. The entry is written here instead with an unquoted `Exec`,
//! falling back to a symbolic link at a space-free location when the real path
//! genuinely needs quoting.
//!
//! There is a second trap. When `XDG_CURRENT_DESKTOP` is set, `xdg-mime query`
//! reads `<desktop>-mimeapps.list` before the generic `mimeapps.list`, but
//! `xdg-mime default` only ever writes the generic one. An application that
//! claimed the scheme in the prefixed file therefore keeps it forever, however
//! many times another application registers correctly. The prefixed files are
//! updated here as well, and only where they already name this scheme.

use crate::error::{AppError, Result};
use std::path::{Path, PathBuf};

const MIME: &str = "x-scheme-handler/nxm";

/// The executable a launcher should point at. Under an AppImage the mounted
/// `current_exe` disappears on exit, so the outer image path is used.
fn executable() -> Result<PathBuf> {
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        return Ok(PathBuf::from(appimage));
    }
    std::env::current_exe().map_err(AppError::Io)
}

/// The plugin derives this from the executable name, and its `is_registered`
/// and `unregister` both look it up, so it has to match exactly.
fn desktop_file_name(executable: &Path) -> String {
    format!(
        "{}-handler.desktop",
        executable
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("app"))
            .to_string_lossy()
    )
}

/// A path is usable unquoted only when nothing in it would be split or eaten by
/// the shell word-splitting `xdg-mime` performs.
pub fn needs_launcher(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.is_empty()
        || text
            .chars()
            .any(|c| c.is_whitespace() || "\"'\\$`".contains(c))
}

/// The desktop entry. `Exec` is deliberately unquoted; see the module comment.
pub fn entry_contents(product_name: &str, launcher: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={product_name}\n\
         Exec={} %u\n\
         Terminal=false\n\
         NoDisplay=true\n\
         MimeType={MIME};\n",
        launcher.display()
    )
}

/// Resolves the path to put in `Exec`, creating a space-free symbolic link when
/// the executable's own path cannot be written unquoted.
fn launcher(data_dir: &Path) -> Result<PathBuf> {
    let executable = executable()?;
    if !needs_launcher(&executable) {
        return Ok(executable);
    }
    let link = data_dir.join("nxm-handler");
    std::fs::create_dir_all(data_dir)?;
    // A stale link would point at a previous build.
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(&executable, &link)?;
    if needs_launcher(&link) {
        return Err(AppError::Other(format!(
            "The application data directory contains characters that xdg-mime cannot handle: {}",
            link.display()
        )));
    }
    Ok(link)
}

/// Rewrites the `[Default Applications]` entry for `mime`, if the file already
/// has one and it differs. Returns `None` when nothing needed changing, so a
/// file that does not mention the scheme is never rewritten.
pub fn set_default_line(text: &str, mime: &str, desktop_file: &str) -> Option<String> {
    edit_default(text, mime, Some(desktop_file))
}

/// Removes the `[Default Applications]` entry for `mime`, but only when it
/// points at `desktop_file`, so another application's choice is left alone.
pub fn remove_default_line(text: &str, mime: &str, desktop_file: &str) -> Option<String> {
    edit_default(text, mime, None)
        .filter(|_| current_default(text, mime).is_some_and(|value| value.trim() == desktop_file))
}

fn current_default<'a>(text: &'a str, mime: &str) -> Option<&'a str> {
    let mut in_default = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_default = trimmed == "[Default Applications]";
        } else if in_default {
            if let Some(value) = trimmed.strip_prefix(mime).and_then(|r| r.strip_prefix('=')) {
                return Some(value);
            }
        }
    }
    None
}

fn edit_default(text: &str, mime: &str, replacement: Option<&str>) -> Option<String> {
    let existing = current_default(text, mime)?;
    if replacement.is_some_and(|value| existing.trim() == value) {
        return None;
    }
    let mut in_default = false;
    let mut output = Vec::new();
    let mut done = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_default = trimmed == "[Default Applications]";
        } else if in_default
            && !done
            && trimmed
                .strip_prefix(mime)
                .is_some_and(|rest| rest.starts_with('='))
        {
            done = true;
            match replacement {
                Some(value) => output.push(format!("{mime}={value}")),
                None => continue,
            }
            continue;
        }
        output.push(line.to_string());
    }
    let mut result = output.join("\n");
    result.push('\n');
    Some(result)
}

/// The desktop-prefixed lists `xdg-mime query` consults before the generic one.
fn prefixed_lists() -> Vec<PathBuf> {
    let Some(config) = dirs::config_dir() else {
        return Vec::new();
    };
    std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .filter(|desktop| !desktop.is_empty())
        .map(|desktop| config.join(format!("{}-mimeapps.list", desktop.to_ascii_lowercase())))
        .filter(|path| path.is_file())
        .collect()
}

fn apply_to_prefixed_lists(file_name: &str, claim: bool) -> Result<()> {
    for path in prefixed_lists() {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let updated = if claim {
            set_default_line(&text, MIME, file_name)
        } else {
            remove_default_line(&text, MIME, file_name)
        };
        if let Some(updated) = updated {
            std::fs::write(&path, updated)?;
        }
    }
    Ok(())
}

fn run(command: &str, args: &[&std::ffi::OsStr]) -> Result<()> {
    let status = std::process::Command::new(command)
        .args(args)
        .status()
        .map_err(|e| {
            AppError::Other(format!(
                "{command} is required to register nxm:// links but could not be run: {e}"
            ))
        })?;
    if !status.success() {
        return Err(AppError::Other(format!("{command} reported a failure.")));
    }
    Ok(())
}

/// Writes the desktop entry and makes it the default `nxm://` handler.
pub fn register(product_name: &str, data_dir: &Path) -> Result<()> {
    let launcher = launcher(data_dir)?;
    let file_name = desktop_file_name(&executable()?);
    let applications = dirs::data_dir()
        .ok_or_else(|| AppError::Other("No data directory is available.".into()))?
        .join("applications");
    std::fs::create_dir_all(&applications)?;
    std::fs::write(
        applications.join(&file_name),
        entry_contents(product_name, &launcher),
    )?;
    run("update-desktop-database", &[applications.as_os_str()])?;
    run(
        "xdg-mime",
        &[
            std::ffi::OsStr::new("default"),
            std::ffi::OsStr::new(&file_name),
            std::ffi::OsStr::new(MIME),
        ],
    )?;
    // `xdg-mime default` wrote the generic list, which a desktop-prefixed list
    // overrides. Claim the scheme there too, where one already names it.
    apply_to_prefixed_lists(&file_name, true)
}

/// Hands the scheme back: removes this application's entry from the prefixed
/// lists, leaving whatever else the system knows about to take over again.
pub fn unregister() -> Result<()> {
    apply_to_prefixed_lists(&desktop_file_name(&executable()?), false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_paths_need_no_launcher() {
        assert!(!needs_launcher(Path::new(
            "/home/user/.local/bin/zcom-mod-manager"
        )));
        assert!(!needs_launcher(Path::new("/usr/bin/zcom-mod-manager")));
    }

    #[test]
    fn paths_the_shell_would_split_need_a_launcher() {
        for path in [
            "/home/user/ZCOM modding/zcom-mod-manager",
            "/home/user/apps/zcom mod manager",
            "/home/user/it's/zcom-mod-manager",
            "/home/user/$HOME/zcom-mod-manager",
        ] {
            assert!(needs_launcher(Path::new(path)), "{path}");
        }
    }

    #[test]
    fn exec_is_written_unquoted() {
        let entry = entry_contents("ZCOM Mod Manager", Path::new("/usr/bin/zcom-mod-manager"));
        assert!(
            entry.contains("Exec=/usr/bin/zcom-mod-manager %u\n"),
            "xdg-mime cannot resolve a quoted Exec: {entry}"
        );
        assert!(!entry.contains('"'));
        assert!(entry.contains("MimeType=x-scheme-handler/nxm;"));
        assert!(entry.contains("Name=ZCOM Mod Manager"));
    }

    const HYPRLAND: &str = "[Default Applications]\n\nx-scheme-handler/nxm=other.desktop\n[Added Associations]\nx-scheme-handler/nxm=other.desktop\n";

    #[test]
    fn claims_the_scheme_in_a_desktop_prefixed_list() {
        let updated = set_default_line(HYPRLAND, "x-scheme-handler/nxm", "ours.desktop").unwrap();
        assert!(updated.contains("[Default Applications]\n\nx-scheme-handler/nxm=ours.desktop\n"));
        // Added Associations is not a default and must survive untouched.
        assert!(updated.contains("[Added Associations]\nx-scheme-handler/nxm=other.desktop"));
    }

    #[test]
    fn leaves_a_list_that_does_not_name_the_scheme_alone() {
        let text = "[Default Applications]\ntext/plain=editor.desktop\n";
        assert!(set_default_line(text, "x-scheme-handler/nxm", "ours.desktop").is_none());
    }

    #[test]
    fn rewriting_an_entry_that_already_matches_changes_nothing() {
        let text = "[Default Applications]\nx-scheme-handler/nxm=ours.desktop\n";
        assert!(set_default_line(text, "x-scheme-handler/nxm", "ours.desktop").is_none());
    }

    #[test]
    fn handing_back_only_removes_our_own_entry() {
        let ours = "[Default Applications]\nx-scheme-handler/nxm=ours.desktop\n";
        let removed = remove_default_line(ours, "x-scheme-handler/nxm", "ours.desktop").unwrap();
        assert!(!removed.contains("x-scheme-handler/nxm"));
        // Another application's choice is never taken away.
        assert!(remove_default_line(HYPRLAND, "x-scheme-handler/nxm", "ours.desktop").is_none());
    }

    #[test]
    fn the_file_name_matches_what_the_plugin_looks_up() {
        assert_eq!(
            desktop_file_name(Path::new("/usr/bin/zcom-mod-manager")),
            "zcom-mod-manager-handler.desktop"
        );
    }
}
