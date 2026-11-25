//! Dedicated `Shots` phase workspace (Phase 1 of Shot Breakdown).
//!
//! Three-pane native `egui` studio layout:
//! - Left: Scene navigator with shot counts and coverage progress bars.
//! - Center: Interactive text-selection screenplay view (`Color Map` and
//!   Hollywood `Coverage Rails` modes). Drag through text (within a sentence or
//!   across blocks), double-click a word, or click an uncovered line to select
//!   beats, then assign a new shot or add overlapping coverage to an existing
//!   shot.
//! - Right: Scene shot list (reorder / delete / select) + Shot Inspector
//!   (code, 10-color palette, shot size, camera movement, lens presets,
//!   description, and exact covered text spans).

use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Id, Layout, Margin, Pos2, Rect, RichText,
    ScrollArea, Sense, Stroke, StrokeKind, TextEdit, Ui, Vec2,
};
use scriptcraft_core::shots::{block_index_map, ShotColor, ShotMovement, ShotRange, ShotSize};
use scriptcraft_core::{ElementType, Project, Scene, ScriptBlock};
use uuid::Uuid;

use crate::app::{bold_mono, MONO};
use crate::theme::{ink_on_accent, kit, kit_is_light, overline, paint_ellipsized};

const CUE_INDENT: f32 = scriptcraft_layout::FINAL_DRAFT_A4_GEOMETRY.cue_mm
    / scriptcraft_layout::FINAL_DRAFT_A4_TEXT_W_MM;
const PAREN_INDENT: f32 = scriptcraft_layout::FINAL_DRAFT_A4_GEOMETRY.paren_mm
    / scriptcraft_layout::FINAL_DRAFT_A4_TEXT_W_MM;
const DIALOGUE_INDENT: f32 = scriptcraft_layout::FINAL_DRAFT_A4_GEOMETRY.dialog_mm
    / scriptcraft_layout::FINAL_DRAFT_A4_TEXT_W_MM;

const LENS_PRESETS: &[&str] = &["24mm", "35mm", "50mm", "85mm"];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShotsViewMode {
    #[default]
    ColorMap,
    CoverageRails,
    FloorPlan,
    Split,
}

impl ShotsViewMode {
    pub const ALL: [Self; 4] = [
        Self::ColorMap,
        Self::CoverageRails,
        Self::FloorPlan,
        Self::Split,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::ColorMap => "Color Map",
            Self::CoverageRails => "Coverage Rails",
            Self::FloorPlan => "Floor Plan",
            Self::Split => "Split",
        }
    }

    fn is_color_map_script(self) -> bool {
        matches!(self, Self::ColorMap | Self::Split)
    }
}

/// Persistent UI state for the Shots workspace.
#[derive(Default)]
pub struct ShotsUiState {
    pub selected_scene: usize,
    pub selected_shot: Option<Uuid>,
    /// Selection anchor `(block_idx, char_offset)`.
    pub sel_anchor: Option<(usize, usize)>,
    /// Selection focus `(block_idx, char_offset)`.
    pub sel_focus: Option<(usize, usize)>,
    pub view_mode: ShotsViewMode,
    pub floor_plan: crate::floor_plan_view::FloorPlanUiState,
    dragging_selection: bool,
}

impl ShotsUiState {
    pub fn reset(&mut self) {
        self.selected_scene = 0;
        self.selected_shot = None;
        self.sel_anchor = None;
        self.sel_focus = None;
        self.dragging_selection = false;
        self.floor_plan.selected = None;
        self.floor_plan.reset_view();
    }

    /// Ordered non-empty character selection `((lo_block, lo_char), (hi_block, hi_char))`.
    pub fn selection_range(&self) -> Option<((usize, usize), (usize, usize))> {
        let a = self.sel_anchor?;
        let b = self.sel_focus.unwrap_or(a);
        if a == b {
            return None;
        }
        if a <= b {
            Some((a, b))
        } else {
            Some((b, a))
        }
    }

    /// Build a [`ShotRange`] from the current text selection. Whole-block
    /// selections store `ShotRange::blocks` so later appends stay covered;
    /// partial text selections store exact `ShotRange::chars`.
    pub fn selection_shot_range(&self, blocks: &[ScriptBlock]) -> Option<ShotRange> {
        let ((lo_b, lo_c), (hi_b, hi_c)) = self.selection_range()?;
        let sb = blocks.get(lo_b)?;
        let eb = blocks.get(hi_b)?;
        let hi_len = eb.text.chars().count();
        if lo_c == 0 && hi_c >= hi_len {
            Some(ShotRange::blocks(sb.id, eb.id))
        } else {
            Some(ShotRange::chars(sb.id, lo_c, eb.id, hi_c))
        }
    }

    /// Half-open `[c0, c1)` slice of `blocks[bi]` covered by the live selection.
    fn selection_in_block(&self, bi: usize, char_len: usize) -> Option<(usize, usize)> {
        let ((lo_b, lo_c), (hi_b, hi_c)) = self.selection_range()?;
        if bi < lo_b || bi > hi_b {
            return None;
        }
        let (c0, c1) = if lo_b == hi_b {
            (lo_c.min(char_len), hi_c.min(char_len))
        } else if bi == lo_b {
            (lo_c.min(char_len), char_len)
        } else if bi == hi_b {
            (0, hi_c.min(char_len))
        } else {
            (0, char_len)
        };
        if c0 < c1 {
            Some((c0, c1))
        } else {
            None
        }
    }

    pub fn clear_selection(&mut self) {
        self.sel_anchor = None;
        self.sel_focus = None;
        self.dragging_selection = false;
    }
}

/// Side-effect returned from one frame of [`show`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShotsAction {
    None,
    Changed,
    ExportColoredScriptPdf,
    ExportShotListPdf,
    ExportShotListCsv,
    OpenInScript(usize),
}

fn shot_color32(color: ShotColor) -> Color32 {
    let (r, g, b) = color.rgb();
    Color32::from_rgb(r, g, b)
}

fn shot_tint32(color: ShotColor, strong: bool) -> Color32 {
    let (r, g, b) = color.rgb();
    let alpha = match (kit_is_light(), strong) {
        (true, true) => 64,
        (true, false) => 42,
        (false, true) => 62,
        (false, false) => 38,
    };
    Color32::from_rgba_unmultiplied(r, g, b, alpha)
}

