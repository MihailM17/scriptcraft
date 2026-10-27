use serde::{Deserialize, Serialize};

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
}

impl From<&ScriptBlock> for RevisionBlock {
    fn from(b: &ScriptBlock) -> Self {
        Self {
            element_type: b.element_type,
            text: b.text.clone(),
            kit_uuid: b.kit_uuid.clone(),
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
}
