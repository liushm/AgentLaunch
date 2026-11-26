use std::env;
use std::path::{Path, PathBuf};

use crate::config::AgentKind;
use crate::discovery::{AgentDescriptor, ResolvedAgent};

pub fn resolve_macos(desc: &AgentDescriptor) -> ResolvedAgent {
    if desc.kind == AgentKind::Cli {
        if let Some((path, src)) = find_cli_macos(desc.cli_names) {
            return ResolvedAgent {
                id: desc.id.to_string(),
                name: desc.name.to_string(),
                kind: desc.kind,
                executable: Some(path),
                version: None,
                source: src,
            };
        }
        return not_found(desc);
    }

    // 1. Check /Applications and ~/Applications for `.app` bundle and resolve `Contents/MacOS/<CFBundleExecutable>`
    if let Some(bundle_name) = desc.mac_app_bundle
        && let Some((exe, version)) = find_app_bundle_executable(bundle_name, desc.mac_bundle_id)
    {
        let bundle_name = exe
            .ancestors()
            .nth(3)
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .unwrap_or(bundle_name);
        let source = format!("macOS bundle ({bundle_name})");
        return ResolvedAgent {
            id: desc.id.to_string(),
            name: desc.name.to_string(),
            kind: desc.kind,
            executable: Some(exe),
            version,
            source,
        };
    }

    // 2. Fallback to CLI name on PATH
    if let Some((path, src)) = find_cli_macos(desc.cli_names) {
        return ResolvedAgent {
            id: desc.id.to_string(),
            name: desc.name.to_string(),
            kind: desc.kind,
            executable: Some(path),
            version: None,
            source: src,
        };
    }

    not_found(desc)
}

fn not_found(desc: &AgentDescriptor) -> ResolvedAgent {
    ResolvedAgent {
        id: desc.id.to_string(),
        name: desc.name.to_string(),
        kind: desc.kind,
        executable: None,
        version: None,
        source: "not installed".to_string(),
    }
}

fn find_app_bundle_executable(
    bundle_name: &str,
    bundle_id: Option<&str>,
) -> Option<(PathBuf, Option<String>)> {
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Some(home) = env::var_os("HOME") {
        roots.push(PathBuf::from(home).join("Applications"));
    }

    for root in roots {
        let bundle = root.join(bundle_name);
        if let Some(found) = read_bundle_executable(&bundle, bundle_id) {
            return Some(found);
        }
        if bundle_id.is_some()
            && let Ok(entries) = root.read_dir()
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if path != bundle
                    && path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
                    && let Some(found) = read_bundle_executable(&path, bundle_id)
                {
                    return Some(found);
                }
            }
        }
    }
    None
}

fn read_bundle_executable(
    bundle: &Path,
    bundle_id: Option<&str>,
) -> Option<(PathBuf, Option<String>)> {
    let plist_path = bundle.join("Contents").join("Info.plist");
    let value = plist::Value::from_file(&plist_path).ok()?;
    let dict = value.as_dictionary()?;
    if let Some(id) = bundle_id
        && dict.get("CFBundleIdentifier").and_then(plist::Value::as_string) != Some(id)
    {
        return None;
    }
    let exec_name = dict.get("CFBundleExecutable")?.as_string()?;
    let version = dict
        .get("CFBundleShortVersionString")
        .and_then(|v| v.as_string())
        .map(|s| s.to_string());

    let exe_path = bundle.join("Contents").join("MacOS").join(exec_name);
    if exe_path.exists() {
        Some((exe_path, version))
    } else {
        None
    }
}

fn find_cli_macos(names: &[&str]) -> Option<(PathBuf, String)> {
    for name in names {
        if let Ok(path) = which::which(name) {
            return Some((path, "PATH".to_string()));
        }
    }

    let mut extra_dirs = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ];
    if let Some(home) = env::var_os("HOME") {
        let h = PathBuf::from(home);
        extra_dirs.push(h.join(".local").join("bin"));
        extra_dirs.push(h.join(".bun").join("bin"));
        extra_dirs.push(h.join(".cargo").join("bin"));
    }

    for dir in extra_dirs {
        for name in names {
            let candidate = dir.join(name);
            if candidate.exists() {
                return Some((candidate, format!("dir ({})", dir.display())));
            }
        }
    }
    None
}
