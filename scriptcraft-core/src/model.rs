use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Screenplay block types: KIT Scenarist set (Russian template order).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ElementType {
    SceneHeading,
    SceneCharacters,
    Action,
    Character,
    Dialogue,
    Parenthetical,
    Shot,
    Transition,
    NonPrintable,
    Lyrics,
}

impl ElementType {
    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "scene_heading" => Some(Self::SceneHeading),
            "scene_characters" => Some(Self::SceneCharacters),
            "action" => Some(Self::Action),
            "character" => Some(Self::Character),
            "dialog" => Some(Self::Dialogue),
            "parenthetical" => Some(Self::Parenthetical),
            "shot" => Some(Self::Shot),
            "transition" => Some(Self::Transition),
            "note" => Some(Self::NonPrintable),
            "lyrics" => Some(Self::Lyrics),
            _ => None,
        }
    }

    pub fn tag(&self) -> &'static str {
        match self {
            Self::SceneHeading => "scene_heading",
            Self::SceneCharacters => "scene_characters",
            Self::Action => "action",
            Self::Character => "character",
            Self::Dialogue => "dialog",
            Self::Parenthetical => "parenthetical",
            Self::Shot => "shot",
            Self::Transition => "transition",
            Self::NonPrintable => "note",
            Self::Lyrics => "lyrics",
        }
    }

    /// Non-printable blocks are excluded from stats/export counts.
    pub fn is_printable(&self) -> bool {
        !matches!(self, Self::NonPrintable)
    }
}

/// Which half of a dual-dialogue pair a character block owns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DualSide {
    Left,
    Right,
}

/// ScriptCraft-only pairing marker stored on the two character blocks.
/// Persisted in the `scriptcraft_dual_groups` sidecar row; KIT never sees it
/// and continues to edit the underlying blocks as ordinary text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DualMark {
    pub group: Uuid,
    pub side: DualSide,
}

/// Inline text style applied to a character range inside a [`ScriptBlock`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum InlineStyle {
    Bold,
    Italic,
    Underline,
}

/// One inline formatting span over `[start, end)` in Unicode char offsets of
/// `ScriptBlock::text`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleSpan {
    pub start: usize,
    pub end: usize,
    pub style: InlineStyle,
}

/// One screenplay block.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScriptBlock {
    pub id: Uuid,
    /// Only scene headings carry the KIT uuid; others get a fresh v4.
    pub kit_uuid: Option<String>,
    pub element_type: ElementType,
    pub text: String,
    /// Dual-dialogue pairing marker (character blocks only).
    #[serde(default)]
    pub dual: Option<DualMark>,
    /// Inline bold / italic / underline spans (char offsets into `text`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub styles: Vec<StyleSpan>,
}

