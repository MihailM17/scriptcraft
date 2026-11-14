//! ScriptCraft design system: palette, global egui style and the painted
//! widgets that give the app its look.
//!
//! Design language: calm neutral surfaces, one icon-blue accent, generous
//! radii, tinted (not solid) selection, soft shadows on floating things.
//! Everything reads colors through [`kit()`], so switching the theme in
//! Settings repaints the whole app from one palette table.
//!
//! Revision colours are *not* part of a theme: they carry meaning on a real
//! production (blue revision, pink revision…) and must stay identical in
//! every palette.

use egui::{
    Align2, Color32, CornerRadius, FontId, Margin, Painter, Pos2, Rect, Response, Sense, Shadow,
    Stroke, StrokeKind, Ui,
};
use scriptcraft_core::revision::RevisionColor;
use std::sync::Arc;

const fn hex(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

/// Palette. Field names are kept from the old KIT theme so the rest of the
/// app keeps compiling; the new fields are `surface`, `accent_soft`,
/// `accent_text`.
#[derive(Clone, Copy)]
pub struct KitColors {
    /// Panels (navigator, toolbar, status bar).
    pub window: Color32,
    /// Canvas behind the script pages.
    pub desk: Color32,
    pub page: Color32,
    pub page_edge: Color32,
    pub start_tab_active: Color32,
    /// Far-left module rail.
    pub rail: Color32,
    pub text: Color32,
    pub button: Color32,
    pub button_hover: Color32,
    /// Text-input background.
    pub base: Color32,
    pub faint: Color32,
    pub accent: Color32,
    /// Muted / secondary text.
    pub disabled_text: Color32,
    /// Hairline borders.
    pub stroke: Color32,
    pub sep: Color32,
    pub module_bg: Color32,
    pub strip: Color32,
    pub field_stroke: Color32,
    pub hover_light: Color32,
    pub elt_bg: Color32,
    pub elt_stroke: Color32,
    pub heading_text: Color32,
    /// Raised surfaces: cards, popups, windows.
    pub surface: Color32,
    /// Translucent accent used for selected rows and active chips.
    pub accent_soft: Color32,
    /// Accent-colored text/icons that stay readable on the soft tint.
    pub accent_text: Color32,
    /// Misspelling underline. Must clear 3:1 against the page on every
    /// palette, so it is part of the palette rather than a fixed red.
    pub spell_error: Color32,
}

/// Far-left module rail width; matches the toolbar Menu button.
pub const RAIL_WIDTH: f32 = 82.0;

pub const KIT_MIDNIGHT: KitColors = KitColors {
    window: hex(0x16181D),
    desk: hex(0x0F1114),
    page: hex(0x20232A),
    page_edge: hex(0x2D313A),
    start_tab_active: hex(0x16181D),
    rail: hex(0x111317),
    text: hex(0xE6E8EC),
    button: hex(0x23262E),
    button_hover: hex(0x2D313B),
    base: hex(0x1B1E24),
    faint: hex(0x1B1E24),
    accent: hex(0x4294F8),
    disabled_text: hex(0x8B92A1),
    stroke: hex(0x262A32),
    sep: hex(0x242830),
    module_bg: hex(0x1B1E24),
    strip: hex(0x1A1D23),
    field_stroke: hex(0x2E333D),
    hover_light: hex(0x21252C),
    elt_bg: hex(0x1D2027),
    elt_stroke: hex(0x2A2E37),
    heading_text: Color32::WHITE,
    surface: hex(0x1D2027),
    accent_soft: Color32::from_rgba_premultiplied(15, 34, 56, 58),
    accent_text: hex(0x9BC8FF),
    spell_error: hex(0xFF6B6B),
};

pub const KIT_LIGHT: KitColors = KitColors {
    window: hex(0xF8F9FB),
    desk: hex(0xE9EBEF),
    page: hex(0xFFFFFF),
    page_edge: hex(0xDCDFE5),
    start_tab_active: hex(0xFFFFFF),
    rail: hex(0xF0F2F5),
    text: hex(0x1C1F26),
    button: hex(0xEEF0F4),
    button_hover: hex(0xE3E6EC),
    base: hex(0xFFFFFF),
    faint: hex(0xF0F2F5),
    accent: hex(0x4294F8),
    disabled_text: hex(0x7A8190),
    stroke: hex(0xE0E3E9),
    sep: hex(0xE6E8ED),
    module_bg: hex(0xFFFFFF),
    strip: hex(0xF1F3F6),
    field_stroke: hex(0xD5D9E1),
    hover_light: hex(0xEEF0F4),
    elt_bg: hex(0xF0F2F5),
    elt_stroke: hex(0xDCDFE5),
    heading_text: hex(0x12141A),
    surface: hex(0xFFFFFF),
    accent_soft: Color32::from_rgba_premultiplied(10, 23, 39, 40),
    accent_text: hex(0x1B64C4),
    spell_error: hex(0xC62F2F),
};

// ---- additional palettes ---------------------------------------------------
//
// Each palette keeps the same semantic roles as Midnight and Light, so every
// widget reads correctly without a per-theme branch. Values are start points
// tuned against the UI snapshots; `every_theme_is_legible` in the tests keeps
// text, muted text and accents above their contrast floors.

/// Tokyo Night: deep navy surfaces with an electric blue accent.
pub const KIT_TOKYO_NIGHT: KitColors = KitColors {
    window: hex(0x1A1B26),
    desk: hex(0x16161E),
    page: hex(0x1F2335),
    page_edge: hex(0x292E42),
    start_tab_active: hex(0x1A1B26),
    rail: hex(0x16161E),
    text: hex(0xC0CAF5),
    button: hex(0x24283B),
    button_hover: hex(0x2E3350),
    base: hex(0x1F2335),
    faint: hex(0x1B1E2E),
    accent: hex(0x7AA2F7),
    disabled_text: hex(0x787C99),
    stroke: hex(0x292E42),
    sep: hex(0x24283B),
    module_bg: hex(0x1F2335),
    strip: hex(0x1B1E2E),
    field_stroke: hex(0x3B4261),
    hover_light: hex(0x24283B),
    elt_bg: hex(0x1F2335),
    elt_stroke: hex(0x292E42),
    heading_text: Color32::WHITE,
    surface: hex(0x1F2335),
    accent_soft: Color32::from_rgba_premultiplied(22, 29, 44, 45),
    accent_text: hex(0xA9C1FF),
    spell_error: hex(0xFF6B6B),
};

/// Nord: arctic blue-grey, low saturation, frost accent.
pub const KIT_NORD: KitColors = KitColors {
    window: hex(0x2E3440),
    desk: hex(0x272C36),
    page: hex(0x3B4252),
    page_edge: hex(0x4C566A),
    start_tab_active: hex(0x2E3440),
    rail: hex(0x272C36),
    text: hex(0xECEFF4),
    button: hex(0x3B4252),
    button_hover: hex(0x434C5E),
    base: hex(0x2E3440),
    faint: hex(0x2B313B),
    accent: hex(0x88C0D0),
    disabled_text: hex(0x8A97AC),
    stroke: hex(0x434C5E),
    sep: hex(0x3B4252),
    module_bg: hex(0x343B48),
    strip: hex(0x2B313B),
    field_stroke: hex(0x4C566A),
    hover_light: hex(0x39414F),
    elt_bg: hex(0x343B48),
    elt_stroke: hex(0x434C5E),
    heading_text: Color32::WHITE,
    surface: hex(0x353C4A),
    accent_soft: Color32::from_rgba_premultiplied(24, 34, 37, 45),
    accent_text: hex(0x9FD3DE),
    spell_error: hex(0xE07A84),
};

/// Gruvbox Dark: warm retro surfaces, golden accent, low glare.
pub const KIT_GRUVBOX: KitColors = KitColors {
    window: hex(0x282828),
    desk: hex(0x1D2021),
    page: hex(0x32302F),
    page_edge: hex(0x3C3836),
    start_tab_active: hex(0x282828),
    rail: hex(0x1D2021),
    text: hex(0xEBDBB2),
    button: hex(0x3C3836),
    button_hover: hex(0x504945),
    base: hex(0x2A2827),
    faint: hex(0x2A2827),
    accent: hex(0xFABD2F),
    disabled_text: hex(0xA89984),
    stroke: hex(0x3C3836),
    sep: hex(0x3C3836),
    module_bg: hex(0x32302F),
    strip: hex(0x2A2827),
    field_stroke: hex(0x504945),
    hover_light: hex(0x3A3735),
    elt_bg: hex(0x32302F),
    elt_stroke: hex(0x3C3836),
    heading_text: hex(0xFBF1C7),
    surface: hex(0x32302F),
    accent_soft: Color32::from_rgba_premultiplied(37, 28, 7, 38),
    accent_text: hex(0xFFD866),
    spell_error: hex(0xFB4934),
};

/// Rosé Pine: muted plum surfaces, iris accent, soft on long sessions.
pub const KIT_ROSE_PINE: KitColors = KitColors {
    window: hex(0x191724),
    desk: hex(0x14131F),
    page: hex(0x1F1D2E),
    page_edge: hex(0x26233A),
    start_tab_active: hex(0x191724),
    rail: hex(0x14131F),
    text: hex(0xE0DEF4),
    button: hex(0x26233A),
    button_hover: hex(0x2E2B44),
    base: hex(0x1F1D2E),
    faint: hex(0x1B1928),
    accent: hex(0xC4A7E7),
    disabled_text: hex(0x8B87A8),
    stroke: hex(0x26233A),
    sep: hex(0x26233A),
    module_bg: hex(0x1F1D2E),
    strip: hex(0x1B1928),
    field_stroke: hex(0x34314A),
    hover_light: hex(0x241F35),
    elt_bg: hex(0x1F1D2E),
    elt_stroke: hex(0x26233A),
    heading_text: Color32::WHITE,
    surface: hex(0x1F1D2E),
    accent_soft: Color32::from_rgba_premultiplied(32, 28, 38, 42),
    accent_text: hex(0xD9C2F0),
    spell_error: hex(0xEB6F92),
};

/// Sepia: warm paper for daylight writing, burnt-orange ink accent.
pub const KIT_SEPIA: KitColors = KitColors {
    window: hex(0xF4EDDF),
    desk: hex(0xE6DCC8),
    page: hex(0xFDF8EC),
    page_edge: hex(0xE3D8C2),
    start_tab_active: hex(0xF4EDDF),
    rail: hex(0xEEE5D4),
    text: hex(0x3A3226),
    button: hex(0xEEE5D4),
    button_hover: hex(0xE6DBC6),
    base: hex(0xFFFDF7),
    faint: hex(0xF0E8D8),
    accent: hex(0xA85A2E),
    disabled_text: hex(0x7A6E5C),
    stroke: hex(0xDFD3BC),
    sep: hex(0xE7DDC9),
    module_bg: hex(0xFDF8EC),
    strip: hex(0xF1EADD),
    field_stroke: hex(0xD6C8AE),
    hover_light: hex(0xF0E7D5),
    elt_bg: hex(0xF3EBDB),
    elt_stroke: hex(0xE0D4BE),
    heading_text: hex(0x241E15),
    surface: hex(0xFFFDF7),
    accent_soft: Color32::from_rgba_premultiplied(25, 13, 7, 38),
    accent_text: hex(0x8A4622),
    spell_error: hex(0xB23A2E),
};

/// High contrast: pure black, white text, a bright accent. For set monitors
/// and for anyone who needs maximum separation.
pub const KIT_HIGH_CONTRAST: KitColors = KitColors {
    window: hex(0x000000),
    desk: hex(0x000000),
    page: hex(0x111111),
    page_edge: hex(0x3A3A3A),
    start_tab_active: hex(0x000000),
    rail: hex(0x000000),
    text: Color32::WHITE,
    button: hex(0x1A1A1A),
    button_hover: hex(0x2A2A2A),
    base: hex(0x0A0A0A),
    faint: hex(0x141414),
    accent: hex(0x4DA3FF),
    disabled_text: hex(0xC8C8C8),
    stroke: hex(0x6A6A6A),
    sep: hex(0x4A4A4A),
    module_bg: hex(0x0A0A0A),
    strip: hex(0x0E0E0E),
    field_stroke: hex(0x8A8A8A),
    hover_light: hex(0x1F1F1F),
    elt_bg: hex(0x141414),
    elt_stroke: hex(0x6A6A6A),
    heading_text: Color32::WHITE,
    surface: hex(0x141414),
    accent_soft: Color32::from_rgba_premultiplied(17, 35, 55, 55),
    accent_text: hex(0x9CCBFF),
    spell_error: hex(0xFF6B6B),
};

/// User-selectable theme. Persisted by name in `settings.conf`; the old
/// `dark_mode` setting is still honoured as a fallback.
///
/// `ALL` is also the Settings dropdown order: darks first, then lights.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    Midnight,
    TokyoNight,
    Nord,
    Gruvbox,
    RosePine,
    Light,
    Sepia,
    HighContrast,
}

