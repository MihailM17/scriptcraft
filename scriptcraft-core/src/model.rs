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

/// One screenplay block.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScriptBlock {
    pub id: Uuid,
    /// Only scene headings carry the KIT uuid; others get a fresh v4.
    pub kit_uuid: Option<String>,
    pub element_type: ElementType,
    pub text: String,
}

impl ScriptBlock {
    pub fn new(element_type: ElementType, text: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            kit_uuid: None,
            element_type,
            text,
        }
    }
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
