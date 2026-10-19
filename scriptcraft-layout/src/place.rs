//! Resolve paginated chunks into concrete, paintable grid rows.
//!
//! A [`PlacedLine`] is one row of the fixed line grid: a body line of script
//! text, a `(CONT'D)` cue, a `(MORE)` marker, or a scene number in the left
//! margin. Every body line knows the block, paragraph and char range it came
//! from ([`SourceSpan`]), which is what selection and hit-testing will map
//! through once the custom `ScriptView` widget lands.
//!
//! Everything here is in document units: `row` indexes the line grid
//! ([`ROW_MM`] per row) and horizontal positions are millimetres from the left
//! edge of the template's text column. Painters multiply by the current zoom;
//! zoom can therefore never change a single line break.

use crate::paginate;
use crate::wrap;
use scriptcraft_core::editing::{cue_name_for, TemplateMetrics};
use scriptcraft_core::model::{ElementType, ScriptBlock};

/// Millimetres per character cell (0.6 em at 12 pt).
pub const MM_PER_CELL: f32 = 2.54;
/// Width of the Russian template's text column (A4 210 − 37.5 − 25 mm).
pub const TEXT_W_MM: f32 = 147.5;
/// Height of one line-grid row: Courier Prime 12 pt × ROW_FACTOR 2300/2048.
pub const ROW_MM: f32 = 12.0 * (2300.0 / 2048.0) * (25.4 / 72.0);

/// Horizontal columns of a template, in millimetres from the text-column
/// left edge. Mirrors the editor's historical fractions of the column width
/// (`FRAC_CHARACTER_OFF` etc.) with the KIT template's millimetre values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TemplateGeometry {
    pub cue_mm: f32,
    pub paren_mm: f32,
    pub paren_w_mm: f32,
    pub dialog_mm: f32,
    pub dialog_w_mm: f32,
    pub lyrics_mm: f32,
    pub lyrics_w_mm: f32,
    /// Gap between the text-column edge and the right edge of a scene number.
    pub scene_num_gap_mm: f32,
}

/// The Russian template's columns (`russian.kitss`).
pub const RUSSIAN_GEOMETRY: TemplateGeometry = TemplateGeometry {
    cue_mm: 67.5,
    paren_mm: 55.0,
    paren_w_mm: 47.5,
    dialog_mm: 37.5, // (147.5 − 72.5) / 2 — centered dialogue
    dialog_w_mm: 72.5,
    lyrics_mm: 37.5,
    lyrics_w_mm: 110.0,
    scene_num_gap_mm: 4.0,
};

/// What kind of grid row a [`PlacedLine`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineRole {
    /// A line of script text (one display line of one paragraph).
    Body,
    /// `NAME (CONT'D)` above a continued dialogue/lyrics chunk.
    ContdCue,
    /// `(MORE)` below the last line of a split chunk.
    More,
    /// A scene number in the left margin, beside its heading.
    SceneNumber,
}

/// Char range of a body line within its source paragraph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceSpan {
    pub bi: usize,
    pub para: usize,
    /// Char offset (not bytes) into the paragraph.
    pub start: usize,
    pub end: usize,
}

/// One paintable grid row.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedLine {
    /// Grid row from the top of the text area (row height = [`ROW_MM`]).
    pub row: usize,
    /// Horizontal anchor: the left edge of the text, or the right edge when
    /// [`right_aligned`](Self::right_aligned).
    pub x_mm: f32,
    pub text: String,
    pub role: LineRole,
    pub bold: bool,
    pub italic: bool,
    pub right_aligned: bool,
    /// `None` for synthetic rows ((MORE), (CONT'D), scene numbers).
    pub span: Option<SourceSpan>,
}

/// One laid-out page: its rows in placement order, plus the KIT page number
/// (`None` on the first page — KIT numbers nothing on page one).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageLayout {
    pub lines: Vec<PlacedLine>,
    pub number: Option<u32>,
}

/// A whole document, pages in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocLayout {
    pub pages: Vec<PageLayout>,
}

