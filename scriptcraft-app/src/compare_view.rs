//! Read-only draft comparison workspace.
//!
//! The diff engine lives in `scriptcraft_core::revision`; this module only
//! presents it. It never edits the project, the revision history or the
//! current undo stack.

use egui::{
    text::LayoutJob, Align, Align2, Color32, FontId, Key, Label, Modifiers, Pos2, Rect, RichText,
    ScrollArea, Sense, Stroke, TextFormat, Ui, Vec2,
};
use scriptcraft_core::revision::{DiffEntry, DiffKind, DraftDiff, RevisionHistory, RevisionSet};
use scriptcraft_core::{ElementType, Project, ScriptBlock};
use uuid::Uuid;

use crate::theme::{kit, kit_is_light};

const ROW_HEIGHT: f32 = 26.0;
const LIST_ROW_HEIGHT: f32 = 21.0;
const DIFF_FONT_SIZE: f32 = 12.5;

const KEYBOARD_HINT: &str = "J/K navigate · V switch view · Enter open · Esc close";

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
    /// Save the base revision as a new project without touching this draft.
    Restore(usize),
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
        DiffKind::Removed => "-",
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

/// Soft tint painted behind a row of the given kind.
fn kind_fill(kind: DiffKind) -> Color32 {
    if !kind.is_change() {
        return Color32::TRANSPARENT;
    }
    let color = kind_color(kind);
    let alpha = if kit_is_light() { 22 } else { 44 };
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

/// Slightly stronger tint used for side-by-side cells.
fn cell_fill(kind: DiffKind) -> Color32 {
    let color = kind_color(kind);
    let alpha = if kit_is_light() { 30 } else { 56 };
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

/// The differing middle of two changed lines, with the common prefix and
/// suffix split out so only the edit itself is highlighted.
#[derive(Debug, PartialEq, Eq)]
struct InlineDiff<'a> {
    prefix: &'a str,
    old_mid: &'a str,
    new_mid: &'a str,
    suffix: &'a str,
}

fn inline_diff<'a>(old: &'a str, new: &'a str) -> InlineDiff<'a> {
    let old_chars: Vec<(usize, char)> = old.char_indices().collect();
    let new_chars: Vec<(usize, char)> = new.char_indices().collect();

    let mut prefix_chars = 0;
    while prefix_chars < old_chars.len()
        && prefix_chars < new_chars.len()
        && old_chars[prefix_chars].1 == new_chars[prefix_chars].1
    {
        prefix_chars += 1;
    }
    let mut suffix_chars = 0;
    while suffix_chars < old_chars.len().saturating_sub(prefix_chars)
        && suffix_chars < new_chars.len().saturating_sub(prefix_chars)
        && old_chars[old_chars.len() - 1 - suffix_chars].1
            == new_chars[new_chars.len() - 1 - suffix_chars].1
    {
        suffix_chars += 1;
    }

    let prefix = if prefix_chars == 0 {
        ""
    } else if prefix_chars == old_chars.len() {
        old
    } else {
        &old[..old_chars[prefix_chars].0]
    };
    let old_end = old_chars.len() - suffix_chars;
    let new_end = new_chars.len() - suffix_chars;
    let old_start_byte = old_chars
        .get(prefix_chars)
        .map_or(old.len(), |(byte, _)| *byte);
    let old_end_byte = old_chars.get(old_end).map_or(old.len(), |(byte, _)| *byte);
    let new_start_byte = new_chars
        .get(prefix_chars)
        .map_or(new.len(), |(byte, _)| *byte);
    let new_end_byte = new_chars.get(new_end).map_or(new.len(), |(byte, _)| *byte);
    let old_mid = &old[old_start_byte..old_end_byte];
    let new_mid = &new[new_start_byte..new_end_byte];
    let suffix = if suffix_chars == 0 {
        ""
    } else {
        &old[old_end_byte..]
    };
    InlineDiff {
        prefix,
        old_mid,
        new_mid,
        suffix,
    }
}

fn text_job(width: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = width.max(1.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');
    job
}

/// Keep the end of a long prefix so the actual edit stays on screen. A diff
/// that begins off-screen is not useful, however correct the colours are.
fn tail_with_ellipsis(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    let mut out = String::from("… ");
    out.extend(text.chars().skip(count - max_chars));
    out
}

fn chars_for_width(width: f32) -> usize {
    // Courier Prime at DIFF_FONT_SIZE is roughly 7.6 px per character.
    (width / 7.6).max(20.0) as usize
}

fn push_text(job: &mut LayoutJob, text: &str, font: &FontId, color: Color32, strike: bool) {
    if text.is_empty() {
        return;
    }
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: font.clone(),
            color,
            strikethrough: if strike {
                Stroke::new(1.0_f32, color)
            } else {
                Stroke::NONE
            },
            ..Default::default()
        },
    );
}

