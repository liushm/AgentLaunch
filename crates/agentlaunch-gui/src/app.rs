use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Duration;

use agentlaunch_core::{
    AgentKind, AppConfig, DiscoveryEngine, LaunchMode, LaunchOptions, Launcher, ProxyCheckResult,
    ProxyProfile, ResolvedAgent, check_proxy_reachable, config::default_bypass,
};
use egui::{Color32, CornerRadius, Margin, RichText, Stroke, Vec2};

use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveTab {
    Agents,
    Settings,
}

#[derive(Debug, Clone)]
struct StatusToast {
    is_error: bool,
    text: String,
}

pub struct AgentLaunchApp {
    config: AppConfig,
    config_path: PathBuf,
    resolved_agents: Vec<ResolvedAgent>,
    running_counts: BTreeMap<String, usize>,
    proxy_checks: BTreeMap<String, ProxyCheckResult>,

    active_tab: ActiveTab,
    show_uninstalled: bool,
    expanded_details: BTreeSet<String>,
    show_add_proxy: bool,
    toast: Option<StatusToast>,

    // New proxy inline form
    new_proxy_id: String,
    new_proxy_name: String,
    new_proxy_url: String,
    new_proxy_bypass: String,
}

impl AgentLaunchApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::fonts::configure_system_fonts(&cc.egui_ctx);
        theme::apply_magpie_theme(&cc.egui_ctx);

        let (config, config_path, init_toast) = match AppConfig::load_or_create_default() {
            Ok((cfg, path)) => (cfg, path, None),
            Err(e) => (
                AppConfig::default(),
                PathBuf::from("config.toml"),
                Some(StatusToast {
                    is_error: true,
                    text: format!("读取配置失败，已回退默认配置: {e}"),
                }),
            ),
        };

        let mut app = Self {
            config,
            config_path,
            resolved_agents: Vec::new(),
            running_counts: BTreeMap::new(),
            proxy_checks: BTreeMap::new(),
            active_tab: ActiveTab::Agents,
            show_uninstalled: false,
            expanded_details: BTreeSet::new(),
            show_add_proxy: false,
            toast: init_toast,
            new_proxy_id: String::new(),
            new_proxy_name: String::new(),
            new_proxy_url: "http://127.0.0.1:7890".to_string(),
            new_proxy_bypass: default_bypass().join(";"),
        };

        app.refresh_all();
        app
    }

    fn refresh_all(&mut self) {
        self.resolved_agents = DiscoveryEngine::resolve_all(&self.config.agents);
        self.refresh_running_status();
        self.refresh_proxy_checks();
    }

    fn refresh_running_status(&mut self) {
        self.running_counts.clear();
        for agent in &self.resolved_agents {
            if agent.kind == AgentKind::Electron
                && let Some(exe) = &agent.executable
            {
                let count = Launcher::find_running_instances(exe).len();
                if count > 0 {
                    self.running_counts.insert(agent.id.clone(), count);
                }
            }
        }
    }

    fn refresh_proxy_checks(&mut self) {
        self.proxy_checks.clear();
        for (id, profile) in &self.config.proxies {
            let res = check_proxy_reachable(profile, Duration::from_millis(180));
            self.proxy_checks.insert(id.clone(), res);
        }
    }

    fn save_config(&mut self) {
        if let Err(e) = self.config.save_to(&self.config_path) {
            self.toast = Some(StatusToast {
                is_error: true,
                text: format!("保存配置失败: {e}"),
            });
        }
    }

    fn launch_agent(&mut self, agent_id: &str, force_restart: bool) {
        let Some(agent_cfg) = self.config.find_agent(agent_id).cloned() else {
            return;
        };

        let opts = LaunchOptions {
            proxy_override: None,
            extra_args: Vec::new(),
            working_dir_override: None,
            mode: LaunchMode::SpawnTerminal,
            force_restart,
            skip_proxy_check: false,
        };

        match Launcher::launch(&self.config, &agent_cfg, &opts) {
            Ok(res) => {
                let proxy_desc = if res.plan.proxy_url.is_empty() {
                    "直连".to_string()
                } else {
                    res.plan.proxy_url.clone()
                };
                let verb = if !res.killed_pids.is_empty() {
                    "已重启并代理"
                } else {
                    "已启动"
                };
                self.toast = Some(StatusToast {
                    is_error: false,
                    text: format!(
                        "{verb} {}  ·  {} ({})",
                        res.plan.agent_name, res.plan.proxy_id, proxy_desc
                    ),
                });
                self.refresh_running_status();
            }
            Err(e) => {
                self.toast = Some(StatusToast {
                    is_error: true,
                    text: format!("启动失败: {e}"),
                });
            }
        }
    }

    fn render_top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // Left: minimalist brand
            ui.label(
                RichText::new("AgentLaunch")
                    .size(14.5)
                    .strong()
                    .color(theme::TEXT_PRIMARY),
            );

            ui.add_space(20.0);

            // Center-left: Magpie-style pill navigation tabs
            if theme::nav_tab(ui, "Agent", self.active_tab == ActiveTab::Agents).clicked() {
                self.active_tab = ActiveTab::Agents;
            }
            if theme::nav_tab(ui, "设置", self.active_tab == ActiveTab::Settings).clicked() {
                self.active_tab = ActiveTab::Settings;
            }

            // Right: compact proxy indicator + refresh
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let refresh_btn = egui::Button::new(
                    RichText::new("刷新").size(12.5).color(theme::TEXT_SECONDARY),
                )
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::NONE)
                .corner_radius(CornerRadius::same(6));
                if ui.add(refresh_btn).on_hover_text("重新扫描本机 Agent 与代理端口").clicked() {
                    self.refresh_all();
                    self.toast = Some(StatusToast {
                        is_error: false,
                        text: "已刷新 Agent 路径与代理连通性".to_string(),
                    });
                }

                if let Some(check) = self.proxy_checks.get(&self.config.default_proxy) {
                    let (dot_color, text) = if check.reachable {
                        let proxy_name = self
                            .config
                            .proxies
                            .get(&self.config.default_proxy)
                            .map(|p| p.name.as_str())
                            .unwrap_or(&self.config.default_proxy);
                        (
                            theme::STATUS_GREEN,
                            format!("● {proxy_name} · {}ms", check.latency_ms.unwrap_or(0)),
                        )
                    } else {
                        (theme::STATUS_RED, "● 默认代理未连通".to_string())
                    };
                    ui.label(RichText::new(text).size(12.0).color(dot_color))
                        .on_hover_text(&check.detail);
                }
            });
        });
    }

    fn render_agents_tab(&mut self, ui: &mut egui::Ui) {
        let mut config_changed = false;
        let mut launch_request: Option<(String, bool)> = None;

        // Subtle intro line (Magpie style)
        ui.label(
            RichText::new("为每个 Agent 指定独立代理，启动时自动解析最新版本路径并注入参数与环境变量。")
                .size(12.5)
                .color(theme::TEXT_SECONDARY),
        );
        ui.add_space(12.0);

        let installed_indices: Vec<usize> = self
            .resolved_agents
            .iter()
            .enumerate()
            .filter(|(_, a)| a.is_installed())
            .map(|(i, _)| i)
            .collect();
        let uninstalled_indices: Vec<usize> = self
            .resolved_agents
            .iter()
            .enumerate()
            .filter(|(_, a)| !a.is_installed())
            .map(|(i, _)| i)
            .collect();

        let proxy_options: Vec<(String, String)> = self
            .config
            .proxies
            .iter()
            .map(|(k, v)| (k.clone(), v.name.clone()))
            .collect();

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // Unified rounded card container (Magpie style)
                theme::card_frame().show(ui, |ui| {
                    let mut row_count = 0;

                    for &idx in &installed_indices {
                        if row_count > 0 {
                            theme::card_divider(ui);
                        }
                        self.render_agent_row(
                            ui,
                            idx,
                            &proxy_options,
                            &mut config_changed,
                            &mut launch_request,
                        );
                        row_count += 1;
                    }

                    if self.show_uninstalled {
                        for &idx in &uninstalled_indices {
                            if row_count > 0 {
                                theme::card_divider(ui);
                            }
                            self.render_agent_row(
                                ui,
                                idx,
                                &proxy_options,
                                &mut config_changed,
                                &mut launch_request,
                            );
                            row_count += 1;
                        }
                    }

                    if row_count == 0 {
                        egui::Frame::new()
                            .inner_margin(Margin::symmetric(16, 20))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new("未检测到已安装的内置 Agent。")
                                        .color(theme::TEXT_SECONDARY),
                                );
                            });
                    }

                    // Bottom expand/collapse footer inside the card (Magpie "显示其余 N 个")
                    if !uninstalled_indices.is_empty() {
                        theme::card_divider(ui);
                        egui::Frame::new()
                            .inner_margin(Margin::symmetric(16, 8))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.with_layout(
                                        egui::Layout::centered_and_justified(
                                            egui::Direction::LeftToRight,
                                        ),
                                        |ui| {
                                            let label = if self.show_uninstalled {
                                                "收起未安装 ▴".to_string()
                                            } else {
                                                format!(
                                                    "显示其余 {} 个 ▾",
                                                    uninstalled_indices.len()
                                                )
                                            };
                                            let btn = egui::Button::new(
                                                RichText::new(label)
                                                    .size(12.0)
                                                    .color(theme::TEXT_MUTED),
                                            )
                                            .fill(Color32::TRANSPARENT)
                                            .stroke(Stroke::NONE);
                                            if ui.add(btn).clicked() {
                                                self.show_uninstalled = !self.show_uninstalled;
                                            }
                                        },
                                    );
                                });
                            });
                    }
                });
            });

        if config_changed {
            self.save_config();
        }
        if let Some((id, force)) = launch_request {
            self.launch_agent(&id, force);
        }
    }

    fn render_agent_row(
        &mut self,
        ui: &mut egui::Ui,
        idx: usize,
        proxy_options: &[(String, String)],
        config_changed: &mut bool,
        launch_request: &mut Option<(String, bool)>,
    ) {
        let resolved = self.resolved_agents[idx].clone();
        let running = self.running_counts.get(&resolved.id).copied().unwrap_or(0);
        let is_expanded = self.expanded_details.contains(&resolved.id);

        let effective_proxy_key = self
            .config
            .agents
            .get(idx)
            .and_then(|a| a.proxy.as_deref())
            .unwrap_or(&self.config.default_proxy)
            .to_string();
        let effective_proxy_url = self
            .config
            .proxies
            .get(&effective_proxy_key)
            .map(|p| {
                if p.is_direct() {
                    "直连".to_string()
                } else {
                    p.url.clone()
                }
            })
            .unwrap_or_else(|| effective_proxy_key.clone());

        egui::Frame::new()
            .inner_margin(Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Left: icon badge + 2-line title/subtitle
                    theme::agent_icon_badge(ui, &resolved.id);
                    ui.add_space(6.0);

                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(&resolved.name)
                                    .size(14.0)
                                    .strong()
                                    .color(theme::TEXT_PRIMARY),
                            );
                            if let Some(ver) = &resolved.version {
                                ui.label(
                                    RichText::new(ver).size(11.5).color(theme::TEXT_MUTED),
                                );
                            }
                        });

                        // Subtitle line with colored status dot (Magpie style)
                        ui.horizontal(|ui| {
                            if !resolved.is_installed() {
                                ui.label(
                                    RichText::new("● 未安装")
                                        .size(11.5)
                                        .color(theme::TEXT_MUTED),
                                );
                            } else if running > 0 {
                                ui.label(
                                    RichText::new(format!(
                                        "● 运行中 ({running}) · 代理 {effective_proxy_url}"
                                    ))
                                    .size(11.5)
                                    .color(theme::STATUS_AMBER),
                                );
                            } else {
                                let kind_str = match resolved.kind {
                                    AgentKind::Electron => "Electron",
                                    AgentKind::Cli => "CLI",
                                    AgentKind::Generic => "App",
                                };
                                ui.label(
                                    RichText::new(format!(
                                        "● 就绪 · {kind_str} · {effective_proxy_url}"
                                    ))
                                    .size(11.5)
                                    .color(theme::STATUS_GREEN),
                                );
                            }
                        });
                    });

                    // Right: single smart action button + compact proxy selector + 详情 ›
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;

                        if resolved.is_installed() {
                            // Single smart Launch / Restart button!
                            let is_restart = running > 0 && resolved.kind == AgentKind::Electron;
                            let (btn_text, bg, fg, tooltip) = if is_restart {
                                (
                                    "重启",
                                    Color32::from_rgb(56, 46, 26),
                                    theme::STATUS_AMBER,
                                    "当前已有实例运行，点击将终止旧进程并带代理重启",
                                )
                            } else {
                                (
                                    "启动",
                                    theme::TEXT_PRIMARY,
                                    theme::BG_CANVAS,
                                    "带当前所选代理启动",
                                )
                            };

                            let action_btn = egui::Button::new(
                                RichText::new(btn_text).size(12.5).strong().color(fg),
                            )
                            .fill(bg)
                            .stroke(Stroke::NONE)
                            .corner_radius(CornerRadius::same(14))
                            .min_size(Vec2::new(60.0, 28.0));

                            if ui.add(action_btn).on_hover_text(tooltip).clicked() {
                                *launch_request = Some((resolved.id.clone(), is_restart));
                            }
                        }

                        // Compact proxy ComboBox
                        let current_override = self
                            .config
                            .agents
                            .get(idx)
                            .and_then(|a| a.proxy.clone());
                        let selected_label = match &current_override {
                            None => format!(
                                "默认 ({})",
                                self.config
                                    .proxies
                                    .get(&self.config.default_proxy)
                                    .map(|p| p.name.as_str())
                                    .unwrap_or(&self.config.default_proxy)
                            ),
                            Some(pid) => self
                                .config
                                .proxies
                                .get(pid)
                                .map(|p| p.name.clone())
                                .unwrap_or_else(|| pid.clone()),
                        };

                        egui::ComboBox::from_id_salt(format!("proxy_combo_{}", resolved.id))
                            .width(168.0)
                            .selected_text(RichText::new(selected_label).size(12.5))
                            .show_ui(ui, |ui| {
                                if let Some(agent_cfg) = self.config.agents.get_mut(idx) {
                                    if ui
                                        .selectable_value(&mut agent_cfg.proxy, None, "跟随默认")
                                        .changed()
                                    {
                                        *config_changed = true;
                                    }
                                    for (pid, pname) in proxy_options {
                                        if ui
                                            .selectable_value(
                                                &mut agent_cfg.proxy,
                                                Some(pid.clone()),
                                                pname,
                                            )
                                            .changed()
                                        {
                                            *config_changed = true;
                                        }
                                    }
                                }
                            });

                        // Subtle `详情 ›` toggle link (Magpie style)
                        let detail_label = if is_expanded { "收起 ⌄" } else { "详情 ›" };
                        let detail_btn = egui::Button::new(
                            RichText::new(detail_label)
                                .size(12.0)
                                .color(theme::TEXT_MUTED),
                        )
                        .fill(Color32::TRANSPARENT)
                        .stroke(Stroke::NONE);
                        if ui.add(detail_btn).clicked() {
                            if is_expanded {
                                self.expanded_details.remove(&resolved.id);
                            } else {
                                self.expanded_details.insert(resolved.id.clone());
                            }
                        }
                    });
                });

                // Expandable inline details drawer
                if is_expanded {
                    ui.add_space(8.0);
                    egui::Frame::new()
                        .fill(Color32::from_rgb(22, 22, 25))
                        .stroke(Stroke::new(1.0, theme::DIVIDER))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            let path_str = resolved
                                .executable
                                .as_ref()
                                .map(|p| p.display().to_string())
                                .unwrap_or_else(|| "未找到可执行文件".to_string());
                            ui.label(
                                RichText::new(format!("路径: {path_str}"))
                                    .size(11.5)
                                    .color(theme::TEXT_SECONDARY),
                            );
                            ui.label(
                                RichText::new(format!("发现方式: {}", resolved.source))
                                    .size(11.5)
                                    .color(theme::TEXT_MUTED),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "CLI 命令: al run {} --proxy {}",
                                    resolved.id, effective_proxy_key
                                ))
                                .size(11.5)
                                .monospace()
                                .color(theme::ACCENT_BLUE),
                            );
                        });
                }
            });
    }

    fn render_settings_tab(&mut self, ui: &mut egui::Ui) {
        let mut config_changed = false;
        let mut to_delete: Option<String> = None;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // === Section 1: 代理 (Magpie screenshot 2 style) ===
                ui.label(RichText::new("代理").size(12.0).color(theme::TEXT_MUTED));
                ui.add_space(6.0);

                theme::card_frame().show(ui, |ui| {
                    // Header row inside card with right-aligned "添加代理" & "测试连通性"
                    egui::Frame::new()
                        .inner_margin(Margin::symmetric(16, 10))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("代理节点")
                                        .size(13.0)
                                        .color(theme::TEXT_PRIMARY),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let add_btn = egui::Button::new(
                                            RichText::new(if self.show_add_proxy {
                                                "取消"
                                            } else {
                                                "添加代理"
                                            })
                                            .size(12.5)
                                            .color(theme::TEXT_PRIMARY),
                                        )
                                        .fill(Color32::TRANSPARENT)
                                        .stroke(Stroke::NONE);
                                        if ui.add(add_btn).clicked() {
                                            self.show_add_proxy = !self.show_add_proxy;
                                        }

                                        let test_btn = egui::Button::new(
                                            RichText::new("测试连通性")
                                                .size(12.5)
                                                .color(theme::TEXT_SECONDARY),
                                        )
                                        .fill(Color32::TRANSPARENT)
                                        .stroke(Stroke::NONE);
                                        if ui.add(test_btn).clicked() {
                                            self.refresh_proxy_checks();
                                        }
                                    },
                                );
                            });
                        });

                    // Existing proxy rows
                    let keys: Vec<String> = self.config.proxies.keys().cloned().collect();
                    for key in keys {
                        theme::card_divider(ui);
                        let check = self.proxy_checks.get(&key).cloned();
                        let is_default = self.config.default_proxy == key;

                        if let Some(profile) = self.config.proxies.get_mut(&key) {
                            egui::Frame::new()
                                .inner_margin(Margin::symmetric(16, 10))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let dot_color = match &check {
                                            Some(c) if c.reachable => theme::STATUS_GREEN,
                                            Some(_) => theme::STATUS_RED,
                                            None => theme::TEXT_MUTED,
                                        };
                                        ui.label(RichText::new("●").size(10.0).color(dot_color));

                                        ui.add(
                                            egui::TextEdit::singleline(&mut profile.name)
                                                .desired_width(140.0),
                                        )
                                        .changed()
                                        .then(|| config_changed = true);

                                        ui.add(
                                            egui::TextEdit::singleline(&mut profile.url)
                                                .desired_width(210.0)
                                                .hint_text("留空表示直连"),
                                        )
                                        .changed()
                                        .then(|| config_changed = true);

                                        let mut bypass_str = profile.bypass.join(";");
                                        if ui
                                            .add(
                                                egui::TextEdit::singleline(&mut bypass_str)
                                                    .desired_width(150.0)
                                                    .hint_text("绕过列表"),
                                            )
                                            .changed()
                                        {
                                            profile.bypass = bypass_str
                                                .split([';', ','])
                                                .map(|s| s.trim().to_string())
                                                .filter(|s| !s.is_empty())
                                                .collect();
                                            config_changed = true;
                                        }

                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if !is_default && key != "direct" {
                                                    let del = egui::Button::new(
                                                        RichText::new("删除")
                                                            .size(12.0)
                                                            .color(theme::STATUS_RED),
                                                    )
                                                    .fill(Color32::TRANSPARENT)
                                                    .stroke(Stroke::NONE);
                                                    if ui.add(del).clicked() {
                                                        to_delete = Some(key.clone());
                                                    }
                                                }

                                                if is_default {
                                                    ui.label(
                                                        RichText::new("默认")
                                                            .size(12.0)
                                                            .color(theme::STATUS_GREEN),
                                                    );
                                                } else {
                                                    let set_def = egui::Button::new(
                                                        RichText::new("设为默认")
                                                            .size(12.0)
                                                            .color(theme::TEXT_SECONDARY),
                                                    )
                                                    .fill(Color32::TRANSPARENT)
                                                    .stroke(Stroke::NONE);
                                                    if ui.add(set_def).clicked() {
                                                        self.config.default_proxy = key.clone();
                                                        config_changed = true;
                                                    }
                                                }
                                            },
                                        );
                                    });
                                });
                        }
                    }

                    // Inline add proxy row (only shown when user clicks "添加代理")
                    if self.show_add_proxy {
                        theme::card_divider(ui);
                        egui::Frame::new()
                            .fill(Color32::from_rgb(24, 24, 27))
                            .inner_margin(Margin::symmetric(16, 10))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.new_proxy_id)
                                            .desired_width(90.0)
                                            .hint_text("ID (如 v2ray)"),
                                    );
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.new_proxy_name)
                                            .desired_width(130.0)
                                            .hint_text("名称"),
                                    );
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.new_proxy_url)
                                            .desired_width(200.0)
                                            .hint_text("http://127.0.0.1:7890"),
                                    );
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.new_proxy_bypass)
                                            .desired_width(130.0)
                                            .hint_text("绕过列表"),
                                    );

                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            let save = egui::Button::new(
                                                RichText::new("保存")
                                                    .size(12.5)
                                                    .color(theme::ACCENT_BLUE),
                                            )
                                            .fill(Color32::TRANSPARENT)
                                            .stroke(Stroke::NONE);
                                            if ui.add(save).clicked() {
                                                let id = self.new_proxy_id.trim().to_string();
                                                if !id.is_empty() {
                                                    let name = if self.new_proxy_name.trim().is_empty() {
                                                        id.clone()
                                                    } else {
                                                        self.new_proxy_name.trim().to_string()
                                                    };
                                                    let bypass = self
                                                        .new_proxy_bypass
                                                        .split([';', ','])
                                                        .map(|s| s.trim().to_string())
                                                        .filter(|s| !s.is_empty())
                                                        .collect();
                                                    self.config.proxies.insert(
                                                        id,
                                                        ProxyProfile {
                                                            name,
                                                            url: self.new_proxy_url.trim().to_string(),
                                                            bypass,
                                                        },
                                                    );
                                                    self.new_proxy_id.clear();
                                                    self.new_proxy_name.clear();
                                                    self.show_add_proxy = false;
                                                    config_changed = true;
                                                }
                                            }
                                        },
                                    );
                                });
                            });
                    }
                });

                ui.add_space(18.0);

                // === Section 2: 启动策略 (Magpie segmented toggle cards) ===
                ui.label(RichText::new("启动策略").size(12.0).color(theme::TEXT_MUTED));
                ui.add_space(6.0);

                theme::section_card_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("启动前检测代理端口连通性")
                                .size(13.0)
                                .color(theme::TEXT_PRIMARY),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if theme::segmented_bool_toggle(
                                ui,
                                &mut self.config.check_proxy_before_launch,
                            ) {
                                config_changed = true;
                            }
                        });
                    });
                });

                ui.add_space(8.0);

                theme::section_card_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("配置文件路径")
                                .size(13.0)
                                .color(theme::TEXT_PRIMARY),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(self.config_path.display().to_string())
                                    .size(12.0)
                                    .color(theme::TEXT_SECONDARY),
                            );
                        });
                    });
                });
            });

        if let Some(key) = to_delete {
            self.config.proxies.remove(&key);
            config_changed = true;
        }
        if config_changed {
            self.save_config();
            self.refresh_proxy_checks();
        }
    }
}

impl eframe::App for AgentLaunchApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_bar")
            .frame(
                egui::Frame::new()
                    .fill(theme::BG_CANVAS)
                    .inner_margin(Margin::symmetric(20, 10))
                    .stroke(Stroke::new(1.0, theme::DIVIDER)),
            )
            .show(ctx, |ui| {
                self.render_top_bar(ui);
            });

        if let Some(toast) = &self.toast {
            egui::TopBottomPanel::bottom("toast_bar")
                .frame(
                    egui::Frame::new()
                        .fill(theme::BG_CARD)
                        .inner_margin(Margin::symmetric(20, 8))
                        .stroke(Stroke::new(1.0, theme::DIVIDER)),
                )
                .show(ctx, |ui| {
                    let color = if toast.is_error {
                        theme::STATUS_RED
                    } else {
                        theme::STATUS_GREEN
                    };
                    ui.label(RichText::new(&toast.text).size(12.0).color(color));
                });
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::BG_CANVAS)
                    .inner_margin(Margin::symmetric(20, 16)),
            )
            .show(ctx, |ui| match self.active_tab {
                ActiveTab::Agents => self.render_agents_tab(ui),
                ActiveTab::Settings => self.render_settings_tab(ui),
            });
    }
}
