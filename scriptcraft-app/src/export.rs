//! KIT-style Export Scenario dialog.
//!
//! The renderer is extracted from `app.rs` unchanged. The app passes its
//! export state by reference and performs the chosen preview/export action
//! through the existing `preview_pdf` / `save_pdf` / `save_fdx` /
//! `save_fountain` paths.

use crate::theme::{ink_on_accent, kit};
use egui::Context;
use scriptcraft_core::kitsp;
use scriptcraft_layout::pdf::ExportOptions;

/// Export dialog modules, mirroring KIT's Research / Outline / Script tabs.
#[derive(Default, PartialEq, Eq, Clone, Copy)]
pub enum ExportTab {
    Research,
    Outline,
    #[default]
    Script,
}

/// Export dialog file formats, mirroring KIT's format radio group.
#[derive(Default, PartialEq, Eq, Clone, Copy)]
pub enum ExportFormat {
    #[default]
    Pdf,
    Fdx,
    Fountain,
    FadeIn,
    Markdown,
}

impl ExportFormat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pdf => "PDF",
            Self::Fdx => "FDX",
            Self::Fountain => "Fountain",
            Self::FadeIn => "Fade In",
            Self::Markdown => "Markdown",
        }
    }
}

/// Mutable dialog state borrowed from `ScriptCraftApp` for one frame.
pub struct ExportDialog<'a> {
    pub title: &'a str,
    pub research: &'a [kitsp::ResearchEntry],
    pub has_revisions: bool,
    pub has_shots: bool,
    pub open: &'a mut bool,
    pub tab: &'a mut ExportTab,
    pub format: &'a mut ExportFormat,
    pub path: &'a mut String,
    pub additional: &'a mut bool,
    pub options: &'a mut ExportOptions,
    pub check_breaks: &'a mut bool,
    pub watermark_on: &'a mut bool,
    pub watermark: &'a mut String,
}

/// What the dialog asked the app to do after this frame.
pub enum ExportAction {
    None,
    Preview,
    Export {
        path: std::path::PathBuf,
        format: ExportFormat,
    },
    Close,
}

