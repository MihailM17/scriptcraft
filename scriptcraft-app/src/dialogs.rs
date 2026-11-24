//! Small modal/modeless windows extracted from `app.rs` (About, Help,
//! clear-revision confirmation).

use crate::theme::{ink_on_accent, kit};
use egui::{Align2, Context, FontId, Sense, Stroke};

fn dialog_hairline(ui: &mut egui::Ui) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(r, 0.0, kit().sep);
}

fn spec_row(ui: &mut egui::Ui, label: &str, value: &str) {
    let c = kit();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), Sense::hover());
    ui.painter().text(
        egui::pos2(rect.left(), rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(11.5),
        c.disabled_text,
    );
    ui.painter().text(
        egui::pos2(rect.right(), rect.center().y),
        Align2::RIGHT_CENTER,
        value,
        FontId::proportional(12.0),
        c.text,
    );
}

fn shortcut_row(ui: &mut egui::Ui, keys: &str, desc: &str) {
    let c = kit();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), Sense::hover());
    let key_g =
        ui.painter()
            .layout_no_wrap(keys.to_string(), FontId::monospace(11.0), c.accent_text);
    let pill_w = (key_g.size().x + 12.0).max(52.0);
    let pill = egui::Rect::from_min_size(
        egui::pos2(rect.left(), rect.center().y - 10.0),
        egui::vec2(pill_w, 20.0),
    );
    ui.painter().rect_filled(pill, 5.0, c.accent_soft);
    ui.painter().galley(
        egui::pos2(
            pill.center().x - key_g.size().x / 2.0,
            pill.center().y - key_g.size().y / 2.0,
        ),
        key_g,
        c.accent_text,
    );
    ui.painter().text(
        egui::pos2(rect.left() + pill_w.max(84.0) + 10.0, rect.center().y),
        Align2::LEFT_CENTER,
        desc,
        FontId::proportional(12.5),
        c.text,
    );
}

/// About window; `open` is the visibility flag.
pub fn about(ctx: &Context, open: &mut bool) {
    let mut win_open = *open;
    let mut close = false;
    egui::Window::new("About ScriptCraft")
        .open(&mut win_open)
        .collapsible(false)
        .resizable(false)
        .fixed_size(egui::vec2(440.0, 0.0))
        .anchor(Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            let c = kit();
            ui.label(
                egui::RichText::new("PRODUCTION SCREENWRITING STUDIO")
                    .size(10.0)
                    .strong()
                    .color(c.accent_text),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("ScriptCraft")
                        .size(22.0)
                        .strong()
                        .color(c.heading_text),
                );
                let ver = format!("v{}", env!("CARGO_PKG_VERSION"));
                let g = ui
                    .painter()
                    .layout_no_wrap(ver, FontId::proportional(11.0), c.accent_text);
                let (pill, _) = ui.allocate_exact_size(
                    egui::vec2(g.size().x + 12.0, 20.0),
                    Sense::hover(),
                );
                ui.painter().rect_filled(pill, 6.0, c.accent_soft);
                ui.painter().galley(
                    egui::pos2(
                        pill.center().x - g.size().x / 2.0,
                        pill.center().y - g.size().y / 2.0,
                    ),
                    g,
                    c.accent_text,
                );
            });
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "Native, offline-first screenwriting studio engineered for production revisions and page-exact PDF output.",
                )
                .size(12.5)
                .color(c.disabled_text),
            );
            ui.add_space(14.0);

            egui::Frame::new()
                .fill(c.window)
                .stroke(Stroke::new(1.0_f32, c.sep))
                .corner_radius(8.0)
                .inner_margin(egui::Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    spec_row(ui, "Interchange", ".kitsp · .fdx · .fadein · .fountain · .md");
                    spec_row(ui, "Page Geometry", "1/6-inch pitch · A4 & US Letter");
                    spec_row(ui, "Typography", "Courier Prime · Plex Mono · Liberation");
                    spec_row(ui, "Themes", "35 WCAG contrast-tested palettes");
                    spec_row(ui, "Engine", "100% Rust · egui + eframe");
                });

            ui.add_space(14.0);
            dialog_hairline(ui);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Free & open-source · Offline-first")
                        .size(11.5)
                        .color(c.disabled_text),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });
            });
        });
    *open = win_open && !close;
}

