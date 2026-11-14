//! Editor 2.0: paints the shared layout engine's [`DocLayout`]
//! (`scriptcraft-layout`) and turns keyboard/pointer input into
//! [`EditCommand`]s instead of per-chunk `TextEdit`s.
//!
//! Layout happens in document units (millimetres on the fixed row grid);
//! `zoom` only scales painted points. Line breaks and page composition are
//! therefore identical at every zoom — the regression the old editor failed
//! when wrapping by pixel heuristics (see `ROADMAP.md`, Editor 2.0).
//!
//! The widget never mutates the project: it keeps a lazily built working
//! copy for a frame's worth of events (so positions stay valid within the
//! frame) and returns the commands for the app to apply through
//! `scriptcraft_core::edit::History`, which gives command-based undo/redo.
//!
//! Only pages intersecting the viewport are painted; the scroll area still
//! spans the full document height so scrolling and the scrollbar behave
//! normally.

use eframe::egui;
use scriptcraft_core::edit::{char_index, clamp_pos, pos_at_char, EditCommand, History, Pos};
use scriptcraft_core::editing::{
    auto_upper, candidates, complete_at, complete_line_at, enter_next, tab_next, tab_prev,
    TemplateMetrics,
};
use scriptcraft_core::model::{ElementType, ScriptBlock};
use scriptcraft_core::revision::RevisionColor;
use scriptcraft_layout::pdf::{REVISION_BAR_W_MM, REVISION_BAR_X_MM};
use scriptcraft_layout::place::{
    self, DocLayout, LineRole, PageLayout, PlacedLine, MM_PER_CELL, ROW_MM, RUSSIAN_GEOMETRY,
};

use crate::app::{bold_mono, italic_mono, MONO};
use crate::theme::{kit, paint_page, paint_revision, KitColors};

/// Points per millimetre at zoom 1.0: 12 pt Courier prime metrics make the
/// Russian template's text column exactly A4 width at 72 dpi.
pub const PT_PER_MM: f32 = 72.0 / 25.4;

/// Template margins in mm: left / top / right / bottom of the A4 page.
const PAD_L_MM: f32 = 37.5;
const PAD_T_MM: f32 = 25.0;
const PAD_R_MM: f32 = 25.0;

/// Page size in egui points at the given zoom.
pub fn page_size_pt(zoom: f32) -> egui::Vec2 {
    egui::vec2(210.0 * PT_PER_MM * zoom, 297.0 * PT_PER_MM * zoom)
}

/// Scroll-content paddings around the page stack, at zoom 1.0.
const VIEW_TOP_PAD_PT: f32 = 22.0;
const VIEW_BOTTOM_PAD_PT: f32 = 24.0;

/// Zoom at which a whole A4 page (plus the view's top/bottom padding) fits
/// into `avail` (points). Used by View ▸ Fit Page.
pub fn fit_page_zoom(avail: egui::Vec2) -> f32 {
    let z = (avail.y - 8.0) / (page_size_pt(1.0).y + VIEW_TOP_PAD_PT + VIEW_BOTTOM_PAD_PT);
    z.clamp(0.25, 4.0)
}

/// Zoom at which the page width spans `avail` (points, scrollbar slack
/// included). Used by View ▸ Fit Width.
pub fn fit_width_zoom(avail: egui::Vec2) -> f32 {
    let z = (avail.x - 16.0) / page_size_pt(1.0).x;
    z.clamp(0.25, 4.0)
}

/// Paginate and place a whole document (no zoom input by design).
pub fn document_layout(
    blocks: &[ScriptBlock],
    m: &TemplateMetrics,
    scene_labels: &[String],
) -> DocLayout {
    place::layout(blocks, m, &RUSSIAN_GEOMETRY, scene_labels)
}

/// [`document_layout`] with production page locks.
pub fn document_layout_with_page_lock(
    blocks: &[ScriptBlock],
    m: &TemplateMetrics,
    scene_labels: &[String],
    page_lock: Option<&scriptcraft_core::page_lock::PageLock>,
) -> DocLayout {
    place::layout_with_page_lock(blocks, m, &RUSSIAN_GEOMETRY, scene_labels, page_lock)
}

/// A selection between two document positions. Anchor is the fixed end a
/// shift-selection grows from; focus is the moving end (where the caret is).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub anchor: Pos,
    pub focus: Pos,
}

impl Selection {
    fn caret(at: Pos) -> Self {
        Self {
            anchor: at,
            focus: at,
        }
    }

    /// The two ends in document order.
    fn sorted(&self) -> (Pos, Pos) {
        if self.anchor <= self.focus {
            (self.anchor, self.focus)
        } else {
            (self.focus, self.anchor)
        }
    }
}

/// An open autocomplete dropdown anchored at the typing caret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Auto {
    pub items: Vec<String>,
    /// The caret the dropdown completes at (anywhere else it is stale).
    pub caret: Pos,
}

/// An open spellcheck menu: the flagged word a right-click landed on and
/// where the popup was asked to open.
#[derive(Clone, Debug, PartialEq)]
pub struct SpellMenu {
    pub bi: usize,
    pub para: usize,
    /// Char offsets of the word inside its paragraph.
    pub start: usize,
    pub end: usize,
    pub word: String,
    /// Screen position of the right-click (the popup anchor).
    pub pos: egui::Pos2,
}

/// Persistent Editor 2.0 state owned by the app.
#[derive(Debug, Default)]
pub struct EditState {
    pub selection: Option<Selection>,
    /// Command-based undo/redo for this editor.
    pub history: History,
    /// Structured clipboard from the last in-app copy/cut, so pasting
    /// across blocks keeps element types and paragraph breaks.
    pub internal_clip: Vec<(ElementType, String)>,
    /// Ask the scroll area to reveal the caret on the next paint.
    pub scroll_caret: bool,
    /// Open autocomplete dropdown, if any.
    pub autocomplete: Option<Auto>,
    /// Open spellcheck menu, if any.
    pub spell_menu: Option<SpellMenu>,
    /// Caret screen rect from the last painted frame (popup anchor, IME).
    pub caret_rect: Option<egui::Rect>,
    /// Zoom and scroll state of the previous frame, used to keep the
    /// document point under the pointer (or the viewport center) fixed
    /// while zooming.
    pub last_zoom: Option<f32>,
    pub last_offset: egui::Vec2,
    pub last_viewport: Option<egui::Rect>,
    /// Middle-button pan in progress (keeps panning while the pointer
    /// leaves the viewport).
    pub pan_active: bool,
    /// Page rectangles from the last painted frame (screen coordinates);
    /// interaction tests click through these.
    #[doc(hidden)]
    pub debug_pages: Vec<egui::Rect>,
    /// Suggestion rows from the last painted dropdown: `(text, rect)`;
    /// interaction tests click these.
    #[doc(hidden)]
    pub debug_autocomplete: Vec<(String, egui::Rect)>,
    /// Suggestion rows of the last painted spellcheck menu: `(text, rect)`;
    /// interaction tests click these.
    #[doc(hidden)]
    pub debug_spell_menu: Vec<(String, egui::Rect)>,
}

/// KIT Scenario Editor preferences that change typing or painting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorOptions {
    pub show_scene_numbers: bool,
    pub highlight_current_line: bool,
    pub capitalize_first_word: bool,
    pub correct_double_capitals: bool,
    pub replace_three_dots: bool,
    pub smart_quotes: bool,
}

impl Default for EditorOptions {
    fn default() -> Self {
        Self {
            show_scene_numbers: true,
            highlight_current_line: false,
            capitalize_first_word: false,
            correct_double_capitals: false,
            replace_three_dots: false,
            smart_quotes: false,
        }
    }
}

/// Per-frame display options that are not part of the document layout.
pub struct ScriptViewStyle<'a> {
    pub typewriter: bool,
    pub revision_colors: &'a [Option<RevisionColor>],
    /// Block ids hidden by omitted scenes; the caret skips over them.
    pub hidden_blocks: &'a [uuid::Uuid],
    /// Heading ids of omitted scenes; Enter there is refused so the hidden
    /// body cannot be split into view.
    pub omitted_headings: &'a [uuid::Uuid],
    /// Scenario Editor preferences (numbers, current-line highlight, typing).
    pub options: EditorOptions,
}

/// What the widget wants the app to apply this frame.
#[derive(Debug, Default)]
pub struct ScriptViewOutput {
    pub commands: Vec<EditCommand>,
    pub undo: bool,
    pub redo: bool,
}