/// Render the full 3-pane Shots workspace inside `ctx`.
pub fn show(
    ctx: &egui::Context,
    project: &mut Project,
    scene_labels: &[String],
    state: &mut ShotsUiState,
    bold_character_cues: bool,
) -> ShotsAction {
    let mut action = ShotsAction::None;

    let scenes = project.scenes();
    if !scenes.is_empty() && state.selected_scene >= scenes.len() {
        state.selected_scene = scenes.len() - 1;
    }

    // Drop stale shot selection if deleted externally.
    if let Some(id) = state.selected_shot {
        if project.shots.get(id).is_none() {
            state.selected_shot = None;
        }
    }

    let char_coverage = project.shots.resolve_block_char_coverage(&project.blocks);
    let block_coverage: Vec<Vec<usize>> = char_coverage
        .iter()
        .map(|spans| {
            let mut ids = Vec::new();
            for &(si, _, _) in spans {
                if !ids.contains(&si) {
                    ids.push(si);
                }
            }
            ids
        })
        .collect();

    // Overall script coverage KPI
    let total_body_blocks = project
        .blocks
        .iter()
        .filter(|b| b.element_type != ElementType::SceneHeading && b.element_type.is_printable())
        .count();
    let covered_body_blocks = project
        .blocks
        .iter()
        .enumerate()
        .filter(|(bi, b)| {
            b.element_type != ElementType::SceneHeading
                && b.element_type.is_printable()
                && block_coverage.get(*bi).is_some_and(|v| !v.is_empty())
        })
        .count();
    let total_pct = (covered_body_blocks * 100)
        .checked_div(total_body_blocks)
        .unwrap_or(0);

    // 1. Top workspace header strip (view mode + KPI + export controls)
    egui::TopBottomPanel::top("shots_header")
        .exact_height(46.0)
        .frame(
            egui::Frame::new()
                .fill(kit().window)
                .inner_margin(Margin::symmetric(16, 8))
                .stroke(Stroke::NONE),
        )
        .show(ctx, |ui| {
            let bottom_y = ui.max_rect().bottom() + 8.0;
            ui.painter().line_segment(
                [
                    Pos2::new(ui.min_rect().left() - 16.0, bottom_y),
                    Pos2::new(ui.max_rect().right() + 16.0, bottom_y),
                ],
                Stroke::new(1.0_f32, kit().sep),
            );

            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;

                // Segmented view mode toggle
                egui::Frame::new()
                    .fill(kit().base)
                    .stroke(Stroke::new(1.0_f32, kit().stroke))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::same(2))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for mode in ShotsViewMode::ALL {
                            let active = state.view_mode == mode;
                            let fill = if active {
                                kit().accent_soft
                            } else {
                                Color32::TRANSPARENT
                            };
                            let text_col = if active {
                                kit().accent_text
                            } else {
                                kit().disabled_text
                            };
                            let btn = egui::Button::new(
                                RichText::new(mode.label())
                                    .size(12.0)
                                    .color(text_col)
                                    .strong(),
                            )
                            .fill(fill)
                            .stroke(Stroke::NONE)
                            .corner_radius(CornerRadius::same(4))
                            .min_size(Vec2::new(0.0, 24.0));
                            if ui.add(btn).clicked() {
                                state.view_mode = mode;
                            }
                        }
                    });

                // Coverage KPI pill
                egui::Frame::new()
                    .fill(kit().surface)
                    .stroke(Stroke::new(1.0_f32, kit().stroke))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::symmetric(10, 4))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(format!(
                                "{} {}  ·  {}% covered",
                                project.shots.len(),
                                if project.shots.len() == 1 {
                                    "shot"
                                } else {
                                    "shots"
                                },
                                total_pct
                            ))
                            .size(12.0)
                            .color(kit().text),
                        );
                    });

                // Right-aligned export actions
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;

                    let has_shots = !project.shots.is_empty();
                    let colored_btn = egui::Button::new(
                        RichText::new("Colored Script PDF…")
                            .size(12.0)
                            .color(if has_shots {
                                ink_on_accent(kit().accent)
                            } else {
                                kit().disabled_text
                            })
                            .strong(),
                    )
                    .fill(if has_shots {
                        kit().accent
                    } else {
                        kit().button
                    })
                    .stroke(Stroke::NONE)
                    .corner_radius(CornerRadius::same(6))
                    .min_size(Vec2::new(0.0, 26.0));
                    if ui
                        .add_enabled(has_shots, colored_btn)
                        .on_hover_text("Export screenplay PDF with shot highlights and legend")
                        .clicked()
                    {
                        action = ShotsAction::ExportColoredScriptPdf;
                    }

                    let list_btn =
                        egui::Button::new(RichText::new("Shot List PDF…").size(12.0).color(
                            if has_shots {
                                kit().text
                            } else {
                                kit().disabled_text
                            },
                        ))
                        .fill(kit().button)
                        .stroke(Stroke::new(1.0_f32, kit().stroke))
                        .corner_radius(CornerRadius::same(6))
                        .min_size(Vec2::new(0.0, 26.0));
                    if ui
                        .add_enabled(has_shots, list_btn)
                        .on_hover_text("Export formatted shot list table PDF grouped by scene")
                        .clicked()
                    {
                        action = ShotsAction::ExportShotListPdf;
                    }

                    let csv_btn =
                        egui::Button::new(RichText::new("CSV…").size(12.0).color(if has_shots {
                            kit().text
                        } else {
                            kit().disabled_text
                        }))
                        .fill(kit().button)
                        .stroke(Stroke::new(1.0_f32, kit().stroke))
                        .corner_radius(CornerRadius::same(6))
                        .min_size(Vec2::new(0.0, 26.0));
                    if ui
                        .add_enabled(has_shots, csv_btn)
                        .on_hover_text("Export shot list spreadsheet (UTF-8 CSV)")
                        .clicked()
                    {
                        action = ShotsAction::ExportShotListCsv;
                    }
                });
            });
        });

    // 2. Left Scene Navigator pane
    egui::SidePanel::left("shots_scenes")
        .default_width(236.0)
        .width_range(190.0..=320.0)
        .resizable(true)
        .frame(
            egui::Frame::new()
                .fill(kit().list_bg)
                .inner_margin(Margin::symmetric(10, 12))
                .stroke(Stroke::NONE),
        )
        .show(ctx, |ui| {
            let r = ui.max_rect();
            ui.painter().line_segment(
                [
                    Pos2::new(r.right() + 10.0, r.top() - 12.0),
                    Pos2::new(r.right() + 10.0, r.bottom() + 12.0),
                ],
                Stroke::new(1.0_f32, kit().sep),
            );

            ui.horizontal(|ui| {
                overline(ui, "SCENES");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{}", scenes.len()))
                            .size(11.0)
                            .color(kit().disabled_text),
                    );
                });
            });
            ui.add_space(6.0);

            ScrollArea::vertical()
                .id_salt("shots_scenes_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    for (si, scene) in scenes.iter().enumerate() {
                        let label = scene_labels
                            .get(si)
                            .cloned()
                            .unwrap_or_else(|| (si + 1).to_string());
                        let (shot_count, covered, total) = project.shots.scene_coverage_stats(
                            scene,
                            &project.blocks,
                            &block_coverage,
                        );
                        let pct = (covered * 100).checked_div(total).unwrap_or(0);
                        let active = state.selected_scene == si;

                        let row_resp =
                            scene_nav_row(ui, &label, &scene.heading, shot_count, pct, active);
                        if row_resp.clicked() && state.selected_scene != si {
                            state.selected_scene = si;
                            state.clear_selection();
                            state.floor_plan.selected = None;
                            state.floor_plan.reset_view();
                            let first_shot = project
                                .shots
                                .shots_for_scene(scene, &project.blocks)
                                .first()
                                .and_then(|&idx| project.shots.shots.get(idx))
                                .map(|s| s.id);
                            state.selected_shot = first_shot;
                        }
                    }
                    if scenes.is_empty() {
                        ui.label(
                            RichText::new("No scenes yet.")
                                .size(12.0)
                                .color(kit().disabled_text),
                        );
                    }
                });
        });

    let active_scene: Option<Scene> = scenes.get(state.selected_scene).cloned();
    let active_scene_label: String = scene_labels
        .get(state.selected_scene)
        .cloned()
        .unwrap_or_else(|| (state.selected_scene + 1).to_string());

    // 3. Right Shot List + Shot Inspector pane
    egui::SidePanel::right("shots_inspector")
        .default_width(340.0)
        .width_range(290.0..=440.0)
        .resizable(true)
        .frame(
            egui::Frame::new()
                .fill(kit().list_bg)
                .inner_margin(Margin::symmetric(12, 12))
                .stroke(Stroke::NONE),
        )
        .show(ctx, |ui| {
            let r = ui.max_rect();
            ui.painter().line_segment(
                [
                    Pos2::new(r.left() - 12.0, r.top() - 12.0),
                    Pos2::new(r.left() - 12.0, r.bottom() + 12.0),
                ],
                Stroke::new(1.0_f32, kit().sep),
            );

            let inspector_changed = show_right_pane(
                ui,
                project,
                active_scene.as_ref(),
                &active_scene_label,
                state,
            );
            if inspector_changed && action == ShotsAction::None {
                action = ShotsAction::Changed;
            }
        });

    // 4. Center Interactive Screenplay Coverage / 2D Floor Plan stage
    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(kit().desk)
                .inner_margin(Margin::same(0)),
        )
        .show(ctx, |ui| match state.view_mode {
            ShotsViewMode::ColorMap | ShotsViewMode::CoverageRails => {
                let center_action = show_center_pane(
                    ui,
                    project,
                    &scenes,
                    active_scene.as_ref(),
                    &active_scene_label,
                    &block_coverage,
                    &char_coverage,
                    state,
                    bold_character_cues,
                );
                if center_action != ShotsAction::None {
                    action = center_action;
                }
            }
            ShotsViewMode::FloorPlan => {
                let fp_changed = crate::floor_plan_view::show(
                    ui,
                    project,
                    active_scene.as_ref(),
                    &mut state.selected_shot,
                    &mut state.floor_plan,
                );
                if fp_changed && action == ShotsAction::None {
                    action = ShotsAction::Changed;
                }
            }
            ShotsViewMode::Split => {
                let avail = ui.available_rect_before_wrap();
                let left_w = (avail.width() * 0.48).max(260.0);
                let right_w = (avail.width() - left_w - 1.0).max(240.0);
                let left_rect = Rect::from_min_size(avail.min, Vec2::new(left_w, avail.height()));
                let div_x = left_rect.right();
                let right_rect = Rect::from_min_size(
                    Pos2::new(div_x + 1.0, avail.top()),
                    Vec2::new(right_w, avail.height()),
                );

                let mut left_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(left_rect)
                        .layout(Layout::top_down(Align::Min)),
                );
                left_ui.set_clip_rect(left_rect);
                let center_action = show_center_pane(
                    &mut left_ui,
                    project,
                    &scenes,
                    active_scene.as_ref(),
                    &active_scene_label,
                    &block_coverage,
                    &char_coverage,
                    state,
                    bold_character_cues,
                );
                if center_action != ShotsAction::None {
                    action = center_action;
                }

                ui.painter().line_segment(
                    [
                        Pos2::new(div_x, avail.top()),
                        Pos2::new(div_x, avail.bottom()),
                    ],
                    Stroke::new(1.0_f32, kit().sep),
                );

                let mut right_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(right_rect)
                        .layout(Layout::top_down(Align::Min)),
                );
                right_ui.set_clip_rect(right_rect);
                let fp_changed = crate::floor_plan_view::show(
                    &mut right_ui,
                    project,
                    active_scene.as_ref(),
                    &mut state.selected_shot,
                    &mut state.floor_plan,
                );
                if fp_changed && action == ShotsAction::None {
                    action = ShotsAction::Changed;
                }
            }
        });

    if action == ShotsAction::Changed {
        ctx.request_repaint();
    }

    action
}

