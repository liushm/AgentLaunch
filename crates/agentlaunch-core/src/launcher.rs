use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use sysinfo::{ProcessesToUpdate, System};

use crate::config::{AgentConfig, AgentKind, AppConfig, TerminalKind};
use crate::discovery::DiscoveryEngine;
use crate::error::{CoreError, Result};
use crate::proxy::{ProxyEnv, check_proxy_reachable};

/// How CLI agents should be executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchMode {
    /// Run CLI agents inside the caller's current terminal (used by `agentlaunch-cli`).
    InheritTerminal,
    /// Open a new OS terminal window for CLI agents (used by `agentlaunch-gui`).
    SpawnTerminal,
}

#[derive(Debug, Clone)]
pub struct LaunchOptions {
    pub proxy_override: Option<String>,
    pub extra_args: Vec<String>,
    pub working_dir_override: Option<PathBuf>,
    pub mode: LaunchMode,
    /// If true, terminate any already-running instances of this Electron app before launching.
    pub force_restart: bool,
    /// Skip pre-launch TCP proxy reachability check.
    pub skip_proxy_check: bool,
}

impl Default for LaunchOptions {
    fn default() -> Self {
        Self {
            proxy_override: None,
            extra_args: Vec::new(),
            working_dir_override: None,
            mode: LaunchMode::SpawnTerminal,
            force_restart: false,
            skip_proxy_check: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RunningInstance {
    pub pid: u32,
    pub name: String,
    pub exe: Option<PathBuf>,
}

/// A fully resolved launch specification (can be inspected for dry-run or executed).
#[derive(Debug, Clone)]
pub struct LaunchPlan {
    pub agent_id: String,
    pub agent_name: String,
    pub kind: AgentKind,
    pub proxy_id: String,
    pub proxy_url: String,
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub env_vars: BTreeMap<String, String>,
    pub unset_vars: Vec<String>,
    pub working_dir: Option<PathBuf>,
    pub terminal: TerminalKind,
    pub mode: LaunchMode,
}

#[derive(Debug, Clone)]
pub struct LaunchResult {
    pub pid: Option<u32>,
    pub plan: LaunchPlan,
    pub killed_pids: Vec<u32>,
}

pub struct Launcher;

impl Launcher {
    /// Builds a `LaunchPlan` without starting the process (great for `--dry-run` or UI preview).
    pub fn build_plan(
        config: &AppConfig,
        agent: &AgentConfig,
        opts: &LaunchOptions,
    ) -> Result<LaunchPlan> {
        let (proxy_id, proxy_profile) =
            config.resolve_proxy_for(agent, opts.proxy_override.as_deref())?;

        let resolved = DiscoveryEngine::resolve_config(agent);
        let executable = resolved.executable.ok_or_else(|| CoreError::ExecutableNotFound {
            id: agent.id.clone(),
            detail: resolved.source,
        })?;

        let proxy_env = ProxyEnv::from_profile(proxy_profile);

        let mut args = Vec::new();
        if agent.kind == AgentKind::Electron {
            args.extend(proxy_env.electron_args.clone());
        }
        args.extend(agent.extra_args.clone());
        args.extend(opts.extra_args.clone());

        let mut env_vars = proxy_env.env_vars;
        for (k, v) in &agent.extra_env {
            env_vars.insert(k.clone(), v.clone());
        }

        let working_dir = opts
            .working_dir_override
            .clone()
            .or_else(|| agent.working_dir.clone());

        Ok(LaunchPlan {
            agent_id: agent.id.clone(),
            agent_name: agent.name.clone(),
            kind: agent.kind,
            proxy_id: proxy_id.to_string(),
            proxy_url: proxy_profile.url.clone(),
            executable,
            args,
            env_vars,
            unset_vars: proxy_env.unset_vars,
            working_dir,
            terminal: agent.terminal,
            mode: opts.mode,
        })
    }

    /// Finds processes whose executable resolves to the same path.
    pub fn find_running_instances(executable: &Path) -> Vec<RunningInstance> {
        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::All, true);

        matching_processes(&sys, executable)
            .map(|process| RunningInstance {
                pid: process.pid().as_u32(),
                name: process.name().to_string_lossy().into_owned(),
                exe: process.exe().map(Path::to_path_buf),
            })
            .collect()
    }

    /// Terminates running processes matching `executable` and returns killed PIDs.
    pub fn kill_running_instances(executable: &Path) -> Vec<u32> {
        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::All, true);

        matching_processes(&sys, executable)
            .filter(|process| process.kill())
            .map(|process| process.pid().as_u32())
            .collect()
    }

    /// Executes the full launch flow (proxy check, single-instance check, process spawn).
    pub fn launch(
        config: &AppConfig,
        agent: &AgentConfig,
        opts: &LaunchOptions,
    ) -> Result<LaunchResult> {
        let (_, proxy_profile) =
            config.resolve_proxy_for(agent, opts.proxy_override.as_deref())?;

        if config.check_proxy_before_launch && !opts.skip_proxy_check && !proxy_profile.is_direct()
        {
            let check = check_proxy_reachable(proxy_profile, Duration::from_millis(250));
            if !check.reachable {
                return Err(CoreError::ProxyUnreachable {
                    url: proxy_profile.url.clone(),
                    reason: check.detail,
                });
            }
        }

        let plan = Self::build_plan(config, agent, opts)?;

        // Electron apps enforce a single-instance lock; warn or restart if already running.
        let mut killed_pids = Vec::new();
        if plan.kind == AgentKind::Electron {
            let running = Self::find_running_instances(&plan.executable);
            if !running.is_empty() {
                if opts.force_restart {
                    killed_pids = Self::kill_running_instances(&plan.executable);
                    std::thread::sleep(Duration::from_millis(300));
                } else if config.warn_if_already_running {
                    return Err(CoreError::AlreadyRunning {
                        id: plan.agent_id,
                        count: running.len(),
                    });
                }
            }
        }

        let pid = Self::spawn_plan(&plan)?;
        Ok(LaunchResult {
            pid,
            plan,
            killed_pids,
        })
    }

    fn spawn_plan(plan: &LaunchPlan) -> Result<Option<u32>> {
        match (plan.kind, plan.mode) {
            (AgentKind::Cli, LaunchMode::InheritTerminal) => spawn_cli_inherit(plan),
            (AgentKind::Cli, LaunchMode::SpawnTerminal) => spawn_cli_in_terminal(plan),
            _ => spawn_detached_gui(plan),
        }
    }
}

fn matching_processes<'a>(
    system: &'a System,
    executable: &Path,
) -> impl Iterator<Item = &'a sysinfo::Process> {
    let target = executable.canonicalize().ok();
    system.processes().values().filter(move |process| {
        target.as_ref().is_some_and(|path| {
            process.exe().and_then(|exe| exe.canonicalize().ok()).as_ref() == Some(path)
        })
    })
}