/// Draw the Export Scenario window. Path selection is handled here; actual
/// PDF/FDX/Fountain work stays in the app.
pub fn dialog(ctx: &Context, state: &mut ExportDialog<'_>) -> ExportAction {
    let mut win_open = true;
    let mut close = false;
    let mut do_preview = false;
    let mut do_export = false;
    let mut browse = false;
    let mut action = ExportAction::None;
    let win_h = if state.has_shots && *state.additional {
        544.0
    } else {
        500.0
    };
    egui::Window::new("Export Scenario")
        .open(&mut win_open)
        .collapsible(false)
        .resizable(false)
        .fixed_size(egui::vec2(560.0, win_h))
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            let c = kit();
            // Segmented tab bar across the top (Research / Outline / Script).
            ui.spacing_mut().item_spacing.x = 6.0;
            let tab_w = (ui.available_width() - 12.0) / 3.0;
            ui.horizontal(|ui| {
                for (tab, name) in [
                    (ExportTab::Research, "Export research"),
                    (ExportTab::Outline, "Export outline"),
                    (ExportTab::Script, "Export script"),
                ] {
                    let active = *state.tab == tab;
                    let (rect, resp) =
                        ui.allocate_exact_size(egui::vec2(tab_w, 30.0), egui::Sense::click());
                    let bg = if active {
                        c.accent_soft
                    } else if resp.hovered() {
                        c.hover_light
                    } else {
                        c.button
                    };
                    ui.painter().rect_filled(rect, 7.0, bg);
                    ui.painter().rect_stroke(
                        rect,
                        7.0,
                        egui::Stroke::new(1.0_f32, if active { c.accent } else { c.sep }),
                        egui::StrokeKind::Inside,
                    );
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        name,
                        egui::FontId::proportional(12.5),
                        if active { c.accent_text } else { c.text },
                    );
                    if resp
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        *state.tab = tab;
                    }
                }
            });
            ui.add_space(10.0);
            let script_tab = *state.tab == ExportTab::Script;

            // Destination path + browse button.
            ui.label(
                egui::RichText::new("DESTINATION FILE")
                    .size(10.5)
                    .strong()
                    .color(c.disabled_text),
            );
            ui.add_space(4.0);
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 40.0).max(80.0);
                ui.add_sized(
                    [w, 26.0],
                    egui::TextEdit::singleline(state.path).hint_text("Choose a destination file…"),
                );
                if ui
                    .add_sized([34.0, 26.0], egui::Button::new("…"))
                    .on_hover_text("Browse…")
                    .clicked()
                {
                    browse = true;
                }
            });
            let file_exists = !state.path.is_empty() && std::path::Path::new(&*state.path).exists();
            if file_exists {
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new("File exists and will be replaced on export.")
                        .size(11.5)
                        .color(c.accent_text),
                );
            }

            match *state.tab {
                ExportTab::Script => {
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Format:")
                                .size(12.5)
                                .color(c.disabled_text),
                        );
                        ui.add_space(4.0);
                        for fmt in [
                            ExportFormat::Pdf,
                            ExportFormat::Fdx,
                            ExportFormat::Fountain,
                            ExportFormat::FadeIn,
                            ExportFormat::Markdown,
                        ] {
                            ui.radio_value(state.format, fmt, fmt.label());
                        }
                    });
                    ui.add_space(8.0);
                    if state.has_shots && *state.format == ExportFormat::Pdf {
                        ui.checkbox(
                            &mut state.options.color_by_shots,
                            "Color script by shots",
                        )
                        .on_hover_text(
                            "Highlight screenplay lines with their assigned shot colors and margin tags",
                        );
                        if state.options.color_by_shots {
                            ui.horizontal(|ui| {
                                ui.add_space(18.0);
                                ui.checkbox(
                                    &mut state.options.include_shot_legend,
                                    "Include shot legend page",
                                );
                            });
                        }
                        ui.add_space(4.0);
                    }
                    ui.checkbox(state.additional, "Additional (Numbering, Title Page etc.)");
                    if *state.additional {
                        ui.add_space(6.0);
                        egui::Frame::new()
                            .fill(c.window)
                            .stroke(egui::Stroke::new(1.0_f32, c.sep))
                            .corner_radius(8.0)
                            .inner_margin(egui::Margin::same(12))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.spacing_mut().item_spacing.y = 5.0;
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("Template:")
                                            .size(12.5)
                                            .color(c.disabled_text),
                                    );
                                    egui::ComboBox::from_id_salt("export_template")
                                        .selected_text(state.options.page_size.template_label())
                                        .show_ui(ui, |ui| {
                                            for size in scriptcraft_core::editing::PageSize::ALL {
                                                ui.selectable_value(
                                                    &mut state.options.page_size,
                                                    size,
                                                    size.template_label(),
                                                );
                                            }
                                        });
                                });
                                ui.add_space(2.0);
                                ui.checkbox(&mut state.options.title_page, "Print title page");
                                ui.checkbox(&mut state.options.page_numbers, "Print page numbers");
                                ui.checkbox(
                                    &mut state.options.scene_numbers,
                                    "Print scene numbers",
                                );
                                if state.has_revisions {
                                    ui.checkbox(
                                        &mut state.options.revision_marks,
                                        "Print revision marks (current)",
                                    );
                                    ui.checkbox(
                                        &mut state.options.revision_pages_only,
                                        "Print current revision pages only",
                                    );
                                }
                                ui.add_enabled(
                                    false,
                                    egui::Checkbox::new(&mut false, "Print dialogue numbers"),
                                )
                                .on_disabled_hover_text("Not supported yet");
                                ui.add_enabled(
                                    false,
                                    egui::Checkbox::new(&mut false, "Save review marks"),
                                )
                                .on_disabled_hover_text("No review marks in this app");
                                ui.checkbox(state.check_breaks, "Check page breaks");
                                ui.horizontal(|ui| {
                                    ui.checkbox(state.watermark_on, "Print watermark:");
                                    ui.add_enabled(
                                        *state.watermark_on,
                                        egui::TextEdit::singleline(state.watermark)
                                            .hint_text("e.g. CONFIDENTIAL")
                                            .desired_width(180.0),
                                    );
                                });
                            });
                    }
                }
                ExportTab::Research => {
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new("Research export is not implemented yet.")
                            .small()
                            .weak(),
                    );
                    egui::ScrollArea::vertical()
                        .max_height(260.0)
                        .show(ui, |ui| {
                            for e in state.research {
                                let kind = match e.kind {
                                    kitsp::ResearchKind::Character => "character",
                                    kitsp::ResearchKind::Location => "place",
                                    kitsp::ResearchKind::Other => "note",
                                };
                                ui.label(format!("{}  ({kind})", e.name));
                            }
                        });
                }
                ExportTab::Outline => {
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new("Outline export is not implemented yet.")
                            .small()
                            .weak(),
                    );
                }
            }

            // Footer action row separated by a clean hairline rule.
            let gap = (ui.available_height() - 42.0).max(8.0);
            ui.add_space(gap);
            let (rule, _) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
            ui.painter().rect_filled(rule, 0.0, c.sep);
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let supported = script_tab
                    && !state.path.is_empty()
                    && matches!(
                        *state.format,
                        ExportFormat::Pdf
                            | ExportFormat::Fdx
                            | ExportFormat::Fountain
                            | ExportFormat::FadeIn
                            | ExportFormat::Markdown
                    );
                let label = if file_exists { "Rewrite" } else { "Export" };
                let primary_btn = if supported {
                    egui::Button::new(
                        egui::RichText::new(label)
                            .strong()
                            .color(ink_on_accent(c.accent)),
                    )
                    .fill(c.accent)
                } else {
                    egui::Button::new(label)
                };
                if ui.add_enabled(supported, primary_btn).clicked() {
                    do_export = true;
                }
                if ui.button("Print Preview").clicked() {
                    do_preview = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    *state.open = win_open && !close && *state.open;
    if browse {
        let mut dlg = rfd::FileDialog::new();
        let (name, ext, filter) = match *state.format {
            ExportFormat::Pdf => (
                if state.title.is_empty() {
                    "script.pdf".to_string()
                } else {
                    format!("{}.pdf", state.title)
                },
                "pdf",
                "PDF",
            ),
            ExportFormat::Fountain => (
                if state.title.is_empty() {
                    "script.fountain".to_string()
                } else {
                    format!("{}.fountain", state.title)
                },
                "fountain",
                "Fountain",
            ),
            ExportFormat::Fdx => ("script.fdx".to_string(), "fdx", "FDX"),
            ExportFormat::FadeIn => (
                if state.title.is_empty() {
                    "script.fadein".to_string()
                } else {
                    format!("{}.fadein", state.title)
                },
                "fadein",
                "Fade In",
            ),
            ExportFormat::Markdown => (
                if state.title.is_empty() {
                    "script.md".to_string()
                } else {
                    format!("{}.md", state.title)
                },
                "md",
                "Markdown",
            ),
        };
        dlg = dlg.add_filter(filter, &[ext]).set_file_name(&name);
        if let Some(dst) = dlg.save_file() {
            *state.path = dst.to_string_lossy().into_owned();
        }
    }
    if do_preview {
        action = ExportAction::Preview;
    } else if do_export {
        action = ExportAction::Export {
            path: std::path::PathBuf::from(&*state.path),
            format: *state.format,
        };
    } else if close || !*state.open {
        action = ExportAction::Close;
    }
    action
}
