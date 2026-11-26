use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

use crate::config::AgentKind;
use crate::discovery::{AgentDescriptor, ResolvedAgent, parse_version_parts};

pub fn resolve_windows(desc: &AgentDescriptor) -> ResolvedAgent {
    // CLI agents: search PATH and common user bin directories first.
    if desc.kind == AgentKind::Cli {
        if let Some((path, src)) = find_cli(desc.cli_names) {
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

    // 1. Appx / MSIX packages (e.g., OpenAI.Codex, AnthropicPBC.Claude whose path changes on every update).
    if !desc.win_appx_prefixes.is_empty()
        && !desc.win_appx_rel_exes.is_empty()
        && let Some((exe, version)) =
            find_appx_executable(desc.win_appx_prefixes, desc.win_appx_rel_exes)
    {
        return ResolvedAgent {
            id: desc.id.to_string(),
            name: desc.name.to_string(),
            kind: desc.kind,
            executable: Some(exe),
            version,
            source: "Windows Appx/MSIX".to_string(),
        };
    }

    // 2. Squirrel versioned directories under %LOCALAPPDATA%\<Dir>\app-*\<exe>
    if let Some(dir) = desc.win_squirrel_dir
        && let Some((exe, version)) = find_squirrel_executable(dir, desc.win_exe_name)
    {
        return ResolvedAgent {
            id: desc.id.to_string(),
            name: desc.name.to_string(),
            kind: desc.kind,
            executable: Some(exe),
            version,
            source: "Squirrel app dir".to_string(),
        };
    }

    // 3. Standard user/system installation directories (%LOCALAPPDATA%\Programs, %ProgramFiles%)
    if let Some(exe) = find_in_program_dirs(desc.win_program_rel_paths) {
        return ResolvedAgent {
            id: desc.id.to_string(),
            name: desc.name.to_string(),
            kind: desc.kind,
            executable: Some(exe),
            version: None,
            source: "Programs dir".to_string(),
        };
    }

    // 4. Windows Uninstall registry keys (handles custom install drives like F:\software\...)
    if !desc.win_uninstall_keywords.is_empty()
        && let Some((exe, version)) =
            find_in_uninstall_registry(desc.win_uninstall_keywords, desc.win_exe_name)
    {
        return ResolvedAgent {
            id: desc.id.to_string(),
            name: desc.name.to_string(),
            kind: desc.kind,
            executable: Some(exe),
            version,
            source: "Windows Registry".to_string(),
        };
    }

    // 5. Fallback to PATH for Electron apps that expose a CLI wrapper
    if let Some((path, src)) = find_cli(desc.cli_names) {
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

/// Reads `HKCU\Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages`
/// to locate the newest installed MSIX/Appx package matching any entry in `prefixes` (e.g., `OpenAI.Codex`, `AnthropicPBC.Claude`).
/// This avoids spawning PowerShell and takes < 1ms.
fn find_appx_executable(prefixes: &[&str], rel_exes: &[&str]) -> Option<(PathBuf, Option<String>)> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let packages_key = hkcu
        .open_subkey(
            r"Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages",
        )
        .ok()?;

    let prefixes_lower: Vec<String> = prefixes.iter().map(|p| p.to_ascii_lowercase()).collect();
    let mut candidates: Vec<(Vec<u64>, Option<String>, PathBuf)> = Vec::new();

    for subkey_name in packages_key.enum_keys().flatten() {
        let lower = subkey_name.to_ascii_lowercase();
        let matched = prefixes_lower.iter().any(|prefix_lower| {
            lower.starts_with(prefix_lower)
                && (lower.len() == prefix_lower.len()
                    || matches!(lower.as_bytes()[prefix_lower.len()], b'_' | b'.'))
        });
        if !matched {
            continue;
        }

        if let Ok(subkey) = packages_key.open_subkey(&subkey_name)
            && let Ok(root) = subkey.get_value::<String, _>("PackageRootFolder")
        {
            let root_path = PathBuf::from(&root);
            if let Some(exe_path) = rel_exes
                .iter()
                .map(|rel| root_path.join(rel))
                .find(|p| p.exists())
            {
                // Package full name format: `<Name>_<Version>_<Arch>_<ResourceId>_<PublisherId>`
                let version = subkey_name.split('_').nth(1).map(|v| v.to_string());
                let sort_key = version
                    .as_deref()
                    .map(parse_version_parts)
                    .unwrap_or_default();
                candidates.push((sort_key, version, exe_path));
            }
        }
    }

    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    candidates
        .into_iter()
        .next()
        .map(|(_, ver, path)| (path, ver))
}

/// Scans `%LOCALAPPDATA%\<app_dir>\app-*` and picks the highest version containing `exe_name`.
fn find_squirrel_executable(app_dir: &str, exe_name: &str) -> Option<(PathBuf, Option<String>)> {
    let local_app_data = env::var_os("LOCALAPPDATA")?;
    let base = PathBuf::from(local_app_data).join(app_dir);
    if !base.is_dir() {
        return None;
    }

    let mut candidates: Vec<(Vec<u64>, String, PathBuf)> = Vec::new();
    if let Ok(entries) = fs::read_dir(&base) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(ver_str) = name.strip_prefix("app-") {
                let exe = entry.path().join(exe_name);
                if exe.exists() {
                    candidates.push((parse_version_parts(ver_str), ver_str.to_string(), exe));
                }
            }
        }
    }

    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    if let Some((_, ver, path)) = candidates.into_iter().next() {
        return Some((path, Some(ver)));
    }

    // Fallback: direct exe inside base dir
    let direct = base.join(exe_name);
    if direct.exists() {
        return Some((direct, None));
    }

    None
}

fn find_in_program_dirs(rel_paths: &[&str]) -> Option<PathBuf> {
    let mut bases: Vec<PathBuf> = Vec::new();
    if let Some(local) = env::var_os("LOCALAPPDATA") {
        bases.push(PathBuf::from(&local).join("Programs"));
        bases.push(PathBuf::from(local));
    }
    if let Some(pf) = env::var_os("ProgramFiles") {
        bases.push(PathBuf::from(pf));
    }
    if let Some(pf86) = env::var_os("ProgramFiles(x86)") {
        bases.push(PathBuf::from(pf86));
    }

    for base in bases {
        for rel in rel_paths {
            let candidate = base.join(rel);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

fn find_in_uninstall_registry(
    keywords: &[&str],
    exe_name: &str,
) -> Option<(PathBuf, Option<String>)> {
    let roots = [
        (
            HKEY_CURRENT_USER,
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
        (
            HKEY_LOCAL_MACHINE,
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
        (
            HKEY_LOCAL_MACHINE,
            r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
    ];

    for (hive, subkey_path) in roots {
        let Ok(key) = RegKey::predef(hive).open_subkey(subkey_path) else {
            continue;
        };
        for name in key.enum_keys().flatten() {
            let Ok(app_key) = key.open_subkey(&name) else {
                continue;
            };
            let Ok(display_name) = app_key.get_value::<String, _>("DisplayName") else {
                continue;
            };
            let dn_lower = display_name.to_ascii_lowercase();
            if !keywords
                .iter()
                .any(|kw| dn_lower.contains(&kw.to_ascii_lowercase()))
            {
                continue;
            }

            let version = app_key.get_value::<String, _>("DisplayVersion").ok();

            // Try InstallLocation + exe_name
            if let Ok(loc) = app_key.get_value::<String, _>("InstallLocation")
                && !loc.trim().is_empty()
            {
                let candidate = PathBuf::from(loc.trim().trim_matches('"')).join(exe_name);
                if candidate.exists() {
                    return Some((candidate, version));
                }
            }

            // Try DisplayIcon (often `C:\Path\App.exe,0`)
            if let Ok(icon) = app_key.get_value::<String, _>("DisplayIcon") {
                let raw_path = icon.split(',').next().unwrap_or("").trim().trim_matches('"');
                let p = Path::new(raw_path);
                if p.exists()
                    && p.file_name()
                        .and_then(|f| f.to_str())
                        .is_some_and(|f| f.eq_ignore_ascii_case(exe_name))
                {
                    return Some((p.to_path_buf(), version));
                }
            }
        }
    }
    None
}

fn find_cli(names: &[&str]) -> Option<(PathBuf, String)> {
    // 1. Standard PATH lookup (prefer .exe / .cmd over .ps1 when available)
    for name in names {
        for ext in [".exe", ".cmd", ""] {
            let query = format!("{name}{ext}");
            if let Ok(path) = which::which(&query) {
                return Some((path, "PATH".to_string()));
            }
        }
    }

    // 2. Common user tool directories on Windows
    let mut extra_dirs: Vec<PathBuf> = Vec::new();
    if let Some(appdata) = env::var_os("APPDATA") {
        extra_dirs.push(PathBuf::from(appdata).join("npm"));
    }
    if let Some(profile) = env::var_os("USERPROFILE") {
        let home = PathBuf::from(profile);
        extra_dirs.push(home.join(".local").join("bin"));
        extra_dirs.push(home.join(".bun").join("bin"));
        extra_dirs.push(home.join(".cargo").join("bin"));
    }

    for dir in extra_dirs {
        for name in names {
            for ext in [".exe", ".cmd", ".ps1"] {
                let candidate = dir.join(format!("{name}{ext}"));
                if candidate.exists() {
                    return Some((candidate, format!("dir ({})", dir.display())));
                }
            }
        }
    }

    None
}