impl Theme {
    pub const ALL: [Theme; 8] = [
        Theme::Midnight,
        Theme::TokyoNight,
        Theme::Nord,
        Theme::Gruvbox,
        Theme::RosePine,
        Theme::Light,
        Theme::Sepia,
        Theme::HighContrast,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Theme::Midnight => "Midnight",
            Theme::TokyoNight => "Tokyo Night",
            Theme::Nord => "Nord",
            Theme::Gruvbox => "Gruvbox",
            Theme::RosePine => "Rosé Pine",
            Theme::Light => "Light",
            Theme::Sepia => "Sepia",
            Theme::HighContrast => "High Contrast",
        }
    }

    /// Stable key used in `settings.conf`.
    pub fn key(self) -> &'static str {
        match self {
            Theme::Midnight => "midnight",
            Theme::TokyoNight => "tokyo-night",
            Theme::Nord => "nord",
            Theme::Gruvbox => "gruvbox",
            Theme::RosePine => "rose-pine",
            Theme::Light => "light",
            Theme::Sepia => "sepia",
            Theme::HighContrast => "high-contrast",
        }
    }

    /// Parse a stored key. `None` for keys this build doesn't know, so an
    /// older or newer settings file can't silently change the theme.
    pub fn from_key(key: &str) -> Option<Theme> {
        Theme::ALL.into_iter().find(|t| t.key() == key)
    }

    /// Palette every widget reads through [`kit()`].
    pub fn palette(self) -> &'static KitColors {
        match self {
            Theme::Midnight => &KIT_MIDNIGHT,
            Theme::TokyoNight => &KIT_TOKYO_NIGHT,
            Theme::Nord => &KIT_NORD,
            Theme::Gruvbox => &KIT_GRUVBOX,
            Theme::RosePine => &KIT_ROSE_PINE,
            Theme::Light => &KIT_LIGHT,
            Theme::Sepia => &KIT_SEPIA,
            Theme::HighContrast => &KIT_HIGH_CONTRAST,
        }
    }

    /// Light palettes need dark ink; everything in `kit_visuals` branches on
    /// this instead of the old single `Light` variant.
    pub fn is_light(self) -> bool {
        matches!(self, Theme::Light | Theme::Sepia)
    }
}