/// Paint the interactive script view into `ui` with the caller's zoom and
/// the app's cached document layout. `blocks` must be the same list the
/// layout was built from.
pub fn script_view(
    ui: &mut egui::Ui,
    zoom: f32,
    zoom_anchor: Option<egui::Pos2>,
    blocks: &[ScriptBlock],
    doc: &DocLayout,
    style: ScriptViewStyle<'_>,
    state: &mut EditState,
) -> ScriptViewOutput {
    let ScriptViewStyle {
        typewriter,
        revision_colors,
        hidden_blocks,
        omitted_headings,
        options,
    } = style;
    let page = page_size_pt(zoom);
    let scale = PT_PER_MM * zoom;
    let body = 12.0 * zoom;
    let row_h = ROW_MM * scale;
    let cell_w = MM_PER_CELL * scale;
    let mut out = ScriptViewOutput::default();

    // Camera-style framing: when the zoom changes, keep the document point
    // under the anchor (pointer for pinch, viewport center for toolbar,
    // menu and fit-mode resizes) fixed on screen. The stack scales linearly
    // with zoom, so this is exact and the text stays laid out once.
    let view_rect = state
        .last_viewport
        .unwrap_or_else(|| ui.available_rect_before_wrap());
    let mut scroll_to: Option<egui::Vec2> = None;
    if let Some(prev_zoom) = state.last_zoom {
        if (zoom - prev_zoom).abs() > 1e-6 {
            let anchor = zoom_anchor
                .filter(|p| view_rect.contains(*p))
                .unwrap_or_else(|| view_rect.center());
            scroll_to = Some(anchored_scroll_offset(
                state.last_offset,
                prev_zoom,
                zoom,
                anchor,
                view_rect,
            ));
        }
    }

    // Middle-button drag pans the stack (text selection owns the primary
    // button), matching PDF viewers and layout tools. Horizontal scrolling
    // and pinch/⌘+wheel zooming come from the scroll area itself.
    let (middle_down, pointer_pos, pointer_delta) = ui.input(|i| {
        (
            i.pointer.button_down(egui::PointerButton::Middle),
            i.pointer.latest_pos(),
            i.pointer.delta(),
        )
    });
    if !middle_down {
        state.pan_active = false;
    } else if state.pan_active || pointer_pos.is_some_and(|p| view_rect.contains(p)) {
        state.pan_active = true;
        let offset = scroll_to.unwrap_or(state.last_offset) - pointer_delta;
        scroll_to = Some(offset.max(egui::Vec2::ZERO));
    }
    if state.pan_active {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }

    let mut area = egui::ScrollArea::both()
        .id_salt("editor2_preview")
        .auto_shrink([false, false]);
    if let Some(offset) = scroll_to {
        area = area
            .vertical_scroll_offset(offset.y)
            .horizontal_scroll_offset(offset.x);
    }
    let area_out = area.show(ui, |ui| {
        let gap = 18.0 * zoom;
        let pad_top = VIEW_TOP_PAD_PT * zoom;
        let n = doc.pages.len().max(1);
        let total_h =
            pad_top + n as f32 * page.y + (n - 1) as f32 * gap + VIEW_BOTTOM_PAD_PT * zoom;
        let width = ui.available_width().max(page.x + 2.0);
        let (rect, resp) =
            ui.allocate_exact_size(egui::vec2(width, total_h), egui::Sense::click_and_drag());
        let resp = resp.on_hover_cursor(egui::CursorIcon::Text);
        let left = rect.center().x - page.x * 0.5;
        let page_rects: Vec<egui::Rect> = (0..doc.pages.len())
            .map(|pi| {
                let top = rect.top() + pad_top + pi as f32 * (page.y + gap);
                egui::Rect::from_min_size(egui::pos2(left, top), page)
            })
            .collect();
        state.debug_pages = page_rects.clone();

        // A press on an open dropdown row must not wrest keyboard focus
        // from the page (egui surrenders focus on presses over other
        // widgets), or the click's release would land on a popup that
        // vanished together with the focus.
        let pressed_at = ui.input(|i| {
            if i.pointer.any_pressed() {
                i.pointer.interact_pos()
            } else {
                None
            }
        });
        let press_on_popup =
            pressed_at.is_some_and(|p| state.debug_autocomplete.iter().any(|(_, r)| r.contains(p)));
        if press_on_popup {
            resp.request_focus();
        }

        let focused = resp.has_focus();
        if focused {
            // We own Tab and the arrows while the script has focus.
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    resp.id,
                    egui::EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: false,
                    },
                );
            });
        }

        let events: Vec<egui::Event> = if focused {
            ui.input(|i| i.events.clone())
        } else {
            Vec::new()
        };
        let pointer_work = resp.clicked() || resp.drag_started() || resp.dragged();
        let mut text_edited = false;
        let mut caret_moved = false;
        if pointer_work || !events.is_empty() {
            let mut ed = Editor::with_options(blocks, state.selection, options);
            if resp.clicked() {
                resp.request_focus();
                caret_moved = true;
                if let Some(p) = resp.interact_pointer_pos() {
                    if let Some(at) = hit_test(doc, &page_rects, scale, row_h, cell_w, p) {
                        ed.set_caret(at);
                    }
                }
            }
            if resp.drag_started() {
                resp.request_focus();
                caret_moved = true;
                let origin = resp
                    .interact_pointer_pos()
                    .or_else(|| ui.input(|i| i.pointer.press_origin()));
                if let Some(p) = origin {
                    if let Some(at) = hit_test(doc, &page_rects, scale, row_h, cell_w, p) {
                        ed.set_selection(at, at);
                    }
                }
            }
            if resp.dragged() {
                caret_moved = true;
                if let Some(p) = resp.interact_pointer_pos() {
                    if let Some(at) = hit_test(doc, &page_rects, scale, row_h, cell_w, p) {
                        let anchor = ed.sel.map_or(at, |s| s.anchor);
                        ed.set_selection(anchor, at);
                        ed.scroll = true;
                    }
                }
            }
            if focused {
                let (edited, moved) = handle_events(
                    ui,
                    doc,
                    &mut ed,
                    &events,
                    state,
                    hidden_blocks,
                    omitted_headings,
                );
                text_edited |= edited;
                caret_moved |= moved;
            }
            state.selection = ed.sel;
            if ed.scroll {
                state.scroll_caret = true;
            }
            out.commands = std::mem::take(&mut ed.cmds);
            if focused {
                if caret_moved {
                    state.autocomplete = None;
                }
                if text_edited {
                    state.autocomplete = autocomplete_for(ed.blocks(), state.selection);
                }
            }
        }
        if focused && state.autocomplete.is_some() && ui.input(|i| i.key_pressed(egui::Key::Escape))
        {
            state.autocomplete = None;
        }

        let selection = state
            .selection
            .filter(|s| s.anchor != s.focus)
            .map(|s| s.sorted());
        let caret_at = state.selection.map(|s| s.focus);
        let caret_geo =
            caret_at.and_then(|at| caret_geometry(doc, &page_rects, scale, row_h, cell_w, at));
        state.caret_rect = if focused {
            caret_geo.map(|(_, _, r)| r)
        } else {
            None
        };

        if state.scroll_caret {
            if let Some((_, _, caret)) = caret_geo {
                if typewriter {
                    ui.scroll_to_rect(caret.expand(4.0), Some(egui::Align::Center));
                } else {
                    ui.scroll_to_rect(caret.expand(4.0), None);
                }
            }
            state.scroll_caret = false;
        }
        if focused {
            if let Some((_, page_rect, caret)) = caret_geo {
                ui.ctx().output_mut(|o| {
                    o.ime = Some(egui::output::IMEOutput {
                        rect: page_rect,
                        cursor_rect: caret,
                    });
                });
            }
        }

        let painter = ui.painter();
        let clip = ui.clip_rect();
        let sel_fill = ui.visuals().selection.bg_fill;
        let caret_color = ui.visuals().text_cursor.stroke.color;
        let caret_w = ui.visuals().text_cursor.stroke.width.max(1.0);
        // Current-line tint follows the theme accent; Midnight and Light both
        // use #4294F8, so their pixels are unchanged.
        let accent = kit().accent;
        let line_band =
            egui::Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 26);
        for (pi, page_lay) in doc.pages.iter().enumerate() {
            let prect = page_rects[pi];
            if !clip.intersects(prect) {
                continue;
            }
            paint_page(painter, prect);
            paint_page_number(painter, prect, page_lay, body, row_h, scale);
            if focused && options.highlight_current_line {
                if let Some((cpi, page_rect, caret)) = caret_geo {
                    if cpi == pi {
                        let band = egui::Rect::from_min_max(
                            egui::pos2(page_rect.left() + PAD_L_MM * scale, caret.top()),
                            egui::pos2(page_rect.right() - PAD_R_MM * scale, caret.top() + row_h),
                        );
                        painter.rect_filled(band, 2.0, line_band);
                    }
                }
            }
            let content_top = prect.top() + PAD_T_MM * scale;
            for line in &page_lay.lines {
                if line.role == LineRole::SceneNumber && !options.show_scene_numbers {
                    continue;
                }
                let y = content_top + line.row as f32 * row_h;
                if let Some((a, f)) = selection {
                    if let Some((c0, c1)) = selection_columns(line, a, f) {
                        let x0 = line_origin_x(prect, line, scale, cell_w);
                        let highlight = egui::Rect::from_min_size(
                            egui::pos2(x0 + c0 as f32 * cell_w, y),
                            egui::vec2((c1 - c0) as f32 * cell_w, row_h),
                        );
                        painter.rect_filled(highlight, 0.0, sel_fill);
                    }
                }
                let revision = line
                    .source_bi
                    .and_then(|bi| revision_colors.get(bi).copied().flatten());
                let paint = LinePaint {
                    body,
                    scale,
                    row_h,
                    revision,
                };
                paint_line(painter, prect, blocks, line, paint);
                if line.role == LineRole::Body {
                    paint_spelling(painter, prect, blocks, line, paint, cell_w);
                }
            }
            if focused {
                if let Some((cpi, _, caret)) = caret_geo {
                    if cpi == pi {
                        let bar = egui::Rect::from_min_size(
                            caret.min,
                            egui::vec2(caret_w, caret.height()),
                        );
                        painter.rect_filled(bar, 0.0, caret_color);
                    }
                }
            }
        }

        // Spellcheck menu: right-click a flagged word for suggestions and
        // the two ways to teach the dictionary. Only flagged words open it,
        // so the editor's right-click stays inert everywhere else.
        if resp.secondary_clicked() {
            state.spell_menu = resp
                .interact_pointer_pos()
                .and_then(|p| hit_test(doc, &page_rects, scale, row_h, cell_w, p).map(|at| (at, p)))
                .and_then(|(at, p)| {
                    let text = blocks.get(at.bi)?.text.split('\n').nth(at.para)?;
                    let token = crate::spell::word_at(text, at.ch)?;
                    let flagged = crate::spell::flags(at.bi, at.para, text)
                        .iter()
                        .any(|m| m.contains(at.ch));
                    flagged.then(|| SpellMenu {
                        bi: at.bi,
                        para: at.para,
                        start: token.start,
                        end: token.end,
                        word: token.word.to_string(),
                        pos: p,
                    })
                });
        }
        let mut spell_rows: Vec<(String, egui::Rect)> = Vec::new();
        if let Some(menu) = state.spell_menu.clone() {
            let open = if resp.secondary_clicked() {
                Some(egui::SetOpenCommand::Bool(true))
            } else if resp.clicked() {
                Some(egui::SetOpenCommand::Bool(false))
            } else {
                None
            };
            let shown = egui::Popup::from_response(&resp)
                .at_position(menu.pos)
                .open_memory(open)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show(|ui| {
                    ui.set_min_width(200.0);
                    ui.spacing_mut().item_spacing.y = 2.0;
                    let suggestions = crate::spell::suggestions(&menu.word, 6);
                    let mut picked: Option<String> = None;
                    let mut add_error: Option<String> = None;
                    if suggestions.is_empty() {
                        ui.add_enabled(false, egui::Label::new("No suggestions"));
                    }
                    for s in &suggestions {
                        let r = ui.selectable_label(false, s);
                        spell_rows.push((s.clone(), r.rect));
                        if r.clicked() {
                            picked = Some(s.clone());
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui.button("Add to dictionary").clicked() {
                        match crate::spell::add_word(&menu.word) {
                            Ok(()) => ui.close(),
                            Err(err) => add_error = Some(err),
                        }
                    }
                    if ui.button("Ignore").clicked() {
                        crate::spell::ignore_word(&menu.word);
                        ui.close();
                    }
                    if let Some(err) = &add_error {
                        ui.label(egui::RichText::new(err).color(kit().spell_error));
                    }
                    picked
                });
            match shown {
                Some(inner) => {
                    if let Some(word) = inner.inner {
                        let at = Pos {
                            bi: menu.bi,
                            para: menu.para,
                            ch: menu.start,
                        };
                        let end = Pos {
                            bi: menu.bi,
                            para: menu.para,
                            ch: menu.end,
                        };
                        out.commands
                            .push(EditCommand::DeleteRange { start: at, end });
                        out.commands
                            .push(EditCommand::InsertText { at, text: word });
                        state.spell_menu = None;
                    }
                }
                // The popup closed itself (Escape or a click elsewhere).
                None => state.spell_menu = None,
            }
        }
        state.debug_spell_menu = spell_rows;

        // Floating autocomplete at the caret (KIT-style dropdown). The
        // widget owns the state because the popup lives in the same
        // layout pass as the caret that anchors it.
        let mut rows: Vec<(String, egui::Rect)> = Vec::new();
        if focused {
            if let (Some(auto), Some((_, _, caret_rect))) = (state.autocomplete.clone(), caret_geo)
            {
                if state.selection.map(|s| s.focus) == Some(auto.caret) {
                    let anchor = caret_rect.left_bottom() + egui::vec2(0.0, 6.0);
                    let mut picked: Option<String> = None;
                    egui::Area::new(egui::Id::new("editor2_autocomplete"))
                        .order(egui::Order::Foreground)
                        .fixed_pos(anchor)
                        .constrain(true)
                        .show(ui.ctx(), |ui| {
                            egui::Frame::popup(ui.style()).show(ui, |ui| {
                                ui.set_min_width(220.0);
                                ui.spacing_mut().item_spacing.y = 2.0;
                                for s in &auto.items {
                                    let r = ui.selectable_label(false, s);
                                    if r.clicked() {
                                        picked = Some(s.clone());
                                    }
                                    rows.push((s.clone(), r.rect));
                                }
                            });
                        });
                    if let Some(s) = picked {
                        apply_completion(blocks, auto.caret, &s, &mut out.commands, state);
                        rows.clear();
                    }
                } else {
                    state.autocomplete = None;
                }
            }
        }
        state.debug_autocomplete = rows;
        resp
    });
    state.last_offset = area_out.state.offset;
    state.last_viewport = Some(area_out.inner_rect);
    state.last_zoom = Some(zoom);

    out
}

/// Scroll offset that keeps the page-space point under `anchor` fixed when
/// zooming from `prev_zoom` to `new_zoom`. The page is centered while it
/// fits the viewport; once it overflows, the same rule anchors the visible
/// slice horizontally.
fn anchored_scroll_offset(
    prev_offset: egui::Vec2,
    prev_zoom: f32,
    new_zoom: f32,
    anchor: egui::Pos2,
    viewport: egui::Rect,
) -> egui::Vec2 {
    let a = anchor - viewport.min;
    let prev_page = page_size_pt(prev_zoom);
    let new_page = page_size_pt(new_zoom);
    let prev_left = (viewport.width().max(prev_page.x + 2.0) - prev_page.x) * 0.5;
    let new_left = (viewport.width().max(new_page.x + 2.0) - new_page.x) * 0.5;
    // Page-space coordinates (zoom 1.0) under the anchor.
    let doc_x = (prev_offset.x + a.x - prev_left) / prev_zoom;
    let doc_y = (prev_offset.y + a.y) / prev_zoom;
    egui::vec2(
        (doc_x * new_zoom + new_left - a.x).max(0.0),
        (doc_y * new_zoom - a.y).max(0.0),
    )
}

