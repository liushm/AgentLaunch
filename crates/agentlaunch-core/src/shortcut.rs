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

/// Native Win32 COM (`IShellLinkW` + `IPersistFile`) shortcut creation — completes in < 1ms
/// without spawning PowerShell.
#[cfg(windows)]
fn create_windows_lnk(
    desktop: &Path,
    agent: &AgentConfig,
    launcher_exe: &Path,
    icon_source: Option<&Path>,
) -> Result<ShortcutResult> {
    use windows::Win32::Foundation::{RPC_E_CHANGED_MODE, S_FALSE, S_OK};
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize, IPersistFile,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    use windows::core::{HSTRING, Interface};

    let safe_name: String = agent
        .name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            _ => c,
        })
        .collect();
    let lnk_path = desktop.join(format!("{safe_name} (Proxy).lnk"));

    let target_h = HSTRING::from(launcher_exe.as_os_str());
    let args_h = HSTRING::from(format!("--launch {} --force-restart", agent.id));
    let desc_h = HSTRING::from(format!("Launch {} via AgentLaunch proxy", agent.name));
    let icon_h = HSTRING::from(icon_source.unwrap_or(launcher_exe).as_os_str());
    let lnk_h = HSTRING::from(lnk_path.as_os_str());

    unsafe {
        let init_hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let should_uninit = init_hr == S_OK || init_hr == S_FALSE;
        if init_hr.is_err() && init_hr != RPC_E_CHANGED_MODE {
            return Err(CoreError::Config(format!(
                "CoInitializeEx failed: {init_hr:?}"
            )));
        }

        let res: Result<()> = (|| {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| CoreError::Config(format!("CoCreateInstance(ShellLink) failed: {e}")))?;

            link.SetPath(&target_h)
                .map_err(|e| CoreError::Config(format!("IShellLinkW::SetPath failed: {e}")))?;
            link.SetArguments(&args_h)
                .map_err(|e| CoreError::Config(format!("IShellLinkW::SetArguments failed: {e}")))?;
            link.SetDescription(&desc_h)
                .map_err(|e| CoreError::Config(format!("IShellLinkW::SetDescription failed: {e}")))?;
            link.SetIconLocation(&icon_h, 0)
                .map_err(|e| CoreError::Config(format!("IShellLinkW::SetIconLocation failed: {e}")))?;

            let persist: IPersistFile = link
                .cast()
                .map_err(|e| CoreError::Config(format!("cast to IPersistFile failed: {e}")))?;
            persist
                .Save(&lnk_h, true)
                .map_err(|e| CoreError::Config(format!("IPersistFile::Save failed: {e}")))?;
            Ok(())
        })();

        if should_uninit {
            CoUninitialize();
        }
        res?;
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
