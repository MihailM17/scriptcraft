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
use crate::revision::RevisionColor;
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

/// Curated 20-color studio palette (10 vivid + 10 rich darker variants) for
/// distinguishing camera setups on dark/light UI surfaces and white PDF pages.
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
    // Darker variants (matched 1-to-1 underneath the 10 primary hues)
    Navy,
    Forest,
    Ochre,
    Burgundy,
    Plum,
    DeepTeal,
    Rust,
    Midnight,
    Moss,
    Wine,
}

impl ShotColor {
    pub const ALL_STANDARD: [Self; 10] = [
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

    pub const ALL_DARK: [Self; 10] = [
        Self::Navy,
        Self::Forest,
        Self::Ochre,
        Self::Burgundy,
        Self::Plum,
        Self::DeepTeal,
        Self::Rust,
        Self::Midnight,
        Self::Moss,
        Self::Wine,
    ];

    pub const ALL: [Self; 20] = [
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
        Self::Navy,
        Self::Forest,
        Self::Ochre,
        Self::Burgundy,
        Self::Plum,
        Self::DeepTeal,
        Self::Rust,
        Self::Midnight,
        Self::Moss,
        Self::Wine,
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
            Self::Navy => "Dark Cobalt",
            Self::Forest => "Dark Emerald",
            Self::Ochre => "Dark Amber",
            Self::Burgundy => "Dark Crimson",
            Self::Plum => "Dark Violet",
            Self::DeepTeal => "Dark Teal",
            Self::Rust => "Dark Coral",
            Self::Midnight => "Dark Indigo",
            Self::Moss => "Dark Olive",
            Self::Wine => "Dark Magenta",
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
            // Rich darker variants
            Self::Navy => (30, 64, 175),
            Self::Forest => (6, 105, 76),
            Self::Ochre => (154, 92, 8),
            Self::Burgundy => (153, 27, 27),
            Self::Plum => (88, 28, 135),
            Self::DeepTeal => (15, 104, 96),
            Self::Rust => (164, 58, 12),
            Self::Midnight => (55, 48, 163),
            Self::Moss => (77, 124, 15),
            Self::Wine => (157, 23, 77),
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
    /// Optional storyboard or reference frame image stored as base64-encoded PNG/JPEG.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_image: Option<String>,
}

impl Shot {
    /// Decode the attached storyboard/reference image bytes (if any).
    pub fn reference_image_bytes(&self) -> Option<Vec<u8>> {
        self.reference_image.as_deref().and_then(decode_base64)
    }

    /// Store or clear the attached storyboard/reference image bytes.
    pub fn set_reference_image_bytes(&mut self, bytes: Option<&[u8]>) {
        self.reference_image = bytes.filter(|b| !b.is_empty()).map(encode_base64);
    }
}

/// Standard architectural, furniture, basic geometry, & lighting prop types for
/// the 2D top-down floor plan.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PropKind {
    #[default]
    Wall,
    Door,
    Window,
    Table,
    Chair,
    Bed,
    Car,
    Square,
    Circle,
    Triangle,
    Light,
    Custom,
}

impl PropKind {
    pub const ALL: [Self; 12] = [
        Self::Wall,
        Self::Door,
        Self::Window,
        Self::Table,
        Self::Chair,
        Self::Bed,
        Self::Car,
        Self::Square,
        Self::Circle,
        Self::Triangle,
        Self::Light,
        Self::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Wall => "Wall",
            Self::Door => "Door",
            Self::Window => "Window",
            Self::Table => "Table",
            Self::Chair => "Chair",
            Self::Bed => "Bed",
            Self::Car => "Car",
            Self::Square => "Square",
            Self::Circle => "Circle",
            Self::Triangle => "Triangle",
            Self::Light => "Light",
            Self::Custom => "Prop",
        }
    }

    /// Default footprint `(width_m, height_m)` in meters.
    pub fn default_size(self) -> (f32, f32) {
        match self {
            Self::Wall => (4.0, 0.22),
            Self::Door => (1.0, 0.22),
            Self::Window => (1.6, 0.22),
            Self::Table => (1.8, 1.0),
            Self::Chair => (0.55, 0.55),
            Self::Bed => (2.0, 1.6),
            Self::Car => (4.2, 1.8),
            Self::Square => (1.0, 1.0),
            Self::Circle => (1.0, 1.0),
            Self::Triangle => (1.1, 1.0),
            Self::Light => (0.55, 0.55),
            Self::Custom => (1.0, 1.0),
        }
    }
}

/// One actor mark placed on a scene's 2D top-down floor plan.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanActor {
    pub id: Uuid,
    pub name: String,
    /// Position in meters (origin at canvas center).
    pub x: f32,
    pub y: f32,
    /// Facing angle in degrees (`0` = right, `90` = down, `-90` = up).
    #[serde(default)]
    pub angle_deg: f32,
    #[serde(default)]
    pub color: ShotColor,
    /// Optional blocking movement destination `(x, y)` in meters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub move_to: Option<(f32, f32)>,
}

/// One architectural element or set prop on a scene's 2D top-down floor plan.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanProp {
    pub id: Uuid,
    #[serde(default)]
    pub kind: PropKind,
    #[serde(default)]
    pub label: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    #[serde(default)]
    pub angle_deg: f32,
}

/// Default visual reach (meters) of a camera FOV cone on the 2D floor plan.
pub const DEFAULT_CAMERA_CONE_M: f32 = 2.6;

/// One camera setup placed on a scene's 2D top-down floor plan, linked 1-to-1
/// to a [`Shot`] via `shot_id` (inheriting its code, color, lens, and movement).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanCamera {
    pub shot_id: Uuid,
    pub x: f32,
    pub y: f32,
    /// Aim angle in degrees (`0` = right, `90` = down, `-90` = up).
    #[serde(default)]
    pub angle_deg: f32,
    /// Optional explicit horizontal FOV override (degrees). When `None`,
    /// derived automatically from the linked shot's `lens` string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fov_deg: Option<f32>,
    /// Optional visual FOV beam length in meters (defaults to [`DEFAULT_CAMERA_CONE_M`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cone_length: Option<f32>,
    /// Optional dolly / tracking end position `(x, y)` in meters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_to: Option<(f32, f32)>,
}

