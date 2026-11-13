//! Research module: the KIT-style tree and its panes (script metadata,
//! title page, logline/synopsis, versions/revisions, and research entries).
//!
//! The UI is extracted from `app.rs` unchanged. `ResearchView` borrows the
//! fields the panes edit, and every operation that needs the wider app
//! (saving, importing, starting revisions) comes back as a [`ResearchAction`]
//! handled by `ScriptCraftApp` after the frame.

use egui::{Context, Ui};
use scriptcraft_core::{
    kitsp::{self, Metadata},
    revision::{RevisionHistory, RevisionSet},
};

use crate::app::{
    draw_kit_icon, kit_button, kit_field, kit_field_multi, kit_icon_button, kit_module_strip,
    kit_mono_editor, kit_section, kit_tree_row, KitIcon, KitTreeRow,
};
use crate::theme::kit;

/// Research module selection (KIT's research tree).
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum ResearchPane {
    Script,
    TitlePage,
    Logline,
    Synopsis,
    Versions,
    Characters,
    Locations,
    Documents,
    Entry(i64),
}

/// Editable buffer for one research entry (synced on selection).
#[derive(Debug, Clone)]
pub(crate) struct ResearchEdit {
    pub id: i64,
    pub kind: kitsp::ResearchKind,
    pub name: String,
    pub url: String,
    pub description: String,
}

/// Borrowed state for one frame of the Research module.
pub(crate) struct ResearchView<'a> {
    pub has_project: bool,
    /// Read-only fixture or unsaved project: research edits are disabled.
    pub read_only: bool,
    pub entries: &'a [kitsp::ResearchEntry],
    pub pane: &'a mut ResearchPane,
    pub meta: &'a mut Metadata,
    pub meta_dirty: &'a mut bool,
    pub scene_labels: &'a [String],
    pub path: &'a str,
    pub versions: &'a [(String, String)],
    pub revisions: &'a RevisionHistory,
    pub latest: Option<&'a RevisionSet>,
    pub show_current_revision: &'a mut bool,
    pub revision_marks_dirty: &'a mut bool,
    pub title_logline: &'a mut String,
    pub title_synopsis: &'a mut String,
    pub research_sel: &'a mut Option<i64>,
    pub research_edit: &'a mut Option<ResearchEdit>,
    pub research_tex: &'a mut Option<(i64, u64, egui::TextureHandle)>,
    pub status: &'a mut String,
}

/// Side effects requested by the Research UI this frame.
pub(crate) enum ResearchAction {
    AddEntry(kitsp::ResearchKind, &'static str),
    ImportCharacters,
    ImportLocations,
    StartRevision,
    SaveVersionDialog,
    ImportVersions,
    ShowHistory,
    OpenCompare,
    OpenSummary,
    ClearHistory,
    SaveEntry,
    DeleteEntry,
    SetPhoto(i64, Option<Vec<u8>>),
    LockPages,
    UnlockPages,
}

/// Draw the whole Research module and return the actions the app must run.
pub(crate) fn view(ctx: &Context, state: &mut ResearchView<'_>) -> Vec<ResearchAction> {
    let mut actions = Vec::new();
    egui::CentralPanel::default().show(ctx, |ui| {
        if !state.has_project {
            ui.centered_and_justified(|ui| {
                ui.label("Open a .kitsp file to browse research.");
            });
            return;
        }
        ui.spacing_mut().item_spacing.y = 0.0;
        kit_module_strip(ui, "Research", &[]);
        ui.spacing_mut().item_spacing.y = 6.0;
        // Compare is the one revision workspace that is useful from every
        // Research pane, so keep it in the module header instead of burying it
        // below the versions list.
        ui.horizontal(|ui| {
            let can_compare = state.has_project && !state.revisions.is_empty();
            if ui
                .add_enabled(can_compare, egui::Button::new("Compare Drafts…"))
                .on_disabled_hover_text("Start a revision to compare drafts.")
                .clicked()
            {
                actions.push(ResearchAction::OpenCompare);
            }
            ui.label(
                egui::RichText::new(if can_compare {
                    "Read-only: latest revision vs current draft"
                } else {
                    "Start a revision to compare drafts."
                })
                .size(11.0)
                .color(egui::Color32::from_gray(0xA1)),
            );
        });
        ui.add_space(2.0);
        let pane_h = ui.available_height();
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(330.0, pane_h),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_min_width(330.0);
                    ui.set_max_width(330.0);
                    egui::ScrollArea::vertical()
                        .id_salt("research_tree")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            tree(ui, state, &mut actions);
                        });
                },
            );
            ui.add_space(8.0);
            let (sep, _) = ui.allocate_exact_size(egui::vec2(1.0, pane_h), egui::Sense::hover());
            ui.painter().rect_filled(sep, 0.0, kit().sep);
            ui.add_space(4.0);
            egui::Frame::new()
                .fill(kit().module_bg)
                .inner_margin(egui::Margin::symmetric(14, 12))
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("research_pane")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                pane_view(ui, state, &mut actions);
                            });
                    });
                });
        });
    });
    actions
}

