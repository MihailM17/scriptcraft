//! Shot list & script coverage model (Phase 1 of Shot Breakdown).
//!
//! Shots anchor to stable [`ScriptBlock::id`] UUIDs rather than positional
//! block indices, so edits, scene reordering, and production revisions keep
//! shot coverage aligned. Multiple shots may overlap the same blocks (real
//! filmmaking coverage: Wide Master + OTS + Close-Up).
//!
//! Persisted inside `.kitsp` as the `scriptcraft_shots` JSON sidecar row in
//! `scenario_data`, so KIT Scenarist ignores it cleanly.

use crate::model::{ElementType, Scene, ScriptBlock, ACTION_S_PER_WORD, DIALOG_S_PER_WORD};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub const SHOTS_SCHEMA: u32 = 1;

fn default_shots_schema() -> u32 {
    SHOTS_SCHEMA
}

/// Standard film shot sizes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShotSize {
    Ews,
    #[default]
    Ws,
    Ms,
    Mcu,
    Cu,
    Ecu,
    Ots,
    Pov,
    Insert,
    TwoShot,
}

impl ShotSize {
    pub const ALL: [Self; 10] = [
        Self::Ews,
        Self::Ws,
        Self::Ms,
        Self::Mcu,
        Self::Cu,
        Self::Ecu,
        Self::Ots,
        Self::Pov,
        Self::Insert,
        Self::TwoShot,
    ];

    pub fn short(self) -> &'static str {
        match self {
            Self::Ews => "EWS",
            Self::Ws => "WS",
            Self::Ms => "MS",
            Self::Mcu => "MCU",
            Self::Cu => "CU",
            Self::Ecu => "ECU",
            Self::Ots => "OTS",
            Self::Pov => "POV",
            Self::Insert => "INS",
            Self::TwoShot => "2-SHOT",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Ews => "EWS — Extreme Wide",
            Self::Ws => "WS — Wide Shot",
            Self::Ms => "MS — Medium Shot",
            Self::Mcu => "MCU — Medium Close-Up",
            Self::Cu => "CU — Close-Up",
            Self::Ecu => "ECU — Extreme Close-Up",
            Self::Ots => "OTS — Over the Shoulder",
            Self::Pov => "POV — Point of View",
            Self::Insert => "INS — Insert",
            Self::TwoShot => "2-SHOT — Two Shot",
        }
    }
}

/// Standard camera movement types.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShotMovement {
    #[default]
    Static,
    Pan,
    Tilt,
    Dolly,
    Track,
    Handheld,
    Steadicam,
    Crane,
    Zoom,
}

impl ShotMovement {
    pub const ALL: [Self; 9] = [
        Self::Static,
        Self::Pan,
        Self::Tilt,
        Self::Dolly,
        Self::Track,
        Self::Handheld,
        Self::Steadicam,
        Self::Crane,
        Self::Zoom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Static => "Static",
            Self::Pan => "Pan",
            Self::Tilt => "Tilt",
            Self::Dolly => "Dolly",
            Self::Track => "Tracking",
            Self::Handheld => "Handheld",
            Self::Steadicam => "Steadicam",
            Self::Crane => "Crane / Jib",
            Self::Zoom => "Zoom",
        }
    }
}

/// Curated 10-color studio palette for distinguishing camera setups on both
/// dark/light UI surfaces and white printed PDF pages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShotColor {
    #[default]
    Cobalt,
    Emerald,
    Amber,
    Crimson,
    Violet,
    Teal,
    Coral,
    Indigo,
    Olive,
    Magenta,
}

impl ShotColor {
    pub const ALL: [Self; 10] = [
        Self::Cobalt,
        Self::Emerald,
        Self::Amber,
        Self::Crimson,
        Self::Violet,
        Self::Teal,
        Self::Coral,
        Self::Indigo,
        Self::Olive,
        Self::Magenta,
    ];

