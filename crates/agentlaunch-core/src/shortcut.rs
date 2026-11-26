use std::path::{Path, PathBuf};

use directories::UserDirs;

use crate::config::AgentConfig;
use crate::discovery::DiscoveryEngine;
use crate::error::{CoreError, Result};

/// Result of creating a desktop shortcut for an agent.
#[derive(Debug, Clone)]
pub struct ShortcutResult {
    pub shortcut_path: PathBuf,
    pub target_launcher: PathBuf,
    pub icon_source: Option<PathBuf>,
}

/// Creates a desktop shortcut that invokes `launcher_exe --launch <agent.id> --force-restart`
/// while preserving the agent's original executable icon.
pub fn create_desktop_shortcut(
    agent: &AgentConfig,
    launcher_exe: &Path,
) -> Result<ShortcutResult> {
    let user_dirs = UserDirs::new()
        .ok_or_else(|| CoreError::Config("cannot determine user directories".into()))?;
    let desktop = user_dirs
        .desktop_dir()
        .ok_or_else(|| CoreError::Config("cannot determine Desktop directory".into()))?;

    let resolved = DiscoveryEngine::resolve_config(agent);
    let icon_source = resolved.executable;

    #[cfg(windows)]
    {
        create_windows_lnk(desktop, agent, launcher_exe, icon_source.as_deref())
    }
    #[cfg(target_os = "macos")]
    {
        create_macos_app_shortcut(desktop, agent, launcher_exe, icon_source.as_deref())
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (desktop, agent, launcher_exe, icon_source);
        Err(CoreError::Config(
            "desktop shortcut creation is currently supported on Windows and macOS".into(),
        ))
    }
}

#[cfg(windows)]
fn create_windows_lnk(
    desktop: &Path,
    agent: &AgentConfig,
    launcher_exe: &Path,
    icon_source: Option<&Path>,
) -> Result<ShortcutResult> {
    use std::process::Command;

    let safe_name: String = agent
        .name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            _ => c,
        })
        .collect();
    let lnk_path = desktop.join(format!("{safe_name} (Proxy).lnk"));

    let ps_escape = |s: &str| s.replace('\'', "''");
    let lnk_str = ps_escape(&lnk_path.display().to_string());
    let target_str = ps_escape(&launcher_exe.display().to_string());
    let args_str = ps_escape(&format!("--launch {} --force-restart", agent.id));
    let desc_str = ps_escape(&format!("Launch {} via AgentLaunch proxy", agent.name));

    let icon_line = if let Some(icon_exe) = icon_source {
        format!(
            "$s.IconLocation = '{},0';",
            ps_escape(&icon_exe.display().to_string())
        )
    } else {
        format!("$s.IconLocation = '{target_str},0';")
    };

    let script = format!(
        "$ws = New-Object -ComObject WScript.Shell; \
         $s = $ws.CreateShortcut('{lnk_str}'); \
         $s.TargetPath = '{target_str}'; \
         $s.Arguments = '{args_str}'; \
         $s.Description = '{desc_str}'; \
         {icon_line} \
         $s.Save();"
    );

    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
        .output()
        .map_err(CoreError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CoreError::Config(format!(
            "failed to create .lnk shortcut: {}",
            stderr.trim()
        )));
    }

    Ok(ShortcutResult {
        shortcut_path: lnk_path,
        target_launcher: launcher_exe.to_path_buf(),
        icon_source: icon_source.map(Path::to_path_buf),
    })
}

#[cfg(target_os = "macos")]
fn create_macos_app_shortcut(
    desktop: &Path,
    agent: &AgentConfig,
    launcher_exe: &Path,
    icon_source: Option<&Path>,
) -> Result<ShortcutResult> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    let app_dir = desktop.join(format!("{} (Proxy).app", agent.name));
    let contents = app_dir.join("Contents");
    let macos_dir = contents.join("MacOS");
    fs::create_dir_all(&macos_dir)?;

    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>launch</string>
    <key>CFBundleName</key>
    <string>{name} (Proxy)</string>
    <key>CFBundleIdentifier</key>
    <string>dev.agentlaunch.shortcut.{id}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
</dict>
</plist>
"#,
        name = agent.name,
        id = agent.id
    );
    fs::write(contents.join("Info.plist"), plist)?;

    let script = format!(
        "#!/bin/sh\nexec \"{}\" --launch \"{}\" --force-restart\n",
        launcher_exe.display(),
        agent.id
    );
    let exec_path = macos_dir.join("launch");
    fs::write(&exec_path, script)?;
    let mut perms = fs::metadata(&exec_path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&exec_path, perms)?;

    Ok(ShortcutResult {
        shortcut_path: app_dir,
        target_launcher: launcher_exe.to_path_buf(),
        icon_source: icon_source.map(Path::to_path_buf),
    })
}