fn unified_job(entry: &DiffEntry, width: f32) -> LayoutJob {
    let c = kit();
    let font = FontId::monospace(DIFF_FONT_SIZE);
    let mut job = text_job(width);
    let left = entry_base_text(entry);
    let right = entry_current_text(entry);
    match entry.kind {
        DiffKind::Unchanged => push_text(&mut job, right, &font, c.disabled_text, false),
        DiffKind::Added => push_text(&mut job, right, &font, kind_color(DiffKind::Added), false),
        DiffKind::Removed => push_text(&mut job, left, &font, kind_color(DiffKind::Removed), true),
        DiffKind::Changed => {
            let diff = inline_diff(left, right);
            let budget = chars_for_width(width);
            let change_len = diff.old_mid.chars().count() + diff.new_mid.chars().count() + 3;
            let prefix = if diff.prefix.chars().count() + change_len > budget {
                tail_with_ellipsis(diff.prefix, budget.saturating_sub(change_len + 6).max(10))
            } else {
                diff.prefix.to_string()
            };
            push_text(&mut job, &prefix, &font, c.text, false);
            push_text(
                &mut job,
                diff.old_mid,
                &font,
                kind_color(DiffKind::Removed),
                true,
            );
            if !diff.old_mid.is_empty() && !diff.new_mid.is_empty() {
                push_text(&mut job, " -> ", &font, c.disabled_text, false);
            }
            push_text(
                &mut job,
                diff.new_mid,
                &font,
                kind_color(DiffKind::Added),
                false,
            );
            push_text(&mut job, diff.suffix, &font, c.text, false);
        }
    }
    job
}

fn side_job(entry: &DiffEntry, old_side: bool, width: f32) -> LayoutJob {
    let c = kit();
    let font = FontId::monospace(DIFF_FONT_SIZE);
    let mut job = text_job(width);
    let left = entry_base_text(entry);
    let right = entry_current_text(entry);
    match (entry.kind, old_side) {
        (DiffKind::Unchanged, true) => push_text(&mut job, left, &font, c.disabled_text, false),
        (DiffKind::Unchanged, false) => push_text(&mut job, right, &font, c.disabled_text, false),
        (DiffKind::Added, true) | (DiffKind::Removed, false) => {
            push_text(&mut job, "—", &font, c.disabled_text, false);
        }
        (DiffKind::Added, false) => {
            push_text(&mut job, right, &font, kind_color(DiffKind::Added), false)
        }
        (DiffKind::Removed, true) => {
            push_text(&mut job, left, &font, kind_color(DiffKind::Removed), true)
        }
        (DiffKind::Changed, old_side) => {
            let diff = inline_diff(left, right);
            let mid_len = diff
                .old_mid
                .chars()
                .count()
                .max(diff.new_mid.chars().count());
            let budget = chars_for_width(width);
            let prefix = if diff.prefix.chars().count() + mid_len > budget {
                tail_with_ellipsis(diff.prefix, budget.saturating_sub(mid_len + 6).max(8))
            } else {
                diff.prefix.to_string()
            };
            push_text(&mut job, &prefix, &font, c.text, false);
            if old_side {
                push_text(
                    &mut job,
                    diff.old_mid,
                    &font,
                    kind_color(DiffKind::Removed),
                    true,
                );
            } else {
                push_text(
                    &mut job,
                    diff.new_mid,
                    &font,
                    kind_color(DiffKind::Added),
                    false,
                );
            }
            push_text(&mut job, diff.suffix, &font, c.text, false);
        }
    }
    job
}

fn paint_marker(ui: &Ui, rect: Rect, kind: DiffKind) {
    if !kind.is_change() {
        return;
    }
    let painter = ui.painter();
    painter.rect_filled(rect, 5.0, cell_fill(kind));
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        kind_mark(kind),
        FontId::monospace(12.0),
        kind_color(kind),
    );
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

fn scene_number(group: &SceneChanges<'_>, scene_count: usize) -> String {
    if group.scene_index < scene_count {
        format!("{}", group.scene_index + 1)
    } else {
        "+".to_string()
    }
}