/// Help window; `open` is the visibility flag.
pub fn help(ctx: &Context, open: &mut bool) {
    let p = crate::app::shortcut_prefix();
    let mut win_open = *open;
    let mut close = false;
    egui::Window::new("Help")
        .open(&mut win_open)
        .collapsible(false)
        .resizable(false)
        .fixed_size(egui::vec2(480.0, 0.0))
        .anchor(Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            let c = kit();
            ui.label(
                egui::RichText::new("QUICK REFERENCE")
                    .size(10.0)
                    .strong()
                    .color(c.accent_text),
            );
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new("ScriptCraft Shortcuts & Workflow")
                    .size(18.0)
                    .strong()
                    .color(c.heading_text),
            );
            ui.add_space(12.0);

            egui::Frame::new()
                .fill(c.window)
                .stroke(Stroke::new(1.0_f32, c.sep))
                .corner_radius(8.0)
                .inner_margin(egui::Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        egui::RichText::new("WRITING & FORMATTING")
                            .size(10.0)
                            .strong()
                            .color(c.disabled_text),
                    );
                    ui.add_space(4.0);
                    shortcut_row(ui, "Tab / Shift+Tab", "Cycle screenplay element type");
                    shortcut_row(ui, &format!("{p}1 – {p}0"), "Jump directly to an element type");
                    shortcut_row(
                        ui,
                        &format!("{p}B / {p}I / {p}U"),
                        "Inline bold, italic, and underline",
                    );
                    shortcut_row(ui, &format!("{p}D"), "Pair or unpair dual dialogue");
                    shortcut_row(ui, &format!("{p}F"), "Find & replace (case / whole word)");
                    ui.add_space(8.0);
                    dialog_hairline(ui);
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("FILES & PRODUCTION")
                            .size(10.0)
                            .strong()
                            .color(c.disabled_text),
                    );
                    ui.add_space(4.0);
                    shortcut_row(ui, &format!("{p}S / Shift+{p}S"), "Save or Save As (.kitsp)");
                    shortcut_row(ui, &format!("{p}Z / Shift+{p}Z"), "Undo / Redo");
                    shortcut_row(ui, "F12", "Print preview PDF");
                });

            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(
                    "Use Research > Versions to start colored revisions, compare drafts, lock pages/scenes, and export revised pages.",
                )
                .size(12.0)
                .color(c.disabled_text),
            );
            ui.add_space(12.0);
            dialog_hairline(ui);
            ui.add_space(10.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Keep writing!").clicked() {
                    close = true;
                }
            });
        });
    *open = win_open && !close;
}

/// Confirmation dialog for clearing all revision history. Returns true when
/// the user confirms clearing.
pub fn confirm_clear_revisions(ctx: &Context, open: &mut bool) -> bool {
    let mut win_open = *open;
    let mut clear = false;
    let mut cancel = false;
    egui::Window::new("Clear Revision History")
        .open(&mut win_open)
        .collapsible(false)
        .resizable(false)
        .fixed_size(egui::vec2(400.0, 0.0))
        .anchor(Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            let c = kit();
            ui.label(
                egui::RichText::new("Remove all revision colours and baselines from this project?")
                    .size(13.5)
                    .strong()
                    .color(c.heading_text),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "Your screenplay text will not be changed, but revision marks and comparison baselines will be cleared.",
                )
                .size(12.0)
                .color(c.disabled_text),
            );
            ui.add_space(14.0);
            dialog_hairline(ui);
            ui.add_space(10.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("Clear Revision History")
                                .strong()
                                .color(ink_on_accent(c.spell_error)),
                        )
                        .fill(c.spell_error)
                        .stroke(Stroke::NONE),
                    )
                    .clicked()
                {
                    clear = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
    if clear || cancel || !win_open {
        *open = false;
    } else {
        *open = win_open;
    }
    clear
}
