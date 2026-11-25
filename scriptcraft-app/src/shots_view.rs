//! Dedicated `Shots` phase workspace (Phase 1 of Shot Breakdown).
//!
//! Three-pane native `egui` studio layout:
//! - Left: Scene navigator with shot counts and coverage progress bars.
//! - Center: Interactive colored screenplay view (`Color Map` and Hollywood
//!   `Coverage Rails` modes) where clicking, Shift+clicking, or dragging
//!   selects blocks to create a new shot or add overlapping coverage to an
//!   existing shot.
//! - Right: Scene shot list (reorder / delete / select) + Shot Inspector
//!   (code, 10-color palette, shot size, camera movement, lens presets,
//!   description, and covered spans).

use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Id, Layout, Margin, Pos2, Rect, RichText,
    ScrollArea, Sense, Stroke, StrokeKind, TextEdit, Ui, Vec2,
};
use scriptcraft_core::shots::{block_index_map, ShotColor, ShotMovement, ShotRange, ShotSize};
use scriptcraft_core::{ElementType, Project, Scene, ScriptBlock};
use uuid::Uuid;

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
}

impl ShotsViewMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::ColorMap => "Color Map",
            Self::CoverageRails => "Coverage Rails",
        }
    }
}

/// Persistent UI state for the Shots workspace.
#[derive(Default)]
pub struct ShotsUiState {
    pub selected_scene: usize,
    pub selected_shot: Option<Uuid>,
    pub sel_anchor: Option<usize>,
    pub sel_focus: Option<usize>,
    pub view_mode: ShotsViewMode,
    dragging_selection: bool,
}

impl ShotsUiState {
    pub fn reset(&mut self) {
        self.selected_scene = 0;
        self.selected_shot = None;
        self.sel_anchor = None;
        self.sel_focus = None;
        self.dragging_selection = false;
    }

    pub fn selection_range(&self) -> Option<(usize, usize)> {
        let a = self.sel_anchor?;
        let b = self.sel_focus.unwrap_or(a);
        Some((a.min(b), a.max(b)))
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
        (true, true) => 48,
        (true, false) => 30,
        (false, true) => 46,
        (false, false) => 28,
    };
    Color32::from_rgba_unmultiplied(r, g, b, alpha)
}