fn scene_heading(group: &SceneChanges<'_>) -> String {
    if group.heading.is_empty() {
        "(no heading)".to_string()
    } else {
        elide(group.heading, 42)
    }
}

fn scene_count_label(ui: &mut Ui, value: usize, kind: DiffKind) {
    if value == 0 {
        return;
    }
    let mark = kind_mark(kind);
    ui.label(
        RichText::new(format!("{mark}{value}"))
            .small()
            .strong()
            .color(kind_color(kind)),
    );
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
                .inner_margin(egui::Margin::symmetric(16, 10)),
        )
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 7.0);

            // Title row: identity, read-only guarantee, and the two workspace
            // actions that leave the comparison.
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Compare Drafts")
                        .strong()
                        .size(18.0)
                        .color(kit().heading_text),
                );
                egui::Frame::new()
                    .fill(kit().accent_soft)
                    .corner_radius(9)
                    .inner_margin(egui::Margin::symmetric(8, 2))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("Read-only")
                                .small()
                                .strong()
                                .color(kit().accent_text),
                        )
                        .on_hover_text(
                            "Comparing never edits your draft or revision history.",
                        );
                    });
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Close").clicked() {
                        action = CompareAction::Close;
                    }
                    if ui
                        .button("Restore copy…")
                        .on_hover_text(
                            "Save the base revision as a new .kitsp. The current draft is not changed.",
                        )
                        .clicked()
                    {
                        action = CompareAction::Restore(state.base);
                    }
                });
            });

            // Selection row: what is compared, how it is displayed, and how to
            // move through it.
            ui.horizontal(|ui| {
                ui.label(RichText::new("Base").small().color(kit().disabled_text));
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
                ui.label(RichText::new("to").small().color(kit().disabled_text));
                let mut target = state.target;
                egui::ComboBox::from_id_salt("compare_target")
                    .width(180.0)
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

                ui.add_space(4.0);
                egui::Frame::new()
                    .fill(kit().base)
                    .stroke(Stroke::new(1.0_f32, kit().field_stroke))
                    .corner_radius(7)
                    .inner_margin(egui::Margin::same(2))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        if ui
                            .selectable_label(state.view == CompareView::Unified, "Unified")
                            .clicked()
                        {
                            state.set_view(CompareView::Unified);
                        }
                        if ui
                            .selectable_label(
                                state.view == CompareView::SideBySide,
                                "Side by side",
                            )
                            .clicked()
                        {
                            state.set_view(CompareView::SideBySide);
                        }
                    });

                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    let can_jump = state.selected.is_some();
                    if ui
                        .add_enabled(can_jump, egui::Button::new("Open in editor"))
                        .clicked()
                    {
                        if let Some(block) = state.jump_block(project) {
                            action = CompareAction::Jump(block);
                        }
                    }
                    if ui.button("Next change").clicked() {
                        state.step_change(true);
                    }
                    if ui.button("Previous change").clicked() {
                        state.step_change(false);
                    }
                });
            });

            // Summary row: coloured counts double as the diff legend.
            if let Some(diff) = state.diff() {
                ui.horizontal(|ui| {
                    summary_chip(ui, &format!("{} added", diff.added), DiffKind::Added);
                    summary_chip(ui, &format!("{} removed", diff.removed), DiffKind::Removed);
                    summary_chip(ui, &format!("{} changed", diff.changed), DiffKind::Changed);
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!(
                            "{} scene(s) affected",
                            diff.scenes_changed.len()
                        ))
                        .small()
                        .color(kit().disabled_text),
                    );
                    if !diff.has_changes() {
                        ui.label(
                            RichText::new("The two drafts are identical.")
                                .small()
                                .color(kit().accent_text),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(KEYBOARD_HINT)
                                .small()
                                .color(kit().disabled_text),
                        );
                    });
                });
            }
        });

    egui::SidePanel::left("compare_changes")
        .resizable(true)
        .default_width(310.0)
        .width_range(230.0..=470.0)
        .frame(
            egui::Frame::new()
                .fill(kit().window)
                .inner_margin(egui::Margin::symmetric(12, 10)),
        )
        .show(ctx, |ui| {
            let selected = state.selected;
            let reveal = state.take_list_reveal();
            let mut filter = state.filter;
            let Some(diff) = state.diff() else {
                return;
            };
            let total_changes = diff
                .entries
                .iter()
                .filter(|entry| entry.kind.is_change())
                .count();
            ui.horizontal(|ui| {
                ui.label(RichText::new("Changes").strong().size(14.0));
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{total_changes} total"))
                            .small()
                            .color(kit().disabled_text),
                    );
                });
            });
            ui.add_space(6.0);

            // Filter chips keep their exact labels ("All 2", "Changed 2", …)
            // because the interaction tests select them by name.
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
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
                    let color = match option {
                        ChangeFilter::All => kit().accent_text,
                        ChangeFilter::Added => kind_color(DiffKind::Added),
                        ChangeFilter::Removed => kind_color(DiffKind::Removed),
                        ChangeFilter::Changed => kind_color(DiffKind::Changed),
                    };
                    ui.selectable_value(
                        &mut filter,
                        option,
                        RichText::new(label).small().color(color),
                    );
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
            ui.add_space(6.0);
            ui.separator();

            let mut clicked = None;
            let groups = scene_changes(diff, filter);
            ScrollArea::vertical()
                .id_salt("compare_change_list")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    for group in &groups {
                        let force_open =
                            selected.is_some_and(|selected| group.entries.contains(&selected));
                        let id = ui.make_persistent_id((
                            "compare_scene",
                            group.scene_index,
                            group.heading,
                        ));
                        let mut collapsing =
                            egui::collapsing_header::CollapsingState::load_with_default_open(
                                ui.ctx(),
                                id,
                                false,
                            );
                        if force_open {
                            collapsing.set_open(true);
                        }
                        egui::Frame::new()
                            .fill(kit().surface)
                            .stroke(Stroke::new(1.0_f32, kit().sep))
                            .corner_radius(8)
                            .inner_margin(egui::Margin::symmetric(8, 5))
                            .show(ui, |ui| {
                                let header = collapsing.show_header(ui, |ui| {
                                    scene_badge(ui, &scene_number(group, scene_count));
                                    ui.add_space(2.0);
                                    ui.label(RichText::new(scene_heading(group)).small().strong());
                                    ui.with_layout(
                                        egui::Layout::right_to_left(Align::Center),
                                        |ui| {
                                            scene_count_label(ui, group.changed, DiffKind::Changed);
                                            scene_count_label(ui, group.removed, DiffKind::Removed);
                                            scene_count_label(ui, group.added, DiffKind::Added);
                                        },
                                    );
                                });
                                header.body(|ui| {
                                    ui.spacing_mut().item_spacing.y = 2.0;
                                    for &index in &group.entries {
                                        let entry = &diff.entries[index];
                                        let response =
                                            change_list_row(ui, entry, selected == Some(index));
                                        if response.clicked() {
                                            clicked = Some(index);
                                        }
                                        if reveal == Some(index) {
                                            response.scroll_to_me(Some(Align::Center));
                                        }
                                    }
                                });
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
                RichText::new("Compare never edits your draft or revision history.")
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
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new("Nothing to compare")
                                .strong()
                                .size(16.0)
                                .color(kit().heading_text),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new("The selected drafts contain no script blocks.")
                                .small()
                                .color(kit().disabled_text),
                        );
                    });
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
                .inner_margin(egui::Margin::symmetric(14, 7))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if view == CompareView::SideBySide {
                            let width = ui.available_width();
                            let column = ((width - 52.0) / 2.0).max(80.0);
                            ui.add_space(24.0);
                            column_header(
                                ui,
                                column,
                                kit().disabled_text,
                                &format!("Base · {base_name}"),
                            );
                            ui.add_space(28.0);
                            column_header(
                                ui,
                                column,
                                kit().accent,
                                &format!("Target · {target_name}"),
                            );
                        } else {
                            header_dot(ui, kit().disabled_text);
                            ui.label(
                                RichText::new(format!("Base · {base_name}"))
                                    .small()
                                    .strong()
                                    .color(kit().text),
                            );
                            ui.label(RichText::new("to").small().color(kit().disabled_text));
                            header_dot(ui, kit().accent);
                            ui.label(
                                RichText::new(format!("Target · {target_name}"))
                                    .small()
                                    .strong()
                                    .color(kit().text),
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
                    let response = diff_row(ui, entry, view, selected == Some(index));
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

/// Fixed-width text cell whose text starts at the left edge. `add_sized`
/// centres its child, which is wrong for script lines.
fn add_text_cell(ui: &mut Ui, width: f32, height: f32, job: LayoutJob) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let mut cell = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(Align::Center).with_main_align(Align::Min)),
    );
    cell.add(Label::new(job).halign(Align::LEFT).truncate());
}

