//! Read-only draft comparison workspace.
//!
//! The diff engine lives in `scriptcraft_core::revision`; this module only
//! presents it. It never edits the project, the revision history or the
//! current undo stack.

use egui::{Align, Color32, Key, Label, Modifiers, RichText, ScrollArea, Sense, Ui, Vec2};
use scriptcraft_core::revision::{DiffEntry, DiffKind, DraftDiff, RevisionHistory, RevisionSet};
use scriptcraft_core::{ElementType, Project, ScriptBlock};
use uuid::Uuid;

use crate::theme::kit;

const ROW_HEIGHT: f32 = 23.0;
const LIST_ROW_HEIGHT: f32 = 19.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompareTarget {
    Current,
    Revision(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompareView {
    Unified,
    SideBySide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeFilter {
    All,
    Added,
    Removed,
    Changed,
}

impl ChangeFilter {
    pub const ALL: [ChangeFilter; 4] = [Self::All, Self::Added, Self::Removed, Self::Changed];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Added => "Added",
            Self::Removed => "Removed",
            Self::Changed => "Changed",
        }
    }

    fn matches(self, kind: DiffKind) -> bool {
        match self {
            Self::All => kind.is_change(),
            Self::Added => kind == DiffKind::Added,
            Self::Removed => kind == DiffKind::Removed,
            Self::Changed => kind == DiffKind::Changed,
        }
    }
}

/// One aligned block in the comparison, cached until the selection changes.
pub struct CompareState {
    base: usize,
    target: CompareTarget,
    view: CompareView,
    filter: ChangeFilter,
    selected: Option<usize>,
    /// Entry index the main view should scroll to on the next frame.
    scroll_to: Option<usize>,
    /// Entry index the change list should reveal on the next frame.
    reveal_in_list: Option<usize>,
    diff: Option<DraftDiff>,
    dirty: bool,
}

impl CompareState {
    /// Open with the latest revision as the base and the live draft as target.
    pub fn open_latest(history: &RevisionHistory, current: &[ScriptBlock]) -> Option<Self> {
        let base = history.sets.len().checked_sub(1)?;
        let mut state = Self {
            base,
            target: CompareTarget::Current,
            view: CompareView::Unified,
            filter: ChangeFilter::All,
            selected: None,
            scroll_to: None,
            reveal_in_list: None,
            diff: None,
            dirty: true,
        };
        state.sync(history, current);
        Some(state)
    }

    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    /// Recompute the diff when the selection or source data changed.
    pub fn sync(&mut self, history: &RevisionHistory, current: &[ScriptBlock]) {
        if !self.dirty && self.diff.is_some() {
            return;
        }
        self.dirty = false;
        if history.sets.is_empty() {
            self.diff = None;
            self.selected = None;
            return;
        }
        self.base = self.base.min(history.sets.len() - 1);
        if let CompareTarget::Revision(index) = self.target {
            if index >= history.sets.len() || index == self.base {
                self.target = CompareTarget::Current;
            }
        }
        let diff = match self.target {
            CompareTarget::Current => history.sets[self.base].diff_against_current(current),
            CompareTarget::Revision(index) => {
                history.sets[self.base].diff_against(&history.sets[index])
            }
        };
        if self.selected.is_none() {
            self.selected = diff.entries.iter().position(|entry| entry.kind.is_change());
            self.scroll_to = self.selected;
        } else if self
            .selected
            .is_some_and(|index| index >= diff.entries.len())
        {
            self.selected = None;
        }
        self.diff = Some(diff);
    }

    pub fn diff(&self) -> Option<&DraftDiff> {
        self.diff.as_ref()
    }

    pub fn base(&self) -> usize {
        self.base
    }

    pub fn target(&self) -> CompareTarget {
        self.target
    }

    pub fn view(&self) -> CompareView {
        self.view
    }

    pub fn filter(&self) -> ChangeFilter {
        self.filter
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn set_base(&mut self, base: usize, history: &RevisionHistory) {
        if history.sets.is_empty() {
            return;
        }
        self.base = base.min(history.sets.len() - 1);
        if self.target == CompareTarget::Revision(self.base) {
            self.target = CompareTarget::Current;
        }
        self.selected = None;
        self.invalidate();
    }

    pub fn set_target(&mut self, target: CompareTarget) {
        self.target = target;
        self.selected = None;
        self.invalidate();
    }

    pub fn set_view(&mut self, view: CompareView) {
        self.view = view;
    }

    pub fn toggle_view(&mut self) {
        self.view = match self.view {
            CompareView::Unified => CompareView::SideBySide,
            CompareView::SideBySide => CompareView::Unified,
        };
    }

    pub fn set_filter(&mut self, filter: ChangeFilter) {
        self.filter = filter;
    }

    /// Indices of the changes matching the active filter.
    pub fn change_indices(&self) -> Vec<usize> {
        let Some(diff) = &self.diff else {
            return Vec::new();
        };
        diff.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| self.filter.matches(entry.kind))
            .map(|(index, _)| index)
            .collect()
    }

    /// Select `index` in the main view and reveal it in the change list.
    pub fn select_entry(&mut self, index: usize) {
        self.selected = Some(index);
        self.scroll_to = Some(index);
        self.reveal_in_list = Some(index);
    }

    /// Move to the next/previous change in the active filter. Returns whether
    /// a change was found.
    pub fn step_change(&mut self, forward: bool) -> bool {
        let changes = self.change_indices();
        if changes.is_empty() {
            self.selected = None;
            return false;
        }
        let position = self
            .selected
            .and_then(|selected| changes.iter().position(|&index| index == selected));
        let next = match (position, forward) {
            (None, true) => 0,
            (None, false) => changes.len() - 1,
            (Some(position), true) => (position + 1) % changes.len(),
            (Some(position), false) => (position + changes.len() - 1) % changes.len(),
        };
        self.selected = Some(changes[next]);
        self.scroll_to = self.selected;
        self.reveal_in_list = self.selected;
        true
    }

    pub fn take_scroll_row(&mut self) -> Option<usize> {
        self.scroll_to.take()
    }

    pub fn take_list_reveal(&mut self) -> Option<usize> {
        self.reveal_in_list.take()
    }

    /// The editor block a jump should land on. Removed blocks and scenes fall
    /// back to the first block of the nearest surviving scene.
    pub fn jump_block(&self, project: Option<&Project>) -> Option<usize> {
        let project = project?;
        let entry = self.diff.as_ref()?.entries.get(self.selected?)?;
        if self.target == CompareTarget::Current {
            if let Some(block) = entry.current_block {
                return Some(block);
            }
        } else if let Some(current) = &entry.current {
            // Comparing revisions: map the target baseline block into the
            // live draft by stable id, then by heading text.
            if current.id != Uuid::nil() {
                if let Some(index) = project.blocks.iter().position(|b| b.id == current.id) {
                    return Some(index);
                }
            }
            if let Some(index) = project.blocks.iter().position(|b| {
                b.element_type == ElementType::SceneHeading && b.text == entry.scene_heading
            }) {
                return Some(index);
            }
        }
        if project.blocks.is_empty() {
            return None;
        }
        let scenes = project.scenes();
        if scenes.is_empty() {
            return Some(project.blocks.len() - 1);
        }
        let index = entry.scene_index.min(scenes.len() - 1);
        scenes[index].blocks.first().copied()
    }

    /// Number of scenes in the target draft (current or revision baseline).
    pub fn target_scene_count(&self, history: &RevisionHistory, current: &[ScriptBlock]) -> usize {
        match self.target {
            CompareTarget::Current => current
                .iter()
                .filter(|block| block.element_type == ElementType::SceneHeading)
                .count(),
            CompareTarget::Revision(index) => history
                .sets
                .get(index)
                .map(|set| {
                    set.baseline
                        .iter()
                        .filter(|block| block.element_type == ElementType::SceneHeading)
                        .count()
                })
                .unwrap_or(0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompareAction {
    None,
    Close,
    Jump(usize),
}

fn set_label(set: &RevisionSet) -> String {
    let name = if set.name.is_empty() {
        format!("{} Revision", set.color.label())
    } else {
        set.name.clone()
    };
    if set.started.is_empty() {
        name
    } else {
        format!("{name} · {}", set.started)
    }
}

fn target_label(target: CompareTarget, history: &RevisionHistory) -> String {
    match target {
        CompareTarget::Current => "Current draft".to_string(),
        CompareTarget::Revision(index) => history
            .sets
            .get(index)
            .map(set_label)
            .unwrap_or_else(|| "Revision".to_string()),
    }
}

fn kind_color(kind: DiffKind) -> Color32 {
    match kind {
        DiffKind::Unchanged => kit().disabled_text,
        DiffKind::Added => Color32::from_rgb(0x3E, 0x9E, 0x62),
        DiffKind::Removed => Color32::from_rgb(0xD6, 0x55, 0x55),
        DiffKind::Changed => Color32::from_rgb(0xD2, 0x96, 0x2B),
    }
}

fn kind_mark(kind: DiffKind) -> &'static str {
    match kind {
        DiffKind::Unchanged => " ",
        DiffKind::Added => "+",
        DiffKind::Removed => "−",
        DiffKind::Changed => "~",
    }
}

fn entry_base_text(entry: &DiffEntry) -> &str {
    entry
        .base
        .as_ref()
        .map(|block| block.text.as_str())
        .unwrap_or("")
}

fn entry_current_text(entry: &DiffEntry) -> &str {
    entry
        .current
        .as_ref()
        .map(|block| block.text.as_str())
        .unwrap_or("")
}

fn elide(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

struct SceneChanges<'a> {
    scene_index: usize,
    heading: &'a str,
    entries: Vec<usize>,
    added: usize,
    removed: usize,
    changed: usize,
}

fn scene_changes<'a>(diff: &'a DraftDiff, filter: ChangeFilter) -> Vec<SceneChanges<'a>> {
    let mut groups: Vec<SceneChanges<'a>> = Vec::new();
    for (index, entry) in diff.entries.iter().enumerate() {
        if !filter.matches(entry.kind) {
            continue;
        }
        let same_scene = groups.last().is_some_and(|group| {
            group.scene_index == entry.scene_index && group.heading == entry.scene_heading
        });
        if !same_scene {
            groups.push(SceneChanges {
                scene_index: entry.scene_index,
                heading: entry.scene_heading.as_str(),
                entries: Vec::new(),
                added: 0,
                removed: 0,
                changed: 0,
            });
        }
        let group = groups.last_mut().expect("group just inserted");
        group.entries.push(index);
        match entry.kind {
            DiffKind::Added => group.added += 1,
            DiffKind::Removed => group.removed += 1,
            DiffKind::Changed => group.changed += 1,
            DiffKind::Unchanged => {}
        }
    }
    groups
}

fn counts_label(added: usize, removed: usize, changed: usize) -> String {
    let mut parts = Vec::new();
    if added > 0 {
        parts.push(format!("+{added}"));
    }
    if removed > 0 {
        parts.push(format!("−{removed}"));
    }
    if changed > 0 {
        parts.push(format!("~{changed}"));
    }
    parts.join(" ")
}

fn scene_title(group: &SceneChanges<'_>, scene_count: usize) -> String {
    let number = if group.scene_index < scene_count {
        format!("Scene {}", group.scene_index + 1)
    } else {
        format!("After scene {scene_count}")
    };
    let heading = if group.heading.is_empty() {
        "(no heading)".to_string()
    } else {
        elide(group.heading, 34)
    };
    format!("{number} · {heading}")
}

/// Draw the compare workspace. Returns an action for the app to apply after
/// the frame (close the workspace or open a block in the editor).
pub fn show(
    ctx: &egui::Context,
    state: &mut CompareState,
    history: &RevisionHistory,
    current: &[ScriptBlock],
    project: Option<&Project>,
) -> CompareAction {
    state.sync(history, current);
    if history.sets.is_empty() {
        return CompareAction::Close;
    }

    // Keyboard: J/K or arrows move through changes, Enter opens the selected
    // change in the editor, V toggles the view, Escape closes.
    if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
        return CompareAction::Close;
    }
    if ctx.input_mut(|input| {
        input.consume_key(Modifiers::NONE, Key::J)
            || input.consume_key(Modifiers::NONE, Key::ArrowDown)
    }) {
        state.step_change(true);
    }
    if ctx.input_mut(|input| {
        input.consume_key(Modifiers::NONE, Key::K)
            || input.consume_key(Modifiers::NONE, Key::ArrowUp)
    }) {
        state.step_change(false);
    }
    if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::V)) {
        state.toggle_view();
    }
    if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter)) {
        if let Some(block) = state.jump_block(project) {
            return CompareAction::Jump(block);
        }
    }

    let mut action = CompareAction::None;
    let scene_count = state.target_scene_count(history, current);

    egui::TopBottomPanel::top("compare_header")
        .frame(
            egui::Frame::new()
                .fill(kit().window)
                .inner_margin(egui::Margin::symmetric(12, 8)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Compare Drafts")
                        .strong()
                        .size(16.0)
                        .color(kit().heading_text),
                );
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);
                ui.label("Base");
                let mut base = state.base;
                egui::ComboBox::from_id_salt("compare_base")
                    .width(190.0)
                    .selected_text(history.sets.get(base).map(set_label).unwrap_or_default())
                    .show_ui(ui, |ui| {
                        for (index, set) in history.sets.iter().enumerate() {
                            ui.selectable_value(&mut base, index, set_label(set));
                        }
                    });
                if base != state.base {
                    state.set_base(base, history);
                }
                ui.label("→");
                let mut target = state.target;
                egui::ComboBox::from_id_salt("compare_target")
                    .width(170.0)
                    .selected_text(target_label(target, history))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut target, CompareTarget::Current, "Current draft");
                        for (index, set) in history.sets.iter().enumerate() {
                            if index == state.base {
                                continue;
                            }
                            ui.selectable_value(
                                &mut target,
                                CompareTarget::Revision(index),
                                set_label(set),
                            );
                        }
                    });
                if target != state.target {
                    state.set_target(target);
                }
                ui.separator();
                if ui
                    .selectable_label(state.view == CompareView::Unified, "Unified")
                    .clicked()
                {
                    state.set_view(CompareView::Unified);
                }
                if ui
                    .selectable_label(state.view == CompareView::SideBySide, "Side by side")
                    .clicked()
                {
                    state.set_view(CompareView::SideBySide);
                }
                ui.separator();
                if ui.button("Previous change").clicked() {
                    state.step_change(false);
                }
                if ui.button("Next change").clicked() {
                    state.step_change(true);
                }
                let can_jump = state.selected.is_some();
                if ui
                    .add_enabled(can_jump, egui::Button::new("Open in editor"))
                    .clicked()
                {
                    if let Some(block) = state.jump_block(project) {
                        action = CompareAction::Jump(block);
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Close").clicked() {
                        action = CompareAction::Close;
                    }
                });
            });
            if let Some(diff) = state.diff() {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "{} added · {} removed · {} changed · {} scene(s) affected",
                            diff.added,
                            diff.removed,
                            diff.changed,
                            diff.scenes_changed.len()
                        ))
                        .small()
                        .color(kit().disabled_text),
                    );
                    if !diff.has_changes() {
                        ui.label(
                            RichText::new("The two drafts are identical.")
                                .small()
                                .color(kit().disabled_text),
                        );
                    }
                });
            }
        });

    egui::SidePanel::left("compare_changes")
        .resizable(true)
        .default_width(300.0)
        .width_range(220.0..=460.0)
        .frame(
            egui::Frame::new()
                .fill(kit().window)
                .inner_margin(egui::Margin::symmetric(10, 8)),
        )
        .show(ctx, |ui| {
            let selected = state.selected;
            let reveal = state.take_list_reveal();
            let mut filter = state.filter;
            let Some(diff) = state.diff() else {
                return;
            };
            ui.label(RichText::new("Changes").strong());
            ui.add_space(4.0);
            let total_changes = diff
                .entries
                .iter()
                .filter(|entry| entry.kind.is_change())
                .count();
            ui.horizontal_wrapped(|ui| {
                for option in ChangeFilter::ALL {
                    let count = diff
                        .entries
                        .iter()
                        .filter(|entry| option.matches(entry.kind))
                        .count();
                    let label = if option == ChangeFilter::All {
                        format!("All {count}")
                    } else {
                        format!("{} {count}", option.label())
                    };
                    ui.selectable_value(&mut filter, option, label);
                }
            });
            if total_changes == 0 {
                ui.add_space(12.0);
                ui.label(
                    RichText::new("No changes to list.")
                        .small()
                        .color(kit().disabled_text),
                );
            }
            ui.add_space(4.0);
            ui.separator();
            let mut clicked = None;
            let groups = scene_changes(diff, filter);
            ScrollArea::vertical()
                .id_salt("compare_change_list")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for group in &groups {
                        let counts = counts_label(group.added, group.removed, group.changed);
                        let title = format!("{}   {counts}", scene_title(group, scene_count));
                        let force_open =
                            selected.is_some_and(|selected| group.entries.contains(&selected));
                        let mut header =
                            egui::CollapsingHeader::new(RichText::new(title).small().strong())
                                .id_salt(("compare_scene", group.scene_index, group.heading));
                        if force_open {
                            header = header.open(Some(true));
                        }
                        header.show(ui, |ui| {
                            for &index in &group.entries {
                                let entry = &diff.entries[index];
                                let text = if entry.kind == DiffKind::Removed {
                                    entry_base_text(entry)
                                } else {
                                    entry_current_text(entry)
                                };
                                let text = elide(text, 74);
                                let marker = kind_mark(entry.kind);
                                let rich = RichText::new(format!("{marker} {text}"))
                                    .small()
                                    .color(kind_color(entry.kind));
                                let response = ui.add_sized(
                                    [ui.available_width(), LIST_ROW_HEIGHT],
                                    egui::Button::selectable(selected == Some(index), rich),
                                );
                                let response = response.on_hover_text(format!(
                                    "{}\n\n{}",
                                    entry_base_text(entry),
                                    entry_current_text(entry)
                                ));
                                if response.clicked() {
                                    clicked = Some(index);
                                }
                                if reveal == Some(index) {
                                    response.scroll_to_me(Some(Align::Center));
                                }
                            }
                        });
                    }
                });
            drop(groups);
            if filter != state.filter {
                state.set_filter(filter);
            }
            if let Some(index) = clicked {
                state.select_entry(index);
            }
            ui.add_space(4.0);
            ui.label(
                RichText::new("Read-only. No script or revision data is changed.")
                    .small()
                    .color(kit().disabled_text),
            );
        });

    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(kit().desk)
                .inner_margin(egui::Margin::same(0)),
        )
        .show(ctx, |ui| {
            let scroll_row = state.take_scroll_row();
            let Some(diff) = state.diff() else {
                return;
            };
            if diff.entries.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label("Nothing to compare.");
                });
                return;
            }
            let base_name = history
                .sets
                .get(state.base)
                .map(set_label)
                .unwrap_or_default();
            let target_name = target_label(state.target, history);
            let mut clicked = None;
            let mut double_clicked = None;
            let selected = state.selected;
            let view = state.view;

            egui::Frame::new()
                .fill(kit().module_bg)
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if view == CompareView::SideBySide {
                            let width = ui.available_width();
                            let column = ((width - 80.0) / 2.0).max(80.0);
                            ui.add_sized(
                                [column, 18.0],
                                Label::new(
                                    RichText::new(format!("Base · {base_name}"))
                                        .small()
                                        .strong(),
                                )
                                .truncate(),
                            );
                            ui.add_space(48.0);
                            ui.add_sized(
                                [column, 18.0],
                                Label::new(
                                    RichText::new(format!("Target · {target_name}"))
                                        .small()
                                        .strong(),
                                )
                                .truncate(),
                            );
                        } else {
                            ui.label(
                                RichText::new(format!("{base_name} → {target_name}"))
                                    .small()
                                    .strong(),
                            );
                        }
                    });
                });

            let mut area = ScrollArea::vertical()
                .id_salt("compare_main")
                .auto_shrink([false, false]);
            if let Some(row) = scroll_row {
                let viewport = ui.available_height();
                let offset = (row as f32 * ROW_HEIGHT - viewport * 0.35).max(0.0);
                area = area.vertical_scroll_offset(offset);
            }
            ui.spacing_mut().item_spacing.y = 0.0;
            area.show_rows(ui, ROW_HEIGHT, diff.entries.len(), |ui, range| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for index in range {
                    let entry = &diff.entries[index];
                    let response = diff_row(ui, entry, index, view, selected == Some(index));
                    if response.clicked() {
                        clicked = Some(index);
                    }
                    if response.double_clicked() {
                        double_clicked = Some(index);
                    }
                }
            });
            if let Some(index) = clicked {
                state.select_entry(index);
            }
            if let Some(index) = double_clicked {
                state.select_entry(index);
                if let Some(block) = state.jump_block(project) {
                    action = CompareAction::Jump(block);
                }
            }
        });

    action
}