/// Splice an autocomplete pick into the caret's line: the partial token (or
/// the whole heading line) is replaced with the suggestion, uppercase for
/// cue-like elements. Emitted as one `SetText` command.
fn apply_completion(
    blocks: &[ScriptBlock],
    caret: Pos,
    pick: &str,
    cmds: &mut Vec<EditCommand>,
    state: &mut EditState,
) {
    let Some(b) = blocks.get(caret.bi) else {
        state.autocomplete = None;
        return;
    };
    let abs = char_index(&b.text, caret);
    let completed = if b.element_type == ElementType::SceneHeading {
        complete_line_at(&b.text, abs, pick)
    } else {
        complete_at(&b.text, abs, pick)
    };
    let completed = if auto_upper(b.element_type) {
        completed.to_uppercase()
    } else {
        completed
    };
    let chars: Vec<char> = b.text.chars().collect();
    let abs = abs.min(chars.len());
    let from = if b.element_type == ElementType::SceneHeading {
        chars[..abs]
            .iter()
            .rposition(|&c| c == '\n')
            .map(|i| i + 1)
            .unwrap_or(0)
    } else {
        let mut t = abs;
        while t > 0 && !chars[t - 1].is_whitespace() {
            t -= 1;
        }
        t
    };
    let after = pos_at_char(&b.text, caret.bi, from + pick.chars().count());
    cmds.push(EditCommand::SetText {
        bi: caret.bi,
        text: completed,
    });
    state.selection = Some(Selection::caret(after));
    state.scroll_caret = true;
    state.autocomplete = None;
}

/// Process text/keys/clipboard events in order against the working copy.
/// Returns `(text_edited, caret_moved)` for autocomplete bookkeeping.
fn handle_events(
    ui: &mut egui::Ui,
    doc: &DocLayout,
    ed: &mut Editor,
    events: &[egui::Event],
    state: &mut EditState,
    hidden_blocks: &[uuid::Uuid],
    omitted_headings: &[uuid::Uuid],
) -> (bool, bool) {
    let mut text_edited = false;
    let mut caret_moved = false;
    // Command chords produce no text here; ignoring the frame's text events
    // keeps ⌘A/⌘C from also typing.
    let cmd_down = ui.input(|i| i.modifiers.command);
    for ev in events {
        match ev {
            egui::Event::Text(t) if !cmd_down => {
                // Enter arrives as a key event; drop its text form.
                if !t.is_empty() && !t.chars().all(|c| c == '\r') {
                    ed.insert(t);
                    text_edited = true;
                }
            }
            egui::Event::Ime(egui::ImeEvent::Commit(t)) => {
                ed.insert(t);
                text_edited = true;
            }
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => {
                if modifiers.command && *key == egui::Key::A {
                    if let (Some(first), Some(last)) = (
                        first_visible_pos(ed.blocks(), hidden_blocks),
                        last_visible_pos(ed.blocks(), hidden_blocks),
                    ) {
                        ed.set_selection(first, last);
                        ed.scroll = true;
                    }
                    caret_moved = true;
                    continue;
                }
                if let Some(action) = key_action(
                    ed.blocks(),
                    doc,
                    ed.sel,
                    *key,
                    *modifiers,
                    hidden_blocks,
                    omitted_headings,
                ) {
                    let edited = !action.commands.is_empty();
                    ed.sel = action.selection;
                    ed.press(action.commands);
                    if action.scroll {
                        ed.scroll = true;
                    }
                    if edited {
                        text_edited = true;
                    } else {
                        caret_moved = true;
                    }
                }
            }
            egui::Event::Copy => {
                if let Some((a, f)) = ed.nonempty_selection() {
                    let text = text_between(ed.blocks(), a, f);
                    state.internal_clip = fragments_between(ed.blocks(), a, f);
                    ui.ctx().copy_text(text);
                }
            }
            egui::Event::Cut => {
                if let Some((a, f)) = ed.nonempty_selection() {
                    let text = text_between(ed.blocks(), a, f);
                    state.internal_clip = fragments_between(ed.blocks(), a, f);
                    ui.ctx().copy_text(text);
                    ed.press(delete_selection_cmds(ed.blocks(), a, f));
                    ed.sel = Some(Selection::caret(a));
                    ed.scroll = true;
                    text_edited = true;
                }
            }
            egui::Event::Paste(raw) => {
                let text = raw.replace("\r\n", "\n").replace('\r', "\n");
                if !text.is_empty() {
                    ed.paste(&text, &state.internal_clip);
                    text_edited = true;
                }
            }
            _ => {}
        }
    }
    (text_edited, caret_moved)
}

/// Candidates for the caret's current partial line, if any (opened only on
/// an actual edit — focusing a block never pops the dropdown).
fn autocomplete_for(blocks: &[ScriptBlock], sel: Option<Selection>) -> Option<Auto> {
    let s = sel?;
    if s.anchor != s.focus {
        return None;
    }
    let pos = s.focus;
    let b = blocks.get(pos.bi)?;
    let para = b.text.split('\n').nth(pos.para)?;
    let typed: String = para.chars().take(pos.ch).collect();
    if typed.trim().is_empty() {
        return None;
    }
    let items = candidates(blocks, b.element_type, &typed);
    if items.is_empty() {
        return None;
    }
    Some(Auto { items, caret: pos })
}

/// The widget's working copy for one frame plus the commands it produced.
struct Editor {
    work: Vec<ScriptBlock>,
    sel: Option<Selection>,
    cmds: Vec<EditCommand>,
    scroll: bool,
    options: EditorOptions,
}

impl Editor {
    #[cfg(test)]
    fn new(base: &[ScriptBlock], sel: Option<Selection>) -> Self {
        Self::with_options(base, sel, EditorOptions::default())
    }

    fn with_options(base: &[ScriptBlock], sel: Option<Selection>, options: EditorOptions) -> Self {
        let work = base.to_vec();
        // Undo or another editor may have shrunk the document since the
        // caret was placed; never operate on a stale position.
        let sel = sel.and_then(|s| {
            Some(Selection {
                anchor: clamp_pos(&work, s.anchor)?,
                focus: clamp_pos(&work, s.focus)?,
            })
        });
        Self {
            work,
            sel,
            cmds: Vec::new(),
            scroll: false,
            options,
        }
    }

    fn blocks(&self) -> &[ScriptBlock] {
        &self.work
    }

    fn set_caret(&mut self, at: Pos) {
        self.sel = Some(Selection::caret(at));
    }

    fn set_selection(&mut self, anchor: Pos, focus: Pos) {
        self.sel = Some(Selection { anchor, focus });
    }

    fn nonempty_selection(&self) -> Option<(Pos, Pos)> {
        let (a, f) = self.sel?.sorted();
        (a != f).then_some((a, f))
    }

    /// Apply commands to the working copy, keeping only the ones that were
    /// valid; the same list is what the app records as one user step.
    fn press(&mut self, cmds: Vec<EditCommand>) {
        for cmd in cmds {
            match cmd.clone().apply(&mut self.work) {
                Ok(_) => self.cmds.push(cmd),
                Err(_) => break,
            }
        }
    }

    /// Delete the selection (if any), returning the collapsed caret position.
    fn delete_selection(&mut self) -> Option<Pos> {
        let (a, f) = self.nonempty_selection()?;
        self.press(delete_selection_cmds(self.blocks(), a, f));
        self.sel = Some(Selection::caret(a));
        Some(a)
    }

    fn insert(&mut self, text: &str) {
        if text.is_empty() || self.work.is_empty() {
            return;
        }
        let at = self
            .delete_selection()
            .or_else(|| self.sel.map(|s| s.focus))
            .unwrap_or(Pos {
                bi: 0,
                para: 0,
                ch: 0,
            });
        let Some(block) = self.work.get(at.bi) else {
            return;
        };
        // Scene headings, cues and transitions are uppercase-on-type in KIT;
        // writers should never have to hold Shift or fix case afterwards.
        let upper = auto_upper(block.element_type);
        let old = block.text.clone();
        let prefix = text_upto(&old, at);
        let mut typed = if upper {
            text.to_uppercase()
        } else {
            text.to_string()
        };
        if self.options.smart_quotes {
            typed = smart_quotes(&typed, &prefix);
        }
        if self.options.capitalize_first_word && !upper {
            typed = capitalize_sentence_start(&typed, &prefix);
        }

        let abs = char_index(&old, at);
        let mut new_text: String = old.chars().take(abs).collect();
        new_text.push_str(&typed);
        new_text.extend(old.chars().skip(abs));
        let mut caret_abs = abs + typed.chars().count();
        let mut rewritten = false;
        if self.options.replace_three_dots {
            if let Some((text, caret)) = replace_ellipsis(&new_text, caret_abs) {
                new_text = text;
                caret_abs = caret;
                rewritten = true;
            }
        }
        if self.options.correct_double_capitals {
            if let Some((text, caret)) = correct_double_capitals(&new_text, caret_abs) {
                new_text = text;
                caret_abs = caret;
                rewritten = true;
            }
        }
        if rewritten {
            let caret = pos_at_char(&new_text, at.bi, caret_abs);
            self.press(vec![EditCommand::SetText {
                bi: at.bi,
                text: new_text,
            }]);
            self.sel = Some(Selection::caret(caret));
        } else {
            self.press(vec![EditCommand::InsertText {
                at,
                text: typed.clone(),
            }]);
            self.sel = Some(Selection::caret(advance_pos(at, &typed)));
        }
        self.scroll = true;
    }