fn scene_nav_row(
    ui: &mut Ui,
    label: &str,
    heading: &str,
    shot_count: usize,
    pct: usize,
    active: bool,
) -> egui::Response {
    let width = ui.available_width();
    let height = 54.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    resp.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            format!("Scene {label} {heading}"),
        )
    });

    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let bg = if active {
            kit().accent_soft
        } else if resp.hovered() {
            kit().hover_light
        } else {
            Color32::TRANSPARENT
        };
        if bg != Color32::TRANSPARENT {
            p.rect_filled(rect, CornerRadius::same(6), bg);
        }
        if active {
            let bar = Rect::from_min_size(
                Pos2::new(rect.left(), rect.top() + 6.0),
                Vec2::new(3.0, rect.height() - 12.0),
            );
            p.rect_filled(bar, CornerRadius::same(2), kit().accent);
        }

        let num_font = FontId::monospace(11.0);
        let num_galley = p.layout_no_wrap(
            label.to_string(),
            num_font,
            if active {
                kit().accent_text
            } else {
                kit().disabled_text
            },
        );
        let badge_w = (num_galley.size().x + 10.0).max(24.0);
        let badge_rect = Rect::from_min_size(
            Pos2::new(rect.left() + 10.0, rect.top() + 8.0),
            Vec2::new(badge_w, 18.0),
        );
        p.rect_filled(
            badge_rect,
            CornerRadius::same(4),
            if active { kit().surface } else { kit().base },
        );
        p.galley(
            Pos2::new(
                badge_rect.center().x - num_galley.size().x * 0.5,
                badge_rect.center().y - num_galley.size().y * 0.5,
            ),
            num_galley,
            Color32::TRANSPARENT,
        );

        let head_x = badge_rect.right() + 7.0;
        let head_w = (rect.right() - 10.0 - head_x).max(20.0);
        let head_text = if heading.trim().is_empty() {
            "UNTITLED SCENE"
        } else {
            heading.trim()
        };
        paint_ellipsized(
            p,
            Pos2::new(head_x, badge_rect.center().y),
            badge_rect.center().y,
            head_text,
            FontId::proportional(12.0),
            if active {
                kit().heading_text
            } else {
                kit().text
            },
            head_w,
        );

        let meta = format!(
            "{shot_count} {} · {pct}%",
            if shot_count == 1 { "shot" } else { "shots" }
        );
        let meta_galley = p.layout_no_wrap(meta, FontId::proportional(10.5), kit().disabled_text);
        let meta_pos = Pos2::new(
            rect.right() - 10.0 - meta_galley.size().x,
            rect.bottom() - 18.0,
        );
        let bar_left = rect.left() + 10.0;
        let bar_right = (meta_pos.x - 8.0).max(bar_left + 16.0);
        let bar_y = rect.bottom() - 12.5;
        let track = Rect::from_min_max(
            Pos2::new(bar_left, bar_y),
            Pos2::new(bar_right, bar_y + 3.5),
        );
        p.rect_filled(track, CornerRadius::same(2), kit().stroke);
        if pct > 0 {
            let frac = (pct.min(100) as f32) / 100.0;
            let fill = Rect::from_min_size(
                track.min,
                Vec2::new((track.width() * frac).max(4.0), track.height()),
            );
            let bar_col = if pct >= 100 {
                Color32::from_rgb(16, 185, 129)
            } else {
                kit().accent
            };
            p.rect_filled(fill, CornerRadius::same(2), bar_col);
        }
        p.galley(meta_pos, meta_galley, Color32::TRANSPARENT);
    }

    resp
}

