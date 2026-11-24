use crate::dual;
use crate::paginate::{page_labels, paginate, paginate_with_breaks};
#[cfg(test)]
use crate::place::FINAL_DRAFT_A4_GEOMETRY;
use crate::place::{page_spec, ROW_MM};
use crate::wrap::wrap_lines;
use printpdf::{
    Color, Greyscale, Line, LinePoint, Mm, Op, ParsedFont, PdfDocument, PdfFontHandle, PdfPage,
    PdfSaveOptions, Point, Pt, Rect as PdfRect, Rgb, TextItem, TextMatrix,
};
#[cfg(test)]
use scriptcraft_core::editing::FINAL_DRAFT_A4;
use scriptcraft_core::editing::{cue_name_for, PageSize};
use scriptcraft_core::kitsp::Metadata;
use scriptcraft_core::model::{ElementType, Project};
use scriptcraft_core::page_lock::PageLock;
use scriptcraft_core::revision::{RevisionColor, RevisionSet};
use scriptcraft_core::summary::RevisionSummary;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PdfError {
    #[error("font could not be parsed")]
    Font,
    #[error("no printable content")]
    Empty,
    #[error("no pages in the current revision")]
    NoRevisionPages,
}

/// A4 page geometry (mm) for KIT's default template,
/// `final_draft_a4.kitss`: margins 38/24/17/24, 155 mm text column (61 chars
/// at 10 cpi Courier).
const PAGE_W: f32 = 210.0;
const PAGE_H: f32 = 297.0;
#[cfg(test)]
const MARGIN_L: f32 = 38.0;
const MARGIN_T: f32 = 24.0;
const FONT_PT: f32 = 12.0;
/// 0.6em advance at 12pt = 7.2pt = 2.54mm per column.
const CHAR_MM: f32 = 2.54;
/// Template line pitch, shared with the editor's row grid.
const PITCH_MM: f32 = ROW_MM;
/// Millimetres per PDF point (TextMatrix offsets are in points).
const MM_TO_PT: f32 = 72.0 / 25.4;
/// Courier Prime 12pt metrics (hhea 1600/-700 per 2048 em).
const ASCENT_MM: f32 = 1600.0 / 2048.0 * 12.0 / 72.0 * 25.4;
const DESCENT_MM: f32 = 700.0 / 2048.0 * 12.0 / 72.0 * 25.4;
/// Baseline offset below the slot top under the top margin.
const BASELINE_OFF: f32 = ASCENT_MM;
#[cfg(test)]
const X_CUE: f32 = MARGIN_L + FINAL_DRAFT_A4_GEOMETRY.cue_mm;
#[cfg(test)]
const X_PAREN: f32 = MARGIN_L + FINAL_DRAFT_A4_GEOMETRY.paren_mm;
#[cfg(test)]
const X_DLG: f32 = MARGIN_L + FINAL_DRAFT_A4_GEOMETRY.dialog_mm;
#[cfg(test)]
const SCENE_NUM_GAP: f32 = FINAL_DRAFT_A4_GEOMETRY.scene_num_gap_mm;
/// Footer baseline, inside the bottom margin.
const FOOTER_BASELINE_MM: f32 = 12.0;
/// Revision margin bars shared with the editor (mm from the page edge).
pub const REVISION_BAR_X_MM: f32 = 10.0;
pub const REVISION_BAR_W_MM: f32 = 3.0;
/// Title-page nudges measured from KIT's export (mm below the slot row).
const TITLE_DY: f32 = 1.4;
const TITLE_VALUE_DY: f32 = 1.5;
const TITLE_CONTACT_DY: f32 = 1.8;
/// Bottom-anchored title-page rows (mm above the page edge). KIT's slot grid
/// puts the draft date at a negative height on A4, which clipped it; these
/// keep the date and the REVISED block on the page in both the preview and
/// the exported PDF.
const TITLE_YEAR_Y_MM: f32 = 22.0;
const TITLE_REVISED_Y_MM: f32 = 40.0;
const TITLE_REVISED_STEP_MM: f32 = 5.0;

/// Geometry the title-page editor preview needs to paint the same page as
/// the PDF exporter. Sharing it keeps the preview from drifting.
#[derive(Clone, Copy, Debug)]
pub struct TitlePageMetrics {
    pub page_w_mm: f32,
    pub page_h_mm: f32,
    pub margin_top_mm: f32,
    pub baseline_off_mm: f32,
    pub pitch_mm: f32,
    pub char_mm: f32,
    pub font_pt: f32,
}

pub const TITLE_PAGE_METRICS: TitlePageMetrics = TitlePageMetrics {
    page_w_mm: PAGE_W,
    page_h_mm: PAGE_H,
    margin_top_mm: MARGIN_T,
    baseline_off_mm: BASELINE_OFF,
    pitch_mm: PITCH_MM,
    char_mm: CHAR_MM,
    font_pt: FONT_PT,
};

/// Baseline of title-page line `index` measured from the page bottom (mm).
///
/// Lines with an explicit `y_from_bottom_mm` anchor (the draft date and the
/// REVISED block) ignore the slot grid: the grid's slot 57 sits at a negative
/// height on A4, which is why the draft date used to fall off the page.
pub fn title_page_line_y_mm(index: usize, line: &PrintLine) -> f32 {
    if let Some(y) = line.y_from_bottom_mm {
        return y;
    }
    PAGE_H - MARGIN_T - BASELINE_OFF - index as f32 * PITCH_MM - line.dy_mm
}

/// Toggles mirrored by KIT's Export Script dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportOptions {
    pub page_size: PageSize,
    pub title_page: bool,
    pub page_numbers: bool,
    pub scene_numbers: bool,
    pub speaking_parts_count: bool,
    pub revision_marks: bool,
    pub revision_pages_only: bool,
    pub watermark: Option<String>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            page_size: PageSize::A4,
            title_page: true,
            page_numbers: true,
            scene_numbers: true,
            speaking_parts_count: false,
            revision_marks: false,
            revision_pages_only: false,
            watermark: None,
        }
    }
}

/// Text that sits in a page margin instead of the body flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarginBand {
    Header,
    Footer,
}

/// One uniformly-styled segment inside a [`PrintLine`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledRun {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

/// One printable text line on a page.
#[derive(Debug, Clone, PartialEq)]
pub struct PrintLine {
    pub x_mm: f32,
    pub text: String,
    pub bold: bool,
    pub right_aligned: bool,
    /// Page-number lines live in the top margin and consume no body space.
    pub is_page_number: bool,
    pub underline: bool,
    /// Extra downward offset (mm); used for the measured title-page nudges.
    pub dy_mm: f32,
    /// Draw on the previous line's row instead of advancing (scene numbers).
    pub share_row: bool,
    /// Revision colour for the margin bar, when this line belongs to a
    /// changed block.
    pub revision: Option<RevisionColor>,
    /// Header/footer text: drawn in the page margin, never advances the body.
    pub margin: Option<MarginBand>,
    /// Title-page lines that sit at a fixed height above the page bottom
    /// instead of a grid slot. The title-page grid runs out of page before
    /// KIT's draft-date slot, so the date and the REVISED block are anchored
    /// to the bottom edge explicitly (see [`title_page_line_y_mm`]).
    pub y_from_bottom_mm: Option<f32>,
    /// Optional inline-styled runs covering `text` (populated only when the
    /// source block carries inline bold / italic / underline spans).
    pub runs: Vec<StyledRun>,
}

impl PrintLine {
    fn text_line(x_mm: f32, text: String, bold: bool) -> Self {
        Self {
            x_mm,
            text,
            bold,
            right_aligned: false,
            is_page_number: false,
            underline: false,
            dy_mm: 0.0,
            share_row: false,
            revision: None,
            margin: None,
            y_from_bottom_mm: None,
            runs: Vec::new(),
        }
    }
}

/// Build inline [`StyledRun`]s for one wrapped display line of `block`
/// given the paragraph's char offset in `block.text` and the display line's
/// `cols` mapping (source offset per display char).
fn styled_runs_for_line(
    block: &scriptcraft_core::model::ScriptBlock,
    para_offset: usize,
    display_text: &str,
    cols: &[usize],
    base_bold: bool,
    base_italic: bool,
) -> Vec<StyledRun> {
    if block.styles.is_empty() || display_text.is_empty() {
        return Vec::new();
    }
    let mut runs: Vec<StyledRun> = Vec::new();
    let mut cur_text = String::new();
    let mut cur_flags: Option<(bool, bool, bool)> = None;
    for (ci, ch) in display_text.chars().enumerate() {
        let src_in_para = cols.get(ci).copied().unwrap_or(ci);
        let (sb, si, su) = block.styles_at(para_offset + src_in_para);
        let flags = (base_bold || sb, base_italic || si, su);
        if cur_flags == Some(flags) {
            cur_text.push(ch);
        } else {
            if let Some((b, i, u)) = cur_flags {
                runs.push(StyledRun {
                    text: std::mem::take(&mut cur_text),
                    bold: b,
                    italic: i,
                    underline: u,
                });
            }
            cur_text.push(ch);
            cur_flags = Some(flags);
        }
    }
    if let Some((b, i, u)) = cur_flags {
        runs.push(StyledRun {
            text: cur_text,
            bold: b,
            italic: i,
            underline: u,
        });
    }
    // Only keep `runs` when at least one char differs from unstyled plain text.
    if runs
        .iter()
        .any(|r| r.bold != base_bold || r.italic != base_italic || r.underline)
    {
        runs
    } else {
        Vec::new()
    }
}