fn summary_chip(ui: &mut Ui, text: &str, kind: DiffKind) {
    egui::Frame::new()
        .fill(kind_fill(kind))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(7, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).small().strong().color(kind_color(kind)));
        });
}

fn header_dot(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(8.0, 18.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.5, color);
}

fn column_header(ui: &mut Ui, width: f32, dot: Color32, text: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 18.0), Sense::hover());
    let mut cell = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(Align::Center).with_main_align(Align::Min)),
    );
    cell.spacing_mut().item_spacing.x = 5.0;
    header_dot(&mut cell, dot);
    cell.add(
        Label::new(RichText::new(text).small().strong().color(kit().text))
            .halign(Align::LEFT)
            .truncate(),
    );
}

fn scene_badge(ui: &mut Ui, label: &str) {
    egui::Frame::new()
        .fill(kit().elt_bg)
        .corner_radius(5)
        .inner_margin(egui::Margin::symmetric(5, 1))
        .show(ui, |ui| {
            ui.label(
                RichText::new(label)
                    .small()
                    .strong()
                    .color(kit().disabled_text),
            );
        });
}

fn change_list_row(ui: &mut Ui, entry: &DiffEntry, selected: bool) -> egui::Response {
    let width = ui.available_width().max(80.0);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, LIST_ROW_HEIGHT), Sense::click());
    let background = if selected {
        kit().accent_soft
    } else if response.hovered() {
        kit().hover_light
    } else {
        kind_fill(entry.kind)
    };
    if background != Color32::TRANSPARENT {
        ui.painter()
            .rect_filled(rect.shrink2(Vec2::new(1.0, 0.0)), 4.0, background);
    }
    let inner = rect.shrink2(Vec2::new(6.0, 0.0));
    let mut row = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(Align::Center)),
    );
    row.spacing_mut().item_spacing.x = 5.0;
    let color = if entry.kind == DiffKind::Unchanged {
        kit().disabled_text
    } else {
        kind_color(entry.kind)
    };
    row.label(
        RichText::new(kind_mark(entry.kind))
            .small()
            .strong()
            .color(color),
    );
    let text = if entry.kind == DiffKind::Removed {
        entry_base_text(entry)
    } else {
        entry_current_text(entry)
    };
    let text_width = row.available_width().max(20.0);
    let mut job = text_job(text_width);
    push_text(
        &mut job,
        &elide(text, 74),
        &FontId::monospace(11.0),
        color,
        false,
    );
    add_text_cell(&mut row, text_width, LIST_ROW_HEIGHT, job);
    response.on_hover_text(format!(
        "{}\n\nBase: {}\nTarget: {}",
        entry.scene_heading,
        if entry_base_text(entry).is_empty() {
            "—"
        } else {
            entry_base_text(entry)
        },
        if entry_current_text(entry).is_empty() {
            "—"
        } else {
            entry_current_text(entry)
        }
    ))
}