fn show_right_pane(
    ui: &mut Ui,
    project: &mut Project,
    active_scene: Option<&Scene>,
    scene_label: &str,
    state: &mut ShotsUiState,
) -> bool {
    let mut changed = false;

    let scene_shot_indices: Vec<usize> = match active_scene {
        Some(sc) => project.shots.shots_for_scene(sc, &project.blocks),
        None => (0..project.shots.shots.len()).collect(),
    };

    ui.horizontal(|ui| {
        overline(
            ui,
            &format!("SCENE {} SHOTS", scene_label.trim_end_matches('.')),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(format!("{}", scene_shot_indices.len()))
                    .size(11.0)
                    .color(kit().disabled_text),
            );
        });
    });
    ui.add_space(6.0);

    let list_max_h = (ui.available_height() * 0.40).clamp(130.0, 320.0);
    let mut move_req: Option<(Uuid, isize)> = None;
    let mut delete_req: Option<Uuid> = None;

    ScrollArea::vertical()
        .id_salt("shots_list_scroll")
        .max_height(list_max_h)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            if scene_shot_indices.is_empty() {
                egui::Frame::new()
                    .fill(kit().base)
                    .stroke(Stroke::new(1.0_f32, kit().stroke))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::same(12))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("No shots in this scene yet.")
                                .size(12.5)
                                .strong()
                                .color(kit().text),
                        );
                        ui.add_space(3.0);
                        ui.label(
                            RichText::new(
                                "Highlight any words or lines in the script, then click + New Shot.",
                            )
                            .size(11.5)
                            .color(kit().disabled_text),
                        );
                    });
            } else {
                for (pos, &shot_idx) in scene_shot_indices.iter().enumerate() {
                    let Some(shot) = project.shots.shots.get(shot_idx) else {
                        continue;
                    };
                    let shot_id = shot.id;
                    let code = shot.code.clone();
                    let size_short = shot.size.short();
                    let movement_label = shot.movement.label();
                    let lens = shot.lens.clone();
                    let color = shot_color32(shot.color);
                    let preview = if !shot.description.trim().is_empty() {
                        shot.description.trim().replace('\n', " ")
                    } else {
                        project.shots.covered_excerpt(shot, &project.blocks)
                    };
                    let selected = state.selected_shot == Some(shot_id);

                    let border_col = if selected { kit().accent } else { kit().stroke };
                    let fill_col = if selected {
                        kit().accent_soft
                    } else {
                        kit().surface
                    };

                    let mut action_btns_rect = Rect::NOTHING;
                    let frame_resp = egui::Frame::new()
                        .fill(fill_col)
                        .stroke(Stroke::new(
                            if selected { 1.5_f32 } else { 1.0_f32 },
                            border_col,
                        ))
                        .corner_radius(CornerRadius::same(7))
                        .inner_margin(Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (dot_rect, _) =
                                    ui.allocate_exact_size(Vec2::new(10.0, 10.0), Sense::hover());
                                ui.painter().circle_filled(dot_rect.center(), 4.5, color);

                                ui.label(
                                    RichText::new(&code)
                                        .font(FontId::monospace(13.0))
                                        .strong()
                                        .color(kit().heading_text),
                                );

                                let mut spec = size_short.to_string();
                                if !lens.trim().is_empty() {
                                    spec.push_str(" · ");
                                    spec.push_str(lens.trim());
                                }
                                if movement_label != "Static" {
                                    spec.push_str(" · ");
                                    spec.push_str(movement_label);
                                }
                                ui.label(
                                    RichText::new(spec)
                                        .size(11.0)
                                        .color(kit().disabled_text),
                                );

                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.spacing_mut().item_spacing.x = 2.0;
                                    let del = ui
                                        .add(
                                            egui::Button::new(
                                                RichText::new("x")
                                                    .size(11.0)
                                                    .color(kit().disabled_text),
                                            )
                                            .fill(Color32::TRANSPARENT)
                                            .stroke(Stroke::NONE)
                                            .min_size(Vec2::new(18.0, 18.0)),
                                        )
                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                        .on_hover_text(format!("Delete shot {code}"));
                                    del.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::WidgetType::Button,
                                            true,
                                            format!("Delete {code}"),
                                        )
                                    });
                                    if del.clicked() {
                                        delete_req = Some(shot_id);
                                    }

                                    let can_down = pos + 1 < scene_shot_indices.len();
                                    let dn = ui
                                        .add_enabled(
                                            can_down,
                                            egui::Button::new(
                                                RichText::new("v")
                                                    .size(10.5)
                                                    .color(kit().disabled_text),
                                            )
                                            .fill(Color32::TRANSPARENT)
                                            .stroke(Stroke::NONE)
                                            .min_size(Vec2::new(18.0, 18.0)),
                                        )
                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                        .on_hover_text("Move shot down");
                                    dn.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::WidgetType::Button,
                                            can_down,
                                            format!("Move {code} down"),
                                        )
                                    });
                                    if dn.clicked() {
                                        move_req = Some((shot_id, 1));
                                    }

                                    let can_up = pos > 0;
                                    let up = ui
                                        .add_enabled(
                                            can_up,
                                            egui::Button::new(
                                                RichText::new("^")
                                                    .size(10.5)
                                                    .color(kit().disabled_text),
                                            )
                                            .fill(Color32::TRANSPARENT)
                                            .stroke(Stroke::NONE)
                                            .min_size(Vec2::new(18.0, 18.0)),
                                        )
                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                        .on_hover_text("Move shot up");
                                    up.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::WidgetType::Button,
                                            can_up,
                                            format!("Move {code} up"),
                                        )
                                    });
                                    if up.clicked() {
                                        move_req = Some((shot_id, -1));
                                    }

                                    action_btns_rect = del.rect.union(dn.rect).union(up.rect);
                                });
                            });

                            if !preview.is_empty() {
                                ui.add_space(3.0);
                                let trimmed: String = if preview.chars().count() > 48 {
                                    let mut s: String = preview.chars().take(47).collect();
                                    s.push('…');
                                    s
                                } else {
                                    preview
                                };
                                ui.label(
                                    RichText::new(trimmed)
                                        .size(11.5)
                                        .color(kit().disabled_text),
                                );
                            }
                        })
                        .response;

                    if ui.rect_contains_pointer(frame_resp.rect) {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    let clicked_card_body = delete_req.is_none()
                        && move_req.is_none()
                        && ui.input(|i| i.pointer.primary_clicked())
                        && ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| {
                            frame_resp.rect.contains(p) && !action_btns_rect.contains(p)
                        });
                    if clicked_card_body {
                        state.selected_shot = Some(shot_id);
                        state.floor_plan.selected =
                            Some(crate::floor_plan_view::PlanSelection::Camera(shot_id));
                        ui.ctx().request_repaint();
                    }
                }
            }
        });

    if let Some((id, delta)) = move_req {
        if project.shots.move_shot_in_scene(id, delta) {
            changed = true;
        }
    }
    if let Some(id) = delete_req {
        if project.shots.delete_shot(id) {
            if state.selected_shot == Some(id) {
                state.selected_shot = None;
            }
            changed = true;
        }
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    // Bottom section: Selected Shot Inspector
    let Some(sel_id) = state.selected_shot else {
        overline(ui, "SHOT INSPECTOR");
        ui.add_space(6.0);
        ui.label(
            RichText::new("Select a shot above to edit its lens, movement, color, and notes.")
                .size(12.0)
                .color(kit().disabled_text),
        );
        return changed;
    };

    let duration_secs = project
        .shots
        .get(sel_id)
        .map(|s| project.shots.shot_duration_secs(s, &project.blocks))
        .unwrap_or(0.0);

    ui.horizontal(|ui| {
        overline(ui, "SHOT INSPECTOR");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let secs = duration_secs.round().max(1.0) as usize;
            ui.label(
                RichText::new(format!("~{secs}s"))
                    .font(FontId::monospace(11.0))
                    .color(kit().disabled_text),
            );
        });
    });
    ui.add_space(6.0);

    ScrollArea::vertical()
        .id_salt("shots_inspector_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;

            let Some(shot) = project.shots.get_mut(sel_id) else {
                return;
            };

            // Row 1: Code + Lens
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Code").size(11.0).color(kit().disabled_text));
                    let resp = ui.add_sized(
                        Vec2::new(72.0, 26.0),
                        TextEdit::singleline(&mut shot.code)
                            .font(FontId::monospace(13.0))
                            .hint_text("1A"),
                    );
                    resp.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Shot code")
                    });
                    if resp.changed() {
                        changed = true;
                    }
                });

                ui.add_space(8.0);

                ui.vertical(|ui| {
                    ui.label(RichText::new("Lens").size(11.0).color(kit().disabled_text));
                    let w = ui.available_width().max(90.0);
                    let resp = ui.add_sized(
                        Vec2::new(w, 26.0),
                        TextEdit::singleline(&mut shot.lens).hint_text("e.g. 35mm"),
                    );
                    resp.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Shot lens")
                    });
                    if resp.changed() {
                        changed = true;
                    }
                });
            });

            // Quick lens preset chips
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for &preset in LENS_PRESETS {
                    let active = shot.lens.trim().eq_ignore_ascii_case(preset);
                    let chip =
                        egui::Button::new(RichText::new(preset).size(11.0).color(if active {
                            kit().accent_text
                        } else {
                            kit().disabled_text
                        }))
                        .fill(if active {
                            kit().accent_soft
                        } else {
                            kit().base
                        })
                        .stroke(Stroke::new(
                            1.0_f32,
                            if active { kit().accent } else { kit().stroke },
                        ))
                        .corner_radius(CornerRadius::same(12))
                        .min_size(Vec2::new(0.0, 20.0));
                    if ui.add(chip).clicked() {
                        shot.lens = preset.to_string();
                        changed = true;
                    }
                }
            });

            // Row 2: 10-color studio swatch picker
            ui.label(RichText::new("Color").size(11.0).color(kit().disabled_text));
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                for color in ShotColor::ALL {
                    let active = shot.color == color;
                    let (rect, resp) =
                        ui.allocate_exact_size(Vec2::new(22.0, 22.0), Sense::click());
                    let resp = resp
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(color.label());
                    resp.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            true,
                            format!("Color {}", color.label()),
                        )
                    });
                    let c = shot_color32(color);
                    let p = ui.painter();
                    if active {
                        p.circle_stroke(
                            rect.center(),
                            10.0,
                            Stroke::new(2.0_f32, kit().heading_text),
                        );
                    } else if resp.hovered() {
                        p.circle_stroke(rect.center(), 10.0, Stroke::new(1.2_f32, kit().stroke));
                    }
                    p.circle_filled(rect.center(), 7.2, c);
                    if resp.clicked() {
                        shot.color = color;
                        changed = true;
                    }
                }
            });

            // Row 3: Shot Size + Movement
            ui.label(
                RichText::new("Shot Size")
                    .size(11.0)
                    .color(kit().disabled_text),
            );
            egui::ComboBox::from_id_salt(("shot_size_combo", sel_id))
                .width(ui.available_width())
                .selected_text(shot.size.label())
                .show_ui(ui, |ui| {
                    for sz in ShotSize::ALL {
                        if ui
                            .selectable_value(&mut shot.size, sz, sz.label())
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });

            ui.label(
                RichText::new("Camera Movement")
                    .size(11.0)
                    .color(kit().disabled_text),
            );
            egui::ComboBox::from_id_salt(("shot_movement_combo", sel_id))
                .width(ui.available_width())
                .selected_text(shot.movement.label())
                .show_ui(ui, |ui| {
                    for mv in ShotMovement::ALL {
                        if ui
                            .selectable_value(&mut shot.movement, mv, mv.label())
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });

            // Row 4: Description / Setup notes
            ui.label(
                RichText::new("Setup Notes")
                    .size(11.0)
                    .color(kit().disabled_text),
            );
            let notes_resp = ui.add_sized(
                Vec2::new(ui.available_width(), 64.0),
                TextEdit::multiline(&mut shot.description)
                    .desired_rows(3)
                    .hint_text("Framing, blocking, camera angle, action notes..."),
            );
            notes_resp.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Shot setup notes")
            });
            if notes_resp.changed() {
                changed = true;
            }

            // Row 5: Covered Text Spans
            let spans_snapshot = shot.spans.clone();
            let block_index = block_index_map(&project.blocks);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("Covered Spans ({})", spans_snapshot.len()))
                        .size(11.0)
                        .color(kit().disabled_text),
                );
                if let Some(range) = state.selection_shot_range(&project.blocks) {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let add_span = egui::Button::new(
                            RichText::new("+ Add selection")
                                .size(11.0)
                                .color(kit().accent_text),
                        )
                        .fill(kit().accent_soft)
                        .stroke(Stroke::NONE)
                        .corner_radius(CornerRadius::same(4));
                        if ui.add(add_span).clicked() {
                            if project.shots.assign_range(sel_id, range) {
                                changed = true;
                            }
                            state.clear_selection();
                        }
                    });
                }
            });

            let mut remove_span_idx: Option<usize> = None;
            for (idx, span) in spans_snapshot.iter().enumerate() {
                let excerpt = span.excerpt(&block_index, &project.blocks);
                let summary = if excerpt.is_empty() {
                    "Deleted text".to_string()
                } else if excerpt.chars().count() > 38 {
                    let mut s: String = excerpt.chars().take(37).collect();
                    s.push('…');
                    s
                } else {
                    excerpt
                };
                egui::Frame::new()
                    .fill(kit().base)
                    .stroke(Stroke::new(1.0_f32, kit().stroke))
                    .corner_radius(CornerRadius::same(5))
                    .inner_margin(Margin::symmetric(8, 5))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(summary).size(11.0).color(kit().text));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let rm = ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new("x")
                                                .size(10.5)
                                                .color(kit().disabled_text),
                                        )
                                        .fill(Color32::TRANSPARENT)
                                        .stroke(Stroke::NONE)
                                        .min_size(Vec2::new(16.0, 16.0)),
                                    )
                                    .on_hover_text("Remove span from shot");
                                if rm.clicked() {
                                    remove_span_idx = Some(idx);
                                }
                            });
                        });
                    });
            }
            if let Some(rm_idx) = remove_span_idx {
                if project.shots.remove_span(sel_id, rm_idx) {
                    changed = true;
                }
            }

            ui.add_space(6.0);
            let del_shot = egui::Button::new(
                RichText::new("Delete Shot")
                    .size(11.5)
                    .color(kit().spell_error),
            )
            .fill(kit().base)
            .stroke(Stroke::new(1.0_f32, kit().stroke))
            .corner_radius(CornerRadius::same(6))
            .min_size(Vec2::new(ui.available_width(), 26.0));
            if ui
                .add(del_shot)
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
                && project.shots.delete_shot(sel_id)
            {
                state.selected_shot = None;
                changed = true;
            }
        });

    changed
}

