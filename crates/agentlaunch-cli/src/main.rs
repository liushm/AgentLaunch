use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use agentlaunch_core::{
    AppConfig, DiscoveryEngine, LaunchMode, LaunchOptions, Launcher, check_proxy_reachable,
};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "al",
    version,
    about = "AgentLaunch CLI - Launch AI agents with per-agent proxy settings"
)]
struct Cli {
    /// Custom config file path (defaults to OS config directory)
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan and list all known/configured agents and their dynamically resolved paths
    Scan {
        /// Show only installed agents
        #[arg(short, long)]
        installed: bool,
    },
    /// List configured proxy profiles and test TCP reachability
    Proxies,
    /// Launch an agent with proxy settings injected
    Run {
        /// Agent ID or name (e.g., codex, antigravity, claude-code)
        agent: String,
        /// Override proxy profile ID (e.g., local-http, clash, direct)
        #[arg(short, long)]
        proxy: Option<String>,
        /// Terminate existing Electron instances before launching
        #[arg(short, long)]
        force_restart: bool,
        /// Skip TCP reachability check before launching
        #[arg(long)]
        skip_proxy_check: bool,
        /// Print the resolved command, args, and env vars without launching
        #[arg(long)]
        dry_run: bool,
        /// Extra arguments passed through to the agent
        #[arg(last = true)]
        extra_args: Vec<String>,
    },
    /// Create a desktop shortcut for an agent (preserves the original app icon)
    Shortcut {
        /// Agent ID or name (e.g., codex, antigravity)
        agent: String,
    },
    /// Print the active config file path
    Config,
}

fn load_config(custom_path: Option<&PathBuf>) -> agentlaunch_core::Result<(AppConfig, PathBuf)> {
    if let Some(p) = custom_path {
        let cfg = AppConfig::load_from(p)?;
        Ok((cfg, p.clone()))
    } else {
        AppConfig::load_or_create_default()
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let (config, config_path) = match load_config(cli.config.as_ref()) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error loading config: {e}");
            return ExitCode::FAILURE;
        }
    };

    match cli.command {
        Commands::Shortcut { agent } => {
            let Some(agent_cfg) = config.find_agent(&agent) else {
                eprintln!("Agent '{agent}' not found. Run `al scan` to see available IDs.");
                return ExitCode::FAILURE;
            };
            // Prefer sibling GUI executable (no console window flash on Windows); fallback to current exe.
            let current = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("al"));
            let sibling_gui = current.parent().and_then(|dir| {
                for name in ["agentlaunch-slint.exe", "agentlaunch.exe", "agentlaunch-slint", "agentlaunch"] {
                    let cand = dir.join(name);
                    if cand.exists() {
                        return Some(cand);
                    }
                }
                None
            });
            let launcher_exe = sibling_gui.unwrap_or(current);
            match agentlaunch_core::create_desktop_shortcut(agent_cfg, &launcher_exe) {
                Ok(res) => {
                    println!("Created shortcut: {}", res.shortcut_path.display());
                    println!("Launcher target:  {}", res.target_launcher.display());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("Failed to create shortcut: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Commands::Config => {
            println!("{}", config_path.display());
            ExitCode::SUCCESS
        }
        Commands::Scan { installed } => {
            let resolved = DiscoveryEngine::resolve_all(&config.agents);
            println!(
                "{:<16} {:<16} {:<10} {:<16} {}",
                "ID", "NAME", "KIND", "VERSION", "EXECUTABLE"
            );
            println!("{}", "-".repeat(92));
            for item in resolved {
                if installed && !item.is_installed() {
                    continue;
                }
                let kind = format!("{:?}", item.kind);
                let ver = item.version.as_deref().unwrap_or("-");
                let exe = match &item.executable {
                    Some(p) => format!("{} ({})", p.display(), item.source),
                    None => format!("[not found: {}]", item.source),
                };
                println!(
                    "{:<16} {:<16} {:<10} {:<16} {}",
                    item.id, item.name, kind, ver, exe
                );
            }
            ExitCode::SUCCESS
        }
        Commands::Proxies => {
            println!(
                "{:<14} {:<22} {:<28} {}",
                "ID", "NAME", "URL", "STATUS"
            );
            println!("{}", "-".repeat(88));
            for (id, profile) in &config.proxies {
                let marker = if id == &config.default_proxy { "*" } else { " " };
                let check = check_proxy_reachable(profile, Duration::from_millis(250));
                let status = if check.reachable {
                    format!("OK ({})", check.detail)
                } else {
                    format!("DOWN ({})", check.detail)
                };
                let url = if profile.is_direct() {
                    "(direct)"
                } else {
                    profile.url.as_str()
                };
                println!(
                    "{marker}{:<13} {:<22} {:<28} {}",
                    id, profile.name, url, status
                );
            }
            ExitCode::SUCCESS
        }
        Commands::Run {
            agent,
            proxy,
            force_restart,
            skip_proxy_check,
            dry_run,
            extra_args,
        } => {
            let Some(agent_cfg) = config.find_agent(&agent) else {
                eprintln!("Agent '{agent}' not found. Run `al scan` to see available IDs.");
                return ExitCode::FAILURE;
            };

            let opts = LaunchOptions {
                proxy_override: proxy,
                extra_args,
                working_dir_override: std::env::current_dir().ok(),
                mode: LaunchMode::InheritTerminal,
                force_restart,
                skip_proxy_check,
            };

            if dry_run {
                match Launcher::build_plan(&config, agent_cfg, &opts) {
                    Ok(plan) => {
                        println!("Agent:      {} ({:?})", plan.agent_name, plan.kind);
                        println!(
                            "Proxy:      {} ({})",
                            plan.proxy_id,
                            if plan.proxy_url.is_empty() {
                                "direct"
                            } else {
                                &plan.proxy_url
                            }
                        );
                        println!("Executable: {}", plan.executable.display());
                        println!("Args:       {:?}", plan.args);
                        println!("Env:");
                        for (k, v) in &plan.env_vars {
                            println!("  {k}={v}");
                        }
                        if !plan.unset_vars.is_empty() {
                            println!("Unset:      {:?}", plan.unset_vars);
                        }
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("Dry-run failed: {e}");
                        ExitCode::FAILURE
                    }
                }
            } else {
                match Launcher::launch(&config, agent_cfg, &opts) {
                    Ok(res) => {
                        if !res.killed_pids.is_empty() {
                            println!("Stopped previous instance(s): {:?}", res.killed_pids);
                        }
                        println!(
                            "Launched {} (pid: {:?}) via proxy '{}' [{}]",
                            res.plan.agent_name,
                            res.pid,
                            res.plan.proxy_id,
                            if res.plan.proxy_url.is_empty() {
                                "direct"
                            } else {
                                &res.plan.proxy_url
                            }
                        );
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("Launch failed: {e}");
                        ExitCode::FAILURE
                    }
                }
            }
        }
    }
}