/// Char offset of paragraph `para` inside `text` (splitting on `'\n'`).
fn para_char_offset(text: &str, para: usize) -> usize {
    let mut off = 0usize;
    for (i, p) in text.split('\n').enumerate() {
        if i == para {
            return off;
        }
        off += p.chars().count() + 1;
    }
    off
}

/// The number shown ahead of scene `i`: locked label when numbering is
fn scene_label(locked: &Option<Vec<String>>, prefix: &str, start: u32, i: usize) -> String {
    if let Some(l) = locked.as_ref().and_then(|v| v.get(i)) {
        return format!("{l}.");
    }
    format!("{prefix}{}.", start + i as u32)
}

fn locked_scene_labels(meta: &Metadata) -> Option<Vec<String>> {
    let raw = meta.locked_scene_numbers.trim();
    let raw = raw.strip_prefix('*').unwrap_or(raw);
    let v: Vec<String> = raw
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

/// Number of spoken lines per character (one per dialogue block whose most
/// recent cue is that character).
fn speech_counts(blocks: &[scriptcraft_core::model::ScriptBlock]) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut last_cue: Option<String> = None;
    for b in blocks {
        match b.element_type {
            ElementType::Character => last_cue = Some(b.text.trim().to_string()),
            ElementType::Dialogue => {
                if let Some(c) = &last_cue {
                    *counts.entry(c.clone()).or_default() += 1;
                }
            }
            ElementType::SceneHeading => last_cue = None,
            _ => {}
        }
    }
    counts
}

/// `dd.mm.yyyy` (or the date half of a KIT timestamp) for headers and the
/// title page. Keeps imported/foreign timestamps readable instead of
/// rejecting them.
fn revision_date(set: &RevisionSet) -> String {
    let raw = set.started.trim();
    let date = raw.split_whitespace().next().unwrap_or(raw);
    date.to_string()
}

fn expand_margin_text(text: &str, latest: Option<&RevisionSet>, page: &str) -> String {
    let revision = latest
        .map(|s| s.color.label().to_uppercase())
        .unwrap_or_default();
    let date = latest.map(revision_date).unwrap_or_default();
    text.replace("{revision}", &revision)
        .replace("{date}", &date)
        .replace("{page}", page)
}