#[allow(clippy::too_many_arguments)]
fn show_center_pane(
    ui: &mut Ui,
    project: &mut Project,
    scenes: &[Scene],
    active_scene: Option<&Scene>,
    scene_label: &str,
    block_coverage: &[Vec<usize>],
    char_coverage: &[Vec<(usize, usize, usize)>],
    state: &mut ShotsUiState,
    bold_character_cues: bool,
) -> ShotsAction {
    let mut action = ShotsAction::None;

    let scene_id = active_scene.map(|s| s.id);
    let scene_blocks: Vec<usize> = match active_scene {
        Some(sc) => sc.blocks.clone(),
        None => (0..project.blocks.len()).collect(),
    };
    let scene_shot_indices: Vec<usize> = match active_scene {
        Some(sc) => project.shots.shots_for_scene(sc, &project.blocks),
        None => (0..project.shots.shots.len()).collect(),
    };

    if !ui.input(|i| i.pointer.primary_down()) {
        state.dragging_selection = false;
    }

    // 1. Pinned Selection Action Bar at the top of the center stage
    let bar_resp = egui::Frame::new()
        .fill(kit().surface)
        .inner_margin(Margin::symmetric(16, 8))
        .stroke(Stroke::NONE)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.set_height(26.0);
                ui.spacing_mut().item_spacing.x = 8.0;

                if let Some(shot_range) = state.selection_shot_range(&project.blocks) {
                    let lo_bi = state.selection_range().map(|((b, _), _)| b).unwrap_or(0);
                    let index = block_index_map(&project.blocks);
                    let selected_text = shot_range.excerpt(&index, &project.blocks);
                    let words = selected_text.split_whitespace().count();
                    let badge_label = if words > 0 {
                        format!(
                            "{words} {} selected",
                            if words == 1 { "word" } else { "words" }
                        )
                    } else {
                        "Selection ready".to_string()
                    };

                    egui::Frame::new()
                        .fill(kit().accent_soft)
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(Margin::symmetric(9, 3))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(badge_label)
                                    .size(11.5)
                                    .strong()
                                    .color(kit().accent_text),
                            );
                        });

                    let next_code = project.shots.next_code_for_scene(scene_label, scene_id);
                    let new_btn = egui::Button::new(
                        RichText::new(format!("+ New Shot ({next_code})"))
                            .size(12.0)
                            .strong()
                            .color(ink_on_accent(kit().accent)),
                    )
                    .fill(kit().accent)
                    .stroke(Stroke::NONE)
                    .corner_radius(CornerRadius::same(6))
                    .min_size(Vec2::new(0.0, 26.0));
                    if ui.add(new_btn).clicked() {
                        let new_id = project.shots.add_shot(
                            scene_id,
                            scene_label,
                            ShotSize::Ws,
                            ShotMovement::Static,
                            "35mm".to_string(),
                            String::new(),
                            shot_range.clone(),
                        );
                        if let Some(sid) = scene_id {
                            project.shots.sync_camera_for_shot(sid, new_id);
                        }
                        state.selected_shot = Some(new_id);
                        state.floor_plan.selected =
                            Some(crate::floor_plan_view::PlanSelection::Camera(new_id));
                        state.clear_selection();
                        action = ShotsAction::Changed;
                    }

                    // Quick-assign pills for existing shots in this scene (overlap!)
                    for &si in scene_shot_indices.iter().take(4) {
                        if let Some(shot) = project.shots.shots.get(si) {
                            let sid = shot.id;
                            let code = shot.code.clone();
                            let col = shot_color32(shot.color);
                            let btn = egui::Button::new(
                                RichText::new(format!("+ {code}"))
                                    .font(FontId::monospace(11.5))
                                    .color(kit().text),
                            )
                            .fill(kit().button)
                            .stroke(Stroke::new(1.0_f32, col))
                            .corner_radius(CornerRadius::same(6))
                            .min_size(Vec2::new(0.0, 24.0));
                            if ui
                                .add(btn)
                                .on_hover_text(format!("Add selected text to shot {code}"))
                                .clicked()
                            {
                                if project.shots.assign_range(sid, shot_range.clone()) {
                                    action = ShotsAction::Changed;
                                }
                                state.selected_shot = Some(sid);
                                state.clear_selection();
                            }
                        }
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let open_btn = egui::Button::new(
                            RichText::new("Edit in Script")
                                .size(11.5)
                                .color(kit().text),
                        )
                        .fill(kit().button)
                        .stroke(Stroke::new(1.0_f32, kit().stroke))
                        .corner_radius(CornerRadius::same(6))
                        .min_size(Vec2::new(0.0, 24.0));
                        if ui.add(open_btn).clicked() {
                            action = ShotsAction::OpenInScript(lo_bi);
                        }

                        let clear_btn = egui::Button::new(
                            RichText::new("Clear")
                                .size(11.5)
                                .color(kit().disabled_text),
                        )
                        .fill(Color32::TRANSPARENT)
                        .stroke(Stroke::NONE)
                        .min_size(Vec2::new(0.0, 24.0));
                        if ui.add(clear_btn).clicked() {
                            state.clear_selection();
                        }
                    });
                } else {
                    let hint = if state.view_mode == ShotsViewMode::Split {
                        "Drag text to highlight, then assign a shot."
                    } else {
                        "Drag across text to highlight any phrase or lines, double-click a word, then assign a shot."
                    };
                    ui.add(
                        egui::Label::new(
                            RichText::new(hint).size(12.0).color(kit().disabled_text),
                        )
                        .truncate(),
                    );
                    if let Some(&first_bi) = scene_blocks.first() {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let open_btn = egui::Button::new(
                                RichText::new("Edit in Script")
                                    .size(11.5)
                                    .color(kit().text),
                            )
                            .fill(kit().button)
                            .stroke(Stroke::new(1.0_f32, kit().stroke))
                            .corner_radius(CornerRadius::same(6))
                            .min_size(Vec2::new(0.0, 24.0));
                            if ui.add(open_btn).clicked() {
                                action = ShotsAction::OpenInScript(first_bi);
                            }
                        });
                    }
                }
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

    // 2. Centered Screenplay Paper Stage
    ScrollArea::vertical()
        .id_salt(("shots_script_scroll", state.selected_scene))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(18.0);
            let avail_w = ui.available_width();
            let is_split = state.view_mode == ShotsViewMode::Split;
            let max_sheet_w = match state.view_mode {
                ShotsViewMode::CoverageRails => 820.0_f32,
                _ => 740.0_f32,
            };
            let pad = if is_split { 20.0 } else { 40.0 };
            let sheet_w = (avail_w - pad).min(max_sheet_w).max(200.0);
            let side_pad = ((avail_w - sheet_w) * 0.5).max(8.0);
            let inner_x: i8 = if is_split { 14 } else { 28 };
            let inner_y: i8 = if is_split { 16 } else { 22 };

            ui.horizontal(|ui| {
                ui.add_space(side_pad);
                ui.vertical(|ui| {
                    ui.set_width(sheet_w);

                    // Paper card
                    egui::Frame::new()
                        .fill(kit().page)
                        .stroke(Stroke::new(1.0_f32, kit().page_edge))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(Margin::symmetric(inner_x, inner_y))
                        .show(ui, |ui| {
                            ui.set_width((sheet_w - (inner_x as f32 * 2.0)).max(160.0));

                            // Scene title header inside the paper
                            if let Some(sc) = active_scene {
                                let (n_shots, cov_b, tot_b) = project.shots.scene_coverage_stats(
                                    sc,
                                    &project.blocks,
                                    block_coverage,
                                );
                                let sc_pct = (cov_b * 100).checked_div(tot_b).unwrap_or(0);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "SCENE {}",
                                            scene_label.trim_end_matches('.')
                                        ))
                                        .font(FontId::monospace(11.0))
                                        .strong()
                                        .color(kit().accent_text),
                                    );
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.label(
                                            RichText::new(format!(
                                                "{n_shots} {} · {sc_pct}% covered",
                                                if n_shots == 1 { "shot" } else { "shots" }
                                            ))
                                            .size(11.0)
                                            .color(kit().disabled_text),
                                        );
                                    });
                                });
                                ui.add_space(4.0);
                            }

                            // Coverage Rails track legend at top of page
                            let rail_shots: Vec<(usize, Uuid, String, Color32)> =
                                if state.view_mode == ShotsViewMode::CoverageRails {
                                    scene_shot_indices
                                        .iter()
                                        .take(8)
                                        .filter_map(|&idx| {
                                            project.shots.shots.get(idx).map(|s| {
                                                (idx, s.id, s.code.clone(), shot_color32(s.color))
                                            })
                                        })
                                        .collect()
                                } else {
                                    Vec::new()
                                };
                            let rail_col_w = 24.0_f32;
                            let rail_area_w = if state.view_mode == ShotsViewMode::CoverageRails
                                && !rail_shots.is_empty()
                            {
                                (rail_shots.len() as f32 * rail_col_w) + 12.0
                            } else {
                                0.0
                            };

                            if state.view_mode == ShotsViewMode::CoverageRails
                                && !rail_shots.is_empty()
                            {
                                let total_w = ui.available_width();
                                let (hdr_rect, _) = ui
                                    .allocate_exact_size(Vec2::new(total_w, 22.0), Sense::hover());
                                let rail_x0 = hdr_rect.right() - rail_area_w + 12.0;
                                for (col_i, &(_, sid, ref code, col)) in
                                    rail_shots.iter().enumerate()
                                {
                                    let cx = rail_x0 + col_i as f32 * rail_col_w + rail_col_w * 0.5;
                                    let pill_rect = Rect::from_center_size(
                                        Pos2::new(cx, hdr_rect.center().y),
                                        Vec2::new(rail_col_w - 3.0, 17.0),
                                    );
                                    let active = state.selected_shot == Some(sid);
                                    let resp = ui
                                        .interact(
                                            pill_rect,
                                            Id::new(("rail_hdr", sid)),
                                            Sense::click(),
                                        )
                                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                                    ui.painter().rect_filled(
                                        pill_rect,
                                        CornerRadius::same(3),
                                        if active {
                                            col
                                        } else {
                                            col.gamma_multiply(0.24)
                                        },
                                    );
                                    let g = ui.painter().layout_no_wrap(
                                        code.clone(),
                                        FontId::monospace(9.5),
                                        if active { ink_on_accent(col) } else { col },
                                    );
                                    ui.painter().galley(
                                        Pos2::new(
                                            pill_rect.center().x - g.size().x * 0.5,
                                            pill_rect.center().y - g.size().y * 0.5,
                                        ),
                                        g,
                                        Color32::TRANSPARENT,
                                    );
                                    if resp.clicked() {
                                        state.selected_shot = Some(sid);
                                    }
                                }
                                ui.add_space(4.0);
                            }

                            let shift_down = ui.input(|i| i.modifiers.shift);
                            let total_w = ui.available_width();
                            let tag_area_w = if state.view_mode.is_color_map_script() {
                                if is_split {
                                    50.0
                                } else {
                                    68.0
                                }
                            } else {
                                rail_area_w
                            };
                            let text_area_w = (total_w - tag_area_w - 6.0).max(120.0);

                            // Keep tight vertical spacing so Coverage Rails connect seamlessly
                            ui.spacing_mut().item_spacing.y = 0.0;

                            // Track previous visual row's primary shot for right-margin pill dedup
                            let mut prev_visual_row_primary: Option<usize> = None;
                            // Track active rail connections across blocks (shot_idx -> continues)
                            let mut prev_block_end_shots: Vec<usize> = Vec::new();

                            for (pos_in_scene, &bi) in scene_blocks.iter().enumerate() {
                                let Some(block) = project.blocks.get(bi) else {
                                    continue;
                                };
                                if !block.element_type.is_printable() {
                                    continue;
                                }

                                let spans_here =
                                    char_coverage.get(bi).map(Vec::as_slice).unwrap_or(&[]);

                                // Shots starting at char 0 of the next printable block (for continuous rails)
                                let next_block_start_shots: Vec<usize> = scene_blocks
                                    .get(pos_in_scene + 1)
                                    .and_then(|&nbi| char_coverage.get(nbi))
                                    .map(|v| {
                                        v.iter()
                                            .filter(|&&(_, c0, _)| c0 == 0)
                                            .map(|&(si, _, _)| si)
                                            .collect()
                                    })
                                    .unwrap_or_default();

                                let block_end_shots = render_script_block_row(
                                    ui,
                                    project,
                                    bi,
                                    block,
                                    spans_here,
                                    &prev_block_end_shots,
                                    &next_block_start_shots,
                                    &mut prev_visual_row_primary,
                                    shift_down,
                                    total_w,
                                    text_area_w,
                                    &rail_shots,
                                    rail_col_w,
                                    rail_area_w,
                                    state,
                                    bold_character_cues,
                                );
                                prev_block_end_shots = block_end_shots;
                            }
                        });

                    // Scene Prev / Next footer
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        let has_prev = state.selected_scene > 0;
                        let prev =
                            egui::Button::new(RichText::new("< Previous Scene").size(12.0).color(
                                if has_prev {
                                    kit().text
                                } else {
                                    kit().disabled_text
                                },
                            ))
                            .fill(kit().surface)
                            .stroke(Stroke::new(1.0_f32, kit().stroke))
                            .corner_radius(CornerRadius::same(6));
                        if ui.add_enabled(has_prev, prev).clicked() {
                            state.selected_scene -= 1;
                            state.clear_selection();
                        }

                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let has_next = state.selected_scene + 1 < scenes.len();
                            let next = egui::Button::new(
                                RichText::new("Next Scene >").size(12.0).color(if has_next {
                                    kit().text
                                } else {
                                    kit().disabled_text
                                }),
                            )
                            .fill(kit().surface)
                            .stroke(Stroke::new(1.0_f32, kit().stroke))
                            .corner_radius(CornerRadius::same(6));
                            if ui.add_enabled(has_next, next).clicked() {
                                state.selected_scene += 1;
                                state.clear_selection();
                            }
                        });
                    });
                    ui.add_space(28.0);
                });
            });
        });

    action
}

