//! Small modal/modeless windows extracted from `app.rs` (About, Help,
//! clear-revision confirmation).

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
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.label("Remove all revision colours and baselines from this project?");
            ui.label("The script text is not changed.");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
                if ui.button("Clear Revision History").clicked() {
                    clear = true;
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
