use std::path::PathBuf;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, CoreError>;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("agent '{0}' not found in configuration or built-in registry")]
    AgentNotFound(String),

    #[error("proxy profile '{0}' not found")]
    ProxyNotFound(String),

    #[error("executable for agent '{id}' could not be located ({detail})")]
    ExecutableNotFound { id: String, detail: String },

    #[error("agent '{id}' already has {count} running instance(s); stop them first or pass force_restart")]
    AlreadyRunning { id: String, count: usize },

    #[error("proxy '{url}' is not reachable: {reason}")]
    ProxyUnreachable { url: String, reason: String },

    #[error("failed to launch '{path}': {source}")]
    LaunchFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("TOML serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),
}
