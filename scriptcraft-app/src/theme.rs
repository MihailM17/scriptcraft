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
    /// Content panels that sit on top of the chrome: the scene navigator
    /// list and the element reference list. Defaults to the panel colour; the
    /// KIT palettes override it with the white / grey content ink.
    pub list_bg: Color32,
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
    list_bg: hex(0x16181D),
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
    list_bg: hex(0xF8F9FB),
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

// ---- Omarchy palettes ------------------------------------------------------
//
// Exact backgrounds, foregrounds and accents from basecamp/omarchy's
// `themes/*/colors.toml`. Surface roles (page, panels, borders, muted text)
// are derived from those three colors with the same rules for every theme,
// so a theme keeps its identity while every widget stays legible; the
// contrast floors in `every_theme_is_legible` are checked on each palette.

/// Catppuccin: Omarchy theme.
pub const KIT_CATPPUCCIN: KitColors = KitColors {
    window: hex(0x1E1E2E),
    list_bg: hex(0x1E1E2E),
    desk: hex(0x14141E),
    page: hex(0x28293A),
    page_edge: hex(0x46485B),
    start_tab_active: hex(0x1E1E2E),
    rail: hex(0x171724),
    text: hex(0xCDD6F4),
    button: hex(0x2E2F40),
    button_hover: hex(0x383A4C),
    base: hex(0x252536),
    faint: hex(0x252536),
    accent: hex(0x89B4FA),
    disabled_text: hex(0x767A91),
    stroke: hex(0x353648),
    sep: hex(0x303042),
    module_bg: hex(0x232434),
    strip: hex(0x1B1B29),
    field_stroke: hex(0x44465A),
    hover_light: hex(0x2B2C3D),
    elt_bg: hex(0x272738),
    elt_stroke: hex(0x383A4C),
    heading_text: hex(0xEBEFFB),
    surface: hex(0x272738),
    accent_soft: Color32::from_rgba_premultiplied(30, 40, 55, 56),
    accent_text: hex(0xC4DAFC),
    spell_error: hex(0xFF6B6B),
};
/// Catppuccin Latte: Omarchy theme.
pub const KIT_CATPPUCCIN_LATTE: KitColors = KitColors {
    window: hex(0xEFF1F5),
    list_bg: hex(0xEFF1F5),
    desk: hex(0xDEE0E4),
    page: hex(0xF9F9FB),
    page_edge: hex(0xDBDBDD),
    start_tab_active: hex(0xEFF1F5),
    rail: hex(0xE7E9EC),
    text: hex(0x4C4F69),
    button: hex(0xE0E2E8),
    button_hover: hex(0xD7D9E0),
    base: hex(0xFBFCFC),
    faint: hex(0xFBFCFC),
    accent: hex(0x1E66F5),
    disabled_text: hex(0x888A9C),
    stroke: hex(0xDADCE3),
    sep: hex(0xDFE1E7),
    module_bg: hex(0xF7F8FA),
    strip: hex(0xEAECF0),
    field_stroke: hex(0xCBCDD6),
    hover_light: hex(0xE3E5EA),
    elt_bg: hex(0xE7E9EE),
    elt_stroke: hex(0xD7D9E0),
    heading_text: hex(0x2A2B3A),
    surface: hex(0xE7E9EE),
    accent_soft: Color32::from_rgba_premultiplied(7, 22, 54, 56),
    accent_text: hex(0x0F337A),
    spell_error: hex(0xC62F2F),
};
/// Ethereal: Omarchy theme.
pub const KIT_ETHEREAL: KitColors = KitColors {
    window: hex(0x060B1E),
    list_bg: hex(0x060B1E),
    desk: hex(0x040714),
    page: hex(0x151727),
    page_edge: hex(0x3F383F),
    start_tab_active: hex(0x060B1E),
    rail: hex(0x050917),
    text: hex(0xFFCEAD),
    button: hex(0x1C1D2B),
    button_hover: hex(0x2B2833),
    base: hex(0x101324),
    faint: hex(0x101324),
    accent: hex(0x7D82D9),
    disabled_text: hex(0x826C66),
    stroke: hex(0x262431),
    sep: hex(0x1F1E2C),
    module_bg: hex(0x0D1122),
    strip: hex(0x050A1B),
    field_stroke: hex(0x3D363D),
    hover_light: hex(0x191A29),
    elt_bg: hex(0x121525),
    elt_stroke: hex(0x2B2833),
    heading_text: hex(0xFFEBDE),
    surface: hex(0x121525),
    accent_soft: Color32::from_rgba_premultiplied(27, 29, 48, 56),
    accent_text: hex(0xBEC0EC),
    spell_error: hex(0xFF6B6B),
};
/// Everforest: Omarchy theme.
pub const KIT_EVERFOREST: KitColors = KitColors {
    window: hex(0x2D353B),
    list_bg: hex(0x2D353B),
    desk: hex(0x1D2226),
    page: hex(0x373E42),
    page_edge: hex(0x535655),
    start_tab_active: hex(0x2D353B),
    rail: hex(0x23292E),
    text: hex(0xD3C6AA),
    button: hex(0x3C4245),
    button_hover: hex(0x464B4C),
    base: hex(0x343B3F),
    faint: hex(0x343B3F),
    accent: hex(0x7FBBB3),
    disabled_text: hex(0x807E72),
    stroke: hex(0x434849),
    sep: hex(0x3E4446),
    module_bg: hex(0x32393E),
    strip: hex(0x283035),
    field_stroke: hex(0x525553),
    hover_light: hex(0x394043),
    elt_bg: hex(0x353C41),
    elt_stroke: hex(0x464B4C),
    heading_text: hex(0xEDE8DD),
    surface: hex(0x353C41),
    accent_soft: Color32::from_rgba_premultiplied(28, 41, 39, 56),
    accent_text: hex(0xBFDDD9),
    spell_error: hex(0xFF6B6B),
};
/// Flexoki Light: Omarchy theme.
pub const KIT_FLEXOKI_LIGHT: KitColors = KitColors {
    window: hex(0xFFFCF0),
    list_bg: hex(0xFFFCF0),
    desk: hex(0xEDEADF),
    page: hex(0xFFFEF9),
    page_edge: hex(0xE0E0DB),
    start_tab_active: hex(0xFFFCF0),
    rail: hex(0xF6F3E8),
    text: hex(0x100F0F),
    button: hex(0xE9E7DC),
    button_hover: hex(0xDBD8CE),
    base: hex(0xFFFEFB),
    faint: hex(0xFFFEFB),
    accent: hex(0x205EA6),
    disabled_text: hex(0x888680),
    stroke: hex(0xE0DDD3),
    sep: hex(0xE7E4DA),
    module_bg: hex(0xFFFEF8),
    strip: hex(0xFAF7EB),
    field_stroke: hex(0xCAC8BE),
    hover_light: hex(0xEDEADF),
    elt_bg: hex(0xF3F0E5),
    elt_stroke: hex(0xDBD8CE),
    heading_text: hex(0x090808),
    surface: hex(0xF3F0E5),
    accent_soft: Color32::from_rgba_premultiplied(7, 21, 36, 56),
    accent_text: hex(0x102F53),
    spell_error: hex(0xC62F2F),
};
/// Gruvbox: Omarchy theme.
pub const KIT_GRUVBOX: KitColors = KitColors {
    window: hex(0x282828),
    list_bg: hex(0x282828),
    desk: hex(0x1A1A1A),
    page: hex(0x32312F),
    page_edge: hex(0x4F4A42),
    start_tab_active: hex(0x282828),
    rail: hex(0x1F1F1F),
    text: hex(0xD4BE98),
    button: hex(0x373632),
    button_hover: hex(0x423E39),
    base: hex(0x2F2E2C),
    faint: hex(0x2F2E2C),
    accent: hex(0x7DAEA3),
    disabled_text: hex(0x7E7360),
    stroke: hex(0x3E3C37),
    sep: hex(0x393733),
    module_bg: hex(0x2D2C2B),
    strip: hex(0x242424),
    field_stroke: hex(0x4E4941),
    hover_light: hex(0x353330),
    elt_bg: hex(0x31302E),
    elt_stroke: hex(0x423E39),
    heading_text: hex(0xEEE5D6),
    surface: hex(0x31302E),
    accent_soft: Color32::from_rgba_premultiplied(27, 38, 36, 56),
    accent_text: hex(0xBED6D1),
    spell_error: hex(0xFF6B6B),
};
/// Hackerman: Omarchy theme.
pub const KIT_HACKERMAN: KitColors = KitColors {
    window: hex(0x0B0C16),
    list_bg: hex(0x0B0C16),
    desk: hex(0x07080E),
    page: hex(0x181A24),
    page_edge: hex(0x3B424B),
    start_tab_active: hex(0x0B0C16),
    rail: hex(0x090911),
    text: hex(0xDDF7FF),
    button: hex(0x1E212B),
    button_hover: hex(0x2A2F39),
    base: hex(0x13151F),
    faint: hex(0x13151F),
    accent: hex(0x82FB9C),
    disabled_text: hex(0x74828A),
    stroke: hex(0x262B34),
    sep: hex(0x20242D),
    module_bg: hex(0x11131D),
    strip: hex(0x0A0B14),
    field_stroke: hex(0x394049),
    hover_light: hex(0x1B1E27),
    elt_bg: hex(0x161822),
    elt_stroke: hex(0x2A2F39),
    heading_text: hex(0xF1FCFF),
    surface: hex(0x161822),
    accent_soft: Color32::from_rgba_premultiplied(29, 55, 34, 56),
    accent_text: hex(0xC0FDCE),
    spell_error: hex(0xFF6B6B),
};
/// Jade Horse: Omarchy theme.
pub const KIT_JADE_HORSE: KitColors = KitColors {
    window: hex(0x142620),
    list_bg: hex(0x142620),
    desk: hex(0x0D1915),
    page: hex(0x20322C),
    page_edge: hex(0x42544D),
    start_tab_active: hex(0x142620),
    rail: hex(0x101E19),
    text: hex(0xDFEDE5),
    button: hex(0x263832),
    button_hover: hex(0x32443E),
    base: hex(0x1C2E28),
    faint: hex(0x1C2E28),
    accent: hex(0x8BCBB1),
    disabled_text: hex(0x7A8A82),
    stroke: hex(0x2E403A),
    sep: hex(0x283A34),
    module_bg: hex(0x1A2C26),
    strip: hex(0x12221D),
    field_stroke: hex(0x41524B),
    hover_light: hex(0x23352F),
    elt_bg: hex(0x1E302A),
    elt_stroke: hex(0x32443E),
    heading_text: hex(0xF2F8F5),
    surface: hex(0x1E302A),
    accent_soft: Color32::from_rgba_premultiplied(31, 45, 39, 56),
    accent_text: hex(0xC5E5D8),
    spell_error: hex(0xFF6B6B),
};
/// Kanagawa: Omarchy theme.
pub const KIT_KANAGAWA: KitColors = KitColors {
    window: hex(0x1F1F28),
    list_bg: hex(0x1F1F28),
    desk: hex(0x14141A),
    page: hex(0x2A2A31),
    page_edge: hex(0x4A494A),
    start_tab_active: hex(0x1F1F28),
    rail: hex(0x18181F),
    text: hex(0xDCD7BA),
    button: hex(0x303035),
    button_hover: hex(0x3B3B3E),
    base: hex(0x27262E),
    faint: hex(0x27262E),
    accent: hex(0x7E9CD8),
    disabled_text: hex(0x7E7B71),
    stroke: hex(0x38373B),
    sep: hex(0x323137),
    module_bg: hex(0x25252C),
    strip: hex(0x1C1C24),
    field_stroke: hex(0x494748),
    hover_light: hex(0x2D2D33),
    elt_bg: hex(0x28282F),
    elt_stroke: hex(0x3B3B3E),
    heading_text: hex(0xF1EFE3),
    surface: hex(0x28282F),
    accent_soft: Color32::from_rgba_premultiplied(28, 34, 47, 56),
    accent_text: hex(0xBECEEC),
    spell_error: hex(0xFF6B6B),
};
/// Last Horizon: Omarchy theme.
pub const KIT_LAST_HORIZON: KitColors = KitColors {
    window: hex(0x0C0B0C),
    list_bg: hex(0x0C0B0C),
    desk: hex(0x080708),
    page: hex(0x1A191A),
    page_edge: hex(0x424242),
    start_tab_active: hex(0x0C0B0C),
    rail: hex(0x090909),
    text: hex(0xFAFCFB),
    button: hex(0x212122),
    button_hover: hex(0x302F30),
    base: hex(0x161516),
    faint: hex(0x161516),
    accent: hex(0xB59790),
    disabled_text: hex(0x838484),
    stroke: hex(0x2B2A2B),
    sep: hex(0x242324),
    module_bg: hex(0x131213),
    strip: hex(0x0B0A0B),
    field_stroke: hex(0x404041),
    hover_light: hex(0x1E1D1E),
    elt_bg: hex(0x181718),
    elt_stroke: hex(0x302F30),
    heading_text: hex(0xFDFEFD),
    surface: hex(0x181718),
    accent_soft: Color32::from_rgba_premultiplied(40, 33, 32, 56),
    accent_text: hex(0xDACBC8),
    spell_error: hex(0xFF6B6B),
};
/// Lumon: Omarchy theme.
pub const KIT_LUMON: KitColors = KitColors {
    window: hex(0x16242D),
    list_bg: hex(0x16242D),
    desk: hex(0x0E171D),
    page: hex(0x222F39),
    page_edge: hex(0x424F5A),
    start_tab_active: hex(0x16242D),
    rail: hex(0x111C23),
    text: hex(0xD6E2EE),
    button: hex(0x27353E),
    button_hover: hex(0x33404A),
    base: hex(0x1E2C35),
    faint: hex(0x1E2C35),
    accent: hex(0x8BC9EB),
    disabled_text: hex(0x76838E),
    stroke: hex(0x2F3D46),
    sep: hex(0x293740),
    module_bg: hex(0x1C2A33),
    strip: hex(0x142028),
    field_stroke: hex(0x404E57),
    hover_light: hex(0x24323B),
    elt_bg: hex(0x202E37),
    elt_stroke: hex(0x33404A),
    heading_text: hex(0xEFF3F8),
    surface: hex(0x202E37),
    accent_soft: Color32::from_rgba_premultiplied(31, 44, 52, 56),
    accent_text: hex(0xC5E4F5),
    spell_error: hex(0xFF6B6B),
};
/// Lupine: Omarchy theme.
pub const KIT_LUPINE: KitColors = KitColors {
    window: hex(0xFAFAFA),
    list_bg: hex(0xFAFAFA),
    desk: hex(0xE8E8E8),
    page: hex(0xFDFDFD),
    page_edge: hex(0xDFDFDF),
    start_tab_active: hex(0xFAFAFA),
    rail: hex(0xF1F1F1),
    text: hex(0x212121),
    button: hex(0xE6E6E6),
    button_hover: hex(0xD9D9D9),
    base: hex(0xFEFEFE),
    faint: hex(0xFEFEFE),
    accent: hex(0x3264EB),
    disabled_text: hex(0x8E8E8E),
    stroke: hex(0xDEDEDE),
    sep: hex(0xE4E4E4),
    module_bg: hex(0xFCFCFC),
    strip: hex(0xF5F5F5),
    field_stroke: hex(0xCACACA),
    hover_light: hex(0xEAEAEA),
    elt_bg: hex(0xEFEFEF),
    elt_stroke: hex(0xD9D9D9),
    heading_text: hex(0x121212),
    surface: hex(0xEFEFEF),
    accent_soft: Color32::from_rgba_premultiplied(11, 22, 52, 56),
    accent_text: hex(0x193276),
    spell_error: hex(0xC62F2F),
};
/// Matte Black: Omarchy theme.
pub const KIT_MATTE_BLACK: KitColors = KitColors {
    window: hex(0x121212),
    list_bg: hex(0x121212),
    desk: hex(0x0C0C0C),
    page: hex(0x1C1C1C),
    page_edge: hex(0x393939),
    start_tab_active: hex(0x121212),
    rail: hex(0x0E0E0E),
    text: hex(0xBEBEBE),
    button: hex(0x212121),
    button_hover: hex(0x2C2C2C),
    base: hex(0x191919),
    faint: hex(0x191919),
    accent: hex(0xE68E0D),
    disabled_text: hex(0x686868),
    stroke: hex(0x282828),
    sep: hex(0x232323),
    module_bg: hex(0x171717),
    strip: hex(0x101010),
    field_stroke: hex(0x383838),
    hover_light: hex(0x1F1F1F),
    elt_bg: hex(0x1B1B1B),
    elt_stroke: hex(0x2C2C2C),
    heading_text: hex(0xE5E5E5),
    surface: hex(0x1B1B1B),
    accent_soft: Color32::from_rgba_premultiplied(51, 31, 3, 56),
    accent_text: hex(0xF2C686),
    spell_error: hex(0xFF6B6B),
};
/// Matcha: Omarchy theme.
pub const KIT_MATCHA: KitColors = KitColors {
    window: hex(0x222222),
    list_bg: hex(0x222222),
    desk: hex(0x161616),
    page: hex(0x2C2C2B),
    page_edge: hex(0x474743),
    start_tab_active: hex(0x222222),
    rail: hex(0x1B1B1B),
    text: hex(0xC2C2B0),
    button: hex(0x30302F),
    button_hover: hex(0x3A3A37),
    base: hex(0x282828),
    faint: hex(0x282828),
    accent: hex(0x78824B),
    disabled_text: hex(0x727269),
    stroke: hex(0x373734),
    sep: hex(0x323230),
    module_bg: hex(0x272726),
    strip: hex(0x1F1F1F),
    field_stroke: hex(0x454541),
    hover_light: hex(0x2E2E2D),
    elt_bg: hex(0x2A2A29),
    elt_stroke: hex(0x3A3A37),
    heading_text: hex(0xE7E7DF),
    surface: hex(0x2A2A29),
    accent_soft: Color32::from_rgba_premultiplied(26, 29, 16, 56),
    accent_text: hex(0xBCC0A5),
    spell_error: hex(0xFF6B6B),
};
/// Nord: Omarchy theme.
pub const KIT_NORD: KitColors = KitColors {
    window: hex(0x2E3440),
    list_bg: hex(0x2E3440),
    desk: hex(0x1E222A),
    page: hex(0x383E4A),
    page_edge: hex(0x555B67),
    start_tab_active: hex(0x2E3440),
    rail: hex(0x242932),
    text: hex(0xD8DEE9),
    button: hex(0x3D434F),
    button_hover: hex(0x484E59),
    base: hex(0x353B47),
    faint: hex(0x353B47),
    accent: hex(0x81A1C1),
    disabled_text: hex(0x838994),
    stroke: hex(0x444A56),
    sep: hex(0x3F4551),
    module_bg: hex(0x333945),
    strip: hex(0x292F3A),
    field_stroke: hex(0x535965),
    hover_light: hex(0x3B414D),
    elt_bg: hex(0x363C48),
    elt_stroke: hex(0x484E59),
    heading_text: hex(0xEFF2F6),
    surface: hex(0x363C48),
    accent_soft: Color32::from_rgba_premultiplied(28, 35, 42, 56),
    accent_text: hex(0xC0D0E0),
    spell_error: hex(0xFF6B6B),
};
/// Retro 82: Omarchy theme.
pub const KIT_RETRO_82: KitColors = KitColors {
    window: hex(0x05182E),
    list_bg: hex(0x05182E),
    desk: hex(0x03101E),
    page: hex(0x132436),
    page_edge: hex(0x3C454B),
    start_tab_active: hex(0x05182E),
    rail: hex(0x041324),
    text: hex(0xF6DCAC),
    button: hex(0x1B2A39),
    button_hover: hex(0x293541),
    base: hex(0x0F2033),
    faint: hex(0x0F2033),
    accent: hex(0xFAA968),
    disabled_text: hex(0x7E7A6D),
    stroke: hex(0x24313E),
    sep: hex(0x1D2C3B),
    module_bg: hex(0x0C1E32),
    strip: hex(0x041629),
    field_stroke: hex(0x3A434A),
    hover_light: hex(0x172737),
    elt_bg: hex(0x112234),
    elt_stroke: hex(0x293541),
    heading_text: hex(0xFBF1DE),
    surface: hex(0x112234),
    accent_soft: Color32::from_rgba_premultiplied(55, 37, 23, 56),
    accent_text: hex(0xFCD4B4),
    spell_error: hex(0xFF6B6B),
};
/// Ristretto: Omarchy theme.
pub const KIT_RISTRETTO: KitColors = KitColors {
    window: hex(0x2C2525),
    list_bg: hex(0x2C2525),
    desk: hex(0x1D1818),
    page: hex(0x373030),
    page_edge: hex(0x564E4F),
    start_tab_active: hex(0x2C2525),
    rail: hex(0x221D1D),
    text: hex(0xE6D9DB),
    button: hex(0x3D3535),
    button_hover: hex(0x484040),
    base: hex(0x332C2C),
    faint: hex(0x332C2C),
    accent: hex(0xF38D70),
    disabled_text: hex(0x897F80),
    stroke: hex(0x443C3D),
    sep: hex(0x3F3737),
    module_bg: hex(0x322A2A),
    strip: hex(0x282121),
    field_stroke: hex(0x554D4D),
    hover_light: hex(0x3A3233),
    elt_bg: hex(0x352E2E),
    elt_stroke: hex(0x484040),
    heading_text: hex(0xF5F0F1),
    surface: hex(0x352E2E),
    accent_soft: Color32::from_rgba_premultiplied(53, 31, 25, 56),
    accent_text: hex(0xF9C6B8),
    spell_error: hex(0xFF6B6B),
};
/// Rosé Pine: Omarchy theme.
pub const KIT_ROSE_PINE: KitColors = KitColors {
    window: hex(0xFAF4ED),
    list_bg: hex(0xFAF4ED),
    desk: hex(0xE8E3DC),
    page: hex(0xFDFBF8),
    page_edge: hex(0xDFDDDA),
    start_tab_active: hex(0xFAF4ED),
    rail: hex(0xF1EBE5),
    text: hex(0x534F74),
    button: hex(0xEBE5E3),
    button_hover: hex(0xE2DCDC),
    base: hex(0xFEFCFA),
    faint: hex(0xFEFCFA),
    accent: hex(0x56949F),
    disabled_text: hex(0x908BA2),
    stroke: hex(0xE5DFDE),
    sep: hex(0xEAE4E1),
    module_bg: hex(0xFCFAF6),
    strip: hex(0xF5EFE8),
    field_stroke: hex(0xD6D0D3),
    hover_light: hex(0xEEE8E4),
    elt_bg: hex(0xF2ECE7),
    elt_stroke: hex(0xE2DCDC),
    heading_text: hex(0x2E2B40),
    surface: hex(0xF2ECE7),
    accent_soft: Color32::from_rgba_premultiplied(19, 33, 35, 56),
    accent_text: hex(0x2B4A50),
    spell_error: hex(0xC62F2F),
};
/// Solitude: Omarchy theme.
pub const KIT_SOLITUDE: KitColors = KitColors {
    window: hex(0x101315),
    list_bg: hex(0x101315),
    desk: hex(0x0A0C0E),
    page: hex(0x1B1E20),
    page_edge: hex(0x3A3D3F),
    start_tab_active: hex(0x101315),
    rail: hex(0x0C0F10),
    text: hex(0xCACCCC),
    button: hex(0x212425),
    button_hover: hex(0x2C2F30),
    base: hex(0x171A1C),
    faint: hex(0x171A1C),
    accent: hex(0x798186),
    disabled_text: hex(0x6D7070),
    stroke: hex(0x282B2D),
    sep: hex(0x232627),
    module_bg: hex(0x16191A),
    strip: hex(0x0E1113),
    field_stroke: hex(0x393C3D),
    hover_light: hex(0x1E2123),
    elt_bg: hex(0x191C1E),
    elt_stroke: hex(0x2C2F30),
    heading_text: hex(0xEAEBEB),
    surface: hex(0x191C1E),
    accent_soft: Color32::from_rgba_premultiplied(27, 28, 29, 56),
    accent_text: hex(0xBCC0C2),
    spell_error: hex(0xFF6B6B),
};
/// Starship: Omarchy theme.
pub const KIT_STARSHIP: KitColors = KitColors {
    window: hex(0x1C1C1E),
    list_bg: hex(0x1C1C1E),
    desk: hex(0x121214),
    page: hex(0x29292B),
    page_edge: hex(0x4E4E50),
    start_tab_active: hex(0x1C1C1E),
    rail: hex(0x161617),
    text: hex(0xF5F5F7),
    button: hex(0x303032),
    button_hover: hex(0x3D3D3F),
    base: hex(0x252527),
    faint: hex(0x252527),
    accent: hex(0xFF9F0A),
    disabled_text: hex(0x88888A),
    stroke: hex(0x38383A),
    sep: hex(0x323234),
    module_bg: hex(0x232325),
    strip: hex(0x19191B),
    field_stroke: hex(0x4C4C4E),
    hover_light: hex(0x2C2C2E),
    elt_bg: hex(0x272729),
    elt_stroke: hex(0x3D3D3F),
    heading_text: hex(0xFBFBFC),
    surface: hex(0x272729),
    accent_soft: Color32::from_rgba_premultiplied(56, 35, 2, 56),
    accent_text: hex(0xFFCF84),
    spell_error: hex(0xFF6B6B),
};
/// Tokyo Night: Omarchy theme.
pub const KIT_TOKYO_NIGHT: KitColors = KitColors {
    window: hex(0x1A1B26),
    list_bg: hex(0x1A1B26),
    desk: hex(0x111219),
    page: hex(0x232431),
    page_edge: hex(0x3B3D4F),
    start_tab_active: hex(0x1A1B26),
    rail: hex(0x14151E),
    text: hex(0xA9B1D6),
    button: hex(0x272836),
    button_hover: hex(0x2F3240),
    base: hex(0x20212D),
    faint: hex(0x20212D),
    accent: hex(0x7AA2F7),
    disabled_text: hex(0x62667E),
    stroke: hex(0x2D2E3D),
    sep: hex(0x282A38),
    module_bg: hex(0x1E202B),
    strip: hex(0x171822),
    field_stroke: hex(0x393C4D),
    hover_light: hex(0x252633),
    elt_bg: hex(0x21222F),
    elt_stroke: hex(0x2F3240),
    heading_text: hex(0xDDE0EF),
    surface: hex(0x21222F),
    accent_soft: Color32::from_rgba_premultiplied(27, 36, 54, 56),
    accent_text: hex(0xBCD0FB),
    spell_error: hex(0xFF6B6B),
};
/// Vantablack: Omarchy theme.
pub const KIT_VANTABLACK: KitColors = KitColors {
    window: hex(0x000000),
    list_bg: hex(0x000000),
    desk: hex(0x000000),
    page: hex(0x0F0F0F),
    page_edge: hex(0x3A3A3A),
    start_tab_active: hex(0x000000),
    rail: hex(0x000000),
    text: hex(0xFFFFFF),
    button: hex(0x171717),
    button_hover: hex(0x262626),
    base: hex(0x0A0A0A),
    faint: hex(0x0A0A0A),
    accent: hex(0x8D8D8D),
    disabled_text: hex(0x808080),
    stroke: hex(0x212121),
    sep: hex(0x1A1A1A),
    module_bg: hex(0x080808),
    strip: hex(0x000000),
    field_stroke: hex(0x383838),
    hover_light: hex(0x131313),
    elt_bg: hex(0x0D0D0D),
    elt_stroke: hex(0x262626),
    heading_text: hex(0xFFFFFF),
    surface: hex(0x0D0D0D),
    accent_soft: Color32::from_rgba_premultiplied(31, 31, 31, 56),
    accent_text: hex(0xC6C6C6),
    spell_error: hex(0xFF6B6B),
};
/// White: Omarchy theme.
pub const KIT_WHITE: KitColors = KitColors {
    window: hex(0xFFFFFF),
    list_bg: hex(0xFFFFFF),
    desk: hex(0xEDEDED),
    page: hex(0xFFFFFF),
    page_edge: hex(0xE0E0E0),
    start_tab_active: hex(0xFFFFFF),
    rail: hex(0xF6F6F6),
    text: hex(0x000000),
    button: hex(0xE8E8E8),
    button_hover: hex(0xD9D9D9),
    base: hex(0xFFFFFF),
    faint: hex(0xFFFFFF),
    accent: hex(0x6E6E6E),
    disabled_text: hex(0x808080),
    stroke: hex(0xDEDEDE),
    sep: hex(0xE6E6E6),
    module_bg: hex(0xFFFFFF),
    strip: hex(0xFAFAFA),
    field_stroke: hex(0xC7C7C7),
    hover_light: hex(0xECECEC),
    elt_bg: hex(0xF2F2F2),
    elt_stroke: hex(0xD9D9D9),
    heading_text: hex(0x000000),
    surface: hex(0xF2F2F2),
    accent_soft: Color32::from_rgba_premultiplied(24, 24, 24, 56),
    accent_text: hex(0x373737),
    spell_error: hex(0xC62F2F),
};

