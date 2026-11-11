//! Small modal/modeless windows (About, Help) extracted from `app.rs`.

use egui::Context;

/// About window; `open` is the visibility flag.
pub fn about(ctx: &Context, open: &mut bool) {
    egui::Window::new("About ScriptCraft")
        .open(open)
        .collapsible(false)
        .show(ctx, |ui| {
            ui.heading(format!("ScriptCraft {}", env!("CARGO_PKG_VERSION")));
            ui.label("A 100% Rust screenwriting studio (egui + eframe).");
            ui.separator();
            ui.label("Design language after KIT Scenarist 0.7.2:");
            ui.label("• Calm dark/light themes with the icon-blue accent #4294f8");
            ui.label("• Courier Prime script face (OFL), PT Sans UI");
            ui.label("• Russian-template metrics, calibrated chronometry");
            ui.label("• Fixed A4 pages — zoom scales pages, never reflows");
            ui.separator();
            ui.label("Opens .kitsp, exports Fountain. Start, Script, Research,");
            ui.label("Cards, Statistics and Settings modules.");
        });
}

/// Help window; `open` is the visibility flag.
pub fn help(ctx: &Context, open: &mut bool) {
    let p = crate::app::shortcut_prefix();
    egui::Window::new("Help")
        .open(open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.heading("ScriptCraft");
            ui.label("Menu ▸ New / Open / Save / Export (Fountain, PDF).");
            ui.label(format!("{p}S save · {p}Z / ⇧{p}Z undo · {p}F find."));
            ui.label("Research ▸ Versions starts a colour revision; save keeps it.");
            ui.label("Research holds Title Page, Logline, Synopsis and notes.");
            ui.label("Statistics reports counts, timings and speaking parts.");
            ui.separator();
            ui.label("Keep writing!");
        });
}
