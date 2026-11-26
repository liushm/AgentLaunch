use std::collections::BTreeMap;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use crate::config::ProxyProfile;

/// Environment variables and CLI arguments derived from a `ProxyProfile`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProxyEnv {
    /// Environment variables to inject into the launched process.
    pub env_vars: BTreeMap<String, String>,
    /// Environment variables to explicitly remove (e.g., when switching to Direct mode).
    pub unset_vars: Vec<String>,
    /// Chromium / Electron proxy flags.
    pub electron_args: Vec<String>,
}

const PROXY_ENV_KEYS: &[&str] = &[
    "HTTP_PROXY",
    "http_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
    "NO_PROXY",
    "no_proxy",
    "GRPC_PROXY",
    "grpc_proxy",
];

impl ProxyEnv {
    pub fn from_profile(profile: &ProxyProfile) -> Self {
        if profile.is_direct() {
            return Self {
                env_vars: BTreeMap::new(),
                unset_vars: PROXY_ENV_KEYS.iter().map(|s| s.to_string()).collect(),
                electron_args: vec!["--no-proxy-server".to_string()],
            };
        }

        let url = profile.url.trim().to_string();
        let mut env_vars = BTreeMap::new();

        for key in [
            "HTTP_PROXY",
            "http_proxy",
            "HTTPS_PROXY",
            "https_proxy",
            "ALL_PROXY",
            "all_proxy",
            "GRPC_PROXY",
            "grpc_proxy",
        ] {
            env_vars.insert(key.to_string(), url.clone());
        }

        // Node.js 22.21+/24+ built-in fetch (undici) respects env proxies when enabled.
        env_vars.insert("NODE_USE_ENV_PROXY".to_string(), "1".to_string());

        let mut electron_args = vec![format!("--proxy-server={url}")];

        if !profile.bypass.is_empty() {
            let no_proxy_csv = profile.bypass.join(",");
            env_vars.insert("NO_PROXY".to_string(), no_proxy_csv.clone());
            env_vars.insert("no_proxy".to_string(), no_proxy_csv);

            // Chromium `--proxy-bypass-list` uses semicolons (`;`).
            let bypass_semi = profile.bypass.join(";");
            electron_args.push(format!("--proxy-bypass-list={bypass_semi}"));
        }

        Self {
            env_vars,
            unset_vars: Vec::new(),
            electron_args,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyCheckResult {
    pub reachable: bool,
    pub latency_ms: Option<u128>,
    pub detail: String,
}

/// Parses `scheme://[user:pass@]host:port[/...]` into `(host, port)`.
pub fn parse_proxy_host_port(url: &str) -> Option<(String, u16)> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return None;
    }

    let without_scheme = match trimmed.split_once("://") {
        Some((_, rest)) => rest,
        None => trimmed,
    };

    // Strip path/query
    let authority = without_scheme.split('/').next()?;
    // Strip optional userinfo (`user:pass@`)
    let host_port = match authority.rsplit_once('@') {
        Some((_, hp)) => hp,
        None => authority,
    };

    // Handle IPv6 `[::1]:8118` or IPv4/hostname `127.0.0.1:8118`
    if let Some(rest) = host_port.strip_prefix('[') {
        let (host, port_part) = rest.split_once("]:")?;
        let port = port_part.parse::<u16>().ok()?;
        Some((host.to_string(), port))
    } else {
        let (host, port_str) = host_port.rsplit_once(':')?;
        if host.is_empty() {
            return None;
        }
        let port = port_str.parse::<u16>().ok()?;
        Some((host.to_string(), port))
    }
}

/// Performs a fast TCP handshake to check whether the proxy port is listening.
pub fn check_proxy_reachable(profile: &ProxyProfile, timeout: Duration) -> ProxyCheckResult {
    if profile.is_direct() {
        return ProxyCheckResult {
            reachable: true,
            latency_ms: Some(0),
            detail: "Direct connection (no proxy)".to_string(),
        };
    }

    let Some((host, port)) = parse_proxy_host_port(&profile.url) else {
        return ProxyCheckResult {
            reachable: false,
            latency_ms: None,
            detail: format!("Invalid proxy URL: {}", profile.url),
        };
    };

    let addr_str = format!("{host}:{port}");
    let addrs: Vec<SocketAddr> = match addr_str.to_socket_addrs() {
        Ok(iter) => iter.collect(),
        Err(e) => {
            return ProxyCheckResult {
                reachable: false,
                latency_ms: None,
                detail: format!("DNS/address error for {addr_str}: {e}"),
            };
        }
    };

    if addrs.is_empty() {
        return ProxyCheckResult {
            reachable: false,
            latency_ms: None,
            detail: format!("No socket address resolved for {addr_str}"),
        };
    }

    let start = Instant::now();
    let mut last_err = None;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(_) => {
                let ms = start.elapsed().as_millis();
                return ProxyCheckResult {
                    reachable: true,
                    latency_ms: Some(ms),
                    detail: format!("Reachable ({addr_str}, {ms}ms)"),
                };
            }
            Err(e) => last_err = Some(e),
        }
    }

    ProxyCheckResult {
        reachable: false,
        latency_ms: None,
        detail: format!(
            "Cannot connect to {addr_str}: {}",
            last_err
                .map(|e| e.to_string())
                .unwrap_or_else(|| "unknown error".to_string())
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::default_bypass;

    #[test]
    fn parses_proxy_urls() {
        assert_eq!(
            parse_proxy_host_port("http://127.0.0.1:8118"),
            Some(("127.0.0.1".into(), 8118))
        );
        assert_eq!(
            parse_proxy_host_port("socks5://user:pass@127.0.0.1:7890"),
            Some(("127.0.0.1".into(), 7890))
        );
        assert_eq!(
            parse_proxy_host_port("http://[::1]:8118"),
            Some(("::1".into(), 8118))
        );
        assert_eq!(parse_proxy_host_port(""), None);
    }

    #[test]
    fn builds_electron_and_env_separators_correctly() {
        let profile = ProxyProfile {
            name: "test".into(),
            url: "http://127.0.0.1:8118".into(),
            bypass: default_bypass(),
        };
        let env = ProxyEnv::from_profile(&profile);
        assert_eq!(
            env.env_vars.get("HTTP_PROXY").map(String::as_str),
            Some("http://127.0.0.1:8118")
        );
        assert_eq!(
            env.env_vars.get("http_proxy").map(String::as_str),
            Some("http://127.0.0.1:8118")
        );
        assert_eq!(
            env.env_vars.get("NO_PROXY").map(String::as_str),
            Some("localhost,127.0.0.1,::1")
        );
        assert_eq!(
            env.electron_args,
            vec![
                "--proxy-server=http://127.0.0.1:8118",
                "--proxy-bypass-list=localhost;127.0.0.1;::1",
            ]
        );
    }

    #[test]
    fn direct_profile_unsets_proxy_vars() {
        let profile = ProxyProfile {
            name: "direct".into(),
            url: String::new(),
            bypass: vec![],
        };
        let env = ProxyEnv::from_profile(&profile);
        assert!(env.env_vars.is_empty());
        assert!(env.unset_vars.contains(&"HTTP_PROXY".to_string()));
        assert_eq!(env.electron_args, vec!["--no-proxy-server"]);
    }
}
