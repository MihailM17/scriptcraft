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
}

/// Standard architectural & set prop types for the 2D top-down floor plan.
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
    Custom,
}

impl PropKind {
    pub const ALL: [Self; 8] = [
        Self::Wall,
        Self::Door,
        Self::Window,
        Self::Table,
        Self::Chair,
        Self::Bed,
        Self::Car,
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
}
