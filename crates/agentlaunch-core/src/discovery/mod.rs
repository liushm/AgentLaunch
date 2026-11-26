pub mod registry;

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(windows)]
pub mod windows;

use std::path::PathBuf;

use crate::config::{AgentConfig, AgentKind, Locator};

/// Result of resolving an agent's executable path on the current machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAgent {
    pub id: String,
    pub name: String,
    pub kind: AgentKind,
    /// Resolved absolute path to the executable, if installed.
    pub executable: Option<PathBuf>,
    /// Detected version string (e.g. from Appx folder name or Squirrel dir), if available.
    pub version: Option<String>,
    /// Human-readable description of how the path was located.
    pub source: String,
}

impl ResolvedAgent {
    pub fn is_installed(&self) -> bool {
        self.executable.is_some()
    }
}

/// Static descriptor for a known agent in the built-in registry.
#[derive(Debug, Clone)]
pub struct AgentDescriptor {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: AgentKind,
    /// Windows Appx/MSIX package name prefix (e.g. `"OpenAI.Codex"`).
    pub win_appx_prefix: Option<&'static str>,
    /// Candidate relative executable paths inside the Appx package root (checked in order).
    pub win_appx_rel_exes: &'static [&'static str],
    /// Windows Squirrel folder name under `%LOCALAPPDATA%` (e.g. `"AnthropicClaude"`).
    pub win_squirrel_dir: Option<&'static str>,
    /// Candidate relative paths under `%LOCALAPPDATA%\Programs` or `%PROGRAMFILES%`.
    pub win_program_rel_paths: &'static [&'static str],
    /// Keywords for matching Windows Uninstall registry `DisplayName`.
    pub win_uninstall_keywords: &'static [&'static str],
    /// Expected executable filename (e.g. `"Antigravity.exe"`, `"claude"`).
    pub win_exe_name: &'static str,
    /// macOS `.app` bundle name (e.g. `"Antigravity.app"`).
    pub mac_app_bundle: Option<&'static str>,
    /// CLI binary names to search on PATH / common bin dirs.
    pub cli_names: &'static [&'static str],
}

impl AgentDescriptor {
    pub fn into_default_config(&self) -> AgentConfig {
        AgentConfig {
            id: self.id.to_string(),
            name: self.name.to_string(),
            kind: self.kind,
            locator: Locator::Builtin(self.id.to_string()),
            override_path: None,
            proxy: None,
            extra_args: Vec::new(),
            extra_env: Default::default(),
            working_dir: None,
            terminal: Default::default(),
        }
    }
}

pub struct DiscoveryEngine;

impl DiscoveryEngine {
    /// Scans all built-in known agents on this machine.
    pub fn scan_builtin() -> Vec<ResolvedAgent> {
        registry::builtin_descriptors()
            .iter()
            .map(Self::resolve_descriptor)
            .collect()
    }

    /// Resolves every agent defined in `agents` (preserving order).
    pub fn resolve_all(agents: &[AgentConfig]) -> Vec<ResolvedAgent> {
        agents.iter().map(Self::resolve_config).collect()
    }

    /// Dynamically resolves the latest executable path for a configured agent.
    pub fn resolve_config(agent: &AgentConfig) -> ResolvedAgent {
        // 1. User manual override takes top priority.
        if let Some(path) = &agent.override_path {
            let exists = path.exists();
            return ResolvedAgent {
                id: agent.id.clone(),
                name: agent.name.clone(),
                kind: agent.kind,
                executable: if exists { Some(path.clone()) } else { None },
                version: None,
                source: if exists {
                    "manual override".to_string()
                } else {
                    format!("manual override not found: {}", path.display())
                },
            };
        }

        // 2. Resolve according to Locator strategy.
        match &agent.locator {
            Locator::Builtin(builtin_id) => {
                if let Some(desc) = registry::find_descriptor(builtin_id) {
                    let mut resolved = Self::resolve_descriptor(&desc);
                    resolved.id = agent.id.clone();
                    resolved.name = agent.name.clone();
                    resolved.kind = agent.kind;
                    resolved
                } else {
                    ResolvedAgent {
                        id: agent.id.clone(),
                        name: agent.name.clone(),
                        kind: agent.kind,
                        executable: None,
                        version: None,
                        source: format!("unknown built-in locator '{builtin_id}'"),
                    }
                }
            }
            Locator::Path(path) => {
                if path.exists() {
                    ResolvedAgent {
                        id: agent.id.clone(),
                        name: agent.name.clone(),
                        kind: agent.kind,
                        executable: Some(path.clone()),
                        version: None,
                        source: "custom path".to_string(),
                    }
                } else if let Some(cmd) = path.to_str().and_then(|s| which::which(s).ok()) {
                    ResolvedAgent {
                        id: agent.id.clone(),
                        name: agent.name.clone(),
                        kind: agent.kind,
                        executable: Some(cmd),
                        version: None,
                        source: "PATH lookup".to_string(),
                    }
                } else {
                    ResolvedAgent {
                        id: agent.id.clone(),
                        name: agent.name.clone(),
                        kind: agent.kind,
                        executable: None,
                        version: None,
                        source: format!("path not found: {}", path.display()),
                    }
                }
            }
        }
    }

    pub fn resolve_descriptor(desc: &AgentDescriptor) -> ResolvedAgent {
        #[cfg(windows)]
        {
            windows::resolve_windows(desc)
        }
        #[cfg(target_os = "macos")]
        {
            macos::resolve_macos(desc)
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            resolve_fallback_unix(desc)
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn resolve_fallback_unix(desc: &AgentDescriptor) -> ResolvedAgent {
    for name in desc.cli_names {
        if let Ok(path) = which::which(name) {
            return ResolvedAgent {
                id: desc.id.to_string(),
                name: desc.name.to_string(),
                kind: desc.kind,
                executable: Some(path),
                version: None,
                source: "PATH".to_string(),
            };
        }
    }
    ResolvedAgent {
        id: desc.id.to_string(),
        name: desc.name.to_string(),
        kind: desc.kind,
        executable: None,
        version: None,
        source: "not found".to_string(),
    }
}

/// Parses a dotted numeric version string like `26.1120.6357.0` or `1.2.3` into a sortable tuple.
pub(crate) fn parse_version_parts(s: &str) -> Vec<u64> {
    s.trim_start_matches('v')
        .split('.')
        .map(|part| {
            let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse::<u64>().unwrap_or(0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_numerically() {
        assert!(parse_version_parts("26.1120.6357.0") > parse_version_parts("26.900.100.0"));
        assert!(parse_version_parts("1.10.0") > parse_version_parts("1.9.9"));
        assert!(parse_version_parts("v2.0.1") > parse_version_parts("v1.9.9"));
    }
}