fn fit_margin_text(text: String, text_w_mm: f32, reserved: usize) -> Option<String> {
    let max = ((text_w_mm / CHAR_MM) as usize).saturating_sub(reserved);
    let text = text.trim().to_string();
    if text.is_empty() {
        return None;
    }
    if text.chars().count() <= max {
        return Some(text);
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    Some(out)
}

fn margin_line(margin_l_mm: f32, text: String, band: MarginBand) -> PrintLine {
    PrintLine {
        margin: Some(band),
        ..PrintLine::text_line(margin_l_mm, text, false)
    }
}

/// User header with `{revision}`, `{date}`, `{page}` tokens. When it is empty
/// and revision colouring is on, a production-style colour/date header is
/// supplied automatically; page numbers stay right-aligned in the same band.
fn page_header(
    meta: &Metadata,
    latest: Option<&RevisionSet>,
    show_revision: bool,
    page: &str,
    text_w_mm: f32,
) -> Option<String> {
    let raw = meta.header.trim();
    let text = if raw.is_empty() {
        if !show_revision {
            return None;
        }
        let set = latest?;
        let name = set.name.trim();
        if name.is_empty() {
            format!(
                "{} REVISION — {}",
                set.color.label().to_uppercase(),
                revision_date(set)
            )
        } else {
            format!("{} — {}", name, revision_date(set))
        }
    } else {
        expand_margin_text(raw, latest, page)
    };
    fit_margin_text(text, text_w_mm, 8)
}

fn page_footer(
    meta: &Metadata,
    latest: Option<&RevisionSet>,
    page: &str,
    text_w_mm: f32,
) -> Option<String> {
    let raw = meta.footer.trim();
    if raw.is_empty() {
        return None;
    }
    fit_margin_text(expand_margin_text(raw, latest, page), text_w_mm, 0)
}

/// Typeset a project into positioned lines per page (pure function of the
/// document — unit-testable without fonts). Non-printable blocks are
/// excluded, matching KIT's print behavior. Geometry follows the editor's
/// Russian-locale KIT template so screen and PDF paginate identically.
pub fn layout_pages(p: &Project, meta: &Metadata, opts: &ExportOptions) -> Vec<Vec<PrintLine>> {
    layout_pages_with_marks_and_latest(p, meta, opts, &[], None)
}

pub fn layout_pages_with_marks(
    p: &Project,
    meta: &Metadata,
    opts: &ExportOptions,
    marks: &[Option<RevisionColor>],
) -> Vec<Vec<PrintLine>> {
    layout_pages_with_marks_and_latest(p, meta, opts, marks, None)
}

pub fn layout_pages_with_marks_and_latest(
    p: &Project,
    meta: &Metadata,
    opts: &ExportOptions,
    marks: &[Option<RevisionColor>],
    latest: Option<&RevisionSet>,
) -> Vec<Vec<PrintLine>> {
    // Omitted scenes collapse to heading + OMITTED; everything downstream
    // (pagination, scene numbering, revision marks) sees the page view.
    let view = p.has_omitted_scenes().then(|| p.document_view());
    let source_blocks = view.as_ref().map(|v| &v.blocks).unwrap_or(&p.blocks);
    let blocks: Vec<usize> = (0..source_blocks.len())
        .filter(|&i| source_blocks[i].element_type.is_printable())
        .collect();
    let renum: Vec<scriptcraft_core::model::ScriptBlock> =
        blocks.iter().map(|&i| source_blocks[i].clone()).collect();
    let show_marks = opts.revision_marks || opts.revision_pages_only;
    let revisions: Vec<Option<RevisionColor>> = if show_marks {
        blocks
            .iter()
            .map(|&i| {
                let original = view
                    .as_ref()
                    .and_then(|view| view.revision_sources.get(i).copied().flatten())
                    .or_else(|| view.is_none().then_some(i));
                original.and_then(|bi| marks.get(bi).copied().flatten())
            })
            .collect()
    } else {
        vec![None; blocks.len()]
    };
    let page_lock = PageLock::from_json(&meta.locked_pages)
        .ok()
        .filter(|l| !l.is_empty());
    let page_anchors = match (&page_lock, &view) {
        (Some(lock), Some(view)) => {
            // Omission-aware anchors already carry vanished body-page labels
            // forward; map them from the full view onto the printable view.
            let view_to_print: Vec<Option<usize>> = {
                let mut map = vec![None; view.blocks.len()];
                for (pi, &vi) in blocks.iter().enumerate() {
                    map[vi] = Some(pi);
                }
                map
            };
            lock.resolve_view(&p.blocks, view)
                .into_iter()
                .filter_map(|anchor| {
                    let pi = view_to_print.get(anchor.bi).copied().flatten()?;
                    Some(scriptcraft_core::page_lock::ResolvedAnchor {
                        bi: pi,
                        para: anchor.para,
                        page: anchor.page,
                    })
                })
                .collect()
        }
        (Some(lock), None) => lock.resolve(&renum),
        (None, _) => Vec::new(),
    };
    let spec = page_spec(opts.page_size);
    let margin_l = spec.margin_l_mm;
    let text_right = spec.text_right_mm();
    let x_cue = margin_l + spec.geometry.cue_mm;
    let x_paren = margin_l + spec.geometry.paren_mm;
    let x_dlg = margin_l + spec.geometry.dialog_mm;
    let scene_num_gap = spec.geometry.scene_num_gap_mm;
    let breaks: Vec<(usize, usize)> = page_anchors.iter().map(|a| (a.bi, a.para)).collect();
    let pages = if page_lock.is_some() {
        paginate_with_breaks(&renum, spec.metrics, &breaks)
    } else {
        paginate(&renum, spec.metrics)
    };
    let page_labels = if page_lock.is_some() {
        page_labels(&pages, &page_anchors)
    } else {
        (1..=pages.len()).map(|n| n.to_string()).collect()
    };
    let locked = locked_scene_labels(meta);
    let prefix = meta.scene_number_prefix.trim().to_string();
    let start: u32 = meta.scene_number_start.trim().parse().unwrap_or(1);
    let counts = if opts.speaking_parts_count {
        speech_counts(&renum)
    } else {
        HashMap::new()
    };
    let mut scene_i = 0usize;
    let mut out = Vec::new();
    for (pi, page) in pages.iter().enumerate() {
        let mut lines: Vec<PrintLine> = Vec::new();
        let label = page_labels.get(pi).map(String::as_str).unwrap_or("1");
        // KIT numbers script pages starting from the second one ("2."…),
        // since the title page and first script page stay unnumbered.
        if opts.page_numbers && pi + 1 >= 2 {
            let pg = format!("{label}.");
            lines.push(PrintLine {
                text: pg,
                is_page_number: true,
                bold: true,
                right_aligned: true,
                ..PrintLine::text_line(margin_l, String::new(), false)
            });
        }
        let show_revision = opts.revision_marks || opts.revision_pages_only;
        if let Some(header) =
            page_header(meta, latest, show_revision, label, spec.geometry.text_w_mm)
        {
            lines.push(margin_line(margin_l, header, MarginBand::Header));
        }
        if let Some(footer) = page_footer(meta, latest, label, spec.geometry.text_w_mm) {
            lines.push(margin_line(margin_l, footer, MarginBand::Footer));
        }
        for chunk in &page.chunks {
            for _ in 0..chunk.top {
                lines.push(PrintLine::text_line(margin_l, String::new(), false));
            }
            if let Some(dual_chunk) = &chunk.dual {
                let rows = dual::build_rows(&renum, dual_chunk.pair, spec.metrics);
                if dual_chunk.contd_left || dual_chunk.contd_right {
                    for (right, cue_bi, contd) in [
                        (false, dual_chunk.pair.left, dual_chunk.contd_left),
                        (true, dual_chunk.pair.right, dual_chunk.contd_right),
                    ] {
                        if !contd {
                            continue;
                        }
                        if let Some(cue) = cue_name_for(&renum, cue_bi + 1) {
                            let text = format!("{cue} (CONT'D)");
                            let mut line = PrintLine::text_line(
                                margin_l + dual::cue_x_mm(&text, right),
                                text,
                                false,
                            );
                            line.revision = revisions[cue_bi];
                            lines.push(line);
                        }
                    }
                }
                let start = dual_chunk.start_row.min(rows.rows.len());
                let end = dual_chunk.end_row.min(rows.rows.len());
                for row in &rows.rows[start..end] {
                    for cell in [&row.left, &row.right].into_iter().flatten() {
                        let mut line = PrintLine::text_line(
                            margin_l + cell.x_mm,
                            cell.text.clone(),
                            cell.bold,
                        );
                        line.revision = cell.span.map(|span| revisions[span.bi]).unwrap_or(None);
                        if let Some(span) = cell.span {
                            let src = &renum[span.bi];
                            if !src.styles.is_empty() {
                                let off = para_char_offset(&src.text, span.para);
                                line.runs = styled_runs_for_line(
                                    src,
                                    off,
                                    &cell.text,
                                    &cell.cols,
                                    cell.bold,
                                    cell.italic,
                                );
                            }
                        }
                        lines.push(line);
                    }
                }
                let mut more = |right: bool, cue_bi: usize| {
                    let text = "(MORE)".to_string();
                    let x = margin_l + dual::more_x_mm(right) - 6.0 * CHAR_MM;
                    let mut line = PrintLine::text_line(x, text, false);
                    line.revision = revisions[cue_bi];
                    lines.push(line);
                };
                if dual_chunk.more_left {
                    more(false, dual_chunk.pair.left);
                }
                if dual_chunk.more_right {
                    more(true, dual_chunk.pair.right);
                }
                continue;
            }
            let b = &renum[chunk.bi];
            let revision = revisions[chunk.bi];
            // `final_draft_a4.kitss` sets bold="false" on every block;
            // transitions are the only block aligned to the right edge.
            let (x, bold, right_aligned) = match b.element_type {
                ElementType::SceneHeading => (margin_l, false, false),
                ElementType::SceneCharacters => (margin_l, false, false),
                ElementType::Action | ElementType::Shot => (margin_l, false, false),
                ElementType::Character => (x_cue, false, false),
                ElementType::Parenthetical => (x_paren, false, false),
                ElementType::Dialogue => (x_dlg, false, false),
                ElementType::Lyrics => (x_dlg, false, false),
                ElementType::Transition => (text_right, false, true),
                ElementType::NonPrintable => (x_cue, false, false),
            };
            if chunk.contd {
                if let Some(cue) = cue_name_for(&renum, chunk.bi) {
                    let mut line = PrintLine::text_line(x_cue, format!("{cue} (CONT'D)"), false);
                    line.revision = revision;
                    lines.push(line);
                }
            }
            // Scene number, right-aligned in the left margin so that a
            // one-column gap remains before the heading text.
            let mut number_pushed = false;
            if b.element_type == ElementType::SceneHeading && opts.scene_numbers {
                let label = scene_label(&locked, &prefix, start, scene_i);
                let w = label.chars().count() as f32 * CHAR_MM;
                lines.push(PrintLine::text_line(
                    margin_l - scene_num_gap - w,
                    label,
                    false,
                ));
                number_pushed = true;
            }
            if b.element_type == ElementType::SceneHeading {
                scene_i += 1;
            }
            if b.styles.is_empty() {
                let mut text = b
                    .text
                    .split('\n')
                    .skip(chunk.start)
                    .take(chunk.end - chunk.start)
                    .collect::<Vec<_>>()
                    .join("\n");
                if b.element_type == ElementType::Character && !counts.is_empty() {
                    if let Some(n) = counts.get(b.text.trim()) {
                        text = format!("{} ({n})", text.trim_end());
                    }
                }
                // Same wrap budget as the paginator: identical line breaks.
                let wrapped = wrap_lines(&text, spec.metrics.col_chars(b.element_type));
                for (li, line) in wrapped.into_iter().enumerate() {
                    let mut l = PrintLine::text_line(x, line, bold);
                    l.right_aligned = right_aligned;
                    l.share_row = li == 0 && number_pushed;
                    l.revision = revision;
                    lines.push(l);
                }
            } else {
                let col_chars = spec.metrics.col_chars(b.element_type);
                let mut li = 0usize;
                let mut para_off = para_char_offset(&b.text, chunk.start);
                for (pi_rel, para_text) in b
                    .text
                    .split('\n')
                    .skip(chunk.start)
                    .take(chunk.end - chunk.start)
                    .enumerate()
                {
                    let is_last_para = chunk.start + pi_rel + 1 == chunk.end;
                    let owned;
                    let slice = if is_last_para
                        && b.element_type == ElementType::Character
                        && !counts.is_empty()
                    {
                        if let Some(n) = counts.get(b.text.trim()) {
                            owned = format!("{} ({n})", para_text.trim_end());
                            owned.as_str()
                        } else {
                            para_text
                        }
                    } else {
                        para_text
                    };
                    for wl in crate::wrap::wrap_paragraph(slice, col_chars) {
                        let runs =
                            styled_runs_for_line(b, para_off, &wl.text, &wl.cols, bold, false);
                        let mut l = PrintLine::text_line(x, wl.text, bold);
                        l.right_aligned = right_aligned;
                        l.share_row = li == 0 && number_pushed;
                        l.revision = revision;
                        l.runs = runs;
                        lines.push(l);
                        li += 1;
                    }
                    para_off += para_text.chars().count() + 1;
                }
            }
            if chunk.more {
                let mut line = PrintLine {
                    right_aligned: true,
                    ..PrintLine::text_line(0.0, "(MORE)".to_string(), false)
                };
                line.revision = revision;
                lines.push(line);
            }
        }
        out.push(lines);
    }
    out
}

/// Greedy wrap at the title page's 58-character column width. A single
/// oversized word is split across lines so it cannot run off the page.
fn wrap_title_line(text: &str, max: usize) -> Vec<String> {
    if max == 0 {
        return vec![text.to_string()];
    }
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let wc = word.chars().count();
        if wc > max {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            let chars: Vec<char> = word.chars().collect();
            let chunks: Vec<String> = chars
                .chunks(max)
                .map(|c| c.iter().collect::<String>())
                .collect();
            for chunk in &chunks[..chunks.len().saturating_sub(1)] {
                out.push(chunk.clone());
            }
            if let Some(last) = chunks.last() {
                cur = last.clone();
            }
            continue;
        }
        let extra = if cur.is_empty() { wc } else { wc + 1 };
        if cur.chars().count() + extra > max {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

/// Lay out the title page exactly like KIT's exporter: uppercase underlined
/// title on slot 16, genre / author / director / additional info from slot
/// 20 (every other slot, empties skipped), contacts on slot 39, year on
/// slot 57 (measured nudges below used for the last two). Long titles and
/// contact lines wrap at the 147.5 mm text column instead of running off the
/// page; short fields keep KIT's exact slots.
pub fn layout_title_page(meta: &Metadata) -> Vec<PrintLine> {
    layout_title_page_with_latest(meta, None)
}

pub fn layout_title_page_with_latest(
    meta: &Metadata,
    latest: Option<&RevisionSet>,
) -> Vec<PrintLine> {
    layout_title_page_for(meta, latest, PageSize::A4)
}

pub fn layout_title_page_for(
    meta: &Metadata,
    latest: Option<&RevisionSet>,
    page_size: PageSize,
) -> Vec<PrintLine> {
    let spec = page_spec(page_size);
    let page_w = spec.width_mm;
    let margin_l = spec.margin_l_mm;
    let centered = |text: String, bold: bool, underline: bool, dy_mm: f32| -> PrintLine {
        let w = text.chars().count() as f32 * CHAR_MM;
        PrintLine {
            x_mm: (page_w - w) / 2.0,
            text,
            bold,
            right_aligned: false,
            is_page_number: false,
            underline,
            dy_mm,
            share_row: false,
            revision: None,
            margin: None,
            y_from_bottom_mm: None,
            runs: Vec::new(),
        }
    };
    let blank = || PrintLine::text_line(margin_l, String::new(), false);
    let max_chars = (spec.geometry.text_w_mm / CHAR_MM) as usize;
    let mut lines: Vec<PrintLine> = Vec::new();
    while lines.len() < 16 {
        lines.push(blank());
    }
    let title_lines = wrap_title_line(&meta.title.to_uppercase(), max_chars);
    for part in &title_lines {
        lines.push(centered(part.clone(), true, true, TITLE_DY));
    }
    let mut slot = (16 + title_lines.len()).max(20);
    for value in [
        &meta.genre,
        &meta.author,
        &meta.director,
        &meta.additional_info,
    ] {
        let v = value.trim();
        if v.is_empty() {
            continue;
        }
        let parts = wrap_title_line(v, max_chars);
        slot = slot.max(lines.len());
        while lines.len() < slot {
            lines.push(blank());
        }
        for part in &parts {
            lines.push(centered(part.clone(), false, false, TITLE_VALUE_DY));
        }
        slot += parts.len() + 1;
    }
    while lines.len() < 39 {
        lines.push(blank());
    }
    for raw in meta.contacts.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        for part in wrap_title_line(line, max_chars) {
            lines.push(PrintLine {
                text: part,
                dy_mm: TITLE_CONTACT_DY,
                ..blank()
            });
        }
    }
    let year_slot = lines.len().max(57);
    while lines.len() < year_slot {
        lines.push(blank());
    }
    // The draft date keeps KIT's slot index (so the line list shape is
    // unchanged) but is painted at a fixed height above the page bottom: slot
    // 57 on the 4.754 mm pitch is already below the paper edge.
    let mut year = centered(meta.year.trim().to_string(), false, false, 0.0);
    year.y_from_bottom_mm = Some(TITLE_YEAR_Y_MM);
    lines.push(year);
    if let Some(set) = latest {
        let revised_slot = lines.len().max(60);
        while lines.len() < revised_slot {
            lines.push(blank());
        }
        let mut heading = centered("REVISED".to_string(), true, false, 0.0);
        heading.y_from_bottom_mm = Some(TITLE_REVISED_Y_MM);
        lines.push(heading);
        let name = set.name.trim();
        if !name.is_empty() {
            let mut line = centered(name.to_string(), false, false, 0.0);
            line.y_from_bottom_mm = Some(TITLE_REVISED_Y_MM - TITLE_REVISED_STEP_MM);
            lines.push(line);
        }
        let date = revision_date(set);
        if !date.is_empty() {
            let mut line = centered(date, false, false, 0.0);
            line.y_from_bottom_mm = Some(TITLE_REVISED_Y_MM - 2.0 * TITLE_REVISED_STEP_MM);
            lines.push(line);
        }
    }
    lines
}

/// Fonts embedded in an exported PDF. Courier Prime carries the screenplay
/// metrics; IBM Plex Mono covers what Courier Prime lacks (Cyrillic, Greek) at
/// the same 0.6 em advance, so the monospace grid — and with it the abstract
/// pagination — stays identical. Text is split into runs and each grabby
/// character is rendered with the first font that has it.
pub struct ExportFonts<'a> {
    pub regular: &'a [u8],
    pub bold: &'a [u8],
    pub italic: Option<&'a [u8]>,
    pub fallback_regular: &'a [u8],
    pub fallback_bold: &'a [u8],
    pub fallback_italic: Option<&'a [u8]>,
}

struct Face {
    id: printpdf::FontId,
    font: ParsedFont,
}

/// Split `text` into runs that must be drawn with either the primary or the
/// fallback face. A character missing from both stays in a primary run (it
/// renders as .notdef, same as before there was a fallback).
fn split_runs<'t>(
    text: &'t str,
    primary: &ParsedFont,
    fallback: &ParsedFont,
) -> Vec<(bool, &'t str)> {
    let mut runs: Vec<(bool, &'t str)> = Vec::new();
    let mut start = 0usize;
    let mut mode = false;
    for (i, c) in text.char_indices() {
        let fb = primary.lookup_glyph_index(c as u32).is_none()
            && fallback.lookup_glyph_index(c as u32).is_some();
        if i == 0 {
            mode = fb;
        } else if fb != mode {
            runs.push((mode, &text[start..i]));
            start = i;
            mode = fb;
        }
    }
    if start < text.len() {
        runs.push((mode, &text[start..]));
    }
    runs
}

/// Render laid-out pages to a PDF file (fonts embedded). The first
/// page is the KIT title page (unless disabled); script pages follow with
/// KIT numbering (first script page unnumbered, the rest bold "N." in the
/// top-right margin).
pub fn to_pdf_bytes(
    p: &Project,
    meta: &Metadata,
    opts: &ExportOptions,
    fonts: &ExportFonts<'_>,
) -> Result<Vec<u8>, PdfError> {
    to_pdf_bytes_with_marks_and_latest(p, meta, opts, &[], None, fonts)
}

pub fn to_pdf_bytes_with_marks(
    p: &Project,
    meta: &Metadata,
    opts: &ExportOptions,
    marks: &[Option<RevisionColor>],
    fonts: &ExportFonts<'_>,
) -> Result<Vec<u8>, PdfError> {
    to_pdf_bytes_with_marks_and_latest(p, meta, opts, marks, None, fonts)
}

pub fn to_pdf_bytes_with_marks_and_latest(
    p: &Project,
    meta: &Metadata,
    opts: &ExportOptions,
    marks: &[Option<RevisionColor>],
    latest: Option<&RevisionSet>,
    fonts: &ExportFonts<'_>,
) -> Result<Vec<u8>, PdfError> {
    if p.blocks.is_empty() {
        return Err(PdfError::Empty);
    }
    let title = if p.title.is_empty() {
        "Untitled".to_string()
    } else {
        p.title.clone()
    };
    let mut doc = PdfDocument::new(&title);
    let mut font_warnings = Vec::new();
    let mut parse = |bytes: &[u8]| ParsedFont::from_bytes(bytes, 0, &mut font_warnings);
    let regular_font = parse(fonts.regular).ok_or(PdfError::Font)?;
    let bold_font = parse(fonts.bold).ok_or(PdfError::Font)?;
    let fallback_regular_font = parse(fonts.fallback_regular).ok_or(PdfError::Font)?;
    let fallback_bold_font = parse(fonts.fallback_bold).ok_or(PdfError::Font)?;
    let regular = Face {
        id: doc.add_font(&regular_font),
        font: regular_font,
    };
    let bold = Face {
        id: doc.add_font(&bold_font),
        font: bold_font,
    };
    let fallback_regular = Face {
        id: doc.add_font(&fallback_regular_font),
        font: fallback_regular_font,
    };
    let fallback_bold = Face {
        id: doc.add_font(&fallback_bold_font),
        font: fallback_bold_font,
    };
    let italic_opt: Option<Face> = fonts.italic.and_then(&mut parse).map(|font| Face {
        id: doc.add_font(&font),
        font,
    });
    let fallback_italic_opt: Option<Face> =
        fonts.fallback_italic.and_then(&mut parse).map(|font| Face {
            id: doc.add_font(&font),
            font,
        });
    let italic_face = italic_opt.as_ref().unwrap_or(&regular);
    let fallback_italic_face = fallback_italic_opt.as_ref().unwrap_or(&fallback_regular);
    let spec = page_spec(opts.page_size);
    let page_w = spec.width_mm;
    let page_h = spec.height_mm;
    let margin_l = spec.margin_l_mm;
    let margin_t = spec.margin_t_mm;
    let text_right = spec.text_right_mm();
    let page_num_baseline = margin_t - 0.4 * PITCH_MM - DESCENT_MM;
    let mut pages: Vec<Vec<PrintLine>> = Vec::new();
    if opts.title_page {
        pages.push(layout_title_page_for(meta, latest, opts.page_size));
    }
    pages.extend(layout_pages_with_marks_and_latest(
        p, meta, opts, marks, latest,
    ));
    if opts.revision_pages_only {
        let skip_title = usize::from(opts.title_page);
        pages = pages
            .into_iter()
            .skip(skip_title)
            .filter(|lines| lines.iter().any(|l| l.revision.is_some()))
            .collect();
        if pages.is_empty() {
            return Err(PdfError::NoRevisionPages);
        }
    }
    let watermark = opts
        .watermark
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let black = Color::Greyscale(Greyscale::new(0.0, None));
    // All coordinates in this module are millimetres; TextMatrix takes points.
    let show = push_text;

    let mut doc_pages: Vec<PdfPage> = Vec::new();
    for lines in pages.iter() {
        let mut ops: Vec<Op> = vec![Op::SetOutlineThickness { pt: Pt(0.72) }];
        if let Some(wm) = watermark {
            let size = 40.0;
            let w = wm.chars().count() as f32 * CHAR_MM * (size / FONT_PT);
            ops.push(Op::SetFillColor {
                col: Color::Greyscale(Greyscale::new(0.82, None)),
            });
            let mut cx = ((page_w - w) / 2.0).max(0.0);
            for (use_fb, run) in split_runs(wm, &regular.font, &fallback_regular.font) {
                let id = if use_fb {
                    &fallback_regular.id
                } else {
                    &regular.id
                };
                show(&mut ops, id, size, cx, page_h / 2.0, run);
                cx += run.chars().count() as f32 * CHAR_MM * (size / FONT_PT);
            }
        }
        let mut y = page_h - margin_t - BASELINE_OFF;
        for line in lines.iter() {
            if line.share_row {
                y += PITCH_MM;
            }
            ops.push(Op::SetFillColor { col: black.clone() });
            if let Some(band) = line.margin {
                let yy = match band {
                    MarginBand::Header => page_h - page_num_baseline,
                    MarginBand::Footer => FOOTER_BASELINE_MM,
                };
                let (face, fb_face) = if line.bold {
                    (&bold, &fallback_bold)
                } else {
                    (&regular, &fallback_regular)
                };
                let mut cx = line.x_mm;
                for (use_fb, run) in split_runs(&line.text, &face.font, &fb_face.font) {
                    let id = if use_fb { &fb_face.id } else { &face.id };
                    show(&mut ops, id, FONT_PT, cx, yy, run);
                    cx += run.chars().count() as f32 * CHAR_MM;
                }
                continue;
            }
            if line.is_page_number {
                let w = line.text.chars().count() as f32 * CHAR_MM;
                let mut cx = text_right - w;
                let (face, fb_face) = if line.bold {
                    (&bold, &fallback_bold)
                } else {
                    (&regular, &fallback_regular)
                };
                for (use_fb, run) in split_runs(&line.text, &face.font, &fb_face.font) {
                    let id = if use_fb { &fb_face.id } else { &face.id };
                    show(&mut ops, id, FONT_PT, cx, page_h - page_num_baseline, run);
                    cx += run.chars().count() as f32 * CHAR_MM;
                }
                continue;
            }
            let yy = line.y_from_bottom_mm.unwrap_or(y - line.dy_mm);
            if let Some(revision) = line.revision {
                let (r, g, b) = revision.rgb();
                ops.push(Op::SetFillColor {
                    col: Color::Rgb(Rgb::new(
                        r as f32 / 255.0,
                        g as f32 / 255.0,
                        b as f32 / 255.0,
                        None,
                    )),
                });
                let mut bar =
                    PdfRect::from_wh(Pt(REVISION_BAR_W_MM * MM_TO_PT), Pt(PITCH_MM * MM_TO_PT));
                bar.x = Pt(REVISION_BAR_X_MM * MM_TO_PT);
                bar.y = Pt((yy + BASELINE_OFF) * MM_TO_PT);
                ops.push(Op::DrawPolygon {
                    polygon: bar.to_polygon(),
                });
                ops.push(Op::SetFillColor { col: black.clone() });
            }
            if !line.text.is_empty() {
                let w = line.text.chars().count() as f32 * CHAR_MM;
                let x = if line.right_aligned {
                    (text_right - w).max(margin_l)
                } else {
                    line.x_mm
                };
                if line.runs.is_empty() {
                    let (face, fb_face) = if line.bold {
                        (&bold, &fallback_bold)
                    } else {
                        (&regular, &fallback_regular)
                    };
                    let mut cx = x;
                    for (use_fb, run) in split_runs(&line.text, &face.font, &fb_face.font) {
                        let id = if use_fb { &fb_face.id } else { &face.id };
                        show(&mut ops, id, FONT_PT, cx, yy, run);
                        cx += run.chars().count() as f32 * CHAR_MM;
                    }
                    if line.underline {
                        ops.push(Op::DrawLine {
                            line: Line {
                                points: vec![
                                    LinePoint {
                                        p: Point::new(Mm(x), Mm(yy - 1.2)),
                                        bezier: false,
                                    },
                                    LinePoint {
                                        p: Point::new(Mm(x + w), Mm(yy - 1.2)),
                                        bezier: false,
                                    },
                                ],
                                is_closed: false,
                            },
                        });
                    }
                } else {
                    let mut cx = x;
                    for styled in &line.runs {
                        let rw = styled.text.chars().count() as f32 * CHAR_MM;
                        let run_start_x = cx;
                        let (face, fb_face) = if styled.italic {
                            (italic_face, fallback_italic_face)
                        } else if styled.bold {
                            (&bold, &fallback_bold)
                        } else {
                            (&regular, &fallback_regular)
                        };
                        let faux_bold_italic = styled.bold && styled.italic;
                        for (use_fb, seg) in split_runs(&styled.text, &face.font, &fb_face.font) {
                            let id = if use_fb { &fb_face.id } else { &face.id };
                            show(&mut ops, id, FONT_PT, cx, yy, seg);
                            if faux_bold_italic {
                                show(&mut ops, id, FONT_PT, cx + 0.09, yy, seg);
                            }
                            cx += seg.chars().count() as f32 * CHAR_MM;
                        }
                        if styled.underline || line.underline {
                            ops.push(Op::DrawLine {
                                line: Line {
                                    points: vec![
                                        LinePoint {
                                            p: Point::new(Mm(run_start_x), Mm(yy - 1.2)),
                                            bezier: false,
                                        },
                                        LinePoint {
                                            p: Point::new(Mm(run_start_x + rw), Mm(yy - 1.2)),
                                            bezier: false,
                                        },
                                    ],
                                    is_closed: false,
                                },
                            });
                        }
                    }
                }
            }
            y -= PITCH_MM;
        }
        doc_pages.push(PdfPage::new(Mm(page_w), Mm(page_h), ops));
    }
    doc.with_pages(doc_pages);
    let mut warnings = Vec::new();
    let bytes = doc.save(&PdfSaveOptions::default(), &mut warnings);
    Ok(bytes)
}

/// One text run at a millimetre position (baseline). Shared by the script
/// and summary exporters.
fn push_text(ops: &mut Vec<Op>, font: &printpdf::FontId, size: f32, x: f32, y: f32, text: &str) {
    ops.push(Op::StartTextSection);
    ops.push(Op::SetFont {
        font: PdfFontHandle::External(font.clone()),
        size: Pt(size),
    });
    ops.push(Op::SetTextMatrix {
        matrix: TextMatrix::Translate(Pt(x * MM_TO_PT), Pt(y * MM_TO_PT)),
    });
    ops.push(Op::ShowText {
        items: vec![TextItem::Text(text.to_string())],
    });
    ops.push(Op::EndTextSection);
}

fn draw_summary_text(
    ops: &mut Vec<Op>,
    face: &Face,
    fallback: &Face,
    size: f32,
    x: f32,
    y: f32,
    text: &str,
) {
    let mut cx = x;
    for (use_fb, run) in split_runs(text, &face.font, &fallback.font) {
        let id = if use_fb { &fallback.id } else { &face.id };
        push_text(ops, id, size, cx, y, run);
        cx += run.chars().count() as f32 * CHAR_MM * (size / FONT_PT);
    }
}

fn right_aligned_x(text: &str, size: f32, right: f32) -> f32 {
    right - text.chars().count() as f32 * CHAR_MM * (size / FONT_PT)
}

const SUMMARY_MARGIN_MM: f32 = 25.0;
const SUMMARY_LINE_MM: f32 = 6.0;
const SUMMARY_BOTTOM_MM: f32 = 30.0;
const SUMMARY_SCENE_X: f32 = SUMMARY_MARGIN_MM;
const SUMMARY_HEADING_X: f32 = 57.0;
const SUMMARY_HEADING_CHARS: usize = 42;
const SUMMARY_ADD_RIGHT: f32 = 163.0;
const SUMMARY_REM_RIGHT: f32 = 174.0;
const SUMMARY_CHG_RIGHT: f32 = 185.0;

/// Paginated sheet builder for the printable revision summary.
struct SummarySheet<'a> {
    pages: Vec<Vec<Op>>,
    ops: Vec<Op>,
    y: f32,
    regular: &'a Face,
    bold: &'a Face,
    fallback_regular: &'a Face,
    fallback_bold: &'a Face,
}

