use crate::{
    error::{AppError, Result},
    models::ToolInfo,
};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug)]
pub struct Inspection {
    pub package_ids: Vec<String>,
    pub package_paths: Vec<String>,
    pub details: String,
}

pub fn find(configured: Option<&str>) -> ToolInfo {
    let name = if cfg!(windows) { "retoc.exe" } else { "retoc" };
    let path = configured
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .or_else(|| {
            std::env::current_exe().ok().and_then(|exe| {
                let directory = exe.parent()?;
                [
                    directory.join(name),
                    directory.join(if cfg!(windows) { "retoc.exe" } else { "retoc" }),
                ]
                .into_iter()
                .find(|path| path.is_file())
            })
        })
        .or_else(|| {
            std::env::var_os("PATH").and_then(|v| {
                std::env::split_paths(&v)
                    .map(|d| d.join(name))
                    .find(|p| p.is_file())
            })
        });
    let version = path
        .as_ref()
        .and_then(|p| Command::new(p).arg("--version").output().ok())
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    ToolInfo {
        found: path.is_some(),
        path: path.map(|p| p.display().to_string()),
        version,
    }
}

fn sanitized(text: &str) -> String {
    let home = dirs::home_dir().map(|p| p.display().to_string());
    let mut value = text.trim().to_string();
    if let Some(home) = home {
        value = value.replace(&home, "~");
    }
    value.lines().take(20).collect::<Vec<_>>().join("\n")
}
pub fn inspect(tool: &ToolInfo, utoc: &Path) -> Result<Inspection> {
    let binary = tool.path.as_ref().ok_or(AppError::RetocNotFound)?;
    let verify = Command::new(binary).args(["verify"]).arg(utoc).output()?;
    if !verify.status.success() {
        let detail = sanitized(&format!(
            "{}\n{}",
            String::from_utf8_lossy(&verify.stdout),
            String::from_utf8_lossy(&verify.stderr)
        ));
        return Err(AppError::RetocVerificationFailed(detail));
    }
    let list = Command::new(binary)
        .args(["list", "--package", "--path"])
        .arg(utoc)
        .output()?;
    if !list.status.success() {
        return Err(AppError::RetocVerificationFailed(sanitized(
            &String::from_utf8_lossy(&list.stderr),
        )));
    }
    let mut ids = Vec::new();
    let mut paths = Vec::new();
    let digits = Regex::new(r"^\d+$").unwrap();
    for line in String::from_utf8_lossy(&list.stdout).lines() {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if let Some(index) = parts.iter().position(|p| digits.is_match(p)) {
            let path = parts
                .last()
                .filter(|p| p.contains('/'))
                .map(|p| p.to_string());
            let mut hasher = Sha256::new();
            hasher.update(parts[index].as_bytes());
            ids.push(hex::encode(hasher.finalize()));
            if let Some(path) = path {
                paths.push(path);
            }
        }
    }
    ids.sort();
    ids.dedup();
    paths.sort();
    paths.dedup();
    let package_count = paths.len();
    Ok(Inspection {
        package_ids: ids,
        package_paths: paths,
        details: format!(
            "retoc {} verified {} container entr{}.",
            tool.version.as_deref().unwrap_or("unknown"),
            package_count,
            if package_count == 1 { "y" } else { "ies" }
        ),
    })
}