impl PlanCamera {
    pub fn effective_cone_length(&self) -> f32 {
        self.cone_length
            .unwrap_or(DEFAULT_CAMERA_CONE_M)
            .clamp(0.7, 12.0)
    }
}

/// 2D top-down blocking diagram for one scene (anchored by `Scene::id`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SceneFloorPlan {
    pub scene_id: Uuid,
    #[serde(default)]
    pub actors: Vec<PlanActor>,
    #[serde(default)]
    pub props: Vec<PlanProp>,
    #[serde(default)]
    pub cameras: Vec<PlanCamera>,
}

impl SceneFloorPlan {
    pub fn empty(scene_id: Uuid) -> Self {
        Self {
            scene_id,
            actors: Vec::new(),
            props: Vec::new(),
            cameras: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.actors.is_empty() && self.props.is_empty() && self.cameras.is_empty()
    }
}

/// Estimate horizontal field-of-view (degrees) from a focal-length string like
/// `"35mm"`, `"50"`, or `"24-70mm"` on a 35mm full-frame (36mm) sensor.
pub fn lens_to_fov_deg(lens: &str) -> f32 {
    let digits: String = lens
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let Ok(focal_mm) = digits.parse::<f32>() else {
        return 50.0;
    };
    if focal_mm <= 1.0 {
        return 50.0;
    }
    let fov = 2.0 * (18.0_f32 / focal_mm).atan().to_degrees();
    fov.clamp(12.0, 115.0)
}

/// Versioned collection of shots and per-scene blocking floor plans persisted
/// in `.kitsp` (`scriptcraft_shots`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShotList {
    #[serde(default = "default_shots_schema")]
    pub schema: u32,
    #[serde(default)]
    pub shots: Vec<Shot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scene_plans: Vec<SceneFloorPlan>,
}