thread_local! {
    static KIT_PAL: std::cell::Cell<KitColors> = const { std::cell::Cell::new(KIT_MIDNIGHT) };
    static KIT_THEME: std::cell::Cell<Theme> = const { std::cell::Cell::new(Theme::Midnight) };
}

/// Industry-standard revision colour. Never themed: a blue revision is blue
/// on every production, whatever the app's palette.
pub fn revision_color(color: RevisionColor) -> Color32 {
    let (r, g, b) = color.rgb();
    Color32::from_rgb(r, g, b)
}

/// Hairline edge for revision marks. Pale revision colours (yellow, buff,
/// goldenrod) sit well under 2:1 against a white or cream page, so the mark
/// carries an edge in the opposite direction of the page. That keeps its
/// boundary visible on every palette without changing the colour itself.
pub fn revision_mark_edge() -> Color32 {
    if kit_is_light() {
        Color32::from_black_alpha(120)
    } else {
        Color32::from_white_alpha(120)
    }
}

/// Fill a revision-coloured region (margin bar, legend swatch) with its edge.
pub fn paint_revision(painter: &Painter, rect: Rect, color: RevisionColor, radius: f32) {
    painter.rect_filled(rect, radius, revision_color(color));
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(1.0_f32, revision_mark_edge()),
        StrokeKind::Inside,
    );
}