impl ScriptBlock {
    pub fn new(element_type: ElementType, text: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            kit_uuid: None,
            element_type,
            text,
            dual: None,
            styles: Vec::new(),
        }
    }

    /// Normalize inline style spans: clamp to text length, drop empties, sort,
    /// and coalesce overlapping/adjacent spans of the same style.
    pub fn normalize_styles(&mut self) {
        let len = self.text.chars().count();
        self.styles = normalize_style_spans(std::mem::take(&mut self.styles), len);
    }

    /// True when every character in `[start, end)` carries `style`.
    pub fn has_style_over(&self, start: usize, end: usize, style: InlineStyle) -> bool {
        if start >= end {
            return false;
        }
        let mut covered = start;
        let mut spans: Vec<(usize, usize)> = self
            .styles
            .iter()
            .filter(|s| s.style == style && s.end > start && s.start < end)
            .map(|s| (s.start.max(start), s.end.min(end)))
            .collect();
        spans.sort_unstable();
        for (a, b) in spans {
            if a > covered {
                return false;
            }
            covered = covered.max(b);
            if covered >= end {
                return true;
            }
        }
        covered >= end
    }

    /// Toggle `style` over `[start, end)`, returning the normalized span list.
    /// Removes `style` from `[start, end)` when the entire range already has
    /// it; otherwise adds `style` across `[start, end)`.
    pub fn toggle_style_range(
        &self,
        start: usize,
        end: usize,
        style: InlineStyle,
    ) -> Vec<StyleSpan> {
        let len = self.text.chars().count();
        let a = start.min(len);
        let b = end.min(len);
        if a >= b {
            return self.styles.clone();
        }
        let mut out = Vec::with_capacity(self.styles.len() + 2);
        if self.has_style_over(a, b, style) {
            for s in &self.styles {
                if s.style != style || s.end <= a || s.start >= b {
                    out.push(s.clone());
                } else {
                    if s.start < a {
                        out.push(StyleSpan {
                            start: s.start,
                            end: a,
                            style,
                        });
                    }
                    if s.end > b {
                        out.push(StyleSpan {
                            start: b,
                            end: s.end,
                            style,
                        });
                    }
                }
            }
        } else {
            out.extend(self.styles.iter().cloned());
            out.push(StyleSpan {
                start: a,
                end: b,
                style,
            });
        }
        normalize_style_spans(out, len)
    }

    /// Active `(bold, italic, underline)` flags at char offset `ch`.
    pub fn styles_at(&self, ch: usize) -> (bool, bool, bool) {
        let mut bold = false;
        let mut italic = false;
        let mut underline = false;
        for s in &self.styles {
            if s.start <= ch && ch < s.end {
                match s.style {
                    InlineStyle::Bold => bold = true,
                    InlineStyle::Italic => italic = true,
                    InlineStyle::Underline => underline = true,
                }
            }
        }
        (bold, italic, underline)
    }

    /// Partition `[start, end)` into contiguous uniform-style runs
    /// `(a, b, bold, italic, underline)` covering `[start, end)`.
    pub fn style_runs_in(&self, start: usize, end: usize) -> Vec<(usize, usize, bool, bool, bool)> {
        if start >= end {
            return Vec::new();
        }
        let mut cuts = vec![start, end];
        for s in &self.styles {
            if s.end > start && s.start < end {
                cuts.push(s.start.max(start));
                cuts.push(s.end.min(end));
            }
        }
        cuts.sort_unstable();
        cuts.dedup();
        let mut runs = Vec::with_capacity(cuts.len().saturating_sub(1));
        for w in cuts.windows(2) {
            let (a, b) = (w[0], w[1]);
            if a < b {
                let (bold, italic, underline) = self.styles_at(a);
                runs.push((a, b, bold, italic, underline));
            }
        }
        runs
    }

    /// Shift spans after inserting `len` chars at char offset `at`.
    pub fn shift_styles_insert(&mut self, at: usize, len: usize) {
        if self.styles.is_empty() || len == 0 {
            return;
        }
        for s in &mut self.styles {
            if s.start >= at {
                s.start += len;
                s.end += len;
            } else if s.end >= at {
                // Typing inside or immediately at the end of a styled span extends it.
                s.end += len;
            }
        }
        self.normalize_styles();
    }

    /// Shift/clip spans after deleting `[start, end)`.
    pub fn shift_styles_delete(&mut self, start: usize, end: usize) {
        if self.styles.is_empty() || start >= end {
            return;
        }
        let removed = end - start;
        let mut out = Vec::with_capacity(self.styles.len());
        for s in &self.styles {
            if s.end <= start {
                out.push(s.clone());
            } else if s.start >= end {
                out.push(StyleSpan {
                    start: s.start - removed,
                    end: s.end - removed,
                    style: s.style,
                });
            } else {
                let ns = s.start.min(start);
                let ne = if s.end <= end { start } else { s.end - removed };
                if ns < ne {
                    out.push(StyleSpan {
                        start: ns,
                        end: ne,
                        style: s.style,
                    });
                }
            }
        }
        self.styles = out;
        self.normalize_styles();
    }

    /// Split spans at `at`: head keeps `[0, at)`, tail gets `[at + skip, ..)`
    /// shifted down to start at 0.
    pub fn split_styles(&self, at: usize, skip: usize) -> (Vec<StyleSpan>, Vec<StyleSpan>) {
        if self.styles.is_empty() {
            return (Vec::new(), Vec::new());
        }
        let tail_start = at + skip;
        let mut head = Vec::new();
        let mut tail = Vec::new();
        for s in &self.styles {
            if s.start < at {
                head.push(StyleSpan {
                    start: s.start,
                    end: s.end.min(at),
                    style: s.style,
                });
            }
            if s.end > tail_start {
                tail.push(StyleSpan {
                    start: s.start.saturating_sub(tail_start),
                    end: s.end - tail_start,
                    style: s.style,
                });
            }
        }
        (head, tail)
    }

    /// Append `other` spans shifted by `offset` chars and normalize.
    pub fn merge_styles(&mut self, offset: usize, other: &[StyleSpan]) {
        if other.is_empty() {
            return;
        }
        for s in other {
            self.styles.push(StyleSpan {
                start: s.start + offset,
                end: s.end + offset,
                style: s.style,
            });
        }
        self.normalize_styles();
    }
}