/// Extract `(row_char_start, row_char_end_excl_nl, row_rect)` for every wrapped
/// visual row in `galley`.
fn galley_row_ranges(galley: &egui::Galley) -> Vec<(usize, usize, Rect)> {
    let mut out = Vec::with_capacity(galley.rows.len());
    let mut off = 0usize;
    for placed in &galley.rows {
        let len_no_nl = placed.char_count_excluding_newline();
        let len_with_nl = placed.char_count_including_newline();
        out.push((off, off + len_no_nl, placed.rect()));
        off += len_with_nl;
    }
    out
}

/// Paint a highlighter rectangle covering exact characters `[c0, c1)` across
/// one or more wrapped visual rows of `galley`.
fn paint_char_highlight(
    painter: &egui::Painter,
    galley: &egui::Galley,
    text_pos: Pos2,
    c0: usize,
    c1: usize,
    fill: Color32,
    outline: Option<Stroke>,
) {
    if c0 >= c1 {
        return;
    }
    let mut off = 0usize;
    for placed in &galley.rows {
        let len_no_nl = placed.char_count_excluding_newline();
        let len_with_nl = placed.char_count_including_newline();
        let r_start = off;
        let r_end = off + len_no_nl;
        off += len_with_nl;

        if c1 <= r_start || c0 >= r_end {
            continue;
        }
        let local_0 = c0.max(r_start) - r_start;
        let local_1 = c1.min(r_end) - r_start;
        let x0 = text_pos.x + placed.x_offset(local_0);
        let x1 = text_pos.x + placed.x_offset(local_1);
        let prow = placed.rect();
        let rect = Rect::from_min_max(
            Pos2::new(x0 - 1.5, text_pos.y + prow.min.y - 1.0),
            Pos2::new((x1 + 1.5).max(x0 + 4.0), text_pos.y + prow.max.y + 1.0),
        );
        painter.rect_filled(rect, CornerRadius::same(3), fill);
        if let Some(st) = outline {
            painter.rect_stroke(rect, CornerRadius::same(3), st, StrokeKind::Inside);
        }
    }
}