    fn paste(&mut self, text: &str, internal: &[(ElementType, String)]) {
        if self.work.is_empty() {
            return;
        }
        let joint = internal
            .iter()
            .map(|(_, s)| s.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if internal.len() > 1 && joint == text {
            self.paste_structured(internal);
        } else if let Some(blocks) = scriptcraft_core::smart_paste::parse(text) {
            self.paste_screenplay(&blocks);
        } else {
            self.insert(text);
        }
    }

    /// Insert recognised screenplay text as typed blocks. An empty host block
    /// adopts the first block's type; otherwise the first block joins the
    /// current block and the rest follow with their detected types.
    fn paste_screenplay(&mut self, blocks: &[ScriptBlock]) {
        if blocks.is_empty() || self.work.is_empty() {
            return;
        }
        let at = self
            .delete_selection()
            .or_else(|| self.sel.map(|s| s.focus))
            .unwrap_or(Pos {
                bi: 0,
                para: 0,
                ch: 0,
            });
        let replace_host = self
            .work
            .get(at.bi)
            .is_some_and(|b| at.para == 0 && at.ch == 0 && b.text.trim().is_empty());
        let first = &blocks[0];

        if replace_host {
            if self.work[at.bi].element_type != first.element_type {
                self.press(vec![EditCommand::SetType {
                    bi: at.bi,
                    t: first.element_type,
                }]);
            }
            self.press(vec![EditCommand::SetText {
                bi: at.bi,
                text: first.text.clone(),
            }]);
        } else {
            let text = match self.work.get(at.bi) {
                Some(b) if auto_upper(b.element_type) => first.text.to_uppercase(),
                _ => first.text.clone(),
            };
            self.press(vec![EditCommand::InsertText {
                at,
                text: text.clone(),
            }]);
        }

        let mut caret = advance_pos(at, &first.text);
        for (offset, block) in blocks[1..].iter().enumerate() {
            let insert_at = at.bi + 1 + offset;
            self.press(vec![EditCommand::InsertBlock {
                at: insert_at,
                block: block.clone(),
            }]);
            caret = advance_pos(
                Pos {
                    bi: insert_at,
                    para: 0,
                    ch: 0,
                },
                &block.text,
            );
        }
        self.sel = Some(Selection::caret(caret));
        self.scroll = true;
    }

    /// Rebuild a multi-block internal copy: the first fragment joins the
    /// current block, the rest come back as blocks of their original types.
    fn paste_structured(&mut self, internal: &[(ElementType, String)]) {
        let at = self
            .delete_selection()
            .or_else(|| self.sel.map(|s| s.focus))
            .unwrap_or(Pos {
                bi: 0,
                para: 0,
                ch: 0,
            });
        let (_, first) = &internal[0];
        let first = match self.work.get(at.bi) {
            Some(b) if auto_upper(b.element_type) => first.to_uppercase(),
            _ => first.clone(),
        };
        self.press(vec![EditCommand::InsertText {
            at,
            text: first.clone(),
        }]);
        let mut caret = advance_pos(at, &first);
        for (insert_at, (t, s)) in (at.bi + 1..).zip(internal[1..].iter()) {
            self.press(vec![EditCommand::InsertBlock {
                at: insert_at,
                block: ScriptBlock::new(*t, s.clone()),
            }]);
            caret = Pos {
                bi: insert_at,
                para: s.split('\n').count() - 1,
                ch: s
                    .rsplit('\n')
                    .next()
                    .map(|p| p.chars().count())
                    .unwrap_or(0),
            };
        }
        self.sel = Some(Selection::caret(caret));
        self.scroll = true;
    }
}

/// Result of one handled key press.
struct KeyAction {
    selection: Option<Selection>,
    commands: Vec<EditCommand>,
    scroll: bool,
}

impl KeyAction {
    fn caret_at(at: Pos) -> Self {
        Self {
            selection: Some(Selection::caret(at)),
            commands: Vec::new(),
            scroll: true,
        }
    }
}

/// Pure key handling: navigation, deletion, splitting/merging, type cycling.
/// Returns `None` for keys the script editor does not own.
fn key_action(
    blocks: &[ScriptBlock],
    doc: &DocLayout,
    sel: Option<Selection>,
    key: egui::Key,
    mods: egui::Modifiers,
    hidden: &[uuid::Uuid],
    omitted_headings: &[uuid::Uuid],
) -> Option<KeyAction> {
    use egui::Key;
    if blocks.is_empty() {
        return None;
    }
    let extend = mods.shift;
    let sel = sel.unwrap_or(Selection::caret(Pos {
        bi: 0,
        para: 0,
        ch: 0,
    }));
    let (a, f) = sel.sorted();
    let caret = sel.focus;
    let collapsed = a == f;

    match key {
        Key::ArrowLeft | Key::ArrowRight => {
            let right = key == Key::ArrowRight;
            if !mods.command && !mods.alt && !extend && !collapsed {
                return Some(KeyAction::caret_at(if right { f } else { a }));
            }
            let target = if mods.command {
                line_boundary(doc, blocks, caret, right)
            } else if mods.alt {
                if right {
                    word_right(blocks, caret)
                } else {
                    word_left(blocks, caret)
                }
            } else if right {
                next_visible_pos(blocks, hidden, caret).unwrap_or(caret)
            } else {
                prev_visible_pos(blocks, hidden, caret).unwrap_or(caret)
            };
            Some(KeyAction {
                selection: Some(if extend {
                    Selection {
                        anchor: sel.anchor,
                        focus: target,
                    }
                } else {
                    Selection::caret(target)
                }),
                commands: Vec::new(),
                scroll: true,
            })
        }
        Key::ArrowUp | Key::ArrowDown => {
            let up = key == egui::Key::ArrowUp;
            if mods.command {
                let target = if up {
                    first_visible_pos(blocks, hidden)
                } else {
                    last_visible_pos(blocks, hidden)
                }
                .unwrap_or(caret);
                return Some(KeyAction {
                    selection: Some(if extend {
                        Selection {
                            anchor: sel.anchor,
                            focus: target,
                        }
                    } else {
                        Selection::caret(target)
                    }),
                    commands: Vec::new(),
                    scroll: true,
                });
            }
            if mods.alt {
                let target = paragraph_edge(blocks, caret, !up);
                return Some(KeyAction {
                    selection: Some(if extend {
                        Selection {
                            anchor: sel.anchor,
                            focus: target,
                        }
                    } else {
                        Selection::caret(target)
                    }),
                    commands: Vec::new(),
                    scroll: true,
                });
            }
            let lines = body_lines(doc);
            let target = caret_line_index(&lines, caret).and_then(|idx| {
                let to = if key == Key::ArrowUp {
                    idx.checked_sub(1)
                } else {
                    (idx + 1 < lines.len()).then_some(idx + 1)
                }?;
                let from = lines[idx].span?;
                let span = lines[to].span?;
                let col = caret.ch.saturating_sub(from.start);
                let ch = span.start + col.min(span.end - span.start);
                Some(Pos {
                    bi: span.bi,
                    para: span.para,
                    ch,
                })
            });
            let Some(target) = target else {
                return Some(KeyAction::caret_at(caret));
            };
            Some(KeyAction {
                selection: Some(if extend {
                    Selection {
                        anchor: sel.anchor,
                        focus: target,
                    }
                } else {
                    Selection::caret(target)
                }),
                commands: Vec::new(),
                scroll: true,
            })
        }
        Key::Home | Key::End => {
            let lines = body_lines(doc);
            let target = caret_line_index(&lines, caret).and_then(|idx| {
                let span = lines[idx].span?;
                Some(Pos {
                    bi: span.bi,
                    para: span.para,
                    ch: if key == Key::Home {
                        span.start
                    } else {
                        span.end
                    },
                })
            });
            let Some(target) = target else {
                return Some(KeyAction::caret_at(caret));
            };
            Some(KeyAction {
                selection: Some(if extend {
                    Selection {
                        anchor: sel.anchor,
                        focus: target,
                    }
                } else {
                    Selection::caret(target)
                }),
                commands: Vec::new(),
                scroll: true,
            })
        }
        Key::Backspace => {
            if !collapsed {
                return Some(KeyAction {
                    selection: Some(Selection::caret(a)),
                    commands: delete_selection_cmds(blocks, a, f),
                    scroll: true,
                });
            }
            if mods.command || mods.alt {
                let from = if mods.command {
                    if caret.ch > 0 {
                        Pos { ch: 0, ..caret }
                    } else {
                        prev_pos(blocks, caret).unwrap_or(caret)
                    }
                } else {
                    word_left(blocks, caret)
                };
                if from == caret {
                    return Some(KeyAction::caret_at(caret));
                }
                return Some(KeyAction {
                    selection: Some(Selection::caret(from)),
                    commands: delete_selection_cmds(blocks, from, caret),
                    scroll: true,
                });
            }
            let (commands, target) = backspace_cmds(blocks, hidden, caret);
            Some(KeyAction {
                selection: Some(Selection::caret(target.unwrap_or(caret))),
                commands,
                scroll: true,
            })
        }
        Key::Delete => {
            if !collapsed {
                return Some(KeyAction {
                    selection: Some(Selection::caret(a)),
                    commands: delete_selection_cmds(blocks, a, f),
                    scroll: true,
                });
            }
            if mods.command || mods.alt {
                let to = if mods.command {
                    let len = paragraph_len(blocks, caret.bi, caret.para);
                    if caret.ch < len {
                        Pos { ch: len, ..caret }
                    } else {
                        next_pos(blocks, caret).unwrap_or(caret)
                    }
                } else {
                    word_right(blocks, caret)
                };
                if to == caret {
                    return Some(KeyAction::caret_at(caret));
                }
                return Some(KeyAction {
                    selection: Some(Selection::caret(caret)),
                    commands: delete_selection_cmds(blocks, caret, to),
                    scroll: true,
                });
            }
            Some(KeyAction {
                selection: Some(Selection::caret(caret)),
                commands: delete_fwd_cmds(blocks, hidden, caret),
                scroll: true,
            })
        }
        Key::Enter => {
            let at = if collapsed { caret } else { a };
            let at_omitted_heading = blocks
                .get(at.bi)
                .is_some_and(|b| omitted_headings.contains(&b.id));
            if at_omitted_heading {
                return Some(KeyAction::caret_at(caret));
            }
            let mut commands = if collapsed {
                Vec::new()
            } else {
                delete_selection_cmds(blocks, a, f)
            };
            let t = blocks.get(at.bi)?.element_type;
            commands.push(EditCommand::SplitBlock {
                at,
                new_type: enter_next(t),
                fuse: at.ch > 0,
            });
            Some(KeyAction {
                selection: Some(Selection::caret(Pos {
                    bi: at.bi + 1,
                    para: 0,
                    ch: 0,
                })),
                commands,
                scroll: true,
            })
        }
        Key::Tab => {
            let t = blocks.get(caret.bi)?.element_type;
            let next_type = if mods.shift { tab_prev(t) } else { tab_next(t) };
            Some(KeyAction {
                selection: Some(sel),
                commands: vec![EditCommand::SetType {
                    bi: caret.bi,
                    t: next_type,
                }],
                scroll: false,
            })
        }
        _ => None,
    }
}

fn hidden_at(blocks: &[ScriptBlock], hidden: &[uuid::Uuid], bi: usize) -> bool {
    blocks.get(bi).is_some_and(|b| hidden.contains(&b.id))
}

fn first_visible_pos(blocks: &[ScriptBlock], hidden: &[uuid::Uuid]) -> Option<Pos> {
    let bi = (0..blocks.len()).find(|&bi| !hidden_at(blocks, hidden, bi))?;
    Some(Pos { bi, para: 0, ch: 0 })
}

fn last_visible_pos(blocks: &[ScriptBlock], hidden: &[uuid::Uuid]) -> Option<Pos> {
    let bi = (0..blocks.len())
        .rev()
        .find(|&bi| !hidden_at(blocks, hidden, bi))?;
    let para = paragraph_count(blocks, bi) - 1;
    Some(Pos {
        bi,
        para,
        ch: paragraph_len(blocks, bi, para),
    })
}

fn paragraph_len(blocks: &[ScriptBlock], bi: usize, para: usize) -> usize {
    blocks
        .get(bi)
        .and_then(|b| b.text.split('\n').nth(para))
        .map(|p| p.chars().count())
        .unwrap_or(0)
}

fn paragraph_count(blocks: &[ScriptBlock], bi: usize) -> usize {
    blocks
        .get(bi)
        .map(|b| b.text.split('\n').count())
        .unwrap_or(0)
}

/// True for characters that stick together as a word for Option-arrow
/// movement. Accented and Cyrillic letters are alphanumeric, so this works
/// for every language the editor supports.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn paragraph_chars(blocks: &[ScriptBlock], p: Pos) -> Vec<char> {
    blocks
        .get(p.bi)
        .and_then(|b| b.text.split('\n').nth(p.para))
        .map(|s| s.chars().collect())
        .unwrap_or_default()
}

/// Start of the word to the left of `p`, or the end of the previous
/// paragraph when `p` sits at the very beginning of its own.
fn word_left(blocks: &[ScriptBlock], p: Pos) -> Pos {
    let chars = paragraph_chars(blocks, p);
    let mut i = p.ch.min(chars.len());
    while i > 0 && !is_word_char(chars[i - 1]) {
        i -= 1;
    }
    while i > 0 && is_word_char(chars[i - 1]) {
        i -= 1;
    }
    if i == p.ch {
        return prev_pos(blocks, p).unwrap_or(p);
    }
    Pos { ch: i, ..p }
}

/// End of the word to the right of `p`, or the start of the next paragraph
/// at the end of the current one.
fn word_right(blocks: &[ScriptBlock], p: Pos) -> Pos {
    let chars = paragraph_chars(blocks, p);
    let len = chars.len();
    let mut i = p.ch.min(len);
    while i < len && !is_word_char(chars[i]) {
        i += 1;
    }
    while i < len && is_word_char(chars[i]) {
        i += 1;
    }
    if i == p.ch {
        return next_pos(blocks, p).unwrap_or(p);
    }
    Pos { ch: i, ..p }
}

/// Start (or end) of the paragraph holding `p`; a second press crosses to
/// the neighbouring paragraph, matching macOS Option+Up/Down.
fn paragraph_edge(blocks: &[ScriptBlock], p: Pos, to_end: bool) -> Pos {
    let len = paragraph_len(blocks, p.bi, p.para);
    if to_end {
        let end = Pos { ch: len, ..p };
        if end != p {
            return end;
        }
        return next_pos(blocks, end)
            .map(|q| Pos {
                ch: paragraph_len(blocks, q.bi, q.para),
                ..q
            })
            .unwrap_or(end);
    }
    let start = Pos { ch: 0, ..p };
    if start != p {
        return start;
    }
    prev_pos(blocks, start)
        .map(|q| Pos { ch: 0, ..q })
        .unwrap_or(start)
}

/// Start or end of the visual (wrapped) line holding `caret`. At a line
/// boundary the motion crosses to the neighbouring line, like macOS.
fn line_boundary(doc: &DocLayout, blocks: &[ScriptBlock], caret: Pos, to_end: bool) -> Pos {
    let lines = body_lines(doc);
    let Some(idx) = caret_line_index(&lines, caret) else {
        return Pos {
            ch: if to_end {
                paragraph_len(blocks, caret.bi, caret.para)
            } else {
                0
            },
            ..caret
        };
    };
    let span = lines[idx].span.expect("body lines always carry a span");
    if to_end {
        if caret.ch < span.end || idx + 1 >= lines.len() {
            Pos {
                ch: span.end,
                ..caret
            }
        } else {
            let next = lines[idx + 1].span.expect("body lines always carry a span");
            Pos {
                bi: next.bi,
                para: next.para,
                ch: next.end,
            }
        }
    } else if caret.ch > span.start || idx == 0 {
        Pos {
            ch: span.start,
            ..caret
        }
    } else {
        let prev = lines[idx - 1].span.expect("body lines always carry a span");
        Pos {
            bi: prev.bi,
            para: prev.para,
            ch: prev.start,
        }
    }
}

