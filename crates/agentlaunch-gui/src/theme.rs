use egui::{Color32, CornerRadius, Margin, Stroke, Vec2, Visuals, style::WidgetVisuals};

// Magpie-inspired matte dark palette
pub const BG_CANVAS: Color32 = Color32::from_rgb(20, 20, 22);
pub const BG_CARD: Color32 = Color32::from_rgb(28, 28, 31);
pub const BG_CONTROL: Color32 = Color32::from_rgb(38, 38, 42);
pub const BG_CONTROL_HOVER: Color32 = Color32::from_rgb(48, 48, 54);
pub const BG_CONTROL_ACTIVE: Color32 = Color32::from_rgb(56, 56, 64);

pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(42, 42, 47);
pub const BORDER_CONTROL: Color32 = Color32::from_rgb(54, 54, 62);
pub const DIVIDER: Color32 = Color32::from_rgb(36, 36, 40);

pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(236, 236, 239);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(142, 142, 152);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(107, 107, 117);

pub const STATUS_GREEN: Color32 = Color32::from_rgb(52, 211, 153);
pub const STATUS_AMBER: Color32 = Color32::from_rgb(251, 191, 36);
pub const STATUS_RED: Color32 = Color32::from_rgb(248, 113, 113);
pub const ACCENT_BLUE: Color32 = Color32::from_rgb(96, 165, 250);

pub fn apply_magpie_theme(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();
    visuals.panel_fill = BG_CANVAS;
    visuals.window_fill = BG_CARD;
    visuals.extreme_bg_color = Color32::from_rgb(24, 24, 27);
    visuals.faint_bg_color = Color32::from_rgb(32, 32, 36);
    visuals.window_stroke = Stroke::new(1.0, BORDER_SUBTLE);
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.menu_corner_radius = CornerRadius::same(8);
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 6],
        blur: 16,
        spread: 0,
        color: Color32::from_black_alpha(120),
    };
    visuals.override_text_color = Some(TEXT_PRIMARY);
    visuals.selection.bg_fill = Color32::from_rgb(58, 58, 68);
    visuals.selection.stroke = Stroke::new(1.0, TEXT_PRIMARY);

    let rounding = CornerRadius::same(6);

    visuals.widgets.noninteractive = WidgetVisuals {
        bg_fill: BG_CARD,
        weak_bg_fill: BG_CARD,
        bg_stroke: Stroke::new(1.0, BORDER_SUBTLE),
        corner_radius: rounding,
        fg_stroke: Stroke::new(1.0, TEXT_SECONDARY),
        expansion: 0.0,
    };
    visuals.widgets.inactive = WidgetVisuals {
        bg_fill: BG_CONTROL,
        weak_bg_fill: BG_CONTROL,
        bg_stroke: Stroke::new(1.0, BORDER_CONTROL),
        corner_radius: rounding,
        fg_stroke: Stroke::new(1.0, TEXT_PRIMARY),
        expansion: 0.0,
    };
    visuals.widgets.hovered = WidgetVisuals {
        bg_fill: BG_CONTROL_HOVER,
        weak_bg_fill: BG_CONTROL_HOVER,
        bg_stroke: Stroke::new(1.0, Color32::from_rgb(72, 72, 82)),
        corner_radius: rounding,
        fg_stroke: Stroke::new(1.0, TEXT_PRIMARY),
        expansion: 0.0,
    };
    visuals.widgets.active = WidgetVisuals {
        bg_fill: BG_CONTROL_ACTIVE,
        weak_bg_fill: BG_CONTROL_ACTIVE,
        bg_stroke: Stroke::new(1.0, Color32::from_rgb(90, 90, 102)),
        corner_radius: rounding,
        fg_stroke: Stroke::new(1.0, TEXT_PRIMARY),
        expansion: 0.0,
    };
    visuals.widgets.open = visuals.widgets.hovered;

    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 5.0);
    style.spacing.combo_height = 240.0;
    style.spacing.scroll.bar_width = 6.0;
    ctx.set_style(style);
}

