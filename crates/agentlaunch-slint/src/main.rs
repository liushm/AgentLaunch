#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use agentlaunch_core::{
    AgentKind, AppConfig, DiscoveryEngine, LaunchMode, LaunchOptions, Launcher, ProxyCheckResult,
    ProxyProfile, ResolvedAgent, check_proxy_reachable, create_desktop_shortcut,
};
use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel};
use agentlaunch_core::config::Appearance;
use slint::language::ColorScheme;
use slint::winit_030::{WinitWindowAccessor, winit::window::Theme};

slint::include_modules!();

struct AppState {
    config: AppConfig,
    config_path: PathBuf,
    resolved_agents: Vec<ResolvedAgent>,
    running_counts: BTreeMap<String, usize>,
    proxy_checks: BTreeMap<String, ProxyCheckResult>,
    proxy_keys: Vec<String>,
    expanded_ids: BTreeSet<String>,
}

impl AppState {
    fn new() -> (Self, Option<(bool, String)>) {
        let (config, config_path, toast) = match AppConfig::load_or_create_default() {
            Ok((cfg, path)) => (cfg, path, None),
            Err(e) => (
                AppConfig::default(),
                PathBuf::from("config.toml"),
                Some((true, format!("读取配置失败，已使用默认配置: {e}"))),
            ),
        };
        let mut state = Self {
            config,
            config_path,
            resolved_agents: Vec::new(),
            running_counts: BTreeMap::new(),
            proxy_checks: BTreeMap::new(),
            proxy_keys: Vec::new(),
            expanded_ids: BTreeSet::new(),
        };
        state.refresh_all();
        (state, toast)
    }

    fn refresh_all(&mut self) {
        self.resolved_agents = DiscoveryEngine::resolve_all(&self.config.agents);
        self.refresh_running();
        self.refresh_proxies();
    }

    fn refresh_running(&mut self) {
        self.running_counts.clear();
        for a in &self.resolved_agents {
            if a.kind == AgentKind::Electron
                && let Some(exe) = &a.executable
            {
                let count = Launcher::find_running_instances(exe).len();
                if count > 0 {
                    self.running_counts.insert(a.id.clone(), count);
                }
            }
        }
    }

    fn refresh_proxies(&mut self) {
        self.proxy_keys = self.config.proxies.keys().cloned().collect();
        self.proxy_checks.clear();
        for (id, profile) in &self.config.proxies {
            let check = check_proxy_reachable(profile, Duration::from_millis(180));
            self.proxy_checks.insert(id.clone(), check);
        }
    }

    fn save(&self) -> Result<(), String> {
        self.config
            .save_to(&self.config_path)
            .map_err(|e| format!("保存配置失败: {e}"))
    }