impl<'a> SummarySheet<'a> {
    fn new(
        regular: &'a Face,
        bold: &'a Face,
        fallback_regular: &'a Face,
        fallback_bold: &'a Face,
    ) -> Self {
        Self {
            pages: Vec::new(),
            ops: vec![Op::SetOutlineThickness { pt: Pt(0.72) }],
            y: PAGE_H - SUMMARY_MARGIN_MM,
            regular,
            bold,
            fallback_regular,
            fallback_bold,
        }
    }

    /// Start a new page if `lines` would cross the bottom margin.
    fn ensure(&mut self, lines: usize) {
        let last = self.y - (lines.saturating_sub(1)) as f32 * SUMMARY_LINE_MM;
        if last < SUMMARY_BOTTOM_MM {
            self.new_page();
        }
    }

    fn new_page(&mut self) {
        self.pages.push(std::mem::take(&mut self.ops));
        self.ops = vec![Op::SetOutlineThickness { pt: Pt(0.72) }];
        self.y = PAGE_H - SUMMARY_MARGIN_MM;
        self.text(
            12.0,
            SUMMARY_MARGIN_MM,
            "REVISION SUMMARY (continued)",
            true,
        );
        self.y -= 1.0;
    }

    fn text(&mut self, size: f32, x: f32, text: &str, bold: bool) {
        self.ensure(1);
        let (face, fallback) = if bold {
            (self.bold, self.fallback_bold)
        } else {
            (self.regular, self.fallback_regular)
        };
        draw_summary_text(&mut self.ops, face, fallback, size, x, self.y, text);
        self.y -= SUMMARY_LINE_MM;
    }