    pub fn from_index(i: usize) -> Self {
        Self::ALL[i % Self::ALL.len()]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Cobalt => "Cobalt",
            Self::Emerald => "Emerald",
            Self::Amber => "Amber",
            Self::Crimson => "Crimson",
            Self::Violet => "Violet",
            Self::Teal => "Teal",
            Self::Coral => "Coral",
            Self::Indigo => "Indigo",
            Self::Olive => "Olive",
            Self::Magenta => "Magenta",
        }
    }

    pub fn rgb(self) -> (u8, u8, u8) {
        match self {
            Self::Cobalt => (59, 130, 246),
            Self::Emerald => (16, 185, 129),
            Self::Amber => (245, 158, 11),
            Self::Crimson => (239, 68, 68),
            Self::Violet => (139, 92, 246),
            Self::Teal => (20, 184, 166),
            Self::Coral => (249, 115, 22),
            Self::Indigo => (99, 102, 241),
            Self::Olive => (132, 204, 22),
            Self::Magenta => (236, 72, 153),
        }
    }
}

/// One inclusive screenplay range covered by a [`Shot`], anchored by stable
/// [`ScriptBlock::id`] UUIDs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShotRange {
    pub start_block: Uuid,
    pub end_block: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_char: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_char: Option<usize>,
}

impl ShotRange {
    pub fn blocks(start_block: Uuid, end_block: Uuid) -> Self {
        Self {
            start_block,
            end_block,
            start_char: None,
            end_char: None,
        }
    }

    /// Exact character-range selection `[start_char, end_char)` spanning one
    /// or more screenplay blocks.
    pub fn chars(start_block: Uuid, start_char: usize, end_block: Uuid, end_char: usize) -> Self {
        Self {
            start_block,
            end_block,
            start_char: Some(start_char),
            end_char: Some(end_char),
        }
    }

    /// Resolve to inclusive `(lo, hi)` indices in `blocks`. Gracefully falls
    /// back to whichever endpoint still exists if the other was deleted.
    pub fn resolve(&self, index: &HashMap<Uuid, usize>) -> Option<(usize, usize)> {
        match (index.get(&self.start_block), index.get(&self.end_block)) {
            (Some(&a), Some(&b)) => Some((a.min(b), a.max(b))),
            (Some(&a), None) => Some((a, a)),
            (None, Some(&b)) => Some((b, b)),
            (None, None) => None,
        }
    }

    /// Resolve to ordered `((lo_block, lo_char), (hi_block, hi_char))` in
    /// `blocks`, clamping char offsets to each block's char length. Whole-block
    /// ranges (`start_char`/`end_char` `None`) span `0..block_len`.
    pub fn resolve_chars(
        &self,
        index: &HashMap<Uuid, usize>,
        blocks: &[ScriptBlock],
    ) -> Option<((usize, usize), (usize, usize))> {
        let len_of = |bi: usize| blocks.get(bi).map(|b| b.text.chars().count()).unwrap_or(0);
        match (index.get(&self.start_block), index.get(&self.end_block)) {
            (Some(&a), Some(&b)) => {
                let ca = self.start_char.unwrap_or(0).min(len_of(a));
                let cb = self.end_char.unwrap_or_else(|| len_of(b)).min(len_of(b));
                if (a, ca) <= (b, cb) {
                    Some(((a, ca), (b, cb)))
                } else {
                    Some(((b, cb), (a, ca)))
                }
            }
            (Some(&a), None) => {
                let l = len_of(a);
                let ca = self.start_char.unwrap_or(0).min(l);
                Some(((a, ca), (a, l)))
            }
            (None, Some(&b)) => {
                let cb = self.end_char.unwrap_or_else(|| len_of(b)).min(len_of(b));
                Some(((b, 0), (b, cb)))
            }
            (None, None) => None,
        }
    }

    /// Half-open `[c0, c1)` character slice of `blocks[bi]` covered by this
    /// range, or `None` if `bi` has no covered characters.
    pub fn char_span_in_block(
        &self,
        bi: usize,
        index: &HashMap<Uuid, usize>,
        blocks: &[ScriptBlock],
    ) -> Option<(usize, usize)> {
        let ((lo_b, lo_c), (hi_b, hi_c)) = self.resolve_chars(index, blocks)?;
        if bi < lo_b || bi > hi_b {
            return None;
        }
        let len = blocks.get(bi).map(|b| b.text.chars().count()).unwrap_or(0);
        let (c0, c1) = if lo_b == hi_b {
            (lo_c, hi_c)
        } else if bi == lo_b {
            (lo_c, len)
        } else if bi == hi_b {
            (0, hi_c)
        } else {
            (0, len)
        };
        if len == 0 {
            // Whole-block span on an empty block still counts as covering it.
            if self.start_char.is_none() && self.end_char.is_none() {
                Some((0, 0))
            } else {
                None
            }
        } else if c0 < c1 {
            Some((c0, c1))
        } else {
            None
        }
    }

