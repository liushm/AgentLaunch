pub mod config;
pub mod discovery;
pub mod error;
pub mod launcher;
pub mod proxy;
pub mod shortcut;

pub use config::{AgentConfig, AgentKind, AppConfig, Locator, ProxyProfile, TerminalKind};
pub use discovery::{AgentDescriptor, DiscoveryEngine, ResolvedAgent};
pub use error::{CoreError, Result};
pub use launcher::{
    LaunchMode, LaunchOptions, LaunchPlan, LaunchResult, Launcher, RunningInstance,
};
pub use proxy::{ProxyCheckResult, ProxyEnv, check_proxy_reachable};
pub use shortcut::{ShortcutResult, create_desktop_shortcut};