/// Sepia: warm paper for daylight writing, burnt-orange ink accent.
pub const KIT_SEPIA: KitColors = KitColors {
    window: hex(0xF4EDDF),
    list_bg: hex(0xF4EDDF),
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
    list_bg: hex(0x000000),
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

// ---- ScriptCraft's KIT imitation ---------------------------------------------

// KIT Scenarist's own theme editor exposes five colours: Text, Background,
// Folder Text, Folder Background and Non Printable Text. ScriptCraft has more
// semantic roles than that, so the five values set the surfaces, ink and
// accent; borders and raised surfaces are derived tones of the same hue.
//
// Light theme: background #fefefe, text #000000, folder text #fefefe, folder
// background #cbc6c3. A grey folder chip cannot double as the accent (1.1:1
// against the page), so the accent is a deepened, still-neutral tone of that
// folder grey. Non-printable green (#00c509) is the scene-number/secondary
// ink from KIT; it stays out of the palette fields because scene numbers must
// keep the theme's contrast floor.

/// KIT Scenarist, light: grey chrome (#e4e4e4) with white content ink, KIT
/// blue (#2b78da) accent and Courier Prime's paper-white page (#fefefe).
/// Every value here is the KIT Scenarist look sampled from its own editor and
/// PDF export: the toolbar, module rail and panels are grey, the navigator
/// list and the script page are white, and the one accent is the box-blue
/// KIT uses for selections.
pub const KIT_SCENARIST: KitColors = KitColors {
    window: hex(0xE4E4E4),
    list_bg: hex(0xFFFFFF),
    desk: hex(0xF2F2F2),
    page: hex(0xFEFEFE),
    page_edge: hex(0xBDBEBF),
    start_tab_active: hex(0xE4E4E4),
    rail: hex(0xE4E4E4),
    text: hex(0x000000),
    button: hex(0xE4E4E4),
    button_hover: hex(0xD6D6D6),
    base: hex(0xFFFFFF),
    faint: hex(0xF6F6F6),
    accent: hex(0x2B78DA),
    disabled_text: hex(0x6B6B6B),
    stroke: hex(0xBDBEBF),
    sep: hex(0xD0D0D0),
    module_bg: hex(0xFEFEFE),
    strip: hex(0xE4E4E4),
    field_stroke: hex(0xBDBEBF),
    hover_light: hex(0xDCDCDC),
    elt_bg: hex(0xEFEFEF),
    elt_stroke: hex(0xBDBEBF),
    heading_text: hex(0x000000),
    surface: hex(0xF6F6F6),
    accent_soft: Color32::from_rgba_premultiplied(8, 24, 43, 50),
    accent_text: hex(0x1E5FA8),
    spell_error: hex(0xC62F2F),
};

/// KIT Scenarist, dark: grey chrome (#414244) with dark content ink, KIT
/// blue (#2b78da) selections and the paper-dark page (#3d3d3d). The blue is
/// lifted to #4c9aff for UI accents — on the dark page #2b78da only clears
/// the 3:1 accent floor barely at best — while the selected row keeps the
/// box-blue KIT paints.
pub const KIT_SCENARIST_DARK: KitColors = KitColors {
    window: hex(0x414244),
    list_bg: hex(0x404040),
    desk: hex(0x404040),
    page: hex(0x3D3D3D),
    page_edge: hex(0x26282A),
    start_tab_active: hex(0x414244),
    rail: hex(0x414244),
    text: hex(0xEBEBEB),
    button: hex(0x414244),
    button_hover: hex(0x4D4F52),
    base: hex(0x3D3D3D),
    faint: hex(0x38393B),
    accent: hex(0x4C9AFF),
    disabled_text: hex(0xA3A4A6),
    stroke: hex(0x696765),
    sep: hex(0x2A2C2E),
    module_bg: hex(0x3D3D3D),
    strip: hex(0x38393B),
    field_stroke: hex(0x696765),
    hover_light: hex(0x4A4C4E),
    elt_bg: hex(0x414244),
    elt_stroke: hex(0x696765),
    heading_text: hex(0xEBEBEB),
    surface: hex(0x414244),
    accent_soft: Color32::from_rgba_premultiplied(19, 39, 64, 64),
    accent_text: hex(0x8FC0F7),
    spell_error: hex(0xFF6B6B),
};

/// ScriptCraft's own palette set grows with the writer-facing editor themes:
/// Midnight Ink, Amber Projector, Noir Emerald, Velvet Curtain, Red Light and
/// Paper and Rust, added from the design brief that ships with the app. Every
/// value below follows the same semantic roles as Midnight / Light / the KIT
/// imitation, so widgets need no per-theme branch.
/// Midnight Ink: cool, calm, closest to the current Midnight look.
pub const KIT_MIDNIGHT_INK: KitColors = KitColors {
    window: hex(0x0E1117),
    list_bg: hex(0x151A23),
    desk: hex(0x151A23),
    page: hex(0x1B212C),
    page_edge: hex(0x2A3140),
    start_tab_active: hex(0x0E1117),
    rail: hex(0x0E1117),
    text: hex(0xE6E9EF),
    button: hex(0x1A1F28),
    button_hover: hex(0x242B37),
    base: hex(0x151A23),
    faint: hex(0x151A23),
    accent: hex(0x5B8CFF),
    disabled_text: hex(0x7C8596),
    stroke: hex(0x232936),
    sep: hex(0x1B2029),
    module_bg: hex(0x0E1117),
    strip: hex(0x12161D),
    field_stroke: hex(0x2A3140),
    hover_light: hex(0x1A1F28),
    elt_bg: hex(0x161B24),
    elt_stroke: hex(0x2A3140),
    heading_text: hex(0xE6E9EF),
    surface: hex(0x161B24),
    accent_soft: Color32::from_rgba_premultiplied(91, 140, 255, 51),
    accent_text: hex(0xB9D0FF),
    spell_error: hex(0xFF6B6B),
};

/// Amber Projector: warm, like light through a film gate.
pub const KIT_AMBER_PROJECTOR: KitColors = KitColors {
    window: hex(0x14110F),
    list_bg: hex(0x1C1814),
    desk: hex(0x1C1814),
    page: hex(0x231E19),
    page_edge: hex(0x332B20),
    start_tab_active: hex(0x14110F),
    rail: hex(0x14110F),
    text: hex(0xEDE4D6),
    button: hex(0x1E1A16),
    button_hover: hex(0x2A241D),
    base: hex(0x1C1814),
    faint: hex(0x1C1814),
    accent: hex(0xF2A541),
    disabled_text: hex(0x8A7D6B),
    stroke: hex(0x2E251D),
    sep: hex(0x211B15),
    module_bg: hex(0x14110F),
    strip: hex(0x171210),
    field_stroke: hex(0x332B20),
    hover_light: hex(0x201B15),
    elt_bg: hex(0x1B1712),
    elt_stroke: hex(0x332B20),
    heading_text: hex(0xEDE4D6),
    surface: hex(0x1B1712),
    accent_soft: Color32::from_rgba_premultiplied(242, 165, 65, 51),
    accent_text: hex(0xFFC37A),
    spell_error: hex(0xFF6B6B),
};

/// Noir Emerald: moody green, easy on the eyes at night.
pub const KIT_NOIR_EMERALD: KitColors = KitColors {
    window: hex(0x0B1210),
    list_bg: hex(0x111A17),
    desk: hex(0x111A17),
    page: hex(0x16221E),
    page_edge: hex(0x1F312C),
    start_tab_active: hex(0x0B1210),
    rail: hex(0x0B1210),
    text: hex(0xDDEBE5),
    button: hex(0x0F1815),
    button_hover: hex(0x152019),
    base: hex(0x111A17),
    faint: hex(0x111A17),
    accent: hex(0x3DDBA4),
    disabled_text: hex(0x6F8A80),
    stroke: hex(0x1B2925),
    sep: hex(0x0E1714),
    module_bg: hex(0x0B1210),
    strip: hex(0x0D1412),
    field_stroke: hex(0x1F312C),
    hover_light: hex(0x101A17),
    elt_bg: hex(0x0E1814),
    elt_stroke: hex(0x1F312C),
    heading_text: hex(0xDDEBE5),
    surface: hex(0x0E1814),
    accent_soft: Color32::from_rgba_premultiplied(61, 219, 164, 51),
    accent_text: hex(0x8FE8CC),
    spell_error: hex(0xFF6B6B),
};

/// Velvet Curtain: theatre plum with a crimson accent.
pub const KIT_VELVET_CURTAIN: KitColors = KitColors {
    window: hex(0x120D14),
    list_bg: hex(0x1A1320),
    desk: hex(0x1A1320),
    page: hex(0x221829),
    page_edge: hex(0x2F2140),
    start_tab_active: hex(0x120D14),
    rail: hex(0x120D14),
    text: hex(0xEBDFF0),
    button: hex(0x17101C),
    button_hover: hex(0x211628),
    base: hex(0x1A1320),
    faint: hex(0x1A1320),
    accent: hex(0xE5566D),
    disabled_text: hex(0x8B7A95),
    stroke: hex(0x2A1E30),
    sep: hex(0x140E18),
    module_bg: hex(0x120D14),
    strip: hex(0x150E18),
    field_stroke: hex(0x2F2140),
    hover_light: hex(0x18111F),
    elt_bg: hex(0x160F1B),
    elt_stroke: hex(0x2F2140),
    heading_text: hex(0xEBDFF0),
    surface: hex(0x160F1B),
    accent_soft: Color32::from_rgba_premultiplied(229, 86, 109, 51),
    accent_text: hex(0xF2A7B4),
    spell_error: hex(0xFF6B6B),
};

/// Red Light: neutral graphite with one REC red accent.
pub const KIT_REDLIGHT: KitColors = KitColors {
    window: hex(0x0F0F10),
    list_bg: hex(0x171718),
    desk: hex(0x171718),
    page: hex(0x1E1E20),
    page_edge: hex(0x2B2B30),
    start_tab_active: hex(0x0F0F10),
    rail: hex(0x0F0F10),
    text: hex(0xECECEC),
    button: hex(0x17171A),
    button_hover: hex(0x202023),
    base: hex(0x171718),
    faint: hex(0x171718),
    accent: hex(0xFF5A4F),
    disabled_text: hex(0x85858A),
    stroke: hex(0x2A2A30),
    sep: hex(0x141416),
    module_bg: hex(0x0F0F10),
    strip: hex(0x121214),
    field_stroke: hex(0x2B2B30),
    hover_light: hex(0x17171A),
    elt_bg: hex(0x161618),
    elt_stroke: hex(0x2B2B30),
    heading_text: hex(0xECECEC),
    surface: hex(0x161618),
    accent_soft: Color32::from_rgba_premultiplied(255, 90, 79, 51),
    accent_text: hex(0xFF9B93),
    spell_error: hex(0xFF6B6B),
};

/// Paper and Rust: light theme, like a printed script.
pub const KIT_PAPER_RUST: KitColors = KitColors {
    window: hex(0xE9E4DA),
    list_bg: hex(0xF2EEE5),
    desk: hex(0xF2EEE5),
    page: hex(0xFBF9F4),
    page_edge: hex(0xD8D0C0),
    start_tab_active: hex(0xE9E4DA),
    rail: hex(0xE9E4DA),
    text: hex(0x2A2622),
    button: hex(0xF2EEE5),
    button_hover: hex(0xE7E1D6),
    base: hex(0xFBF9F4),
    faint: hex(0xF2EEE5),
    accent: hex(0xB5532A),
    disabled_text: hex(0x7E7669),
    stroke: hex(0xD8D0C0),
    sep: hex(0xDDD6C9),
    module_bg: hex(0xFBF9F4),
    strip: hex(0xF2EEE5),
    field_stroke: hex(0xD8D0C0),
    hover_light: hex(0xF2EEE5),
    elt_bg: hex(0xF2EEE5),
    elt_stroke: hex(0xD8D0C0),
    heading_text: hex(0x2A2622),
    surface: hex(0xFBF9F4),
    accent_soft: Color32::from_rgba_premultiplied(181, 83, 42, 51),
    accent_text: hex(0x8F3E1F),
    spell_error: hex(0xC62F2F),
};

/// User-selectable theme. Persisted by name in `settings.conf`; the old
/// `dark_mode` setting is still honoured as a fallback.
///
/// `ALL` is the Settings dropdown order: ScriptCraft's own palettes, then the
/// themed palettes. The dropdown shows one heading per [`ThemeGroup`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    Midnight,
    Light,
    Sepia,
    HighContrast,
    KitScenarist,
    KitScenaristDark,
    MidnightInk,
    AmberProjector,
    NoirEmerald,
    VelvetCurtain,
    RedLight,
    PaperAndRust,
    Catppuccin,
    CatppuccinLatte,
    Ethereal,
    Everforest,
    FlexokiLight,
    Gruvbox,
    Hackerman,
    JadeHorse,
    Kanagawa,
    LastHorizon,
    Lumon,
    Lupine,
    MatteBlack,
    Matcha,
    Nord,
    Retro82,
    Ristretto,
    RosePine,
    Solitude,
    Starship,
    TokyoNight,
    Vantablack,
    White,
}