    fn sync_to_ui(&self, ui: &AppWindow) {
        // Proxy choices for agent dropdown: index 0 = "跟随默认", 1.. = proxy profiles
        let mut choices: Vec<SharedString> = Vec::with_capacity(self.proxy_keys.len() + 1);
        let default_name = self
            .config
            .proxies
            .get(&self.config.default_proxy)
            .map(|p| p.name.as_str())
            .unwrap_or(&self.config.default_proxy);
        choices.push(SharedString::from(format!("默认 ({default_name})")));
        for key in &self.proxy_keys {
            let name = self
                .config
                .proxies
                .get(key)
                .map(|p| p.name.as_str())
                .unwrap_or(key.as_str());
            choices.push(SharedString::from(name));
        }
        ui.set_agent_proxy_choices(ModelRc::new(VecModel::from(choices)));

        // Top bar default proxy badge
        if let Some(check) = self.proxy_checks.get(&self.config.default_proxy) {
            ui.set_default_proxy_ok(check.reachable);
            if check.reachable {
                ui.set_default_proxy_label(SharedString::from(format!(
                    "{default_name} · {}ms",
                    check.latency_ms.unwrap_or(0)
                )));
            } else {
                ui.set_default_proxy_label(SharedString::from("默认代理未连通"));
            }
        }

        ui.set_check_proxy_before_launch(self.config.check_proxy_before_launch);
        ui.set_config_path(SharedString::from(self.config_path.display().to_string()));

        // Agent rows
        let mut agent_items = Vec::with_capacity(self.resolved_agents.len());
        let mut uninstalled_count = 0i32;

        for (idx, resolved) in self.resolved_agents.iter().enumerate() {
            if !resolved.is_installed() {
                uninstalled_count += 1;
            }
            let running = self.running_counts.get(&resolved.id).copied().unwrap_or(0);
            let agent_cfg = &self.config.agents[idx];
            let proxy_index = match &agent_cfg.proxy {
                None => 0i32,
                Some(pid) => self
                    .proxy_keys
                    .iter()
                    .position(|k| k == pid)
                    .map(|p| (p + 1) as i32)
                    .unwrap_or(0),
            };

            let effective_proxy_key = agent_cfg
                .proxy
                .as_deref()
                .unwrap_or(&self.config.default_proxy);
            let effective_proxy_url = self
                .config
                .proxies
                .get(effective_proxy_key)
                .map(|p| {
                    if p.is_direct() {
                        "直连".to_string()
                    } else {
                        p.url.clone()
                    }
                })
                .unwrap_or_else(|| effective_proxy_key.to_string());

            let status_text = if !resolved.is_installed() {
                "未安装".to_string()
            } else if running > 0 {
                format!("运行中 ({running}) · 代理 {effective_proxy_url}")
            } else {
                let kind_str = match resolved.kind {
                    AgentKind::Electron => "Electron",
                    AgentKind::Cli => "CLI",
                    AgentKind::Generic => "App",
                };
                format!("就绪 · {kind_str} · {effective_proxy_url}")
            };

            let (glyph, badge_bg, badge_fg) = badge_style(&resolved.id);
            let executable_path = resolved
                .executable
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "未找到可执行文件".to_string());

            agent_items.push(AgentItem {
                id: SharedString::from(&resolved.id),
                name: SharedString::from(&resolved.name),
                glyph: SharedString::from(glyph),
                badge_bg,
                badge_fg,
                version: SharedString::from(resolved.version.as_deref().unwrap_or("")),
                installed: resolved.is_installed(),
                running_count: running as i32,
                is_electron: resolved.kind == AgentKind::Electron,
                status_text: SharedString::from(status_text),
                executable_path: SharedString::from(executable_path),
                source: SharedString::from(&resolved.source),
                cli_command: SharedString::from(format!(
                    "al run {} --proxy {}",
                    resolved.id, effective_proxy_key
                )),
                proxy_index,
                expanded: self.expanded_ids.contains(&resolved.id),
            });
        }

        // Keep installed agents first so row dividers are always clean when uninstalled ones are collapsed
        agent_items.sort_by_key(|a| !a.installed);

        ui.set_uninstalled_count(uninstalled_count);
        ui.set_agents(ModelRc::new(VecModel::from(agent_items)));

        // Proxy rows
        let proxy_items: Vec<ProxyItem> = self
            .proxy_keys
            .iter()
            .filter_map(|key| {
                let profile = self.config.proxies.get(key)?;
                let check = self.proxy_checks.get(key);
                let is_default = self.config.default_proxy == *key;
                Some(ProxyItem {
                    id: SharedString::from(key),
                    name: SharedString::from(&profile.name),
                    url: SharedString::from(&profile.url),
                    bypass: SharedString::from(profile.bypass.join(";")),
                    is_default,
                    reachable: check.map(|c| c.reachable).unwrap_or(false),
                    status_detail: SharedString::from(
                        check.map(|c| c.detail.as_str()).unwrap_or(""),
                    ),
                    deletable: !is_default && key != "direct",
                })
            })
            .collect();
        ui.set_proxies(ModelRc::new(VecModel::from(proxy_items)));
    }
}

fn badge_style(agent_id: &str) -> (&'static str, Color, Color) {
    match agent_id {
        "antigravity" => (
            "A",
            Color::from_rgb_u8(30, 42, 64),
            Color::from_rgb_u8(96, 165, 250),
        ),
        "codex" | "codex-cli" => (
            "C",
            Color::from_rgb_u8(26, 48, 42),
            Color::from_rgb_u8(52, 211, 153),
        ),
        "claude-desktop" | "claude-code" => (
            "Cl",
            Color::from_rgb_u8(56, 38, 28),
            Color::from_rgb_u8(251, 146, 60),
        ),
        "cursor" => (
            "Cu",
            Color::from_rgb_u8(44, 36, 58),
            Color::from_rgb_u8(192, 132, 252),
        ),
        "vscode" => (
            "VS",
            Color::from_rgb_u8(24, 42, 62),
            Color::from_rgb_u8(56, 189, 248),
        ),
        _ => (
            "Ag",
            Color::from_rgb_u8(38, 38, 42),
            Color::from_rgb_u8(236, 236, 239),
        ),
    }
}

