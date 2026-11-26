use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// How an agent should be launched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    /// Chromium / Electron desktop app: receives `--proxy-server` + `--proxy-bypass-list` + env vars.
    Electron,
    /// Terminal-based CLI agent (e.g., `claude`, `codex`): receives env vars and runs in a terminal.
    Cli,
    /// Generic GUI/background executable: receives env vars only.
    Generic,
}

/// How to locate an agent's executable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Locator {
    /// Dynamically resolve via a built-in detector ID (e.g., "codex", "antigravity", "claude-code").
    Builtin(String),
    /// Fixed custom executable path (or command name in PATH).
    Path(PathBuf),
}

impl Default for Locator {
    fn default() -> Self {
        Self::Builtin(String::new())
    }
}

/// Preferred terminal emulator when launching a `Cli` agent from GUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TerminalKind {
    #[default]
    Auto,
    WindowsTerminal,
    PowerShell,
    Cmd,
    MacTerminal,
    ITerm2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyProfile {
    pub name: String,
    /// Proxy URL, e.g. `http://127.0.0.1:8118` or `socks5://127.0.0.1:7890`.
    /// Empty string means direct connection (no proxy).
    #[serde(default)]
    pub url: String,
    /// Hosts/patterns that bypass the proxy.
    #[serde(default = "default_bypass")]
    pub bypass: Vec<String>,
}

impl ProxyProfile {
    pub fn is_direct(&self) -> bool {
        self.url.trim().is_empty()
    }
}

pub fn default_bypass() -> Vec<String> {
    vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentConfig {
    pub id: String,
    pub name: String,
    pub kind: AgentKind,
    pub locator: Locator,
    /// Optional manual path override (takes precedence over `locator`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_path: Option<PathBuf>,
    /// Proxy profile ID. If `None`, uses `AppConfig::default_proxy`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<String>,
    /// Extra CLI arguments appended on launch.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_args: Vec<String>,
    /// Extra environment variables injected on launch.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra_env: BTreeMap<String, String>,
    /// Optional working directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<PathBuf>,
    /// Terminal preference for `AgentKind::Cli`.
    #[serde(default)]
    pub terminal: TerminalKind,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub appearance: Appearance,
    #[serde(default = "default_proxy_id")]
    pub default_proxy: String,
    #[serde(default = "default_true")]
    pub check_proxy_before_launch: bool,
    #[serde(default = "default_true")]
    pub warn_if_already_running: bool,
    #[serde(default)]
    pub proxies: BTreeMap<String, ProxyProfile>,
    #[serde(default)]
    pub agents: Vec<AgentConfig>,
}

fn default_proxy_id() -> String {
    "local-http".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for AppConfig {
    fn default() -> Self {
        let mut proxies = BTreeMap::new();
        proxies.insert(
            "local-http".to_string(),
            ProxyProfile {
                name: "Local HTTP (8118)".to_string(),
                url: "http://127.0.0.1:8118".to_string(),
                bypass: default_bypass(),
            },
        );

        let agents = crate::discovery::registry::builtin_descriptors()
            .into_iter()
            .map(|d| d.into_default_config())
            .collect();

        Self {
            default_proxy: default_proxy_id(),
            appearance: Appearance::System,
            check_proxy_before_launch: true,
            warn_if_already_running: true,
            proxies,
            agents,
        }
    }
}

impl AppConfig {
    /// Standard per-OS configuration file path:
    /// - Windows: `%APPDATA%\AgentLaunch\config.toml`
    /// - macOS: `~/Library/Application Support/AgentLaunch/config.toml`
    /// - Linux: `~/.config/agentlaunch/config.toml`
    pub fn default_path() -> Result<PathBuf> {
        let proj = ProjectDirs::from("dev", "AgentLaunch", "AgentLaunch")
            .ok_or_else(|| CoreError::Config("cannot determine OS config directory".into()))?;
        let existing_path = proj.config_dir().join("config.toml");
        #[cfg(target_os = "macos")]
        {
            let base = directories::BaseDirs::new()
                .ok_or_else(|| CoreError::Config("cannot determine OS config directory".into()))?;
            let path = base.config_dir().join("AgentLaunch/config.toml");
            if path.try_exists()? || !existing_path.try_exists()? {
                return Ok(path);
            }
        }
        Ok(existing_path)
    }

    pub fn load_or_create_default() -> Result<(Self, PathBuf)> {
        let path = Self::default_path()?;
        if path.exists() {
            let cfg = Self::load_from(&path)?;
            Ok((cfg, path))
        } else {
            let cfg = Self::default();
            cfg.save_to(&path)?;
            Ok((cfg, path))
        }
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let cfg: Self = toml::from_str(&content)?;
        Ok(cfg)
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let serialized = toml::to_string_pretty(self)?;
        fs::write(path, serialized)?;
        Ok(())
    }

    pub fn find_agent(&self, id: &str) -> Option<&AgentConfig> {
        self.agents
            .iter()
            .find(|a| a.id.eq_ignore_ascii_case(id) || a.name.eq_ignore_ascii_case(id))
    }

    pub fn find_agent_mut(&mut self, id: &str) -> Option<&mut AgentConfig> {
        self.agents
            .iter_mut()
            .find(|a| a.id.eq_ignore_ascii_case(id) || a.name.eq_ignore_ascii_case(id))
    }

    pub fn resolve_proxy_for<'a>(
        &'a self,
        agent: &AgentConfig,
        override_proxy: Option<&str>,
    ) -> Result<(&'a str, &'a ProxyProfile)> {
        let key = override_proxy
            .or(agent.proxy.as_deref())
            .unwrap_or(&self.default_proxy);
        let (stored_key, profile) = self
            .proxies
            .get_key_value(key)
            .ok_or_else(|| CoreError::ProxyNotFound(key.to_string()))?;
        Ok((stored_key.as_str(), profile))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_roundtrip() {
        let cfg = AppConfig::default();
        let toml_str = toml::to_string_pretty(&cfg).expect("serialize");
        let parsed: AppConfig = toml::from_str(&toml_str).expect("deserialize");
        assert_eq!(cfg, parsed);
        assert!(parsed.proxies.contains_key("local-http"));
        assert!(parsed.find_agent("codex").is_some());
        assert!(parsed.find_agent("antigravity").is_some());
    }
}
