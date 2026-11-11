//! Revision summary window: revision picker, scene-grouped table and totals.
//!
//! The counts themselves live in [`scriptcraft_core::summary`] and are derived
//! from the same `DraftDiff` as compare; this module only paints them. File
//! export and PDF rendering stay in `app.rs`, which owns fonts and dialogs.

use egui::Context;
use scriptcraft_core::revision::RevisionSet;
use scriptcraft_core::summary::RevisionSummary;

use crate::theme::revision_color;

/// What the user asked the window to do this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SummaryAction {
    None,
    /// The baseline combo changed; the caller must invalidate its cache.
    BaseChanged,
    Close,
    SaveCsv,
    Print,
}

/// Paint the summary window. `base` is the selected revision index; when the
/// user picks another baseline the caller recomputes before the next frame.
pub fn window(
    ctx: &Context,
    open: &mut bool,
    base: &mut usize,
    summary: Option<&RevisionSummary>,
    sets: &[RevisionSet],
) -> SummaryAction {
    let mut win_open = *open;
    let mut selected_base = *base;
    let mut action = SummaryAction::None;
    egui::Window::new("Revision Summary")
        .open(&mut win_open)
        .collapsible(false)
        .resizable(true)
        .default_width(660.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Baseline:");
                let selected = sets
                    .get(selected_base)
                    .map(|set| format!("{} · {}", revision_name(set), set.started))
                    .unwrap_or_else(|| "Latest revision".to_string());
                egui::ComboBox::from_id_salt("summary_baseline")
                    .selected_text(selected)
                    .width(360.0)
                    .show_ui(ui, |ui| {
                        for (i, set) in sets.iter().enumerate().rev() {
                            let label = format!("{} · {}", revision_name(set), set.started);
                            ui.selectable_value(&mut selected_base, i, label);
                        }
                    });
            });
            let Some(summary) = summary else {
                ui.label("No revision data.");
                return;
            };
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let (swatch, _) =
                    ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                ui.painter()
                    .rect_filled(swatch, 2.0, revision_color(summary.color));
                let name = if summary.name.trim().is_empty() {
                    format!("{} Revision", summary.color.label())
                } else {
                    summary.name.clone()
                };
                ui.label(egui::RichText::new(name).strong());
                if !summary.date.is_empty() {
                    ui.label(egui::RichText::new(&summary.date).weak());
                }
            });
            let pages = if summary.base_pages == 0 {
                format!("Current draft: {} pages", summary.current_pages)
            } else {
                format!(
                    "{} pages to {} pages ({:+})",
                    summary.base_pages,
                    summary.current_pages,
                    summary.pages_delta()
                )
            };
            ui.label(pages);
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .max_height(360.0)
                .show(ui, |ui| {
                    egui::Grid::new("summary_scene_grid")
                        .num_columns(5)
                        .striped(true)
                        .spacing(egui::vec2(18.0, 6.0))
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new("Scene").strong());
                            ui.label(egui::RichText::new("Heading").strong());
                            ui.label(egui::RichText::new("Added").strong());
                            ui.label(egui::RichText::new("Removed").strong());
                            ui.label(egui::RichText::new("Changed").strong());
                            ui.end_row();
                            for scene in &summary.scenes {
                                ui.label(summary.scene_label(scene));
                                ui.label(&scene.heading);
                                ui.label(scene.added.to_string());
                                ui.label(scene.removed.to_string());
                                ui.label(scene.changed.to_string());
                                ui.end_row();
                            }
                            ui.label(egui::RichText::new("TOTAL").strong());
                            ui.label("");
                            ui.label(egui::RichText::new(summary.added.to_string()).strong());
                            ui.label(egui::RichText::new(summary.removed.to_string()).strong());
                            ui.label(egui::RichText::new(summary.changed.to_string()).strong());
                            ui.end_row();
                        });
                });
            if summary.scenes.is_empty() {
                ui.label(
                    egui::RichText::new("No changes in this revision.")
                        .small()
                        .weak(),
                );
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Save CSV…").clicked() {
                    action = SummaryAction::SaveCsv;
                }
                if ui.button("Print Summary…").clicked() {
                    action = SummaryAction::Print;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Close").clicked() {
                        action = SummaryAction::Close;
                    }
                });
            });
        });
    *open = win_open && action != SummaryAction::Close;
    if selected_base != *base {
        *base = selected_base;
        if action == SummaryAction::None {
            action = SummaryAction::BaseChanged;
        }
    }
    action
}

fn revision_name(set: &RevisionSet) -> String {
    if set.name.is_empty() {
        format!("{} Revision", set.color.label())
    } else {
        set.name.clone()
    }
}