/// Supports silent headless launch when invoked from a desktop shortcut:
/// `agentlaunch-slint.exe --launch <agent_id> [--force-restart]`
/// Returns `Some(exit_code)` if handled in headless mode.
fn try_silent_launch() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let launch_pos = args.iter().position(|a| a == "--launch")?;
    let agent_id = args.get(launch_pos + 1)?;
    let force_restart = args.iter().any(|a| a == "--force-restart");

    let (config, _) = AppConfig::load_or_create_default().ok()?;
    let agent = config.find_agent(agent_id)?;
    let opts = LaunchOptions {
        mode: LaunchMode::SpawnTerminal,
        force_restart,
        ..Default::default()
    };
    match Launcher::launch(&config, agent, &opts) {
        Ok(_) => Some(0),
        Err(_) => Some(1),
    }
}

fn set_toast(ui: &AppWindow, is_error: bool, text: impl Into<SharedString>) {
    ui.set_toast_is_error(is_error);
    ui.set_toast_text(text.into());
}

fn apply_appearance(ui: &AppWindow, appearance: Appearance) {
    let (index, scheme, theme) = match appearance {
        Appearance::System => (0, ColorScheme::Unknown, None),
        Appearance::Light => (1, ColorScheme::Light, Some(Theme::Light)),
        Appearance::Dark => (2, ColorScheme::Dark, Some(Theme::Dark)),
    };
    ui.set_appearance_index(index);
    ui.set_widget_scheme(scheme);
    ui.window().with_winit_window(|window| window.set_theme(theme));
}