    fn gap(&mut self, mm: f32) {
        self.y -= mm;
    }

    fn finish(mut self) -> Vec<Vec<Op>> {
        if !self.ops.is_empty() {
            self.pages.push(std::mem::take(&mut self.ops));
        }
        let total = self.pages.len();
        for (index, ops) in self.pages.iter_mut().enumerate() {
            let label = format!("Page {} of {}", index + 1, total);
            let x = right_aligned_x(&label, 9.0, PAGE_W - SUMMARY_MARGIN_MM);
            draw_summary_text(
                ops,
                self.regular,
                self.fallback_regular,
                9.0,
                x,
                15.0,
                &label,
            );
        }
        self.pages
    }
}

/// Render the revision summary as a printable A4 PDF: revision metadata,
/// totals, and one row per changed scene with a totals row. This is the
/// artifact behind the summary's Print button, not a screenplay export.
pub fn to_summary_pdf_bytes(
    summary: &RevisionSummary,
    project_title: &str,
    fonts: &ExportFonts<'_>,
) -> Result<Vec<u8>, PdfError> {
    let mut doc = PdfDocument::new("Revision Summary");
    let mut font_warnings = Vec::new();
    let mut parse = |bytes: &[u8]| ParsedFont::from_bytes(bytes, 0, &mut font_warnings);
    let regular_font = parse(fonts.regular).ok_or(PdfError::Font)?;
    let bold_font = parse(fonts.bold).ok_or(PdfError::Font)?;
    let fallback_regular_font = parse(fonts.fallback_regular).ok_or(PdfError::Font)?;
    let fallback_bold_font = parse(fonts.fallback_bold).ok_or(PdfError::Font)?;
    let regular = Face {
        id: doc.add_font(&regular_font),
        font: regular_font,
    };
    let bold = Face {
        id: doc.add_font(&bold_font),
        font: bold_font,
    };
    let fallback_regular = Face {
        id: doc.add_font(&fallback_regular_font),
        font: fallback_regular_font,
    };
    let fallback_bold = Face {
        id: doc.add_font(&fallback_bold_font),
        font: fallback_bold_font,
    };

    let mut sheet = SummarySheet::new(&regular, &bold, &fallback_regular, &fallback_bold);
    sheet.text(16.0, SUMMARY_MARGIN_MM, "REVISION SUMMARY", true);
    sheet.gap(1.0);
    sheet.text(12.0, SUMMARY_MARGIN_MM, project_title, false);
    sheet.gap(1.5);
    let revision = if summary.name.trim().is_empty() {
        format!("{} Revision", summary.color.label())
    } else {
        format!("{} Revision - {}", summary.color.label(), summary.name)
    };
    sheet.text(12.0, SUMMARY_MARGIN_MM, &revision, true);
    let pages = format!(
        "{} pages to {} pages ({:+})",
        summary.base_pages,
        summary.current_pages,
        summary.pages_delta()
    );
    let date = if summary.date.is_empty() {
        pages
    } else {
        format!("{}  -  {}", summary.date, pages)
    };
    sheet.text(10.0, SUMMARY_MARGIN_MM, &date, false);
    sheet.gap(3.0);
    sheet.text(
        11.0,
        SUMMARY_MARGIN_MM,
        &format!(
            "Added {}   Removed {}   Changed {}",
            summary.added, summary.removed, summary.changed
        ),
        false,
    );
    sheet.gap(4.0);

    sheet.text(10.0, SUMMARY_SCENE_X, "SCENE", true);
    sheet.text(10.0, SUMMARY_HEADING_X, "HEADING", true);
    for (label, right) in [
        ("+ADD", SUMMARY_ADD_RIGHT),
        ("-REM", SUMMARY_REM_RIGHT),
        ("~CHG", SUMMARY_CHG_RIGHT),
    ] {
        sheet.text(10.0, right_aligned_x(label, 10.0, right), label, true);
    }

    if summary.scenes.is_empty() {
        sheet.text(
            10.0,
            SUMMARY_MARGIN_MM,
            "No changes in this revision.",
            false,
        );
    }
    for scene in &summary.scenes {
        let parts = wrap_title_line(&scene.heading, SUMMARY_HEADING_CHARS);
        sheet.ensure(parts.len());
        let label = summary.scene_label(scene);
        sheet.text(10.0, SUMMARY_SCENE_X, &label, false);
        for (index, part) in parts.iter().enumerate() {
            sheet.text(10.0, SUMMARY_HEADING_X, part, false);
            if index == 0 {
                let counts = [
                    (scene.added.to_string(), SUMMARY_ADD_RIGHT),
                    (scene.removed.to_string(), SUMMARY_REM_RIGHT),
                    (scene.changed.to_string(), SUMMARY_CHG_RIGHT),
                ];
                for (count, right) in counts {
                    sheet.text(10.0, right_aligned_x(&count, 10.0, right), &count, false);
                }
            }
        }
    }

    sheet.gap(1.0);
    sheet.ensure(1);
    sheet.text(10.0, SUMMARY_HEADING_X, "TOTAL", true);
    for (count, right) in [
        (summary.added.to_string(), SUMMARY_ADD_RIGHT),
        (summary.removed.to_string(), SUMMARY_REM_RIGHT),
        (summary.changed.to_string(), SUMMARY_CHG_RIGHT),
    ] {
        sheet.text(10.0, right_aligned_x(&count, 10.0, right), &count, true);
    }

    let pages: Vec<PdfPage> = sheet
        .finish()
        .into_iter()
        .map(|ops| PdfPage::new(Mm(PAGE_W), Mm(PAGE_H), ops))
        .collect();
    doc.with_pages(pages);
    let mut warnings = Vec::new();
    let bytes = doc.save(&PdfSaveOptions::default(), &mut warnings);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use scriptcraft_core::model::ScriptBlock;

    fn sample() -> Project {
        let mut p = Project {
            title: "T".into(),
            blocks: vec![],
            ..Default::default()
        };
        for (t, text) in [
            (ElementType::SceneHeading, "EXT. PARK - DAY"),
            (ElementType::Action, "A man walks in."),
            (ElementType::Character, "IVAN"),
            (
                ElementType::Dialogue,
                "Hello there, how are you doing today my friend?",
            ),
            (ElementType::Parenthetical, "(softly)"),
            (ElementType::Dialogue, "Again."),
            (ElementType::Transition, "FADE OUT:"),
        ] {
            p.blocks.push(ScriptBlock::new(t, text.into()));
        }
        p
    }

    fn default_opts() -> ExportOptions {
        ExportOptions::default()
    }

    #[test]
    fn layout_matches_pagination_line_budget() {
        let p = sample();
        let pages = paginate(&p.blocks, &FINAL_DRAFT_A4);
        let laid = layout_pages(&p, &Default::default(), &default_opts());
        assert_eq!(laid.len(), pages.len());
        // One printed row per budgeted row, plus out-of-band lines:
        // scene numbers and page numbers share a row, (top) blanks and
        // contd cues count as rows.
        for (pi, (page, lines)) in pages.iter().zip(laid.iter()).enumerate() {
            let mut rows = 0usize;
            for c in &page.chunks {
                rows += c.top;
                let b = &p.blocks[c.bi];
                let slice: String = b
                    .text
                    .split('\n')
                    .skip(c.start)
                    .take(c.end - c.start)
                    .collect::<Vec<_>>()
                    .join("\n");
                rows += wrap_lines(&slice, FINAL_DRAFT_A4.col_chars(b.element_type))
                    .len()
                    .max(1);
                if c.contd {
                    rows += 1;
                }
                if c.more {
                    rows += 1;
                }
            }
            assert!(rows <= FINAL_DRAFT_A4.lines_per_page, "page {pi} overflow");
            let share = lines.iter().filter(|l| l.share_row).count();
            let banded = lines
                .iter()
                .filter(|l| l.is_page_number || l.margin.is_some())
                .count();
            assert_eq!(
                rows + share + banded,
                lines.len(),
                "page {pi} line mismatch"
            );
        }
    }

    #[test]
    fn layout_positions_match_template_columns() {
        let p = sample();
        let laid = layout_pages(&p, &Default::default(), &default_opts());
        let page = &laid[0];
        // Scene number outdented, heading shares its row at the margin.
        let num = page.iter().position(|l| l.text == "1.").unwrap();
        let w = 2.0 * CHAR_MM;
        assert!((page[num].x_mm - (MARGIN_L - SCENE_NUM_GAP - w)).abs() < 0.01);
        assert!(!page[num].bold && !page[num].share_row);
        let head = &page[num + 1];
        assert_eq!(head.text, "EXT. PARK - DAY");
        assert_eq!(head.x_mm, MARGIN_L);
        assert!(!head.bold && head.share_row);
        // Character cue at 38 + 51.
        let cue = page.iter().find(|l| l.text == "IVAN").unwrap();
        assert!((cue.x_mm - X_CUE).abs() < 0.01);
        assert!(!cue.bold);
        // Dialogue at 38 + 26.
        let dlg = page.iter().find(|l| l.text.starts_with("Hello")).unwrap();
        assert!((dlg.x_mm - X_DLG).abs() < 0.01);
        assert!(!dlg.bold);
        // Parenthetical at 38 + 36.
        let par = page.iter().find(|l| l.text == "(softly)").unwrap();
        assert!((par.x_mm - X_PAREN).abs() < 0.01);
        // Transitions are right-aligned, regular weight, like the template.
        let tr = page.iter().find(|l| l.text == "FADE OUT:").unwrap();
        assert!(tr.right_aligned && !tr.bold);
    }

    #[test]
    fn revision_marks_color_the_changed_blocks() {
        let p = sample();
        let mut marks = vec![None; p.blocks.len()];
        marks[1] = Some(RevisionColor::Blue);
        marks[3] = Some(RevisionColor::Pink);
        let opts = ExportOptions {
            revision_marks: true,
            ..default_opts()
        };
        let laid = layout_pages_with_marks(&p, &Default::default(), &opts, &marks);
        let action = laid[0]
            .iter()
            .find(|l| l.text == "A man walks in.")
            .unwrap();
        assert_eq!(action.revision, Some(RevisionColor::Blue));
        let dialogue = laid[0]
            .iter()
            .find(|l| l.text.starts_with("Hello"))
            .unwrap();
        assert_eq!(dialogue.revision, Some(RevisionColor::Pink));
        let heading = laid[0]
            .iter()
            .find(|l| l.text == "EXT. PARK - DAY")
            .unwrap();
        assert_eq!(heading.revision, None);

        let plain = layout_pages_with_marks(&p, &Default::default(), &default_opts(), &marks);
        assert!(plain.iter().flatten().all(|l| l.revision.is_none()));
    }

    #[test]
    fn layout_skips_non_printable() {
        let mut p = sample();
        p.blocks.push(ScriptBlock::new(
            ElementType::NonPrintable,
            "secret note".into(),
        ));
        let laid = layout_pages(&p, &Default::default(), &default_opts());
        assert!(!laid.iter().flatten().any(|l| l.text.contains("secret")));
    }

    #[test]
    fn scene_numbers_follow_meta_and_option() {
        let p = sample();
        let mut meta = Metadata {
            scene_number_prefix: "SC".into(),
            scene_number_start: "7".into(),
            ..Default::default()
        };
        let laid = layout_pages(&p, &meta, &default_opts());
        assert!(laid[0].iter().any(|l| l.text == "SC7."));
        // Locked labels win.
        meta.locked_scene_numbers = "*A,B".into();
        let laid = layout_pages(&p, &meta, &default_opts());
        assert!(laid[0].iter().any(|l| l.text == "A."));
        // Option off: no numbers at all (only scene numbers share a row).
        let opts = ExportOptions {
            scene_numbers: false,
            ..default_opts()
        };
        let laid = layout_pages(&p, &Default::default(), &opts);
        assert!(!laid.iter().flatten().any(|l| l.share_row));
    }

    #[test]
    fn speaking_parts_count_appends_counts() {
        let p = sample();
        let opts = ExportOptions {
            speaking_parts_count: true,
            ..default_opts()
        };
        let laid = layout_pages(&p, &Default::default(), &opts);
        let cue = laid[0].iter().find(|l| l.text.starts_with("IVAN")).unwrap();
        assert_eq!(cue.text, "IVAN (2)");
        // Default keeps the plain cue.
        let laid = layout_pages(&p, &Default::default(), &default_opts());
        assert!(laid[0].iter().any(|l| l.text == "IVAN"));
    }

    fn sample_meta() -> scriptcraft_core::kitsp::Metadata {
        scriptcraft_core::kitsp::Metadata {
            title: "Test Title".to_string(),
            genre: "Drama".to_string(),
            author: "Jane Doe".to_string(),
            director: "John Smith".to_string(),
            contacts: "jane@example.com".to_string(),
            year: "2026".to_string(),
            additional_info: String::new(),
            logline_html: String::new(),
            synopsis_html: String::new(),
            ..Default::default()
        }
    }

    #[test]
    fn title_page_matches_kit_layout() {
        let lines = layout_title_page(&sample_meta());
        assert_eq!(lines.len(), 58, "title page is exactly one page");
        // KIT layout: 16 blanks, uppercase underlined title on slot 16.
        assert!(lines[..16].iter().all(|l| l.text.is_empty()));
        assert_eq!(lines[16].text, "TEST TITLE");
        assert!(lines[16].bold && lines[16].underline);
        assert!(lines[16].x_mm > MARGIN_L, "title is centered");
        // Genre / author / director from slot 20, every other slot.
        assert_eq!(lines[20].text, "Drama");
        assert_eq!(lines[22].text, "Jane Doe");
        assert_eq!(lines[24].text, "John Smith");
        assert!((lines[20].dy_mm - TITLE_VALUE_DY).abs() < 0.01);
        // Contacts on slot 39, left aligned; year on slot 57, centered.
        assert_eq!(lines[39].text, "jane@example.com");
        assert_eq!(lines[39].x_mm, MARGIN_L);
        assert_eq!(lines[57].text, "2026");
        assert!(lines[57].x_mm > MARGIN_L);
    }

    #[test]
    fn header_footer_expand_revision_tokens() {
        let p = sample();
        let set = RevisionSet {
            color: RevisionColor::Blue,
            name: "Blue Revision".into(),
            started: "21.10.2026 16:03".into(),
            pages: 1,
            baseline: vec![],
        };
        let meta = Metadata {
            header: "{revision} DRAFT — {date}".into(),
            footer: "Page {page} · {revision}".into(),
            ..Default::default()
        };
        let opts = ExportOptions {
            revision_marks: true,
            ..default_opts()
        };
        let laid = layout_pages_with_marks_and_latest(&p, &meta, &opts, &[], Some(&set));
        let header = laid[0]
            .iter()
            .find(|l| l.margin == Some(MarginBand::Header))
            .unwrap();
        assert_eq!(header.text, "BLUE DRAFT — 21.10.2026");
        let footer = laid[0]
            .iter()
            .find(|l| l.margin == Some(MarginBand::Footer))
            .unwrap();
        assert_eq!(footer.text, "Page 1 · BLUE");
        assert!(!footer.is_page_number && footer.revision.is_none());
    }

    #[test]
    fn empty_header_gets_revision_label_only_when_marking() {
        let p = sample();
        let set = RevisionSet {
            color: RevisionColor::Pink,
            name: "Pink Revision".into(),
            started: "22.10.2026 09:00".into(),
            pages: 1,
            baseline: vec![],
        };
        let marked = ExportOptions {
            revision_marks: true,
            ..default_opts()
        };
        let laid =
            layout_pages_with_marks_and_latest(&p, &Default::default(), &marked, &[], Some(&set));
        assert!(laid[0]
            .iter()
            .any(|l| l.text == "Pink Revision — 22.10.2026"));
        let plain = layout_pages_with_marks_and_latest(
            &p,
            &Default::default(),
            &default_opts(),
            &[],
            Some(&set),
        );
        assert!(!plain.iter().flatten().any(|l| l.margin.is_some()));
    }

    #[test]
    fn title_page_wraps_long_titles_and_splits_contacts() {
        let meta = scriptcraft_core::kitsp::Metadata {
            title: "THE EXTRAORDINARILY LONG TITLE OF A FILM THAT KEEPS GOING AND GOING".into(),
            contacts: "writer@example.com\n+1 555 0100".into(),
            ..sample_meta()
        };
        let lines = layout_title_page(&meta);
        let title: Vec<&PrintLine> = lines[16..].iter().take_while(|l| l.underline).collect();
        assert!(title.len() >= 2, "long title wraps instead of running off");
        for line in title {
            let w = line.text.chars().count() as f32 * CHAR_MM;
            assert!(line.x_mm >= 0.0, "wrapped title starts on the page");
            assert!(line.x_mm + w <= PAGE_W, "wrapped title stays on the page");
        }
        assert_eq!(lines[39].text, "writer@example.com");
        assert_eq!(lines[40].text, "+1 555 0100");
        assert_eq!(lines[57].text, "2026");
        assert_eq!(lines.len(), 58, "short page keeps KIT's one-page shape");
    }

    #[test]
    fn title_page_edges_do_not_run_off_the_page() {
        let meta = scriptcraft_core::kitsp::Metadata {
            title: "SUPERCALIFRAGILISTICEXPIALIDOCIOUSSUPERCALIFRAGILISTICEXPIALIDOCIOUS".into(),
            contacts: "x".repeat(120),
            ..sample_meta()
        };
        let lines = layout_title_page(&meta);
        for line in &lines {
            let w = line.text.chars().count() as f32 * CHAR_MM;
            assert!(
                line.x_mm >= 0.0 && line.x_mm + w <= PAGE_W + 0.01,
                "line {:?} runs off the page",
                line.text
            );
        }
    }

    #[test]
    fn title_page_adds_revised_fields() {
        let set = RevisionSet {
            color: RevisionColor::Blue,
            name: "Blue Revision".into(),
            started: "21.10.2026 16:03".into(),
            pages: 342,
            baseline: vec![],
        };
        let lines = layout_title_page_with_latest(&sample_meta(), Some(&set));
        assert!(lines.len() > 60);
        assert_eq!(lines[60].text, "REVISED");
        assert_eq!(lines[61].text, "Blue Revision");
        assert_eq!(lines[62].text, "21.10.2026");
        assert!(lines[60].bold);
    }

    /// Everything on the title page must print inside the paper. The draft
    /// date used to sit at slot 57 on the old font-derived 4.754 mm pitch,
    /// which was below the bottom edge; the REVISED block was further out
    /// still. The draft date stays bottom-anchored so it cannot drift with the
    /// line grid again.
    #[test]
    fn title_page_lines_all_sit_on_the_page() {
        let set = RevisionSet {
            color: RevisionColor::Blue,
            name: "Blue Revision".into(),
            started: "21.10.2026 16:03".into(),
            pages: 342,
            baseline: vec![],
        };
        for latest in [None, Some(&set)] {
            let lines = layout_title_page_with_latest(&sample_meta(), latest);
            for (i, line) in lines.iter().enumerate() {
                if line.text.is_empty() {
                    continue;
                }
                let y = title_page_line_y_mm(i, line);
                assert!(
                    y > 0.0 && y < PAGE_H,
                    "title line {i} {:?} prints at {y} mm, off the page",
                    line.text
                );
            }
        }
    }

    #[test]
    fn draft_date_is_bottom_anchored_not_on_the_slot_grid() {
        let lines = layout_title_page(&sample_meta());
        let year = &lines[57];
        assert_eq!(year.text, "2026");
        assert_eq!(year.y_from_bottom_mm, Some(TITLE_YEAR_Y_MM));
        assert_eq!(title_page_line_y_mm(57, year), TITLE_YEAR_Y_MM);
        // The old Courier-hhea grid (4.754 mm) put slot 57 below the paper
        // edge; the standard 1/6-inch grid keeps it on the page, which is why
        // the bottom anchor still matters for taller title blocks.
        let old_pitch = 4.754_232;
        let old_slot = PAGE_H - MARGIN_T - BASELINE_OFF - 57.0 * old_pitch;
        assert!(old_slot < 0.0, "old slot 57 sat at {old_slot} mm");
        let slot_baseline = PAGE_H - MARGIN_T - BASELINE_OFF - 57.0 * PITCH_MM;
        assert!(slot_baseline > 0.0, "slot 57 sits at {slot_baseline} mm");
    }

    #[test]
    fn revised_block_is_bottom_anchored_and_ordered() {
        let set = RevisionSet {
            color: RevisionColor::Blue,
            name: "Blue Revision".into(),
            started: "21.10.2026 16:03".into(),
            pages: 342,
            baseline: vec![],
        };
        let lines = layout_title_page_with_latest(&sample_meta(), Some(&set));
        let y = |i: usize| title_page_line_y_mm(i, &lines[i]);
        assert_eq!(lines[60].text, "REVISED");
        assert_eq!(lines[61].text, "Blue Revision");
        assert_eq!(lines[62].text, "21.10.2026");
        assert!(
            y(60) > y(61) && y(61) > y(62),
            "REVISED, name, date stack down"
        );
        assert!(y(62) > 0.0, "date stays on the page");
        assert!(y(60) > y(57), "REVISED sits above the draft date");
    }

    #[test]
    fn page_lock_inserts_a_labels() {
        let mut p = sample();
        for i in 0..30 {
            p.blocks.push(ScriptBlock::new(
                ElementType::Action,
                format!("Original action {i}."),
            ));
        }
        let starts = crate::paginate::page_starts(&p.blocks, &FINAL_DRAFT_A4);
        assert!(starts.len() >= 2, "need at least two pages");
        let lock = PageLock::capture(&p.blocks, &starts);
        let meta = Metadata {
            locked_pages: lock.to_json().unwrap(),
            ..Default::default()
        };
        let at = starts[1].0;
        for i in 0..40 {
            p.blocks.insert(
                at,
                ScriptBlock::new(ElementType::Action, format!("Inserted action {i}.")),
            );
        }
        let laid = layout_pages(&p, &meta, &default_opts());
        let numbers: Vec<String> = laid
            .iter()
            .filter_map(|page| {
                page.iter()
                    .find(|l| l.is_page_number)
                    .map(|l| l.text.clone())
            })
            .collect();
        assert_eq!(numbers.first().map(String::as_str), Some("1A."));
        assert!(numbers.iter().any(|n| n == "2."), "{numbers:?}");
    }

    #[test]
    fn script_page_numbers_skip_first_page() {
        // Single-page sample: no number line at all.
        let p = sample();
        let laid = layout_pages(&p, &Default::default(), &default_opts());
        assert!(!laid[0].iter().any(|l| l.is_page_number));
        // A long script: second script page carries a bold right "2.".
        let mut p = sample();
        for _ in 0..80 {
            p.blocks
                .push(ScriptBlock::new(ElementType::Action, "More action.".into()));
        }
        let laid = layout_pages(&p, &Default::default(), &default_opts());
        assert!(laid.len() >= 2);
        assert!(!laid[0].iter().any(|l| l.is_page_number));
        let num = laid[1].iter().find(|l| l.is_page_number).unwrap();
        assert_eq!(num.text, "2.");
        assert!(num.bold && num.right_aligned);
        // Disabled option: never a number line.
        let opts = ExportOptions {
            page_numbers: false,
            ..default_opts()
        };
        let laid = layout_pages(&p, &Default::default(), &opts);
        assert!(!laid.iter().flatten().any(|l| l.is_page_number));
    }

    #[test]
    fn us_letter_paginates_at_55_lines_and_defaults_stay_a4() {
        assert_eq!(ExportOptions::default().page_size, PageSize::A4);
        let mut p = sample();
        for i in 0..60 {
            p.blocks.push(ScriptBlock::new(
                ElementType::Action,
                format!("Action beat {i} describing a steady walk down the corridor."),
            ));
        }
        let a4_pages = layout_pages(&p, &Default::default(), &default_opts());
        let letter_opts = ExportOptions {
            page_size: PageSize::UsLetter,
            ..default_opts()
        };
        let letter_pages = layout_pages(&p, &Default::default(), &letter_opts);
        // US Letter holds 55 lines/page vs A4's 58, so the same long script
        // needs at least as many (typically more) pages on US Letter.
        assert!(letter_pages.len() >= a4_pages.len());
        let spec = page_spec(PageSize::UsLetter);
        for page in &letter_pages {
            let body_rows = page
                .iter()
                .filter(|l| !l.is_page_number && l.margin.is_none() && !l.share_row)
                .count();
            assert!(
                body_rows <= spec.metrics.lines_per_page,
                "US Letter page overflowed 55 lines: {body_rows}"
            );
        }
    }

    #[test]
    fn inline_styles_split_print_lines_into_styled_runs() {
        use scriptcraft_core::model::{InlineStyle, StyleSpan};
        let mut blk = ScriptBlock::new(ElementType::Action, "She whispers loudly now.".into());
        blk.styles = vec![
            StyleSpan {
                start: 4,
                end: 12,
                style: InlineStyle::Italic,
            },
            StyleSpan {
                start: 13,
                end: 19,
                style: InlineStyle::Bold,
            },
            StyleSpan {
                start: 20,
                end: 23,
                style: InlineStyle::Underline,
            },
        ];
        blk.normalize_styles();
        let p = Project {
            title: "Styled".into(),
            blocks: vec![blk],
            ..Default::default()
        };
        let pages = layout_pages(&p, &Default::default(), &default_opts());
        let line = pages[0]
            .iter()
            .find(|l| l.text.contains("whispers"))
            .expect("action line present");
        assert!(line.runs.iter().any(|r| r.text == "whispers" && r.italic));
        assert!(line.runs.iter().any(|r| r.text == "loudly" && r.bold));
        assert!(line.runs.iter().any(|r| r.text == "now" && r.underline));
    }
}
