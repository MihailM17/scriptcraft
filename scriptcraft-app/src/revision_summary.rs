//! Revision summary window: revision picker, scene-grouped table and totals.
//!
//! The counts themselves live in [`scriptcraft_core::summary`] and are derived
//! from the same `DraftDiff` as compare; this module only paints them. File
//! export and PDF rendering stay in `app.rs`, which owns fonts and dialogs.

use egui::{Align2, Context, FontId, Sense, Stroke, StrokeKind};
use scriptcraft_core::revision::RevisionSet;
use scriptcraft_core::summary::RevisionSummary;

use crate::theme::{ink_on_accent, kit, paint_ellipsized, paint_revision};

/// What the user asked the window to do this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SummaryAction {
    None,
    /// The baseline combo changed; the caller must invalidate its cache.
    BaseChanged,
    /// User clicked a scene row in the summary table to inspect it in the script.
    JumpScene(usize),
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
        .default_width(680.0)
        .show(ctx, |ui| {
            let c = kit();
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("BASELINE")
                        .size(10.5)
                        .strong()
                        .color(c.disabled_text),
                );
                ui.add_space(6.0);
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
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new("No revision data.")
                        .size(13.0)
                        .color(c.disabled_text),
                );
                return;
            };
            ui.add_space(10.0);

            // Recessed revision overview card.
            egui::Frame::new()
                .fill(c.window)
                .stroke(Stroke::new(1.0_f32, c.sep))
                .corner_radius(8.0)
                .inner_margin(egui::Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let (swatch, _) =
                            ui.allocate_exact_size(egui::vec2(12.0, 12.0), Sense::hover());
                        paint_revision(ui.painter(), swatch, summary.color, 2.5);
                        ui.add_space(4.0);
                        let name = if summary.name.trim().is_empty() {
                            format!("{} Revision", summary.color.label())
                        } else {
                            summary.name.clone()
                        };
                        ui.label(
                            egui::RichText::new(name)
                                .size(14.0)
                                .strong()
                                .color(c.heading_text),
                        );
                        if !summary.date.is_empty() {
                            ui.label(
                                egui::RichText::new(format!("· {}", summary.date))
                                    .size(12.0)
                                    .color(c.disabled_text),
                            );
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let pages = if summary.base_pages == 0 {
                                format!("{} pages", summary.current_pages)
                            } else {
                                format!(
                                    "{} to {} pages ({:+})",
                                    summary.base_pages,
                                    summary.current_pages,
                                    summary.pages_delta()
                                )
                            };
                            let g = ui.painter().layout_no_wrap(
                                pages,
                                FontId::proportional(11.5),
                                c.accent_text,
                            );
                            let (pill, _) = ui.allocate_exact_size(
                                egui::vec2(g.size().x + 14.0, 22.0),
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
                    });
                });

            ui.add_space(12.0);

            // Editorial scene-grouped breakdown table.
            let table_w = ui.available_width().max(420.0);
            let scene_w = 78.0;
            let num_w = 68.0;
            let head_w = (table_w - scene_w - num_w * 3.0).max(120.0);

            // Column headers.
            let (hdr, _) = ui.allocate_exact_size(egui::vec2(table_w, 24.0), Sense::hover());
            let p = ui.painter();
            let hfont = FontId::proportional(10.5);
            p.text(
                egui::pos2(hdr.left() + 8.0, hdr.center().y),
                Align2::LEFT_CENTER,
                "SCENE",
                hfont.clone(),
                c.disabled_text,
            );
            p.text(
                egui::pos2(hdr.left() + scene_w, hdr.center().y),
                Align2::LEFT_CENTER,
                "HEADING",
                hfont.clone(),
                c.disabled_text,
            );
            for (idx, col) in ["ADDED", "REMOVED", "CHANGED"].iter().enumerate() {
                let right = hdr.left() + scene_w + head_w + num_w * (idx as f32 + 1.0) - 8.0;
                p.text(
                    egui::pos2(right, hdr.center().y),
                    Align2::RIGHT_CENTER,
                    *col,
                    hfont.clone(),
                    c.disabled_text,
                );
            }
            p.line_segment(
                [
                    egui::pos2(hdr.left(), hdr.bottom() - 1.0),
                    egui::pos2(hdr.right(), hdr.bottom() - 1.0),
                ],
                Stroke::new(1.0_f32, c.sep),
            );

            egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    if summary.scenes.is_empty() {
                        ui.add_space(18.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new("No changes in this revision.")
                                    .size(12.5)
                                    .color(c.disabled_text),
                            );
                        });
                        ui.add_space(18.0);
                    } else {
                        for scene in &summary.scenes {
                            let (row, resp) =
                                ui.allocate_exact_size(egui::vec2(table_w, 28.0), Sense::click());
                            if resp.hovered() {
                                ui.painter().rect_filled(row, 5.0, c.hover_light);
                            }
                            let font = FontId::proportional(12.5);
                            let rp = ui.painter();
                            rp.text(
                                egui::pos2(row.left() + 8.0, row.center().y),
                                Align2::LEFT_CENTER,
                                summary.scene_label(scene),
                                font.clone(),
                                if resp.hovered() {
                                    c.accent_text
                                } else {
                                    c.disabled_text
                                },
                            );
                            paint_ellipsized(
                                rp,
                                egui::pos2(row.left() + scene_w, row.center().y),
                                row.center().y,
                                &scene.heading,
                                font.clone(),
                                c.text,
                                (head_w - 12.0).max(40.0),
                            );
                            for (idx, val) in [scene.added, scene.removed, scene.changed]
                                .iter()
                                .enumerate()
                            {
                                let right =
                                    row.left() + scene_w + head_w + num_w * (idx as f32 + 1.0)
                                        - 8.0;
                                rp.text(
                                    egui::pos2(right, row.center().y),
                                    Align2::RIGHT_CENTER,
                                    val.to_string(),
                                    font.clone(),
                                    if *val > 0 {
                                        c.heading_text
                                    } else {
                                        c.disabled_text
                                    },
                                );
                            }
                            let hairline = egui::Color32::from_rgba_unmultiplied(
                                c.sep.r(),
                                c.sep.g(),
                                c.sep.b(),
                                120,
                            );
                            rp.line_segment(
                                [
                                    egui::pos2(row.left() + 8.0, row.bottom() - 0.5),
                                    egui::pos2(row.right() - 8.0, row.bottom() - 0.5),
                                ],
                                Stroke::new(0.75_f32, hairline),
                            );
                            resp.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::Button,
                                    true,
                                    scene.heading.clone(),
                                )
                            });
                            let resp = resp
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .on_hover_text("Click to jump to this scene");
                            if resp.clicked() {
                                action = SummaryAction::JumpScene(scene.scene_index);
                            }
                        }
                    }
                });

            // Totals summary strip.
            let (tot, tot_resp) = ui.allocate_exact_size(egui::vec2(table_w, 30.0), Sense::hover());
            tot_resp
                .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, "TOTAL"));
            let tp = ui.painter();
            tp.rect_filled(tot, 6.0, c.window);
            tp.rect_stroke(tot, 6.0, Stroke::new(1.0_f32, c.sep), StrokeKind::Inside);
            tp.text(
                egui::pos2(tot.left() + 8.0, tot.center().y),
                Align2::LEFT_CENTER,
                "TOTAL",
                FontId::proportional(11.0),
                c.heading_text,
            );
            for (idx, val) in [summary.added, summary.removed, summary.changed]
                .iter()
                .enumerate()
            {
                let right = tot.left() + scene_w + head_w + num_w * (idx as f32 + 1.0) - 8.0;
                tp.text(
                    egui::pos2(right, tot.center().y),
                    Align2::RIGHT_CENTER,
                    val.to_string(),
                    FontId::proportional(12.5),
                    if *val > 0 {
                        c.accent_text
                    } else {
                        c.disabled_text
                    },
                );
            }

            ui.add_space(12.0);
            let (rule, _) = ui.allocate_exact_size(egui::vec2(table_w, 1.0), Sense::hover());
            ui.painter().rect_filled(rule, 0.0, c.sep);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Save CSV…").clicked() {
                    action = SummaryAction::SaveCsv;
                }
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("Print Summary…")
                                .strong()
                                .color(ink_on_accent(c.accent)),
                        )
                        .fill(c.accent),
                    )
                    .clicked()
                {
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