fn main() -> Result<(), slint::PlatformError> {
    if let Some(code) = try_silent_launch() {
        std::process::exit(code);
    }

    #[cfg(target_os = "macos")]
    slint::BackendSelector::new()
        .with_winit_window_attributes_hook(|attributes| attributes.with_transparent(false))
        .select()?;

    let ui = AppWindow::new()?;
    let (state_inner, init_toast) = AppState::new();
    let state = Rc::new(RefCell::new(state_inner));

    state.borrow().sync_to_ui(&ui);
    if let Some((is_err, msg)) = init_toast {
        set_toast(&ui, is_err, msg);
    }

    // refresh-all
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_refresh_all(move || {
            let mut st = state.borrow_mut();
            st.refresh_all();
            if let Some(ui) = ui_weak.upgrade() {
                st.sync_to_ui(&ui);
                set_toast(&ui, false, "已刷新 Agent 路径与代理连通性");
            }
        });
    }

    // launch-agent
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_launch_agent(move |agent_id, force_restart| {
            let mut st = state.borrow_mut();
            let Some(agent_cfg) = st.config.find_agent(agent_id.as_str()).cloned() else {
                return;
            };
            let opts = LaunchOptions {
                mode: LaunchMode::SpawnTerminal,
                force_restart,
                ..Default::default()
            };
            let result = Launcher::launch(&st.config, &agent_cfg, &opts);
            st.refresh_running();
            if let Some(ui) = ui_weak.upgrade() {
                st.sync_to_ui(&ui);
                match result {
                    Ok(res) => {
                        let proxy_desc = if res.plan.proxy_url.is_empty() {
                            "直连".to_string()
                        } else {
                            res.plan.proxy_url
                        };
                        let verb = if !res.killed_pids.is_empty() {
                            "已重启并代理"
                        } else {
                            "已启动"
                        };
                        set_toast(
                            &ui,
                            false,
                            format!("{verb} {} · {} ({proxy_desc})", res.plan.agent_name, res.plan.proxy_id),
                        );
                    }
                    Err(e) => set_toast(&ui, true, format!("启动失败: {e}")),
                }
            }
        });
    }

    // select-agent-proxy
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_select_agent_proxy(move |agent_id, idx| {
            let mut st = state.borrow_mut();
            let new_proxy = if idx <= 0 {
                None
            } else {
                st.proxy_keys.get((idx - 1) as usize).cloned()
            };
            if let Some(agent) = st.config.find_agent_mut(agent_id.as_str()) {
                agent.proxy = new_proxy;
            }
            let save_res = st.save();
            if let Some(ui) = ui_weak.upgrade() {
                st.sync_to_ui(&ui);
                if let Err(e) = save_res {
                    set_toast(&ui, true, e);
                }
            }
        });
    }

    // toggle-agent-details
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_toggle_agent_details(move |agent_id| {
            let mut st = state.borrow_mut();
            let id = agent_id.to_string();
            if !st.expanded_ids.remove(&id) {
                st.expanded_ids.insert(id);
            }
            if let Some(ui) = ui_weak.upgrade() {
                st.sync_to_ui(&ui);
            }
        });
    }

    // create-shortcut
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_create_shortcut(move |agent_id| {
            let st = state.borrow();
            let Some(agent_cfg) = st.config.find_agent(agent_id.as_str()) else {
                return;
            };
            let current_exe =
                std::env::current_exe().unwrap_or_else(|_| PathBuf::from("agentlaunch-slint"));
            let res = create_desktop_shortcut(agent_cfg, &current_exe);
            if let Some(ui) = ui_weak.upgrade() {
                match res {
                    Ok(s) => set_toast(
                        &ui,
                        false,
                        format!("已创建桌面快捷方式: {}", s.shortcut_path.display()),
                    ),
                    Err(e) => set_toast(&ui, true, format!("创建快捷方式失败: {e}")),
                }
            }
        });
    }

    // test-proxies
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_test_proxies(move || {
            let mut st = state.borrow_mut();
            st.refresh_proxies();
            if let Some(ui) = ui_weak.upgrade() {
                st.sync_to_ui(&ui);
                set_toast(&ui, false, "已完成全部代理节点连通性测试");
            }
        });
    }

    // set-default-proxy
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_set_default_proxy(move |proxy_id| {
            let mut st = state.borrow_mut();
            st.config.default_proxy = proxy_id.to_string();
            let save_res = st.save();
            if let Some(ui) = ui_weak.upgrade() {
                st.sync_to_ui(&ui);
                match save_res {
                    Ok(()) => set_toast(&ui, false, format!("已将默认代理切换为 {proxy_id}")),
                    Err(e) => set_toast(&ui, true, e),
                }
            }
        });
    }

    // update-proxy
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_update_proxy(move |id, name, url, bypass_str| {
            let mut st = state.borrow_mut();
            if let Some(profile) = st.config.proxies.get_mut(id.as_str()) {
                profile.name = name.trim().to_string();
                profile.url = url.trim().to_string();
                profile.bypass = bypass_str
                    .split([';', ','])
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            let save_res = st.save();
            st.refresh_proxies();
            if let Some(ui) = ui_weak.upgrade() {
                st.sync_to_ui(&ui);
                match save_res {
                    Ok(()) => set_toast(&ui, false, format!("已保存代理节点 {id}")),
                    Err(e) => set_toast(&ui, true, e),
                }
            }
        });
    }

    // delete-proxy
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_delete_proxy(move |id| {
            let mut st = state.borrow_mut();
            st.config.proxies.remove(id.as_str());
            let save_res = st.save();
            st.refresh_proxies();
            if let Some(ui) = ui_weak.upgrade() {
                st.sync_to_ui(&ui);
                match save_res {
                    Ok(()) => set_toast(&ui, false, format!("已删除代理节点 {id}")),
                    Err(e) => set_toast(&ui, true, e),
                }
            }
        });
    }

    // add-proxy
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_add_proxy(move |id, name, url, bypass_str| {
            let id_str = id.trim().to_string();
            if id_str.is_empty() {
                if let Some(ui) = ui_weak.upgrade() {
                    set_toast(&ui, true, "代理 ID 不能为空");
                }
                return;
            }
            let name_str = if name.trim().is_empty() {
                id_str.clone()
            } else {
                name.trim().to_string()
            };
            let bypass = bypass_str
                .split([';', ','])
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            let mut st = state.borrow_mut();
            st.config.proxies.insert(
                id_str.clone(),
                ProxyProfile {
                    name: name_str,
                    url: url.trim().to_string(),
                    bypass,
                },
            );
            let save_res = st.save();
            st.refresh_proxies();
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_new_proxy_id(SharedString::default());
                ui.set_new_proxy_name(SharedString::default());
                ui.set_show_add_proxy(false);
                st.sync_to_ui(&ui);
                match save_res {
                    Ok(()) => set_toast(&ui, false, format!("已添加代理节点 {id_str}")),
                    Err(e) => set_toast(&ui, true, e),
                }
            }
        });
    }

    // set-check-proxy-before-launch
    {
        let state = state.clone();
        let ui_weak = ui.as_weak();
        ui.on_set_check_proxy_before_launch(move |enabled| {
            let mut st = state.borrow_mut();
            st.config.check_proxy_before_launch = enabled;
            let save_res = st.save();
            if let Some(ui) = ui_weak.upgrade()
                && let Err(e) = save_res
            {
                set_toast(&ui, true, e);
            }
        });
    }

    ui.show()?;
    apply_appearance(&ui, state.borrow().config.appearance);
    let weak = ui.as_weak();
    ui.on_choose_appearance(move |index| {
        let Some(ui) = weak.upgrade() else { return };
        let mut state = state.borrow_mut();
        let previous = state.config.appearance;
        state.config.appearance = match index {
            1 => Appearance::Light,
            2 => Appearance::Dark,
            _ => Appearance::System,
        };
        if let Err(error) = state.save() {
            state.config.appearance = previous;
            set_toast(&ui, true, error);
        }
        apply_appearance(&ui, state.config.appearance);
    });
    slint::run_event_loop()
}