/// Family a theme belongs to. The Settings dropdown renders a heading per
/// group, so a long list stays navigable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeGroup {
    ScriptCraft,
    Omarchy,
}

impl ThemeGroup {
    pub const ALL: [ThemeGroup; 2] = [ThemeGroup::ScriptCraft, ThemeGroup::Omarchy];

    pub fn label(self) -> &'static str {
        match self {
            ThemeGroup::ScriptCraft => "ScriptCraft",
            ThemeGroup::Omarchy => "Omarchy",
        }
    }

    /// Themes in this group, in dropdown order.
    pub fn themes(self) -> &'static [Theme] {
        match self {
            ThemeGroup::ScriptCraft => &Theme::SCRIPT,
            ThemeGroup::Omarchy => &Theme::OMARCHY,
        }
    }
}

impl Theme {
    /// ScriptCraft's own palettes, including a light imitation of KIT
    /// Scenarist's near-white paper and neutral chrome, plus its dark sibling.
    pub const SCRIPT: [Theme; 12] = [
        Theme::Midnight,
        Theme::Light,
        Theme::Sepia,
        Theme::HighContrast,
        Theme::KitScenarist,
        Theme::KitScenaristDark,
        Theme::MidnightInk,
        Theme::AmberProjector,
        Theme::NoirEmerald,
        Theme::VelvetCurtain,
        Theme::RedLight,
        Theme::PaperAndRust,
    ];

