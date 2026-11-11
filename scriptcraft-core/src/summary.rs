//! Revision summary: scene-grouped counts derived from a [`DraftDiff`], plus
//! the deterministic CSV table the app exports.
//!
//! The summary is deliberately derived from `DraftDiff` rather than walking
//! the script again: the compare view and the summary then report the same
//! numbers by construction.

use crate::revision::{DiffKind, DraftDiff, RevisionColor, RevisionSet};

/// UTF-8 byte-order mark. Excel on Windows reads UTF-8 CSV correctly only
/// with it; Numbers accepts it too.
const BOM: &str = "\u{feff}";

/// One scene (or the preamble) with at least one changed block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneRevisionSummary {
    /// Target scene index; removed scenes use their insertion index.
    pub scene_index: usize,
    /// Heading as it reads in the target draft (or the removed scene).
    pub heading: String,
    pub added: usize,
    pub removed: usize,
    pub changed: usize,
}

impl SceneRevisionSummary {
    pub fn total(&self) -> usize {
        self.added + self.removed + self.changed
    }
}

/// What changed between a revision baseline and the current draft (or another
/// baseline), grouped by scene and ready for the screen, CSV and print.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevisionSummary {
    pub color: RevisionColor,
    pub name: String,
    /// Date half of the revision's start timestamp (`dd.mm.yyyy` for sessions
    /// started here; imported timestamps stay readable).
    pub date: String,
    /// Page count when the revision started.
    pub base_pages: usize,
    /// Page count of the draft being summarized.
    pub current_pages: usize,
    pub added: usize,
    pub removed: usize,
    pub changed: usize,
    /// Number of scenes in the target draft, used to label removed scenes
    /// that sat after the last surviving scene.
    pub target_scenes: usize,
    pub scenes: Vec<SceneRevisionSummary>,
}

impl RevisionSummary {
    /// Summarize `diff` for revision `set`. `current_pages` and
    /// `target_scenes` describe the target draft (for a baseline-to-baseline
    /// diff, pass the target set's values).
    pub fn from_diff(
        set: &RevisionSet,
        diff: &DraftDiff,
        current_pages: usize,
        target_scenes: usize,
    ) -> Self {
        let mut scenes: Vec<SceneRevisionSummary> = Vec::new();
        for entry in diff.entries.iter().filter(|e| e.is_change()) {
            let same_scene = scenes.last().is_some_and(|scene| {
                scene.scene_index == entry.scene_index && scene.heading == entry.scene_heading
            });
            if !same_scene {
                scenes.push(SceneRevisionSummary {
                    scene_index: entry.scene_index,
                    heading: entry.scene_heading.clone(),
                    added: 0,
                    removed: 0,
                    changed: 0,
                });
            }
            let scene = scenes.last_mut().expect("scene just pushed");
            match entry.kind {
                DiffKind::Added => scene.added += 1,
                DiffKind::Removed => scene.removed += 1,
                DiffKind::Changed => scene.changed += 1,
                DiffKind::Unchanged => {}
            }
        }
        let date = set
            .started
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string();
        Self {
            color: set.color,
            name: set.name.clone(),
            date,
            base_pages: set.pages,
            current_pages,
            added: diff.added,
            removed: diff.removed,
            changed: diff.changed,
            target_scenes,
            scenes,
        }
    }

    /// Current pages minus baseline pages; negative when the revision cut
    /// pages.
    pub fn pages_delta(&self) -> i64 {
        self.current_pages as i64 - self.base_pages as i64
    }

    pub fn total(&self) -> usize {
        self.added + self.removed + self.changed
    }

    pub fn has_changes(&self) -> bool {
        self.total() > 0
    }

    /// `Scene 3` / `After scene 21` for a changed-scene row.
    pub fn scene_label(&self, scene: &SceneRevisionSummary) -> String {
        if scene.scene_index < self.target_scenes {
            format!("Scene {}", scene.scene_index + 1)
        } else {
            format!("After scene {}", self.target_scenes)
        }
    }

    /// Stable UTF-8 CSV: one row per changed scene, then a TOTAL row.
    /// Metadata columns repeat on every row so the file is a single table.
    pub fn csv(&self) -> String {
        let mut out = String::from(BOM);
        out.push_str(
            "Revision,Colour,Date,Base pages,Current pages,Pages delta,Scene,Heading,\
             Added blocks,Removed blocks,Changed blocks\n",
        );
        let meta = format!(
            "{},{},{},{},{},{}",
            escape(&self.name),
            self.color.label(),
            escape(&self.date),
            self.base_pages,
            self.current_pages,
            self.pages_delta()
        );
        for scene in &self.scenes {
            out.push_str(&meta);
            out.push(',');
            out.push_str(&escape(&self.scene_label(scene)));
            out.push(',');
            out.push_str(&escape(&scene.heading));
            out.push_str(&format!(
                ",{},{},{}\n",
                scene.added, scene.removed, scene.changed
            ));
        }
        out.push_str(&format!(
            "TOTAL,,,{},{},{},,,{},{},{}\n",
            self.base_pages,
            self.current_pages,
            self.pages_delta(),
            self.added,
            self.removed,
            self.changed
        ));
        out
    }
}