/// Normalize style spans against a text length of `max_len` chars.
pub fn normalize_style_spans(spans: Vec<StyleSpan>, max_len: usize) -> Vec<StyleSpan> {
    let mut cleaned: Vec<StyleSpan> = spans
        .into_iter()
        .filter_map(|mut s| {
            s.start = s.start.min(max_len);
            s.end = s.end.min(max_len);
            (s.start < s.end).then_some(s)
        })
        .collect();
    cleaned.sort_by_key(|s| (s.style, s.start, s.end));
    let mut merged: Vec<StyleSpan> = Vec::with_capacity(cleaned.len());
    for s in cleaned {
        if let Some(last) = merged.last_mut() {
            if last.style == s.style && s.start <= last.end {
                last.end = last.end.max(s.end);
                continue;
            }
        }
        merged.push(s);
    }
    merged
}

/// A validated dual pair: two adjacent speech units. Each unit is a
/// `Character` block plus any following `Parenthetical`/`Dialogue`/`Lyrics`
/// blocks; `left_end`/`right_end` are exclusive block indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DualPair {
    pub left: usize,
    pub right: usize,
    pub left_end: usize,
    pub right_end: usize,
}

/// Exclusive end of the speech unit starting at `start`, or `None` when
/// `start` is not a character cue. A unit is the cue plus following
/// parenthetical/dialogue/lyrics blocks.
pub fn speech_unit_end(blocks: &[ScriptBlock], start: usize) -> Option<usize> {
    if blocks.get(start)?.element_type != ElementType::Character {
        return None;
    }
    let mut end = start + 1;
    while end < blocks.len()
        && matches!(
            blocks[end].element_type,
            ElementType::Parenthetical | ElementType::Dialogue | ElementType::Lyrics
        )
    {
        end += 1;
    }
    Some(end)
}

/// Start of the speech unit containing `bi`: the cue itself, or the nearest
/// cue above a parenthetical/dialogue/lyrics block.
pub fn speech_unit_start(blocks: &[ScriptBlock], bi: usize) -> Option<usize> {
    let block = blocks.get(bi)?;
    match block.element_type {
        ElementType::Character => Some(bi),
        ElementType::Parenthetical | ElementType::Dialogue | ElementType::Lyrics => {
            let mut i = bi;
            while i > 0 {
                i -= 1;
                match blocks.get(i)?.element_type {
                    ElementType::Character => return Some(i),
                    ElementType::Parenthetical | ElementType::Dialogue | ElementType::Lyrics => {}
                    _ => return None,
                }
            }
            None
        }
        _ => None,
    }
}

/// The speech unit immediately after `start`'s unit, if it starts with a cue.
pub fn next_speech_unit(blocks: &[ScriptBlock], start: usize) -> Option<usize> {
    let end = speech_unit_end(blocks, start)?;
    (blocks.get(end)?.element_type == ElementType::Character).then_some(end)
}

/// All valid dual pairs derived from block markers. A pair is valid only when
/// both sides are character cues of the same group, the left unit ends exactly
/// at the right cue, and each unit has at least one dialogue line. Orphaned,
/// duplicated or reordered markers are ignored rather than guessed at.
pub fn dual_pairs(blocks: &[ScriptBlock]) -> Vec<DualPair> {
    use std::collections::HashMap;
    let mut lefts: HashMap<Uuid, Vec<usize>> = HashMap::new();
    let mut rights: HashMap<Uuid, Vec<usize>> = HashMap::new();
    for (bi, b) in blocks.iter().enumerate() {
        let Some(mark) = b.dual else { continue };
        if b.element_type != ElementType::Character {
            continue;
        }
        match mark.side {
            DualSide::Left => lefts.entry(mark.group).or_default().push(bi),
            DualSide::Right => rights.entry(mark.group).or_default().push(bi),
        }
    }
    let mut out = Vec::new();
    for (group, left) in lefts {
        let Some(right) = rights.get(&group) else {
            continue;
        };
        if left.len() != 1 || right.len() != 1 {
            continue;
        }
        let (left, right) = (left[0], right[0]);
        let Some(left_end) = speech_unit_end(blocks, left) else {
            continue;
        };
        let Some(right_end) = speech_unit_end(blocks, right) else {
            continue;
        };
        if left < right && left_end == right && left_end > left + 1 && right_end > right + 1 {
            out.push(DualPair {
                left,
                right,
                left_end,
                right_end,
            });
        }
    }
    out.sort_by_key(|pair| pair.left);
    out
}