fn prev_pos(blocks: &[ScriptBlock], p: Pos) -> Option<Pos> {
    if p.ch > 0 {
        return Some(Pos { ch: p.ch - 1, ..p });
    }
    if p.para > 0 {
        let len = paragraph_len(blocks, p.bi, p.para - 1);
        return Some(Pos {
            para: p.para - 1,
            ch: len,
            ..p
        });
    }
    let bi = p.bi.checked_sub(1)?;
    let para = paragraph_count(blocks, bi).saturating_sub(1);
    Some(Pos {
        bi,
        para,
        ch: paragraph_len(blocks, bi, para),
    })
}

fn next_pos(blocks: &[ScriptBlock], p: Pos) -> Option<Pos> {
    let len = paragraph_len(blocks, p.bi, p.para);
    if p.ch < len {
        return Some(Pos { ch: p.ch + 1, ..p });
    }
    if p.para + 1 < paragraph_count(blocks, p.bi) {
        return Some(Pos {
            para: p.para + 1,
            ch: 0,
            ..p
        });
    }
    Some(Pos {
        bi: p.bi + 1,
        para: 0,
        ch: 0,
    })
    .filter(|q| q.bi < blocks.len())
}

fn prev_visible_pos(blocks: &[ScriptBlock], hidden: &[uuid::Uuid], p: Pos) -> Option<Pos> {
    let mut q = prev_pos(blocks, p)?;
    if hidden_at(blocks, hidden, q.bi) {
        let mut bi = q.bi;
        while hidden_at(blocks, hidden, bi) {
            bi = bi.checked_sub(1)?;
        }
        let para = paragraph_count(blocks, bi) - 1;
        q = Pos {
            bi,
            para,
            ch: paragraph_len(blocks, bi, para),
        };
    }
    Some(q)
}

fn next_visible_pos(blocks: &[ScriptBlock], hidden: &[uuid::Uuid], p: Pos) -> Option<Pos> {
    let mut q = next_pos(blocks, p)?;
    if hidden_at(blocks, hidden, q.bi) {
        let mut bi = q.bi;
        while bi < blocks.len() && hidden_at(blocks, hidden, bi) {
            bi += 1;
        }
        if bi >= blocks.len() {
            return None;
        }
        q = Pos { bi, para: 0, ch: 0 };
    }
    Some(q)
}

/// Position after inserting `text` at `at` (paragraphs advance, not blocks).
fn advance_pos(at: Pos, text: &str) -> Pos {
    let newlines = text.chars().filter(|&c| c == '\n').count();
    if newlines == 0 {
        Pos {
            ch: at.ch + text.chars().count(),
            ..at
        }
    } else {
        Pos {
            para: at.para + newlines,
            ch: text
                .rsplit('\n')
                .next()
                .map(|p| p.chars().count())
                .unwrap_or(0),
            ..at
        }
    }
}

/// KIT's smart quotes: a straight quote after whitespace, an opening bracket
/// or another opening quote becomes typographic; an apostrophe inside a word
/// becomes the right single quote.
fn smart_quotes(typed: &str, prefix: &str) -> String {
    let mut out = String::with_capacity(typed.len());
    let mut prev = prefix.chars().last();
    for c in typed.chars() {
        let c = match c {
            '"' => {
                if opens_quote(prev) {
                    '“'
                } else {
                    '”'
                }
            }
            '\'' => {
                if prev.is_some_and(|p| p.is_alphanumeric()) {
                    '’'
                } else if opens_quote(prev) {
                    '‘'
                } else {
                    '’'
                }
            }
            _ => c,
        };
        out.push(c);
        prev = Some(c);
    }
    out
}

fn opens_quote(prev: Option<char>) -> bool {
    match prev {
        None => true,
        Some(c) => c.is_whitespace() || "([{—–-\"'“‘".contains(c),
    }
}

/// KIT's "Capitalize first word": uppercase the first letter typed at the
/// beginning of a paragraph or after `.`, `!`, `?` or an ellipsis.
fn capitalize_sentence_start(typed: &str, prefix: &str) -> String {
    if !at_sentence_start(prefix) {
        return typed.to_string();
    }
    let mut out = String::with_capacity(typed.len());
    let mut chars = typed.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_alphabetic() {
            out.extend(c.to_uppercase());
            out.extend(chars);
            return out;
        }
        out.push(c);
        if !(c.is_whitespace() || "([{\"'“‘—–-".contains(c)) {
            // Something that is not opening punctuation came first; do not
            // capitalize later words in the same insertion.
            out.extend(chars);
            return out;
        }
    }
    out
}

fn at_sentence_start(prefix: &str) -> bool {
    let mut chars = prefix.chars().rev();
    let Some(last) = chars.next() else {
        return true;
    };
    if last == '\n' {
        return true;
    }
    if last.is_whitespace() {
        for c in chars {
            if c == '\n' {
                return true;
            }
            if !c.is_whitespace() {
                return matches!(c, '.' | '!' | '?' | '…');
            }
        }
        return true;
    }
    matches!(last, '.' | '!' | '?' | '…')
}

/// Replace three typed dots with an ellipsis when the run touches the caret.
fn replace_ellipsis(text: &str, caret: usize) -> Option<(String, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let lo = caret.saturating_sub(3);
    let mut out = Vec::with_capacity(chars.len());
    let mut i = 0;
    let mut caret_out = caret;
    let mut changed = false;
    while i < chars.len() {
        let near_caret = i >= lo && i <= caret + 2;
        if near_caret
            && i + 2 < chars.len()
            && chars[i] == '.'
            && chars[i + 1] == '.'
            && chars[i + 2] == '.'
        {
            out.push('…');
            if caret > i + 2 {
                caret_out -= 2;
            } else if caret >= i {
                caret_out = out.len();
            }
            i += 3;
            changed = true;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    changed.then(|| (out.into_iter().collect(), caret_out))
}

/// KIT's "Correct DOuble CApitals": lowercase the second capital of a word
/// like `DOuble` as soon as the first lowercase letter follows it.
fn correct_double_capitals(text: &str, caret: usize) -> Option<(String, usize)> {
    let mut chars: Vec<char> = text.chars().collect();
    let caret = caret.min(chars.len());
    let mut start = caret;
    while start > 0 && is_word_char(chars[start - 1]) {
        start -= 1;
    }
    let mut end = caret;
    while end < chars.len() && is_word_char(chars[end]) {
        end += 1;
    }
    if end.saturating_sub(start) >= 3
        && chars[start].is_uppercase()
        && chars[start + 1].is_uppercase()
        && chars[start + 2].is_lowercase()
    {
        chars[start + 1] = chars[start + 1].to_lowercase().next()?;
        return Some((chars.into_iter().collect(), caret));
    }
    None
}

fn paras_of(text: &str) -> Vec<&str> {
    text.split('\n').collect()
}

/// Text of a block up to (not including) `p`.
fn text_upto(text: &str, p: Pos) -> String {
    let mut out = String::new();
    for (i, para) in paras_of(text).iter().enumerate() {
        if i == p.para {
            out.extend(para.chars().take(p.ch));
            break;
        }
        out.push_str(para);
        out.push('\n');
    }
    out
}

/// Text of a block from `p` to its end.
fn text_from(text: &str, p: Pos) -> String {
    let mut out = String::new();
    for (i, para) in paras_of(text).iter().enumerate().skip(p.para) {
        if i == p.para {
            out.extend(para.chars().skip(p.ch));
        } else {
            out.push('\n');
            out.push_str(para);
        }
    }
    out
}

/// Text between two positions inside one block.
fn text_within(text: &str, a: Pos, f: Pos) -> String {
    let ps = paras_of(text);
    if a.para == f.para {
        return ps[a.para].chars().skip(a.ch).take(f.ch - a.ch).collect();
    }
    let mut out: String = ps[a.para].chars().skip(a.ch).collect();
    for para in &ps[a.para + 1..f.para] {
        out.push('\n');
        out.push_str(para);
    }
    out.push('\n');
    out.extend(ps[f.para].chars().take(f.ch));
    out
}

/// Text between two positions, blocks joined with paragraph breaks.
fn text_between(blocks: &[ScriptBlock], a: Pos, f: Pos) -> String {
    if a.bi == f.bi {
        return text_within(&blocks[a.bi].text, a, f);
    }
    let mut out = text_from(&blocks[a.bi].text, a);
    out.push('\n');
    for block in &blocks[a.bi + 1..f.bi] {
        out.push_str(&block.text);
        out.push('\n');
    }
    out.push_str(&text_upto(&blocks[f.bi].text, f));
    out
}

/// The selection as typed fragments, one per touched block — the clipboard
/// format that lets paste recreate element types.
fn fragments_between(blocks: &[ScriptBlock], a: Pos, f: Pos) -> Vec<(ElementType, String)> {
    if a.bi == f.bi {
        return vec![(
            blocks[a.bi].element_type,
            text_within(&blocks[a.bi].text, a, f),
        )];
    }
    let mut out = vec![(blocks[a.bi].element_type, text_from(&blocks[a.bi].text, a))];
    for block in &blocks[a.bi + 1..f.bi] {
        out.push((block.element_type, block.text.clone()));
    }
    out.push((blocks[f.bi].element_type, text_upto(&blocks[f.bi].text, f)));
    out
}

/// Commands deleting a (possibly cross-block) range; the caret lands at `a`.
fn delete_selection_cmds(blocks: &[ScriptBlock], a: Pos, f: Pos) -> Vec<EditCommand> {
    if a == f {
        return Vec::new();
    }
    if a.bi == f.bi {
        return vec![EditCommand::DeleteRange { start: a, end: f }];
    }
    let head = text_upto(&blocks[a.bi].text, a);
    let tail = text_from(&blocks[f.bi].text, f);
    let mut cmds = vec![EditCommand::SetText {
        bi: a.bi,
        text: format!("{head}{tail}"),
    }];
    for bi in ((a.bi + 1)..=f.bi).rev() {
        cmds.push(EditCommand::DeleteBlock { bi });
    }
    cmds
}

/// Backspace at a position; returns the commands and the resulting caret.
fn backspace_cmds(
    blocks: &[ScriptBlock],
    hidden: &[uuid::Uuid],
    p: Pos,
) -> (Vec<EditCommand>, Option<Pos>) {
    if p.para == 0 && p.ch == 0 {
        if p.bi == 0 {
            return (Vec::new(), None);
        }
        // Never merge the heading into a hidden body: omission is a view
        // state, not permission to rewrite the stored scene.
        if hidden_at(blocks, hidden, p.bi - 1) {
            return (Vec::new(), Some(p));
        }
        let upper = p.bi - 1;
        let para = paragraph_count(blocks, upper).saturating_sub(1);
        let caret = Pos {
            bi: upper,
            para,
            ch: paragraph_len(blocks, upper, para),
        };
        // Word-style: backspace on a block break joins the two paragraphs
        // into one line (the caret stays at the junction).
        return (
            vec![EditCommand::MergeBlocks {
                bi: p.bi,
                fuse: true,
            }],
            Some(caret),
        );
    }
    let Some(prev) = prev_visible_pos(blocks, hidden, p) else {
        return (Vec::new(), None);
    };
    (
        vec![EditCommand::DeleteRange {
            start: prev,
            end: p,
        }],
        Some(prev),
    )
}

/// Forward delete at a position (Delete key), including joining the next
/// block when pressed at the very end.
fn delete_fwd_cmds(blocks: &[ScriptBlock], hidden: &[uuid::Uuid], p: Pos) -> Vec<EditCommand> {
    let last_para = paragraph_count(blocks, p.bi).saturating_sub(1);
    let at_end = p.para == last_para && p.ch == paragraph_len(blocks, p.bi, last_para);
    if at_end {
        if p.bi + 1 < blocks.len() {
            if hidden_at(blocks, hidden, p.bi + 1) {
                return Vec::new();
            }
            return vec![EditCommand::MergeBlocks {
                bi: p.bi + 1,
                fuse: p.ch > 0,
            }];
        }
        return Vec::new();
    }
    match next_visible_pos(blocks, hidden, p) {
        Some(next) => vec![EditCommand::DeleteRange {
            start: p,
            end: next,
        }],
        None => Vec::new(),
    }
}

/// All body lines in document order (what the caret can stand on).
fn body_lines(doc: &DocLayout) -> Vec<&PlacedLine> {
    doc.pages
        .iter()
        .flat_map(|p| p.lines.iter())
        .filter(|l| l.role == LineRole::Body && l.span.is_some())
        .collect()
}

/// Index into [`body_lines`] of the display line holding `caret`.
fn caret_line_index(lines: &[&PlacedLine], caret: Pos) -> Option<usize> {
    let mut best: Option<(usize, usize)> = None;
    for (i, line) in lines.iter().enumerate() {
        let Some(span) = line.span else { continue };
        if span.bi != caret.bi || span.para != caret.para {
            continue;
        }
        // Prefer the line whose range contains the caret; a caret exactly on
        // a wrap boundary belongs to the following line (Word-like).
        if caret.ch >= span.start && caret.ch <= span.end {
            best = Some((i, 0));
        } else if best.is_none() {
            let d = if caret.ch < span.start {
                span.start - caret.ch
            } else {
                caret.ch - span.end
            };
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((i, d));
            }
        }
    }
    best.map(|(i, _)| i)
}

/// Horizontal origin (left edge of the first character) of a placed line.
fn line_origin_x(prect: egui::Rect, line: &PlacedLine, scale: f32, cell_w: f32) -> f32 {
    let anchor = prect.left() + PAD_L_MM * scale + line.x_mm * scale;
    if line.right_aligned {
        let len = line.span.map(|s| s.end - s.start).unwrap_or(0);
        anchor - len as f32 * cell_w
    } else {
        anchor
    }
}

/// Screen point at the middle of the character `at`; interaction tests
/// right-click through this.
#[doc(hidden)]
pub fn char_point(
    doc: &DocLayout,
    page_rects: &[egui::Rect],
    scale: f32,
    row_h: f32,
    cell_w: f32,
    at: Pos,
) -> Option<egui::Pos2> {
    for (pi, page) in doc.pages.iter().enumerate() {
        for line in &page.lines {
            let Some(span) = line.span else {
                continue;
            };
            if span.bi == at.bi && span.para == at.para && span.start <= at.ch && at.ch < span.end {
                let prect = *page_rects.get(pi)?;
                let origin = line_origin_x(prect, line, scale, cell_w);
                let x = origin + (at.ch - span.start) as f32 * cell_w + cell_w * 0.5;
                let y = prect.top() + PAD_T_MM * scale + line.row as f32 * row_h + row_h * 0.5;
                return Some(egui::pos2(x, y));
            }
        }
    }
    None
}

/// Columns `[c0, c1)` of `line` covered by the selection `[a, f)`.
fn selection_columns(line: &PlacedLine, a: Pos, f: Pos) -> Option<(usize, usize)> {
    let s = line.span?;
    let line_lo = Pos {
        bi: s.bi,
        para: s.para,
        ch: s.start,
    };
    let line_hi = Pos {
        bi: s.bi,
        para: s.para,
        ch: s.end,
    };
    if line_hi <= a || line_lo >= f {
        return None;
    }
    let same_a = a.bi == s.bi && a.para == s.para;
    let same_f = f.bi == s.bi && f.para == s.para;
    let c0 = if same_a {
        a.ch.saturating_sub(s.start)
    } else {
        0
    };
    let c1 = if same_f {
        f.ch.min(s.end).saturating_sub(s.start)
    } else {
        s.end - s.start
    };
    (c1 > c0).then_some((c0, c1))
}

/// Map a pointer position to a caret position (nearest body line and cell).
fn hit_test(
    doc: &DocLayout,
    page_rects: &[egui::Rect],
    scale: f32,
    row_h: f32,
    cell_w: f32,
    pos: egui::Pos2,
) -> Option<Pos> {
    if doc.pages.is_empty() || page_rects.is_empty() {
        return None;
    }
    let dist = |r: &egui::Rect| {
        if pos.y < r.top() {
            r.top() - pos.y
        } else if pos.y > r.bottom() {
            pos.y - r.bottom()
        } else {
            0.0
        }
    };
    let pi = page_rects
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| dist(a).total_cmp(&dist(b)))
        .map(|(i, _)| i)?;
    let prect = page_rects[pi];
    let content_top = prect.top() + PAD_T_MM * scale;
    let row = (((pos.y - content_top) / row_h).floor()).max(0.0) as usize;
    if let Some(line) = doc.pages[pi]
        .lines
        .iter()
        .filter(|l| l.role == LineRole::Body && l.span.is_some())
        .min_by_key(|l| l.row.abs_diff(row))
    {
        return position_in_line(line, prect, scale, cell_w, pos.x);
    }
    // Empty page (only synthetic rows): use the next page's first body line,
    // else the previous page's last one.
    for page in &doc.pages[pi + 1..] {
        if let Some(line) = page
            .lines
            .iter()
            .find(|l| l.role == LineRole::Body && l.span.is_some())
        {
            let span = line.span?;
            return Some(Pos {
                bi: span.bi,
                para: span.para,
                ch: span.start,
            });
        }
    }
    for page in doc.pages[..pi].iter().rev() {
        if let Some(line) = page
            .lines
            .iter()
            .rev()
            .find(|l| l.role == LineRole::Body && l.span.is_some())
        {
            let span = line.span?;
            return Some(Pos {
                bi: span.bi,
                para: span.para,
                ch: span.end,
            });
        }
    }
    None
}