fn apply_env(cmd: &mut Command, plan: &LaunchPlan) {
    for key in &plan.unset_vars {
        cmd.env_remove(key);
    }
    for (k, v) in &plan.env_vars {
        cmd.env(k, v);
    }
    if let Some(dir) = &plan.working_dir {
        cmd.current_dir(dir);
    }
}

fn spawn_detached_gui(plan: &LaunchPlan) -> Result<Option<u32>> {
    #[cfg(target_os = "macos")]
    if plan.working_dir.is_none()
        && let Some(bundle) = macos_bundle_for_executable(&plan.executable)
    {
        // LaunchServices starts the bundle with its own macOS application identity.
        // Wait only for open to deliver the launch request, never for the app to exit.
        let mut cmd = Command::new("/usr/bin/open");
        for key in &plan.unset_vars {
            if !plan.env_vars.contains_key(key) {
                cmd.arg("--env").arg(format!("{key}="));
            }
        }
        for (key, value) in &plan.env_vars {
            cmd.arg("--env").arg(format!("{key}={value}"));
        }
        cmd.arg("-n").arg(bundle).arg("--args").args(&plan.args);
        let output = cmd.output().map_err(|source| CoreError::LaunchFailed {
            path: plan.executable.clone(),
            source,
        })?;
        if !output.status.success() {
            return Err(CoreError::LaunchFailed {
                path: plan.executable.clone(),
                source: std::io::Error::other(String::from_utf8_lossy(&output.stderr).trim()),
            });
        }
        return Ok(None);
    }

    let mut cmd = Command::new(&plan.executable);
    cmd.args(&plan.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    apply_env(&mut cmd, plan);

    let child = cmd.spawn().map_err(|source| CoreError::LaunchFailed {
        path: plan.executable.clone(),
        source,
    })?;
    Ok(Some(child.id()))
}

#[cfg(target_os = "macos")]
fn macos_bundle_for_executable(executable: &Path) -> Option<PathBuf> {
    let executable = executable.canonicalize().ok()?;
    let bundle = executable.ancestors().nth(3)?;
    if bundle.extension()? != "app" {
        return None;
    }
    let contents = bundle.join("Contents");
    let info = plist::Value::from_file(contents.join("Info.plist")).ok()?;
    let name = info.as_dictionary()?.get("CFBundleExecutable")?.as_string()?;
    (executable == contents.join("MacOS").join(name)).then(|| bundle.to_path_buf())
}

fn spawn_cli_inherit(plan: &LaunchPlan) -> Result<Option<u32>> {
    let mut cmd = build_cli_command(&plan.executable, &plan.args);
    apply_env(&mut cmd, plan);

    let mut child = cmd.spawn().map_err(|source| CoreError::LaunchFailed {
        path: plan.executable.clone(),
        source,
    })?;
    let pid = child.id();
    let status = child.wait()?;
    if !status.success() {
        #[cfg(unix)]
        let code = status.code().unwrap_or_else(|| {
            use std::os::unix::process::ExitStatusExt;
            128 + status.signal().unwrap_or_default()
        });
        #[cfg(not(unix))]
        let code = status.code().unwrap_or(1);
        return Err(CoreError::ChildExit(code));
    }
    Ok(Some(pid))
}

/// Builds a `Command` that handles `.cmd`/`.bat`/`.ps1` wrappers cleanly on Windows.
fn build_cli_command(exe: &Path, args: &[String]) -> Command {
    #[cfg(windows)]
    {
        let ext = exe
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext == "ps1" {
            let mut cmd = Command::new("powershell");
            cmd.args(["-ExecutionPolicy", "Bypass", "-File"])
                .arg(exe)
                .args(args);
            return cmd;
        }
        if ext == "cmd" || ext == "bat" {
            let mut cmd = Command::new("cmd");
            cmd.arg("/C").arg(exe).args(args);
            return cmd;
        }
    }
    let mut cmd = Command::new(exe);
    cmd.args(args);
    cmd
}

#[cfg(windows)]
fn spawn_cli_in_terminal(plan: &LaunchPlan) -> Result<Option<u32>> {
    let use_wt = matches!(
        plan.terminal,
        TerminalKind::Auto | TerminalKind::WindowsTerminal
    ) && which::which("wt.exe").is_ok();

    let mut cmd = if use_wt {
        let mut c = Command::new("wt.exe");
        if let Some(dir) = &plan.working_dir {
            c.arg("-d").arg(dir);
        }
        c.arg("--title").arg(&plan.agent_name);
        c.arg("--");
        append_win_shell_invocation(&mut c, &plan.executable, &plan.args);
        c
    } else if matches!(plan.terminal, TerminalKind::Cmd) {
        let mut c = Command::new("cmd.exe");
        c.args(["/K"]);
        c.arg(&plan.executable).args(&plan.args);
        c
    } else {
        let mut c = Command::new("powershell.exe");
        c.args(["-NoExit", "-Command"]);
        let joined = format!(
            "& '{}' {}",
            plan.executable.display(),
            plan.args.join(" ")
        );
        c.arg(joined);
        c
    };

    apply_env(&mut cmd, plan);
    let child = cmd.spawn().map_err(|source| CoreError::LaunchFailed {
        path: plan.executable.clone(),
        source,
    })?;
    Ok(Some(child.id()))
}

#[cfg(windows)]
fn append_win_shell_invocation(cmd: &mut Command, exe: &Path, args: &[String]) {
    let ext = exe
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "ps1" {
        cmd.args(["powershell.exe", "-NoExit", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(exe)
            .args(args);
    } else if ext == "cmd" || ext == "bat" {
        cmd.args(["cmd.exe", "/K"]).arg(exe).args(args);
    } else {
        cmd.arg(exe).args(args);
    }
}

#[cfg(target_os = "macos")]
fn spawn_cli_in_terminal(plan: &LaunchPlan) -> Result<Option<u32>> {
    // Build an inline export + exec shell command so Terminal.app inherits the proxy env vars.
    let mut parts = Vec::new();
    if let Some(dir) = &plan.working_dir {
        parts.push(format!("cd {}", shell_quote(&dir.display().to_string())));
    }
    for key in &plan.unset_vars {
        parts.push(format!("unset {key}"));
    }
    for (k, v) in &plan.env_vars {
        parts.push(format!("export {k}={}", shell_quote(v)));
    }
    let mut run_cmd = shell_quote(&plan.executable.display().to_string());
    for arg in &plan.args {
        run_cmd.push(' ');
        run_cmd.push_str(&shell_quote(arg));
    }
    parts.push(run_cmd);
    let full_cmd = parts.join("; ");

    let escaped = full_cmd.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!("tell application \"Terminal\" to do script \"{escaped}\"");
    let child = Command::new("osascript")
        .args(["-e", &script])
        .spawn()
        .map_err(|source| CoreError::LaunchFailed {
            path: PathBuf::from("osascript"),
            source,
        })?;
    Ok(Some(child.id()))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn spawn_cli_in_terminal(plan: &LaunchPlan) -> Result<Option<u32>> {
    spawn_cli_inherit(plan)
}

#[cfg(target_os = "macos")]
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