/// A scene = heading + following blocks until the next heading.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scene {
    /// Stable identity: the `ScriptBlock::id` of the scene heading.
    pub id: Uuid,
    pub heading: String,
    pub kit_uuid: Option<String>,
    /// Indices into Project::blocks (includes the heading at [0]).
    pub blocks: Vec<usize>,
}

/// ScriptCraft-only per-scene metadata, keyed by the heading block's stable id.
/// Stored in the `.kitsp` `scenario_data.scriptcraft_scene_meta` row; KIT
/// ignores it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneMeta {
    pub id: Uuid,
    #[serde(default)]
    pub omitted: bool,
}

/// Whole screenplay project.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Project {
    pub title: String,
    pub blocks: Vec<ScriptBlock>,
    /// Scene metadata keyed by `Scene::id`. Empty for imported projects until
    /// the user changes a scene-level property.
    #[serde(default)]
    pub scene_meta: Vec<SceneMeta>,
    /// Shot list & coverage ranges anchored to block UUIDs (persisted in
    /// `.kitsp`'s `scriptcraft_shots` sidecar row).
    #[serde(default, skip_serializing_if = "crate::shots::ShotList::is_empty")]
    pub shots: crate::shots::ShotList,
}

impl Project {
    pub fn scenes(&self) -> Vec<Scene> {
        let mut scenes = Vec::new();
        let mut current: Option<Scene> = None;
        for (i, b) in self.blocks.iter().enumerate() {
            if b.element_type == ElementType::SceneHeading {
                if let Some(s) = current.take() {
                    scenes.push(s);
                }
                current = Some(Scene {
                    id: b.id,
                    heading: b.text.clone(),
                    kit_uuid: b.kit_uuid.clone(),
                    blocks: vec![i],
                });
            } else if let Some(s) = current.as_mut() {
                s.blocks.push(i);
            }
        }
        if let Some(s) = current.take() {
            scenes.push(s);
        }
        scenes
    }

    /// Drop metadata for scenes that no longer exist and de-duplicate by id.
    /// Call before saving or after a structural edit.
    pub fn retain_scene_meta(&mut self) {
        let ids: std::collections::HashSet<Uuid> = self
            .blocks
            .iter()
            .filter(|b| b.element_type == ElementType::SceneHeading)
            .map(|b| b.id)
            .collect();
        let mut seen = std::collections::HashSet::new();
        self.scene_meta
            .retain(|m| ids.contains(&m.id) && seen.insert(m.id));
    }

    pub fn scene_meta(&self, scene_id: Uuid) -> Option<&SceneMeta> {
        self.scene_meta.iter().find(|m| m.id == scene_id)
    }

    pub fn set_scene_omitted(&mut self, scene_id: Uuid, omitted: bool) {
        if let Some(meta) = self.scene_meta.iter_mut().find(|m| m.id == scene_id) {
            meta.omitted = omitted;
        } else if omitted {
            self.scene_meta.push(SceneMeta {
                id: scene_id,
                omitted,
            });
        }
    }

    pub fn scene_is_omitted(&self, scene: &Scene) -> bool {
        self.scene_meta(scene.id).is_some_and(|meta| meta.omitted)
    }

    /// Valid dual-dialogue pairs in this project.
    pub fn dual_pairs(&self) -> Vec<DualPair> {
        dual_pairs(&self.blocks)
    }

    /// The validated pair containing block `bi`, if any.
    pub fn dual_pair_for_block(&self, bi: usize) -> Option<DualPair> {
        self.dual_pairs()
            .into_iter()
            .find(|pair| pair.left <= bi && bi < pair.right_end)
    }

    pub fn omitted_scene_count(&self) -> usize {
        self.scenes()
            .iter()
            .filter(|scene| self.scene_is_omitted(scene))
            .count()
    }

    pub fn has_omitted_scenes(&self) -> bool {
        self.scene_meta.iter().any(|meta| meta.omitted) && self.omitted_scene_count() > 0
    }

    /// Per-block flags: `true` for body blocks hidden by an omitted scene.
    /// Headings are never hidden.
    pub fn hidden_blocks(&self) -> Vec<bool> {
        let mut hidden = vec![false; self.blocks.len()];
        for scene in self.scenes() {
            if !self.scene_is_omitted(&scene) {
                continue;
            }
            for &bi in scene.blocks.iter().skip(1) {
                if bi < hidden.len() {
                    hidden[bi] = true;
                }
            }
        }
        hidden
    }