pub fn kit() -> KitColors {
    KIT_PAL.with(|p| p.get())
}

/// Theme currently backing [`kit()`].
pub fn current_theme() -> Theme {
    KIT_THEME.with(|c| c.get())
}

pub fn kit_is_light() -> bool {
    current_theme().is_light()
}

/// Switch the palette; the caller re-applies the style via [`apply_theme`].
pub fn set_kit_theme(theme: Theme) {
    KIT_THEME.with(|c| c.set(theme));
    KIT_PAL.with(|p| p.set(*theme.palette()));
}

/// Global egui visuals for the active palette.
pub fn kit_visuals() -> egui::Visuals {
    let c = kit();
    let light = kit_is_light();
    let mut v = if light {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    v.window_fill = c.surface;
    v.panel_fill = c.window;
    v.faint_bg_color = c.faint;
    v.extreme_bg_color = c.base;
    v.override_text_color = Some(c.text);
    v.weak_text_color = Some(c.disabled_text);
    v.window_stroke = Stroke::new(1.0_f32, c.stroke);
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(10);
    let shadow_a = if light { 36 } else { 110 };
    v.window_shadow = Shadow {
        offset: [0, 10],
        blur: 28,
        spread: 0,
        color: Color32::from_black_alpha(shadow_a),
    };
    v.popup_shadow = Shadow {
        offset: [0, 6],
        blur: 18,
        spread: 0,
        color: Color32::from_black_alpha(shadow_a),
    };
    // Text selection and selected widgets: translucent accent. Each theme's
    // own accent keeps selection readable; Midnight and Light both use
    // #4294F8, so their rendering is unchanged.
    let a = c.accent;
    v.selection.bg_fill =
        Color32::from_rgba_unmultiplied(a.r(), a.g(), a.b(), if light { 70 } else { 105 });
    v.selection.stroke = Stroke::new(1.0_f32, c.accent_text);
    v.hyperlink_color = c.accent_text;
    v.text_cursor.stroke = Stroke::new(2.0_f32, c.accent_text);

    let radius = CornerRadius::same(7);
    let text_stroke = Stroke::new(1.0_f32, c.text);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = radius;
        w.fg_stroke = text_stroke;
        w.expansion = 0.0;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, c.sep);
    v.widgets.inactive.bg_fill = c.button;
    v.widgets.inactive.weak_bg_fill = c.button;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, c.sep);
    v.widgets.hovered.bg_fill = c.button_hover;
    v.widgets.hovered.weak_bg_fill = c.button_hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, c.field_stroke);
    // Pressed: a notch darker than hover, never a loud solid accent.
    v.widgets.active.bg_fill = c.field_stroke;
    v.widgets.active.weak_bg_fill = c.field_stroke;
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32, c.accent);
    v.widgets.open.bg_fill = c.button_hover;
    v.widgets.open.weak_bg_fill = c.button_hover;
    v
}

