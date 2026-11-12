//! KIT-style Export Scenario dialog.
//!
//! The renderer is extracted from `app.rs` unchanged. The app passes its
//! export state by reference and performs the chosen preview/export action
//! through the existing `preview_pdf` / `save_pdf` / `save_fdx` /
//! `save_fountain` paths.

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
}

impl ExportFormat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pdf => "PDF",
            Self::Fdx => "FDX",
            Self::Fountain => "Fountain",
        }
    }
}

/// Mutable dialog state borrowed from `ScriptCraftApp` for one frame.
pub struct ExportDialog<'a> {
    pub title: &'a str,
    pub research: &'a [kitsp::ResearchEntry],
    pub has_revisions: bool,
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
    egui::Window::new("Export Scenario")
        .open(&mut win_open)
        .collapsible(false)
        .resizable(false)
        .fixed_size(egui::vec2(550.0, 500.0))
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            // Tab bar across the top (KIT: Export research / outline / script).
            ui.spacing_mut().item_spacing.x = 4.0;
            let tab_w = (ui.available_width() - 8.0) / 3.0;
            ui.horizontal(|ui| {
                for (tab, name) in [
                    (ExportTab::Research, "Export research"),
                    (ExportTab::Outline, "Export outline"),
                    (ExportTab::Script, "Export script"),
                ] {
                    if ui
                        .add_sized(
                            [tab_w, 26.0],
                            egui::Button::selectable(*state.tab == tab, name),
                        )
                        .clicked()
                    {
                        *state.tab = tab;
                    }
                }
            });
            ui.separator();
            let script_tab = *state.tab == ExportTab::Script;

            // "Choose file to export:" + path field + browse button.
            ui.label("Choose file to export:");
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 36.0).max(80.0);
                ui.add_sized(
                    [w, 22.0],
                    egui::TextEdit::singleline(state.path).hint_text("Choose a destination file…"),
                );
                if ui
                    .add_sized([30.0, 22.0], egui::Button::new("..."))
                    .clicked()
                {
                    browse = true;
                }
            });
            let file_exists = !state.path.is_empty() && std::path::Path::new(&*state.path).exists();
            if file_exists {
                ui.label(
                    egui::RichText::new("File exists. It will be replaced after exporting.")
                        .small()
                        .weak(),
                );
            }

            match *state.tab {
                ExportTab::Script => {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label("Format:");
                        for fmt in [ExportFormat::Pdf, ExportFormat::Fdx, ExportFormat::Fountain] {
                            ui.radio_value(state.format, fmt, fmt.label());
                        }
                    });
                    ui.add_space(2.0);
                    ui.checkbox(state.additional, "Additional (Numbering, Title Page etc.)");
                    if *state.additional {
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Template:");
                                egui::ComboBox::from_id_salt("export_template")
                                    .selected_text("Final Draft Screenplay (A4)")
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut (),
                                            (),
                                            "Final Draft Screenplay (A4)",
                                        );
                                    });
                            });
                            ui.checkbox(&mut state.options.title_page, "Print Title Page");
                            ui.checkbox(&mut state.options.page_numbers, "Print pages numbers");
                            ui.checkbox(&mut state.options.scene_numbers, "Print scenes numbers");
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
                                egui::Checkbox::new(&mut false, "Print dialogues numbers"),
                            )
                            .on_disabled_hover_text("Not supported yet");
                            ui.add_enabled(
                                false,
                                egui::Checkbox::new(&mut false, "Save Review Marks"),
                            )
                            .on_disabled_hover_text("No review marks in this app");
                            ui.checkbox(state.check_breaks, "Check Page Breaks");
                            ui.horizontal(|ui| {
                                ui.checkbox(state.watermark_on, "Print watermark:");
                                ui.add_enabled(
                                    *state.watermark_on,
                                    egui::TextEdit::singleline(state.watermark)
                                        .desired_width(150.0),
                                );
                            });
                        });
                    }
                }
                ExportTab::Research => {
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("Research export is not implemented yet.")
                            .small()
                            .weak(),
                    );
                    egui::ScrollArea::vertical()
                        .max_height(280.0)
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
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("Outline export is not implemented yet.")
                            .small()
                            .weak(),
                    );
                }
            }

            // Bottom-right buttons (KIT order: Export, Print Preview, Cancel).
            let gap = (ui.available_height() - 32.0).max(4.0);
            ui.add_space(gap);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Cancel").clicked() {
                    close = true;
                }
                if ui.button("Print Preview").clicked() {
                    do_preview = true;
                }
                let supported = script_tab
                    && !state.path.is_empty()
                    && matches!(
                        *state.format,
                        ExportFormat::Pdf | ExportFormat::Fdx | ExportFormat::Fountain
                    );
                let label = if file_exists { "Rewrite" } else { "Export" };
                if ui
                    .add_enabled(supported, egui::Button::new(label))
                    .clicked()
                {
                    do_export = true;
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