fn diff_row(ui: &mut Ui, entry: &DiffEntry, view: CompareView, selected: bool) -> egui::Response {
    let width = ui.available_width().max(160.0);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, ROW_HEIGHT), Sense::click());
    let row_rect = rect.shrink2(Vec2::new(4.0, 1.0));
    let background = if selected {
        kit().accent_soft
    } else if response.hovered() {
        kit().hover_light
    } else {
        kind_fill(entry.kind)
    };
    {
        let painter = ui.painter();
        if background != Color32::TRANSPARENT {
            painter.rect_filled(row_rect, 6.0, background);
        }
        if selected {
            let bar = Rect::from_min_max(
                Pos2::new(row_rect.left() + 1.0, row_rect.top() + 4.0),
                Pos2::new(row_rect.left() + 4.0, row_rect.bottom() - 4.0),
            );
            painter.rect_filled(bar, 1.5, kit().accent);
        }
    }

    let inner = rect.shrink2(Vec2::new(12.0, 0.0));
    let mut row = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(egui::Layout::left_to_right(Align::Center)),
    );
    row.spacing_mut().item_spacing.x = 8.0;
    let (marker_slot, _) = row.allocate_exact_size(Vec2::new(18.0, ROW_HEIGHT), Sense::hover());
    paint_marker(&row, marker_slot.shrink2(Vec2::new(0.0, 4.0)), entry.kind);

    match view {
        CompareView::Unified => {
            let text_width = row.available_width().max(40.0);
            add_text_cell(
                &mut row,
                text_width,
                ROW_HEIGHT,
                unified_job(entry, text_width),
            );
        }
        CompareView::SideBySide => {
            let available = row.available_width().max(80.0);
            let column = ((available - 28.0) / 2.0).max(40.0);
            let start = row.cursor().left();
            let top = rect.top() + 3.0;
            let bottom = rect.bottom() - 3.0;
            {
                let painter = row.painter();
                if matches!(entry.kind, DiffKind::Removed | DiffKind::Changed) {
                    let cell = Rect::from_min_max(
                        Pos2::new(start, top),
                        Pos2::new(start + column, bottom),
                    );
                    painter.rect_filled(cell, 5.0, cell_fill(DiffKind::Removed));
                }
                if matches!(entry.kind, DiffKind::Added | DiffKind::Changed) {
                    let cell = Rect::from_min_max(
                        Pos2::new(start + column + 28.0, top),
                        Pos2::new(start + column * 2.0 + 28.0, bottom),
                    );
                    painter.rect_filled(cell, 5.0, cell_fill(DiffKind::Added));
                }
                let divider = start + column + 14.0;
                painter.line_segment(
                    [
                        Pos2::new(divider, rect.top() + 5.0),
                        Pos2::new(divider, rect.bottom() - 5.0),
                    ],
                    Stroke::new(1.0_f32, kit().field_stroke),
                );
            }
            add_text_cell(&mut row, column, ROW_HEIGHT, side_job(entry, true, column));
            row.add_space(12.0);
            add_text_cell(&mut row, column, ROW_HEIGHT, side_job(entry, false, column));
        }
    }

    response.on_hover_text(format!(
        "{}\n\nBase: {}\nTarget: {}",
        entry.scene_heading,
        if entry_base_text(entry).is_empty() {
            "—"
        } else {
            entry_base_text(entry)
        },
        if entry_current_text(entry).is_empty() {
            "—"
        } else {
            entry_current_text(entry)
        }
    ))
}