/// Visuals plus spacing. Call instead of `ctx.set_visuals(kit_visuals())`.
///
/// `interact_size` is deliberately left alone: the script editor's row
/// grid depends on it.
pub fn apply_theme(ctx: &egui::Context) {
    ctx.set_visuals(kit_visuals());
    ctx.style_mut(|s| {
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
        s.spacing.window_margin = Margin::same(16);
        s.spacing.menu_margin = Margin::same(6);
        s.spacing.indent = 18.0;
        if let Some(f) = s.text_styles.get_mut(&egui::TextStyle::Heading) {
            f.size = 22.0;
        }
        if let Some(f) = s.text_styles.get_mut(&egui::TextStyle::Small) {
            f.size = 11.5;
        }
    });
}

// ---- helpers ---------------------------------------------------------------

/// Fill for a flat button: rest / hover / pressed.
pub fn button_fill(resp: &Response) -> Color32 {
    let c = kit();
    if resp.is_pointer_button_down_on() {
        c.field_stroke
    } else if resp.hovered() {
        c.button_hover
    } else {
        c.button
    }
}

/// Single-line text that ends in an ellipsis instead of overflowing.
pub fn ellipsized(
    painter: &Painter,
    text: &str,
    font: FontId,
    color: Color32,
    max_w: f32,
) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap.max_width = max_w.max(1.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('\u{2026}');
    painter.layout_job(job)
}

/// Left-aligned, vertically centered text that is clipped with an ellipsis.
pub fn paint_ellipsized(
    painter: &Painter,
    pos_left: Pos2,
    center_y: f32,
    text: &str,
    font: FontId,
    color: Color32,
    max_w: f32,
) {
    let g = ellipsized(painter, text, font, color, max_w);
    let pos = Pos2::new(pos_left.x, center_y - g.size().y / 2.0);
    painter.galley(pos, g, color);
}

/// Paper sheet with a soft drop shadow, used for every script page.
pub fn paint_page(painter: &Painter, rect: Rect) {
    let c = kit();
    let a = if kit_is_light() { 30 } else { 100 };
    painter.add(
        Shadow {
            offset: [0, 4],
            blur: 14,
            spread: 0,
            color: Color32::from_black_alpha(a),
        }
        .as_shape(rect, 3.0),
    );
    painter.rect_filled(rect, 3.0, c.page);
    painter.rect_stroke(
        rect,
        3.0,
        Stroke::new(1.0_f32, c.page_edge),
        StrokeKind::Inside,
    );
}

/// Tiny dot used for the saved / unsaved indicator.
pub fn status_dot(painter: &Painter, center: Pos2, color: Color32) {
    painter.circle_filled(center, 3.5, color);
}

// ---- widgets ----------------------------------------------------------------