fn position_in_line(
    line: &PlacedLine,
    prect: egui::Rect,
    scale: f32,
    cell_w: f32,
    x: f32,
) -> Option<Pos> {
    let span = line.span?;
    let len = span.end - span.start;
    let origin = line_origin_x(prect, line, scale, cell_w);
    let col = ((x - origin) / cell_w).round().clamp(0.0, len as f32) as usize;
    Some(Pos {
        bi: span.bi,
        para: span.para,
        ch: span.start + col,
    })
}

/// Screen rectangle of the caret plus its page (caret, page, page index).
fn caret_geometry(
    doc: &DocLayout,
    page_rects: &[egui::Rect],
    scale: f32,
    row_h: f32,
    cell_w: f32,
    at: Pos,
) -> Option<(usize, egui::Rect, egui::Rect)> {
    let mut found: Option<(usize, &PlacedLine)> = None;
    for (pi, page) in doc.pages.iter().enumerate() {
        for line in &page.lines {
            let Some(span) = line.span else { continue };
            if span.bi == at.bi && span.para == at.para && at.ch >= span.start && at.ch <= span.end
            {
                found = Some((pi, line));
            }
        }
    }
    let (pi, line) = found?;
    let prect = *page_rects.get(pi)?;
    let span = line.span?;
    let col = at.ch.clamp(span.start, span.end) - span.start;
    let x = line_origin_x(prect, line, scale, cell_w) + col as f32 * cell_w;
    let y = prect.top() + PAD_T_MM * scale + line.row as f32 * row_h;
    let caret = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(2.0, row_h));
    Some((pi, prect, caret))
}

/// KIT page numbers: nothing on page one, "2."… in the top margin after.
fn paint_page_number(
    painter: &egui::Painter,
    prect: egui::Rect,
    page: &PageLayout,
    body: f32,
    row_h: f32,
    scale: f32,
) {
    let Some(n) = page.number.as_deref() else {
        return;
    };
    painter.text(
        egui::pos2(
            prect.right() - PAD_R_MM * scale,
            prect.top() + PAD_T_MM * scale - row_h * 0.4,
        ),
        egui::Align2::RIGHT_BOTTOM,
        format!("{n}."),
        egui::FontId::new(body, MONO),
        kit().disabled_text,
    );
}

#[derive(Clone, Copy)]
struct LinePaint {
    body: f32,
    scale: f32,
    row_h: f32,
    revision: Option<RevisionColor>,
}

fn paint_line(
    painter: &egui::Painter,
    prect: egui::Rect,
    blocks: &[ScriptBlock],
    line: &PlacedLine,
    paint: LinePaint,
) {
    let LinePaint {
        body,
        scale,
        row_h,
        revision,
    } = paint;
    let c = kit();
    let family = if line.bold {
        bold_mono()
    } else if line.italic {
        italic_mono()
    } else {
        MONO
    };
    let font = egui::FontId::new(body, family);
    let color = line_color(line, blocks, &c);
    let cell = MM_PER_CELL * scale;
    let galley = line_galley(painter, &line.text, font, color, cell);
    let size = galley.size();
    let x = prect.left() + PAD_L_MM * scale + line.x_mm * scale;
    let y = prect.top() + PAD_T_MM * scale + line.row as f32 * row_h;
    if matches!(line.role, LineRole::Body | LineRole::Omitted) {
        if let Some(revision) = revision {
            let mark = egui::Rect::from_min_size(
                egui::pos2(prect.left() + REVISION_BAR_X_MM * scale, y),
                egui::vec2(REVISION_BAR_W_MM * scale, row_h),
            );
            paint_revision(painter, mark, revision, 0.0);
        }
    }
    let pos = match line.role {
        // Hanging scene number: right-aligned in the margin, centered on
        // its heading row like the old editor.
        LineRole::SceneNumber => egui::pos2(x - size.x, y + (row_h - size.y) * 0.5),
        _ if line.right_aligned => egui::pos2(x - size.x, y),
        _ => egui::pos2(x, y),
    };
    painter.galley(pos, galley, color);
}

/// Underline the misspellings that fall inside one painted body line.
///
/// `flags` is cached per `(block, paragraph)` and keyed by the paragraph's
/// text hash, so repeated lines of a wrapped paragraph are cheap and an edit
/// invalidates them automatically.
fn paint_spelling(
    painter: &egui::Painter,
    prect: egui::Rect,
    blocks: &[ScriptBlock],
    line: &PlacedLine,
    paint: LinePaint,
    cell_w: f32,
) {
    if !crate::spell::is_enabled() {
        return;
    }
    let Some(span) = line.span else {
        return;
    };
    let Some(para_text) = blocks
        .get(span.bi)
        .and_then(|b| b.text.split('\n').nth(span.para))
    else {
        return;
    };
    let marks = crate::spell::flags(span.bi, span.para, para_text);
    if marks.is_empty() {
        return;
    }
    let color = kit().spell_error;
    let origin = line_origin_x(prect, line, paint.scale, cell_w);
    let y = prect.top() + PAD_T_MM * paint.scale + line.row as f32 * paint.row_h;
    let underline_y = y + paint.row_h * 0.82;
    for mark in marks {
        let c0 = mark.start.max(span.start);
        let c1 = mark.end.min(span.end);
        if c1 > c0 {
            paint_squiggle(
                painter,
                origin + (c0 - span.start) as f32 * cell_w,
                origin + (c1 - span.start) as f32 * cell_w,
                underline_y,
                color,
                paint.row_h,
            );
        }
    }
}

/// A small zigzag, the familiar "this word is not in the dictionary" mark.
fn paint_squiggle(
    painter: &egui::Painter,
    x0: f32,
    x1: f32,
    y: f32,
    color: egui::Color32,
    row_h: f32,
) {
    let step = (row_h * 0.22).max(2.0);
    let amp = (row_h * 0.08).max(0.8);
    let mut points = Vec::new();
    let mut x = x0;
    let mut up = true;
    while x < x1 {
        points.push(egui::pos2(x, if up { y - amp } else { y + amp }));
        up = !up;
        x += step;
    }
    points.push(egui::pos2(x1.max(x0 + 1.0), y));
    painter.add(egui::Shape::line(
        points,
        egui::Stroke::new((row_h * 0.09).max(1.0), color),
    ));
}

/// One galley per line with every character pinned to the monospace cell
/// grid (`cell` points per character). epaint quantizes glyph advances
/// (font hinting, per-fallback-font metrics), so painting a whole string in
/// one run makes the text block widen/narrow relative to the page as the
/// zoom changes. Pinning each character keeps the block exactly `n * cell`
/// wide at every zoom and aligned with the caret/selection grid.
fn line_galley(
    painter: &egui::Painter,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
    cell: f32,
) -> std::sync::Arc<egui::Galley> {
    // Advance of each character in this font. Used only to know where the
    // next character would land; `leading_space` then moves it onto the grid.
    let advances: Vec<f32> = painter
        .ctx()
        .fonts(|f| text.chars().map(|ch| f.glyph_width(&font, ch)).collect());
    let mut job = egui::text::LayoutJob {
        break_on_newline: false,
        ..Default::default()
    };
    job.wrap.max_width = f32::INFINITY;
    // epaint rounds the pen position to the pixel grid after every glyph
    // (`Font::round_to_pixel`); mirror that here so the next `leading_space`
    // lands the glyph exactly on its cell despite the rounding.
    let ppp = painter.pixels_per_point();
    let round_px = |x: f32| (x * ppp).round() / ppp;
    let mut cursor = 0.0_f32;
    for (i, (ch, adv)) in text.chars().zip(advances).enumerate() {
        let target = i as f32 * cell;
        let mut buf = [0_u8; 4];
        job.append(
            ch.encode_utf8(&mut buf),
            target - cursor,
            egui::TextFormat::simple(font.clone(), color),
        );
        cursor = round_px(target + adv);
    }
    painter.layout_job(job)
}