#[cfg(test)]
mod tests {
    use super::inline_diff;

    #[test]
    fn inline_diff_isolates_replaced_word() {
        let diff = inline_diff("INT. KITCHEN - DAY", "INT. KITCHEN - NIGHT");
        assert_eq!(diff.prefix, "INT. KITCHEN - ");
        assert_eq!(diff.old_mid, "DAY");
        assert_eq!(diff.new_mid, "NIGHT");
        assert_eq!(diff.suffix, "");
    }

    #[test]
    fn inline_diff_keeps_common_suffix() {
        let diff = inline_diff("INT. A - DAY", "INT. B - DAY");
        assert_eq!(diff.prefix, "INT. ");
        assert_eq!(diff.old_mid, "A");
        assert_eq!(diff.new_mid, "B");
        assert_eq!(diff.suffix, " - DAY");
    }

    #[test]
    fn inline_diff_handles_multibyte_text() {
        let diff = inline_diff("ИНТ. КУХНЯ – ДЕН", "ИНТ. КУХНЯ – НОЩ");
        assert_eq!(diff.prefix, "ИНТ. КУХНЯ – ");
        assert_eq!(diff.old_mid, "ДЕН");
        assert_eq!(diff.new_mid, "НОЩ");
        assert_eq!(diff.suffix, "");
    }

    #[test]
    fn inline_diff_handles_pure_insertion() {
        let diff = inline_diff("abc", "abcde");
        assert_eq!(diff.prefix, "abc");
        assert_eq!(diff.old_mid, "");
        assert_eq!(diff.new_mid, "de");
        assert_eq!(diff.suffix, "");
    }

    #[test]
    fn inline_diff_handles_pure_deletion() {
        let diff = inline_diff("abcde", "abc");
        assert_eq!(diff.prefix, "abc");
        assert_eq!(diff.old_mid, "de");
        assert_eq!(diff.new_mid, "");
        assert_eq!(diff.suffix, "");
    }

    #[test]
    fn inline_diff_identical_has_no_middle() {
        let diff = inline_diff("INT. KITCHEN - DAY", "INT. KITCHEN - DAY");
        assert_eq!(diff.prefix, "INT. KITCHEN - DAY");
        assert_eq!(diff.old_mid, "");
        assert_eq!(diff.new_mid, "");
        assert_eq!(diff.suffix, "");
    }
}