/// RFC 4180 quoting: commas, quotes and newlines force quotes; quotes double.
fn escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ElementType, Project, ScriptBlock};
    use crate::revision::RevisionHistory;

    fn block(kind: ElementType, text: &str) -> ScriptBlock {
        ScriptBlock::new(kind, text.to_string())
    }

    fn project() -> Project {
        Project {
            title: "Summary Test".into(),
            blocks: vec![
                block(ElementType::SceneHeading, "INT. КАФЕ - ДЕН"),
                block(ElementType::Action, "Иван чете."),
                block(ElementType::SceneHeading, "EXT. ПАРК - ДЕН"),
                block(ElementType::Action, "Мария тича."),
            ],
            scene_meta: Vec::new(),
        }
    }

    fn started(project: &mut Project, name: &str) -> (RevisionHistory, RevisionSet) {
        let mut history = RevisionHistory::default();
        history.start(&project.blocks, name.into(), "05.11.2026 10:00".into(), 34);
        let set = history.latest().unwrap().clone();
        (history, set)
    }

    #[test]
    fn counts_match_the_diff_and_group_by_scene() {
        let mut base = project();
        let (_, set) = started(&mut base, "Blue");

        // Scene 1: change the action. Scene 2: remove the old action and add
        // a new scene at the end.
        let current = vec![
            base.blocks[0].clone(),
            block(ElementType::Action, "Иван пише."),
            base.blocks[2].clone(),
            block(ElementType::SceneHeading, "INT. ВЛАК - ДЕН"),
            block(ElementType::Action, "Мария пътува."),
        ];
        let diff = DraftDiff::against_current(&set, &current);
        let summary = RevisionSummary::from_diff(&set, &diff, 36, 3);

        assert_eq!(summary.added, diff.added);
        assert_eq!(summary.removed, diff.removed);
        assert_eq!(summary.changed, diff.changed);
        assert_eq!(summary.total(), diff.added + diff.removed + diff.changed);
        assert_eq!(summary.color, RevisionColor::Blue);
        assert_eq!(summary.name, "Blue");
        assert_eq!(summary.date, "05.11.2026");
        assert_eq!(summary.base_pages, 34);
        assert_eq!(summary.current_pages, 36);
        assert_eq!(summary.pages_delta(), 2);
        assert!(summary.has_changes());

        let totals = summary.scenes.iter().fold((0, 0, 0), |acc, scene| {
            (
                acc.0 + scene.added,
                acc.1 + scene.removed,
                acc.2 + scene.changed,
            )
        });
        assert_eq!(totals, (diff.added, diff.removed, diff.changed));
        assert_eq!(
            summary.scenes[0].heading, "INT. КАФЕ - ДЕН",
            "the first changed scene keeps its heading"
        );
        assert_eq!(summary.scenes[0].changed, 1);
        assert_eq!(summary.scenes[1].heading, "EXT. ПАРК - ДЕН");
        assert_eq!(summary.scenes[1].removed, 1);
        assert_eq!(summary.scenes[2].heading, "INT. ВЛАК - ДЕН");
        assert_eq!(summary.scenes[2].added, 2);
        assert_eq!(summary.scene_label(&summary.scenes[2]), "Scene 3");
    }

    #[test]
    fn a_removed_scene_is_listed_with_its_heading() {
        let mut base = project();
        let (_, set) = started(&mut base, "Blue");
        // Drop the second scene entirely.
        let current = vec![base.blocks[0].clone(), base.blocks[1].clone()];
        let diff = DraftDiff::against_current(&set, &current);
        let summary = RevisionSummary::from_diff(&set, &diff, 30, 1);

        assert_eq!(summary.removed, 2);
        assert_eq!(summary.scenes.len(), 1);
        assert_eq!(summary.scenes[0].heading, "EXT. ПАРК - ДЕН");
        assert_eq!(summary.scenes[0].removed, 2);
        assert_eq!(
            summary.scene_label(&summary.scenes[0]),
            "After scene 1",
            "a scene removed after the last survivor is labelled as such"
        );
    }

    #[test]
    fn csv_is_deterministic_and_cyrillic_safe() {
        let mut base = project();
        let (_, set) = started(&mut base, "Синя");
        let mut current = vec![base.blocks[0].clone(), base.blocks[1].clone()];
        current[0].text = "INT. КАФЕ, \"ДЕН\"".into();
        current[1].text = "Иван пише.".into();
        let diff = DraftDiff::against_current(&set, &current);
        let summary = RevisionSummary::from_diff(&set, &diff, 34, 1);

        let csv = summary.csv();
        assert_eq!(csv, summary.csv(), "CSV must be deterministic");
        assert!(csv.starts_with(BOM), "Excel needs the UTF-8 BOM");
        assert!(csv.contains("Revision,Colour,Date,Base pages"));
        assert!(csv.contains("Синя"));
        assert!(csv.contains("TOTAL,,,34,34,0,,,0,2,2"));
        assert!(
            csv.contains("\"INT. КАФЕ, \"\"ДЕН\"\"\""),
            "commas and quotes must be escaped: {csv}"
        );
    }

    #[test]
    fn an_empty_revision_is_a_zero_row_not_an_error() {
        let mut base = project();
        let (_, set) = started(&mut base, "Blue");
        let diff = DraftDiff::against_current(&set, &base.blocks);
        let summary = RevisionSummary::from_diff(&set, &diff, 34, 2);

        assert!(!summary.has_changes());
        assert_eq!(summary.total(), 0);
        assert!(summary.scenes.is_empty());
        let csv = summary.csv();
        assert!(csv.contains("TOTAL,,,34,34,0,,,0,0,0"), "{csv}");
        assert_eq!(csv.lines().count(), 2, "header + totals only");
    }
}
