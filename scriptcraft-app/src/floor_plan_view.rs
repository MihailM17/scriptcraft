//! Interactive 2D top-down blocking & floor-plan canvas (Phase 2 of Shot Breakdown).
//!
//! Renders per-scene architectural props, actor marks, 180-degree line of action,
//! and color-matched cameras with live focal-length FOV cones and dolly/tracking
//! paths using native `egui::Painter` primitives.

use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Layout, Margin, Pos2, Rect, RichText, Sense,
    Shape, Stroke, StrokeKind, TextEdit, Ui, Vec2,
};
use scriptcraft_core::shots::{
    lens_to_fov_deg, PlanActor, PlanProp, PropKind, SceneFloorPlan, ShotColor,
};
use scriptcraft_core::{Project, Scene};
use uuid::Uuid;

use crate::theme::{ink_on_accent, kit, kit_is_light};

const DEFAULT_ZOOM: f32 = 54.0; // pixels per meter
const MIN_ZOOM: f32 = 22.0;
const MAX_ZOOM: f32 = 140.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanSelection {
    Camera(Uuid),
    Actor(Uuid),
    Prop(Uuid),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragKind {
    Pan,
    MoveCamera(Uuid),
    AimCamera(Uuid),
    MoveCameraTrack(Uuid),
    MoveActor(Uuid),
    AimActor(Uuid),
    MoveActorTarget(Uuid),
    MoveProp(Uuid),
    RotateProp(Uuid),
}

/// Persistent UI state for the 2D floor plan canvas.
pub struct FloorPlanUiState {
    /// World center visible at canvas center (meters).
    pub pan: Vec2,
    /// Pixels per meter.
    pub zoom: f32,
    /// Currently selected item on the floor plan.
    pub selected: Option<PlanSelection>,
    /// Show the classic 180-degree Line of Action between the first two actors.
    pub show_axis_line: bool,
    drag: Option<DragKind>,
    drag_offset: Vec2,
    last_pan_pos: Option<Pos2>,
}

impl Default for FloorPlanUiState {
    fn default() -> Self {
        Self {
            pan: Vec2::new(0.0, 0.4),
            zoom: DEFAULT_ZOOM,
            selected: None,
            show_axis_line: true,
            drag: None,
            drag_offset: Vec2::ZERO,
            last_pan_pos: None,
        }
    }
}

impl FloorPlanUiState {
    pub fn reset_view(&mut self) {
        self.pan = Vec2::new(0.0, 0.4);
        self.zoom = DEFAULT_ZOOM;
        self.drag = None;
    }

    pub fn fit_to_plan(&mut self, plan: &SceneFloorPlan, canvas_size: Vec2) {
        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let mut expand = |x: f32, y: f32, pad: f32| {
            min_x = min_x.min(x - pad);
            max_x = max_x.max(x + pad);
            min_y = min_y.min(y - pad);
            max_y = max_y.max(y + pad);
        };
        for a in &plan.actors {
            expand(a.x, a.y, 1.0);
            if let Some((mx, my)) = a.move_to {
                expand(mx, my, 0.8);
            }
        }
        for c in &plan.cameras {
            expand(c.x, c.y, 1.2);
            if let Some((tx, ty)) = c.track_to {
                expand(tx, ty, 1.0);
            }
        }
        for p in &plan.props {
            let r = p.w.hypot(p.h) * 0.5 + 0.5;
            expand(p.x, p.y, r);
        }
        if !min_x.is_finite() {
            self.reset_view();
            return;
        }
        self.pan = Vec2::new((min_x + max_x) * 0.5, (min_y + max_y) * 0.5);
        let span_x = (max_x - min_x).max(4.0);
        let span_y = (max_y - min_y).max(3.5);
        let zx = (canvas_size.x * 0.82) / span_x;
        let zy = (canvas_size.y * 0.80) / span_y;
        self.zoom = zx.min(zy).clamp(MIN_ZOOM, MAX_ZOOM);
    }
}

fn shot_color32(color: ShotColor) -> Color32 {
    let (r, g, b) = color.rgb();
    Color32::from_rgb(r, g, b)
}

fn world_to_screen(center: Pos2, pan: Vec2, zoom: f32, wx: f32, wy: f32) -> Pos2 {
    Pos2::new(
        center.x + (wx - pan.x) * zoom,
        center.y + (wy - pan.y) * zoom,
    )
}

fn screen_to_world(center: Pos2, pan: Vec2, zoom: f32, p: Pos2) -> Vec2 {
    Vec2::new(
        (p.x - center.x) / zoom + pan.x,
        (p.y - center.y) / zoom + pan.y,
    )
}

fn rot2(x: f32, y: f32, angle_rad: f32) -> Vec2 {
    let (s, c) = angle_rad.sin_cos();
    Vec2::new(x * c - y * s, x * s + y * c)
}

/// Render the complete interactive Floor Plan panel (toolbar + vector canvas +
/// contextual bottom bar) for `active_scene`.
///
/// Returns `true` when the project's floor plan / shots data was mutated.
pub fn show(
    ui: &mut Ui,
    project: &mut Project,
    active_scene: Option<&Scene>,
    selected_shot: &mut Option<Uuid>,
    state: &mut FloorPlanUiState,
) -> bool {
    let Some(scene) = active_scene else {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new("Select a scene to open its 2D floor plan.")
                    .size(13.0)
                    .color(kit().disabled_text),
            );
        });
        return false;
    };

    let mut changed = false;
    let scene_id = scene.id;

    // Auto-populate floor plan once when a scene has no plan yet (or has shots
    // without cameras yet while plan is empty).
    let needs_seed = project
        .shots
        .plan_for_scene(scene_id)
        .is_none_or(SceneFloorPlan::is_empty);
    if needs_seed {
        project
            .shots
            .auto_populate_floor_plan(scene, &project.blocks);
    }

    // Keep camera selection synced when the user clicks a shot in the right pane.
    if let Some(sid) = *selected_shot {
        if !matches!(
            state.selected,
            Some(PlanSelection::Actor(_)) | Some(PlanSelection::Prop(_))
        ) {
            state.selected = Some(PlanSelection::Camera(sid));
        }
    }

    // 1. Compact studio toolbar above the canvas
    let mut fit_requested = false;
    let bar_resp = egui::Frame::new()
        .fill(kit().surface)
        .inner_margin(Margin::symmetric(12, 6))
        .stroke(Stroke::NONE)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(5.0, 4.0);

                // Add Actor
                if tool_chip(ui, "+ Actor", false, "Add character mark to floor plan").clicked() {
                    let plan = project.shots.ensure_floor_plan(scene_id);
                    let idx = plan.actors.len() + 1;
                    let id = Uuid::new_v4();
                    plan.actors.push(PlanActor {
                        id,
                        name: format!("ACTOR {idx}"),
                        x: state.pan.x + (idx as f32 % 3.0 - 1.0) * 0.7,
                        y: state.pan.y - 0.5,
                        angle_deg: 90.0,
                        color: ShotColor::from_index(idx),
                        move_to: None,
                    });
                    state.selected = Some(PlanSelection::Actor(id));
                    changed = true;
                }

                // Quick set architecture & prop buttons
                for (kind, label) in [
                    (PropKind::Wall, "+ Wall"),
                    (PropKind::Door, "+ Door"),
                    (PropKind::Window, "+ Window"),
                    (PropKind::Table, "+ Table"),
                    (PropKind::Chair, "+ Chair"),
                    (PropKind::Car, "+ Car"),
                ] {
                    if tool_chip(
                        ui,
                        label,
                        false,
                        &format!("Add {} to floor plan", kind.label()),
                    )
                    .clicked()
                    {
                        let plan = project.shots.ensure_floor_plan(scene_id);
                        let (w, h) = kind.default_size();
                        let id = Uuid::new_v4();
                        let offset = (plan.props.len() as f32 % 4.0) * 0.35;
                        plan.props.push(PlanProp {
                            id,
                            kind,
                            label: if matches!(
                                kind,
                                PropKind::Wall | PropKind::Door | PropKind::Window
                            ) {
                                String::new()
                            } else {
                                kind.label().to_string()
                            },
                            x: state.pan.x + offset,
                            y: state.pan.y - 1.4 + offset * 0.5,
                            w,
                            h,
                            angle_deg: 0.0,
                        });
                        state.selected = Some(PlanSelection::Prop(id));
                        changed = true;
                    }
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;

                    if tool_chip(ui, "Fit", false, "Fit floor plan to viewport").clicked() {
                        fit_requested = true;
                    }
                    if tool_chip(
                        ui,
                        "Auto-Layout",
                        false,
                        "Add any missing scene cast & shot cameras",
                    )
                    .clicked()
                    {
                        project
                            .shots
                            .auto_populate_floor_plan(scene, &project.blocks);
                        fit_requested = true;
                        changed = true;
                    }
                    if tool_chip(
                        ui,
                        "180° Line",
                        state.show_axis_line,
                        "Toggle 180-degree Line of Action axis",
                    )
                    .clicked()
                    {
                        state.show_axis_line = !state.show_axis_line;
                    }
                });
            });
        })
        .response;

    ui.painter().line_segment(
        [
            Pos2::new(bar_resp.rect.left(), bar_resp.rect.bottom()),
            Pos2::new(bar_resp.rect.right(), bar_resp.rect.bottom()),
        ],
        Stroke::new(1.0_f32, kit().sep),
    );

    // Reserve bottom bar height when something is selected
    let has_selection = state.selected.is_some();
    let bottom_bar_h = if has_selection { 40.0 } else { 0.0 };
    let canvas_h = (ui.available_height() - bottom_bar_h).max(200.0);
    let canvas_size = Vec2::new(ui.available_width().max(240.0), canvas_h);

    // Auto-fit once on narrow split viewports when zoom is still untouched at default
    let auto_fit_narrow =
        !fit_requested && canvas_size.x < 460.0 && (state.zoom - DEFAULT_ZOOM).abs() < 0.01;
    if fit_requested || auto_fit_narrow {
        if let Some(plan) = project.shots.plan_for_scene(scene_id) {
            state.fit_to_plan(plan, canvas_size);
        }
    }

    // 2. Interactive 2D Top-Down Blueprint Canvas
    let (canvas_rect, resp) = ui.allocate_exact_size(canvas_size, Sense::click_and_drag());
    let center = canvas_rect.center();

    // Smooth wheel zoom when hovering canvas
    if resp.hovered() {
        let scroll_y = ui.input(|i| i.raw_scroll_delta.y);
        if scroll_y.abs() > 0.1 {
            let factor = (1.0 + scroll_y * 0.0025).clamp(0.85, 1.18);
            state.zoom = (state.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        }
    }

    // Handle pointer interactions (hit-testing & dragging)
    let ptr_pos = ui.input(|i| i.pointer.interact_pos());
    let primary_pressed = ui.input(|i| i.pointer.primary_pressed());
    let primary_down = ui.input(|i| i.pointer.primary_down());

    if !primary_down {
        state.drag = None;
        state.last_pan_pos = None;
    }

    if primary_pressed {
        if let Some(pos) = ptr_pos {
            if canvas_rect.contains(pos) {
                let hit = hit_test_plan(
                    project,
                    scene_id,
                    center,
                    state.pan,
                    state.zoom,
                    state.selected,
                    pos,
                );
                match hit {
                    Some((sel, drag_kind, offset)) => {
                        state.selected = Some(sel);
                        state.drag = Some(drag_kind);
                        state.drag_offset = offset;
                        if let PlanSelection::Camera(sid) = sel {
                            *selected_shot = Some(sid);
                        }
                        ui.ctx().request_repaint();
                    }
                    None => {
                        state.selected = None;
                        state.drag = Some(DragKind::Pan);
                        state.last_pan_pos = Some(pos);
                        ui.ctx().request_repaint();
                    }
                }
            }
        }
    } else if primary_down {
        if let (Some(drag), Some(pos)) = (state.drag, ptr_pos) {
            let wpos = screen_to_world(center, state.pan, state.zoom, pos);
            match drag {
                DragKind::Pan => {
                    if let Some(prev) = state.last_pan_pos {
                        let delta = pos - prev;
                        state.pan -= delta / state.zoom;
                    }
                    state.last_pan_pos = Some(pos);
                }
                DragKind::MoveCamera(sid) => {
                    if let Some(plan) = project.shots.plan_for_scene_mut(scene_id) {
                        if let Some(cam) = plan.cameras.iter_mut().find(|c| c.shot_id == sid) {
                            let dx = (wpos.x + state.drag_offset.x) - cam.x;
                            let dy = (wpos.y + state.drag_offset.y) - cam.y;
                            cam.x += dx;
                            cam.y += dy;
                            if let Some((tx, ty)) = cam.track_to.as_mut() {
                                *tx += dx;
                                *ty += dy;
                            }
                            changed = true;
                        }
                    }
                }
                DragKind::AimCamera(sid) => {
                    if let Some(plan) = project.shots.plan_for_scene_mut(scene_id) {
                        if let Some(cam) = plan.cameras.iter_mut().find(|c| c.shot_id == sid) {
                            cam.angle_deg = (wpos.y - cam.y).atan2(wpos.x - cam.x).to_degrees();
                            changed = true;
                        }
                    }
                }
                DragKind::MoveCameraTrack(sid) => {
                    if let Some(plan) = project.shots.plan_for_scene_mut(scene_id) {
                        if let Some(cam) = plan.cameras.iter_mut().find(|c| c.shot_id == sid) {
                            cam.track_to = Some((wpos.x, wpos.y));
                            changed = true;
                        }
                    }
                }
                DragKind::MoveActor(aid) => {
                    if let Some(plan) = project.shots.plan_for_scene_mut(scene_id) {
                        if let Some(actor) = plan.actors.iter_mut().find(|a| a.id == aid) {
                            let dx = (wpos.x + state.drag_offset.x) - actor.x;
                            let dy = (wpos.y + state.drag_offset.y) - actor.y;
                            actor.x += dx;
                            actor.y += dy;
                            if let Some((mx, my)) = actor.move_to.as_mut() {
                                *mx += dx;
                                *my += dy;
                            }
                            changed = true;
                        }
                    }
                }
                DragKind::AimActor(aid) => {
                    if let Some(plan) = project.shots.plan_for_scene_mut(scene_id) {
                        if let Some(actor) = plan.actors.iter_mut().find(|a| a.id == aid) {
                            actor.angle_deg =
                                (wpos.y - actor.y).atan2(wpos.x - actor.x).to_degrees();
                            changed = true;
                        }
                    }
                }
                DragKind::MoveActorTarget(aid) => {
                    if let Some(plan) = project.shots.plan_for_scene_mut(scene_id) {
                        if let Some(actor) = plan.actors.iter_mut().find(|a| a.id == aid) {
                            actor.move_to = Some((wpos.x, wpos.y));
                            changed = true;
                        }
                    }
                }
                DragKind::MoveProp(pid) => {
                    if let Some(plan) = project.shots.plan_for_scene_mut(scene_id) {
                        if let Some(prop) = plan.props.iter_mut().find(|p| p.id == pid) {
                            prop.x = wpos.x + state.drag_offset.x;
                            prop.y = wpos.y + state.drag_offset.y;
                            changed = true;
                        }
                    }
                }
                DragKind::RotateProp(pid) => {
                    if let Some(plan) = project.shots.plan_for_scene_mut(scene_id) {
                        if let Some(prop) = plan.props.iter_mut().find(|p| p.id == pid) {
                            prop.angle_deg = (wpos.y - prop.y).atan2(wpos.x - prop.x).to_degrees();
                            changed = true;
                        }
                    }
                }
            }
            ui.ctx().request_repaint();
        }
    }

    // Paint the entire 2D architectural canvas inside a clipped painter
    let p = ui.painter_at(canvas_rect);
    p.rect_filled(canvas_rect, CornerRadius::ZERO, kit().page);
    paint_grid(&p, canvas_rect, center, state.pan, state.zoom);

    if let Some(plan) = project.shots.plan_for_scene(scene_id) {
        // 180-degree Line of Action between first two actors
        if state.show_axis_line && plan.actors.len() >= 2 {
            paint_axis_line(
                &p,
                canvas_rect,
                center,
                state.pan,
                state.zoom,
                &plan.actors[0],
                &plan.actors[1],
            );
        }

        // Layer 1: Set architecture & props
        for prop in &plan.props {
            let sel = state.selected == Some(PlanSelection::Prop(prop.id));
            paint_prop(&p, center, state.pan, state.zoom, prop, sel);
        }

        // Layer 2: Actors
        for actor in &plan.actors {
            let sel = state.selected == Some(PlanSelection::Actor(actor.id));
            paint_actor(&p, center, state.pan, state.zoom, actor, sel);
        }

        // Layer 3: Cameras + physical FOV cones (selected camera painted last on top)
        let mut cam_order: Vec<usize> = (0..plan.cameras.len()).collect();
        cam_order.sort_by_key(|&i| {
            let sid = plan.cameras[i].shot_id;
            usize::from(
                state.selected == Some(PlanSelection::Camera(sid)) || *selected_shot == Some(sid),
            )
        });
        for ci in cam_order {
            let cam = &plan.cameras[ci];
            let Some(shot) = project.shots.get(cam.shot_id) else {
                continue;
            };
            let sel = state.selected == Some(PlanSelection::Camera(cam.shot_id))
                || *selected_shot == Some(cam.shot_id);
            paint_camera(&p, center, state.pan, state.zoom, cam, shot, sel);
        }
    }

    // Architectural scale indicator in bottom-left corner of canvas
    paint_scale_bar(&p, canvas_rect, state.zoom);

    // 3. Bottom contextual property strip for the selected canvas item
    if has_selection {
        let mut delete_selected = false;
        let bot_resp = egui::Frame::new()
            .fill(kit().surface)
            .inner_margin(Margin::symmetric(12, 6))
            .stroke(Stroke::NONE)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.set_height(26.0);
                    ui.spacing_mut().item_spacing.x = 8.0;

                    match state.selected {
                        Some(PlanSelection::Camera(sid)) => {
                            if let Some(shot) = project.shots.get(sid) {
                                let code = shot.code.clone();
                                let lens = shot.lens.clone();
                                let fov = project
                                    .shots
                                    .plan_for_scene(scene_id)
                                    .and_then(|pl| pl.cameras.iter().find(|c| c.shot_id == sid))
                                    .and_then(|c| c.fov_deg)
                                    .unwrap_or_else(|| lens_to_fov_deg(&lens));
                                let col = shot_color32(shot.color);
                                let (dot, _) =
                                    ui.allocate_exact_size(Vec2::new(10.0, 10.0), Sense::hover());
                                ui.painter().circle_filled(dot.center(), 4.5, col);
                                let code_display: String = if code.chars().count() > 22 {
                                    let mut s: String = code.chars().take(21).collect();
                                    s.push('…');
                                    s
                                } else {
                                    code.clone()
                                };
                                ui.label(
                                    RichText::new(format!(
                                        "Camera {code_display}  ·  {}  ·  {:.0}° FOV",
                                        if lens.trim().is_empty() {
                                            "35mm"
                                        } else {
                                            lens.trim()
                                        },
                                        fov
                                    ))
                                    .size(12.0)
                                    .strong()
                                    .color(kit().heading_text),
                                );

                                let has_track = project
                                    .shots
                                    .plan_for_scene(scene_id)
                                    .and_then(|pl| pl.cameras.iter().find(|c| c.shot_id == sid))
                                    .is_some_and(|c| c.track_to.is_some());

                                if tool_chip(
                                    ui,
                                    if has_track {
                                        "Clear Dolly Track"
                                    } else {
                                        "+ Dolly Track"
                                    },
                                    has_track,
                                    "Toggle camera movement track arrow",
                                )
                                .clicked()
                                {
                                    if let Some(plan) = project.shots.plan_for_scene_mut(scene_id) {
                                        if let Some(cam) =
                                            plan.cameras.iter_mut().find(|c| c.shot_id == sid)
                                        {
                                            if cam.track_to.is_some() {
                                                cam.track_to = None;
                                            } else {
                                                let rad = cam.angle_deg.to_radians();
                                                cam.track_to = Some((
                                                    cam.x + rad.cos() * 1.4,
                                                    cam.y + rad.sin() * 1.4,
                                                ));
                                            }
                                            changed = true;
                                        }
                                    }
                                }

                                if ui.available_width() > 235.0 {
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.label(
                                            RichText::new(
                                                "Drag camera or aim dot · Edit lens in Inspector",
                                            )
                                            .size(11.0)
                                            .color(kit().disabled_text),
                                        );
                                    });
                                }
                            }
                        }
                        Some(PlanSelection::Actor(aid)) => {
                            if let Some(plan) = project.shots.plan_for_scene_mut(scene_id) {
                                if let Some(actor) = plan.actors.iter_mut().find(|a| a.id == aid) {
                                    ui.label(
                                        RichText::new("Actor")
                                            .size(11.5)
                                            .color(kit().disabled_text),
                                    );
                                    let r = ui.add_sized(
                                        Vec2::new(130.0, 24.0),
                                        TextEdit::singleline(&mut actor.name),
                                    );
                                    if r.changed() {
                                        changed = true;
                                    }
                                    let has_move = actor.move_to.is_some();
                                    if tool_chip(
                                        ui,
                                        if has_move {
                                            "Clear Move Path"
                                        } else {
                                            "+ Move Path"
                                        },
                                        has_move,
                                        "Toggle actor blocking movement arrow",
                                    )
                                    .clicked()
                                    {
                                        if actor.move_to.is_some() {
                                            actor.move_to = None;
                                        } else {
                                            let rad = actor.angle_deg.to_radians();
                                            actor.move_to = Some((
                                                actor.x + rad.cos() * 1.5,
                                                actor.y + rad.sin() * 1.5,
                                            ));
                                        }
                                        changed = true;
                                    }

                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if danger_chip(ui, "Delete Actor").clicked() {
                                            delete_selected = true;
                                        }
                                    });
                                }
                            }
                        }
                        Some(PlanSelection::Prop(pid)) => {
                            if let Some(plan) = project.shots.plan_for_scene_mut(scene_id) {
                                if let Some(prop) = plan.props.iter_mut().find(|p| p.id == pid) {
                                    ui.label(
                                        RichText::new(prop.kind.label())
                                            .size(11.5)
                                            .strong()
                                            .color(kit().heading_text),
                                    );
                                    let r = ui.add_sized(
                                        Vec2::new(110.0, 24.0),
                                        TextEdit::singleline(&mut prop.label).hint_text("Label..."),
                                    );
                                    if r.changed() {
                                        changed = true;
                                    }
                                    ui.label(
                                        RichText::new("W").size(11.0).color(kit().disabled_text),
                                    );
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut prop.w)
                                                .speed(0.1)
                                                .range(0.2..=20.0)
                                                .suffix("m"),
                                        )
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                    ui.label(
                                        RichText::new("H").size(11.0).color(kit().disabled_text),
                                    );
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut prop.h)
                                                .speed(0.1)
                                                .range(0.15..=20.0)
                                                .suffix("m"),
                                        )
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                    if tool_chip(ui, "Rotate 45°", false, "Rotate prop 45 degrees")
                                        .clicked()
                                    {
                                        prop.angle_deg = (prop.angle_deg + 45.0) % 360.0;
                                        changed = true;
                                    }
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if danger_chip(ui, "Delete Prop").clicked() {
                                            delete_selected = true;
                                        }
                                    });
                                }
                            }
                        }
                        None => {}
                    }
                });
            })
            .response;
        ui.painter().line_segment(
            [
                Pos2::new(bot_resp.rect.left(), bot_resp.rect.top()),
                Pos2::new(bot_resp.rect.right(), bot_resp.rect.top()),
            ],
            Stroke::new(1.0_f32, kit().sep),
        );

        if delete_selected {
            if let Some(plan) = project.shots.plan_for_scene_mut(scene_id) {
                match state.selected {
                    Some(PlanSelection::Actor(aid)) => {
                        plan.actors.retain(|a| a.id != aid);
                        state.selected = None;
                        changed = true;
                    }
                    Some(PlanSelection::Prop(pid)) => {
                        plan.props.retain(|p| p.id != pid);
                        state.selected = None;
                        changed = true;
                    }
                    _ => {}
                }
            }
        }
    }

    changed
}