impl Default for ShotList {
    fn default() -> Self {
        Self {
            schema: SHOTS_SCHEMA,
            shots: Vec::new(),
            scene_plans: Vec::new(),
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
        self.shots.is_empty() && self.scene_plans.iter().all(SceneFloorPlan::is_empty)
    }

    pub fn plan_for_scene(&self, scene_id: Uuid) -> Option<&SceneFloorPlan> {
        self.scene_plans.iter().find(|p| p.scene_id == scene_id)
    }

    pub fn plan_for_scene_mut(&mut self, scene_id: Uuid) -> Option<&mut SceneFloorPlan> {
        self.scene_plans.iter_mut().find(|p| p.scene_id == scene_id)
    }

    pub fn ensure_floor_plan(&mut self, scene_id: Uuid) -> &mut SceneFloorPlan {
        if let Some(pos) = self.scene_plans.iter().position(|p| p.scene_id == scene_id) {
            return &mut self.scene_plans[pos];
        }
        self.scene_plans.push(SceneFloorPlan::empty(scene_id));
        self.scene_plans.last_mut().unwrap()
    }

    /// Place a default camera on `scene_id`'s floor plan (if a plan exists and
    /// doesn't already have a camera for `shot_id`).
    pub fn sync_camera_for_shot(&mut self, scene_id: Uuid, shot_id: Uuid) {
        let Some(plan) = self.plan_for_scene_mut(scene_id) else {
            return;
        };
        if plan.cameras.iter().any(|c| c.shot_id == shot_id) {
            return;
        }
        let idx = plan.cameras.len();
        let spread = (idx as f32 - 1.5) * 1.6;
        let x = spread.clamp(-4.5, 4.5);
        let y = 2.8 + (idx % 2) as f32 * 0.6;
        let angle_deg = (-0.6_f32 - y).atan2(-x).to_degrees();
        plan.cameras.push(PlanCamera {
            shot_id,
            x,
            y,
            angle_deg,
            fov_deg: None,
            cone_length: None,
            track_to: None,
        });
    }

    /// Populate (or refresh missing cast & cameras on) `scene`'s floor plan
    /// from the screenplay's speaking characters and shots in `scene`.
    pub fn auto_populate_floor_plan(
        &mut self,
        scene: &Scene,
        blocks: &[ScriptBlock],
    ) -> &mut SceneFloorPlan {
        // Collect unique speaking character cues in this scene.
        let mut cast: Vec<String> = Vec::new();
        for &bi in &scene.blocks {
            let Some(b) = blocks.get(bi) else { continue };
            if b.element_type != ElementType::Character {
                continue;
            }
            let raw = b.text.trim();
            let base = raw.split('(').next().unwrap_or(raw).trim().to_uppercase();
            if !base.is_empty() && !cast.contains(&base) {
                cast.push(base);
            }
        }
        let scene_shot_ids: Vec<(Uuid, ShotMovement)> = self
            .shots_for_scene(scene, blocks)
            .into_iter()
            .filter_map(|i| self.shots.get(i).map(|s| (s.id, s.movement)))
            .collect();

        let plan = self.ensure_floor_plan(scene.id);

        // Add any missing cast actors positioned cleanly around the center stage.
        let existing_actors: HashSet<String> = plan
            .actors
            .iter()
            .map(|a| a.name.trim().to_uppercase())
            .collect();
        let missing_cast: Vec<String> = cast
            .into_iter()
            .filter(|name| !existing_actors.contains(name))
            .collect();
        let total_new = missing_cast.len();
        for (i, name) in missing_cast.into_iter().enumerate() {
            let color = ShotColor::from_index(plan.actors.len());
            let (x, y, angle_deg) = if total_new == 1 && plan.actors.is_empty() {
                (0.0, -0.6, 90.0)
            } else if total_new == 2 && plan.actors.is_empty() {
                if i == 0 {
                    (-1.4, -0.7, 15.0)
                } else {
                    (1.4, -0.7, 165.0)
                }
            } else {
                let n = (plan.actors.len() + 1).max(3) as f32;
                let a = std::f32::consts::PI * (i as f32 + 0.5) / n;
                let rx = -a.cos() * 2.1;
                let ry = -0.7 - a.sin() * 1.1;
                let aim = (-0.4_f32 - ry).atan2(-rx).to_degrees();
                (rx, ry, aim)
            };
            plan.actors.push(PlanActor {
                id: Uuid::new_v4(),
                name,
                x,
                y,
                angle_deg,
                color,
                move_to: None,
            });
        }

        // Compute focal center of actors so cameras aim directly at them.
        let (focus_x, focus_y) = if plan.actors.is_empty() {
            (0.0_f32, -0.6_f32)
        } else {
            let n = plan.actors.len() as f32;
            let sx: f32 = plan.actors.iter().map(|a| a.x).sum();
            let sy: f32 = plan.actors.iter().map(|a| a.y).sum();
            (sx / n, sy / n)
        };

        // Add cameras for any scene shots not yet placed on the floor plan.
        let existing_cams: HashSet<Uuid> = plan.cameras.iter().map(|c| c.shot_id).collect();
        let missing_shots: Vec<(Uuid, ShotMovement)> = scene_shot_ids
            .into_iter()
            .filter(|(id, _)| !existing_cams.contains(id))
            .collect();
        let m = missing_shots.len();
        for (i, (shot_id, movement)) in missing_shots.into_iter().enumerate() {
            let t = if m <= 1 {
                0.5
            } else {
                i as f32 / (m - 1) as f32
            };
            // Fan cameras smoothly across a 120-degree front arc below the actors.
            let arc_angle = (30.0 + t * 120.0).to_radians();
            let dist = if i % 2 == 0 { 3.3 } else { 2.7 };
            let x = focus_x + arc_angle.cos() * dist;
            let y = focus_y + arc_angle.sin() * dist;
            let angle_deg = (focus_y - y).atan2(focus_x - x).to_degrees();
            let track_to = matches!(movement, ShotMovement::Dolly | ShotMovement::Track)
                .then(|| (x + (focus_x - x) * 0.28, y + (focus_y - y) * 0.28));
            plan.cameras.push(PlanCamera {
                shot_id,
                x,
                y,
                angle_deg,
                fov_deg: None,
                cone_length: None,
                track_to,
            });
        }

        self.plan_for_scene_mut(scene.id).unwrap()
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
            reference_image: None,
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

    /// Add `range` to `shot_id` and automatically coalesce any overlapping or
    /// adjacent spans in `shot_id` into a single continuous span.
    pub fn assign_range_merged(
        &mut self,
        shot_id: Uuid,
        range: ShotRange,
        blocks: &[ScriptBlock],
    ) -> bool {
        let index = block_index_map(blocks);
        let Some(shot) = self.get_mut(shot_id) else {
            return false;
        };
        let before = shot.spans.clone();

        let mut unresolved: Vec<ShotRange> = Vec::new();
        let mut intervals: Vec<((usize, usize), (usize, usize))> = Vec::new();
        for sp in shot.spans.iter().chain(std::iter::once(&range)) {
            if let Some(iv) = sp.resolve_chars(&index, blocks) {
                if let Some(norm) = normalize_doc_interval(iv.0, iv.1, blocks) {
                    intervals.push(norm);
                }
            } else {
                unresolved.push(sp.clone());
            }
        }

        intervals.sort_unstable();
        let mut merged: Vec<((usize, usize), (usize, usize))> = Vec::new();
        for (lo, hi) in intervals {
            if let Some(last) = merged.last_mut() {
                if intervals_touch_or_overlap(last.1, lo, blocks) {
                    if hi > last.1 {
                        last.1 = hi;
                    }
                    continue;
                }
            }
            merged.push((lo, hi));
        }

        let mut next_spans: Vec<ShotRange> = merged
            .into_iter()
            .filter_map(|(lo, hi)| doc_interval_to_shot_range(lo, hi, blocks))
            .collect();
        next_spans.extend(unresolved);

        if next_spans == before {
            false
        } else {
            shot.spans = next_spans;
            true
        }
    }

    /// Carve `range` out of matching shot(s) (`None` = every shot covering it,
    /// `Some(id)` = only that shot). Splits spans cleanly when carving from the
    /// middle, or trims/removes them cleanly at the edges.
    pub fn unassign_range(
        &mut self,
        shot_filter: Option<Uuid>,
        range: &ShotRange,
        blocks: &[ScriptBlock],
    ) -> bool {
        let index = block_index_map(blocks);
        let Some(cut_raw) = range.resolve_chars(&index, blocks) else {
            return false;
        };
        let Some((cut_lo, cut_hi)) = normalize_doc_interval(cut_raw.0, cut_raw.1, blocks) else {
            return false;
        };

        let mut any_changed = false;
        for shot in &mut self.shots {
            if shot_filter.is_some_and(|id| shot.id != id) {
                continue;
            }
            let mut next_spans: Vec<ShotRange> = Vec::new();
            let mut shot_changed = false;
            for sp in &shot.spans {
                let Some(sp_raw) = sp.resolve_chars(&index, blocks) else {
                    next_spans.push(sp.clone());
                    continue;
                };
                let Some((s_lo, s_hi)) = normalize_doc_interval(sp_raw.0, sp_raw.1, blocks) else {
                    shot_changed = true;
                    continue;
                };
                if cut_hi <= s_lo || cut_lo >= s_hi {
                    next_spans.push(sp.clone());
                    continue;
                }
                shot_changed = true;
                if s_lo < cut_lo {
                    if let Some((l0, l1)) = normalize_doc_interval(s_lo, cut_lo, blocks) {
                        if let Some(sr) = doc_interval_to_shot_range(l0, l1, blocks) {
                            next_spans.push(sr);
                        }
                    }
                }
                if cut_hi < s_hi {
                    if let Some((r0, r1)) = normalize_doc_interval(cut_hi, s_hi, blocks) {
                        if let Some(sr) = doc_interval_to_shot_range(r0, r1, blocks) {
                            next_spans.push(sr);
                        }
                    }
                }
            }
            if shot_changed {
                shot.spans = next_spans;
                any_changed = true;
            }
        }
        any_changed
    }

    /// Reassign `range` from whichever shot(s) currently cover it to
    /// `target_shot_id` (carving it out of other shots and merging into target).
    pub fn reassign_range(
        &mut self,
        target_shot_id: Uuid,
        range: ShotRange,
        blocks: &[ScriptBlock],
    ) -> bool {
        let removed = self.unassign_range(None, &range, blocks);
        let added = self.assign_range_merged(target_shot_id, range, blocks);
        removed || added
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

    /// Delete a shot by id (and remove any linked camera from floor plans).
    pub fn delete_shot(&mut self, shot_id: Uuid) -> bool {
        let before = self.shots.len();
        self.shots.retain(|s| s.id != shot_id);
        for plan in &mut self.scene_plans {
            plan.cameras.retain(|c| c.shot_id != shot_id);
        }
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

    /// Full readable text covered by `shot` across all its spans (no truncation).
    pub fn covered_text_full(&self, shot: &Shot, blocks: &[ScriptBlock]) -> String {
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
        parts.join(" / ")
    }

    /// Short readable excerpt of the exact screenplay text covered by `shot`.
    pub fn covered_excerpt(&self, shot: &Shot, blocks: &[ScriptBlock]) -> String {
        let joined = self.covered_text_full(shot, blocks);
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
            let excerpt = self.covered_text_full(shot, blocks);
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

    /// Derive a chronological playback timeline for `scene` directly from the
    /// screenplay order and word-based estimated screen duration.
    ///
    /// Adjacent runs belonging to the same active shot are merged into single
    /// editorial beats so scrubbing smoothly tracks cuts across the scene.
    pub fn scene_timeline(&self, scene: &Scene, blocks: &[ScriptBlock]) -> SceneTimeline {
        let char_cov = self.resolve_block_char_coverage(blocks);
        let mut shot_total_chars = vec![0usize; self.shots.len()];
        for spans in &char_cov {
            for &(si, c0, c1) in spans {
                if let Some(slot) = shot_total_chars.get_mut(si) {
                    *slot += c1.saturating_sub(c0);
                }
            }
        }

        let mut beats: Vec<TimelineBeat> = Vec::new();
        let mut t = 0.0_f32;

        for &bi in &scene.blocks {
            let Some(block) = blocks.get(bi) else {
                continue;
            };
            if !block.element_type.is_printable() || block.element_type == ElementType::SceneHeading
            {
                continue;
            }
            let char_len = block.text.chars().count();
            if char_len == 0 {
                continue;
            }
            let spans = char_cov.get(bi).map(Vec::as_slice).unwrap_or(&[]);
            let sec_per_word = if matches!(
                block.element_type,
                ElementType::Dialogue | ElementType::Parenthetical | ElementType::Lyrics
            ) {
                DIALOG_S_PER_WORD as f32
            } else {
                ACTION_S_PER_WORD as f32
            };

            // Partition this block into contiguous runs with identical active shot + overlap set
            let mut runs: Vec<(Option<usize>, Vec<Uuid>, usize, usize)> = Vec::new();
            for ci in 0..char_len {
                let mut best: Option<(usize, usize, usize)> = None;
                let mut overlapping: Vec<Uuid> = Vec::new();
                for &(si, c0, c1) in spans {
                    if ci >= c0 && ci < c1 {
                        if let Some(sh) = self.shots.get(si) {
                            if !overlapping.contains(&sh.id) {
                                overlapping.push(sh.id);
                            }
                        }
                        let span_len = c1 - c0;
                        let total_c = shot_total_chars.get(si).copied().unwrap_or(usize::MAX);
                        if best.is_none_or(|(_, bl, bt)| (span_len, total_c) <= (bl, bt)) {
                            best = Some((si, span_len, total_c));
                        }
                    }
                }
                let active_si = best.map(|(si, _, _)| si);
                if let Some(last) = runs.last_mut() {
                    if last.0 == active_si && last.3 == ci {
                        for oid in overlapping {
                            if !last.1.contains(&oid) {
                                last.1.push(oid);
                            }
                        }
                        last.3 = ci + 1;
                        continue;
                    }
                }
                runs.push((active_si, overlapping, ci, ci + 1));
            }

            for (active_si, overlapping, c0, c1) in runs {
                let slice = slice_chars(&block.text, c0, c1);
                let words = slice.split_whitespace().count();
                let dur = if words > 0 {
                    (words as f32 * sec_per_word).max(0.35)
                } else {
                    0.25
                };
                let shot_id = active_si.and_then(|si| self.shots.get(si).map(|s| s.id));
                if let Some(last) = beats.last_mut() {
                    if last.shot_id == shot_id {
                        last.end_sec = t + dur;
                        for oid in overlapping {
                            if !last.overlapping_shot_ids.contains(&oid) {
                                last.overlapping_shot_ids.push(oid);
                            }
                        }
                        if let Some(lr) = last.ranges.last_mut() {
                            if lr.0 == bi && lr.2 == c0 {
                                lr.2 = c1;
                            } else {
                                last.ranges.push((bi, c0, c1));
                            }
                        } else {
                            last.ranges.push((bi, c0, c1));
                        }
                        t += dur;
                        continue;
                    }
                }
                beats.push(TimelineBeat {
                    shot_idx: active_si,
                    shot_id,
                    overlapping_shot_ids: overlapping,
                    ranges: vec![(bi, c0, c1)],
                    start_sec: t,
                    end_sec: t + dur,
                });
                t += dur;
            }
        }

        SceneTimeline {
            beats,
            total_secs: t,
        }
    }

    /// Apply char-offset shifting for a batch of [`crate::edit::EditCommand`]s
    /// against `blocks` (call right before applying `cmds` to `blocks`).
    pub fn sync_for_edit_commands(
        &mut self,
        blocks: &[ScriptBlock],
        cmds: &[crate::edit::EditCommand],
    ) {
        if self.shots.is_empty() {
            return;
        }
        for cmd in cmds {
            match cmd {
                crate::edit::EditCommand::InsertText { at, text } => {
                    if let Some(b) = blocks.get(at.bi) {
                        let abs = crate::edit::char_index(&b.text, *at);
                        let len = text.chars().count();
                        self.shift_block_insert(b.id, abs, len);
                    }
                }
                crate::edit::EditCommand::DeleteRange { start, end } => {
                    let (lo, hi) = if start <= end {
                        (*start, *end)
                    } else {
                        (*end, *start)
                    };
                    if lo.bi == hi.bi {
                        if let Some(b) = blocks.get(lo.bi) {
                            let s = crate::edit::char_index(&b.text, lo);
                            let e = crate::edit::char_index(&b.text, hi);
                            self.shift_block_delete(b.id, s, e);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Shift character-level shot spans after inserting `len` chars at `at` in `block_id`.
    pub fn shift_block_insert(&mut self, block_id: Uuid, at: usize, len: usize) {
        if len == 0 || self.shots.is_empty() {
            return;
        }
        for shot in &mut self.shots {
            for span in &mut shot.spans {
                if span.start_block == block_id {
                    if let Some(sc) = span.start_char.as_mut() {
                        if *sc >= at {
                            *sc += len;
                        }
                    }
                }
                if span.end_block == block_id {
                    if let Some(ec) = span.end_char.as_mut() {
                        if *ec > at {
                            *ec += len;
                        }
                    }
                }
            }
        }
    }

    /// Shift character-level shot spans after deleting `[start, end)` in `block_id`.
    pub fn shift_block_delete(&mut self, block_id: Uuid, start: usize, end: usize) {
        if start >= end || self.shots.is_empty() {
            return;
        }
        let delta = end - start;
        let map_pos = |pos: usize| -> usize {
            if pos <= start {
                pos
            } else if pos <= end {
                start
            } else {
                pos - delta
            }
        };
        for shot in &mut self.shots {
            for span in &mut shot.spans {
                if span.start_block == block_id {
                    if let Some(sc) = span.start_char.as_mut() {
                        *sc = map_pos(*sc);
                    }
                }
                if span.end_block == block_id {
                    if let Some(ec) = span.end_char.as_mut() {
                        *ec = map_pos(*ec);
                    }
                }
            }
            // Drop any single-block char span that collapsed to empty
            shot.spans.retain(|sp| {
                if sp.start_block == sp.end_block {
                    match (sp.start_char, sp.end_char) {
                        (Some(a), Some(b)) => a < b,
                        _ => true,
                    }
                } else {
                    true
                }
            });
        }
    }

    /// Detect revision-modified coverage and deleted-block spans for every shot.
    pub fn shot_alerts(
        &self,
        blocks: &[ScriptBlock],
        revision_marks: &[Option<RevisionColor>],
    ) -> HashMap<Uuid, ShotAlert> {
        let index = block_index_map(blocks);
        let mut out = HashMap::new();
        for shot in &self.shots {
            let mut deleted_spans = false;
            let mut revision: Option<RevisionColor> = None;
            for span in &shot.spans {
                let Some((lo, hi)) = span.resolve(&index) else {
                    deleted_spans = true;
                    continue;
                };
                for bi in lo..=hi {
                    if let Some(Some(col)) = revision_marks.get(bi) {
                        revision = Some(*col);
                    }
                }
            }
            if deleted_spans || revision.is_some() {
                out.insert(
                    shot.id,
                    ShotAlert {
                        deleted_spans,
                        revision,
                    },
                );
            }
        }
        out
    }
}

/// Revision / script-sync status alert for one [`Shot`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShotAlert {
    /// True if at least one of the shot's spans references a deleted script block.
    pub deleted_spans: bool,
    /// Present if any covered script block is marked in the active revision.
    pub revision: Option<RevisionColor>,
}

/// One chronological beat on a scene's derived playback timeline.
#[derive(Clone, Debug, PartialEq)]
pub struct TimelineBeat {
    pub shot_idx: Option<usize>,
    pub shot_id: Option<Uuid>,
    pub overlapping_shot_ids: Vec<Uuid>,
    /// Exact `(block_idx, char_start, char_end)` slices played during this beat.
    pub ranges: Vec<(usize, usize, usize)>,
    pub start_sec: f32,
    pub end_sec: f32,
}

/// Chronological scene timeline derived automatically from shot spans and script timing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SceneTimeline {
    pub beats: Vec<TimelineBeat>,
    pub total_secs: f32,
}

impl SceneTimeline {
    pub fn is_empty(&self) -> bool {
        self.beats.is_empty() || self.total_secs <= 0.0
    }

    /// Return `(beat_index, beat, local_progress_0_to_1)` at timestamp `t` (seconds).
    pub fn beat_at(&self, t: f32) -> Option<(usize, &TimelineBeat, f32)> {
        if self.beats.is_empty() {
            return None;
        }
        let clamped = t.clamp(0.0, self.total_secs);
        for (i, beat) in self.beats.iter().enumerate() {
            if clamped <= beat.end_sec || i + 1 == self.beats.len() {
                let len = (beat.end_sec - beat.start_sec).max(1e-4);
                let frac = ((clamped - beat.start_sec) / len).clamp(0.0, 1.0);
                return Some((i, beat, frac));
            }
        }
        None
    }

    /// Timestamp of the previous beat start (for `<` Prev Cut stepping).
    pub fn prev_cut_time(&self, t: f32) -> f32 {
        let eps = 0.08;
        for beat in self.beats.iter().rev() {
            if beat.start_sec < t - eps {
                return beat.start_sec;
            }
        }
        0.0
    }

    /// Timestamp of the next beat start (for `>` Next Cut stepping).
    pub fn next_cut_time(&self, t: f32) -> f32 {
        let eps = 0.05;
        for beat in &self.beats {
            if beat.start_sec > t + eps {
                return beat.start_sec;
            }
        }
        self.beats
            .last()
            .map(|b| b.start_sec)
            .unwrap_or(self.total_secs)
    }
}

fn encode_base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn decode_base64(input: &str) -> Option<Vec<u8>> {
    fn val(b: u8) -> Option<u8> {
        match b {
            b'A'..=b'Z' => Some(b - b'A'),
            b'a'..=b'z' => Some(b - b'a' + 26),
            b'0'..=b'9' => Some(b - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let cleaned: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if !cleaned.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity((cleaned.len() / 4) * 3);
    for chunk in cleaned.as_chunks::<4>().0 {
        let a = val(chunk[0])?;
        let b = val(chunk[1])?;
        out.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            let c = val(chunk[2])?;
            out.push((b << 4) | (c >> 2));
            if chunk[3] != b'=' {
                let d = val(chunk[3])?;
                out.push((c << 6) | d);
            }
        }
    }
    Some(out)
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

/// Normalize a document `(block, char)` half-open interval so its start doesn't
/// sit past the end of its block and its end doesn't sit at char 0 of a later
/// block. Returns `None` if empty.
fn normalize_doc_interval(
    mut lo: (usize, usize),
    mut hi: (usize, usize),
    blocks: &[ScriptBlock],
) -> Option<((usize, usize), (usize, usize))> {
    if lo >= hi || blocks.is_empty() {
        return None;
    }
    while lo.0 < hi.0 {
        let len = blocks
            .get(lo.0)
            .map(|b| b.text.chars().count())
            .unwrap_or(0);
        if lo.1 >= len {
            lo = (lo.0 + 1, 0);
        } else {
            break;
        }
    }
    while hi.0 > lo.0 && hi.1 == 0 {
        let prev = hi.0 - 1;
        let plen = blocks
            .get(prev)
            .map(|b| b.text.chars().count())
            .unwrap_or(0);
        hi = (prev, plen);
    }
    if lo < hi {
        Some((lo, hi))
    } else {
        None
    }
}

/// True when two normalized document intervals `[a_lo, a_hi)` and `[b_lo, b_hi)`
/// overlap, touch, are separated only by a single space within the same block,
/// or are contiguous across consecutive printable blocks.
fn intervals_touch_or_overlap(
    a_hi: (usize, usize),
    b_lo: (usize, usize),
    blocks: &[ScriptBlock],
) -> bool {
    if b_lo <= a_hi {
        return true;
    }
    if a_hi.0 == b_lo.0 {
        if b_lo.1 <= a_hi.1 + 1 {
            // Merge if touching or separated by at most 1 whitespace character
            if b_lo.1 == a_hi.1 {
                return true;
            }
            if let Some(b) = blocks.get(a_hi.0) {
                if b.text.chars().nth(a_hi.1).is_some_and(char::is_whitespace) {
                    return true;
                }
            }
        }
        return false;
    }
    // Check if a_hi is at the end of its block and b_lo starts at char 0 of the
    // very next printable block (skipping any non-printable blocks in between).
    let a_len = blocks
        .get(a_hi.0)
        .map(|b| b.text.chars().count())
        .unwrap_or(0);
    if a_hi.1 >= a_len && b_lo.1 == 0 {
        let next_printable =
            ((a_hi.0 + 1)..blocks.len()).find(|&bi| blocks[bi].element_type.is_printable());
        if next_printable == Some(b_lo.0) {
            return true;
        }
    }
    false
}

fn doc_interval_to_shot_range(
    lo: (usize, usize),
    hi: (usize, usize),
    blocks: &[ScriptBlock],
) -> Option<ShotRange> {
    let sb = blocks.get(lo.0)?;
    let eb = blocks.get(hi.0)?;
    let hi_len = eb.text.chars().count();
    if lo.1 == 0 && hi.1 >= hi_len {
        Some(ShotRange::blocks(sb.id, eb.id))
    } else {
        Some(ShotRange::chars(sb.id, lo.1, eb.id, hi.1))
    }
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

    #[test]
    fn floor_plan_auto_populates_cast_cameras_and_lens_fov() {
        assert!((lens_to_fov_deg("24mm") - 73.7).abs() < 1.0);
        assert!((lens_to_fov_deg("50mm") - 39.6).abs() < 1.0);
        assert!((lens_to_fov_deg("85mm") - 23.9).abs() < 1.0);
        assert_eq!(lens_to_fov_deg(""), 50.0);

        let p = sample_project();
        let scenes = p.scenes();
        let scene = &scenes[0];
        let mut list = ShotList::default();
        let s1 = list.add_shot(
            Some(scene.id),
            "1",
            ShotSize::Ws,
            ShotMovement::Dolly,
            "28mm".into(),
            "Master".into(),
            ShotRange::blocks(p.blocks[1].id, p.blocks[4].id),
        );
        let plan = list.auto_populate_floor_plan(scene, &p.blocks);
        assert_eq!(plan.actors.len(), 1);
        assert_eq!(plan.actors[0].name, "IVAN");
        assert_eq!(plan.cameras.len(), 1);
        assert_eq!(plan.cameras[0].shot_id, s1);
        assert!(
            plan.cameras[0].track_to.is_some(),
            "Dolly shot should auto-seed a track motion arrow"
        );

        // Deleting the shot must prune its camera from the floor plan
        assert!(list.delete_shot(s1));
        assert!(list.plan_for_scene(scene.id).unwrap().cameras.is_empty());
    }

    #[test]
    fn assign_range_merged_unassign_and_reassign_ranges() {
        let p = sample_project();
        let sid = p.blocks[0].id;
        let b1 = p.blocks[1].id; // "Coffee boils on the stove." (26 chars)
        let b2 = p.blocks[2].id; // "IVAN"
        let b3 = p.blocks[3].id; // "We leave at dawn."

        let mut list = ShotList::default();
        let s1 = list.add_shot(
            Some(sid),
            "1",
            ShotSize::Ws,
            ShotMovement::Static,
            "35mm".into(),
            "Master".into(),
            ShotRange::chars(b1, 0, b1, 12), // "Coffee boils"
        );
        // Extending the same sentence merges into 1 continuous span instead of 2
        assert!(list.assign_range_merged(s1, ShotRange::chars(b1, 7, b1, 26), &p.blocks));
        assert_eq!(list.get(s1).unwrap().spans.len(), 1);
        assert_eq!(
            list.covered_excerpt(list.get(s1).unwrap(), &p.blocks),
            "Coffee boils on the stove."
        );

        // Extending contiguously onto the next block also merges into 1 span
        assert!(list.assign_range_merged(s1, ShotRange::blocks(b2, b2), &p.blocks));
        assert_eq!(list.get(s1).unwrap().spans.len(), 1);
        assert_eq!(list.get(s1).unwrap().spans[0], ShotRange::blocks(b1, b2));

        // Carving out the middle of b1 splits s1 cleanly and moves that part to s2
        let s2 = list.add_shot(
            Some(sid),
            "1",
            ShotSize::Cu,
            ShotMovement::Static,
            "85mm".into(),
            "Detail".into(),
            ShotRange::blocks(b3, b3),
        );
        assert!(list.reassign_range(s2, ShotRange::chars(b1, 7, b1, 15), &p.blocks));
        // s1 no longer covers chars 7..15 ("boils on")
        let s1_cov = list.resolve_block_char_coverage(&p.blocks);
        assert!(
            !s1_cov[1]
                .iter()
                .any(|&(si, c0, c1)| si == 0 && c0 <= 10 && c1 > 10),
            "s1 must no longer cover reassigned slice"
        );
        assert!(
            s1_cov[1]
                .iter()
                .any(|&(si, c0, c1)| si == 1 && c0 == 7 && c1 == 15),
            "s2 must now cover reassigned slice"
        );

        // Unassigning clears coverage cleanly
        assert!(list.unassign_range(None, &ShotRange::blocks(b3, b3), &p.blocks));
        let cov_after = list.resolve_block_coverage(&p.blocks);
        assert!(cov_after[3].is_empty());
    }

    #[test]
    fn twenty_shot_colors_and_camera_beam_length() {
        assert_eq!(ShotColor::ALL_STANDARD.len(), 10);
        assert_eq!(ShotColor::ALL_DARK.len(), 10);
        assert_eq!(ShotColor::ALL.len(), 20);
        // Every color has a unique label and distinct RGB
        let mut labels = std::collections::HashSet::new();
        let mut rgbs = std::collections::HashSet::new();
        for c in ShotColor::ALL {
            assert!(labels.insert(c.label()), "duplicate label {}", c.label());
            assert!(rgbs.insert(c.rgb()), "duplicate rgb for {}", c.label());
        }
        // Standard colors remain brighter on average than their paired dark variants
        for (std_c, dark_c) in ShotColor::ALL_STANDARD
            .iter()
            .zip(ShotColor::ALL_DARK.iter())
        {
            let (sr, sg, sb) = std_c.rgb();
            let (dr, dg, db) = dark_c.rgb();
            let std_luma = u16::from(sr) + u16::from(sg) + u16::from(sb);
            let dark_luma = u16::from(dr) + u16::from(dg) + u16::from(db);
            assert!(
                dark_luma < std_luma,
                "{} should be darker than {}",
                dark_c.label(),
                std_c.label()
            );
        }

        // Camera cone length defaults cleanly and clamps extremes
        let mut cam = PlanCamera {
            shot_id: Uuid::new_v4(),
            x: 0.0,
            y: 0.0,
            angle_deg: 0.0,
            fov_deg: None,
            cone_length: None,
            track_to: None,
        };
        assert!((cam.effective_cone_length() - DEFAULT_CAMERA_CONE_M).abs() < 1e-4);
        cam.cone_length = Some(0.1);
        assert!((cam.effective_cone_length() - 0.7).abs() < 1e-4);
        cam.cone_length = Some(99.0);
        assert!((cam.effective_cone_length() - 12.0).abs() < 1e-4);
        cam.cone_length = Some(1.8);
        assert!((cam.effective_cone_length() - 1.8).abs() < 1e-4);
    }

    #[test]
    fn covered_text_full_preserves_long_text_without_truncation() {
        let long_line = "Rain hammers the skylights above the warehouse while Ivan slowly unlocks the rusted steel briefcase and spreads every photograph across the workbench.";
        assert!(long_line.chars().count() > 120);
        let b0 = ScriptBlock::new(ElementType::SceneHeading, "INT. WAREHOUSE - NIGHT".into());
        let b1 = ScriptBlock::new(ElementType::Action, long_line.into());
        let b1_id = b1.id;
        let blocks = vec![b0, b1];
        let mut list = ShotList::default();
        let sid = list.add_shot(
            Some(blocks[0].id),
            "1",
            ShotSize::Ws,
            ShotMovement::Static,
            "28mm".into(),
            String::new(),
            ShotRange::blocks(b1_id, b1_id),
        );
        let shot = list.get(sid).unwrap();
        assert_eq!(list.covered_text_full(shot, &blocks), long_line);
        assert!(list.covered_excerpt(shot, &blocks).ends_with('…'));
    }

    #[test]
    fn timeline_beats_and_cut_navigation() {
        let p = sample_project();
        let scenes = p.scenes();
        let sc = &scenes[0];
        let b1 = p.blocks[1].id;
        let b2 = p.blocks[2].id;
        let b3 = p.blocks[3].id;
        let mut list = ShotList::default();
        let s1 = list.add_shot(
            Some(sc.id),
            "1",
            ShotSize::Ws,
            ShotMovement::Static,
            "35mm".into(),
            "Master".into(),
            ShotRange::blocks(b1, b3),
        );
        let s2 = list.add_shot(
            Some(sc.id),
            "1",
            ShotSize::Cu,
            ShotMovement::Static,
            "85mm".into(),
            "Cut-in on dialogue".into(),
            ShotRange::blocks(b2, b3),
        );

        let tl = list.scene_timeline(sc, &p.blocks);
        assert!(!tl.is_empty());
        assert!(tl.total_secs > 1.0);
        // First beat should belong to master s1, second beat to tighter cut-in s2
        assert!(tl.beats.len() >= 2, "expected at least 2 beats: {tl:?}");
        assert_eq!(tl.beats[0].shot_id, Some(s1));
        assert!(tl.beats.iter().any(|b| b.shot_id == Some(s2)));

        let (i0, b0, frac0) = tl.beat_at(0.0).unwrap();
        assert_eq!(i0, 0);
        assert_eq!(b0.shot_id, Some(s1));
        assert!((0.0..=0.01).contains(&frac0));

        let t_next = tl.next_cut_time(0.0);
        assert!(t_next > 0.0);
        let (i1, _, _) = tl.beat_at(t_next + 0.01).unwrap();
        assert!(i1 >= 1);
        let t_prev = tl.prev_cut_time(t_next + 0.01);
        assert!((t_prev - 0.0).abs() < 1e-4);
    }

    #[test]
    fn prop_basic_shapes_and_reference_image_roundtrip() {
        assert!(PropKind::ALL.contains(&PropKind::Square));
        assert!(PropKind::ALL.contains(&PropKind::Circle));
        assert!(PropKind::ALL.contains(&PropKind::Triangle));
        assert!(PropKind::ALL.contains(&PropKind::Light));
        for kind in PropKind::ALL {
            assert!(!kind.label().is_empty());
            let (w, d) = kind.default_size();
            assert!(w > 0.0 && d > 0.0);
        }

        let b0 = ScriptBlock::new(ElementType::Action, "Test".into());
        let b0_id = b0.id;
        let mut list = ShotList::default();
        let sid = list.add_shot(
            None,
            "1",
            ShotSize::Ms,
            ShotMovement::Static,
            "50mm".into(),
            String::new(),
            ShotRange::blocks(b0_id, b0_id),
        );
        let payload: Vec<u8> = (0u8..=255).collect();
        list.get_mut(sid)
            .unwrap()
            .set_reference_image_bytes(Some(&payload));
        assert_eq!(
            list.get(sid).unwrap().reference_image_bytes().as_deref(),
            Some(payload.as_slice())
        );

        let json = serde_json::to_string(&list).expect("serialize shots with ref image");
        let parsed: ShotList =
            serde_json::from_str(&json).expect("deserialize shots with ref image");
        assert_eq!(
            parsed.get(sid).unwrap().reference_image_bytes().as_deref(),
            Some(payload.as_slice())
        );
    }

    #[test]
    fn script_edit_shifts_shot_spans_and_revision_alerts() {
        use crate::edit::EditCommand;
        let mut p = sample_project();
        let b1_id = p.blocks[1].id;
        let mut list = ShotList::default();
        // Cover chars 7..15 ("boils on") of block 1 ("Coffee boils on a chipped stove.")
        let sid = list.add_shot(
            Some(p.blocks[0].id),
            "1",
            ShotSize::Cu,
            ShotMovement::Static,
            "85mm".into(),
            String::new(),
            ShotRange::chars(b1_id, 7, b1_id, 15),
        );

        // Insert 4 chars ("Hot ") at offset 0 of block 1
        let ins = EditCommand::InsertText {
            at: crate::edit::Pos {
                bi: 1,
                para: 0,
                ch: 0,
            },
            text: "Hot ".into(),
        };
        list.sync_for_edit_commands(&p.blocks, std::slice::from_ref(&ins));
        p.blocks[1].text.insert_str(0, "Hot ");
        let span = &list.get(sid).unwrap().spans[0];
        assert_eq!(span.start_char, Some(11));
        assert_eq!(span.end_char, Some(19));

        // Delete 4 chars ("Hot ") back at offset 0
        let del = EditCommand::DeleteRange {
            start: crate::edit::Pos {
                bi: 1,
                para: 0,
                ch: 0,
            },
            end: crate::edit::Pos {
                bi: 1,
                para: 0,
                ch: 4,
            },
        };
        list.sync_for_edit_commands(&p.blocks, std::slice::from_ref(&del));
        p.blocks[1].text.drain(..4);
        let span = &list.get(sid).unwrap().spans[0];
        assert_eq!(span.start_char, Some(7));
        assert_eq!(span.end_char, Some(15));

        // Revision alert & deleted-block alert
        let mut marks = vec![None; p.blocks.len()];
        marks[1] = Some(RevisionColor::Blue);
        let alerts = list.shot_alerts(&p.blocks, &marks);
        assert_eq!(
            alerts.get(&sid),
            Some(&ShotAlert {
                deleted_spans: false,
                revision: Some(RevisionColor::Blue),
            })
        );

        // Removing block 1 triggers deleted_spans alert
        p.blocks.remove(1);
        let alerts_del = list.shot_alerts(&p.blocks, &[]);
        assert_eq!(
            alerts_del.get(&sid),
            Some(&ShotAlert {
                deleted_spans: true,
                revision: None,
            })
        );
    }
}
