use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, VecDeque};
use uuid::Uuid;

use crate::model::{ElementType, ScriptBlock};

pub const STORAGE_KEY: &str = "scriptcraft_revisions";
pub const MAX_REVISIONS: usize = 24;
const LCS_CELL_LIMIT: usize = 250_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisionColor {
    Blue,
    Pink,
    Yellow,
    Green,
    Goldenrod,
    Buff,
    Salmon,
    Cherry,
}

impl RevisionColor {
    pub const ALL: [RevisionColor; 8] = [
        Self::Blue,
        Self::Pink,
        Self::Yellow,
        Self::Green,
        Self::Goldenrod,
        Self::Buff,
        Self::Salmon,
        Self::Cherry,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Blue => "Blue",
            Self::Pink => "Pink",
            Self::Yellow => "Yellow",
            Self::Green => "Green",
            Self::Goldenrod => "Goldenrod",
            Self::Buff => "Buff",
            Self::Salmon => "Salmon",
            Self::Cherry => "Cherry",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Blue => Self::Pink,
            Self::Pink => Self::Yellow,
            Self::Yellow => Self::Green,
            Self::Green => Self::Goldenrod,
            Self::Goldenrod => Self::Buff,
            Self::Buff => Self::Salmon,
            Self::Salmon => Self::Cherry,
            Self::Cherry => Self::Blue,
        }
    }

    pub fn rgb(self) -> (u8, u8, u8) {
        match self {
            Self::Blue => (0x2F, 0x6F, 0xEB),
            Self::Pink => (0xE8, 0x3E, 0x8C),
            Self::Yellow => (0xE6, 0xB8, 0x00),
            Self::Green => (0x2E, 0x9E, 0x5B),
            Self::Goldenrod => (0xC5, 0x8B, 0x12),
            Self::Buff => (0xC9, 0xA2, 0x27),
            Self::Salmon => (0xE0, 0x6C, 0x5A),
            Self::Cherry => (0xB5, 0x12, 0x3E),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RevisionBlock {
    pub element_type: ElementType,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kit_uuid: Option<String>,
    /// Stable `ScriptBlock::id` captured when the revision starts. Missing in
    /// baselines written before v0.8; those still diff by content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_id: Option<Uuid>,
}

impl From<&ScriptBlock> for RevisionBlock {
    fn from(b: &ScriptBlock) -> Self {
        Self {
            element_type: b.element_type,
            text: b.text.clone(),
            kit_uuid: b.kit_uuid.clone(),
            block_id: Some(b.id),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RevisionSet {
    pub color: RevisionColor,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub started: String,
    #[serde(default)]
    pub pages: usize,
    #[serde(default)]
    pub baseline: Vec<RevisionBlock>,
}

/// How one block changed between two drafts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffKind {
    Unchanged,
    Added,
    Removed,
    Changed,
}

impl DiffKind {
    pub fn is_change(self) -> bool {
        !matches!(self, Self::Unchanged)
    }
}

/// One aligned block in a draft comparison.
///
/// `scene_index` is the index of the scene in the target draft (or target
/// baseline for [`DraftDiff::between`]). Removed scenes use the target scene
/// index before which they would appear; if they belong after the last target
/// scene, `scene_index == target_scene_count`. `current_block` is the global
/// block index in the target draft/baseline, and is `None` for removed blocks.
#[derive(Clone, Debug, PartialEq)]
pub struct DiffEntry {
    pub kind: DiffKind,
    pub scene_index: usize,
    pub scene_heading: String,
    pub base: Option<RevisionBlock>,
    pub current: Option<ScriptBlock>,
    pub current_block: Option<usize>,
}

impl DiffEntry {
    pub fn is_change(&self) -> bool {
        self.kind.is_change()
    }
}

/// Deterministic, block-level comparison between two drafts.
///
/// Unchanged entries are retained for context. Scenes and blocks with stable
/// IDs are matched by ID first; old baselines fall back to KIT scene UUIDs,
/// heading text and a bounded content diff. The output order follows the
/// target draft, with removed scenes/blocks inserted where they were removed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DraftDiff {
    pub entries: Vec<DiffEntry>,
    pub added: usize,
    pub removed: usize,
    pub changed: usize,
    /// Sorted target-scene indices touched by any added, removed or changed
    /// block. A removed scene contributes its insertion index, which can equal
    /// the target scene count when it followed the last surviving scene.
    pub scenes_changed: Vec<usize>,
}

impl DraftDiff {
    /// Compare a revision baseline with the live draft.
    pub fn against_current(base: &RevisionSet, current: &[ScriptBlock]) -> Self {
        build_diff(&base.baseline, current)
    }

    /// Compare two revision baselines. The target baseline is exposed as the
    /// `current` side of each [`DiffEntry`] for display.
    pub fn between(base: &RevisionSet, target: &RevisionSet) -> Self {
        let current: Vec<ScriptBlock> = target.baseline.iter().map(revision_to_script).collect();
        build_diff(&base.baseline, &current)
    }

    pub fn has_changes(&self) -> bool {
        self.added + self.removed + self.changed > 0
    }
}

impl RevisionSet {
    pub fn diff_against_current(&self, current: &[ScriptBlock]) -> DraftDiff {
        DraftDiff::against_current(self, current)
    }

    pub fn diff_against(&self, target: &RevisionSet) -> DraftDiff {
        DraftDiff::between(self, target)
    }
}

fn revision_to_script(b: &RevisionBlock) -> ScriptBlock {
    ScriptBlock {
        id: b.block_id.unwrap_or_else(Uuid::nil),
        kit_uuid: b.kit_uuid.clone(),
        element_type: b.element_type,
        text: b.text.clone(),
    }
}

// ---------------------------------------------------------------------------
// Draft diff engine
// ---------------------------------------------------------------------------

/// Common accessors used to split either a revision baseline or a live draft
/// into scenes without cloning the whole script.
trait DiffBlock {
    fn element_type(&self) -> ElementType;
    fn text(&self) -> &str;
    fn block_id(&self) -> Option<Uuid>;
    fn kit_uuid(&self) -> Option<&str>;
}

impl DiffBlock for RevisionBlock {
    fn element_type(&self) -> ElementType {
        self.element_type
    }

    fn text(&self) -> &str {
        &self.text
    }

    fn block_id(&self) -> Option<Uuid> {
        self.block_id
    }

    fn kit_uuid(&self) -> Option<&str> {
        self.kit_uuid.as_deref()
    }
}

impl DiffBlock for ScriptBlock {
    fn element_type(&self) -> ElementType {
        self.element_type
    }

    fn text(&self) -> &str {
        &self.text
    }

    fn block_id(&self) -> Option<Uuid> {
        Some(self.id)
    }

    fn kit_uuid(&self) -> Option<&str> {
        self.kit_uuid.as_deref()
    }
}

/// One scene in an aligned draft: `start..end` covers the heading through the
/// last block before the next heading. Blocks before the first heading are the
/// preamble and are compared separately.
#[derive(Clone, Debug)]
struct SceneSpan {
    start: usize,
    end: usize,
    heading: String,
    block_id: Option<Uuid>,
    kit_key: Option<String>,
}

/// Normalize a KIT scene UUID for matching. KIT writes braces around some
/// UUIDs and uses the nil UUID for scenes created outside KIT; both spellings
/// should compare equal, and nil never identifies a scene.
fn kit_key(uuid: Option<&str>) -> Option<String> {
    let raw = uuid?.trim();
    if raw.is_empty() {
        return None;
    }
    let inner = raw
        .trim_start_matches('{')
        .trim_end_matches('}')
        .trim()
        .to_ascii_lowercase();
    if inner.is_empty() || inner.chars().all(|c| c == '0' || c == '-') {
        return None;
    }
    Some(inner)
}

fn scene_spans<T: DiffBlock>(blocks: &[T]) -> (usize, Vec<SceneSpan>) {
    let Some(first) = blocks
        .iter()
        .position(|b| b.element_type() == ElementType::SceneHeading)
    else {
        return (blocks.len(), Vec::new());
    };
    let mut spans = Vec::new();
    let mut start = first;
    for (i, block) in blocks.iter().enumerate().skip(first + 1) {
        if block.element_type() == ElementType::SceneHeading {
            spans.push(span_at(blocks, start, i));
            start = i;
        }
    }
    spans.push(span_at(blocks, start, blocks.len()));
    (first, spans)
}

fn span_at<T: DiffBlock>(blocks: &[T], start: usize, end: usize) -> SceneSpan {
    let heading = &blocks[start];
    SceneSpan {
        start,
        end,
        heading: heading.text().to_string(),
        block_id: heading.block_id(),
        kit_key: kit_key(heading.kit_uuid()),
    }
}

/// Match scenes in priority order: stable block id, KIT UUID, heading text
/// (with a stable-identity conflict guard), body content, then position.
///
/// The returned vector maps base scene index to target scene index. Matching is
/// deliberately allowed to move backwards so reordered scenes are not reported
/// as a delete plus an insert.
fn match_scenes(
    base_blocks: &[RevisionBlock],
    current_blocks: &[ScriptBlock],
    base: &[SceneSpan],
    current: &[SceneSpan],
) -> Vec<Option<usize>> {
    let mut base_to_current = vec![None; base.len()];
    let mut current_used = vec![false; current.len()];

    // 1. Stable block ids.
    let mut by_id: HashMap<Uuid, VecDeque<usize>> = HashMap::new();
    for (i, span) in base.iter().enumerate() {
        if let Some(id) = span.block_id {
            by_id.entry(id).or_default().push_back(i);
        }
    }
    for (cj, span) in current.iter().enumerate() {
        let Some(id) = span.block_id else { continue };
        let Some(queue) = by_id.get_mut(&id) else {
            continue;
        };
        if let Some(bi) = queue.pop_front() {
            base_to_current[bi] = Some(cj);
            current_used[cj] = true;
        }
    }

    // 2. KIT scene UUIDs.
    let mut by_uuid: HashMap<&str, VecDeque<usize>> = HashMap::new();
    for (i, span) in base.iter().enumerate() {
        if base_to_current[i].is_none() {
            if let Some(key) = span.kit_key.as_deref() {
                by_uuid.entry(key).or_default().push_back(i);
            }
        }
    }
    for (cj, span) in current.iter().enumerate() {
        if current_used[cj] {
            continue;
        }
        let Some(key) = span.kit_key.as_deref() else {
            continue;
        };
        let Some(queue) = by_uuid.get_mut(key) else {
            continue;
        };
        while let Some(bi) = queue.pop_front() {
            if base_to_current[bi].is_none() {
                base_to_current[bi] = Some(cj);
                current_used[cj] = true;
                break;
            }
        }
    }

    // 3. Heading text. Prefer the candidate closest to the same relative
    // position so duplicate headings stay stable across small edits.
    for (cj, span) in current.iter().enumerate() {
        if current_used[cj] {
            continue;
        }
        let mut best: Option<(usize, usize)> = None;
        for (bi, candidate) in base.iter().enumerate() {
            if base_to_current[bi].is_some()
                || candidate.heading != span.heading
                || scene_identity_conflict(candidate, span)
            {
                continue;
            }
            let distance = (bi * current.len()).abs_diff(cj * base.len());
            if best.is_none_or(|(best_distance, best_bi)| {
                distance < best_distance || (distance == best_distance && bi < best_bi)
            }) {
                best = Some((distance, bi));
            }
        }
        if let Some((_, bi)) = best {
            base_to_current[bi] = Some(cj);
            current_used[cj] = true;
        }
    }

    // 4. Shared body content. This catches renamed scenes in baselines
    // written before stable block ids existed.
    let base_left: Vec<usize> = (0..base.len())
        .filter(|i| base_to_current[*i].is_none())
        .collect();
    let current_left: Vec<usize> = (0..current.len()).filter(|i| !current_used[*i]).collect();
    let mut candidates = Vec::new();
    for &cj in &current_left {
        for &bi in &base_left {
            let shared = body_similarity(
                &base_blocks[base[bi].start..base[bi].end],
                &current_blocks[current[cj].start..current[cj].end],
            );
            if shared == 0 {
                continue;
            }
            let longest = (base[bi].end - base[bi].start).max(current[cj].end - current[cj].start);
            if shared * 10 >= longest * 3 {
                candidates.push((shared, cj, bi));
            }
        }
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    for (_, cj, bi) in candidates {
        if base_to_current[bi].is_none() && !current_used[cj] {
            base_to_current[bi] = Some(cj);
            current_used[cj] = true;
        }
    }

    // 5. Positional fallback: if the same number of scenes remain on each side
    // they most likely represent rewrites of one another. Never pair scenes
    // whose stable identities explicitly disagree.
    let base_left: Vec<usize> = (0..base.len())
        .filter(|i| base_to_current[*i].is_none())
        .collect();
    let current_left: Vec<usize> = (0..current.len()).filter(|i| !current_used[*i]).collect();
    if base_left.len() == current_left.len() {
        for (bi, cj) in base_left.into_iter().zip(current_left) {
            if !scene_identity_conflict(&base[bi], &current[cj]) {
                base_to_current[bi] = Some(cj);
            }
        }
    }

    base_to_current
}

fn scene_identity_conflict(base: &SceneSpan, current: &SceneSpan) -> bool {
    if let (Some(a), Some(b)) = (base.block_id, current.block_id) {
        if a != b {
            return true;
        }
    }
    if let (Some(a), Some(b)) = (base.kit_key.as_deref(), current.kit_key.as_deref()) {
        if a != b {
            return true;
        }
    }
    false
}

/// Number of identical non-heading blocks shared by two scenes, counted as a
/// multiset so repeated action lines cannot inflate the score.
fn body_similarity(base: &[RevisionBlock], current: &[ScriptBlock]) -> usize {
    let mut counts: HashMap<(&str, &str), usize> = HashMap::new();
    for block in base {
        if block.element_type == ElementType::SceneHeading {
            continue;
        }
        *counts
            .entry((block.element_type.tag(), block.text.as_str()))
            .or_default() += 1;
    }
    let mut shared = 0usize;
    for block in current {
        if block.element_type == ElementType::SceneHeading {
            continue;
        }
        if let Some(left) = counts.get_mut(&(block.element_type.tag(), block.text.as_str())) {
            if *left > 0 {
                *left -= 1;
                shared += 1;
            }
        }
    }
    shared
}

/// One aligned block pair inside a scene. Both indices are relative to the
/// scene slices passed to [`align_blocks`].
#[derive(Clone, Copy, Debug)]
struct BlockPair {
    base: Option<usize>,
    current: Option<usize>,
}

fn align_blocks(base: &[RevisionBlock], current: &[ScriptBlock]) -> Vec<BlockPair> {
    // Fast path for the common case: no blocks were inserted or deleted, so
    // every block keeps its stable id in the same position.
    if base.len() == current.len()
        && base
            .iter()
            .zip(current)
            .all(|(b, c)| b.block_id == Some(c.id))
    {
        return (0..base.len())
            .map(|i| BlockPair {
                base: Some(i),
                current: Some(i),
            })
            .collect();
    }

    let anchors = stable_anchors(base, current);
    if anchors.len() == base.len() && anchors.len() == current.len() {
        return anchors
            .into_iter()
            .map(|(base, current)| BlockPair {
                base: Some(base),
                current: Some(current),
            })
            .collect();
    }
    let mut pairs = Vec::with_capacity(base.len().max(current.len()));

    let (mut base_start, mut current_start) = (0usize, 0usize);
    for (base_anchor, current_anchor) in anchors {
        align_gap(
            &mut pairs,
            base,
            current,
            base_start..base_anchor,
            current_start..current_anchor,
        );
        pairs.push(BlockPair {
            base: Some(base_anchor),
            current: Some(current_anchor),
        });
        base_start = base_anchor + 1;
        current_start = current_anchor + 1;
    }
    align_gap(
        &mut pairs,
        base,
        current,
        base_start..base.len(),
        current_start..current.len(),
    );
    pairs
}

/// Anchors for blocks whose stable ids survived an edit. The longest
/// increasing subsequence keeps the anchors monotonic in both drafts; blocks
/// moved across anchors fall back to content matching.
fn stable_anchors(base: &[RevisionBlock], current: &[ScriptBlock]) -> Vec<(usize, usize)> {
    let mut current_by_id: HashMap<Uuid, usize> = HashMap::with_capacity(current.len());
    for (i, block) in current.iter().enumerate() {
        current_by_id.entry(block.id).or_insert(i);
    }

    let mut candidates = Vec::with_capacity(base.len());
    for (i, block) in base.iter().enumerate() {
        let Some(id) = block.block_id else { continue };
        if let Some(&j) = current_by_id.get(&id) {
            candidates.push((i, j));
        }
    }
    longest_increasing_pairs(&candidates)
}

/// Longest increasing subsequence over the target index of `(base, target)`
/// candidates. Ties keep the earliest candidate, which makes the result
/// deterministic.
fn longest_increasing_pairs(pairs: &[(usize, usize)]) -> Vec<(usize, usize)> {
    if pairs.is_empty() {
        return Vec::new();
    }
    let mut tails: Vec<usize> = Vec::new();
    let mut previous = vec![None; pairs.len()];
    for (index, (_, target)) in pairs.iter().enumerate() {
        match tails.binary_search_by_key(target, |&tail| pairs[tail].1) {
            Ok(position) => {
                previous[index] = if position > 0 {
                    Some(tails[position - 1])
                } else {
                    None
                };
                tails[position] = index;
            }
            Err(position) => {
                previous[index] = if position > 0 {
                    Some(tails[position - 1])
                } else {
                    None
                };
                if position == tails.len() {
                    tails.push(index);
                } else {
                    tails[position] = index;
                }
            }
        }
    }
    let mut out = Vec::with_capacity(tails.len());
    let mut cursor = tails.last().copied();
    while let Some(index) = cursor {
        out.push(pairs[index]);
        cursor = previous[index];
    }
    out.reverse();
    out
}

/// Align a gap between stable anchors. Exact common prefixes and suffixes are
/// cheap to peel off; the bounded edit-distance pass only sees the part that
/// actually changed.
fn align_gap(
    out: &mut Vec<BlockPair>,
    base: &[RevisionBlock],
    current: &[ScriptBlock],
    base_range: std::ops::Range<usize>,
    current_range: std::ops::Range<usize>,
) {
    let mut b0 = base_range.start;
    let mut c0 = current_range.start;
    let b1 = base_range.end;
    let c1 = current_range.end;
    if b0 == b1 && c0 == c1 {
        return;
    }
    if b1 - b0 == 1 && c1 - c0 == 1 {
        if same_block(&base[b0], &current[c0]) || base[b0].element_type == current[c0].element_type
        {
            out.push(BlockPair {
                base: Some(b0),
                current: Some(c0),
            });
        } else {
            out.push(BlockPair {
                base: Some(b0),
                current: None,
            });
            out.push(BlockPair {
                base: None,
                current: Some(c0),
            });
        }
        return;
    }

    while b0 < b1 && c0 < c1 && same_block(&base[b0], &current[c0]) {
        out.push(BlockPair {
            base: Some(b0),
            current: Some(c0),
        });
        b0 += 1;
        c0 += 1;
    }

    let mut suffix = 0usize;
    while suffix < b1 - b0
        && suffix < c1 - c0
        && same_block(&base[b1 - 1 - suffix], &current[c1 - 1 - suffix])
    {
        suffix += 1;
    }
    let b_mid = b1 - suffix;
    let c_mid = c1 - suffix;

    let base_len = b_mid - b0;
    let current_len = c_mid - c0;
    if base_len == 0 {
        for j in c0..c_mid {
            out.push(BlockPair {
                base: None,
                current: Some(j),
            });
        }
    } else if current_len == 0 {
        for i in b0..b_mid {
            out.push(BlockPair {
                base: Some(i),
                current: None,
            });
        }
    } else if base_len * current_len > LCS_CELL_LIMIT {
        align_gap_fallback(out, base, current, b0..b_mid, c0..c_mid);
    } else {
        align_gap_dp(out, base, current, b0..b_mid, c0..c_mid);
    }

    for offset in 0..suffix {
        out.push(BlockPair {
            base: Some(b_mid + offset),
            current: Some(c_mid + offset),
        });
    }
}

/// Edit-distance alignment inside one changed gap. Diagonal moves are only
/// allowed between blocks of the same element type; a type change is reported
/// as a removal plus an addition.
fn align_gap_dp(
    out: &mut Vec<BlockPair>,
    base: &[RevisionBlock],
    current: &[ScriptBlock],
    base_range: std::ops::Range<usize>,
    current_range: std::ops::Range<usize>,
) {
    let b = &base[base_range.clone()];
    let c = &current[current_range.clone()];
    let n = b.len();
    let m = c.len();
    let width = m + 1;
    let mut dp = vec![0u32; (n + 1) * width];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            let mut best = dp[(i + 1) * width + j] + 1;
            best = best.min(dp[i * width + j + 1] + 1);
            if b[i].element_type == c[j].element_type {
                let cost = u32::from(b[i].text != c[j].text);
                best = best.min(dp[(i + 1) * width + j + 1] + cost);
            }
            dp[i * width + j] = best;
        }
    }

    let (mut i, mut j) = (0usize, 0usize);
    while i < n || j < m {
        if i < n && j < m && b[i].element_type == c[j].element_type {
            let cost = u32::from(b[i].text != c[j].text);
            if dp[i * width + j] == dp[(i + 1) * width + j + 1] + cost {
                out.push(BlockPair {
                    base: Some(base_range.start + i),
                    current: Some(current_range.start + j),
                });
                i += 1;
                j += 1;
                continue;
            }
        }
        if i < n && dp[i * width + j] == dp[(i + 1) * width + j] + 1 {
            out.push(BlockPair {
                base: Some(base_range.start + i),
                current: None,
            });
            i += 1;
        } else {
            out.push(BlockPair {
                base: None,
                current: Some(current_range.start + j),
            });
            j += 1;
        }
    }
}

/// Deterministic linear fallback for a pathologically large gap. Pairs
/// same-type blocks, reports text edits as changes, and leaves the rest as
/// removals/additions.
fn align_gap_fallback(
    out: &mut Vec<BlockPair>,
    base: &[RevisionBlock],
    current: &[ScriptBlock],
    base_range: std::ops::Range<usize>,
    current_range: std::ops::Range<usize>,
) {
    let n = base_range.len();
    let m = current_range.len();
    let paired = n.min(m);
    for k in 0..paired {
        let bi = base_range.start + k;
        let cj = current_range.start + k;
        if base[bi].element_type == current[cj].element_type {
            out.push(BlockPair {
                base: Some(bi),
                current: Some(cj),
            });
        } else {
            out.push(BlockPair {
                base: Some(bi),
                current: None,
            });
            out.push(BlockPair {
                base: None,
                current: Some(cj),
            });
        }
    }
    for bi in base_range.start + paired..base_range.end {
        out.push(BlockPair {
            base: Some(bi),
            current: None,
        });
    }
    for cj in current_range.start + paired..current_range.end {
        out.push(BlockPair {
            base: None,
            current: Some(cj),
        });
    }
}

fn build_diff(base: &[RevisionBlock], current: &[ScriptBlock]) -> DraftDiff {
    let (base_preamble_end, base_scenes) = scene_spans(base);
    let (current_preamble_end, current_scenes) = scene_spans(current);
    let mut entries = Vec::new();

    // Blocks before the first scene heading belong to no scene; group them
    // with scene 0 (or the whole script when it has no headings).
    if base_preamble_end > 0 || current_preamble_end > 0 {
        let heading = current_scenes
            .first()
            .or_else(|| base_scenes.first())
            .map(|s| s.heading.as_str())
            .unwrap_or("");
        emit_alignment(
            &mut entries,
            &base[..base_preamble_end],
            &current[..current_preamble_end],
            0,
            heading,
            0,
        );
    }

    let base_to_current = match_scenes(base, current, &base_scenes, &current_scenes);
    let mut current_to_base = vec![None; current_scenes.len()];
    for (bi, cj) in base_to_current.iter().enumerate() {
        if let Some(cj) = cj {
            current_to_base[*cj] = Some(bi);
        }
    }

    // Removed scenes are emitted at the target position where they used to
    // sit, without disturbing the target draft's scene numbering.
    let mut removed_at = vec![Vec::new(); current_scenes.len() + 1];
    let mut last_matched: Option<usize> = None;
    for (bi, matched) in base_to_current.iter().enumerate() {
        if let Some(cj) = matched {
            last_matched = Some(*cj);
            continue;
        }
        let next = base_to_current[bi + 1..].iter().flatten().next().copied();
        let insertion = match (last_matched, next) {
            (Some(previous), Some(next)) => next.max(previous + 1),
            (Some(previous), None) => previous + 1,
            (None, Some(next)) => next,
            (None, None) => 0,
        }
        .min(current_scenes.len());
        removed_at[insertion].push(bi);
    }

    for target in 0..=current_scenes.len() {
        for &bi in &removed_at[target] {
            let span = &base_scenes[bi];
            emit_alignment(
                &mut entries,
                &base[span.start..span.end],
                &[],
                target,
                &span.heading,
                0,
            );
        }
        if target == current_scenes.len() {
            break;
        }
        let span = &current_scenes[target];
        match current_to_base[target] {
            Some(bi) => {
                let base_span = &base_scenes[bi];
                emit_alignment(
                    &mut entries,
                    &base[base_span.start..base_span.end],
                    &current[span.start..span.end],
                    target,
                    &span.heading,
                    span.start,
                );
            }
            None => emit_alignment(
                &mut entries,
                &[],
                &current[span.start..span.end],
                target,
                &span.heading,
                span.start,
            ),
        }
    }

    let added = entries.iter().filter(|e| e.kind == DiffKind::Added).count();
    let removed = entries
        .iter()
        .filter(|e| e.kind == DiffKind::Removed)
        .count();
    let changed = entries
        .iter()
        .filter(|e| e.kind == DiffKind::Changed)
        .count();
    let scenes_changed: Vec<usize> = entries
        .iter()
        .filter(|e| e.is_change())
        .map(|e| e.scene_index)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    DraftDiff {
        entries,
        added,
        removed,
        changed,
        scenes_changed,
    }
}

fn emit_alignment(
    out: &mut Vec<DiffEntry>,
    base: &[RevisionBlock],
    current: &[ScriptBlock],
    scene_index: usize,
    scene_heading: &str,
    current_offset: usize,
) {
    for pair in align_blocks(base, current) {
        let base_block = pair.base.map(|i| &base[i]);
        let current_block = pair.current.map(|j| &current[j]);
        let kind = match (base_block, current_block) {
            (None, None) => continue,
            (Some(_), None) => DiffKind::Removed,
            (None, Some(_)) => DiffKind::Added,
            (Some(b), Some(c)) => {
                if same_block(b, c) {
                    DiffKind::Unchanged
                } else {
                    DiffKind::Changed
                }
            }
        };
        out.push(DiffEntry {
            kind,
            scene_index,
            scene_heading: scene_heading.to_string(),
            base: base_block.cloned(),
            current: current_block.cloned(),
            current_block: pair.current.map(|j| current_offset + j),
        });
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RevisionHistory {
    #[serde(default)]
    pub sets: Vec<RevisionSet>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RevisionMarks {
    pub colors: Vec<Option<RevisionColor>>,
    pub changed: usize,
    pub deleted: usize,
}

impl RevisionHistory {
    pub fn is_empty(&self) -> bool {
        self.sets.is_empty()
    }

    pub fn latest(&self) -> Option<&RevisionSet> {
        self.sets.last()
    }

    pub fn next_color(&self) -> RevisionColor {
        self.latest()
            .map(|s| s.color.next())
            .unwrap_or(RevisionColor::Blue)
    }

    pub fn start(
        &mut self,
        blocks: &[ScriptBlock],
        name: String,
        started: String,
        pages: usize,
    ) -> RevisionColor {
        let color = self.next_color();
        self.sets.push(RevisionSet {
            color,
            name,
            started,
            pages,
            baseline: blocks.iter().map(RevisionBlock::from).collect(),
        });
        if self.sets.len() > MAX_REVISIONS {
            self.sets.remove(0);
        }
        color
    }

    pub fn clear(&mut self) {
        self.sets.clear();
    }

    pub fn marks(&self, current: &[ScriptBlock]) -> RevisionMarks {
        let mut marks = RevisionMarks {
            colors: vec![None; current.len()],
            changed: 0,
            deleted: 0,
        };
        if self.sets.is_empty() {
            return marks;
        }
        let current_segments = split_segments(current, current_scene_key);
        for set in &self.sets {
            marks.deleted = apply_set(set, current, &current_segments, &mut marks.colors);
        }
        marks.changed = marks.colors.iter().filter(|c| c.is_some()).count();
        marks
    }

    pub fn latest_marks(&self, current: &[ScriptBlock]) -> RevisionMarks {
        let mut marks = RevisionMarks {
            colors: vec![None; current.len()],
            changed: 0,
            deleted: 0,
        };
        let Some(set) = self.latest() else {
            return marks;
        };
        let current_segments = split_segments(current, current_scene_key);
        marks.deleted = apply_set(set, current, &current_segments, &mut marks.colors);
        marks.changed = marks.colors.iter().filter(|c| c.is_some()).count();
        marks
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Compare the revision set at `index` with the live draft.
    pub fn diff_against_current(&self, index: usize, current: &[ScriptBlock]) -> Option<DraftDiff> {
        self.sets
            .get(index)
            .map(|set| set.diff_against_current(current))
    }

    /// Compare two revision baselines by index.
    pub fn diff_between(&self, base: usize, target: usize) -> Option<DraftDiff> {
        let base = self.sets.get(base)?;
        let target = self.sets.get(target)?;
        Some(base.diff_against(target))
    }
}

fn apply_set(
    set: &RevisionSet,
    current: &[ScriptBlock],
    current_segments: &[Segment<'_>],
    colors: &mut [Option<RevisionColor>],
) -> usize {
    let base_segments = split_segments(&set.baseline, base_scene_key);
    let mut used = vec![false; base_segments.len()];
    let mut deleted = 0usize;
    for segment in current_segments {
        let base_index = base_segments
            .iter()
            .enumerate()
            .find(|(i, b)| !used[*i] && b.heading == segment.heading)
            .map(|(i, _)| i);
        match base_index {
            Some(bi) => {
                used[bi] = true;
                let base = base_segments[bi];
                deleted += diff_range(
                    &set.baseline[base.start..base.end],
                    &current[segment.start..segment.end],
                    segment.start,
                    colors,
                    set.color,
                );
            }
            None => {
                for color in &mut colors[segment.start..segment.end] {
                    *color = Some(set.color);
                }
            }
        }
    }
    deleted
        + base_segments
            .iter()
            .enumerate()
            .filter(|(i, _)| !used[*i])
            .map(|(_, s)| s.end - s.start)
            .sum::<usize>()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SceneKey<'a> {
    Uuid(&'a str),
    Text(&'a str),
}

fn base_scene_key(b: &RevisionBlock) -> Option<SceneKey<'_>> {
    if b.element_type != ElementType::SceneHeading {
        return None;
    }
    Some(match b.kit_uuid.as_deref() {
        Some(uuid) if !uuid.is_empty() => SceneKey::Uuid(uuid),
        _ => SceneKey::Text(&b.text),
    })
}

fn current_scene_key(b: &ScriptBlock) -> Option<SceneKey<'_>> {
    if b.element_type != ElementType::SceneHeading {
        return None;
    }
    Some(match b.kit_uuid.as_deref() {
        Some(uuid) if !uuid.is_empty() => SceneKey::Uuid(uuid),
        _ => SceneKey::Text(&b.text),
    })
}

#[derive(Clone, Copy)]
struct Segment<'a> {
    heading: Option<SceneKey<'a>>,
    start: usize,
    end: usize,
}

fn split_segments<'a, T>(
    blocks: &'a [T],
    heading: impl Fn(&'a T) -> Option<SceneKey<'a>>,
) -> Vec<Segment<'a>> {
    let mut out = Vec::new();
    let Some(first) = blocks.iter().position(|b| heading(b).is_some()) else {
        if !blocks.is_empty() {
            out.push(Segment {
                heading: None,
                start: 0,
                end: blocks.len(),
            });
        }
        return out;
    };
    if first > 0 {
        out.push(Segment {
            heading: None,
            start: 0,
            end: first,
        });
    }
    let mut start = first;
    let mut current = heading(&blocks[first]);
    for (i, block) in blocks.iter().enumerate().skip(first + 1) {
        if let Some(h) = heading(block) {
            out.push(Segment {
                heading: current,
                start,
                end: i,
            });
            start = i;
            current = Some(h);
        }
    }
    out.push(Segment {
        heading: current,
        start,
        end: blocks.len(),
    });
    out
}

fn same_block(base: &RevisionBlock, current: &ScriptBlock) -> bool {
    base.element_type == current.element_type && base.text == current.text
}

fn diff_range(
    base: &[RevisionBlock],
    current: &[ScriptBlock],
    offset: usize,
    colors: &mut [Option<RevisionColor>],
    color: RevisionColor,
) -> usize {
    let mut prefix = 0usize;
    while prefix < base.len()
        && prefix < current.len()
        && same_block(&base[prefix], &current[prefix])
    {
        prefix += 1;
    }
    let mut suffix = 0usize;
    while suffix < base.len() - prefix
        && suffix < current.len() - prefix
        && same_block(
            &base[base.len() - 1 - suffix],
            &current[current.len() - 1 - suffix],
        )
    {
        suffix += 1;
    }

    let base = &base[prefix..base.len() - suffix];
    let current = &current[prefix..current.len() - suffix];
    if current.is_empty() {
        return base.len();
    }
    let out = &mut colors[offset + prefix..offset + prefix + current.len()];
    if base.is_empty() {
        out.fill(Some(color));
        return 0;
    }
    if base.len() * current.len() > LCS_CELL_LIMIT {
        out.fill(Some(color));
        return base.len();
    }

    let width = current.len() + 1;
    let mut dp = vec![0u32; (base.len() + 1) * width];
    for i in (0..base.len()).rev() {
        for j in (0..current.len()).rev() {
            dp[i * width + j] = if same_block(&base[i], &current[j]) {
                dp[(i + 1) * width + j + 1] + 1
            } else {
                dp[(i + 1) * width + j].max(dp[i * width + j + 1])
            };
        }
    }

    let mut matched = vec![false; current.len()];
    let (mut i, mut j) = (0usize, 0usize);
    while i < base.len() && j < current.len() {
        if same_block(&base[i], &current[j]) {
            matched[j] = true;
            i += 1;
            j += 1;
        } else if dp[(i + 1) * width + j] >= dp[i * width + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }

    let mut kept = 0usize;
    for (j, found) in matched.into_iter().enumerate() {
        if found {
            kept += 1;
        } else {
            out[j] = Some(color);
        }
    }
    base.len() - kept
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heading(text: &str) -> ScriptBlock {
        ScriptBlock::new(ElementType::SceneHeading, text.into())
    }

    fn action(text: &str) -> ScriptBlock {
        ScriptBlock::new(ElementType::Action, text.into())
    }

    fn character(text: &str) -> ScriptBlock {
        ScriptBlock::new(ElementType::Character, text.into())
    }

    fn dialogue(text: &str) -> ScriptBlock {
        ScriptBlock::new(ElementType::Dialogue, text.into())
    }

    fn sample() -> Vec<ScriptBlock> {
        vec![
            heading("INT. ROOM - DAY"),
            action("She waits."),
            character("ANNA"),
            dialogue("Hello."),
            heading("EXT. STREET - NIGHT"),
            action("Rain."),
        ]
    }

    fn start(h: &mut RevisionHistory, blocks: &[ScriptBlock]) -> RevisionColor {
        h.start(blocks, String::new(), String::new(), 0)
    }

    #[test]
    fn colors_advance_and_cycle() {
        let mut history = RevisionHistory::default();
        assert_eq!(history.next_color(), RevisionColor::Blue);
        assert_eq!(
            start(&mut history, &[]),
            RevisionColor::Blue,
            "first revision is Blue"
        );
        assert_eq!(history.next_color(), RevisionColor::Pink);
        for expected in [
            RevisionColor::Pink,
            RevisionColor::Yellow,
            RevisionColor::Green,
            RevisionColor::Goldenrod,
            RevisionColor::Buff,
            RevisionColor::Salmon,
            RevisionColor::Cherry,
            RevisionColor::Blue,
        ] {
            assert_eq!(start(&mut history, &[]), expected);
        }
    }

    #[test]
    fn text_edit_marks_only_its_block() {
        let mut blocks = sample();
        let mut history = RevisionHistory::default();
        start(&mut history, &blocks);
        blocks[1].text = "She leaves.".into();

        let marks = history.marks(&blocks);
        assert_eq!(marks.colors[1], Some(RevisionColor::Blue));
        assert_eq!(marks.changed, 1);
        assert!(marks
            .colors
            .iter()
            .enumerate()
            .all(|(i, c)| i == 1 || c.is_none()));
    }

    #[test]
    fn newer_revision_overrides_older_color() {
        let mut blocks = sample();
        let mut history = RevisionHistory::default();
        start(&mut history, &blocks);

        blocks[3].text = "Hello there.".into();
        start(&mut history, &blocks);

        blocks[1].text = "She leaves.".into();
        blocks[5].text = "Heavy rain.".into();

        let marks = history.marks(&blocks);
        assert_eq!(marks.colors[1], Some(RevisionColor::Pink));
        assert_eq!(marks.colors[3], Some(RevisionColor::Blue));
        assert_eq!(marks.colors[5], Some(RevisionColor::Pink));
        assert_eq!(marks.changed, 3);
    }

    #[test]
    fn latest_marks_show_only_the_newest_revision() {
        let mut blocks = sample();
        let mut history = RevisionHistory::default();
        start(&mut history, &blocks);
        blocks[1].text = "She leaves.".into();
        start(&mut history, &blocks);
        blocks[5].text = "Heavy rain.".into();

        let all = history.marks(&blocks);
        assert_eq!(all.colors[1], Some(RevisionColor::Blue));
        assert_eq!(all.colors[5], Some(RevisionColor::Pink));

        let latest = history.latest_marks(&blocks);
        assert_eq!(latest.colors[1], None);
        assert_eq!(latest.colors[5], Some(RevisionColor::Pink));
        assert_eq!(latest.changed, 1);
        assert_eq!(latest.deleted, 1);
    }

    #[test]
    fn page_counts_survive_json() {
        let mut history = RevisionHistory::default();
        history.start(&sample(), "Blue".into(), "today".into(), 342);
        assert_eq!(history.latest().unwrap().pages, 342);
        let json = history.to_json().unwrap();
        let restored = RevisionHistory::from_json(&json).unwrap();
        assert_eq!(restored.latest().unwrap().pages, 342);
    }

    #[test]
    fn inserted_scene_marks_new_blocks() {
        let mut blocks = sample();
        let mut history = RevisionHistory::default();
        start(&mut history, &blocks);

        blocks.insert(4, heading("INT. HALL - NIGHT"));
        blocks.insert(5, action("A door opens."));

        let marks = history.marks(&blocks);
        assert_eq!(marks.colors[4], Some(RevisionColor::Blue));
        assert_eq!(marks.colors[5], Some(RevisionColor::Blue));
        assert_eq!(marks.changed, 2);
        assert_eq!(marks.deleted, 0);
    }

    #[test]
    fn deleted_block_is_counted_without_a_marker() {
        let mut blocks = sample();
        let mut history = RevisionHistory::default();
        start(&mut history, &blocks);
        blocks.remove(5);

        let marks = history.marks(&blocks);
        assert_eq!(marks.changed, 0);
        assert_eq!(marks.deleted, 1);
    }

    #[test]
    fn reordering_scenes_does_not_mark_them() {
        let mut blocks = sample();
        let mut history = RevisionHistory::default();
        start(&mut history, &blocks);

        let second: Vec<ScriptBlock> = blocks.drain(4..6).collect();
        blocks.splice(0..0, second);

        let marks = history.marks(&blocks);
        assert_eq!(marks.changed, 0);
        assert_eq!(marks.deleted, 0);
    }

    #[test]
    fn renamed_heading_marks_only_heading() {
        let mut blocks = sample();
        blocks[0].kit_uuid = Some("{scene-1}".into());
        blocks[4].kit_uuid = Some("{scene-2}".into());
        let mut history = RevisionHistory::default();
        start(&mut history, &blocks);
        blocks[0].text = "INT. OTHER ROOM - DAY".into();

        let marks = history.marks(&blocks);
        assert_eq!(marks.colors[0], Some(RevisionColor::Blue));
        assert!(marks.colors[1..].iter().all(Option::is_none));
        assert_eq!(marks.changed, 1);
        assert_eq!(marks.deleted, 1);
    }

    #[test]
    fn five_hundred_page_diff_stays_under_eight_milliseconds() {
        use std::time::{Duration, Instant};
        // 500 scenes x 56 rows is roughly a 500-page feature at the
        // Russian template's ~55-line page budget.
        let mut blocks = Vec::with_capacity(28_000);
        for scene in 0..500 {
            blocks.push(heading(&format!("INT. ROOM {scene} - DAY")));
            for line in 0..55 {
                blocks.push(action(&format!("Action line {scene}.{line}.")));
            }
        }
        let mut history = RevisionHistory::default();
        start(&mut history, &blocks);
        for scene in (0..500).step_by(7) {
            let base = scene * 56;
            blocks[base + 3].text.push_str(" Revised.");
            blocks[base + 20].text.push_str(" Revised.");
        }
        let _ = history.marks(&blocks);
        let started = Instant::now();
        let marks = history.marks(&blocks);
        let elapsed = started.elapsed();
        assert!(marks.changed >= 140, "every sampled scene is marked");
        assert!(
            elapsed < Duration::from_millis(8),
            "500-page revision diff took {elapsed:?}"
        );
    }

    #[test]
    fn json_roundtrip_keeps_baselines() {
        let blocks = sample();
        let mut history = RevisionHistory::default();
        start(&mut history, &blocks);
        let json = history.to_json().expect("serialize");
        assert_eq!(
            RevisionHistory::from_json(&json).expect("deserialize"),
            history
        );
    }

    // -- DraftDiff ---------------------------------------------------------

    fn set_from(blocks: &[ScriptBlock]) -> RevisionSet {
        RevisionSet {
            color: RevisionColor::Blue,
            name: "Blue".into(),
            started: "today".into(),
            pages: 0,
            baseline: blocks.iter().map(RevisionBlock::from).collect(),
        }
    }

    fn old_set_from(blocks: &[ScriptBlock]) -> RevisionSet {
        let mut set = set_from(blocks);
        for block in &mut set.baseline {
            block.block_id = None;
        }
        set
    }

    fn at(diff: &DraftDiff, current_block: usize) -> &DiffEntry {
        diff.entries
            .iter()
            .find(|e| e.current_block == Some(current_block))
            .unwrap_or_else(|| panic!("no diff entry for current block {current_block}"))
    }

    #[test]
    fn diff_against_self_is_clean_and_ordered() {
        let blocks = sample();
        let diff = set_from(&blocks).diff_against_current(&blocks);
        assert_eq!(diff.entries.len(), blocks.len());
        assert!(diff.entries.iter().all(|e| e.kind == DiffKind::Unchanged));
        assert_eq!((diff.added, diff.removed, diff.changed), (0, 0, 0));
        assert!(diff.scenes_changed.is_empty());
        assert!(!diff.has_changes());
        assert_eq!(
            diff.entries
                .iter()
                .map(|e| e.current_block.unwrap())
                .collect::<Vec<_>>(),
            (0..blocks.len()).collect::<Vec<_>>()
        );
        assert_eq!(diff.entries[0].scene_index, 0);
        assert_eq!(diff.entries[4].scene_index, 1);
        assert_eq!(diff.entries[4].scene_heading, "EXT. STREET - NIGHT");
    }

    #[test]
    fn single_edit_is_changed_in_the_right_scene() {
        let mut blocks = sample();
        let set = set_from(&blocks);
        blocks[3].text = "Hello there.".into();

        let diff = set.diff_against_current(&blocks);
        assert_eq!((diff.added, diff.removed, diff.changed), (0, 0, 1));
        assert_eq!(diff.scenes_changed, vec![0]);
        let entry = at(&diff, 3);
        assert_eq!(entry.kind, DiffKind::Changed);
        assert_eq!(entry.scene_index, 0);
        assert_eq!(entry.scene_heading, "INT. ROOM - DAY");
        assert_eq!(entry.base.as_ref().unwrap().text, "Hello.");
        assert_eq!(entry.current.as_ref().unwrap().text, "Hello there.");
        assert!(at(&diff, 1).kind == DiffKind::Unchanged);
    }

    #[test]
    fn added_and_removed_blocks_keep_global_indices() {
        let mut blocks = sample();
        let set = set_from(&blocks);
        blocks.insert(2, action("A phone rings."));
        blocks.remove(6); // The original `Rain.` block.

        let diff = set.diff_against_current(&blocks);
        assert_eq!((diff.added, diff.removed, diff.changed), (1, 1, 0));
        let added = at(&diff, 2);
        assert_eq!(added.kind, DiffKind::Added);
        assert_eq!(added.current.as_ref().unwrap().text, "A phone rings.");
        assert!(added.base.is_none());

        let removed = diff
            .entries
            .iter()
            .find(|e| e.kind == DiffKind::Removed)
            .expect("removed block");
        assert_eq!(removed.base.as_ref().unwrap().text, "Rain.");
        assert!(removed.current.is_none());
        assert_eq!(removed.current_block, None);
        assert_eq!(removed.scene_index, 1);
        assert_eq!(removed.scene_heading, "EXT. STREET - NIGHT");
    }

    #[test]
    fn added_scene_reports_every_block_as_added() {
        let mut blocks = sample();
        let set = set_from(&blocks);
        blocks.insert(4, heading("INT. HALL - NIGHT"));
        blocks.insert(5, action("A door opens."));

        let diff = set.diff_against_current(&blocks);
        assert_eq!(diff.added, 2);
        assert_eq!(diff.removed, 0);
        assert_eq!(diff.changed, 0);
        assert_eq!(diff.scenes_changed, vec![1]);
        let added: Vec<_> = diff
            .entries
            .iter()
            .filter(|e| e.kind == DiffKind::Added)
            .collect();
        assert_eq!(added.len(), 2);
        assert!(added.iter().all(|e| e.scene_index == 1));
        assert!(added.iter().all(|e| e.scene_heading == "INT. HALL - NIGHT"));
        assert_eq!(
            added.iter().map(|e| e.current_block).collect::<Vec<_>>(),
            vec![Some(4), Some(5)]
        );
    }

    #[test]
    fn removed_scene_reports_blocks_at_the_target_insertion_point() {
        let mut blocks = sample();
        let set = set_from(&blocks);
        blocks.truncate(4);

        let diff = set.diff_against_current(&blocks);
        assert_eq!((diff.added, diff.removed, diff.changed), (0, 2, 0));
        assert_eq!(diff.entries.len(), 6);
        assert_eq!(diff.entries[4].kind, DiffKind::Removed);
        assert_eq!(diff.entries[4].scene_index, 1);
        assert_eq!(diff.entries[4].scene_heading, "EXT. STREET - NIGHT");
        assert_eq!(
            diff.entries[4].base.as_ref().unwrap().text,
            "EXT. STREET - NIGHT"
        );
        assert_eq!(diff.entries[5].base.as_ref().unwrap().text, "Rain.");
        assert_eq!(diff.scenes_changed, vec![1]);
    }

    #[test]
    fn removed_first_scene_is_emitted_before_the_surviving_scene() {
        let mut blocks = sample();
        let set = set_from(&blocks);
        blocks.drain(0..4);

        let diff = set.diff_against_current(&blocks);
        assert_eq!(diff.removed, 4);
        assert_eq!(diff.entries[0].kind, DiffKind::Removed);
        assert_eq!(diff.entries[0].scene_index, 0);
        assert_eq!(diff.entries[0].scene_heading, "INT. ROOM - DAY");
        assert_eq!(diff.entries[4].kind, DiffKind::Unchanged);
        assert_eq!(diff.entries[4].scene_index, 0);
        assert_eq!(diff.entries[4].scene_heading, "EXT. STREET - NIGHT");
    }

    fn three_scenes() -> Vec<ScriptBlock> {
        vec![
            heading("INT. ONE - DAY"),
            action("First."),
            heading("INT. TWO - NIGHT"),
            action("Second."),
            heading("INT. THREE - DAY"),
            action("Third."),
        ]
    }

    #[test]
    fn reordered_scenes_are_not_reported_as_rewrites() {
        let original = three_scenes();
        let set = set_from(&original);
        let mut blocks = original;
        let third: Vec<_> = blocks.drain(4..6).collect();
        blocks.splice(0..0, third);

        let diff = set.diff_against_current(&blocks);
        assert_eq!((diff.added, diff.removed, diff.changed), (0, 0, 0));
        assert!(diff.entries.iter().all(|e| e.kind == DiffKind::Unchanged));
        assert_eq!(
            diff.entries
                .iter()
                .filter(|e| e.current_block.is_some())
                .map(|e| e.scene_index)
                .collect::<Vec<_>>(),
            vec![0, 0, 1, 1, 2, 2]
        );
        assert_eq!(diff.entries[0].scene_heading, "INT. THREE - DAY");
    }

    #[test]
    fn old_baselines_match_reordered_scenes_by_heading() {
        let original = three_scenes();
        let set = old_set_from(&original);
        let mut blocks = original;
        let third: Vec<_> = blocks.drain(4..6).collect();
        blocks.splice(0..0, third);

        let diff = set.diff_against_current(&blocks);
        assert_eq!((diff.added, diff.removed, diff.changed), (0, 0, 0));
    }

    #[test]
    fn renamed_heading_is_one_change() {
        let mut blocks = sample();
        let set = set_from(&blocks);
        blocks[0].text = "INT. OTHER ROOM - DAY".into();

        let diff = set.diff_against_current(&blocks);
        assert_eq!((diff.added, diff.removed, diff.changed), (0, 0, 1));
        let entry = at(&diff, 0);
        assert_eq!(entry.kind, DiffKind::Changed);
        assert_eq!(entry.base.as_ref().unwrap().text, "INT. ROOM - DAY");
        assert_eq!(
            entry.current.as_ref().unwrap().text,
            "INT. OTHER ROOM - DAY"
        );
        assert_eq!(entry.scene_heading, "INT. OTHER ROOM - DAY");
    }

    #[test]
    fn old_baseline_matches_renamed_scene_by_body() {
        let mut blocks = sample();
        let set = old_set_from(&blocks);
        blocks[0].text = "INT. OTHER ROOM - DAY".into();

        let diff = set.diff_against_current(&blocks);
        assert_eq!(diff.changed, 1);
        assert_eq!(at(&diff, 0).kind, DiffKind::Changed);
        assert_eq!(at(&diff, 1).kind, DiffKind::Unchanged);
    }

    #[test]
    fn duplicate_headings_keep_distinct_scene_identity() {
        let mut blocks = vec![
            heading("INT. ROOM - DAY"),
            action("First scene."),
            heading("INT. ROOM - DAY"),
            action("Second scene."),
        ];
        let set = set_from(&blocks);
        blocks[1].text = "First scene, edited.".into();

        let diff = set.diff_against_current(&blocks);
        assert_eq!(diff.changed, 1);
        assert_eq!(at(&diff, 1).kind, DiffKind::Changed);
        assert_eq!(at(&diff, 3).kind, DiffKind::Unchanged);

        // Reordering the two identically-named scenes must not swap their
        // content identity when stable ids exist.
        let mut reordered = set
            .baseline
            .iter()
            .map(revision_to_script)
            .collect::<Vec<_>>();
        let second: Vec<_> = reordered.drain(2..4).collect();
        reordered.splice(0..0, second);
        let diff = set.diff_against_current(&reordered);
        assert_eq!((diff.added, diff.removed, diff.changed), (0, 0, 0));
    }

    #[test]
    fn cyrillic_text_compares_cleanly() {
        let mut blocks = vec![
            heading("ИНТ. КУХНЯ – ДЕН"),
            action("Тя чака до прозореца."),
            character("АННА"),
            dialogue("Здравей."),
        ];
        let set = set_from(&blocks);
        assert!(!set.diff_against_current(&blocks).has_changes());
        blocks[3].text = "Здравей отново.".into();

        let diff = set.diff_against_current(&blocks);
        assert_eq!(diff.changed, 1);
        assert_eq!(
            at(&diff, 3).current.as_ref().unwrap().text,
            "Здравей отново."
        );
    }

    #[test]
    fn empty_drafts_compare_without_special_cases() {
        let empty: Vec<ScriptBlock> = Vec::new();
        let empty_set = set_from(&empty);
        let diff = empty_set.diff_against_current(&empty);
        assert!(diff.entries.is_empty());
        assert!(!diff.has_changes());

        let blocks = sample();
        let added = empty_set.diff_against_current(&blocks);
        assert_eq!((added.added, added.removed, added.changed), (6, 0, 0));
        assert_eq!(added.entries.len(), 6);
        assert_eq!(
            added
                .entries
                .iter()
                .map(|e| e.scene_index)
                .collect::<Vec<_>>(),
            vec![0, 0, 0, 0, 1, 1]
        );

        let removed = set_from(&blocks).diff_against_current(&empty);
        assert_eq!((removed.added, removed.removed, removed.changed), (0, 6, 0));
        assert!(removed.entries.iter().all(|e| e.kind == DiffKind::Removed));
        assert!(removed.entries.iter().all(|e| e.scene_index == 0));
        assert_eq!(removed.scenes_changed, vec![0]);
    }

    #[test]
    fn preamble_blocks_are_grouped_with_the_first_scene() {
        let mut blocks = vec![
            ScriptBlock::new(ElementType::NonPrintable, "TITLE CARD".into()),
            action("Opening image."),
            heading("INT. ROOM - DAY"),
            action("She waits."),
        ];
        let set = set_from(&blocks);
        blocks[1].text = "Opening image, closer.".into();

        let diff = set.diff_against_current(&blocks);
        assert_eq!(diff.changed, 1);
        assert_eq!(at(&diff, 0).kind, DiffKind::Unchanged);
        let changed = at(&diff, 1);
        assert_eq!(changed.kind, DiffKind::Changed);
        assert_eq!(changed.scene_index, 0);
        assert_eq!(changed.scene_heading, "INT. ROOM - DAY");
        assert_eq!(at(&diff, 3).kind, DiffKind::Unchanged);
    }

    #[test]
    fn baseline_to_baseline_diff_and_history_helpers_agree() {
        let base_blocks = sample();
        let mut target_blocks = base_blocks.clone();
        target_blocks[3].text = "Hello there.".into();
        target_blocks.push(heading("INT. HALL - NIGHT"));
        target_blocks.push(action("A door opens."));

        let base = set_from(&base_blocks);
        let target = set_from(&target_blocks);
        let diff = DraftDiff::between(&base, &target);
        assert_eq!((diff.added, diff.removed, diff.changed), (2, 0, 1));
        assert_eq!(diff.scenes_changed, vec![0, 2]);
        assert_eq!(diff.entries[0].current_block, Some(0));
        assert_eq!(diff.entries[0].scene_heading, "INT. ROOM - DAY");
        assert_eq!(
            diff.entries.last().unwrap().scene_heading,
            "INT. HALL - NIGHT"
        );

        let history = RevisionHistory {
            sets: vec![base, target],
        };
        assert_eq!(history.diff_between(0, 1).as_ref(), Some(&diff));
        assert!(history.diff_between(0, 2).is_none());
        assert_eq!(
            history.diff_against_current(0, &target_blocks).unwrap(),
            diff
        );
    }

    #[test]
    fn removed_middle_scene_is_inserted_between_target_scenes() {
        let original = three_scenes();
        let set = set_from(&original);
        let mut blocks = original;
        blocks.drain(2..4);

        let diff = set.diff_against_current(&blocks);
        assert_eq!(diff.removed, 2);
        assert_eq!(diff.entries[2].kind, DiffKind::Removed);
        assert_eq!(diff.entries[2].scene_index, 1);
        assert_eq!(diff.entries[2].scene_heading, "INT. TWO - NIGHT");
        assert_eq!(diff.entries[4].kind, DiffKind::Unchanged);
        assert_eq!(diff.entries[4].scene_index, 1);
        assert_eq!(diff.entries[4].scene_heading, "INT. THREE - DAY");
    }

    #[test]
    fn old_json_without_block_id_still_diffs() {
        let json = r#"{
            "color": "blue",
            "name": "Blue",
            "started": "2026-10-01",
            "pages": 1,
            "baseline": [
                {"element_type": "SceneHeading", "text": "INT. ROOM - DAY", "kit_uuid": "{scene-1}"},
                {"element_type": "Action", "text": "She waits."}
            ]
        }"#;
        let set: RevisionSet = serde_json::from_str(json).expect("old baseline JSON");
        assert!(set.baseline.iter().all(|b| b.block_id.is_none()));

        let blocks = vec![heading("INT. ROOM - DAY"), action("She waits.")];
        let diff = set.diff_against_current(&blocks);
        assert!(!diff.has_changes());
        assert_eq!(diff.entries.len(), 2);

        let encoded = serde_json::to_string(&set_from(&blocks)).unwrap();
        assert!(encoded.contains("block_id"), "new baselines store ids");
    }

    #[test]
    fn diff_results_are_deterministic() {
        let original = vec![
            heading("INT. ROOM - DAY"),
            action("One."),
            heading("INT. ROOM - DAY"),
            action("Two."),
            heading("EXT. STREET - NIGHT"),
            action("Three."),
        ];
        let set = old_set_from(&original);
        let mut blocks = original;
        blocks[1].text = "One, edited.".into();
        blocks.swap(0, 4);

        let first = set.diff_against_current(&blocks);
        let second = set.diff_against_current(&blocks);
        assert_eq!(first, second);
    }

    #[test]
    fn five_hundred_page_compare_stays_under_twenty_milliseconds() {
        use std::time::{Duration, Instant};
        let mut blocks = Vec::with_capacity(28_000);
        for scene in 0..500 {
            blocks.push(heading(&format!("INT. ROOM {scene} - DAY")));
            for line in 0..55 {
                blocks.push(action(&format!("Action line {scene}.{line}.")));
            }
        }
        let mut history = RevisionHistory::default();
        start(&mut history, &blocks);
        let set = history.latest().unwrap();
        for scene in (0..500).step_by(7) {
            let base = scene * 56;
            blocks[base + 3].text.push_str(" Revised.");
            blocks[base + 20].text.push_str(" Revised.");
        }

        let _ = set.diff_against_current(&blocks);
        let started = Instant::now();
        let diff = set.diff_against_current(&blocks);
        let elapsed = started.elapsed();
        assert_eq!(diff.entries.len(), blocks.len());
        assert!(diff.changed >= 140, "sampled edits are reported");
        assert_eq!(diff.removed, 0);
        assert_eq!(diff.added, 0);
        assert!(
            elapsed < Duration::from_millis(20),
            "500-page draft compare took {elapsed:?}"
        );
    }
}