/// One module on the far-left rail: icon in a rounded pill, label below.
pub fn rail_item(
    ui: &mut Ui,
    label: &str,
    active: bool,
    draw_icon: impl FnOnce(&Painter, Pos2, Color32),
) -> Response {
    let c = kit();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(RAIL_WIDTH, 58.0), Sense::click());
    let hovered = resp.hovered();
    let pill = Rect::from_center_size(
        Pos2::new(rect.center().x, rect.top() + 20.0),
        egui::vec2(42.0, 28.0),
    );
    let p = ui.painter();
    if active {
        p.rect_filled(pill, 10.0, c.accent_soft);
    } else if hovered {
        p.rect_filled(pill, 10.0, c.hover_light);
    }
    let color = if active {
        c.accent_text
    } else if hovered {
        c.text
    } else {
        c.disabled_text
    };
    draw_icon(p, pill.center(), color);
    p.text(
        Pos2::new(rect.center().x, rect.top() + 44.0),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(10.5),
        color,
    );
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Navigator row: scene number, heading, duration, one-line preview.
pub fn scene_row(
    ui: &mut Ui,
    number: &str,
    heading: &str,
    preview: &str,
    duration: &str,
    omitted: bool,
    selected: bool,
) -> Response {
    let c = kit();
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 58.0), Sense::click());
    let card = rect.shrink2(egui::vec2(8.0, 2.0));
    let p = ui.painter();
    if selected {
        p.rect_filled(card, 8.0, c.accent_soft);
        let bar = Rect::from_min_size(
            Pos2::new(card.left(), card.top() + 10.0),
            egui::vec2(3.0, card.height() - 20.0),
        );
        p.rect_filled(bar, 1.5, c.accent);
    } else if resp.hovered() {
        p.rect_filled(card, 8.0, c.hover_light);
    }
    let left = card.left() + 14.0;
    let right = card.right() - 12.0;
    let muted = c.disabled_text;
    let y1 = card.top() + 17.0;

    let dur = p.layout_no_wrap(duration.to_owned(), FontId::proportional(11.5), muted);
    let dur_w = dur.size().x;
    p.galley(
        Pos2::new(right - dur_w, y1 - dur.size().y / 2.0),
        dur,
        muted,
    );

    let mut x = left;
    if !number.is_empty() {
        let num_col = if selected { c.accent_text } else { muted };
        let g = p.layout_no_wrap(number.to_owned(), FontId::proportional(12.5), num_col);
        let w = g.size().x;
        p.galley(Pos2::new(x, y1 - g.size().y / 2.0), g, num_col);
        x += w + 6.0;
    }
    let head_w = (right - dur_w - 10.0 - x).max(10.0);
    paint_ellipsized(
        p,
        Pos2::new(x, y1),
        y1,
        heading,
        FontId::proportional(13.0),
        c.heading_text,
        head_w,
    );
    let mut preview_x = left;
    if omitted {
        let font = FontId::proportional(9.5);
        let g = p.layout_no_wrap("OMITTED".to_owned(), font.clone(), c.text);
        let pill = Rect::from_min_size(
            Pos2::new(left, card.top() + 32.0),
            egui::vec2(g.size().x + 12.0, 16.0),
        );
        p.rect_filled(pill, 8.0, c.field_stroke);
        p.galley(
            Pos2::new(
                pill.center().x - g.size().x / 2.0,
                pill.center().y - g.size().y / 2.0,
            ),
            g,
            c.text,
        );
        preview_x = pill.right() + 8.0;
    }
    if !preview.is_empty() && preview_x < right {
        paint_ellipsized(
            p,
            Pos2::new(preview_x, card.top() + 39.0),
            card.top() + 39.0,
            preview,
            FontId::proportional(11.5),
            muted,
            right - preview_x,
        );
    }
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let mut label = if number.is_empty() {
        heading.to_owned()
    } else {
        format!("{number} {heading}")
    };
    if !preview.is_empty() {
        label.push_str(" — ");
        label.push_str(preview);
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label.clone()));
    resp
}

/// Toolbar "Menu" trigger: hamburger glyph plus label in a soft button.
pub fn menu_trigger(ui: &mut Ui) -> Response {
    let c = kit();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(82.0, 28.0), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 7.0, button_fill(&resp));
    p.rect_stroke(rect, 7.0, Stroke::new(1.0_f32, c.sep), StrokeKind::Inside);
    let st = Stroke::new(1.5_f32, c.text);
    let x0 = rect.left() + 12.0;
    for dy in [-4.0_f32, 0.0, 4.0] {
        let y = rect.center().y + dy;
        p.line_segment([Pos2::new(x0, y), Pos2::new(x0 + 12.0, y)], st);
    }
    p.text(
        Pos2::new(rect.left() + 32.0, rect.center().y),
        Align2::LEFT_CENTER,
        "Menu",
        FontId::proportional(13.5),
        c.text,
    );
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Menu"));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Thin vertical divider between toolbar groups.
pub fn toolbar_sep(ui: &mut Ui) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(1.0, 18.0), Sense::hover());
    ui.painter().rect_filled(r, 0.0, kit().sep);
}

/// Small muted overline label ("RECENT PROJECTS").
pub fn overline(ui: &mut Ui, text: &str) {
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .size(11.0)
            .color(kit().disabled_text),
    );
}

/// Clickable recent-project card for the Start page.
pub struct ProjectCard {
    pub response: Response,
    pub remove_clicked: bool,
}