    /// Readable text slice covered by this single range.
    pub fn excerpt(&self, index: &HashMap<Uuid, usize>, blocks: &[ScriptBlock]) -> String {
        let Some(((lo_b, _), (hi_b, _))) = self.resolve_chars(index, blocks) else {
            return String::new();
        };
        let mut parts = Vec::new();
        for (bi, block) in blocks.iter().enumerate().take(hi_b + 1).skip(lo_b) {
            if let Some((c0, c1)) = self.char_span_in_block(bi, index, blocks) {
                let slice = slice_chars(&block.text, c0, c1).trim().replace('\n', " ");
                if !slice.is_empty() {
                    parts.push(slice);
                }
            }
        }
        parts.join(" / ")
    }
}

fn slice_chars(s: &str, c0: usize, c1: usize) -> String {
    if c0 >= c1 {
        return String::new();
    }
    s.chars().skip(c0).take(c1 - c0).collect()
}

/// One production camera setup / shot covering one or more screenplay beats.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shot {
    pub id: Uuid,
    /// Stable [`ScriptBlock::id`] of the scene heading this shot belongs to.
    #[serde(default)]
    pub scene_id: Option<Uuid>,
    /// Production shot code (e.g. `"1A"`, `"1B"`, `"2A"`).
    pub code: String,
    #[serde(default)]
    pub size: ShotSize,
    #[serde(default)]
    pub movement: ShotMovement,
    /// Lens / focal length note (e.g. `"35mm"`, `"50mm"`).
    #[serde(default)]
    pub lens: String,
    /// Director / DP setup description.
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub color: ShotColor,
    /// Script ranges covered by this shot.
    #[serde(default)]
    pub spans: Vec<ShotRange>,
}

/// Versioned collection of shots persisted in `.kitsp` (`scriptcraft_shots`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShotList {
    #[serde(default = "default_shots_schema")]
    pub schema: u32,
    #[serde(default)]
    pub shots: Vec<Shot>,
}

impl Default for ShotList {
    fn default() -> Self {
        Self {
            schema: SHOTS_SCHEMA,
            shots: Vec::new(),
        }
    }
}

/// Standard Hollywood shot letter sequence (skips `I` and `O` to avoid
/// confusion with `1` and `0`).
const SHOT_LETTERS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";

/// Format a zero-based shot sequence index (`0 -> "A"`, `8 -> "J"`, `24 -> "AA"`).
pub fn shot_letter(index: usize) -> String {
    let base = SHOT_LETTERS.len();
    let mut n = index;
    let mut chars = Vec::new();
    loop {
        chars.push(SHOT_LETTERS[n % base] as char);
        if n < base {
            break;
        }
        n = n / base - 1;
    }
    chars.reverse();
    chars.into_iter().collect()
}

/// Map `ScriptBlock::id -> index` for fast UUID range resolution.
pub fn block_index_map(blocks: &[ScriptBlock]) -> HashMap<Uuid, usize> {
    blocks.iter().enumerate().map(|(i, b)| (b.id, i)).collect()
}

impl ShotList {
    pub fn is_empty(&self) -> bool {
        self.shots.is_empty()
    }

    pub fn len(&self) -> usize {
        self.shots.len()
    }

    pub fn get(&self, id: Uuid) -> Option<&Shot> {
        self.shots.iter().find(|s| s.id == id)
    }

    pub fn get_mut(&mut self, id: Uuid) -> Option<&mut Shot> {
        self.shots.iter_mut().find(|s| s.id == id)
    }

    /// Generate the next unused production shot code for a scene (e.g. `"1A"`,
    /// `"1B"`, …). `scene_label` is typically `"1"`, `"2"`, or a locked label
    /// like `"12A"`.
    pub fn next_code_for_scene(&self, scene_label: &str, scene_id: Option<Uuid>) -> String {
        let prefix = scene_label.trim().trim_end_matches('.');
        let existing: HashSet<String> = self
            .shots
            .iter()
            .filter(|s| s.scene_id == scene_id)
            .map(|s| s.code.trim().to_ascii_uppercase())
            .collect();
        for i in 0..500 {
            let candidate = format!("{prefix}{}", shot_letter(i));
            if !existing.contains(&candidate.to_ascii_uppercase()) {
                return candidate;
            }
        }
        format!("{prefix}X")
    }

