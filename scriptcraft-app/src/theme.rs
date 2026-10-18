//! ScriptCraft design system: palette, global egui style and the painted
//! widgets that give the app its look.
//!
//! Design language: calm neutral surfaces, one icon-blue accent, generous
//! radii, tinted (not solid) selection, soft shadows on floating things.
//! Everything reads colors through [`kit()`], so the dark/light switch in
//! Settings keeps working exactly as before.

use egui::{
    Align2, Color32, CornerRadius, FontId, Margin, Painter, Pos2, Rect, Response, Sense, Shadow,
    Stroke, StrokeKind, Ui,
};
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
}

/// Far-left module rail width; matches the toolbar Menu button.
pub const RAIL_WIDTH: f32 = 82.0;

pub const KIT_DARK: KitColors = KitColors {
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
};

thread_local! {
    static KIT_PAL: std::cell::Cell<KitColors> = const { std::cell::Cell::new(KIT_DARK) };
    static KIT_LIGHT_ON: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Active theme palette.
pub fn kit() -> KitColors {
    KIT_PAL.with(|p| p.get())
}

pub fn kit_is_light() -> bool {
    KIT_LIGHT_ON.with(|c| c.get())
}

/// Switch the palette; the caller re-applies the style via [`apply_theme`].
pub fn set_kit_theme(light: bool) {
    KIT_LIGHT_ON.with(|c| c.set(light));
    KIT_PAL.with(|p| p.set(if light { KIT_LIGHT } else { KIT_DARK }));
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
    // Text selection and selected widgets: translucent accent.
    v.selection.bg_fill = if light {
        Color32::from_rgba_unmultiplied(0x42, 0x94, 0xF8, 70)
    } else {
        Color32::from_rgba_unmultiplied(0x42, 0x94, 0xF8, 105)
    };
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
    if !preview.is_empty() {
        paint_ellipsized(
            p,
            Pos2::new(left, card.top() + 39.0),
            card.top() + 39.0,
            preview,
            FontId::proportional(11.5),
            muted,
            right - left,
        );
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
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
pub fn project_card(ui: &mut Ui, name: &str, path: &str, when: &str) -> Response {
    let c = kit();
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 64.0), Sense::click());
    let card = rect.shrink2(egui::vec2(0.0, 3.0));
    let p = ui.painter();
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
    let when_g = p.layout_no_wrap(when.to_owned(), FontId::proportional(11.5), c.disabled_text);
    let when_w = when_g.size().x;
    p.galley(
        Pos2::new(card.right() - 16.0 - when_w, card.top() + 16.0),
        when_g,
        c.disabled_text,
    );
    let text_w = (card.right() - 16.0 - when_w - 12.0 - x).max(10.0);
    paint_ellipsized(
        p,
        Pos2::new(x, card.top() + 21.0),
        card.top() + 21.0,
        name,
        FontId::proportional(14.5),
        c.heading_text,
        text_w,
    );
    paint_ellipsized(
        p,
        Pos2::new(x, card.top() + 43.0),
        card.top() + 43.0,
        path,
        FontId::proportional(11.5),
        c.disabled_text,
        card.right() - 16.0 - x,
    );
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}