fn diff_row(
    ui: &mut Ui,
    entry: &DiffEntry,
    index: usize,
    view: CompareView,
    selected: bool,
) -> egui::Response {
    let width = ui.available_width().max(120.0);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, ROW_HEIGHT), Sense::click());
    let background = if selected {
        kit().accent_soft
    } else if response.hovered() {
        kit().hover_light
    } else {
        Color32::TRANSPARENT
    };
    if background != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 0.0, background);
    }
    let inner = rect.shrink2(Vec2::new(6.0, 0.0));
    let mut row = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(Align::Center)),
    );
    row.spacing_mut().item_spacing.x = 6.0;
    row.add_sized(
        [14.0, ROW_HEIGHT],
        Label::new(
            RichText::new(kind_mark(entry.kind))
                .strong()
                .color(kind_color(entry.kind)),
        ),
    );
    let left = entry_base_text(entry);
    let right = entry_current_text(entry);
    let left_color = if entry.kind == DiffKind::Removed {
        kind_color(DiffKind::Removed)
    } else if entry.kind == DiffKind::Changed {
        kit().disabled_text
    } else {
        kit().text
    };
    let right_color = if entry.kind == DiffKind::Added || entry.kind == DiffKind::Changed {
        kind_color(entry.kind)
    } else if entry.kind == DiffKind::Unchanged {
        kit().disabled_text
    } else {
        kit().text
    };
    let text_width = (width - 34.0).max(50.0);
    match view {
        CompareView::Unified => {
            let (text, color) = if entry.kind == DiffKind::Removed {
                (left, left_color)
            } else {
                (right, right_color)
            };
            row.add_sized(
                [text_width, ROW_HEIGHT],
                Label::new(RichText::new(text).monospace().color(color)).truncate(),
            );
        }
        CompareView::SideBySide => {
            let column = ((width - 46.0) / 2.0).max(50.0);
            row.add_sized(
                [column, ROW_HEIGHT],
                Label::new(RichText::new(left).monospace().color(left_color)).truncate(),
            );
            row.add_sized(
                [column, ROW_HEIGHT],
                Label::new(RichText::new(right).monospace().color(right_color)).truncate(),
            );
        }
    }
    let _ = index;
    response.on_hover_text(format!(
        "{}\n\nBase: {}\nTarget: {}",
        entry.scene_heading,
        if left.is_empty() { "—" } else { left },
        if right.is_empty() { "—" } else { right }
    ))
}