pub fn project_card(ui: &mut Ui, name: &str, path: &str, when: &str) -> ProjectCard {
    let c = kit();
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 64.0), Sense::click());
    let card = rect.shrink2(egui::vec2(0.0, 3.0));
    let remove_rect = Rect::from_center_size(
        Pos2::new(card.right() - 18.0, card.center().y),
        egui::vec2(26.0, 26.0),
    );
    let remove = ui
        .interact(
            remove_rect,
            ui.id().with(("remove_recent", path)),
            Sense::click(),
        )
        .on_hover_text("Remove from list");
    remove.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Remove from list")
    });
    let p = ui.painter().clone();
    let fill = if resp.hovered() {
        c.hover_light
    } else {
        c.surface
    };
    let border = if resp.hovered() {
        c.field_stroke
    } else {
        c.sep
    };
    p.rect_filled(card, 10.0, fill);
    p.rect_stroke(card, 10.0, Stroke::new(1.0_f32, border), StrokeKind::Inside);

    // Document tile.
    let tile = Rect::from_min_size(
        Pos2::new(card.left() + 14.0, card.center().y - 19.0),
        egui::vec2(38.0, 38.0),
    );
    p.rect_filled(tile, 9.0, c.accent_soft);
    let st = Stroke::new(1.4_f32, c.accent_text);
    let page = Rect::from_center_size(tile.center(), egui::vec2(14.0, 18.0));
    p.rect_stroke(page, 2.0, st, StrokeKind::Inside);
    for dy in [-3.0_f32, 1.0, 5.0] {
        p.line_segment(
            [
                Pos2::new(page.left() + 3.5, page.center().y + dy),
                Pos2::new(page.right() - 3.5, page.center().y + dy),
            ],
            Stroke::new(1.2_f32, c.accent_text),
        );
    }

    let x = tile.right() + 14.0;
    let right = card.right() - 40.0;
    let when_g = p.layout_no_wrap(when.to_owned(), FontId::proportional(11.5), c.disabled_text);
    let when_w = when_g.size().x;
    p.galley(
        Pos2::new(right - when_w, card.top() + 16.0),
        when_g,
        c.disabled_text,
    );
    let text_w = (right - 12.0 - x).max(10.0);
    paint_ellipsized(
        &p,
        Pos2::new(x, card.top() + 21.0),
        card.top() + 21.0,
        name,
        FontId::proportional(14.5),
        c.heading_text,
        text_w,
    );
    paint_ellipsized(
        &p,
        Pos2::new(x, card.top() + 43.0),
        card.top() + 43.0,
        path,
        FontId::proportional(11.5),
        c.disabled_text,
        right - x,
    );

    if remove.hovered() {
        p.rect_filled(remove_rect, 6.0, c.hover_light);
    }
    let ink = if remove.hovered() {
        c.heading_text
    } else {
        c.disabled_text
    };
    let mid = remove_rect.center();
    let r = 5.0;
    let st = Stroke::new(1.4_f32, ink);
    p.line_segment(
        [
            Pos2::new(mid.x - r, mid.y - r),
            Pos2::new(mid.x + r, mid.y + r),
        ],
        st,
    );
    p.line_segment(
        [
            Pos2::new(mid.x + r, mid.y - r),
            Pos2::new(mid.x - r, mid.y + r),
        ],
        st,
    );

    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
    ProjectCard {
        response: resp.on_hover_cursor(egui::CursorIcon::PointingHand),
        remove_clicked: remove.clicked(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// sRGB channel → linear light, per WCAG 2.1.
    fn linear(c: u8) -> f32 {
        let s = c as f32 / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    }

    fn luminance(c: Color32) -> f32 {
        0.2126 * linear(c.r()) + 0.7152 * linear(c.g()) + 0.0722 * linear(c.b())
    }

    fn contrast(a: Color32, b: Color32) -> f32 {
        let (la, lb) = (luminance(a), luminance(b));
        let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// `Color32` is premultiplied, so compositing a translucent foreground
    /// means adding the background scaled by the remaining opacity.
    fn over(fg: Color32, bg: Color32) -> Color32 {
        let inv = 1.0 - fg.a() as f32 / 255.0;
        let mix = |f: u8, b: u8| (f as f32 + b as f32 * inv).round().clamp(0.0, 255.0) as u8;
        Color32::from_rgb(
            mix(fg.r(), bg.r()),
            mix(fg.g(), bg.g()),
            mix(fg.b(), bg.b()),
        )
    }

    #[test]
    fn theme_keys_round_trip() {
        for theme in Theme::ALL {
            assert_eq!(Theme::from_key(theme.key()), Some(theme), "{}", theme.key());
        }
        // A key this build doesn't know must not silently pick a theme.
        assert_eq!(Theme::from_key("solarized"), None);
        assert_eq!(Theme::from_key("Nord"), None, "keys are case-sensitive");
    }

    #[test]
    fn theme_keys_and_labels_are_unique() {
        for (i, a) in Theme::ALL.iter().enumerate() {
            for b in &Theme::ALL[i + 1..] {
                assert_ne!(a.key(), b.key());
                assert_ne!(a.label(), b.label());
            }
        }
    }

    #[test]
    fn every_theme_paints_from_its_own_palette() {
        for theme in Theme::ALL {
            set_kit_theme(theme);
            assert_eq!(current_theme(), theme);
            assert_eq!(kit().window, theme.palette().window, "{}", theme.label());
            assert_eq!(kit_is_light(), theme.is_light(), "{}", theme.label());
        }
    }

    #[test]
    fn light_themes_have_light_pages() {
        for theme in Theme::ALL {
            let page = theme.palette().page;
            if theme.is_light() {
                assert!(
                    luminance(page) > 0.5,
                    "{}: light theme needs a light page",
                    theme.label()
                );
            } else {
                assert!(
                    luminance(page) < 0.5,
                    "{}: dark theme needs a dark page",
                    theme.label()
                );
            }
        }
    }

    #[test]
    fn every_theme_is_legible() {
        for theme in Theme::ALL {
            let c = *theme.palette();
            let name = theme.label();
            let body = contrast(c.text, c.window);
            let heading = contrast(c.heading_text, c.window);
            let muted = contrast(c.disabled_text, c.window);
            let accent = contrast(c.accent, c.page);
            let accent_text = contrast(c.accent_text, c.page);
            assert!(body >= 7.0, "{name}: body text is {body:.2}:1 on panels");
            assert!(
                heading >= 7.0,
                "{name}: heading text is {heading:.2}:1 on panels"
            );
            assert!(muted >= 3.0, "{name}: secondary text is {muted:.2}:1");
            assert!(accent >= 3.0, "{name}: accent is {accent:.2}:1 on the page");
            assert!(
                accent_text >= 4.5,
                "{name}: accent text is {accent_text:.2}:1 on the page"
            );
            assert_ne!(
                c.page, c.desk,
                "{name}: the sheet must separate from the desk"
            );
            assert_ne!(c.page_edge, c.page, "{name}: the sheet needs an edge");
            let squiggle = contrast(c.spell_error, c.page);
            assert!(
                squiggle >= 3.0,
                "{name}: misspelling underline is {squiggle:.2}:1 on the page"
            );
        }
    }

    /// Pale revision colours (yellow, buff, goldenrod) are far under 3:1 on a
    /// white or cream page. The mark's edge is what keeps it locatable, so the
    /// edge — not the colour — has to clear the bar on every palette.
    #[test]
    fn revision_marks_stay_visible_on_every_palette() {
        for theme in Theme::ALL {
            set_kit_theme(theme);
            let page = kit().page;
            let edge = over(revision_mark_edge(), page);
            let ratio = contrast(edge, page);
            assert!(
                ratio >= 3.0,
                "{}: revision edge is {ratio:.2}:1 against the page",
                theme.label()
            );
        }
    }

    /// Revision colours carry meaning, so no palette may change them.
    #[test]
    fn revision_colours_do_not_follow_the_theme() {
        let mut baseline: Option<Vec<(u8, u8, u8)>> = None;
        for theme in Theme::ALL {
            set_kit_theme(theme);
            let row: Vec<(u8, u8, u8)> = RevisionColor::ALL
                .iter()
                .map(|color| {
                    let c = revision_color(*color);
                    (c.r(), c.g(), c.b())
                })
                .collect();
            match &baseline {
                None => baseline = Some(row),
                Some(first) => assert_eq!(
                    &row,
                    first,
                    "revision colours changed with the {} palette",
                    theme.label()
                ),
            }
        }
    }

    /// Selection must follow the theme accent instead of a hard-coded blue.
    #[test]
    fn selection_tint_follows_the_accent() {
        set_kit_theme(Theme::Sepia); // warm accent: red channel leads
        let sepia = kit_visuals().selection.bg_fill;
        assert!(sepia.r() > sepia.b(), "Sepia selection should be warm");

        set_kit_theme(Theme::TokyoNight); // cool accent: blue channel leads
        let tokyo = kit_visuals().selection.bg_fill;
        assert!(
            tokyo.b() > tokyo.r(),
            "Tokyo Night selection should be cool"
        );
    }
}