fn tree(ui: &mut Ui, state: &mut ResearchView<'_>, actions: &mut Vec<ResearchAction>) {
    ui.add_space(4.0);
    let mut sel = *state.pane;

    // Script branch.
    let mut open = ui.data(|d| {
        d.get_temp::<bool>(egui::Id::new("rt_script"))
            .unwrap_or(true)
    });
    let (row, arrow, _) = kit_tree_row(
        ui,
        KitTreeRow {
            sel: false,
            arrow: Some(open),
            plus: false,
        },
        KitIcon::Script,
        "Script",
        "rt_script",
    );
    if row.clicked() || arrow {
        open = !open;
        ui.data_mut(|d| d.insert_temp(egui::Id::new("rt_script"), open));
    }
    if open {
        ui.indent("rt_script_i", |ui| {
            for (pane, icon, name) in [
                (ResearchPane::TitlePage, KitIcon::Book, "Title Page"),
                (ResearchPane::Logline, KitIcon::Text, "Logline"),
                (ResearchPane::Synopsis, KitIcon::Paragraph, "Synopsis"),
                (ResearchPane::Versions, KitIcon::Layers, "Versions"),
            ] {
                let (r, _, _) = kit_tree_row(
                    ui,
                    KitTreeRow {
                        sel: sel == pane,
                        arrow: None,
                        plus: false,
                    },
                    icon,
                    name,
                    name,
                );
                if r.clicked() {
                    sel = pane;
                }
            }
        });
    }

    folder(
        ui,
        state,
        actions,
        "rt_chars",
        KitIcon::Person,
        "Characters",
        kitsp::ResearchKind::Character,
        &mut sel,
    );
    folder(
        ui,
        state,
        actions,
        "rt_locs",
        KitIcon::House,
        "Locations",
        kitsp::ResearchKind::Location,
        &mut sel,
    );
    folder(
        ui,
        state,
        actions,
        "rt_docs",
        KitIcon::Folder,
        "Documents",
        kitsp::ResearchKind::Other,
        &mut sel,
    );

    ui.add_space(8.0);
    *state.pane = sel;
}

#[allow(clippy::too_many_arguments)]
fn folder(
    ui: &mut Ui,
    state: &ResearchView<'_>,
    actions: &mut Vec<ResearchAction>,
    salt: &'static str,
    icon: KitIcon,
    title: &str,
    kind: kitsp::ResearchKind,
    sel: &mut ResearchPane,
) {
    let mut open = ui.data(|d| d.get_temp::<bool>(egui::Id::new(salt)).unwrap_or(true));
    let (row, arrow, plus) = kit_tree_row(
        ui,
        KitTreeRow {
            sel: false,
            arrow: Some(open),
            plus: true,
        },
        icon,
        title,
        salt,
    );
    if row.clicked() || arrow {
        open = !open;
        ui.data_mut(|d| d.insert_temp(egui::Id::new(salt), open));
    }
    if plus {
        egui::Popup::menu(&row).show(|ui| {
            ui.set_min_width(190.0);
            match kind {
                kitsp::ResearchKind::Character => {
                    if ui.button("Add character").clicked() {
                        actions.push(ResearchAction::AddEntry(
                            kitsp::ResearchKind::Character,
                            "New character",
                        ));
                        ui.close();
                    }
                    if ui.button("Import characters from script").clicked() {
                        actions.push(ResearchAction::ImportCharacters);
                        ui.close();
                    }
                }
                kitsp::ResearchKind::Location => {
                    if ui.button("Add location").clicked() {
                        actions.push(ResearchAction::AddEntry(
                            kitsp::ResearchKind::Location,
                            "New location",
                        ));
                        ui.close();
                    }
                    if ui.button("Import locations from script").clicked() {
                        actions.push(ResearchAction::ImportLocations);
                        ui.close();
                    }
                }
                kitsp::ResearchKind::Other => {
                    if ui.button("Add note").clicked() {
                        actions.push(ResearchAction::AddEntry(
                            kitsp::ResearchKind::Other,
                            "New entry",
                        ));
                        ui.close();
                    }
                }
            }
        });
    }
    if open {
        let ids: Vec<(i64, String)> = state
            .entries
            .iter()
            .filter(|e| e.kind == kind)
            .map(|e| (e.id, e.name.clone()))
            .collect();
        ui.indent(salt, |ui| {
            for (id, name) in &ids {
                let (c, _, _) = kit_tree_row(
                    ui,
                    KitTreeRow {
                        sel: *sel == ResearchPane::Entry(*id),
                        arrow: None,
                        plus: false,
                    },
                    icon,
                    name,
                    (salt, *id),
                );
                if c.clicked() {
                    *sel = ResearchPane::Entry(*id);
                }
            }
        });
    }
}