    /// The 23 base themes Omarchy ships in `themes/` (basecamp/omarchy).
    /// Palettes are imported rather than approximated; the order is the
    /// folder order, with the four we carried before in their same places.
    pub const OMARCHY: [Theme; 23] = [
        Theme::Catppuccin,
        Theme::CatppuccinLatte,
        Theme::Ethereal,
        Theme::Everforest,
        Theme::FlexokiLight,
        Theme::Gruvbox,
        Theme::Hackerman,
        Theme::JadeHorse,
        Theme::Kanagawa,
        Theme::LastHorizon,
        Theme::Lumon,
        Theme::Lupine,
        Theme::MatteBlack,
        Theme::Matcha,
        Theme::Nord,
        Theme::Retro82,
        Theme::Ristretto,
        Theme::RosePine,
        Theme::Solitude,
        Theme::Starship,
        Theme::TokyoNight,
        Theme::Vantablack,
        Theme::White,
    ];

    pub const ALL: [Theme; 35] = [
        Theme::Midnight,
        Theme::Light,
        Theme::Sepia,
        Theme::HighContrast,
        Theme::KitScenarist,
        Theme::KitScenaristDark,
        Theme::MidnightInk,
        Theme::AmberProjector,
        Theme::NoirEmerald,
        Theme::VelvetCurtain,
        Theme::RedLight,
        Theme::PaperAndRust,
        Theme::Catppuccin,
        Theme::CatppuccinLatte,
        Theme::Ethereal,
        Theme::Everforest,
        Theme::FlexokiLight,
        Theme::Gruvbox,
        Theme::Hackerman,
        Theme::JadeHorse,
        Theme::Kanagawa,
        Theme::LastHorizon,
        Theme::Lumon,
        Theme::Lupine,
        Theme::MatteBlack,
        Theme::Matcha,
        Theme::Nord,
        Theme::Retro82,
        Theme::Ristretto,
        Theme::RosePine,
        Theme::Solitude,
        Theme::Starship,
        Theme::TokyoNight,
        Theme::Vantablack,
        Theme::White,
    ];