/// Same colors as the old chunk editor: headings light, notes muted.
fn line_color(line: &PlacedLine, blocks: &[ScriptBlock], c: &KitColors) -> egui::Color32 {
    match line.role {
        LineRole::SceneNumber => c.disabled_text,
        LineRole::Omitted => c.disabled_text,
        LineRole::Body => match line.span.and_then(|s| blocks.get(s.bi)) {
            Some(b) if b.element_type == ElementType::NonPrintable => c.disabled_text,
            Some(b) if b.element_type == ElementType::SceneHeading => c.heading_text,
            _ => c.text,
        },
        _ => c.text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scriptcraft_core::editing::RUSSIAN;

    fn sample() -> Vec<ScriptBlock> {
        vec![
            ScriptBlock::new(ElementType::SceneHeading, "INT. СТАЯ - ДЕН".into()),
            ScriptBlock::new(ElementType::Action, "Първи ред.\nВтори ред.".into()),
            ScriptBlock::new(ElementType::Character, "ИВАН".into()),
            ScriptBlock::new(ElementType::Dialogue, "Здравей!".into()),
            ScriptBlock::new(ElementType::Transition, "ПРЕХОД:".into()),
        ]
    }

    fn apply_all(blocks: &mut Vec<ScriptBlock>, cmds: &[EditCommand]) {
        for c in cmds {
            c.clone().apply(blocks).expect("command must apply");
        }
    }

    #[test]
    fn zoom_never_changes_breaks() {
        let blocks = sample();
        let labels = vec!["1.".to_string()];
        let base = document_layout(&blocks, &RUSSIAN, &labels);
        assert!(!base.pages.is_empty());
        // The painter consumes this exact layout at every zoom; zoom only
        // scales painted geometry (see `painted_geometry_scales_linearly`).
        let again = document_layout(&blocks, &RUSSIAN, &labels);
        assert_eq!(again, base, "layout must be deterministic");
    }

    #[test]
    fn typing_uppercases_heading_cues_and_transitions() {
        let mut blocks = sample();
        let mut ed = Editor::new(
            &blocks,
            Some(Selection::caret(Pos {
                bi: 0,
                para: 0,
                ch: 0,
            })),
        );
        ed.insert("int. ");
        apply_all(&mut blocks, &ed.cmds);
        assert!(
            blocks[0].text.starts_with("INT. "),
            "scene heading typed in caps: {}",
            blocks[0].text
        );
        assert_eq!(
            ed.sel,
            Some(Selection::caret(Pos {
                bi: 0,
                para: 0,
                ch: 5
            }))
        );

        let mut ed = Editor::new(
            &blocks,
            Some(Selection::caret(Pos {
                bi: 1,
                para: 0,
                ch: 0,
            })),
        );
        ed.insert("lower");
        apply_all(&mut blocks, &ed.cmds);
        assert!(
            blocks[1].text.starts_with("lower"),
            "action keeps the typed case"
        );

        let mut ed = Editor::new(
            &blocks,
            Some(Selection::caret(Pos {
                bi: 2,
                para: 0,
                ch: 0,
            })),
        );
        ed.insert("ivan ");
        apply_all(&mut blocks, &ed.cmds);
        assert!(
            blocks[2].text.starts_with("IVAN "),
            "character cue typed in caps: {}",
            blocks[2].text
        );
    }

    #[test]
    fn painted_geometry_scales_linearly_with_zoom() {
        let a = page_size_pt(0.95);
        let b = page_size_pt(1.9);
        assert!((b.x - a.x * 2.0).abs() < 1e-3, "width must scale linearly");
        assert!((b.y - a.y * 2.0).abs() < 1e-3, "height must scale linearly");
        // At zoom 1.0 the page is exactly A4 at 72 dpi.
        let one = page_size_pt(1.0);
        assert!((one.x - 595.276).abs() < 0.01);
        assert!((one.y - 841.890).abs() < 0.01);
    }

    /// Painted lines must occupy exactly `n * cell` points at every zoom:
    /// epaint's quantized advances would otherwise make text "breathe"
    /// sideways relative to the page while zooming.
    #[test]
    fn painted_lines_snap_every_glyph_to_the_cell_grid() {
        let ctx = egui::Context::default();
        crate::app::install_fonts(&ctx);
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        let mut cases: Vec<(f32, f32, f32, Vec<f32>)> = Vec::new();
        let _ = ctx.run(raw, |ctx| {
            let painter =
                egui::Painter::new(ctx.clone(), egui::LayerId::debug(), egui::Rect::EVERYTHING);
            for zoom in [1.0_f32, 0.88, 0.79, 0.55, 0.3, 0.25] {
                let font = egui::FontId::new(12.0 * zoom, MONO);
                let cell = MM_PER_CELL * PT_PER_MM * zoom;
                for text in [
                    "СИВА ОБСТАНОВКА: Изгревът мъгла виси над калния паркинг",
                    "KADET MORNING - МОМЕНТАЛНО ОТКРИВАНЕ",
                    "иИмМ аА 123",
                ] {
                    let galley =
                        line_galley(&painter, text, font.clone(), egui::Color32::WHITE, cell);
                    let n = text.chars().count() as f32;
                    let positions = galley
                        .rows
                        .iter()
                        .flat_map(|row| row.glyphs.iter().map(|g| g.pos.x))
                        .collect();
                    cases.push((zoom, galley.rect.width(), n * cell, positions));
                }
            }
        });
        for (zoom, painted, grid, positions) in cases {
            let cell = grid / positions.len() as f32;
            for (i, &x) in positions.iter().enumerate() {
                let target = i as f32 * cell;
                assert!(
                    (x - target).abs() < 0.05,
                    "zoom {zoom}: glyph {i} at {x} must sit on {target}"
                );
            }
            assert!(
                (painted - grid).abs() < 0.8,
                "zoom {zoom}: painted width {painted} must track the {grid}-point grid"
            );
        }
    }

    #[test]
    fn zoom_anchor_keeps_the_document_point_pinned() {
        let viewport = egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(900.0, 700.0));
        let anchor = viewport.center();
        let a = anchor - viewport.min;
        // Scrolled down to page 1 at 100%; zoom in about the pointer.
        let prev_offset = egui::vec2(0.0, 300.0);
        let new = anchored_scroll_offset(prev_offset, 1.0, 2.0, anchor, viewport);
        let prev_page = page_size_pt(1.0);
        let new_page = page_size_pt(2.0);
        let prev_left = (viewport.width().max(prev_page.x + 2.0) - prev_page.x) * 0.5;
        let new_left = (viewport.width().max(new_page.x + 2.0) - new_page.x) * 0.5;
        let doc_x = (prev_offset.x + a.x - prev_left) / 1.0;
        let doc_y = (prev_offset.y + a.y) / 1.0;
        let after_x = (new.x + a.x - new_left) / 2.0;
        let after_y = (new.y + a.y) / 2.0;
        assert!((after_x - doc_x).abs() < 1e-3, "horizontal anchor drifted");
        assert!((after_y - doc_y).abs() < 1e-3, "vertical anchor drifted");

        // Zooming out from an overflowing page recenters it (offset clamps
        // to zero) instead of leaving a negative scroll.
        let out = anchored_scroll_offset(new, 2.0, 1.0, anchor, viewport);
        assert!(out.x >= 0.0 && out.y >= 0.0);
    }

    #[test]
    fn long_documents_paginate_the_same_at_every_zoom() {
        let mut blocks = vec![ScriptBlock::new(
            ElementType::SceneHeading,
            "EXT. ПЪТ - ДЕН".into(),
        )];
        for i in 0..400 {
            blocks.push(ScriptBlock::new(
                ElementType::Action,
                format!("Ред номер {i} от дългия документ."),
            ));
        }
        let labels = vec!["1.".to_string()];
        let base = document_layout(&blocks, &RUSSIAN, &labels);
        assert!(base.pages.len() > 1, "the document must span several pages");
        // Zoom only scales painted geometry; the layout is zoom-independent.
        let again = document_layout(&blocks, &RUSSIAN, &labels);
        assert_eq!(again, base, "layout must be deterministic");
    }

    #[test]
    fn caret_navigation_crosses_paragraphs_and_blocks() {
        let b = sample();
        // End of the first paragraph of block 1.
        let p = Pos {
            bi: 1,
            para: 0,
            ch: 10,
        };
        assert_eq!(
            next_pos(&b, p),
            Some(Pos {
                bi: 1,
                para: 1,
                ch: 0
            })
        );
        // End of the last paragraph: move into the next block.
        let end = Pos {
            bi: 1,
            para: 1,
            ch: 10,
        };
        assert_eq!(
            next_pos(&b, end),
            Some(Pos {
                bi: 2,
                para: 0,
                ch: 0
            })
        );
        assert_eq!(
            prev_pos(
                &b,
                Pos {
                    bi: 2,
                    para: 0,
                    ch: 0
                }
            ),
            Some(Pos {
                bi: 1,
                para: 1,
                ch: 10
            })
        );
        assert_eq!(
            prev_pos(
                &b,
                Pos {
                    bi: 0,
                    para: 0,
                    ch: 0
                }
            ),
            None
        );
        assert_eq!(
            next_pos(
                &b,
                Pos {
                    bi: 4,
                    para: 0,
                    ch: 7
                }
            ),
            None,
            "end of the last block has nowhere to go"
        );
    }

    #[test]
    fn up_down_keep_the_column_across_wrapped_lines() {
        let blocks = vec![
            ScriptBlock::new(
                ElementType::Action,
                "Един достатъчно дълъг ред, който се пренася на няколко реда в колоната.".into(),
            ),
            ScriptBlock::new(ElementType::Action, "Кратък.".into()),
        ];
        let labels: Vec<String> = Vec::new();
        let doc = document_layout(&blocks, &RUSSIAN, &labels);
        let lines = body_lines(&doc);
        assert!(lines.len() >= 3, "first block must wrap");
        let first = lines[0].span.unwrap();
        let caret = Pos {
            bi: first.bi,
            para: first.para,
            ch: first.start + 5,
        };
        let down = key_action(
            &blocks,
            &doc,
            Some(Selection::caret(caret)),
            egui::Key::ArrowDown,
            egui::Modifiers::NONE,
            &[],
            &[],
        )
        .expect("arrow down is handled");
        let target = down.selection.unwrap().focus;
        assert_eq!(target.para, 0, "still in the first paragraph");
        assert_eq!(target.ch, lines[1].span.unwrap().start + 5, "column kept");
        // Moving down past the last line of block 0 keeps the column into
        // block 1 but clamps to its length.
        let last = lines.len() - 1;
        let last_span = lines[last].span.unwrap();
        let down2 = key_action(
            &blocks,
            &doc,
            Some(Selection::caret(Pos {
                bi: last_span.bi,
                para: last_span.para,
                ch: last_span.start,
            })),
            egui::Key::ArrowDown,
            egui::Modifiers::NONE,
            &[],
            &[],
        )
        .expect("handled");
        assert_eq!(
            down2.selection.unwrap().focus,
            Pos {
                bi: last_span.bi,
                para: last_span.para,
                ch: last_span.start
            },
            "no line below"
        );
    }

    #[test]
    fn selection_delete_joins_blocks_and_keeps_the_head() {
        let mut b = sample();
        let a = Pos {
            bi: 1,
            para: 0,
            ch: 6,
        };
        let f = Pos {
            bi: 3,
            para: 0,
            ch: 2,
        };
        let cmds = delete_selection_cmds(&b, a, f);
        apply_all(&mut b, &cmds);
        assert_eq!(b.len(), 3, "blocks 2 and 3 were removed");
        assert_eq!(b[1].text, "Първи равей!");
        assert_eq!(b[1].element_type, ElementType::Action);
        assert_eq!(b[2].element_type, ElementType::Transition);
    }

    #[test]
    fn selection_delete_keeps_types_and_text_of_remaining_blocks() {
        let mut b = sample();
        let a = Pos {
            bi: 2,
            para: 0,
            ch: 0,
        };
        let f = Pos {
            bi: 4,
            para: 0,
            ch: 0,
        };
        let cmds = delete_selection_cmds(&b, a, f);
        apply_all(&mut b, &cmds);
        assert_eq!(b.len(), 3);
        assert_eq!(b[2].text, "ПРЕХОД:");
    }

    #[test]
    fn backspace_merges_at_block_start() {
        let mut b = sample();
        let cmds = backspace_cmds(
            &b,
            &[],
            Pos {
                bi: 2,
                para: 0,
                ch: 0,
            },
        )
        .0;
        assert_eq!(cmds, vec![EditCommand::MergeBlocks { bi: 2, fuse: true }]);
        apply_all(&mut b, &cmds);
        assert_eq!(b.len(), 4);
        assert_eq!(b[1].text, "Първи ред.\nВтори ред.ИВАН");
    }

    #[test]
    fn enter_uses_kit_element_continuation() {
        let mut b = sample();
        let cmds = {
            let mut cmds = Vec::new();
            let t = b[3].element_type;
            cmds.push(EditCommand::SplitBlock {
                at: Pos {
                    bi: 3,
                    para: 0,
                    ch: 0,
                },
                new_type: enter_next(t),
                fuse: false,
            });
            cmds
        };
        let expected = enter_next(ElementType::Dialogue);
        assert_eq!(expected, ElementType::Action, "KIT dialogue continuation");
        apply_all(&mut b, &cmds);
        assert_eq!(b[4].element_type, expected);
        assert_eq!(b[4].text, "Здравей!");
    }

    #[test]
    fn copy_fragments_round_trip_through_delete_and_insert() {
        let b = sample();
        let a = Pos {
            bi: 1,
            para: 1,
            ch: 0,
        };
        let f = Pos {
            bi: 2,
            para: 0,
            ch: 2,
        };
        let frags = fragments_between(&b, a, f);
        assert_eq!(frags.len(), 2);
        assert_eq!(frags[0], (ElementType::Action, "Втори ред.".into()));
        assert_eq!(frags[1], (ElementType::Character, "ИВ".into()));
        let joined: String = frags
            .iter()
            .map(|(_, s)| s.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(joined, text_between(&b, a, f));
    }

    #[test]
    fn hit_test_maps_clicks_to_cells() {
        let blocks = sample();
        let labels: Vec<String> = Vec::new();
        let doc = document_layout(&blocks, &RUSSIAN, &labels);
        let scale = PT_PER_MM;
        let row_h = ROW_MM * scale;
        let cell_w = MM_PER_CELL * scale;
        let page_pt = page_size_pt(1.0);
        let rects: Vec<egui::Rect> = vec![egui::Rect::from_min_size(egui::Pos2::ZERO, page_pt)];
        let line = body_lines(&doc)[0];
        let span = line.span.unwrap();
        let origin = line_origin_x(rects[0], line, scale, cell_w);
        let prect = rects[0];
        let y = prect.top() + PAD_T_MM * scale + line.row as f32 * row_h + row_h * 0.5;
        let at = hit_test(
            &doc,
            &rects,
            scale,
            row_h,
            cell_w,
            egui::pos2(origin + 3.4 * cell_w, y),
        )
        .expect("click hits the first line");
        assert_eq!(at.bi, span.bi);
        assert_eq!(at.para, span.para);
        assert_eq!(at.ch, span.start + 3);
    }

    #[test]
    fn tab_cycles_element_types_and_shift_tab_reverses() {
        let blocks = sample();
        let labels: Vec<String> = Vec::new();
        let doc = document_layout(&blocks, &RUSSIAN, &labels);
        let sel = Some(Selection::caret(Pos {
            bi: 1,
            para: 0,
            ch: 0,
        }));
        let next = key_action(
            &blocks,
            &doc,
            sel,
            egui::Key::Tab,
            egui::Modifiers::NONE,
            &[],
            &[],
        )
        .unwrap();
        assert_eq!(
            next.commands,
            vec![EditCommand::SetType {
                bi: 1,
                t: tab_next(ElementType::Action),
            }]
        );
        let prev = key_action(
            &blocks,
            &doc,
            sel,
            egui::Key::Tab,
            egui::Modifiers::SHIFT,
            &[],
            &[],
        )
        .unwrap();
        assert_eq!(
            prev.commands,
            vec![EditCommand::SetType {
                bi: 1,
                t: tab_prev(ElementType::Action),
            }]
        );
    }

    #[test]
    fn shift_arrow_extends_selection_from_anchor() {
        let blocks = sample();
        let labels: Vec<String> = Vec::new();
        let doc = document_layout(&blocks, &RUSSIAN, &labels);
        let sel = Some(Selection::caret(Pos {
            bi: 1,
            para: 0,
            ch: 2,
        }));
        let action = key_action(
            &blocks,
            &doc,
            sel,
            egui::Key::ArrowRight,
            egui::Modifiers::SHIFT,
            &[],
            &[],
        )
        .unwrap();
        let new_sel = action.selection.unwrap();
        assert_eq!(
            new_sel.anchor,
            Pos {
                bi: 1,
                para: 0,
                ch: 2
            }
        );
        assert_eq!(
            new_sel.focus,
            Pos {
                bi: 1,
                para: 0,
                ch: 3
            }
        );
    }

    #[test]
    fn command_arrows_move_to_line_and_document_edges() {
        let blocks = sample();
        let labels: Vec<String> = Vec::new();
        let doc = document_layout(&blocks, &RUSSIAN, &labels);
        let caret = |bi, ch| Some(Selection::caret(Pos { bi, para: 0, ch }));

        let end = key_action(
            &blocks,
            &doc,
            caret(1, 3),
            egui::Key::ArrowRight,
            egui::Modifiers::COMMAND,
            &[],
            &[],
        )
        .unwrap();
        assert_eq!(
            end.selection.unwrap().focus,
            Pos {
                bi: 1,
                para: 0,
                ch: 10
            },
            "⌘→ goes to the end of the visual line"
        );

        let start = key_action(
            &blocks,
            &doc,
            caret(1, 10),
            egui::Key::ArrowLeft,
            egui::Modifiers::COMMAND,
            &[],
            &[],
        )
        .unwrap();
        assert_eq!(
            start.selection.unwrap().focus,
            Pos {
                bi: 1,
                para: 0,
                ch: 0
            },
            "⌘← goes to the start of the visual line"
        );

        // Shift+Command extends the selection instead of moving the caret.
        let selected = key_action(
            &blocks,
            &doc,
            caret(1, 4),
            egui::Key::ArrowLeft,
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            &[],
            &[],
        )
        .unwrap();
        let selection = selected.selection.unwrap();
        assert_eq!(
            selection.anchor,
            Pos {
                bi: 1,
                para: 0,
                ch: 4
            }
        );
        assert_eq!(
            selection.focus,
            Pos {
                bi: 1,
                para: 0,
                ch: 0
            }
        );

        // Command+Up/Down jump to the visible document bounds.
        let top = key_action(
            &blocks,
            &doc,
            caret(3, 3),
            egui::Key::ArrowUp,
            egui::Modifiers::COMMAND,
            &[],
            &[],
        )
        .unwrap();
        assert_eq!(
            top.selection.unwrap().focus,
            Pos {
                bi: 0,
                para: 0,
                ch: 0
            }
        );
        let bottom = key_action(
            &blocks,
            &doc,
            caret(0, 0),
            egui::Key::ArrowDown,
            egui::Modifiers::COMMAND,
            &[],
            &[],
        )
        .unwrap();
        assert_eq!(
            bottom.selection.unwrap().focus,
            Pos {
                bi: 4,
                para: 0,
                ch: 7
            }
        );
    }

    #[test]
    fn option_arrows_move_and_select_words() {
        let blocks = sample();
        let labels: Vec<String> = Vec::new();
        let doc = document_layout(&blocks, &RUSSIAN, &labels);
        let caret = |ch| Some(Selection::caret(Pos { bi: 0, para: 0, ch }));

        // "INT. СТАЯ - ДЕН" → "INT" ends at 3, "СТАЯ" spans 5..9.
        let first = key_action(
            &blocks,
            &doc,
            caret(0),
            egui::Key::ArrowRight,
            egui::Modifiers::ALT,
            &[],
            &[],
        )
        .unwrap();
        assert_eq!(first.selection.unwrap().focus.ch, 3);

        let second = key_action(
            &blocks,
            &doc,
            caret(3),
            egui::Key::ArrowRight,
            egui::Modifiers::ALT,
            &[],
            &[],
        )
        .unwrap();
        assert_eq!(second.selection.unwrap().focus.ch, 9);

        let back = key_action(
            &blocks,
            &doc,
            caret(6),
            egui::Key::ArrowLeft,
            egui::Modifiers::ALT,
            &[],
            &[],
        )
        .unwrap();
        assert_eq!(back.selection.unwrap().focus.ch, 5);

        let selected = key_action(
            &blocks,
            &doc,
            caret(0),
            egui::Key::ArrowRight,
            egui::Modifiers::ALT | egui::Modifiers::SHIFT,
            &[],
            &[],
        )
        .unwrap();
        let selection = selected.selection.unwrap();
        assert_eq!(selection.anchor.ch, 0);
        assert_eq!(selection.focus.ch, 3);
    }

    #[test]
    fn smart_typography_applies_quote_dot_and_capital_rules() {
        let mut blocks = vec![
            ScriptBlock::new(ElementType::Action, String::new()),
            ScriptBlock::new(ElementType::Action, "Wait..".into()),
        ];
        let opts = EditorOptions {
            smart_quotes: true,
            replace_three_dots: true,
            correct_double_capitals: true,
            capitalize_first_word: true,
            ..EditorOptions::default()
        };
        let caret = |bi, ch| Some(Selection::caret(Pos { bi, para: 0, ch }));

        let mut ed = Editor::with_options(&blocks, caret(0, 0), opts);
        ed.insert("\"Hello\" it's");
        apply_all(&mut blocks, &ed.cmds);
        assert_eq!(blocks[0].text, "“Hello” it’s");

        // The third typed dot becomes one ellipsis character.
        let mut ed = Editor::with_options(&blocks, caret(1, 6), opts);
        ed.insert(".");
        apply_all(&mut blocks, &ed.cmds);
        assert_eq!(blocks[1].text, "Wait…");

        // Second capital lowercased once a lowercase letter follows it.
        let mut ed = Editor::with_options(&blocks, caret(0, 0), opts);
        ed.insert("DO");
        apply_all(&mut blocks, &ed.cmds);
        let mut ed = Editor::with_options(&blocks, caret(0, 2), opts);
        ed.insert("u");
        apply_all(&mut blocks, &ed.cmds);
        assert!(blocks[0].text.starts_with("Dou"));

        // First word of a paragraph and of a new sentence is capitalized.
        let mut cap = vec![ScriptBlock::new(ElementType::Action, String::new())];
        let mut ed = Editor::with_options(&cap, caret(0, 0), opts);
        ed.insert("hello");
        apply_all(&mut cap, &ed.cmds);
        assert_eq!(cap[0].text, "Hello");
        let mut ed = Editor::with_options(&cap, caret(0, 5), opts);
        ed.insert(".");
        apply_all(&mut cap, &ed.cmds);
        let mut ed = Editor::with_options(&cap, caret(0, 6), opts);
        ed.insert(" next");
        apply_all(&mut cap, &ed.cmds);
        assert_eq!(cap[0].text, "Hello. Next");
    }

    #[test]
    fn command_and_option_delete_remove_whole_units() {
        let blocks = sample();
        let labels: Vec<String> = Vec::new();
        let doc = document_layout(&blocks, &RUSSIAN, &labels);
        let caret = |ch| Some(Selection::caret(Pos { bi: 0, para: 0, ch }));
        let mut edited = blocks.clone();
        let action = key_action(
            &edited,
            &doc,
            caret(9),
            egui::Key::Backspace,
            egui::Modifiers::COMMAND,
            &[],
            &[],
        )
        .unwrap();
        apply_all(&mut edited, &action.commands);
        assert_eq!(edited[0].text, " - ДЕН");
        assert_eq!(action.selection.unwrap().focus.ch, 0);

        let mut edited = blocks.clone();
        let action = key_action(
            &edited,
            &doc,
            caret(0),
            egui::Key::Delete,
            egui::Modifiers::ALT,
            &[],
            &[],
        )
        .unwrap();
        apply_all(&mut edited, &action.commands);
        assert_eq!(edited[0].text, ". СТАЯ - ДЕН");
    }
}