fn pane_view(ui: &mut Ui, state: &mut ResearchView<'_>, actions: &mut Vec<ResearchAction>) {
    match *state.pane {
        ResearchPane::Script => script_pane(ui, state, actions),
        ResearchPane::TitlePage => titlepage_pane(ui, state),
        ResearchPane::Logline => logline_pane(ui, state, false),
        ResearchPane::Synopsis => logline_pane(ui, state, true),
        ResearchPane::Versions => versions_pane(ui, state, actions),
        ResearchPane::Entry(id) => entry_pane(ui, state, actions, id),
        ResearchPane::Characters | ResearchPane::Locations | ResearchPane::Documents => {
            ui.label("Select an entry to edit it.");
        }
    }
}

fn script_pane(ui: &mut Ui, state: &mut ResearchView<'_>, actions: &mut Vec<ResearchAction>) {
    let mut dirty = false;
    kit_section(ui, "Script");
    dirty |= kit_field(ui, "Script Name", &mut state.meta.title);
    dirty |= kit_field(ui, "Header", &mut state.meta.header);
    dirty |= kit_field(ui, "Footer", &mut state.meta.footer);
    ui.label(
        egui::RichText::new("Header/footer tokens: {revision} {date} {page}")
            .size(11.0)
            .color(egui::Color32::from_gray(0xA1)),
    );
    dirty |= kit_field(
        ui,
        "Scene Number Prefix",
        &mut state.meta.scene_number_prefix,
    );
    dirty |= kit_field(
        ui,
        "Scene Numbering Starting at",
        &mut state.meta.scene_number_start,
    );
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        if kit_icon_button(ui, KitIcon::LockClosed, "Lock scenes numbers").clicked()
            && !state.scene_labels.is_empty()
        {
            let labels: Vec<String> = state
                .scene_labels
                .iter()
                .map(|label| label.trim_end_matches('.').to_string())
                .collect();
            state.meta.locked_scene_numbers = labels.join(",");
            dirty = true;
        }
        if kit_icon_button(ui, KitIcon::LockClosed, "Lock new scenes numbers").clicked() {
            let labels: Vec<String> = state
                .scene_labels
                .iter()
                .map(|label| label.trim_end_matches('.').to_string())
                .collect();
            state.meta.locked_scene_numbers = format!("*{}", labels.join(","));
            dirty = true;
        }
        if kit_icon_button(ui, KitIcon::LockOpen, "Unlock scenes numbers").clicked() {
            state.meta.locked_scene_numbers.clear();
            dirty = true;
        }
    });
    ui.add_space(6.0);
    kit_section(ui, "Page Locks");
    let page_lock = scriptcraft_core::page_lock::PageLock::from_json(&state.meta.locked_pages)
        .ok()
        .filter(|l| !l.is_empty());
    if let Some(lock) = page_lock {
        ui.label(format!(
            "Locked: {} page{} (inserted pages use A/B labels).",
            lock.pages,
            if lock.pages == 1 { "" } else { "s" }
        ));
    } else {
        ui.label(
            egui::RichText::new("Lock page starts before revisions so page references stay valid.")
                .size(11.0)
                .color(egui::Color32::from_gray(0xA1)),
        );
    }
    ui.horizontal_wrapped(|ui| {
        if kit_icon_button(ui, KitIcon::LockClosed, "Lock pages").clicked() {
            actions.push(ResearchAction::LockPages);
        }
        if kit_icon_button(ui, KitIcon::LockOpen, "Unlock pages").clicked() {
            actions.push(ResearchAction::UnlockPages);
        }
    });
    if dirty {
        *state.meta_dirty = true;
    }
}

fn titlepage_pane(ui: &mut Ui, state: &mut ResearchView<'_>) {
    if crate::title_page::title_page_editor(ui, state.meta, state.latest) {
        *state.meta_dirty = true;
    }
}