    /// Group this theme is listed under in Settings.
    pub fn group(self) -> ThemeGroup {
        if Self::SCRIPT.contains(&self) {
            ThemeGroup::ScriptCraft
        } else {
            ThemeGroup::Omarchy
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Theme::Midnight => "Midnight",
            Theme::Light => "Light",
            Theme::Sepia => "Sepia",
            Theme::HighContrast => "High Contrast",
            Theme::KitScenarist => "KIT Scenarist",
            Theme::KitScenaristDark => "KIT Scenarist Dark",
            Theme::MidnightInk => "Midnight Ink",
            Theme::AmberProjector => "Amber Projector",
            Theme::NoirEmerald => "Noir Emerald",
            Theme::VelvetCurtain => "Velvet Curtain",
            Theme::RedLight => "Red Light",
            Theme::PaperAndRust => "Paper and Rust",
            Theme::Catppuccin => "Catppuccin",
            Theme::CatppuccinLatte => "Catppuccin Latte",
            Theme::Ethereal => "Ethereal",
            Theme::Everforest => "Everforest",
            Theme::FlexokiLight => "Flexoki Light",
            Theme::Gruvbox => "Gruvbox",
            Theme::Hackerman => "Hackerman",
            Theme::JadeHorse => "Jade Horse",
            Theme::Kanagawa => "Kanagawa",
            Theme::LastHorizon => "Last Horizon",
            Theme::Lumon => "Lumon",
            Theme::Lupine => "Lupine",
            Theme::MatteBlack => "Matte Black",
            Theme::Matcha => "Matcha",
            Theme::Nord => "Nord",
            Theme::Retro82 => "Retro 82",
            Theme::Ristretto => "Ristretto",
            Theme::RosePine => "Rosé Pine",
            Theme::Solitude => "Solitude",
            Theme::Starship => "Starship",
            Theme::TokyoNight => "Tokyo Night",
            Theme::Vantablack => "Vantablack",
            Theme::White => "White",
        }
    }