/// Expand character index `ci` to its surrounding word `(start, end)` in `text`.
fn word_bounds_at(text: &str, ci: usize) -> (usize, usize) {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return (0, 0);
    }
    let idx = ci.min(chars.len().saturating_sub(1));
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '\'' || c == '-';
    if !is_word(chars[idx]) {
        return (idx, (idx + 1).min(chars.len()));
    }
    let mut lo = idx;
    while lo > 0 && is_word(chars[lo - 1]) {
        lo -= 1;
    }
    let mut hi = idx + 1;
    while hi < chars.len() && is_word(chars[hi]) {
        hi += 1;
    }
    (lo, hi)
}

#[allow(clippy::too_many_arguments)]
fn render_script_block_row(
    ui: &mut Ui,
    project: &Project,
    bi: usize,
    block: &ScriptBlock,
    spans_here: &[(usize, usize, usize)],
    prev_block_end_shots: &[usize],
    next_block_start_shots: &[usize],
    prev_visual_row_primary: &mut Option<usize>,
    shift_down: bool,
    total_w: f32,
    text_area_w: f32,
    rail_shots: &[(usize, Uuid, String, Color32)],
    rail_col_w: f32,
    rail_area_w: f32,
    state: &mut ShotsUiState,
    bold_character_cues: bool,
) -> Vec<usize> {
    let (indent_frac, right_frac, uppercase, bold, align_right, top_pad, bot_pad) = match block
        .element_type
    {
        ElementType::SceneHeading => (0.0, 0.0, true, true, false, 6.0, 6.0),
        ElementType::Action => (0.0, 0.0, false, false, false, 4.0, 5.0),
        ElementType::Character => (CUE_INDENT, 0.05, true, bold_character_cues, false, 8.0, 1.5),
        ElementType::Parenthetical => (PAREN_INDENT, 0.14, false, false, false, 1.0, 1.0),
        ElementType::Dialogue | ElementType::Lyrics => {
            (DIALOGUE_INDENT, 0.10, false, false, false, 1.5, 5.0)
        }
        ElementType::Transition => (0.0, 0.0, true, true, true, 6.0, 6.0),
        ElementType::Shot => (0.0, 0.0, true, false, false, 4.0, 4.0),
        _ => (0.0, 0.0, false, false, false, 3.0, 3.0),
    };

    let char_len = block.text.chars().count();
    let display_text = if uppercase {
        block.text.to_uppercase()
    } else {
        block.text.clone()
    };
    let display_text = if display_text.trim().is_empty() {
        " ".to_string()
    } else {
        display_text
    };

    let font = FontId::new(12.5, if bold { bold_mono() } else { MONO });
    let left_pad = 12.0 + text_area_w * indent_frac;
    let wrap_w = (text_area_w * (1.0 - indent_frac - right_frac) - 16.0).max(120.0);

    let text_color = if bold { kit().heading_text } else { kit().text };

    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap_w;
    job.append(
        &display_text,
        0.0,
        egui::TextFormat {
            font_id: font,
            color: text_color,
            ..Default::default()
        },
    );
    let galley = ui.fonts(|f| f.layout_job(job));
    let row_h = (galley.size().y + top_pad + bot_pad).max(22.0);

    let (row_rect, resp) = ui.allocate_exact_size(Vec2::new(total_w, row_h), Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::Text);
    resp.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            format!("Script block {}: {}", bi + 1, block.text.trim()),
        )
    });

    let text_x = if align_right {
        (row_rect.left() + text_area_w - galley.size().x - 10.0).max(row_rect.left() + 12.0)
    } else {
        row_rect.left() + left_pad
    };
    let text_pos = Pos2::new(text_x, row_rect.top() + top_pad);
    let visual_rows = galley_row_ranges(&galley);

    // Pick winning shot at one character offset: prefer selected shot if it
    // covers `ci`, otherwise choose the tightest (most specific) covering span.
    let selected_shot = state.selected_shot;
    let pick_shot_at = |ci: usize| -> Option<usize> {
        let mut best: Option<(usize, usize)> = None;
        for &(si, c0, c1) in spans_here {
            if ci >= c0 && ci < c1 {
                if let Some(sel_id) = selected_shot {
                    if project.shots.shots.get(si).is_some_and(|s| s.id == sel_id) {
                        return Some(si);
                    }
                }
                let span_len = c1 - c0;
                if best.is_none_or(|(_, bl)| span_len < bl) {
                    best = Some((si, span_len));
                }
            }
        }
        best.map(|(si, _)| si)
    };

    // Build merged character highlight runs `(shot_idx, c0, c1)` for Color Map
    let mut color_runs: Vec<(usize, usize, usize)> = Vec::new();
    if state.view_mode.is_color_map_script() && !spans_here.is_empty() && char_len > 0 {
        for ci in 0..char_len {
            if let Some(si) = pick_shot_at(ci) {
                if let Some(last) = color_runs.last_mut() {
                    if last.0 == si && last.2 == ci {
                        last.2 = ci + 1;
                        continue;
                    }
                }
                color_runs.push((si, ci, ci + 1));
            }
        }
    }

    if ui.is_rect_visible(row_rect) {
        let p = ui.painter();

        // 1. Shot highlights behind text
        match state.view_mode {
            ShotsViewMode::ColorMap | ShotsViewMode::Split | ShotsViewMode::FloorPlan => {
                for &(si, c0, c1) in &color_runs {
                    if let Some(shot) = project.shots.shots.get(si) {
                        let strong = state.selected_shot == Some(shot.id);
                        paint_char_highlight(
                            p,
                            &galley,
                            text_pos,
                            c0,
                            c1,
                            shot_tint32(shot.color, strong),
                            None,
                        );
                    }
                }
                // Crisp left color bar per visual row reflecting that visual row's primary shot
                for &(r_start, r_end, r_rect) in &visual_rows {
                    let row_shots = shots_overlapping_range(spans_here, r_start, r_end);
                    let row_primary = pick_row_primary(project, &row_shots, state.selected_shot);
                    if let Some(psi) = row_primary {
                        if let Some(shot) = project.shots.shots.get(psi) {
                            let col = shot_color32(shot.color);
                            let bar = Rect::from_min_max(
                                Pos2::new(row_rect.left(), text_pos.y + r_rect.min.y - 1.0),
                                Pos2::new(row_rect.left() + 3.5, text_pos.y + r_rect.max.y + 1.0),
                            );
                            p.rect_filled(bar, CornerRadius::same(2), col);

                            if *prev_visual_row_primary != Some(psi) {
                                let tag_text = if row_shots.len() > 1 {
                                    format!("{} +{}", shot.code, row_shots.len() - 1)
                                } else {
                                    shot.code.clone()
                                };
                                let g = p.layout_no_wrap(
                                    tag_text,
                                    FontId::monospace(10.0),
                                    ink_on_accent(col),
                                );
                                let pill_w = (g.size().x + 10.0).max(28.0);
                                let pill_y = text_pos.y + r_rect.center().y - 8.0;
                                let pill_rect = Rect::from_min_size(
                                    Pos2::new(row_rect.right() - pill_w - 6.0, pill_y),
                                    Vec2::new(pill_w, 16.0),
                                );
                                p.rect_filled(pill_rect, CornerRadius::same(4), col);
                                p.galley(
                                    Pos2::new(
                                        pill_rect.center().x - g.size().x * 0.5,
                                        pill_rect.center().y - g.size().y * 0.5,
                                    ),
                                    g,
                                    Color32::TRANSPARENT,
                                );
                            }
                        }
                    }
                    *prev_visual_row_primary = row_primary;
                }
            }
            ShotsViewMode::CoverageRails => {
                // Spotlight the currently selected shot's exact characters
                if let Some(sel_id) = state.selected_shot {
                    for &(si, c0, c1) in spans_here {
                        if let Some(shot) = project.shots.shots.get(si) {
                            if shot.id == sel_id {
                                paint_char_highlight(
                                    p,
                                    &galley,
                                    text_pos,
                                    c0,
                                    c1,
                                    shot_tint32(shot.color, false),
                                    None,
                                );
                            }
                        }
                    }
                }
            }
        }

        // 2. Live user character selection highlight
        if let Some((sc0, sc1)) = state.selection_in_block(bi, char_len) {
            paint_char_highlight(
                p,
                &galley,
                text_pos,
                sc0,
                sc1,
                kit().accent_soft,
                Some(Stroke::new(1.2_f32, kit().accent)),
            );
        }

        // 3. Screenplay text
        p.galley(text_pos, galley.clone(), text_color);

        // 4. Coverage Rails (exact wrapped visual-line precision!)
        if state.view_mode == ShotsViewMode::CoverageRails && !rail_shots.is_empty() {
            let rail_x0 = row_rect.right() - rail_area_w + 12.0;
            p.line_segment(
                [
                    Pos2::new(rail_x0 - 6.0, row_rect.top()),
                    Pos2::new(rail_x0 - 6.0, row_rect.bottom()),
                ],
                Stroke::new(1.0_f32, kit().sep.gamma_multiply(0.55)),
            );

            for (col_i, &(shot_idx, sid, _, col)) in rail_shots.iter().enumerate() {
                let cx = rail_x0 + col_i as f32 * rail_col_w + rail_col_w * 0.5;
                let active = state.selected_shot == Some(sid);
                let half_w = if active { 2.5 } else { 1.6 };

                let row_hits: Vec<bool> = visual_rows
                    .iter()
                    .map(|&(rs, re, _)| {
                        spans_here
                            .iter()
                            .any(|&(si, c0, c1)| si == shot_idx && c0 < re && c1 > rs)
                    })
                    .collect();

                for (vi, &hit) in row_hits.iter().enumerate() {
                    if !hit {
                        continue;
                    }
                    let is_first_vrow = vi == 0;
                    let is_last_vrow = vi + 1 == row_hits.len();
                    let connects_up = if is_first_vrow {
                        prev_block_end_shots.contains(&shot_idx)
                    } else {
                        row_hits[vi - 1]
                    };
                    let connects_dn = if is_last_vrow {
                        next_block_start_shots.contains(&shot_idx)
                    } else {
                        row_hits[vi + 1]
                    };

                    let r_rect = visual_rows[vi].2;
                    let y0 = if is_first_vrow && connects_up {
                        row_rect.top()
                    } else if connects_up {
                        text_pos.y + r_rect.min.y
                    } else {
                        text_pos.y + r_rect.min.y + 2.0
                    };
                    let y1 = if is_last_vrow && connects_dn {
                        row_rect.bottom()
                    } else if connects_dn {
                        text_pos.y + r_rect.max.y
                    } else {
                        text_pos.y + r_rect.max.y - 2.0
                    };

                    let line_rect =
                        Rect::from_min_max(Pos2::new(cx - half_w, y0), Pos2::new(cx + half_w, y1));
                    p.rect_filled(
                        line_rect,
                        CornerRadius::same(if connects_up && connects_dn { 0 } else { 2 }),
                        col,
                    );
                    if !connects_up {
                        p.circle_filled(Pos2::new(cx, y0 + 1.0), half_w + 1.2, col);
                    }
                    if !connects_dn {
                        p.circle_filled(Pos2::new(cx, y1 - 1.0), half_w + 1.2, col);
                    }

                    let hit_rect = Rect::from_min_max(
                        Pos2::new(cx - rail_col_w * 0.45, y0),
                        Pos2::new(cx + rail_col_w * 0.45, y1),
                    );
                    let rail_hit = ui
                        .interact(hit_rect, Id::new(("rail_seg", bi, vi, sid)), Sense::click())
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if rail_hit.clicked() {
                        state.selected_shot = Some(sid);
                    }
                }
            }
        }
    } else if state.view_mode.is_color_map_script() {
        // Keep prev_visual_row_primary accurate even when scrolled off-screen
        if let Some(&(r_start, r_end, _)) = visual_rows.last() {
            let row_shots = shots_overlapping_range(spans_here, r_start, r_end);
            *prev_visual_row_primary = pick_row_primary(project, &row_shots, state.selected_shot);
        }
    }

    // Hover tooltip listing every shot covering the hovered character
    if !spans_here.is_empty() && resp.hovered() {
        if let Some(ptr) = ui.input(|i| i.pointer.hover_pos()) {
            let ci = galley
                .cursor_from_pos(ptr - text_pos)
                .index
                .min(char_len.saturating_sub(1));
            let mut names = Vec::new();
            let mut seen_si = Vec::new();
            for &(si, c0, c1) in spans_here {
                if ci >= c0 && ci < c1 && !seen_si.contains(&si) {
                    seen_si.push(si);
                    if let Some(s) = project.shots.shots.get(si) {
                        if s.lens.trim().is_empty() {
                            names.push(format!("{} ({})", s.code, s.size.short()));
                        } else {
                            names.push(format!(
                                "{} ({} · {})",
                                s.code,
                                s.size.short(),
                                s.lens.trim()
                            ));
                        }
                    }
                }
            }
            if !names.is_empty() {
                resp.clone()
                    .on_hover_text(format!("Covered by: {}", names.join("  |  ")));
            }
        }
    }

    // Direct text-selection interaction (drag across chars/blocks, double-click word,
    // triple-click block, Shift+click extend, single-click shot or uncovered block).
    let pointer_pos = ui.input(|i| i.pointer.interact_pos());
    let char_at =
        |pos: Pos2| -> usize { galley.cursor_from_pos(pos - text_pos).index.min(char_len) };

    if resp.triple_clicked() {
        state.sel_anchor = Some((bi, 0));
        state.sel_focus = Some((bi, char_len));
        state.dragging_selection = false;
        ui.ctx().request_repaint();
    } else if resp.double_clicked() {
        if let Some(pos) = pointer_pos {
            let ci = char_at(pos);
            let (w0, w1) = word_bounds_at(&block.text, ci);
            state.sel_anchor = Some((bi, w0));
            state.sel_focus = Some((bi, w1));
            state.dragging_selection = false;
            ui.ctx().request_repaint();
        }
    } else if resp.is_pointer_button_down_on() && !state.dragging_selection {
        if let Some(pos) = pointer_pos {
            // Only start a text drag on the text half (not over Coverage Rails clicks)
            if pos.x <= row_rect.left() + text_area_w + 8.0 {
                let ci = char_at(pos);
                state.dragging_selection = true;
                if !shift_down || state.sel_anchor.is_none() {
                    state.sel_anchor = Some((bi, ci));
                }
                state.sel_focus = Some((bi, ci));
                ui.ctx().request_repaint();
            }
        }
    } else if state.dragging_selection && ui.input(|i| i.pointer.primary_down()) {
        if let Some(pos) = pointer_pos {
            if pos.y >= row_rect.top() && pos.y <= row_rect.bottom() {
                let ci = char_at(pos);
                if state.sel_focus != Some((bi, ci)) {
                    state.sel_focus = Some((bi, ci));
                    ui.ctx().request_repaint();
                }
            }
        }
    } else if resp.clicked() {
        if let Some(pos) = pointer_pos {
            if pos.x <= row_rect.left() + text_area_w + 8.0 {
                let ci = char_at(pos);
                if shift_down && state.sel_anchor.is_some() {
                    state.sel_focus = Some((bi, ci));
                    ui.ctx().request_repaint();
                } else if state.sel_anchor == state.sel_focus {
                    // Plain click without dragging: if clicking inside a shot highlight,
                    // select that shot in the inspector; if uncovered, select the whole block.
                    let hit_shot = pick_shot_at(ci.min(char_len.saturating_sub(1)));
                    if let Some(si) = hit_shot {
                        if let Some(shot) = project.shots.shots.get(si) {
                            state.selected_shot = Some(shot.id);
                        }
                        state.clear_selection();
                    } else if char_len > 0 {
                        state.sel_anchor = Some((bi, 0));
                        state.sel_focus = Some((bi, char_len));
                    }
                    ui.ctx().request_repaint();
                }
            }
        }
    }

    // Return shots that reach the end of this block (for continuous Coverage Rails into next block)
    spans_here
        .iter()
        .filter(|&&(_, _, c1)| c1 >= char_len && char_len > 0)
        .map(|&(si, _, _)| si)
        .collect()
}