/// Lay out a whole document: paginate, then resolve every chunk into placed
/// grid rows. `scene_labels` holds one label ("1.", "SC3."…) per scene
/// heading in document order; missing entries simply draw no number.
pub fn layout(
    blocks: &[ScriptBlock],
    m: &TemplateMetrics,
    geometry: &TemplateGeometry,
    scene_labels: &[String],
) -> DocLayout {
    let pages = paginate::paginate(blocks, m);
    let heading_ordinals = heading_ordinals(blocks);
    let mut out: Vec<PageLayout> = Vec::with_capacity(pages.len());
    for (pi, page) in pages.iter().enumerate() {
        out.push(layout_page(
            page,
            pi,
            blocks,
            m,
            geometry,
            scene_labels,
            &heading_ordinals,
        ));
    }
    DocLayout { pages: out }
}

/// Re-layout after an edit without redoing the whole document.
///
/// `old` is the previous layout; `first_affected` is the lowest block index
/// (old indexing) the edit touched. Pages before the resume point are reused
/// verbatim — pagination is sequential, so blocks below `first_affected`
/// cannot change them. New pages are then built one at a time and, when
/// `changed_upto` says the edit was an in-place text change (inclusive last
/// changed old index), the build stops as soon as a rebuilt page equals the
/// old layout's page at the same position: the paginator state and every
/// remaining block are provably unchanged from there on.
///
/// Returns `None` when the assumption cannot be verified (e.g. an empty old
/// layout); callers fall back to [`layout`].
pub fn relayout_from(
    blocks: &[ScriptBlock],
    m: &TemplateMetrics,
    geometry: &TemplateGeometry,
    scene_labels: &[String],
    old: &DocLayout,
    first_affected: usize,
    changed_upto: Option<usize>,
) -> Option<DocLayout> {
    if old.pages.is_empty() || blocks.is_empty() {
        return None;
    }
    // A block's placement also depends on the two blocks after it (orphan
    // guards peek ahead), so recompute from two blocks earlier as well.
    let first = first_affected.saturating_sub(2).min(blocks.len() - 1);
    // Resume page: the first old page holding a body line of a block at or
    // after the edit.
    let mut p0 = old.pages.iter().position(|p| {
        p.lines
            .iter()
            .any(|l| l.span.map(|s| s.bi >= first) == Some(true))
    })?;
    // If that page opens with a continuation, the block it continues began
    // on an earlier page — the resume point must be where that block starts.
    loop {
        let top_body = old.pages[p0]
            .lines
            .iter()
            .filter(|l| l.span.is_some())
            .min_by_key(|l| l.row)?;
        if top_body.span?.para == 0 {
            break;
        }
        if p0 == 0 {
            return None;
        }
        p0 -= 1;
    }
    let bi0 = old.pages[p0]
        .lines
        .iter()
        .filter_map(|l| l.span.map(|s| s.bi))
        .min()?;

    let heading_ordinals = heading_ordinals(blocks);
    let mut pager = paginate::Pager::new(&blocks[bi0..], m);
    let mut new_pages: Vec<PageLayout> = Vec::new();
    let mut i = 0usize;
    let converged_at = loop {
        pager.run_until(i + 2);
        if pager.is_done() {
            pager.trim_final_empty();
        }
        let closed = pager.pages().len() > i + 1 || (pager.is_done() && pager.pages().len() > i);
        if !closed {
            break None;
        }
        // The pager walked `blocks[bi0..]`; rebase its relative chunk indices
        // so `layout_page` can run against the full block list.
        let mut page = pager.pages()[i].clone();
        for c in &mut page.chunks {
            c.bi += bi0;
        }
        let placed = layout_page(
            &page,
            p0 + i,
            blocks,
            m,
            geometry,
            scene_labels,
            &heading_ordinals,
        );
        // Convergence is only provable when everything the remaining pages
        // depend on is untouched: an in-place edit whose last changed block
        // is fully consumed at this page's end. A changed Character block is
        // never safe: later continuation chunks resolve their "(CONT'D)"
        // cue by scanning back to it.
        let safe = match changed_upto {
            Some(last) => {
                let cue_ok = blocks
                    .get(last)
                    .is_none_or(|b| b.element_type != ElementType::Character);
                cue_ok
                    && page
                        .chunks
                        .last()
                        .is_none_or(|c| c.bi > last || (c.bi == last && !c.more))
            }
            None => false,
        };
        let stop_here = safe && old.pages.get(p0 + i).map(|o| o == &placed) == Some(true);
        new_pages.push(placed);
        if stop_here {
            break Some(p0 + i);
        }
        i += 1;
    };

    let mut pages: Vec<PageLayout> = old.pages[..p0].to_vec();
    match converged_at {
        Some(qi) => {
            // `new_pages[i]` duplicates `old.pages[qi]`; keep the old copy.
            let rebuilt = new_pages.len() - 1;
            pages.extend_from_slice(&new_pages[..rebuilt]);
            pages.extend_from_slice(&old.pages[qi..]);
        }
        None => pages.extend_from_slice(&new_pages),
    }
    Some(DocLayout { pages })
}