    /// Stable key used in `settings.conf`.
    pub fn key(self) -> &'static str {
        match self {
            Theme::Midnight => "midnight",
            Theme::Light => "light",
            Theme::Sepia => "sepia",
            Theme::HighContrast => "high-contrast",
            Theme::KitScenarist => "kit-scenarist",
            Theme::KitScenaristDark => "kit-scenarist-dark",
            Theme::MidnightInk => "midnight-ink",
            Theme::AmberProjector => "amber-projector",
            Theme::NoirEmerald => "noir-emerald",
            Theme::VelvetCurtain => "velvet-curtain",
            Theme::RedLight => "red-light",
            Theme::PaperAndRust => "paper-and-rust",
            Theme::Catppuccin => "catppuccin",
            Theme::CatppuccinLatte => "catppuccin-latte",
            Theme::Ethereal => "ethereal",
            Theme::Everforest => "everforest",
            Theme::FlexokiLight => "flexoki-light",
            Theme::Gruvbox => "gruvbox",
            Theme::Hackerman => "hackerman",
            Theme::JadeHorse => "jade-horse",
            Theme::Kanagawa => "kanagawa",
            Theme::LastHorizon => "last-horizon",
            Theme::Lumon => "lumon",
            Theme::Lupine => "lupine",
            Theme::MatteBlack => "matte-black",
            Theme::Matcha => "matcha",
            Theme::Nord => "nord",
            Theme::Retro82 => "retro-82",
            Theme::Ristretto => "ristretto",
            Theme::RosePine => "rose-pine",
            Theme::Solitude => "solitude",
            Theme::Starship => "starship",
            Theme::TokyoNight => "tokyo-night",
            Theme::Vantablack => "vantablack",
            Theme::White => "white",
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
            Theme::Light => &KIT_LIGHT,
            Theme::Sepia => &KIT_SEPIA,
            Theme::HighContrast => &KIT_HIGH_CONTRAST,
            Theme::KitScenarist => &KIT_SCENARIST,
            Theme::KitScenaristDark => &KIT_SCENARIST_DARK,
            Theme::MidnightInk => &KIT_MIDNIGHT_INK,
            Theme::AmberProjector => &KIT_AMBER_PROJECTOR,
            Theme::NoirEmerald => &KIT_NOIR_EMERALD,
            Theme::VelvetCurtain => &KIT_VELVET_CURTAIN,
            Theme::RedLight => &KIT_REDLIGHT,
            Theme::PaperAndRust => &KIT_PAPER_RUST,
            Theme::Catppuccin => &KIT_CATPPUCCIN,
            Theme::CatppuccinLatte => &KIT_CATPPUCCIN_LATTE,
            Theme::Ethereal => &KIT_ETHEREAL,
            Theme::Everforest => &KIT_EVERFOREST,
            Theme::FlexokiLight => &KIT_FLEXOKI_LIGHT,
            Theme::Gruvbox => &KIT_GRUVBOX,
            Theme::Hackerman => &KIT_HACKERMAN,
            Theme::JadeHorse => &KIT_JADE_HORSE,
            Theme::Kanagawa => &KIT_KANAGAWA,
            Theme::LastHorizon => &KIT_LAST_HORIZON,
            Theme::Lumon => &KIT_LUMON,
            Theme::Lupine => &KIT_LUPINE,
            Theme::MatteBlack => &KIT_MATTE_BLACK,
            Theme::Matcha => &KIT_MATCHA,
            Theme::Nord => &KIT_NORD,
            Theme::Retro82 => &KIT_RETRO_82,
            Theme::Ristretto => &KIT_RISTRETTO,
            Theme::RosePine => &KIT_ROSE_PINE,
            Theme::Solitude => &KIT_SOLITUDE,
            Theme::Starship => &KIT_STARSHIP,
            Theme::TokyoNight => &KIT_TOKYO_NIGHT,
            Theme::Vantablack => &KIT_VANTABLACK,
            Theme::White => &KIT_WHITE,
        }
    }

    /// Light palettes need dark ink; everything in `kit_visuals` branches on
    /// this.
    pub fn is_light(self) -> bool {
        matches!(
            self,
            Theme::Light
                | Theme::Sepia
                | Theme::KitScenarist
                | Theme::PaperAndRust
                | Theme::CatppuccinLatte
                | Theme::FlexokiLight
                | Theme::Lupine
                | Theme::RosePine
                | Theme::White
        )
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
    let visuals = kit_visuals();
    // egui keeps one style per brightness and picks it from
    // `theme_preference`, which defaults to following the OS. Styling only the
    // active slot left the other one as stock egui: on a light-mode Windows
    // desktop every popup, menu and window came back white the moment the OS
    // reported its theme, even though the app's own palette is dark. Pin the
    // preference to the palette we just built, then style that slot.
    ctx.set_theme(if visuals.dark_mode {
        egui::ThemePreference::Dark
    } else {
        egui::ThemePreference::Light
    });
    ctx.set_visuals(visuals);
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
pub fn paint_page(painter: &Painter, rect: Rect, paper_mono: bool) {
    let c = kit();
    let a = if paper_mono {
        60
    } else if kit_is_light() {
        30
    } else {
        100
    };
    painter.add(
        Shadow {
            offset: [0, 4],
            blur: 14,
            spread: 0,
            color: Color32::from_black_alpha(a),
        }
        .as_shape(rect, 3.0),
    );
    let (fill, edge) = if paper_mono {
        (Color32::WHITE, Color32::from_gray(0xD5))
    } else {
        (c.page, c.page_edge)
    };
    painter.rect_filled(rect, 3.0, fill);
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0_f32, edge), StrokeKind::Inside);
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
    fn theme_groups_partition_all_themes() {
        let mut seen: Vec<Theme> = Vec::new();
        for group in ThemeGroup::ALL {
            for theme in group.themes() {
                assert!(
                    !seen.contains(theme),
                    "{} appears in two groups",
                    theme.label()
                );
                assert_eq!(theme.group(), group, "{}", theme.label());
                seen.push(*theme);
            }
        }
        assert_eq!(
            seen,
            Theme::ALL.to_vec(),
            "dropdown order covers every theme"
        );
    }

    /// The Omarchy group is the full set from basecamp/omarchy `themes/`:
    /// 23 palettes, imported rather than approximated.
    #[test]
    fn omarchy_group_is_the_full_omarchy_set() {
        let keys: Vec<&str> = Theme::OMARCHY.iter().map(|t| t.key()).collect();
        assert_eq!(
            keys,
            vec![
                "catppuccin",
                "catppuccin-latte",
                "ethereal",
                "everforest",
                "flexoki-light",
                "gruvbox",
                "hackerman",
                "jade-horse",
                "kanagawa",
                "last-horizon",
                "lumon",
                "lupine",
                "matte-black",
                "matcha",
                "nord",
                "retro-82",
                "ristretto",
                "rose-pine",
                "solitude",
                "starship",
                "tokyo-night",
                "vantablack",
                "white",
            ]
        );
        assert_eq!(Theme::OMARCHY.len(), 23);
        // Twelve ScriptCraft palettes: the six original (Midnight, Light,
        // Sepia, HighContrast, KitScenarist, KitScenaristDark) plus the six
        // writer-facing editor themes from the design brief.
        assert_eq!(Theme::SCRIPT.len(), 12);
        assert_eq!(Theme::ALL.len(), 35);
    }

    /// KIT Scenarist colours, sampled from the reference editor/PDF:
    /// light chrome #e4e4e4, white content ink, KIT blue #2b78da accent;
    /// dark chrome #414244, dark content ink, same blue accent.
    #[test]
    fn kit_scenarist_uses_the_published_colours() {
        let light = *Theme::KitScenarist.palette();
        assert!(Theme::KitScenarist.is_light());
        assert_eq!(light.page, Color32::from_rgb(0xFE, 0xFE, 0xFE));
        assert_eq!(light.text, Color32::BLACK);
        assert_eq!(light.window, Color32::from_rgb(0xE4, 0xE4, 0xE4));
        assert_eq!(light.list_bg, Color32::from_rgb(0xFF, 0xFF, 0xFF));
        assert_eq!(light.accent, Color32::from_rgb(0x2B, 0x78, 0xDA));

        let dark = *Theme::KitScenaristDark.palette();
        assert!(!Theme::KitScenaristDark.is_light());
        assert_eq!(dark.page, Color32::from_rgb(0x3D, 0x3D, 0x3D));
        assert_eq!(dark.text, Color32::from_rgb(0xEB, 0xEB, 0xEB));
        assert_eq!(dark.window, Color32::from_rgb(0x41, 0x42, 0x44));
        assert_eq!(dark.list_bg, Color32::from_rgb(0x40, 0x40, 0x40));
        assert_eq!(dark.accent, Color32::from_rgb(0x4C, 0x9A, 0xFF));
        // The lifted dark accent must clear the 3:1 floor on the dark page.
        assert!(contrast(dark.accent, dark.page) >= 3.0);
    }

    /// KIT Scenarist keeps one accent across light and dark: the box blue
    /// #2b78da sampled from the reference, lifted only inside the dark theme
    /// so it clears the contrast floor on the dark page.
    #[test]
    fn kit_scenarist_accent_is_the_kit_box_blue() {
        let light = *Theme::KitScenarist.palette();
        assert_eq!(light.accent, Color32::from_rgb(0x2B, 0x78, 0xDA));
        let dark = *Theme::KitScenaristDark.palette();
        assert_eq!(dark.accent, Color32::from_rgb(0x4C, 0x9A, 0xFF));
        assert!(contrast(light.accent, light.page) >= 3.0);
    }

    #[test]
    fn every_theme_paints_from_its_own_palette() {
        for theme in Theme::ALL {
            set_kit_theme(theme);
            assert_eq!(current_theme(), theme);
            assert_eq!(kit().window, theme.palette().window, "{}", theme.label());
            assert_eq!(kit().list_bg, theme.palette().list_bg, "{}", theme.label());
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

    /// The palette must survive the OS reporting a different brightness. egui
    /// keeps a style per brightness and, with the default preference, switches
    /// to the *other* one on a system theme change — which was unstyled stock
    /// egui. Windows desktops in light mode hit this on the first frame.
    #[test]
    fn a_system_theme_change_does_not_replace_the_palette() {
        let ctx = egui::Context::default();
        set_kit_theme(Theme::Midnight);
        apply_theme(&ctx);

        let input = egui::RawInput {
            system_theme: Some(egui::Theme::Light),
            ..Default::default()
        };
        ctx.begin_pass(input);
        let visuals = ctx.style().visuals.clone();
        assert!(
            visuals.dark_mode,
            "a light desktop must not repaint the app with stock egui light visuals"
        );
        assert_eq!(
            visuals.panel_fill,
            Theme::Midnight.palette().window,
            "the Midnight panel fill must survive the system theme change"
        );
        let _ = ctx.end_pass();

        // The same in reverse: a dark desktop must not override Light.
        set_kit_theme(Theme::Light);
        apply_theme(&ctx);
        let input = egui::RawInput {
            system_theme: Some(egui::Theme::Dark),
            ..Default::default()
        };
        ctx.begin_pass(input);
        let visuals = ctx.style().visuals.clone();
        assert!(
            !visuals.dark_mode,
            "a dark desktop must not repaint the app with stock egui dark visuals"
        );
        assert_eq!(visuals.panel_fill, Theme::Light.palette().window);
        let _ = ctx.end_pass();
    }
}