    /// Pick the first unused [`ShotColor`] in this scene (wrapping once all 10
    /// are used).
    pub fn next_color_for_scene(&self, scene_id: Option<Uuid>) -> ShotColor {
        let used: Vec<ShotColor> = self
            .shots
            .iter()
            .filter(|s| s.scene_id == scene_id)
            .map(|s| s.color)
            .collect();
        for color in ShotColor::ALL {
            if !used.contains(&color) {
                return color;
            }
        }
        ShotColor::from_index(used.len())
    }

    /// Create a new shot covering `range` (with smart default code and color)
    /// and return its id.
    #[allow(clippy::too_many_arguments)]
    pub fn add_shot(
        &mut self,
        scene_id: Option<Uuid>,
        scene_label: &str,
        size: ShotSize,
        movement: ShotMovement,
        lens: String,
        description: String,
        range: ShotRange,
    ) -> Uuid {
        let code = self.next_code_for_scene(scene_label, scene_id);
        let color = self.next_color_for_scene(scene_id);
        let id = Uuid::new_v4();
        self.shots.push(Shot {
            id,
            scene_id,
            code,
            size,
            movement,
            lens,
            description,
            color,
            spans: vec![range],
        });
        id
    }

    /// Add another screenplay range to an existing shot (avoiding exact dupes).
    pub fn assign_range(&mut self, shot_id: Uuid, range: ShotRange) -> bool {
        let Some(shot) = self.get_mut(shot_id) else {
            return false;
        };
        if shot.spans.contains(&range) {
            return false;
        }
        shot.spans.push(range);
        true
    }

    /// Remove a span by index from `shot_id`. Returns `true` if removed.
    pub fn remove_span(&mut self, shot_id: Uuid, span_index: usize) -> bool {
        let Some(shot) = self.get_mut(shot_id) else {
            return false;
        };
        if span_index < shot.spans.len() {
            shot.spans.remove(span_index);
            true
        } else {
            false
        }
    }

    /// Delete a shot by id.
    pub fn delete_shot(&mut self, shot_id: Uuid) -> bool {
        let before = self.shots.len();
        self.shots.retain(|s| s.id != shot_id);
        self.shots.len() != before
    }

    /// Move `shot_id` up (`delta < 0`) or down (`delta > 0`) within its own
    /// scene's ordering.
    pub fn move_shot_in_scene(&mut self, shot_id: Uuid, delta: isize) -> bool {
        let Some(pos) = self.shots.iter().position(|s| s.id == shot_id) else {
            return false;
        };
        let scene_id = self.shots[pos].scene_id;
        let same_scene: Vec<usize> = self
            .shots
            .iter()
            .enumerate()
            .filter(|(_, s)| s.scene_id == scene_id)
            .map(|(i, _)| i)
            .collect();
        let Some(rel) = same_scene.iter().position(|&i| i == pos) else {
            return false;
        };
        let target_rel = if delta < 0 {
            rel.checked_sub(1)
        } else {
            (rel + 1 < same_scene.len()).then_some(rel + 1)
        };
        let Some(target_rel) = target_rel else {
            return false;
        };
        self.shots.swap(pos, same_scene[target_rel]);
        true
    }

    /// For every block index `bi`, return the `(shot_index, start_char, end_char)`
    /// half-open character intervals covering `blocks[bi]` (preserving shot list
    /// order).
    pub fn resolve_block_char_coverage(
        &self,
        blocks: &[ScriptBlock],
    ) -> Vec<Vec<(usize, usize, usize)>> {
        let mut cov = vec![Vec::new(); blocks.len()];
        if self.shots.is_empty() || blocks.is_empty() {
            return cov;
        }
        let index = block_index_map(blocks);
        for (si, shot) in self.shots.iter().enumerate() {
            for span in &shot.spans {
                let Some(((lo_b, _), (hi_b, _))) = span.resolve_chars(&index, blocks) else {
                    continue;
                };
                for (bi, slot) in cov.iter_mut().enumerate().take(hi_b + 1).skip(lo_b) {
                    if let Some((c0, c1)) = span.char_span_in_block(bi, &index, blocks) {
                        let entry = (si, c0, c1);
                        if !slot.contains(&entry) {
                            slot.push(entry);
                        }
                    }
                }
            }
        }
        cov
    }