/// Render the full 3-pane Shots workspace inside `ctx`.
pub fn show(
    ctx: &egui::Context,
    project: &mut Project,
    scene_labels: &[String],
    state: &mut ShotsUiState,
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

    let coverage = project.shots.resolve_block_coverage(&project.blocks);

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
                && coverage.get(*bi).is_some_and(|v| !v.is_empty())
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
            // Crisp bottom hairline
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
                        for mode in [ShotsViewMode::ColorMap, ShotsViewMode::CoverageRails] {
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
                        .on_hover_text("Export screenplay PDF with shot color bands and legend")
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
            // Right hairline border
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
                        let (shot_count, covered, total) =
                            project
                                .shots
                                .scene_coverage_stats(scene, &project.blocks, &coverage);
                        let pct = (covered * 100).checked_div(total).unwrap_or(0);
                        let active = state.selected_scene == si;

                        let row_resp =
                            scene_nav_row(ui, &label, &scene.heading, shot_count, pct, active);
                        if row_resp.clicked() && state.selected_scene != si {
                            state.selected_scene = si;
                            state.clear_selection();
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
            // Left hairline border
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

    // 4. Center Interactive Screenplay Coverage stage
    egui::CentralPanel::default()
        .frame(
            egui::Frame::new()
                .fill(kit().desk)
                .inner_margin(Margin::same(0)),
        )
        .show(ctx, |ui| {
            let center_action = show_center_pane(
                ui,
                project,
                &scenes,
                active_scene.as_ref(),
                &active_scene_label,
                &coverage,
                state,
            );
            if center_action != ShotsAction::None {
                action = center_action;
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

        // Scene number badge
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

        // Truncated scene heading
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

        // Bottom row: coverage progress bar + shot/pct summary
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

    // Scene Shot List header
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

    // Top section: shot cards (bounded height so inspector is always visible)
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
                                "Click or Shift+click lines in the script to select beats, then click + New Shot.",
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

                    let frame_resp = egui::Frame::new()
                        .fill(fill_col)
                        .stroke(Stroke::new(if selected { 1.5_f32 } else { 1.0_f32 }, border_col))
                        .corner_radius(CornerRadius::same(7))
                        .inner_margin(Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                // Color dot
                                let (dot_rect, _) = ui.allocate_exact_size(
                                    Vec2::new(10.0, 10.0),
                                    Sense::hover(),
                                );
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
                                        .on_hover_text("Move shot down");
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
                                        .on_hover_text("Move shot up");
                                    if up.clicked() {
                                        move_req = Some((shot_id, -1));
                                    }
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

                    let card_click = ui.interact(
                        frame_resp.rect,
                        Id::new(("shot_card", shot_id)),
                        Sense::click(),
                    );
                    if card_click.clicked() && delete_req.is_none() && move_req.is_none() {
                        state.selected_shot = Some(shot_id);
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

            // Row 5: Covered Spans
            let spans_snapshot = shot.spans.clone();
            let block_index = block_index_map(&project.blocks);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("Covered Spans ({})", spans_snapshot.len()))
                        .size(11.0)
                        .color(kit().disabled_text),
                );
                if let Some((lo, hi)) = state.selection_range() {
                    if let (Some(sb), Some(eb)) = (project.blocks.get(lo), project.blocks.get(hi)) {
                        let range = ShotRange::blocks(sb.id, eb.id);
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
                }
            });

            let mut remove_span_idx: Option<usize> = None;
            for (idx, span) in spans_snapshot.iter().enumerate() {
                let summary = match span.resolve(&block_index) {
                    Some((lo, hi)) => {
                        let count = hi - lo + 1;
                        let first_txt = project
                            .blocks
                            .get(lo)
                            .map(|b| b.text.trim().replace('\n', " "))
                            .unwrap_or_default();
                        let short: String = if first_txt.chars().count() > 30 {
                            let mut s: String = first_txt.chars().take(29).collect();
                            s.push('…');
                            s
                        } else {
                            first_txt
                        };
                        format!(
                            "{count} {} · {short}",
                            if count == 1 { "line" } else { "lines" }
                        )
                    }
                    None => "Deleted lines".to_string(),
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
    coverage: &[Vec<usize>],
    state: &mut ShotsUiState,
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

    // Release drag-selection when primary pointer lifts.
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

                if let Some((lo, hi)) = state.selection_range() {
                    let count = hi - lo + 1;
                    egui::Frame::new()
                        .fill(kit().accent_soft)
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(Margin::symmetric(9, 3))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(format!(
                                    "{count} {} selected",
                                    if count == 1 { "block" } else { "blocks" }
                                ))
                                .size(11.5)
                                .strong()
                                .color(kit().accent_text),
                            );
                        });

                    let next_code = project
                        .shots
                        .next_code_for_scene(scene_label, scene_id);
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
                        if let (Some(sb), Some(eb)) =
                            (project.blocks.get(lo), project.blocks.get(hi))
                        {
                            let new_id = project.shots.add_shot(
                                scene_id,
                                scene_label,
                                ShotSize::Ws,
                                ShotMovement::Static,
                                "35mm".to_string(),
                                String::new(),
                                ShotRange::blocks(sb.id, eb.id),
                            );
                            state.selected_shot = Some(new_id);
                            state.clear_selection();
                            action = ShotsAction::Changed;
                        }
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
                                .on_hover_text(format!("Add selected blocks to shot {code}"))
                                .clicked()
                            {
                                if let (Some(sb), Some(eb)) =
                                    (project.blocks.get(lo), project.blocks.get(hi))
                                {
                                    if project
                                        .shots
                                        .assign_range(sid, ShotRange::blocks(sb.id, eb.id))
                                    {
                                        action = ShotsAction::Changed;
                                    }
                                    state.selected_shot = Some(sid);
                                    state.clear_selection();
                                }
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
                            action = ShotsAction::OpenInScript(lo);
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
                    ui.label(
                        RichText::new(
                            "Click a line, Shift+click or drag to select a range, then assign a shot.",
                        )
                        .size(12.0)
                        .color(kit().disabled_text),
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
            let max_sheet_w = match state.view_mode {
                ShotsViewMode::ColorMap => 740.0_f32,
                ShotsViewMode::CoverageRails => 820.0_f32,
            };
            let sheet_w = (avail_w - 40.0).clamp(380.0, max_sheet_w);
            let side_pad = ((avail_w - sheet_w) * 0.5).max(16.0);

            ui.horizontal(|ui| {
                ui.add_space(side_pad);
                ui.vertical(|ui| {
                    ui.set_width(sheet_w);

                    // Paper card
                    egui::Frame::new()
                        .fill(kit().page)
                        .stroke(Stroke::new(1.0_f32, kit().page_edge))
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(Margin::symmetric(28, 22))
                        .show(ui, |ui| {
                            ui.set_width(sheet_w - 56.0);

                            // Scene title header inside the paper
                            if let Some(sc) = active_scene {
                                let (n_shots, cov_b, tot_b) = project.shots.scene_coverage_stats(
                                    sc,
                                    &project.blocks,
                                    coverage,
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

                            // Screenplay blocks inside this scene
                            let sel_range = state.selection_range();
                            let shift_down = ui.input(|i| i.modifiers.shift);
                            let total_w = ui.available_width();
                            let tag_area_w = if state.view_mode == ShotsViewMode::ColorMap {
                                68.0
                            } else {
                                rail_area_w
                            };
                            let text_area_w = (total_w - tag_area_w - 8.0).max(200.0);

                            // Keep tight vertical spacing so Coverage Rails connect seamlessly!
                            ui.spacing_mut().item_spacing.y = 0.0;

                            for (pos_in_scene, &bi) in scene_blocks.iter().enumerate() {
                                let Some(block) = project.blocks.get(bi) else {
                                    continue;
                                };
                                if !block.element_type.is_printable() {
                                    continue;
                                }

                                let cov = coverage.get(bi).map(|v| v.as_slice()).unwrap_or(&[]);
                                let prev_cov = pos_in_scene
                                    .checked_sub(1)
                                    .and_then(|p| scene_blocks.get(p))
                                    .and_then(|&pbi| coverage.get(pbi))
                                    .map(|v| v.as_slice())
                                    .unwrap_or(&[]);
                                let next_cov = scene_blocks
                                    .get(pos_in_scene + 1)
                                    .and_then(|&nbi| coverage.get(nbi))
                                    .map(|v| v.as_slice())
                                    .unwrap_or(&[]);

                                let is_selected =
                                    sel_range.is_some_and(|(lo, hi)| bi >= lo && bi <= hi);

                                render_script_block_row(
                                    ui,
                                    project,
                                    bi,
                                    block,
                                    cov,
                                    prev_cov,
                                    next_cov,
                                    is_selected,
                                    shift_down,
                                    total_w,
                                    text_area_w,
                                    &rail_shots,
                                    rail_col_w,
                                    rail_area_w,
                                    state,
                                );
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

#[allow(clippy::too_many_arguments)]
fn render_script_block_row(
    ui: &mut Ui,
    project: &Project,
    bi: usize,
    block: &ScriptBlock,
    cov: &[usize],
    prev_cov: &[usize],
    next_cov: &[usize],
    is_selected: bool,
    shift_down: bool,
    total_w: f32,
    text_area_w: f32,
    rail_shots: &[(usize, Uuid, String, Color32)],
    rail_col_w: f32,
    rail_area_w: f32,
    state: &mut ShotsUiState,
) {
    let (indent_frac, right_frac, uppercase, bold, align_right, top_pad, bot_pad) =
        match block.element_type {
            ElementType::SceneHeading => (0.0, 0.0, true, true, false, 6.0, 6.0),
            ElementType::Action => (0.0, 0.0, false, false, false, 4.0, 5.0),
            ElementType::Character => (CUE_INDENT, 0.05, true, true, false, 8.0, 1.5),
            ElementType::Parenthetical => (PAREN_INDENT, 0.14, false, false, false, 1.0, 1.0),
            ElementType::Dialogue | ElementType::Lyrics => {
                (DIALOGUE_INDENT, 0.10, false, false, false, 1.5, 5.0)
            }
            ElementType::Transition => (0.0, 0.0, true, true, true, 6.0, 6.0),
            ElementType::Shot => (0.0, 0.0, true, false, false, 4.0, 4.0),
            _ => (0.0, 0.0, false, false, false, 3.0, 3.0),
        };

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

    let font = FontId::monospace(12.5);
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
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    resp.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            format!("Script block {}: {}", bi + 1, block.text.trim()),
        )
    });

    // Primary shot for Color Map (prefer the currently selected shot if it covers this block!)
    let pick_primary = |slice: &[usize]| -> Option<usize> {
        if slice.is_empty() {
            None
        } else if let Some(sel_id) = state.selected_shot {
            slice
                .iter()
                .copied()
                .find(|&si| project.shots.shots.get(si).is_some_and(|s| s.id == sel_id))
                .or_else(|| slice.first().copied())
        } else {
            slice.first().copied()
        }
    };
    let primary_shot_idx = pick_primary(cov);
    let prev_primary_shot_idx = pick_primary(prev_cov);

    let text_box = Rect::from_min_max(
        row_rect.min,
        Pos2::new(row_rect.min.x + text_area_w, row_rect.max.y),
    );

    if ui.is_rect_visible(row_rect) {
        let p = ui.painter();

        // Background tint depending on view mode
        match state.view_mode {
            ShotsViewMode::ColorMap => {
                if let Some(psi) = primary_shot_idx {
                    if let Some(shot) = project.shots.shots.get(psi) {
                        let is_active_shot = state.selected_shot == Some(shot.id);
                        let wash_rect = Rect::from_min_max(
                            Pos2::new(row_rect.left(), row_rect.top() + 1.0),
                            Pos2::new(row_rect.right(), row_rect.bottom() - 1.0),
                        );
                        p.rect_filled(
                            wash_rect,
                            CornerRadius::same(4),
                            shot_tint32(shot.color, is_active_shot),
                        );
                        // Solid left color bar
                        let bar = Rect::from_min_max(
                            Pos2::new(row_rect.left(), row_rect.top() + 1.0),
                            Pos2::new(row_rect.left() + 3.5, row_rect.bottom() - 1.0),
                        );
                        p.rect_filled(bar, CornerRadius::same(2), shot_color32(shot.color));
                    }
                }
            }
            ShotsViewMode::CoverageRails => {
                if let Some(sel_id) = state.selected_shot {
                    if let Some(shot) = cov
                        .iter()
                        .filter_map(|&si| project.shots.shots.get(si))
                        .find(|s| s.id == sel_id)
                    {
                        let wash_rect = Rect::from_min_max(
                            Pos2::new(text_box.left(), text_box.top() + 1.0),
                            Pos2::new(text_box.right(), text_box.bottom() - 1.0),
                        );
                        p.rect_filled(
                            wash_rect,
                            CornerRadius::same(4),
                            shot_tint32(shot.color, false),
                        );
                    }
                }
            }
        }

        // Hover cue on uncovered rows
        if resp.hovered() && !is_selected {
            p.rect_filled(
                Rect::from_min_max(
                    Pos2::new(text_box.left(), text_box.top() + 1.0),
                    Pos2::new(text_box.right(), text_box.bottom() - 1.0),
                ),
                CornerRadius::same(4),
                kit().hover_light.gamma_multiply(0.55),
            );
        }

        // Active user block-range selection highlight
        if is_selected {
            let sel_rect = Rect::from_min_max(
                Pos2::new(text_box.left(), text_box.top() + 0.5),
                Pos2::new(text_box.right(), text_box.bottom() - 0.5),
            );
            p.rect_filled(sel_rect, CornerRadius::same(4), kit().accent_soft);
            p.rect_stroke(
                sel_rect,
                CornerRadius::same(4),
                Stroke::new(1.5_f32, kit().accent),
                StrokeKind::Inside,
            );
        }

        // Screenplay text
        let text_x = if align_right {
            (row_rect.left() + text_area_w - galley.size().x - 10.0).max(row_rect.left() + 12.0)
        } else {
            row_rect.left() + left_pad
        };
        let text_pos = Pos2::new(text_x, row_rect.top() + top_pad);
        p.galley(text_pos, galley, text_color);

        // Right margin annotations
        match state.view_mode {
            ShotsViewMode::ColorMap => {
                if let Some(psi) = primary_shot_idx {
                    if let Some(shot) = project.shots.shots.get(psi) {
                        let show_pill = prev_primary_shot_idx != Some(psi) || cov.len() > 1;
                        if show_pill {
                            let tag_text = if cov.len() > 1 {
                                format!("{} +{}", shot.code, cov.len() - 1)
                            } else {
                                shot.code.clone()
                            };
                            let col = shot_color32(shot.color);
                            let g = p.layout_no_wrap(
                                tag_text,
                                FontId::monospace(10.0),
                                ink_on_accent(col),
                            );
                            let pill_w = (g.size().x + 10.0).max(28.0);
                            let pill_rect = Rect::from_min_size(
                                Pos2::new(
                                    row_rect.right() - pill_w - 6.0,
                                    row_rect.top() + top_pad,
                                ),
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
            }
            ShotsViewMode::CoverageRails => {
                if !rail_shots.is_empty() {
                    let rail_x0 = row_rect.right() - rail_area_w + 12.0;
                    // Subtle vertical guide divider between text and rails
                    p.line_segment(
                        [
                            Pos2::new(rail_x0 - 6.0, row_rect.top()),
                            Pos2::new(rail_x0 - 6.0, row_rect.bottom()),
                        ],
                        Stroke::new(1.0_f32, kit().sep.gamma_multiply(0.55)),
                    );

                    for (col_i, &(shot_idx, sid, _, col)) in rail_shots.iter().enumerate() {
                        let cx = rail_x0 + col_i as f32 * rail_col_w + rail_col_w * 0.5;
                        let covered_here = cov.contains(&shot_idx);
                        if !covered_here {
                            continue;
                        }
                        let active = state.selected_shot == Some(sid);
                        let connects_up = prev_cov.contains(&shot_idx);
                        let connects_dn = next_cov.contains(&shot_idx);
                        let y0 = if connects_up {
                            row_rect.top()
                        } else {
                            row_rect.top() + 4.0
                        };
                        let y1 = if connects_dn {
                            row_rect.bottom()
                        } else {
                            row_rect.bottom() - 4.0
                        };
                        let half_w = if active { 2.5 } else { 1.6 };
                        let line_rect = Rect::from_min_max(
                            Pos2::new(cx - half_w, y0),
                            Pos2::new(cx + half_w, y1),
                        );
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

                        // Clickable rail hit target
                        let hit_rect = Rect::from_min_max(
                            Pos2::new(cx - rail_col_w * 0.45, row_rect.top()),
                            Pos2::new(cx + rail_col_w * 0.45, row_rect.bottom()),
                        );
                        let rail_hit = ui
                            .interact(hit_rect, Id::new(("rail_seg", bi, sid)), Sense::click())
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        if rail_hit.clicked() {
                            state.selected_shot = Some(sid);
                        }
                    }
                }
            }
        }
    }

    // Tooltip listing all overlapping shots covering this block
    if !cov.is_empty() && resp.hovered() {
        let names: Vec<String> = cov
            .iter()
            .filter_map(|&si| project.shots.shots.get(si))
            .map(|s| {
                if s.lens.trim().is_empty() {
                    format!("{} ({})", s.code, s.size.short())
                } else {
                    format!("{} ({} · {})", s.code, s.size.short(), s.lens.trim())
                }
            })
            .collect();
        if !names.is_empty() {
            resp.clone()
                .on_hover_text(format!("Covered by: {}", names.join("  |  ")));
        }
    }

    // Selection interaction (click, Shift+click, drag-select)
    if resp.clicked() {
        if shift_down && state.sel_anchor.is_some() {
            state.sel_focus = Some(bi);
        } else {
            state.sel_anchor = Some(bi);
            state.sel_focus = Some(bi);
            if let Some(psi) = primary_shot_idx {
                if let Some(shot) = project.shots.shots.get(psi) {
                    state.selected_shot = Some(shot.id);
                }
            }
        }
        ui.ctx().request_repaint();
    } else if resp.is_pointer_button_down_on() {
        if !state.dragging_selection {
            state.dragging_selection = true;
            if !shift_down || state.sel_anchor.is_none() {
                state.sel_anchor = Some(bi);
            }
            state.sel_focus = Some(bi);
            ui.ctx().request_repaint();
        }
    } else if state.dragging_selection
        && ui.input(|i| i.pointer.primary_down())
        && ui.rect_contains_pointer(row_rect)
        && state.sel_focus != Some(bi)
    {
        state.sel_focus = Some(bi);
        ui.ctx().request_repaint();
    }
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
                "Open or create a screenplay to tag shots, color-code coverage, and export shot lists.",
                FontId::proportional(13.0),
                kit().disabled_text,
            );
        });
}