fn logline_pane(ui: &mut Ui, state: &mut ResearchView<'_>, synopsis: bool) {
    let title = if synopsis { "Synopsis" } else { "Logline" };
    let words = if synopsis {
        state.title_synopsis.split_whitespace().count()
    } else {
        state.title_logline.split_whitespace().count()
    };
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).size(15.0).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(format!("words: {words}")).size(12.0));
        });
    });
    ui.add_space(8.0);
    let available = ui.available_height() - 8.0;
    let text = if synopsis {
        &mut *state.title_synopsis
    } else {
        &mut *state.title_logline
    };
    if kit_mono_editor(ui, text, available.max(120.0)) {
        *state.meta_dirty = true;
    }
}

fn versions_pane(ui: &mut Ui, state: &mut ResearchView<'_>, actions: &mut Vec<ResearchAction>) {
    kit_section(ui, "Versions");
    let can_write = !state.read_only && !state.path.is_empty();
    if ui
        .add_enabled(can_write, egui::Button::new("Save Current as Version…"))
        .on_disabled_hover_text("Save the project before creating a version.")
        .clicked()
    {
        actions.push(ResearchAction::SaveVersionDialog);
    }
    if !can_write {
        ui.label(
            egui::RichText::new("Save the project first — fixture is read-only.")
                .size(11.0)
                .color(egui::Color32::from_gray(0xA1)),
        );
    }
    ui.add_space(8.0);
    if state.versions.is_empty() {
        ui.label(egui::RichText::new("No versions in this project.").size(13.0));
    } else {
        for (name, when) in state.versions {
            let (rect, _) = ui
                .allocate_exact_size(egui::vec2(ui.available_width(), 40.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 2.0, kit().strip);
            draw_kit_icon(
                ui.painter(),
                KitIcon::Layers,
                egui::pos2(rect.left() + 18.0, rect.center().y),
                kit().text,
            );
            ui.painter().text(
                egui::pos2(rect.left() + 36.0, rect.center().y - 7.0),
                egui::Align2::LEFT_CENTER,
                name,
                egui::FontId::proportional(13.0),
                kit().text,
            );
            ui.painter().text(
                egui::pos2(rect.left() + 36.0, rect.center().y + 8.0),
                egui::Align2::LEFT_CENTER,
                when,
                egui::FontId::proportional(11.5),
                egui::Color32::from_gray(0xA1),
            );
            ui.add_space(6.0);
        }
    }
    ui.add_space(4.0);
    if ui
        .add_enabled(can_write, egui::Button::new("Import Versions as Revisions"))
        .clicked()
    {
        actions.push(ResearchAction::ImportVersions);
    }
    ui.label(
        egui::RichText::new("Each saved KIT version becomes a revision baseline.")
            .size(11.0)
            .color(egui::Color32::from_gray(0xA1)),
    );
    ui.add_space(16.0);
    kit_section(ui, "Revisions");
    let has = state.has_project;
    let next_color = state.revisions.next_color();
    if ui
        .add_enabled(
            has,
            egui::Button::new(format!("Start {} Revision", next_color.label())),
        )
        .on_disabled_hover_text("Open a project to start a revision.")
        .clicked()
    {
        actions.push(ResearchAction::StartRevision);
    }
    ui.add_space(4.0);
    if ui
        .add_enabled(has, egui::Button::new("Revision History…"))
        .clicked()
    {
        actions.push(ResearchAction::ShowHistory);
    }
    ui.add_space(4.0);
    if ui
        .add_enabled(
            has && !state.revisions.is_empty(),
            egui::Button::new("Revision Summary…"),
        )
        .on_disabled_hover_text("Start a revision to summarize changes.")
        .clicked()
    {
        actions.push(ResearchAction::OpenSummary);
    }
    if !state.revisions.is_empty() {
        ui.add_space(6.0);
        if ui
            .checkbox(
                &mut *state.show_current_revision,
                "Show current revision only",
            )
            .changed()
        {
            *state.revision_marks_dirty = true;
        }
        ui.add_space(4.0);
        if ui
            .add_enabled(true, egui::Button::new("Clear Revision History…"))
            .clicked()
        {
            actions.push(ResearchAction::ClearHistory);
        }
    }
    ui.label(
        egui::RichText::new(
            "Revision colours mark changed lines and print in the PDF. Versions above are KIT's named snapshots.",
        )
        .size(11.0)
        .color(egui::Color32::from_gray(0xA1)),
    );
}

fn entry_pane(
    ui: &mut Ui,
    state: &mut ResearchView<'_>,
    actions: &mut Vec<ResearchAction>,
    id: i64,
) {
    if *state.research_sel != Some(id) {
        *state.research_sel = Some(id);
        *state.research_edit = sync_buffer(state.entries, Some(id));
    }
    if state.research_edit.is_none() {
        *state.research_edit = sync_buffer(state.entries, *state.research_sel);
    }
    // Refresh the picture texture when the stored bytes change.
    let photo = state
        .entries
        .iter()
        .find(|e| e.id == id)
        .and_then(|e| e.image.clone());
    let want = photo.as_ref().map(|b| (id, image_hash(b)));
    if state.research_tex.as_ref().map(|(i, h, _)| (*i, *h)) != want {
        *state.research_tex = None;
        if let Some(bytes) = &photo {
            if let Some(img) = decode_image(bytes) {
                let tex = ui.ctx().load_texture(
                    format!("research-photo-{id}"),
                    img,
                    egui::TextureOptions::LINEAR,
                );
                *state.research_tex = Some((id, image_hash(bytes), tex));
            }
        }
    }

    let writable = !state.read_only && !state.path.is_empty();
    let mut save = false;
    let mut del = false;
    let mut pick_photo = false;
    let mut remove_photo = false;
    if let Some(edit) = state.research_edit.as_mut() {
        let kind_label = match edit.kind {
            kitsp::ResearchKind::Character => "Character",
            kitsp::ResearchKind::Location => "Location",
            kitsp::ResearchKind::Other => "Entry",
        };
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(edit.name.clone()).size(17.0));
                ui.label(egui::RichText::new(kind_label).size(12.0));
            });
            ui.with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(220.0, 150.0), egui::Sense::hover());
                let p = ui.painter();
                p.rect_filled(rect, 2.0, kit().base);
                p.rect_stroke(
                    rect,
                    2.0,
                    egui::Stroke::new(1.0_f32, kit().field_stroke),
                    egui::StrokeKind::Inside,
                );
                if let Some((_, _, tex)) = state.research_tex.as_ref() {
                    let size = tex.size_vec2();
                    let scale = (rect.width() / size.x).min(rect.height() / size.y).min(2.0);
                    let draw = egui::Rect::from_center_size(rect.center(), size * scale);
                    p.image(
                        tex.id(),
                        draw,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                } else {
                    p.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "No picture",
                        egui::FontId::proportional(12.0),
                        kit().disabled_text,
                    );
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    pick_photo = kit_button(ui, "Picture…").clicked();
                    remove_photo = ui
                        .add_enabled(
                            state.research_tex.is_some() && writable,
                            egui::Button::new("Remove").min_size(egui::vec2(0.0, 22.0)),
                        )
                        .clicked();
                });
            });
        });
        ui.add_space(8.0);
        kit_field(ui, "Name", &mut edit.name);
        kit_field(ui, "URL", &mut edit.url);
        kit_field_multi(ui, "Description", &mut edit.description, 8);
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            save = kit_button(ui, "Save").clicked();
            del = kit_button(ui, "Delete").clicked();
        });
    } else {
        ui.label("Select an entry to edit it.");
    }
    if save {
        actions.push(ResearchAction::SaveEntry);
    }
    if del {
        actions.push(ResearchAction::DeleteEntry);
    }
    if pick_photo {
        if let Some(f) = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg"])
            .pick_file()
        {
            match std::fs::read(&f) {
                Ok(bytes) if decode_image(&bytes).is_some() => {
                    actions.push(ResearchAction::SetPhoto(id, Some(bytes)));
                }
                Ok(_) => *state.status = "Unsupported image file.".to_string(),
                Err(e) => *state.status = format!("Can't read picture: {e}"),
            }
        }
    }
    if remove_photo {
        actions.push(ResearchAction::SetPhoto(id, None));
    }
}

/// Copy one entry into the edit buffer (same behavior as the app helper).
pub(crate) fn sync_buffer(
    entries: &[kitsp::ResearchEntry],
    sel: Option<i64>,
) -> Option<ResearchEdit> {
    entries
        .iter()
        .find(|e| Some(e.id) == sel)
        .map(|e| ResearchEdit {
            id: e.id,
            kind: e.kind,
            name: e.name.clone(),
            url: e.url.clone(),
            description: e.description.clone(),
        })
}

/// Decode a stored KIT research picture (PNG or JPEG) into an egui image.
fn decode_image(bytes: &[u8]) -> Option<egui::ColorImage> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}

fn image_hash(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}