pub fn card_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(BG_CARD)
        .stroke(Stroke::new(1.0, BORDER_SUBTLE))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(0))
}

pub fn section_card_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(BG_CARD)
        .stroke(Stroke::new(1.0, BORDER_SUBTLE))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(16, 12))
}

/// Draws a clean top-bar navigation tab pill (Magpie style).
pub fn nav_tab(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let fill = if selected {
        BG_CONTROL
    } else {
        Color32::TRANSPARENT
    };
    let text_color = if selected {
        TEXT_PRIMARY
    } else {
        TEXT_SECONDARY
    };
    let btn = egui::Button::new(egui::RichText::new(label).size(13.5).color(text_color))
        .fill(fill)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::same(6))
        .min_size(Vec2::new(56.0, 28.0));
    ui.add(btn)
}

/// Draws a Magpie-style segmented boolean toggle: `[ 关闭 | 开启 ]`.
pub fn segmented_bool_toggle(ui: &mut egui::Ui, value: &mut bool) -> bool {
    let mut changed = false;
    egui::Frame::new()
        .fill(Color32::from_rgb(22, 22, 25))
        .stroke(Stroke::new(1.0, BORDER_CONTROL))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(2))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(|ui| {
                let off_btn = egui::Button::new(
                    egui::RichText::new("关闭")
                        .size(12.0)
                        .color(if !*value { TEXT_PRIMARY } else { TEXT_MUTED }),
                )
                .fill(if !*value {
                    BG_CONTROL_HOVER
                } else {
                    Color32::TRANSPARENT
                })
                .stroke(Stroke::NONE)
                .corner_radius(CornerRadius::same(4))
                .min_size(Vec2::new(40.0, 22.0));

                if ui.add(off_btn).clicked() && *value {
                    *value = false;
                    changed = true;
                }

                let on_btn = egui::Button::new(
                    egui::RichText::new("开启")
                        .size(12.0)
                        .color(if *value { TEXT_PRIMARY } else { TEXT_MUTED }),
                )
                .fill(if *value {
                    BG_CONTROL_HOVER
                } else {
                    Color32::TRANSPARENT
                })
                .stroke(Stroke::NONE)
                .corner_radius(CornerRadius::same(4))
                .min_size(Vec2::new(40.0, 22.0));

                if ui.add(on_btn).clicked() && !*value {
                    *value = true;
                    changed = true;
                }
            });
        });
    changed
}

/// Draws a crisp rounded monogram icon badge for each agent.
pub fn agent_icon_badge(ui: &mut egui::Ui, agent_id: &str) {
    let (glyph, bg, fg) = match agent_id {
        "antigravity" => ("A", Color32::from_rgb(30, 42, 64), Color32::from_rgb(96, 165, 250)),
        "codex" | "codex-cli" => ("C", Color32::from_rgb(26, 48, 42), Color32::from_rgb(52, 211, 153)),
        "claude-desktop" | "claude-code" => {
            ("Cl", Color32::from_rgb(56, 38, 28), Color32::from_rgb(251, 146, 60))
        }
        "cursor" => ("Cu", Color32::from_rgb(44, 36, 58), Color32::from_rgb(192, 132, 252)),
        "vscode" => ("VS", Color32::from_rgb(24, 42, 62), Color32::from_rgb(56, 189, 248)),
        _ => ("Ag", BG_CONTROL, TEXT_PRIMARY),
    };

    let (rect, _) = ui.allocate_exact_size(Vec2::splat(32.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(8), bg);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        glyph,
        egui::FontId::proportional(14.0),
        fg,
    );
}

/// Draws a subtle 1px horizontal divider inside a card.
pub fn card_divider(ui: &mut egui::Ui) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 1.0), egui::Sense::hover());
    ui.painter()
        .line_segment([rect.left_center(), rect.right_center()], Stroke::new(1.0, DIVIDER));
}