    /// For every block index `bi`, return the indices into `self.shots` that
    /// cover any part of `bi` (preserving shot list order, deduplicated per block).
    pub fn resolve_block_coverage(&self, blocks: &[ScriptBlock]) -> Vec<Vec<usize>> {
        self.resolve_block_char_coverage(blocks)
            .into_iter()
            .map(|spans| {
                let mut ids = Vec::new();
                for (si, _, _) in spans {
                    if !ids.contains(&si) {
                        ids.push(si);
                    }
                }
                ids
            })
            .collect()
    }

    /// Indices into `self.shots` that belong to `scene_id` (or cover any block
    /// inside `scene`).
    pub fn shots_for_scene(&self, scene: &Scene, blocks: &[ScriptBlock]) -> Vec<usize> {
        let scene_blocks: HashSet<Uuid> = scene
            .blocks
            .iter()
            .filter_map(|&bi| blocks.get(bi).map(|b| b.id))
            .collect();
        self.shots
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                s.scene_id == Some(scene.id)
                    || s.spans.iter().any(|sp| {
                        scene_blocks.contains(&sp.start_block)
                            || scene_blocks.contains(&sp.end_block)
                    })
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// `(shot_count, covered_body_blocks, total_body_blocks)` for one scene
    /// (excluding the heading and non-printable notes).
    pub fn scene_coverage_stats(
        &self,
        scene: &Scene,
        blocks: &[ScriptBlock],
        coverage: &[Vec<usize>],
    ) -> (usize, usize, usize) {
        let shot_count = self.shots_for_scene(scene, blocks).len();
        let mut total = 0usize;
        let mut covered = 0usize;
        for &bi in &scene.blocks {
            let Some(b) = blocks.get(bi) else { continue };
            if b.element_type == ElementType::SceneHeading || !b.element_type.is_printable() {
                continue;
            }
            total += 1;
            if coverage.get(bi).is_some_and(|v| !v.is_empty()) {
                covered += 1;
            }
        }
        (shot_count, covered, total)
    }

    /// Short readable excerpt of the exact screenplay text covered by `shot`.
    pub fn covered_excerpt(&self, shot: &Shot, blocks: &[ScriptBlock]) -> String {
        let index = block_index_map(blocks);
        let mut per_block: HashMap<usize, Vec<(usize, usize)>> = HashMap::new();
        for span in &shot.spans {
            let Some(((lo_b, _), (hi_b, _))) = span.resolve_chars(&index, blocks) else {
                continue;
            };
            for bi in lo_b..=hi_b {
                if let Some((c0, c1)) = span.char_span_in_block(bi, &index, blocks) {
                    per_block.entry(bi).or_default().push((c0, c1));
                }
            }
        }
        let mut block_indices: Vec<usize> = per_block.keys().copied().collect();
        block_indices.sort_unstable();

        let mut parts: Vec<String> = Vec::new();
        for bi in block_indices {
            let Some(block) = blocks.get(bi) else {
                continue;
            };
            let intervals = merge_intervals(per_block.remove(&bi).unwrap_or_default());
            for (c0, c1) in intervals {
                let t = slice_chars(&block.text, c0, c1).trim().replace('\n', " ");
                if !t.is_empty() {
                    parts.push(t);
                }
            }
        }
        let joined = parts.join(" / ");
        if joined.chars().count() > 96 {
            let mut s: String = joined.chars().take(95).collect();
            s.push('…');
            s
        } else {
            joined
        }
    }

    /// Estimated screen duration (seconds) of the text covered by `shot`.
    pub fn shot_duration_secs(&self, shot: &Shot, blocks: &[ScriptBlock]) -> f64 {
        let index = block_index_map(blocks);
        let mut per_block: HashMap<usize, Vec<(usize, usize)>> = HashMap::new();
        for span in &shot.spans {
            let Some(((lo_b, _), (hi_b, _))) = span.resolve_chars(&index, blocks) else {
                continue;
            };
            for bi in lo_b..=hi_b {
                if let Some((c0, c1)) = span.char_span_in_block(bi, &index, blocks) {
                    per_block.entry(bi).or_default().push((c0, c1));
                }
            }
        }
        let mut action_words = 0usize;
        let mut dialog_words = 0usize;
        for (bi, raw) in per_block {
            let Some(b) = blocks.get(bi) else {
                continue;
            };
            if !b.element_type.is_printable() {
                continue;
            }
            let w: usize = merge_intervals(raw)
                .into_iter()
                .map(|(c0, c1)| slice_chars(&b.text, c0, c1).split_whitespace().count())
                .sum();
            if matches!(
                b.element_type,
                ElementType::Dialogue | ElementType::Parenthetical | ElementType::Lyrics
            ) {
                dialog_words += w;
            } else {
                action_words += w;
            }
        }
        action_words as f64 * ACTION_S_PER_WORD + dialog_words as f64 * DIALOG_S_PER_WORD
    }

    /// Export the shot list to UTF-8 BOM RFC-4180 CSV (Excel + Cyrillic safe).
    pub fn to_csv(
        &self,
        blocks: &[ScriptBlock],
        scenes: &[Scene],
        scene_labels: &[String],
    ) -> String {
        fn esc(s: &str) -> String {
            if s.contains([',', '"', '\n', '\r']) {
                format!("\"{}\"", s.replace('"', "\"\""))
            } else {
                s.to_string()
            }
        }
        let scene_label_for = |scene_id: Option<Uuid>| -> (String, String) {
            let Some(sid) = scene_id else {
                return (String::new(), String::new());
            };
            for (i, sc) in scenes.iter().enumerate() {
                if sc.id == sid {
                    let num = scene_labels
                        .get(i)
                        .cloned()
                        .unwrap_or_else(|| (i + 1).to_string());
                    return (num, sc.heading.clone());
                }
            }
            (String::new(), String::new())
        };

        let mut out = String::from("\u{FEFF}Scene,Heading,Shot,Size,Movement,Lens,Color,Description,Covered Text,Est. Duration (s)\r\n");
        for shot in &self.shots {
            let (sc_num, heading) = scene_label_for(shot.scene_id);
            let excerpt = self.covered_excerpt(shot, blocks);
            let dur = self.shot_duration_secs(shot, blocks).round() as u64;
            let row = [
                esc(&sc_num),
                esc(&heading),
                esc(&shot.code),
                esc(shot.size.short()),
                esc(shot.movement.label()),
                esc(&shot.lens),
                esc(shot.color.label()),
                esc(&shot.description),
                esc(&excerpt),
                dur.to_string(),
            ]
            .join(",");
            out.push_str(&row);
            out.push_str("\r\n");
        }
        out
    }
}

fn merge_intervals(mut raw: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    raw.retain(|(a, b)| a < b);
    if raw.is_empty() {
        return raw;
    }
    raw.sort_unstable();
    let mut merged = vec![raw[0]];
    for (a, b) in raw.into_iter().skip(1) {
        let last = merged.last_mut().unwrap();
        if a <= last.1 {
            last.1 = last.1.max(b);
        } else {
            merged.push((a, b));
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Project;

    fn sample_project() -> Project {
        Project {
            title: "Test".into(),
            blocks: vec![
                ScriptBlock::new(ElementType::SceneHeading, "INT. KITCHEN - DAY".into()),
                ScriptBlock::new(ElementType::Action, "Coffee boils on the stove.".into()),
                ScriptBlock::new(ElementType::Character, "IVAN".into()),
                ScriptBlock::new(ElementType::Dialogue, "We leave at dawn.".into()),
                ScriptBlock::new(ElementType::Action, "He grabs the keys.".into()),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn shot_letter_skips_i_and_o() {
        assert_eq!(shot_letter(0), "A");
        assert_eq!(shot_letter(7), "H");
        assert_eq!(shot_letter(8), "J"); // skips I
        assert_eq!(shot_letter(12), "N");
        assert_eq!(shot_letter(13), "P"); // skips O
        assert_eq!(shot_letter(23), "Z");
        assert_eq!(shot_letter(24), "AA");
    }

    #[test]
    fn overlapping_shots_resolve_and_export_csv() {
        let p = sample_project();
        let scenes = p.scenes();
        let scene = &scenes[0];
        let mut list = ShotList::default();
        let s1 = list.add_shot(
            Some(scene.id),
            "1",
            ShotSize::Ws,
            ShotMovement::Static,
            "24mm".into(),
            "Master of the kitchen".into(),
            ShotRange::blocks(p.blocks[1].id, p.blocks[4].id),
        );
        let s2 = list.add_shot(
            Some(scene.id),
            "1",
            ShotSize::Cu,
            ShotMovement::Handheld,
            "50mm".into(),
            "Close on Ivan's face".into(),
            ShotRange::blocks(p.blocks[2].id, p.blocks[3].id),
        );
        assert_eq!(list.get(s1).unwrap().code, "1A");
        assert_eq!(list.get(s2).unwrap().code, "1B");
        assert_ne!(list.get(s1).unwrap().color, list.get(s2).unwrap().color);

        let cov = list.resolve_block_coverage(&p.blocks);
        assert!(cov[0].is_empty());
        assert_eq!(cov[1], vec![0]);
        assert_eq!(cov[2], vec![0, 1]);
        assert_eq!(cov[3], vec![0, 1]);
        assert_eq!(cov[4], vec![0]);

        let (count, covered, total) = list.scene_coverage_stats(scene, &p.blocks, &cov);
        assert_eq!((count, covered, total), (2, 4, 4));

        let csv = list.to_csv(&p.blocks, &scenes, &["1".into()]);
        assert!(csv.starts_with('\u{FEFF}'));
        assert!(csv.contains("1A,WS,Static,24mm"));
        assert!(csv.contains("1B,CU,Handheld,50mm"));
    }

    #[test]
    fn reorder_and_delete_shots_in_scene() {
        let p = sample_project();
        let sid = p.blocks[0].id;
        let mut list = ShotList::default();
        let a = list.add_shot(
            Some(sid),
            "1",
            ShotSize::Ws,
            ShotMovement::Static,
            String::new(),
            "First".into(),
            ShotRange::blocks(p.blocks[1].id, p.blocks[1].id),
        );
        let b = list.add_shot(
            Some(sid),
            "1",
            ShotSize::Cu,
            ShotMovement::Dolly,
            String::new(),
            "Second".into(),
            ShotRange::blocks(p.blocks[2].id, p.blocks[2].id),
        );
        assert!(list.move_shot_in_scene(b, -1));
        assert_eq!(list.shots[0].id, b);
        assert_eq!(list.shots[1].id, a);
        assert!(list.delete_shot(b));
        assert_eq!(list.len(), 1);
        assert_eq!(list.shots[0].id, a);
    }

    #[test]
    fn char_range_slicing_excerpt_and_block_coverage() {
        let p = sample_project();
        let sid = p.blocks[0].id;
        let mut list = ShotList::default();
        // Select only "boils on the stove" inside block 1 ("Coffee boils on the stove.")
        let id = list.add_shot(
            Some(sid),
            "1",
            ShotSize::Insert,
            ShotMovement::Static,
            "85mm".into(),
            "Stove insert".into(),
            ShotRange::chars(p.blocks[1].id, 7, p.blocks[1].id, 25),
        );
        let shot = list.get(id).unwrap();
        assert_eq!(list.covered_excerpt(shot, &p.blocks), "boils on the stove");
        let char_cov = list.resolve_block_char_coverage(&p.blocks);
        assert_eq!(char_cov[1], vec![(0, 7, 25)]);
        assert!(char_cov[2].is_empty());

        // Cross-block partial char selection: tail of block 1 into head of block 3
        let id2 = list.add_shot(
            Some(sid),
            "1",
            ShotSize::Cu,
            ShotMovement::Handheld,
            "50mm".into(),
            "Cross-block cut".into(),
            ShotRange::chars(p.blocks[1].id, 20, p.blocks[3].id, 8),
        );
        let shot2 = list.get(id2).unwrap();
        let ex2 = list.covered_excerpt(shot2, &p.blocks);
        assert!(ex2.starts_with("stove."), "got: {ex2}");
        assert!(ex2.ends_with("We leave"), "got: {ex2}");
    }
}