fn layout_page(
    page: &paginate::Page,
    pi: usize,
    blocks: &[ScriptBlock],
    m: &TemplateMetrics,
    geometry: &TemplateGeometry,
    scene_labels: &[String],
    heading_ordinals: &[Option<usize>],
) -> PageLayout {
    let mut lines: Vec<PlacedLine> = Vec::new();
    let mut row = 0usize;
    for chunk in &page.chunks {
        row += chunk.top;
        let Some(block) = blocks.get(chunk.bi) else {
            continue;
        };
        let t = block.element_type;
        if chunk.contd {
            if let Some(cue) = cue_name_for(blocks, chunk.bi) {
                lines.push(PlacedLine {
                    row,
                    x_mm: geometry.cue_mm,
                    text: format!("{cue} (CONT'D)"),
                    role: LineRole::ContdCue,
                    bold: true,
                    italic: false,
                    right_aligned: false,
                    span: None,
                });
            }
            // The row is reserved even when no cue can be resolved, exactly
            // like the paginator budgeted it.
            row += 1;
        }
        let first_row = row;
        let paras: Vec<&str> = block.text.split('\n').collect();
        let cpl = m.col_chars(t);
        let x_mm = indent_mm(t, geometry);
        let end = chunk.end.min(paras.len());
        for (para, text) in paras.iter().enumerate().take(end).skip(chunk.start) {
            for wl in wrap::wrap_paragraph(text, cpl) {
                lines.push(PlacedLine {
                    row,
                    x_mm,
                    text: wl.text,
                    role: LineRole::Body,
                    bold: is_bold(t),
                    italic: t == ElementType::Lyrics,
                    right_aligned: false,
                    span: Some(SourceSpan {
                        bi: chunk.bi,
                        para,
                        start: wl.start,
                        end: wl.end,
                    }),
                });
                row += 1;
            }
        }
        if chunk.more {
            lines.push(PlacedLine {
                row,
                x_mm: TEXT_W_MM,
                text: "(MORE)".to_string(),
                role: LineRole::More,
                bold: false,
                italic: false,
                right_aligned: true,
                span: None,
            });
            row += 1;
        }
        if t == ElementType::SceneHeading {
            if let Some(Some(ordinal)) = heading_ordinals.get(chunk.bi).copied() {
                if let Some(label) = scene_labels.get(ordinal) {
                    lines.push(PlacedLine {
                        row: first_row,
                        x_mm: -geometry.scene_num_gap_mm,
                        text: label.clone(),
                        role: LineRole::SceneNumber,
                        bold: true,
                        italic: false,
                        right_aligned: true,
                        span: None,
                    });
                }
            }
        }
    }
    PageLayout {
        lines,
        number: if pi > 0 { Some(pi as u32 + 1) } else { None },
    }
}

/// Scene ordinal (0-based) of every scene-heading block; `None` elsewhere.
fn heading_ordinals(blocks: &[ScriptBlock]) -> Vec<Option<usize>> {
    let mut ordinal = 0usize;
    blocks
        .iter()
        .map(|b| {
            if b.element_type == ElementType::SceneHeading {
                let here = ordinal;
                ordinal += 1;
                Some(here)
            } else {
                None
            }
        })
        .collect()
}

fn indent_mm(t: ElementType, g: &TemplateGeometry) -> f32 {
    match t {
        ElementType::Character | ElementType::NonPrintable => g.cue_mm,
        ElementType::Parenthetical => g.paren_mm,
        ElementType::Dialogue => g.dialog_mm,
        ElementType::Lyrics => g.lyrics_mm,
        _ => 0.0,
    }
}

fn is_bold(t: ElementType) -> bool {
    matches!(
        t,
        ElementType::SceneHeading | ElementType::Character | ElementType::Transition
    )
}