fn tool_chip(ui: &mut Ui, label: &str, active: bool, tip: &str) -> egui::Response {
    let btn = egui::Button::new(RichText::new(label).size(11.5).color(if active {
        kit().accent_text
    } else {
        kit().text
    }))
    .fill(if active {
        kit().accent_soft
    } else {
        kit().button
    })
    .stroke(Stroke::new(
        1.0_f32,
        if active { kit().accent } else { kit().stroke },
    ))
    .corner_radius(CornerRadius::same(5))
    .min_size(Vec2::new(0.0, 24.0));
    ui.add(btn)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(tip)
}

fn danger_chip(ui: &mut Ui, label: &str) -> egui::Response {
    let btn = egui::Button::new(RichText::new(label).size(11.5).color(kit().spell_error))
        .fill(kit().base)
        .stroke(Stroke::new(1.0_f32, kit().stroke))
        .corner_radius(CornerRadius::same(5))
        .min_size(Vec2::new(0.0, 24.0));
    ui.add(btn).on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn hit_test_plan(
    project: &Project,
    scene_id: Uuid,
    center: Pos2,
    pan: Vec2,
    zoom: f32,
    selected: Option<PlanSelection>,
    pos: Pos2,
) -> Option<(PlanSelection, DragKind, Vec2)> {
    let plan = project.shots.plan_for_scene(scene_id)?;
    let wpos = screen_to_world(center, pan, zoom, pos);

    // 1. Check active selection handles first (aim / rotate / motion endpoints)
    match selected {
        Some(PlanSelection::Camera(sid)) => {
            if let Some(cam) = plan.cameras.iter().find(|c| c.shot_id == sid) {
                let rad = cam.angle_deg.to_radians();
                let aim_p = world_to_screen(
                    center,
                    pan,
                    zoom,
                    cam.x + rad.cos() * 1.45,
                    cam.y + rad.sin() * 1.45,
                );
                if aim_p.distance(pos) <= 13.0 {
                    return Some((
                        PlanSelection::Camera(sid),
                        DragKind::AimCamera(sid),
                        Vec2::ZERO,
                    ));
                }
                if let Some((tx, ty)) = cam.track_to {
                    let tp = world_to_screen(center, pan, zoom, tx, ty);
                    if tp.distance(pos) <= 14.0 {
                        return Some((
                            PlanSelection::Camera(sid),
                            DragKind::MoveCameraTrack(sid),
                            Vec2::ZERO,
                        ));
                    }
                }
            }
        }
        Some(PlanSelection::Actor(aid)) => {
            if let Some(actor) = plan.actors.iter().find(|a| a.id == aid) {
                let rad = actor.angle_deg.to_radians();
                let aim_p = world_to_screen(
                    center,
                    pan,
                    zoom,
                    actor.x + rad.cos() * 0.72,
                    actor.y + rad.sin() * 0.72,
                );
                if aim_p.distance(pos) <= 12.0 {
                    return Some((
                        PlanSelection::Actor(aid),
                        DragKind::AimActor(aid),
                        Vec2::ZERO,
                    ));
                }
                if let Some((mx, my)) = actor.move_to {
                    let mp = world_to_screen(center, pan, zoom, mx, my);
                    if mp.distance(pos) <= 14.0 {
                        return Some((
                            PlanSelection::Actor(aid),
                            DragKind::MoveActorTarget(aid),
                            Vec2::ZERO,
                        ));
                    }
                }
            }
        }
        Some(PlanSelection::Prop(pid)) => {
            if let Some(prop) = plan.props.iter().find(|p| p.id == pid) {
                let rad = prop.angle_deg.to_radians();
                let handle_dist = prop.w * 0.5 + 0.55;
                let hp = world_to_screen(
                    center,
                    pan,
                    zoom,
                    prop.x + rad.cos() * handle_dist,
                    prop.y + rad.sin() * handle_dist,
                );
                if hp.distance(pos) <= 13.0 {
                    return Some((
                        PlanSelection::Prop(pid),
                        DragKind::RotateProp(pid),
                        Vec2::ZERO,
                    ));
                }
            }
        }
        None => {}
    }

    // 2. Cameras
    for cam in plan.cameras.iter().rev() {
        let cp = world_to_screen(center, pan, zoom, cam.x, cam.y);
        if cp.distance(pos) <= 20.0 {
            return Some((
                PlanSelection::Camera(cam.shot_id),
                DragKind::MoveCamera(cam.shot_id),
                Vec2::new(cam.x - wpos.x, cam.y - wpos.y),
            ));
        }
    }

    // 3. Actors
    for actor in plan.actors.iter().rev() {
        let ap = world_to_screen(center, pan, zoom, actor.x, actor.y);
        if ap.distance(pos) <= 19.0 {
            return Some((
                PlanSelection::Actor(actor.id),
                DragKind::MoveActor(actor.id),
                Vec2::new(actor.x - wpos.x, actor.y - wpos.y),
            ));
        }
    }

    // 4. Props (oriented bounding box in world coordinates)
    for prop in plan.props.iter().rev() {
        let local = rot2(
            wpos.x - prop.x,
            wpos.y - prop.y,
            -prop.angle_deg.to_radians(),
        );
        let pad_x = (10.0 / zoom).max(0.12);
        let pad_y = (10.0 / zoom).max(0.12);
        if local.x.abs() <= prop.w * 0.5 + pad_x && local.y.abs() <= prop.h * 0.5 + pad_y {
            return Some((
                PlanSelection::Prop(prop.id),
                DragKind::MoveProp(prop.id),
                Vec2::new(prop.x - wpos.x, prop.y - wpos.y),
            ));
        }
    }

    None
}

fn paint_grid(p: &egui::Painter, rect: Rect, center: Pos2, pan: Vec2, zoom: f32) {
    let tl = screen_to_world(center, pan, zoom, rect.min);
    let br = screen_to_world(center, pan, zoom, rect.max);

    let minor_col = kit()
        .sep
        .gamma_multiply(if kit_is_light() { 0.45 } else { 0.35 });
    let major_col = kit()
        .sep
        .gamma_multiply(if kit_is_light() { 0.85 } else { 0.72 });

    // Minor 0.5m grid when zoomed in enough
    if zoom >= 36.0 {
        let mut x = (tl.x * 2.0).floor() * 0.5;
        while x <= br.x {
            let sx = center.x + (x - pan.x) * zoom;
            p.line_segment(
                [Pos2::new(sx, rect.top()), Pos2::new(sx, rect.bottom())],
                Stroke::new(0.7_f32, minor_col),
            );
            x += 0.5;
        }
        let mut y = (tl.y * 2.0).floor() * 0.5;
        while y <= br.y {
            let sy = center.y + (y - pan.y) * zoom;
            p.line_segment(
                [Pos2::new(rect.left(), sy), Pos2::new(rect.right(), sy)],
                Stroke::new(0.7_f32, minor_col),
            );
            y += 0.5;
        }
    }

    // Major 1m grid
    let mut x = tl.x.floor();
    while x <= br.x {
        let sx = center.x + (x - pan.x) * zoom;
        p.line_segment(
            [Pos2::new(sx, rect.top()), Pos2::new(sx, rect.bottom())],
            Stroke::new(1.0_f32, major_col),
        );
        x += 1.0;
    }
    let mut y = tl.y.floor();
    while y <= br.y {
        let sy = center.y + (y - pan.y) * zoom;
        p.line_segment(
            [Pos2::new(rect.left(), sy), Pos2::new(rect.right(), sy)],
            Stroke::new(1.0_f32, major_col),
        );
        y += 1.0;
    }
}

fn paint_axis_line(
    p: &egui::Painter,
    rect: Rect,
    center: Pos2,
    pan: Vec2,
    zoom: f32,
    a: &PlanActor,
    b: &PlanActor,
) {
    let pa = world_to_screen(center, pan, zoom, a.x, a.y);
    let pb = world_to_screen(center, pan, zoom, b.x, b.y);
    let dir = pb - pa;
    if dir.length() < 4.0 {
        return;
    }
    let u = dir.normalized();
    let span = rect.width().hypot(rect.height());
    let start = pa - u * span;
    let end = pb + u * span;

    let col = Color32::from_rgba_unmultiplied(239, 68, 68, if kit_is_light() { 115 } else { 105 });
    draw_dashed_line(p, start, end, 7.0, 6.0, Stroke::new(1.2_f32, col));

    // Crisp "180° AXIS" pill near midpoint offset slightly
    let mid = Pos2::new((pa.x + pb.x) * 0.5, (pa.y + pb.y) * 0.5);
    let normal = Vec2::new(-u.y, u.x);
    let tag_pos = mid + normal * 14.0;
    let g = p.layout_no_wrap("180° AXIS".into(), FontId::monospace(9.5), col);
    let pill = Rect::from_center_size(tag_pos, g.size() + Vec2::new(8.0, 4.0));
    p.rect_filled(pill, CornerRadius::same(3), kit().page.gamma_multiply(0.92));
    p.galley(
        Pos2::new(
            pill.center().x - g.size().x * 0.5,
            pill.center().y - g.size().y * 0.5,
        ),
        g,
        Color32::TRANSPARENT,
    );
}

fn draw_dashed_line(p: &egui::Painter, a: Pos2, b: Pos2, dash: f32, gap: f32, stroke: Stroke) {
    let delta = b - a;
    let len = delta.length();
    if len < 0.5 {
        return;
    }
    let dir = delta / len;
    let step = dash + gap;
    let mut t = 0.0_f32;
    while t < len {
        let t1 = (t + dash).min(len);
        p.line_segment([a + dir * t, a + dir * t1], stroke);
        t += step;
    }
}

fn draw_arrow_head(p: &egui::Painter, tip: Pos2, dir: Vec2, size: f32, color: Color32) {
    let u = dir.normalized();
    let perp = Vec2::new(-u.y, u.x);
    let back = tip - u * size;
    p.add(Shape::convex_polygon(
        vec![
            tip,
            back + perp * (size * 0.55),
            back - perp * (size * 0.55),
        ],
        color,
        Stroke::NONE,
    ));
}

fn paint_prop(
    p: &egui::Painter,
    center: Pos2,
    pan: Vec2,
    zoom: f32,
    prop: &PlanProp,
    selected: bool,
) {
    let rad = prop.angle_deg.to_radians();
    let to_pt = |lx: f32, ly: f32| -> Pos2 {
        let r = rot2(lx, ly, rad);
        world_to_screen(center, pan, zoom, prop.x + r.x, prop.y + r.y)
    };
    let hw = prop.w * 0.5;
    let hh = prop.h * 0.5;
    let corners = vec![
        to_pt(-hw, -hh),
        to_pt(hw, -hh),
        to_pt(hw, hh),
        to_pt(-hw, hh),
    ];

    let stroke_col = if selected {
        kit().accent
    } else {
        kit().disabled_text.gamma_multiply(0.85)
    };
    let stroke_w = if selected { 2.0_f32 } else { 1.3_f32 };

    match prop.kind {
        PropKind::Wall => {
            let fill = if kit_is_light() {
                Color32::from_rgb(72, 78, 90)
            } else {
                Color32::from_rgb(104, 112, 128)
            };
            p.add(Shape::convex_polygon(
                corners,
                fill,
                Stroke::new(stroke_w, stroke_col),
            ));
        }
        PropKind::Window => {
            let glass = Color32::from_rgba_unmultiplied(96, 180, 255, 65);
            p.add(Shape::convex_polygon(
                corners,
                glass,
                Stroke::new(stroke_w, stroke_col),
            ));
            p.line_segment(
                [to_pt(-hw, 0.0), to_pt(hw, 0.0)],
                Stroke::new(1.1_f32, Color32::from_rgb(96, 180, 255)),
            );
        }
        PropKind::Door => {
            // Threshold ticks
            p.line_segment(
                [to_pt(-hw, -hh), to_pt(-hw, hh)],
                Stroke::new(2.0_f32, stroke_col),
            );
            p.line_segment(
                [to_pt(hw, -hh), to_pt(hw, hh)],
                Stroke::new(2.0_f32, stroke_col),
            );
            // Open door leaf swung 65 degrees
            let hinge = to_pt(-hw, 0.0);
            let swing_rad = rad - 65.0_f32.to_radians();
            let leaf_end = world_to_screen(
                center,
                pan,
                zoom,
                prop.x - hw * rad.cos() + prop.w * swing_rad.cos(),
                prop.y - hw * rad.sin() + prop.w * swing_rad.sin(),
            );
            p.line_segment([hinge, leaf_end], Stroke::new(2.4_f32, stroke_col));
            // Quarter swing arc
            let mut arc_pts = Vec::with_capacity(13);
            for k in 0..=12 {
                let a = rad - (65.0_f32 * k as f32 / 12.0).to_radians();
                arc_pts.push(world_to_screen(
                    center,
                    pan,
                    zoom,
                    prop.x - hw * rad.cos() + prop.w * a.cos(),
                    prop.y - hw * rad.sin() + prop.w * a.sin(),
                ));
            }
            p.add(Shape::line(
                arc_pts,
                Stroke::new(1.0_f32, stroke_col.gamma_multiply(0.55)),
            ));
        }
        PropKind::Car => {
            // Overhead vehicle silhouette: body, windshield, roof, rear window, wheels
            let body_fill = if kit_is_light() {
                Color32::from_rgba_unmultiplied(110, 122, 140, 48)
            } else {
                Color32::from_rgba_unmultiplied(130, 146, 174, 52)
            };
            p.add(Shape::convex_polygon(
                corners,
                body_fill,
                Stroke::new(stroke_w, stroke_col),
            ));
            // Cabin / roof inner rect
            let cabin = vec![
                to_pt(-hw * 0.45, -hh * 0.78),
                to_pt(hw * 0.35, -hh * 0.78),
                to_pt(hw * 0.35, hh * 0.78),
                to_pt(-hw * 0.45, hh * 0.78),
            ];
            p.add(Shape::convex_polygon(
                cabin,
                kit().surface.gamma_multiply(0.85),
                Stroke::new(1.0_f32, stroke_col.gamma_multiply(0.8)),
            ));
            // Windshield + rear glass
            let windshield = vec![
                to_pt(hw * 0.35, -hh * 0.78),
                to_pt(hw * 0.58, -hh * 0.68),
                to_pt(hw * 0.58, hh * 0.68),
                to_pt(hw * 0.35, hh * 0.78),
            ];
            p.add(Shape::convex_polygon(
                windshield,
                Color32::from_rgba_unmultiplied(96, 180, 255, 65),
                Stroke::new(1.0_f32, stroke_col.gamma_multiply(0.75)),
            ));
            // Headlight indicators on front (+X)
            for sy in [-hh * 0.62, hh * 0.62] {
                p.circle_filled(
                    to_pt(hw * 0.96, sy),
                    (0.09 * zoom).clamp(2.5, 6.0),
                    Color32::from_rgb(245, 200, 90),
                );
            }
        }
        PropKind::Chair => {
            let fill = kit().surface;
            p.add(Shape::convex_polygon(
                corners,
                fill,
                Stroke::new(stroke_w, stroke_col),
            ));
            // Thick backrest on -X edge
            p.line_segment(
                [to_pt(-hw, -hh), to_pt(-hw, hh)],
                Stroke::new(3.2_f32, stroke_col),
            );
        }
        PropKind::Bed => {
            let fill = kit().surface;
            p.add(Shape::convex_polygon(
                corners,
                fill,
                Stroke::new(stroke_w, stroke_col),
            ));
            // Two pillows at -X end
            for py in [-hh * 0.45, hh * 0.45] {
                let pil = vec![
                    to_pt(-hw * 0.85, py - hh * 0.32),
                    to_pt(-hw * 0.45, py - hh * 0.32),
                    to_pt(-hw * 0.45, py + hh * 0.32),
                    to_pt(-hw * 0.85, py + hh * 0.32),
                ];
                p.add(Shape::convex_polygon(
                    pil,
                    kit().base,
                    Stroke::new(1.0_f32, stroke_col.gamma_multiply(0.7)),
                ));
            }
        }
        PropKind::Table | PropKind::Custom => {
            let fill = if kit_is_light() {
                Color32::from_rgba_unmultiplied(120, 128, 145, 36)
            } else {
                Color32::from_rgba_unmultiplied(150, 160, 180, 32)
            };
            p.add(Shape::convex_polygon(
                corners,
                fill,
                Stroke::new(stroke_w, stroke_col),
            ));
        }
    }

    // Optional label centered on prop
    if !prop.label.trim().is_empty() {
        let cpos = world_to_screen(center, pan, zoom, prop.x, prop.y);
        p.text(
            cpos,
            Align2::CENTER_CENTER,
            prop.label.trim(),
            FontId::proportional(10.5),
            kit().disabled_text,
        );
    }

    // Rotate handle when selected
    if selected {
        let handle = to_pt(hw + 0.55, 0.0);
        let edge = to_pt(hw, 0.0);
        p.line_segment([edge, handle], Stroke::new(1.2_f32, kit().accent));
        p.circle_filled(handle, 5.0, kit().accent);
        p.circle_stroke(handle, 5.0, Stroke::new(1.4_f32, kit().page));
    }
}

fn paint_actor(
    p: &egui::Painter,
    center: Pos2,
    pan: Vec2,
    zoom: f32,
    actor: &PlanActor,
    selected: bool,
) {
    let ap = world_to_screen(center, pan, zoom, actor.x, actor.y);
    let rad = actor.angle_deg.to_radians();
    let col = if kit_is_light() {
        Color32::from_rgb(56, 64, 78)
    } else {
        Color32::from_rgb(218, 224, 236)
    };
    let accent_col = shot_color32(actor.color);

    // Optional movement path arrow
    if let Some((mx, my)) = actor.move_to {
        let mp = world_to_screen(center, pan, zoom, mx, my);
        let dir = mp - ap;
        if dir.length() > 8.0 {
            draw_dashed_line(
                p,
                ap,
                mp - dir.normalized() * 10.0,
                5.0,
                4.0,
                Stroke::new(1.6_f32, accent_col),
            );
            draw_arrow_head(p, mp - dir.normalized() * 8.0, dir, 7.0, accent_col);
            p.circle_stroke(mp, 8.5, Stroke::new(1.5_f32, accent_col));
            if selected {
                p.circle_filled(mp, 4.0, accent_col);
            }
        }
    }

    // Overhead shoulders capsule (rotated perpendicular to facing direction)
    let fwd = Vec2::new(rad.cos(), rad.sin());
    let right = Vec2::new(-fwd.y, fwd.x);
    let sh_half = (0.28 * zoom).clamp(10.0, 22.0);
    let sh_thick = (0.12 * zoom).clamp(5.0, 10.0);
    let sh_pts = vec![
        ap - right * sh_half - fwd * (sh_thick * 0.7),
        ap + right * sh_half - fwd * (sh_thick * 0.7),
        ap + right * (sh_half * 0.85) + fwd * (sh_thick * 0.7),
        ap - right * (sh_half * 0.85) + fwd * (sh_thick * 0.7),
    ];
    p.add(Shape::convex_polygon(
        sh_pts,
        kit().surface,
        Stroke::new(
            if selected { 2.0_f32 } else { 1.4_f32 },
            if selected { kit().accent } else { accent_col },
        ),
    ));

    // Directional gaze nose triangle
    let nose_tip = ap + fwd * (sh_thick * 1.65);
    let nose_base = ap + fwd * (sh_thick * 0.65);
    p.add(Shape::convex_polygon(
        vec![
            nose_tip,
            nose_base + right * (sh_thick * 0.65),
            nose_base - right * (sh_thick * 0.65),
        ],
        if selected { kit().accent } else { accent_col },
        Stroke::NONE,
    ));

    // Circular head
    let head_r = (0.13 * zoom).clamp(5.5, 11.0);
    p.circle_filled(ap, head_r, col);
    p.circle_stroke(
        ap,
        head_r,
        Stroke::new(1.5_f32, if selected { kit().accent } else { kit().page }),
    );

    // Facing handle when selected
    if selected {
        let handle = world_to_screen(
            center,
            pan,
            zoom,
            actor.x + rad.cos() * 0.72,
            actor.y + rad.sin() * 0.72,
        );
        p.line_segment([nose_tip, handle], Stroke::new(1.2_f32, kit().accent));
        p.circle_filled(handle, 4.5, kit().accent);
        p.circle_stroke(handle, 4.5, Stroke::new(1.4_f32, kit().page));
    }

    // Name pill below actor
    let label = actor.name.trim();
    if !label.is_empty() {
        let g = p.layout_no_wrap(
            label.to_string(),
            FontId::proportional(10.5),
            kit().heading_text,
        );
        let pill = Rect::from_center_size(
            Pos2::new(ap.x, ap.y + sh_half + 10.0),
            g.size() + Vec2::new(8.0, 4.0),
        );
        p.rect_filled(pill, CornerRadius::same(4), kit().surface);
        p.rect_stroke(
            pill,
            CornerRadius::same(4),
            Stroke::new(1.0_f32, if selected { kit().accent } else { kit().stroke }),
            StrokeKind::Inside,
        );
        p.galley(
            Pos2::new(
                pill.center().x - g.size().x * 0.5,
                pill.center().y - g.size().y * 0.5,
            ),
            g,
            Color32::TRANSPARENT,
        );
    }
}

fn paint_camera(
    p: &egui::Painter,
    center: Pos2,
    pan: Vec2,
    zoom: f32,
    cam: &scriptcraft_core::shots::PlanCamera,
    shot: &scriptcraft_core::shots::Shot,
    selected: bool,
) {
    let cp = world_to_screen(center, pan, zoom, cam.x, cam.y);
    let rad = cam.angle_deg.to_radians();
    let col = shot_color32(shot.color);
    let fov_deg = cam.fov_deg.unwrap_or_else(|| lens_to_fov_deg(&shot.lens));
    let half_fov = (fov_deg * 0.5).to_radians();

    // 1. Live focal-length FOV cone
    let cone_dist_m = 3.4_f32;
    let steps = 10usize;
    let mut wedge = Vec::with_capacity(steps + 2);
    wedge.push(cp);
    for k in 0..=steps {
        let a = rad - half_fov + (2.0 * half_fov) * (k as f32 / steps as f32);
        wedge.push(world_to_screen(
            center,
            pan,
            zoom,
            cam.x + a.cos() * cone_dist_m,
            cam.y + a.sin() * cone_dist_m,
        ));
    }
    let cone_alpha = match (kit_is_light(), selected) {
        (true, true) => 56,
        (true, false) => 28,
        (false, true) => 54,
        (false, false) => 24,
    };
    let (r, g, b) = shot.color.rgb();
    let cone_fill = Color32::from_rgba_unmultiplied(r, g, b, cone_alpha);
    // Fan triangles from apex so even wide >90° cones stay strictly convex
    for k in 1..wedge.len() - 1 {
        p.add(Shape::convex_polygon(
            vec![cp, wedge[k], wedge[k + 1]],
            cone_fill,
            Stroke::NONE,
        ));
    }
    // Crisp FOV boundary rays
    let ray_stroke = Stroke::new(
        if selected { 1.5_f32 } else { 1.0_f32 },
        Color32::from_rgba_unmultiplied(r, g, b, if selected { 210 } else { 125 }),
    );
    p.line_segment([cp, wedge[1]], ray_stroke);
    p.line_segment([cp, *wedge.last().unwrap()], ray_stroke);
    // Dashed center optical axis when selected
    if selected {
        let axis_end = world_to_screen(
            center,
            pan,
            zoom,
            cam.x + rad.cos() * cone_dist_m,
            cam.y + rad.sin() * cone_dist_m,
        );
        draw_dashed_line(
            p,
            cp,
            axis_end,
            5.0,
            5.0,
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(r, g, b, 150)),
        );
    }

    // 2. Optional Dolly / Tracking path
    if let Some((tx, ty)) = cam.track_to {
        let tp = world_to_screen(center, pan, zoom, tx, ty);
        let dir = tp - cp;
        if dir.length() > 10.0 {
            let perp = Vec2::new(-dir.y, dir.x).normalized() * 4.0;
            // Classic twin dolly rails
            draw_dashed_line(
                p,
                cp + perp,
                tp + perp,
                6.0,
                4.0,
                Stroke::new(1.3_f32, col.gamma_multiply(0.8)),
            );
            draw_dashed_line(
                p,
                cp - perp,
                tp - perp,
                6.0,
                4.0,
                Stroke::new(1.3_f32, col.gamma_multiply(0.8)),
            );
            draw_arrow_head(p, tp, dir, 7.5, col);
            // Destination ghost camera ring
            p.circle_stroke(tp, 8.0, Stroke::new(1.5_f32, col));
            if selected {
                p.circle_filled(tp, 4.0, col);
            }
        }
    }

    // 3. Camera silhouette (body + front lens matte box)
    let fwd = Vec2::new(rad.cos(), rad.sin());
    let right = Vec2::new(-fwd.y, fwd.x);
    let bw = 11.0_f32;
    let bh = 8.0_f32;
    let body = vec![
        cp - fwd * bh - right * bw,
        cp + fwd * bh - right * bw,
        cp + fwd * bh + right * bw,
        cp - fwd * bh + right * bw,
    ];
    let hood = vec![
        cp + fwd * bh - right * 5.0,
        cp + fwd * (bh + 7.5) - right * 9.0,
        cp + fwd * (bh + 7.5) + right * 9.0,
        cp + fwd * bh + right * 5.0,
    ];
    if selected {
        p.circle_stroke(cp, 17.5, Stroke::new(2.0_f32, kit().heading_text));
    }
    p.add(Shape::convex_polygon(
        hood,
        col,
        Stroke::new(1.2_f32, kit().page),
    ));
    p.add(Shape::convex_polygon(
        body,
        col,
        Stroke::new(1.4_f32, kit().page),
    ));

    // Shot code centered right on the camera body (clamped so long custom names never overflow)
    let badge_code: String = if shot.code.chars().count() > 4 {
        let mut s: String = shot.code.chars().take(3).collect();
        s.push('…');
        s
    } else {
        shot.code.clone()
    };
    let g = p.layout_no_wrap(badge_code, FontId::monospace(9.5), ink_on_accent(col));
    p.galley(
        Pos2::new(cp.x - g.size().x * 0.5, cp.y - g.size().y * 0.5),
        g,
        Color32::TRANSPARENT,
    );

    // Compact lens tag below camera
    if !shot.lens.trim().is_empty() {
        p.text(
            Pos2::new(cp.x, cp.y + 18.0),
            Align2::CENTER_TOP,
            shot.lens.trim(),
            FontId::monospace(9.5),
            col,
        );
    }

    // 4. Interactive Aim Handle dot when selected
    if selected {
        let aim = world_to_screen(
            center,
            pan,
            zoom,
            cam.x + rad.cos() * 1.45,
            cam.y + rad.sin() * 1.45,
        );
        p.circle_filled(aim, 5.5, col);
        p.circle_stroke(aim, 5.5, Stroke::new(1.8_f32, kit().heading_text));
    }
}

fn paint_scale_bar(p: &egui::Painter, rect: Rect, zoom: f32) {
    let x0 = rect.left() + 14.0;
    let y0 = rect.bottom() - 16.0;
    let bar_w = zoom.clamp(24.0, 120.0);
    let col = kit().disabled_text;
    p.line_segment(
        [Pos2::new(x0, y0), Pos2::new(x0 + bar_w, y0)],
        Stroke::new(1.4_f32, col),
    );
    p.line_segment(
        [Pos2::new(x0, y0 - 4.0), Pos2::new(x0, y0 + 2.0)],
        Stroke::new(1.4_f32, col),
    );
    p.line_segment(
        [
            Pos2::new(x0 + bar_w, y0 - 4.0),
            Pos2::new(x0 + bar_w, y0 + 2.0),
        ],
        Stroke::new(1.4_f32, col),
    );
    p.text(
        Pos2::new(x0 + bar_w * 0.5, y0 - 6.0),
        Align2::CENTER_BOTTOM,
        "1 m",
        FontId::monospace(10.0),
        col,
    );
}
