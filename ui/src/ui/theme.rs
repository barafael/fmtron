//! Gruvbox dark, in the flat, dense style: zero margins, no rounding, no
//! shadows, a small palette every element's colour comes from.

use egui::{Color32, CornerRadius, FontId, Shadow, Stroke, Style, TextStyle, Theme};

/// Semantic colours.
pub struct Palette {
    /// Window background.
    pub bg: Color32,
    /// Bars and popups.
    pub surface: Color32,
    /// The selected row.
    pub stripe: Color32,
    /// Hovered widgets and keycaps.
    pub hover: Color32,
    /// Separators and the width ruler.
    pub border: Color32,
    /// Body text.
    pub text: Color32,
    /// Emphasized text.
    pub strong: Color32,
    /// Secondary text: inherited defaults, flags.
    pub weak: Color32,
    /// Selection and focus.
    pub accent: Color32,
    /// Text on top of `accent`.
    pub on_accent: Color32,
    /// Field names.
    pub label: Color32,
    pub green: Color32,
    pub red: Color32,
    pub yellow: Color32,
    /// Comments in RON.
    pub comment: Color32,
    /// Numbers in RON.
    pub number: Color32,
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub const P: Palette = Palette {
    bg: hex(0x1d2021),
    surface: hex(0x32302f),
    stripe: hex(0x282828),
    hover: hex(0x504945),
    border: hex(0x665c54),
    text: hex(0xebdbb2),
    strong: hex(0xfbf1c7),
    weak: hex(0xbdae93),
    accent: hex(0xfe8019),
    on_accent: hex(0x1d2021),
    label: hex(0x8ec07c),
    green: hex(0xb8bb26),
    red: hex(0xfb4934),
    yellow: hex(0xfabd2f),
    comment: hex(0x928374),
    number: hex(0xd3869b),
};

/// Installs the theme. There is one, so the system preference is ignored.
pub fn install(ctx: &egui::Context) {
    ctx.style_mut_of(Theme::Dark, apply);
    ctx.set_theme(egui::ThemePreference::Dark);
}

fn apply(style: &mut Style) {
    use egui::FontFamily::{Monospace, Proportional};
    let p = &P;
    style.text_styles = [
        (TextStyle::Heading, FontId::new(17.0, Proportional)),
        (TextStyle::Body, FontId::new(15.0, Proportional)),
        (TextStyle::Button, FontId::new(15.0, Proportional)),
        (TextStyle::Monospace, FontId::new(14.0, Monospace)),
        (TextStyle::Small, FontId::new(12.5, Proportional)),
    ]
    .into();

    let spacing = &mut style.spacing;
    spacing.item_spacing = egui::vec2(6.0, 2.0);
    spacing.button_padding = egui::vec2(10.0, 3.0);
    spacing.window_margin = egui::Margin::ZERO;
    spacing.menu_margin = egui::Margin::ZERO;
    spacing.interact_size.y = 22.0;

    let v = &mut style.visuals;
    v.override_text_color = None;
    v.weak_text_color = Some(p.weak);
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.window_stroke = Stroke::new(1.0, p.accent);
    v.extreme_bg_color = p.bg;
    v.faint_bg_color = p.stripe;
    v.code_bg_color = p.hover;
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.yellow;
    v.error_fg_color = p.red;
    v.selection.bg_fill = p.accent;
    v.selection.stroke = Stroke::new(1.0, p.on_accent);
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);
    v.window_corner_radius = CornerRadius::ZERO;
    v.menu_corner_radius = CornerRadius::ZERO;
    v.window_shadow = Shadow::NONE;
    v.popup_shadow = Shadow::NONE;

    let w = &mut v.widgets;
    for (widget, bg, fg, stroke) in [
        (&mut w.noninteractive, p.bg, p.text, p.border),
        (
            &mut w.inactive,
            Color32::TRANSPARENT,
            p.text,
            Color32::TRANSPARENT,
        ),
        (&mut w.hovered, p.hover, p.strong, p.accent),
        (&mut w.active, p.accent, p.on_accent, p.accent),
        (&mut w.open, p.hover, p.strong, p.accent),
    ] {
        widget.bg_fill = bg;
        widget.weak_bg_fill = bg;
        widget.fg_stroke = Stroke::new(1.0, fg);
        widget.bg_stroke = Stroke::new(1.0, stroke);
        widget.corner_radius = CornerRadius::ZERO;
        widget.expansion = 0.0;
    }
}