fn shots_overlapping_range(
    spans_here: &[(usize, usize, usize)],
    r_start: usize,
    r_end: usize,
) -> Vec<usize> {
    let mut out = Vec::new();
    for &(si, c0, c1) in spans_here {
        if c0 < r_end && c1 > r_start && !out.contains(&si) {
            out.push(si);
        }
    }
    out
}

fn pick_row_primary(
    project: &Project,
    row_shots: &[usize],
    selected_shot: Option<Uuid>,
) -> Option<usize> {
    if row_shots.is_empty() {
        return None;
    }
    if let Some(sel_id) = selected_shot {
        if let Some(&si) = row_shots
            .iter()
            .find(|&&si| project.shots.shots.get(si).is_some_and(|s| s.id == sel_id))
        {
            return Some(si);
        }
    }
    row_shots.first().copied()
}

/// Calm empty-state view when no project is open.
pub fn show_empty(ctx: &egui::Context) {
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(kit().desk))
        .show(ctx, |ui| {
            let rect = ui.available_rect_before_wrap();
            ui.painter().text(
                rect.center() - Vec2::new(0.0, 12.0),
                Align2::CENTER_CENTER,
                "Shot Breakdown",
                FontId::proportional(20.0),
                kit().heading_text,
            );
            ui.painter().text(
                rect.center() + Vec2::new(0.0, 16.0),
                Align2::CENTER_CENTER,
                "Open or create a screenplay to tag shots, highlight coverage, and export shot lists.",
                FontId::proportional(13.0),
                kit().disabled_text,
            );
        });
}
