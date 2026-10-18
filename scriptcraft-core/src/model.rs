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
#[derive(Clone, Debug, Serialize, Deserialize)]
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
    pub heading: String,
    pub kit_uuid: Option<String>,
    /// Indices into Project::blocks (includes the heading at [0]).
    pub blocks: Vec<usize>,
}

/// Whole screenplay project.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Project {
    pub title: String,
    pub blocks: Vec<ScriptBlock>,
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

    pub fn characters(&self) -> Vec<String> {
        let mut out = Vec::new();
        for b in &self.blocks {
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
        self.blocks
            .iter()
            .filter(|b| b.element_type.is_printable())
            .map(|b| b.text.split_whitespace().count())
            .sum()
    }

    /// Estimated screen time, calibrated against KIT Scenarist chronometry
    /// on A95 (total 17:55 predicted vs 17:54 shown; mean scene error 5.7s):
    /// action-ish words cost ~0.22s, dialogue-ish words ~0.27s.
    pub fn duration_secs(&self) -> f64 {
        duration_range(&self.blocks, self.blocks.len())
    }

    pub fn scene_duration_secs(&self, scene: Option<&Scene>) -> f64 {
        match scene {
            Some(s) => {
                let idx: Vec<usize> = s.blocks.clone();
                duration_indexed(&self.blocks, &idx)
            }
            None => self.duration_secs(),
        }
    }

    /// Estimated screen time of blocks before (and including) `upto_bi`,
    /// for the toolbar's "current position" counter.
    pub fn duration_upto(&self, upto_bi: usize) -> f64 {
        duration_range(
            &self.blocks,
            upto_bi.saturating_add(1).min(self.blocks.len()),
        )
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

fn duration_range(blocks: &[ScriptBlock], end_exclusive: usize) -> f64 {
    let end = end_exclusive.min(blocks.len());
    let mut a = 0usize;
    let mut d = 0usize;
    for b in &blocks[..end] {
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

fn duration_indexed(blocks: &[ScriptBlock], idx: &[usize]) -> f64 {
    let mut a = 0usize;
    let mut d = 0usize;
    for &i in idx {
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
        let mut s = Self {
            blocks: p.blocks.len(),
            words: p.word_count(),
            ..Default::default()
        };
        for b in &p.blocks {
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