    /// Per-block flags: `true` for headings whose scene is omitted.
    pub fn omitted_heading_blocks(&self) -> Vec<bool> {
        let mut headings = vec![false; self.blocks.len()];
        for scene in self.scenes() {
            if self.scene_is_omitted(&scene) {
                if let Some(&bi) = scene.blocks.first() {
                    if bi < headings.len() {
                        headings[bi] = true;
                    }
                }
            }
        }
        headings
    }

    /// The document as it appears on the page: omitted scenes keep their
    /// heading, gain a synthetic `OMITTED` line and lose their body blocks.
    /// `sources` maps every view block back to its original block index;
    /// `original_to_view` maps every original block (including hidden body
    /// blocks, which point at their scene's `OMITTED` line).
    pub fn document_view(&self) -> DocumentView {
        let hidden = self.hidden_blocks();
        let omitted_heading = self.omitted_heading_blocks();
        let marker_for_heading: Vec<Option<usize>> = omitted_heading
            .iter()
            .enumerate()
            .map(|(bi, omitted)| (*omitted).then_some(bi))
            .collect();
        let mut heading_of_hidden = vec![None; self.blocks.len()];
        for scene in self.scenes() {
            if !self.scene_is_omitted(&scene) {
                continue;
            }
            let Some(&heading) = scene.blocks.first() else {
                continue;
            };
            for &bi in scene.blocks.iter().skip(1) {
                if bi < heading_of_hidden.len() {
                    heading_of_hidden[bi] = Some(heading);
                }
            }
        }

        let mut view = DocumentView {
            blocks: Vec::with_capacity(self.blocks.len()),
            sources: Vec::with_capacity(self.blocks.len()),
            omitted: Vec::with_capacity(self.blocks.len()),
            original_to_view: vec![None; self.blocks.len()],
            revision_sources: Vec::with_capacity(self.blocks.len()),
        };
        let mut marker_at = vec![None; self.blocks.len()];
        for (bi, block) in self.blocks.iter().enumerate() {
            if hidden[bi] {
                continue;
            }
            let vi = view.blocks.len();
            view.blocks.push(block.clone());
            view.sources.push(Some(bi));
            view.omitted.push(false);
            view.revision_sources.push(Some(bi));
            view.original_to_view[bi] = Some(vi);
            if marker_for_heading[bi].is_some() {
                let mi = view.blocks.len();
                view.blocks.push(ScriptBlock {
                    id: block.id,
                    kit_uuid: None,
                    element_type: ElementType::Action,
                    text: "OMITTED".to_string(),
                    dual: None,
                    styles: Vec::new(),
                });
                view.sources.push(None);
                view.omitted.push(true);
                view.revision_sources.push(Some(bi));
                marker_at[bi] = Some(mi);
            }
        }
        for bi in 0..self.blocks.len() {
            if !hidden[bi] {
                continue;
            }
            if let Some(heading) = heading_of_hidden[bi] {
                view.original_to_view[bi] = marker_at[heading];
            }
        }
        view
    }

    pub fn characters(&self) -> Vec<String> {
        let hidden = self.hidden_blocks();
        let mut out = Vec::new();
        for (bi, b) in self.blocks.iter().enumerate() {
            if hidden.get(bi).copied().unwrap_or(false) {
                continue;
            }
            if b.element_type == ElementType::Character && !out.contains(&b.text) {
                out.push(b.text.clone());
            }
        }
        out.sort();
        out
    }

    pub fn stats(&self) -> Stats {
        Stats::from_project(self)
    }

    pub fn word_count(&self) -> usize {
        let hidden = self.hidden_blocks();
        self.blocks
            .iter()
            .enumerate()
            .filter(|(bi, b)| {
                !hidden.get(*bi).copied().unwrap_or(false) && b.element_type.is_printable()
            })
            .map(|(_, b)| b.text.split_whitespace().count())
            .sum()
    }

    /// Estimated screen time, calibrated against KIT Scenarist chronometry
    /// on A95 (total 17:55 predicted vs 17:54 shown; mean scene error 5.7s):
    /// action-ish words cost ~0.22s, dialogue-ish words ~0.27s.
    pub fn duration_secs(&self) -> f64 {
        let hidden = self.hidden_blocks();
        duration_range_hidden(&self.blocks, self.blocks.len(), &hidden)
    }

    pub fn scene_duration_secs(&self, scene: Option<&Scene>) -> f64 {
        let hidden = self.hidden_blocks();
        match scene {
            Some(s) => {
                let idx: Vec<usize> = s.blocks.clone();
                duration_indexed(&self.blocks, &idx, &hidden)
            }
            None => duration_range_hidden(&self.blocks, self.blocks.len(), &hidden),
        }
    }

    /// Estimated screen time of blocks before (and including) `upto_bi`,
    /// for the toolbar's "current position" counter.
    pub fn duration_upto(&self, upto_bi: usize) -> f64 {
        let hidden = self.hidden_blocks();
        duration_range_hidden(
            &self.blocks,
            upto_bi.saturating_add(1).min(self.blocks.len()),
            &hidden,
        )
    }
}

/// A page-view of a project with omitted scenes collapsed. View block indices
/// are what the layout engine paginates; `sources` maps them back to
/// `Project::blocks` so editor spans and statistics never lose identity.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocumentView {
    pub blocks: Vec<ScriptBlock>,
    /// Original block index for real blocks, `None` for synthetic lines.
    pub sources: Vec<Option<usize>>,
    /// `true` for the synthetic `OMITTED` line.
    pub omitted: Vec<bool>,
    /// Original block index → view index. Hidden body blocks point at the
    /// `OMITTED` line of their scene.
    pub original_to_view: Vec<Option<usize>>,
    /// Original block whose revision colour paints this view line.
    pub revision_sources: Vec<Option<usize>>,
}

impl DocumentView {
    pub fn is_identity(&self) -> bool {
        self.blocks.len() == self.sources.len()
            && self
                .sources
                .iter()
                .enumerate()
                .all(|(i, source)| *source == Some(i))
            && self.omitted.iter().all(|omitted| !omitted)
    }
}

/// Calibrated KIT chronometry rates (seconds per word).
pub const ACTION_S_PER_WORD: f64 = 0.22;
pub const DIALOG_S_PER_WORD: f64 = 0.27;

fn is_dialog_kind(t: ElementType) -> bool {
    matches!(
        t,
        ElementType::Dialogue | ElementType::Parenthetical | ElementType::Lyrics
    )
}

fn duration_range_hidden(blocks: &[ScriptBlock], end_exclusive: usize, hidden: &[bool]) -> f64 {
    let end = end_exclusive.min(blocks.len());
    let mut a = 0usize;
    let mut d = 0usize;
    for (bi, b) in blocks[..end].iter().enumerate() {
        if hidden.get(bi).copied().unwrap_or(false) {
            continue;
        }
        if !b.element_type.is_printable() {
            continue;
        }
        let w = b.text.split_whitespace().count();
        if is_dialog_kind(b.element_type) {
            d += w;
        } else {
            a += w;
        }
    }
    a as f64 * ACTION_S_PER_WORD + d as f64 * DIALOG_S_PER_WORD
}

fn duration_indexed(blocks: &[ScriptBlock], idx: &[usize], hidden: &[bool]) -> f64 {
    let mut a = 0usize;
    let mut d = 0usize;
    for &i in idx {
        if hidden.get(i).copied().unwrap_or(false) {
            continue;
        }
        let Some(b) = blocks.get(i) else { continue };
        if !b.element_type.is_printable() {
            continue;
        }
        let w = b.text.split_whitespace().count();
        if is_dialog_kind(b.element_type) {
            d += w;
        } else {
            a += w;
        }
    }
    a as f64 * ACTION_S_PER_WORD + d as f64 * DIALOG_S_PER_WORD
}

/// Cheap stats for the Statistics panel (M3 will chart these).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Stats {
    pub scenes: usize,
    pub blocks: usize,
    pub actions: usize,
    pub dialogues: usize,
    pub characters: usize,
    pub words: usize,
    pub est_pages: f32,
}

impl Stats {
    pub fn from_project(p: &Project) -> Self {
        let hidden = p.hidden_blocks();
        let mut s = Self {
            blocks: hidden.iter().filter(|hidden| !**hidden).count(),
            words: p.word_count(),
            ..Default::default()
        };
        for (bi, b) in p.blocks.iter().enumerate() {
            if hidden.get(bi).copied().unwrap_or(false) {
                continue;
            }
            match b.element_type {
                ElementType::SceneHeading => s.scenes += 1,
                ElementType::Action | ElementType::Shot | ElementType::SceneCharacters => {
                    s.actions += 1
                }
                ElementType::Dialogue => s.dialogues += 1,
                _ => {}
            }
        }
        s.characters = p.characters().len();
        s.est_pages = s.words as f32 / 180.0;
        s
    }
}
