use crate::dual;
use crate::paginate::{page_labels, paginate, paginate_with_breaks};
#[cfg(test)]
use crate::place::FINAL_DRAFT_A4_GEOMETRY;
use crate::place::{page_spec, ROW_MM};
use crate::wrap::wrap_lines;
use printpdf::{
    Color, Greyscale, Line, LinePoint, Mm, Op, PaintMode, ParsedFont, PdfDocument, PdfFontHandle,
    PdfPage, PdfSaveOptions, Point, Polygon, PolygonRing, Pt, RawImage, RawImageData,
    RawImageFormat, Rect as PdfRect, Rgb, TextItem, TextMatrix, WindingOrder, XObjectId,
    XObjectTransform,
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
    pub bold_character_cues: bool,
    pub color_by_shots: bool,
    pub include_shot_legend: bool,
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
            bold_character_cues: false,
            color_by_shots: false,
            include_shot_legend: false,
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
    /// Optional shot-coverage pastel tint RGB (populated only for colored
    /// shot-script exports).
    pub shot_tint: Option<(u8, u8, u8)>,
    /// Exact display-column highlighter spans `(col_start, col_end, rgb)` on
    /// `text` for character-level shot coverage.
    pub shot_highlights: Vec<(usize, usize, (u8, u8, u8))>,
    /// Optional shot code badge `(label, rgb)` drawn in the right margin on
    /// the first line of a shot-covered span.
    pub shot_tag: Option<(String, (u8, u8, u8))>,
    /// Moment-specific beat cue spans `(col_start, col_end, margin_note_text)` on `text`.
    /// `margin_note_text` is non-empty on the first wrapped line of a note and empty on
    /// continuation lines so the underline spans cleanly while the margin callout prints once.
    pub shot_notes: Vec<(usize, usize, String)>,
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
            shot_tint: None,
            shot_highlights: Vec::new(),
            shot_tag: None,
            shot_notes: Vec::new(),
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

/// Compute merged `(col_start, col_end, rgb)` highlight spans and distinct
/// covering shot indices for one wrapped display line. When multiple shots
/// overlap a character, the tighter/more specific span wins its highlight color
/// so partial cuts remain visible inside wider masters.
#[allow(clippy::type_complexity)]
fn line_shot_highlights(
    shots: &scriptcraft_core::shots::ShotList,
    shot_total_chars: &[usize],
    block_spans: &[(usize, usize, usize)],
    para_off: usize,
    cols: &[usize],
) -> (Vec<(usize, usize, (u8, u8, u8))>, Vec<usize>, Option<usize>) {
    let mut hl: Vec<(usize, usize, (u8, u8, u8))> = Vec::new();
    let mut line_shots: Vec<usize> = Vec::new();
    let mut char_counts: HashMap<usize, usize> = HashMap::new();
    let shown = cols.len().saturating_sub(1);
    for (ci, &src_col) in cols.iter().take(shown).enumerate() {
        let abs = para_off + src_col;
        // Prefer the tightest span in this block; break ties by smallest total
        // shot span across the script (so cut-in shots win over wide masters),
        // then later-added shot.
        let mut best: Option<(usize, usize, usize)> = None; // (shot_idx, block_span_len, total_chars)
        for &(si, c0, c1) in block_spans {
            if abs >= c0 && abs < c1 {
                if !line_shots.contains(&si) {
                    line_shots.push(si);
                }
                let span_len = c1 - c0;
                let total_c = shot_total_chars.get(si).copied().unwrap_or(usize::MAX);
                if best.is_none_or(|(_, bl, bt)| (span_len, total_c) <= (bl, bt)) {
                    best = Some((si, span_len, total_c));
                }
            }
        }
        if let Some((si, _, _)) = best {
            *char_counts.entry(si).or_insert(0) += 1;
            if let Some(shot) = shots.shots.get(si) {
                let rgb = shot.color.rgb();
                if let Some(last) = hl.last_mut() {
                    if last.1 == ci && last.2 == rgb {
                        last.1 = ci + 1;
                        continue;
                    }
                }
                hl.push((ci, ci + 1, rgb));
            }
        }
    }
    // Primary shot for the left bar & margin label is the shot covering the
    // most visible characters on this line (ties broken by first in line_shots).
    let primary_shot = line_shots
        .iter()
        .copied()
        .max_by_key(|si| char_counts.get(si).copied().unwrap_or(0));
    (hl, line_shots, primary_shot)
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
    layout_pages_inner(p, meta, opts, marks, latest, false)
}

fn layout_pages_inner(
    p: &Project,
    meta: &Metadata,
    opts: &ExportOptions,
    marks: &[Option<RevisionColor>],
    latest: Option<&RevisionSet>,
    include_shots: bool,
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
    let shot_char_cov: Vec<Vec<(usize, usize, usize)>> = if include_shots && !p.shots.is_empty() {
        p.shots.resolve_block_char_coverage(&renum)
    } else {
        Vec::new()
    };
    let shot_block_notes: Vec<Vec<(usize, usize, usize, usize)>> =
        if include_shots && !p.shots.is_empty() {
            p.shots.resolve_block_notes(&renum)
        } else {
            Vec::new()
        };
    let mut emitted_note_callouts: std::collections::HashSet<(usize, usize)> =
        std::collections::HashSet::new();
    let mut shot_total_chars = vec![0usize; p.shots.shots.len()];
    for spans in &shot_char_cov {
        for &(si, c0, c1) in spans {
            if let Some(slot) = shot_total_chars.get_mut(si) {
                *slot += c1.saturating_sub(c0);
            }
        }
    }
    let mut scene_i = 0usize;
    let mut last_tagged_shot: Option<String> = None;
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
                                opts.bold_character_cues,
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
                        let cell_bold = cell.bold
                            || (opts.bold_character_cues
                                && cell.span.is_some_and(|sp| {
                                    renum[sp.bi].element_type == ElementType::Character
                                }));
                        let mut line = PrintLine::text_line(
                            margin_l + cell.x_mm,
                            cell.text.clone(),
                            cell_bold,
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
                                    cell_bold,
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
                ElementType::Character => (x_cue, opts.bold_character_cues, false),
                ElementType::Parenthetical => (x_paren, false, false),
                ElementType::Dialogue => (x_dlg, false, false),
                ElementType::Lyrics => (x_dlg, false, false),
                ElementType::Transition => (text_right, false, true),
                ElementType::NonPrintable => (x_cue, false, false),
            };
            if chunk.contd {
                if let Some(cue) = cue_name_for(&renum, chunk.bi) {
                    let mut line = PrintLine::text_line(
                        x_cue,
                        format!("{cue} (CONT'D)"),
                        opts.bold_character_cues,
                    );
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
                // Keep a clear visual gutter between scene numbers and the shot color bar.
                let gap = if include_shots && !p.shots.is_empty() {
                    scene_num_gap.max(5.2)
                } else {
                    scene_num_gap
                };
                lines.push(PrintLine::text_line(margin_l - gap - w, label, false));
                number_pushed = true;
            }
            if b.element_type == ElementType::SceneHeading {
                scene_i += 1;
            }
            let block_char_spans: &[(usize, usize, usize)] = shot_char_cov
                .get(chunk.bi)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let block_note_spans: &[(usize, usize, usize, usize)] = shot_block_notes
                .get(chunk.bi)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            if b.styles.is_empty() && block_char_spans.is_empty() && block_note_spans.is_empty() {
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
                    if li == 0 {
                        last_tagged_shot = None;
                    }
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
                        let runs = if b.styles.is_empty() {
                            Vec::new()
                        } else {
                            styled_runs_for_line(b, para_off, &wl.text, &wl.cols, bold, false)
                        };
                        let mut l = PrintLine::text_line(x, wl.text, bold);
                        l.right_aligned = right_aligned;
                        l.share_row = li == 0 && number_pushed;
                        l.revision = revision;
                        l.runs = runs;
                        if !block_char_spans.is_empty() {
                            let (hl, line_shots, primary_shot) = line_shot_highlights(
                                &p.shots,
                                &shot_total_chars,
                                block_char_spans,
                                para_off,
                                &wl.cols,
                            );
                            if !hl.is_empty() {
                                let primary_rgb = primary_shot
                                    .and_then(|si| p.shots.shots.get(si))
                                    .map(|s| s.color.rgb())
                                    .unwrap_or(hl[0].2);
                                l.shot_tint = Some(primary_rgb);
                                l.shot_highlights = hl;
                                if let Some(psi) = primary_shot {
                                    if let Some(primary) = p.shots.shots.get(psi) {
                                        let code = primary.code.trim();
                                        let tag = if line_shots.len() > 1 {
                                            format!("{code} +{}", line_shots.len() - 1)
                                        } else {
                                            code.to_string()
                                        };
                                        if last_tagged_shot.as_deref() != Some(tag.as_str()) {
                                            l.shot_tag = Some((tag.clone(), primary.color.rgb()));
                                            last_tagged_shot = Some(tag);
                                        }
                                    }
                                }
                            } else {
                                last_tagged_shot = None;
                            }
                        } else if li == 0 {
                            last_tagged_shot = None;
                        }
                        if !block_note_spans.is_empty() {
                            let shown = wl.cols.len().saturating_sub(1);
                            for &(si, ni, nc0, nc1) in block_note_spans {
                                let note_text = p
                                    .shots
                                    .shots
                                    .get(si)
                                    .and_then(|s| s.notes.get(ni))
                                    .map(|n| n.text.trim())
                                    .unwrap_or("");
                                if note_text.is_empty() {
                                    continue;
                                }
                                let mut c_lo: Option<usize> = None;
                                let mut c_hi: usize = 0;
                                for (ci, &src_col) in wl.cols.iter().take(shown).enumerate() {
                                    let abs = para_off + src_col;
                                    if abs >= nc0 && abs < nc1 {
                                        if c_lo.is_none() {
                                            c_lo = Some(ci);
                                        }
                                        c_hi = ci + 1;
                                    }
                                }
                                if let Some(col0) = c_lo {
                                    let callout = if emitted_note_callouts.insert((si, ni)) {
                                        note_text.to_string()
                                    } else {
                                        String::new()
                                    };
                                    l.shot_notes.push((col0, c_hi, callout));
                                }
                            }
                        }
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
            shot_tint: None,
            shot_highlights: Vec::new(),
            shot_tag: None,
            shot_notes: Vec::new(),
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
    to_pdf_bytes_inner(p, meta, opts, marks, latest, fonts, false)
}

/// Export the screenplay with shot-coverage pastel line highlights, left color
/// bars, right-margin shot code tags, and a prepended **Shot Legend** table.
pub fn to_colored_script_pdf_bytes(
    p: &Project,
    meta: &Metadata,
    opts: &ExportOptions,
    fonts: &ExportFonts<'_>,
) -> Result<Vec<u8>, PdfError> {
    to_pdf_bytes_inner(p, meta, opts, &[], None, fonts, true)
}

fn to_pdf_bytes_inner(
    p: &Project,
    meta: &Metadata,
    opts: &ExportOptions,
    marks: &[Option<RevisionColor>],
    latest: Option<&RevisionSet>,
    fonts: &ExportFonts<'_>,
    include_shots: bool,
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
    let color_shots = (include_shots || opts.color_by_shots) && !p.shots.is_empty();
    let emit_legend =
        (include_shots || (opts.color_by_shots && opts.include_shot_legend)) && !p.shots.is_empty();
    let mut pages: Vec<Vec<PrintLine>> = Vec::new();
    if opts.title_page {
        pages.push(layout_title_page_for(meta, latest, opts.page_size));
    }
    pages.extend(layout_pages_inner(
        p,
        meta,
        opts,
        marks,
        latest,
        color_shots,
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
    let has_title_page = opts.title_page && !opts.revision_pages_only && !pages.is_empty();
    let legend_insert_at = if has_title_page { 1usize } else { 0usize };
    let mut legend_inserted = false;
    let push_legend = |doc_pages: &mut Vec<PdfPage>| {
        let legend_ops = build_shot_sheet_pages(
            p,
            meta,
            &title,
            page_w,
            page_h,
            &regular,
            &bold,
            &fallback_regular,
            &fallback_bold,
            true,
        );
        for ops in legend_ops {
            doc_pages.push(PdfPage::new(Mm(page_w), Mm(page_h), ops));
        }
    };
    for (page_idx, lines) in pages.iter().enumerate() {
        if emit_legend && !legend_inserted && page_idx == legend_insert_at {
            push_legend(&mut doc_pages);
            legend_inserted = true;
        }
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
        for (line_idx, line) in lines.iter().enumerate() {
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
            if !line.shot_highlights.is_empty() || line.shot_tint.is_some() {
                let base_x = if line.right_aligned {
                    line.x_mm - line.text.chars().count() as f32 * CHAR_MM
                } else {
                    line.x_mm
                };
                if !line.shot_highlights.is_empty() {
                    for &(c0, c1, (r, g, b)) in &line.shot_highlights {
                        let tr = 0.74 + 0.26 * (r as f32 / 255.0);
                        let tg = 0.74 + 0.26 * (g as f32 / 255.0);
                        let tb = 0.74 + 0.26 * (b as f32 / 255.0);
                        ops.push(Op::SetFillColor {
                            col: Color::Rgb(Rgb::new(tr, tg, tb, None)),
                        });
                        let w_mm = ((c1 - c0) as f32 * CHAR_MM) + 1.0;
                        let mut strip =
                            PdfRect::from_wh(Pt(w_mm * MM_TO_PT), Pt(PITCH_MM * MM_TO_PT));
                        strip.x = Pt((base_x + c0 as f32 * CHAR_MM - 0.5) * MM_TO_PT);
                        strip.y = Pt((yy - DESCENT_MM * 0.85) * MM_TO_PT);
                        ops.push(Op::DrawPolygon {
                            polygon: strip.to_polygon(),
                        });
                    }
                } else if let Some((r, g, b)) = line.shot_tint {
                    let tr = 0.76 + 0.24 * (r as f32 / 255.0);
                    let tg = 0.76 + 0.24 * (g as f32 / 255.0);
                    let tb = 0.76 + 0.24 * (b as f32 / 255.0);
                    ops.push(Op::SetFillColor {
                        col: Color::Rgb(Rgb::new(tr, tg, tb, None)),
                    });
                    let mut strip = PdfRect::from_wh(
                        Pt((text_right - margin_l + 4.0) * MM_TO_PT),
                        Pt(PITCH_MM * MM_TO_PT),
                    );
                    strip.x = Pt((margin_l - 2.0) * MM_TO_PT);
                    strip.y = Pt((yy - DESCENT_MM * 0.85) * MM_TO_PT);
                    ops.push(Op::DrawPolygon {
                        polygon: strip.to_polygon(),
                    });
                }
                let bar_x = margin_l - 3.0;
                if let Some((r, g, b)) = line.shot_tint {
                    // Solid 1.5mm color bar along the left edge of the text column.
                    ops.push(Op::SetFillColor {
                        col: Color::Rgb(Rgb::new(
                            r as f32 / 255.0,
                            g as f32 / 255.0,
                            b as f32 / 255.0,
                            None,
                        )),
                    });
                    let mut bar = PdfRect::from_wh(Pt(1.5 * MM_TO_PT), Pt(PITCH_MM * MM_TO_PT));
                    bar.x = Pt(bar_x * MM_TO_PT);
                    bar.y = Pt((yy - DESCENT_MM * 0.85) * MM_TO_PT);
                    ops.push(Op::DrawPolygon {
                        polygon: bar.to_polygon(),
                    });
                }
                if let Some((ref tag, (sr, sg, sb))) = line.shot_tag {
                    // Render full descriptive shot names right-aligned inside the wide
                    // 38mm left margin beside the vertical color bar so they never collide
                    // with long screenplay lines or get cut off at the right page edge.
                    let tag_pt = 7.2_f32;
                    let tag_char_w = CHAR_MM * (tag_pt / FONT_PT);
                    let tag_left_min = 5.5_f32;
                    let tag_right = if line.share_row {
                        lines
                            .get(line_idx.saturating_sub(1))
                            .map(|prev| (prev.x_mm - 2.0).max(16.0))
                            .unwrap_or(bar_x - 1.6)
                    } else {
                        bar_x - 1.6
                    };
                    let avail_mm = (tag_right - tag_left_min).max(10.0);
                    let max_chars = (avail_mm / tag_char_w).floor().max(6.0) as usize;
                    // Allow wrapping onto 2 lines when the row below doesn't start another shot tag.
                    let next_has_tag = lines
                        .get(line_idx + 1)
                        .is_some_and(|nl| nl.shot_tag.is_some());
                    let max_lines = if next_has_tag { 1usize } else { 2usize };
                    let mut wrapped = wrap_title_line(tag, max_chars);
                    if wrapped.len() > max_lines {
                        wrapped.truncate(max_lines);
                        if let Some(last) = wrapped.last_mut() {
                            let take_n = max_chars.saturating_sub(1);
                            let mut s: String = last.chars().take(take_n).collect();
                            s.push('…');
                            *last = s;
                        }
                    }
                    ops.push(Op::SetFillColor {
                        col: Color::Rgb(Rgb::new(
                            (sr as f32 / 255.0) * 0.68,
                            (sg as f32 / 255.0) * 0.68,
                            (sb as f32 / 255.0) * 0.68,
                            None,
                        )),
                    });
                    for (ri, part) in wrapped.iter().enumerate() {
                        let w_mm = part.chars().count() as f32 * tag_char_w;
                        let tx = (tag_right - w_mm).max(tag_left_min);
                        let ty = yy + 0.35 - (ri as f32) * 3.4;
                        draw_summary_text(&mut ops, &bold, &fallback_bold, tag_pt, tx, ty, part);
                    }
                }
                ops.push(Op::SetFillColor { col: black.clone() });
            }
            if !line.shot_notes.is_empty() {
                draw_shot_note_ops(
                    &mut ops,
                    line,
                    yy,
                    text_right,
                    page_w,
                    &regular,
                    &fallback_regular,
                );
                ops.push(Op::SetFillColor { col: black.clone() });
            }
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
    if emit_legend && !legend_inserted {
        push_legend(&mut doc_pages);
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

/// Export a standalone production **Shot List** PDF grouped by scene with
/// color swatches, shot codes, size/movement/lens specs, descriptions, and
/// covered screenplay excerpts.
pub fn to_shot_list_pdf_bytes(
    p: &Project,
    meta: &Metadata,
    opts: &ExportOptions,
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
    let mut doc = PdfDocument::new(&format!("{title} - Shot List"));
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
    let spec = page_spec(opts.page_size);
    let raw_pages = build_shot_sheet_pages(
        p,
        meta,
        &title,
        spec.width_mm,
        spec.height_mm,
        &regular,
        &bold,
        &fallback_regular,
        &fallback_bold,
        false,
    );
    let pages: Vec<PdfPage> = raw_pages
        .into_iter()
        .map(|ops| PdfPage::new(Mm(spec.width_mm), Mm(spec.height_mm), ops))
        .collect();
    doc.with_pages(pages);
    let mut warnings = Vec::new();
    let bytes = doc.save(&PdfSaveOptions::default(), &mut warnings);
    Ok(bytes)
}

/// Shared paginated builder for both the **Shot Legend** (prepended to the
/// colored script PDF) and the standalone **Shot List** PDF.
#[allow(clippy::too_many_arguments)]
fn build_shot_sheet_pages(
    p: &Project,
    meta: &Metadata,
    title: &str,
    page_w: f32,
    page_h: f32,
    regular: &Face,
    bold: &Face,
    fallback_regular: &Face,
    fallback_bold: &Face,
    legend_mode: bool,
) -> Vec<Vec<Op>> {
    let margin = 14.0_f32;
    let bottom = 22.0_f32;
    let line_h = 4.9_f32;
    let body_pt = 8.2_f32;
    let char_w = CHAR_MM * (body_pt / FONT_PT);
    let black = Color::Greyscale(Greyscale::new(0.0, None));
    let muted = Color::Greyscale(Greyscale::new(0.42, None));
    let rule_col = Color::Greyscale(Greyscale::new(0.86, None));
    let heading_text = if legend_mode {
        "SHOT LEGEND"
    } else {
        "SHOT LIST"
    };

    let scenes = p.scenes();
    let locked = locked_scene_labels(meta);
    let prefix = meta.scene_number_prefix.trim();
    let start_num: u32 = meta.scene_number_start.trim().parse().unwrap_or(1);
    let scene_labels: Vec<String> = (0..scenes.len())
        .map(|i| {
            scene_label(&locked, prefix, start_num, i)
                .trim_end_matches('.')
                .to_string()
        })
        .collect();

    // Non-overlapping column X coordinates (mm) with generous gutters and safe margins
    let x_swatch = margin;
    let x_code = margin + 5.0;
    let code_chars = (30.0 / char_w).floor().max(12.0) as usize;
    let x_size = margin + 37.5;
    let x_move = margin + 50.5;
    let x_lens = margin + 72.0;
    let lens_chars = (13.5 / char_w).floor().max(6.0) as usize;
    let x_est = margin + 86.5;
    let x_desc = margin + 98.0;
    let desc_chars = ((page_w - margin - x_desc) / char_w).floor().max(24.0) as usize;
    let excerpt_char_w = CHAR_MM * (7.8 / FONT_PT);
    let excerpt_chars = ((page_w - margin - x_desc) / excerpt_char_w)
        .floor()
        .max(26.0) as usize;

    let draw_col_headers = |ops: &mut Vec<Op>, y: &mut f32| {
        ops.push(Op::SetFillColor { col: muted.clone() });
        for (x, label) in [
            (x_code, "SHOT"),
            (x_size, "SIZE"),
            (x_move, "MOVE"),
            (x_lens, "LENS"),
            (x_est, "EST"),
            (x_desc, "DESCRIPTION"),
        ] {
            draw_summary_text(ops, bold, fallback_bold, 7.6, x, *y, label);
        }
        ops.push(Op::SetOutlineColor {
            col: rule_col.clone(),
        });
        ops.push(Op::SetOutlineThickness { pt: Pt(0.5) });
        ops.push(Op::DrawLine {
            line: Line {
                points: vec![
                    LinePoint {
                        p: Point::new(Mm(margin), Mm(*y - 1.4)),
                        bezier: false,
                    },
                    LinePoint {
                        p: Point::new(Mm(page_w - margin), Mm(*y - 1.4)),
                        bezier: false,
                    },
                ],
                is_closed: false,
            },
        });
        ops.push(Op::SetFillColor { col: black.clone() });
        *y -= 5.0;
    };

    let mut pages: Vec<Vec<Op>> = Vec::new();
    let mut ops: Vec<Op> = vec![Op::SetOutlineThickness { pt: Pt(0.5) }];
    let mut y = page_h - margin;

    let flush_new_page = |pages: &mut Vec<Vec<Op>>, ops: &mut Vec<Op>, y: &mut f32| {
        pages.push(std::mem::take(ops));
        *ops = vec![Op::SetOutlineThickness { pt: Pt(0.5) }];
        *y = page_h - margin;
        ops.push(Op::SetFillColor { col: black.clone() });
        draw_summary_text(
            ops,
            bold,
            fallback_bold,
            10.5,
            margin,
            *y,
            &format!("{heading_text} (continued)"),
        );
        *y -= line_h * 1.3;
        draw_col_headers(ops, y);
    };

    // Header block
    ops.push(Op::SetFillColor { col: black.clone() });
    draw_summary_text(&mut ops, bold, fallback_bold, 15.0, margin, y, heading_text);
    y -= 6.5;
    draw_summary_text(&mut ops, regular, fallback_regular, 11.0, margin, y, title);
    y -= 5.5;
    let cov = p.shots.resolve_block_coverage(&p.blocks);
    let covered_blocks = p
        .blocks
        .iter()
        .enumerate()
        .filter(|(i, b)| {
            b.element_type != ElementType::SceneHeading
                && b.element_type.is_printable()
                && cov.get(*i).is_some_and(|v| !v.is_empty())
        })
        .count();
    let total_blocks = p
        .blocks
        .iter()
        .filter(|b| b.element_type != ElementType::SceneHeading && b.element_type.is_printable())
        .count();
    let pct = if covered_blocks == 0 || total_blocks == 0 {
        0
    } else {
        ((covered_blocks * 100 + total_blocks / 2) / total_blocks).max(1)
    };
    let proj_cuts = p.shots.project_cut_stats(&scenes, &p.blocks);
    let summary_hdr = if proj_cuts.cut_count > 0 {
        format!(
            "{} {}  |  {} {} (avg {:.1}s)  |  {} {}  |  {}% coverage",
            p.shots.len(),
            if p.shots.len() == 1 { "shot" } else { "shots" },
            proj_cuts.cut_count,
            if proj_cuts.cut_count == 1 {
                "cut"
            } else {
                "cuts"
            },
            proj_cuts.avg_cut_secs,
            scenes.len(),
            if scenes.len() == 1 { "scene" } else { "scenes" },
            pct
        )
    } else {
        format!(
            "{} {} across {} {}  |  {}% script coverage",
            p.shots.len(),
            if p.shots.len() == 1 { "shot" } else { "shots" },
            scenes.len(),
            if scenes.len() == 1 { "scene" } else { "scenes" },
            pct
        )
    };
    ops.push(Op::SetFillColor { col: muted.clone() });
    draw_summary_text(
        &mut ops,
        regular,
        fallback_regular,
        9.2,
        margin,
        y,
        &summary_hdr,
    );
    y -= 7.0;

    if p.shots.is_empty() {
        ops.push(Op::SetFillColor { col: black.clone() });
        draw_summary_text(
            &mut ops,
            regular,
            fallback_regular,
            10.0,
            margin,
            y,
            "No shots have been assigned yet.",
        );
        pages.push(ops);
        return pages;
    }

    let wrap_paragraphs = |text: &str, max: usize| -> Vec<String> {
        let mut lines = Vec::new();
        for raw in text.lines() {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            lines.extend(wrap_title_line(trimmed, max));
        }
        lines
    };

    let render_shot_row = |pages: &mut Vec<Vec<Op>>,
                           ops: &mut Vec<Op>,
                           y: &mut f32,
                           shot: &scriptcraft_core::shots::Shot,
                           draw_divider: bool| {
        let code_lines = {
            let v = wrap_paragraphs(&shot.code, code_chars);
            if v.is_empty() {
                vec!["-".to_string()]
            } else {
                v
            }
        };
        let lens_raw = if shot.lens.trim().is_empty() {
            "-"
        } else {
            shot.lens.trim()
        };
        let lens_lines = wrap_paragraphs(lens_raw, lens_chars);
        let full_covered = p.shots.covered_text_full(shot, &p.blocks);
        let note_block_index = scriptcraft_core::shots::block_index_map(&p.blocks);
        let mut note_suffix_lines: Vec<String> = Vec::new();
        for note in &shot.notes {
            let nt = note.text.trim();
            if nt.is_empty() {
                continue;
            }
            let ex = note.range.excerpt(&note_block_index, &p.blocks);
            let ex_short = if ex.chars().count() > 28 {
                let mut s: String = ex.chars().take(27).collect();
                s.push('…');
                s
            } else {
                ex
            };
            let line = if ex_short.is_empty() {
                format!("* Cue: {nt}")
            } else {
                format!("* \"{ex_short}\" - {nt}")
            };
            note_suffix_lines.extend(wrap_paragraphs(&line, desc_chars));
        }
        let (mut desc_lines, excerpt_lines) = if shot.description.trim().is_empty() {
            let src = if legend_mode {
                p.shots.covered_excerpt(shot, &p.blocks)
            } else {
                full_covered
            };
            (wrap_paragraphs(&src, desc_chars), Vec::new())
        } else {
            let d = wrap_paragraphs(shot.description.trim(), desc_chars);
            let ex = if !legend_mode && !full_covered.is_empty() {
                wrap_paragraphs(&full_covered, excerpt_chars)
            } else {
                Vec::new()
            };
            (d, ex)
        };
        desc_lines.extend(note_suffix_lines);

        // Ensure at least the first line of the row fits on the current page.
        if *y - line_h * 1.5 < bottom {
            flush_new_page(pages, ops, y);
        }

        // Color swatch box
        let (r, g, b) = shot.color.rgb();
        ops.push(Op::SetFillColor {
            col: Color::Rgb(Rgb::new(
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                None,
            )),
        });
        let mut swatch = PdfRect::from_wh(Pt(3.2 * MM_TO_PT), Pt(3.2 * MM_TO_PT));
        swatch.x = Pt(x_swatch * MM_TO_PT);
        swatch.y = Pt((*y - 0.5) * MM_TO_PT);
        ops.push(Op::DrawPolygon {
            polygon: swatch.to_polygon(),
        });

        ops.push(Op::SetFillColor { col: black.clone() });
        draw_summary_text(
            ops,
            regular,
            fallback_regular,
            body_pt,
            x_size,
            *y,
            shot.size.short(),
        );
        draw_summary_text(
            ops,
            regular,
            fallback_regular,
            body_pt,
            x_move,
            *y,
            shot.movement.label(),
        );
        let dur_s = p.shots.shot_duration_secs(shot, &p.blocks).round().max(1.0) as usize;
        ops.push(Op::SetFillColor { col: muted.clone() });
        draw_summary_text(
            ops,
            regular,
            fallback_regular,
            body_pt,
            x_est,
            *y,
            &format!("~{dur_s}s"),
        );
        ops.push(Op::SetFillColor { col: black.clone() });

        let main_rows = code_lines
            .len()
            .max(lens_lines.len())
            .max(desc_lines.len())
            .max(1);
        for ri in 0..main_rows {
            if ri > 0 {
                *y -= line_h * 0.84;
                if *y < bottom {
                    flush_new_page(pages, ops, y);
                }
                ops.push(Op::SetFillColor { col: black.clone() });
            }
            if let Some(cl) = code_lines.get(ri) {
                draw_summary_text(ops, bold, fallback_bold, body_pt, x_code, *y, cl);
            }
            if let Some(ll) = lens_lines.get(ri) {
                draw_summary_text(ops, regular, fallback_regular, body_pt, x_lens, *y, ll);
            }
            if let Some(dl) = desc_lines.get(ri) {
                draw_summary_text(ops, regular, fallback_regular, body_pt, x_desc, *y, dl);
            }
        }
        if !excerpt_lines.is_empty() {
            for el in &excerpt_lines {
                *y -= line_h * 0.80;
                if *y < bottom {
                    flush_new_page(pages, ops, y);
                }
                ops.push(Op::SetFillColor { col: muted.clone() });
                draw_summary_text(ops, regular, fallback_regular, 7.8, x_desc, *y, el);
            }
            ops.push(Op::SetFillColor { col: black.clone() });
        }
        if draw_divider {
            ops.push(Op::SetOutlineColor {
                col: rule_col.clone(),
            });
            ops.push(Op::SetOutlineThickness { pt: Pt(0.35) });
            ops.push(Op::DrawLine {
                line: Line {
                    points: vec![
                        LinePoint {
                            p: Point::new(Mm(margin), Mm(*y - 2.1)),
                            bezier: false,
                        },
                        LinePoint {
                            p: Point::new(Mm(page_w - margin), Mm(*y - 2.1)),
                            bezier: false,
                        },
                    ],
                    is_closed: false,
                },
            });
        }
        *y -= line_h * 1.15;
    };

    let draw_banner = |pages: &mut Vec<Vec<Op>>, ops: &mut Vec<Op>, y: &mut f32, text: &str| {
        let max_banner = ((page_w - 2.0 * margin - 4.0) / (CHAR_MM * (9.0 / FONT_PT)))
            .floor()
            .max(24.0) as usize;
        let banner_lines = wrap_paragraphs(text, max_banner);
        let banner_h = 2.0 + (banner_lines.len().max(1) as f32) * 4.4;
        if *y - (banner_h + line_h * 3.0) < bottom {
            pages.push(std::mem::take(ops));
            *ops = vec![Op::SetOutlineThickness { pt: Pt(0.5) }];
            *y = page_h - margin;
            ops.push(Op::SetFillColor { col: black.clone() });
            draw_summary_text(
                ops,
                bold,
                fallback_bold,
                10.5,
                margin,
                *y,
                &format!("{heading_text} (continued)"),
            );
            *y -= line_h * 1.4;
        }
        ops.push(Op::SetFillColor {
            col: Color::Greyscale(Greyscale::new(0.93, None)),
        });
        let mut banner = PdfRect::from_wh(
            Pt((page_w - 2.0 * margin) * MM_TO_PT),
            Pt(banner_h * MM_TO_PT),
        );
        banner.x = Pt(margin * MM_TO_PT);
        banner.y = Pt((*y - banner_h + 4.4) * MM_TO_PT);
        ops.push(Op::DrawPolygon {
            polygon: banner.to_polygon(),
        });
        ops.push(Op::SetFillColor { col: black.clone() });
        for (bi, bl) in banner_lines.iter().enumerate() {
            if bi > 0 {
                *y -= 4.4;
            }
            draw_summary_text(ops, bold, fallback_bold, 9.0, margin + 2.0, *y, bl);
        }
        *y -= 6.2;
        draw_col_headers(ops, y);
    };

    // Group shots by scene (plus any unassigned shots at the end).
    let mut rendered_ids = std::collections::HashSet::new();
    for (si, scene) in scenes.iter().enumerate() {
        let shot_indices = p.shots.shots_for_scene(scene, &p.blocks);
        if shot_indices.is_empty() {
            continue;
        }
        let banner_label = format!("SCENE {} - {}", scene_labels[si], scene.heading);
        draw_banner(&mut pages, &mut ops, &mut y, &banner_label);

        let last_i = shot_indices.len().saturating_sub(1);
        for (pos, idx) in shot_indices.into_iter().enumerate() {
            let shot = &p.shots.shots[idx];
            rendered_ids.insert(shot.id);
            render_shot_row(&mut pages, &mut ops, &mut y, shot, pos < last_i);
        }
        y -= 2.5;
    }

    // Any shots not linked to a scene heading
    let unassigned: Vec<&scriptcraft_core::shots::Shot> = p
        .shots
        .shots
        .iter()
        .filter(|s| !rendered_ids.contains(&s.id))
        .collect();
    if !unassigned.is_empty() {
        draw_banner(&mut pages, &mut ops, &mut y, "UNASSIGNED SHOTS");
        let last_i = unassigned.len().saturating_sub(1);
        for (pos, shot) in unassigned.into_iter().enumerate() {
            render_shot_row(&mut pages, &mut ops, &mut y, shot, pos < last_i);
        }
    }

    pages.push(ops);

    // Stamp crisp "Page X of N" footer on every Shot List / Shot Legend page
    let total_pages = pages.len();
    for (idx, page_ops) in pages.iter_mut().enumerate() {
        let label = format!("Page {} of {}", idx + 1, total_pages);
        let fx = right_aligned_x(&label, 8.5, page_w - margin);
        page_ops.push(Op::SetFillColor { col: muted.clone() });
        draw_summary_text(page_ops, regular, fallback_regular, 8.5, fx, 12.0, &label);
    }

    pages
}

/// Decoded RGB8 storyboard / reference frame image ready for embedding into a PDF.
#[derive(Clone, Debug)]
pub struct DecodedShotImage {
    pub width: usize,
    pub height: usize,
    /// Tightly packed `width * height * 3` RGB8 bytes.
    pub rgb: Vec<u8>,
}

/// Export the complete **Director's Breakdown PDF** (Phase 3):
/// 1. Title Page (when `opts.title_page` is enabled).
/// 2. Per-scene visual breakdown sheets containing the razor-sharp **vector 2D
///    overhead floor plan** and color-coded **shot cards** (with optional
///    embedded storyboard/reference frame images, lens/movement specs, duration,
///    setup notes, and covered text).
/// 3. The full **Colored Script** pages with left-margin shot labels.
#[allow(clippy::too_many_arguments)]
pub fn to_directors_breakdown_pdf_bytes(
    p: &Project,
    meta: &Metadata,
    opts: &ExportOptions,
    marks: &[Option<RevisionColor>],
    latest: Option<&RevisionSet>,
    fonts: &ExportFonts<'_>,
    shot_images: &HashMap<uuid::Uuid, DecodedShotImage>,
) -> Result<Vec<u8>, PdfError> {
    if p.blocks.is_empty() {
        return Err(PdfError::Empty);
    }
    let title = if p.title.is_empty() {
        "Untitled".to_string()
    } else {
        p.title.clone()
    };
    let mut doc = PdfDocument::new(&format!("{title} - Director's Breakdown"));
    let mut font_warnings = Vec::new();
    let mut parse = |bytes: &[u8]| ParsedFont::from_bytes(bytes, 0, &mut font_warnings);
    let regular_font = parse(fonts.regular).ok_or(PdfError::Font)?;
    let bold_font = parse(fonts.bold).ok_or(PdfError::Font)?;
    let italic_font = fonts.italic.and_then(&mut parse);
    let fallback_regular_font = parse(fonts.fallback_regular).ok_or(PdfError::Font)?;
    let fallback_bold_font = parse(fonts.fallback_bold).ok_or(PdfError::Font)?;
    let fallback_italic_font = fonts.fallback_italic.and_then(&mut parse);
    let regular = Face {
        id: doc.add_font(&regular_font),
        font: regular_font,
    };
    let bold = Face {
        id: doc.add_font(&bold_font),
        font: bold_font,
    };
    let italic = italic_font.map(|f| Face {
        id: doc.add_font(&f),
        font: f,
    });
    let fallback_regular = Face {
        id: doc.add_font(&fallback_regular_font),
        font: fallback_regular_font,
    };
    let fallback_bold = Face {
        id: doc.add_font(&fallback_bold_font),
        font: fallback_bold_font,
    };
    let fallback_italic = fallback_italic_font.map(|f| Face {
        id: doc.add_font(&f),
        font: f,
    });

    // Register attached storyboard/reference images as PDF Image XObjects.
    let mut xobjects: HashMap<uuid::Uuid, (XObjectId, usize, usize)> = HashMap::new();
    for (&shot_id, img) in shot_images {
        if img.width == 0 || img.height == 0 || img.rgb.len() != img.width * img.height * 3 {
            continue;
        }
        let raw = RawImage {
            pixels: RawImageData::U8(img.rgb.clone()),
            width: img.width,
            height: img.height,
            data_format: RawImageFormat::RGB8,
            tag: Vec::new(),
        };
        let xid = doc.add_image(&raw);
        xobjects.insert(shot_id, (xid, img.width, img.height));
    }

    let spec = page_spec(opts.page_size);
    let page_w = spec.width_mm;
    let page_h = spec.height_mm;

    let mut script_opts = opts.clone();
    script_opts.color_by_shots = true;
    script_opts.include_shot_legend = false;
    let mut script_pages: Vec<Vec<PrintLine>> = Vec::new();
    if script_opts.title_page {
        script_pages.push(layout_title_page_for(meta, latest, script_opts.page_size));
    }
    script_pages.extend(layout_pages_inner(
        p,
        meta,
        &script_opts,
        marks,
        latest,
        true,
    ));

    let mut doc_pages: Vec<PdfPage> = Vec::new();
    let has_title = script_opts.title_page && !script_pages.is_empty();
    let watermark = script_opts
        .watermark
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    // 1. Title Page (if enabled)
    if has_title {
        let ops = render_script_page_ops(
            &script_pages[0],
            spec,
            watermark,
            &regular,
            &bold,
            italic.as_ref(),
            &fallback_regular,
            &fallback_bold,
            fallback_italic.as_ref(),
        );
        doc_pages.push(PdfPage::new(Mm(page_w), Mm(page_h), ops));
    }

    // 2. Per-scene Visual Breakdown sheets (Vector Floor Plan + Shot Cards + Storyboards)
    let breakdown_pages = build_directors_breakdown_pages(
        p,
        meta,
        &title,
        page_w,
        page_h,
        &regular,
        &bold,
        &fallback_regular,
        &fallback_bold,
        &xobjects,
    );
    for ops in breakdown_pages {
        doc_pages.push(PdfPage::new(Mm(page_w), Mm(page_h), ops));
    }

    // 3. Full Colored Script pages
    let script_start = usize::from(has_title);
    for lines in script_pages.iter().skip(script_start) {
        let ops = render_script_page_ops(
            lines,
            spec,
            watermark,
            &regular,
            &bold,
            italic.as_ref(),
            &fallback_regular,
            &fallback_bold,
            fallback_italic.as_ref(),
        );
        doc_pages.push(PdfPage::new(Mm(page_w), Mm(page_h), ops));
    }

    doc.with_pages(doc_pages);
    let mut warnings = Vec::new();
    let bytes = doc.save(&PdfSaveOptions::default(), &mut warnings);
    Ok(bytes)
}

#[allow(clippy::too_many_arguments)]
fn render_script_page_ops(
    lines: &[PrintLine],
    spec: crate::place::PageSpec,
    watermark: Option<&str>,
    regular: &Face,
    bold: &Face,
    italic: Option<&Face>,
    fallback_regular: &Face,
    fallback_bold: &Face,
    fallback_italic: Option<&Face>,
) -> Vec<Op> {
    let page_w = spec.width_mm;
    let page_h = spec.height_mm;
    let margin_l = spec.margin_l_mm;
    let margin_t = spec.margin_t_mm;
    let text_right = spec.text_right_mm();
    let page_num_baseline = margin_t - 0.4 * PITCH_MM - DESCENT_MM;
    let italic_face = italic.unwrap_or(regular);
    let fallback_italic_face = fallback_italic.unwrap_or(fallback_regular);
    let black = Color::Greyscale(Greyscale::new(0.0, None));
    let show = push_text;

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
    for (line_idx, line) in lines.iter().enumerate() {
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
                (bold, fallback_bold)
            } else {
                (regular, fallback_regular)
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
                (bold, fallback_bold)
            } else {
                (regular, fallback_regular)
            };
            for (use_fb, run) in split_runs(&line.text, &face.font, &fb_face.font) {
                let id = if use_fb { &fb_face.id } else { &face.id };
                show(&mut ops, id, FONT_PT, cx, page_h - page_num_baseline, run);
                cx += run.chars().count() as f32 * CHAR_MM;
            }
            continue;
        }
        let yy = line.y_from_bottom_mm.unwrap_or(y - line.dy_mm);
        if !line.shot_highlights.is_empty() || line.shot_tint.is_some() {
            let base_x = if line.right_aligned {
                line.x_mm - line.text.chars().count() as f32 * CHAR_MM
            } else {
                line.x_mm
            };
            if !line.shot_highlights.is_empty() {
                for &(c0, c1, (r, g, b)) in &line.shot_highlights {
                    let tr = 0.74 + 0.26 * (r as f32 / 255.0);
                    let tg = 0.74 + 0.26 * (g as f32 / 255.0);
                    let tb = 0.74 + 0.26 * (b as f32 / 255.0);
                    ops.push(Op::SetFillColor {
                        col: Color::Rgb(Rgb::new(tr, tg, tb, None)),
                    });
                    let w_mm = ((c1 - c0) as f32 * CHAR_MM) + 1.0;
                    let mut strip = PdfRect::from_wh(Pt(w_mm * MM_TO_PT), Pt(PITCH_MM * MM_TO_PT));
                    strip.x = Pt((base_x + c0 as f32 * CHAR_MM - 0.5) * MM_TO_PT);
                    strip.y = Pt((yy - DESCENT_MM * 0.85) * MM_TO_PT);
                    ops.push(Op::DrawPolygon {
                        polygon: strip.to_polygon(),
                    });
                }
            } else if let Some((r, g, b)) = line.shot_tint {
                let tr = 0.76 + 0.24 * (r as f32 / 255.0);
                let tg = 0.76 + 0.24 * (g as f32 / 255.0);
                let tb = 0.76 + 0.24 * (b as f32 / 255.0);
                ops.push(Op::SetFillColor {
                    col: Color::Rgb(Rgb::new(tr, tg, tb, None)),
                });
                let mut strip = PdfRect::from_wh(
                    Pt((text_right - margin_l + 4.0) * MM_TO_PT),
                    Pt(PITCH_MM * MM_TO_PT),
                );
                strip.x = Pt((margin_l - 2.0) * MM_TO_PT);
                strip.y = Pt((yy - DESCENT_MM * 0.85) * MM_TO_PT);
                ops.push(Op::DrawPolygon {
                    polygon: strip.to_polygon(),
                });
            }
            let bar_x = margin_l - 3.0;
            if let Some((r, g, b)) = line.shot_tint {
                ops.push(Op::SetFillColor {
                    col: Color::Rgb(Rgb::new(
                        r as f32 / 255.0,
                        g as f32 / 255.0,
                        b as f32 / 255.0,
                        None,
                    )),
                });
                let mut bar = PdfRect::from_wh(Pt(1.5 * MM_TO_PT), Pt(PITCH_MM * MM_TO_PT));
                bar.x = Pt(bar_x * MM_TO_PT);
                bar.y = Pt((yy - DESCENT_MM * 0.85) * MM_TO_PT);
                ops.push(Op::DrawPolygon {
                    polygon: bar.to_polygon(),
                });
            }
            if let Some((ref tag, (sr, sg, sb))) = line.shot_tag {
                let tag_pt = 7.2_f32;
                let tag_char_w = CHAR_MM * (tag_pt / FONT_PT);
                let tag_left_min = 5.5_f32;
                let tag_right = if line.share_row {
                    lines
                        .get(line_idx.saturating_sub(1))
                        .map(|prev| (prev.x_mm - 2.0).max(16.0))
                        .unwrap_or(bar_x - 1.6)
                } else {
                    bar_x - 1.6
                };
                let avail_mm = (tag_right - tag_left_min).max(10.0);
                let max_chars = (avail_mm / tag_char_w).floor().max(6.0) as usize;
                let next_has_tag = lines
                    .get(line_idx + 1)
                    .is_some_and(|nl| nl.shot_tag.is_some());
                let max_lines = if next_has_tag { 1usize } else { 2usize };
                let mut wrapped = wrap_title_line(tag, max_chars);
                if wrapped.len() > max_lines {
                    wrapped.truncate(max_lines);
                    if let Some(last) = wrapped.last_mut() {
                        let take_n = max_chars.saturating_sub(1);
                        let mut s: String = last.chars().take(take_n).collect();
                        s.push('…');
                        *last = s;
                    }
                }
                ops.push(Op::SetFillColor {
                    col: Color::Rgb(Rgb::new(
                        (sr as f32 / 255.0) * 0.68,
                        (sg as f32 / 255.0) * 0.68,
                        (sb as f32 / 255.0) * 0.68,
                        None,
                    )),
                });
                for (ri, part) in wrapped.iter().enumerate() {
                    let w_mm = part.chars().count() as f32 * tag_char_w;
                    let tx = (tag_right - w_mm).max(tag_left_min);
                    let ty = yy + 0.35 - (ri as f32) * 3.4;
                    draw_summary_text(&mut ops, bold, fallback_bold, tag_pt, tx, ty, part);
                }
            }
            ops.push(Op::SetFillColor { col: black.clone() });
        }
        if !line.shot_notes.is_empty() {
            draw_shot_note_ops(
                &mut ops,
                line,
                yy,
                text_right,
                page_w,
                regular,
                fallback_regular,
            );
            ops.push(Op::SetFillColor { col: black.clone() });
        }
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
                    (bold, fallback_bold)
                } else {
                    (regular, fallback_regular)
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
                        (bold, fallback_bold)
                    } else {
                        (regular, fallback_regular)
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
    ops
}

fn push_pdf_polygon(ops: &mut Vec<Op>, pts: &[(f32, f32)], mode: PaintMode) {
    if pts.len() < 3 {
        return;
    }
    let points: Vec<LinePoint> = pts
        .iter()
        .map(|&(x, y)| LinePoint {
            p: Point::new(Mm(x), Mm(y)),
            bezier: false,
        })
        .collect();
    ops.push(Op::DrawPolygon {
        polygon: Polygon {
            rings: vec![PolygonRing { points }],
            mode,
            winding_order: WindingOrder::NonZero,
        },
    });
}

fn push_pdf_polyline(ops: &mut Vec<Op>, pts: &[(f32, f32)], closed: bool) {
    if pts.len() < 2 {
        return;
    }
    let points: Vec<LinePoint> = pts
        .iter()
        .map(|&(x, y)| LinePoint {
            p: Point::new(Mm(x), Mm(y)),
            bezier: false,
        })
        .collect();
    ops.push(Op::DrawLine {
        line: Line {
            points,
            is_closed: closed,
        },
    });
}

fn push_pdf_dashed_line(
    ops: &mut Vec<Op>,
    a: (f32, f32),
    b: (f32, f32),
    dash_mm: f32,
    gap_mm: f32,
) {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len = dx.hypot(dy);
    if len < 0.2 {
        return;
    }
    let ux = dx / len;
    let uy = dy / len;
    let step = dash_mm + gap_mm;
    let mut t = 0.0_f32;
    while t < len {
        let t1 = (t + dash_mm).min(len);
        push_pdf_polyline(
            ops,
            &[(a.0 + ux * t, a.1 + uy * t), (a.0 + ux * t1, a.1 + uy * t1)],
            false,
        );
        t += step;
    }
}

fn pdf_rot2(x: f32, y: f32, rad: f32) -> (f32, f32) {
    let (s, c) = rad.sin_cos();
    (x * c - y * s, x * s + y * c)
}

fn push_pdf_rounded_rect(
    ops: &mut Vec<Op>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
    mode: PaintMode,
) {
    let r = r.min(w * 0.5).min(h * 0.5).max(0.0);
    if r < 0.05 {
        push_pdf_polygon(ops, &[(x, y), (x + w, y), (x + w, y + h), (x, y + h)], mode);
        return;
    }
    let segs = 4usize;
    let corners = [
        (x + w - r, y + r, -std::f32::consts::FRAC_PI_2, 0.0),
        (x + w - r, y + h - r, 0.0, std::f32::consts::FRAC_PI_2),
        (
            x + r,
            y + h - r,
            std::f32::consts::FRAC_PI_2,
            std::f32::consts::PI,
        ),
        (
            x + r,
            y + r,
            std::f32::consts::PI,
            std::f32::consts::PI * 1.5,
        ),
    ];
    let mut pts = Vec::with_capacity(corners.len() * (segs + 1));
    for (cx, cy, a0, a1) in corners {
        for k in 0..=segs {
            let a = a0 + (a1 - a0) * (k as f32 / segs as f32);
            pts.push((cx + r * a.cos(), cy + r * a.sin()));
        }
    }
    push_pdf_polygon(ops, &pts, mode);
}

fn push_pdf_arrow_head(ops: &mut Vec<Op>, tip: (f32, f32), dir: (f32, f32), size_mm: f32) {
    let len = dir.0.hypot(dir.1);
    if len < 1e-4 {
        return;
    }
    let ux = dir.0 / len;
    let uy = dir.1 / len;
    let px = -uy;
    let py = ux;
    let back = (tip.0 - ux * size_mm, tip.1 - uy * size_mm);
    let half = size_mm * 0.52;
    push_pdf_polygon(
        ops,
        &[
            tip,
            (back.0 + px * half, back.1 + py * half),
            (back.0 - px * half, back.1 - py * half),
        ],
        PaintMode::Fill,
    );
}

/// Exact Sutherland-Hodgman polygon clip against `[min_x, max_x] x [min_y, max_y]`.
fn clip_polygon_to_rect(
    poly: &[(f32, f32)],
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
) -> Vec<(f32, f32)> {
    fn clip_edge<F, G>(pts: &[(f32, f32)], inside: F, intersect: G) -> Vec<(f32, f32)>
    where
        F: Fn((f32, f32)) -> bool,
        G: Fn((f32, f32), (f32, f32)) -> (f32, f32),
    {
        if pts.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(pts.len() + 2);
        let mut s = *pts.last().unwrap();
        let mut s_in = inside(s);
        for &e in pts {
            let e_in = inside(e);
            if e_in {
                if !s_in {
                    out.push(intersect(s, e));
                }
                out.push(e);
            } else if s_in {
                out.push(intersect(s, e));
            }
            s = e;
            s_in = e_in;
        }
        out
    }

    let p = clip_edge(
        poly,
        |pt| pt.0 >= min_x,
        |a, b| {
            let t = ((min_x - a.0) / (b.0 - a.0).max(1e-6_f32.copysign(b.0 - a.0))).clamp(0.0, 1.0);
            (min_x, a.1 + t * (b.1 - a.1))
        },
    );
    let p = clip_edge(
        &p,
        |pt| pt.0 <= max_x,
        |a, b| {
            let denom = b.0 - a.0;
            let t = if denom.abs() < 1e-6 {
                0.0
            } else {
                ((max_x - a.0) / denom).clamp(0.0, 1.0)
            };
            (max_x, a.1 + t * (b.1 - a.1))
        },
    );
    let p = clip_edge(
        &p,
        |pt| pt.1 >= min_y,
        |a, b| {
            let denom = b.1 - a.1;
            let t = if denom.abs() < 1e-6 {
                0.0
            } else {
                ((min_y - a.1) / denom).clamp(0.0, 1.0)
            };
            (a.0 + t * (b.0 - a.0), min_y)
        },
    );
    clip_edge(
        &p,
        |pt| pt.1 <= max_y,
        |a, b| {
            let denom = b.1 - a.1;
            let t = if denom.abs() < 1e-6 {
                0.0
            } else {
                ((max_y - a.1) / denom).clamp(0.0, 1.0)
            };
            (a.0 + t * (b.0 - a.0), max_y)
        },
    )
}

/// Exact Liang-Barsky segment clip against `[min_x, max_x] x [min_y, max_y]`.
fn clip_segment_to_rect(
    a: (f32, f32),
    b: (f32, f32),
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
) -> Option<((f32, f32), (f32, f32))> {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let mut t0 = 0.0_f32;
    let mut t1 = 1.0_f32;
    for (p, q) in [
        (-dx, a.0 - min_x),
        (dx, max_x - a.0),
        (-dy, a.1 - min_y),
        (dy, max_y - a.1),
    ] {
        if p.abs() < 1e-6 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                if r > t1 {
                    return None;
                }
                if r > t0 {
                    t0 = r;
                }
            } else {
                if r < t0 {
                    return None;
                }
                if r < t1 {
                    t1 = r;
                }
            }
        }
    }
    Some((
        (a.0 + t0 * dx, a.1 + t0 * dy),
        (a.0 + t1 * dx, a.1 + t1 * dy),
    ))
}

fn rect_overlap_area(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> f32 {
    let ox = (a.2.min(b.2) - a.0.max(b.0)).max(0.0);
    let oy = (a.3.min(b.3) - a.1.max(b.1)).max(0.0);
    ox * oy
}

fn ink_contrast_on_rgb(r: u8, g: u8, b: u8) -> Color {
    let lum = 0.299 * (r as f32) + 0.587 * (g as f32) + 0.114 * (b as f32);
    if lum > 170.0 {
        Color::Greyscale(Greyscale::new(0.10, None))
    } else {
        Color::Greyscale(Greyscale::new(1.0, None))
    }
}

/// Tight world bounding box `(min_x, max_x, min_y, max_y)` for a scene's 2D
/// floor plan, keeping room architecture large and crisp without letting long
/// FOV beams shrink the room down.
fn floor_plan_world_bounds(plan: &scriptcraft_core::shots::SceneFloorPlan) -> (f32, f32, f32, f32) {
    use scriptcraft_core::shots::PropKind;
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
    for prop in &plan.props {
        let rad = prop.angle_deg.to_radians();
        let hw = prop.w * 0.5;
        let hh = prop.h * 0.5;
        for (lx, ly) in [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)] {
            let (rx, ry) = pdf_rot2(lx, ly, rad);
            expand(prop.x + rx, prop.y + ry, 0.25);
        }
        if prop.kind == PropKind::Door {
            let swing_rad = rad - 65.0_f32.to_radians();
            expand(
                prop.x - hw * rad.cos() + prop.w * swing_rad.cos(),
                prop.y - hw * rad.sin() + prop.w * swing_rad.sin(),
                0.22,
            );
        }
    }
    for actor in &plan.actors {
        expand(actor.x, actor.y, 0.72);
        if let Some((mx, my)) = actor.move_to {
            expand(mx, my, 0.45);
        }
    }
    for cam in &plan.cameras {
        let cone = cam.effective_cone_length().min(3.0);
        let rad = cam.angle_deg.to_radians();
        expand(cam.x, cam.y, 0.62);
        expand(
            cam.x + rad.cos() * cone * 0.45,
            cam.y + rad.sin() * cone * 0.45,
            0.35,
        );
        if let Some((tx, ty)) = cam.track_to {
            expand(tx, ty, 0.50);
        }
    }
    if !min_x.is_finite() {
        (-3.5, 3.5, -2.5, 2.5)
    } else {
        (min_x, max_x, min_y, max_y)
    }
}

/// Draw a razor-sharp vector 2D overhead floor plan into `(box_x, box_y, box_w, box_h)` mm
/// (where `box_y` is the bottom edge in PDF coordinates).
///
/// Cameras display crisp centered index numbers from `shot_numbers` (paired
/// with a dedicated Camera Key legend beside/below the diagram) so even dense
/// multi-camera setups around one table stay completely legible with zero
/// overlapping text.
#[allow(clippy::too_many_arguments)]
fn draw_floor_plan_vector(
    ops: &mut Vec<Op>,
    p: &Project,
    plan: &scriptcraft_core::shots::SceneFloorPlan,
    shot_numbers: &HashMap<uuid::Uuid, usize>,
    box_x: f32,
    box_y: f32,
    box_w: f32,
    box_h: f32,
    regular: &Face,
    bold: &Face,
    fallback_regular: &Face,
    fallback_bold: &Face,
) {
    use scriptcraft_core::shots::{lens_to_fov_deg, PropKind};

    let inner_min_x = box_x + 0.5;
    let inner_min_y = box_y + 0.5;
    let inner_max_x = box_x + box_w - 0.5;
    let inner_max_y = box_y + box_h - 0.5;

    // Frame background
    ops.push(Op::SetFillColor {
        col: Color::Greyscale(Greyscale::new(0.988, None)),
    });
    push_pdf_polygon(
        ops,
        &[
            (box_x, box_y),
            (box_x + box_w, box_y),
            (box_x + box_w, box_y + box_h),
            (box_x, box_y + box_h),
        ],
        PaintMode::Fill,
    );

    let (min_x, max_x, min_y, max_y) = floor_plan_world_bounds(plan);
    let span_x = (max_x - min_x).max(3.2);
    let span_y = (max_y - min_y).max(2.6);
    let pad_mm = 4.5_f32;
    let scale = ((box_w - 2.0 * pad_mm) / span_x)
        .min((box_h - 2.0 * pad_mm) / span_y)
        .clamp(4.0, 28.0);
    let plan_cx = (min_x + max_x) * 0.5;
    let plan_cy = (min_y + max_y) * 0.5;
    let box_cx = box_x + box_w * 0.5;
    let box_cy = box_y + box_h * 0.5;

    // World (+Y down on screen) -> PDF (+Y up)
    let to_mm = |wx: f32, wy: f32| -> (f32, f32) {
        (
            box_cx + (wx - plan_cx) * scale,
            box_cy - (wy - plan_cy) * scale,
        )
    };

    // Precompute camera FOV geometry for two-pass rendering (all soft fills
    // first, then grid, then all crisp FOV boundary rays so no cone fill ever
    // erases another camera's FOV edges).
    struct ConeGeom {
        cp: (f32, f32),
        arc: Vec<(f32, f32)>,
        col: Color,
        tint: Color,
        arc_col: Color,
    }
    let mut cones: Vec<ConeGeom> = Vec::with_capacity(plan.cameras.len());
    for cam in &plan.cameras {
        let Some(shot) = p.shots.get(cam.shot_id) else {
            continue;
        };
        let cp = to_mm(cam.x, cam.y);
        let rad = cam.angle_deg.to_radians();
        let (r, g, b) = shot.color.rgb();
        let rf = r as f32 / 255.0;
        let gf = g as f32 / 255.0;
        let bf = b as f32 / 255.0;
        let col = Color::Rgb(Rgb::new(rf, gf, bf, None));
        let tint = Color::Rgb(Rgb::new(
            0.915 + 0.085 * rf,
            0.915 + 0.085 * gf,
            0.915 + 0.085 * bf,
            None,
        ));
        let arc_col = Color::Rgb(Rgb::new(
            0.55 + 0.45 * rf,
            0.55 + 0.45 * gf,
            0.55 + 0.45 * bf,
            None,
        ));
        let fov_deg = cam.fov_deg.unwrap_or_else(|| lens_to_fov_deg(&shot.lens));
        let half_fov = (fov_deg * 0.5).to_radians();
        let cone_m = cam.effective_cone_length();
        let steps = 20usize;
        let mut arc = Vec::with_capacity(steps + 1);
        for k in 0..=steps {
            let a = rad - half_fov + (2.0 * half_fov) * (k as f32 / steps as f32);
            arc.push(to_mm(cam.x + a.cos() * cone_m, cam.y + a.sin() * cone_m));
        }
        cones.push(ConeGeom {
            cp,
            arc,
            col,
            tint,
            arc_col,
        });
    }

    // Layer 0a: Airy pastel FOV wedge fills (clipped cleanly to diagram frame)
    for cone in &cones {
        let mut poly = Vec::with_capacity(cone.arc.len() + 1);
        poly.push(cone.cp);
        poly.extend_from_slice(&cone.arc);
        let clipped =
            clip_polygon_to_rect(&poly, inner_min_x, inner_min_y, inner_max_x, inner_max_y);
        if clipped.len() >= 3 {
            ops.push(Op::SetFillColor {
                col: cone.tint.clone(),
            });
            push_pdf_polygon(ops, &clipped, PaintMode::Fill);
        }
    }

    // Layer 0b: Subtle 1m architectural blueprint grid (painted lightly over
    // pastel fills so the scale grid stays visible across the whole floor plan)
    ops.push(Op::SetOutlineColor {
        col: Color::Greyscale(Greyscale::new(0.895, None)),
    });
    ops.push(Op::SetOutlineThickness { pt: Pt(0.28) });
    let half_w_m = (box_w * 0.5) / scale;
    let half_h_m = (box_h * 0.5) / scale;
    let mut gx = (plan_cx - half_w_m).ceil();
    while gx <= plan_cx + half_w_m {
        let sx = box_cx + (gx - plan_cx) * scale;
        if sx > inner_min_x + 0.5 && sx < inner_max_x - 0.5 {
            push_pdf_polyline(ops, &[(sx, inner_min_y), (sx, inner_max_y)], false);
        }
        gx += 1.0;
    }
    let mut gy = (plan_cy - half_h_m).ceil();
    while gy <= plan_cy + half_h_m {
        let sy = box_cy - (gy - plan_cy) * scale;
        if sy > inner_min_y + 0.5 && sy < inner_max_y - 0.5 {
            push_pdf_polyline(ops, &[(inner_min_x, sy), (inner_max_x, sy)], false);
        }
        gy += 1.0;
    }

    // Precompute camera centers in mm so prop labels know if a camera sits on top
    let cam_positions_mm: Vec<(f32, f32)> = plan
        .cameras
        .iter()
        .filter(|c| p.shots.get(c.shot_id).is_some())
        .map(|c| to_mm(c.x, c.y))
        .collect();

    // Obstacle boxes in mm for smart collision-free actor name pill placement:
    // (min_x, min_y, max_x, max_y, penalty_weight)
    let mut obstacles: Vec<(f32, f32, f32, f32, f32)> = Vec::new();

    // Layer 1a: Furniture & floor props (tables, chairs, shapes, lights) first,
    // then FOV sightlines on top of furniture so camera framing over tables is
    // crystal clear, then Layer 1c: architectural walls/doors/windows.
    let is_arch = |k: PropKind| matches!(k, PropKind::Wall | PropKind::Door | PropKind::Window);
    for arch_pass in [false, true] {
        if arch_pass {
            // Layer 1b: Crisp FOV boundary rays + subtle outer arcs (drawn OVER
            // tables/furniture so every camera's exact framing stays visible)
            for cone in &cones {
                if cone.arc.len() < 2 {
                    continue;
                }
                ops.push(Op::SetOutlineColor {
                    col: cone.arc_col.clone(),
                });
                ops.push(Op::SetOutlineThickness { pt: Pt(0.32) });
                for seg in cone.arc.windows(2) {
                    if let Some((a, b)) = clip_segment_to_rect(
                        seg[0],
                        seg[1],
                        inner_min_x,
                        inner_min_y,
                        inner_max_x,
                        inner_max_y,
                    ) {
                        push_pdf_polyline(ops, &[a, b], false);
                    }
                }
                ops.push(Op::SetOutlineColor {
                    col: cone.col.clone(),
                });
                ops.push(Op::SetOutlineThickness { pt: Pt(0.55) });
                for &edge_pt in &[cone.arc[0], *cone.arc.last().unwrap()] {
                    if let Some((a, b)) = clip_segment_to_rect(
                        cone.cp,
                        edge_pt,
                        inner_min_x,
                        inner_min_y,
                        inner_max_x,
                        inner_max_y,
                    ) {
                        push_pdf_polyline(ops, &[a, b], false);
                    }
                }
            }

            // 180-degree Line of Action between first two actors (clipped to frame)
            if plan.actors.len() >= 2 {
                let a = to_mm(plan.actors[0].x, plan.actors[0].y);
                let b = to_mm(plan.actors[1].x, plan.actors[1].y);
                let dx = b.0 - a.0;
                let dy = b.1 - a.1;
                let len = dx.hypot(dy);
                if len > 2.0 {
                    let ux = dx / len;
                    let uy = dy / len;
                    let ext = box_w.hypot(box_h);
                    let p0 = (a.0 - ux * ext, a.1 - uy * ext);
                    let p1 = (b.0 + ux * ext, b.1 + uy * ext);
                    if let Some((c0, c1)) = clip_segment_to_rect(
                        p0,
                        p1,
                        inner_min_x + 1.0,
                        inner_min_y + 1.0,
                        inner_max_x - 1.0,
                        inner_max_y - 1.0,
                    ) {
                        ops.push(Op::SetOutlineColor {
                            col: Color::Rgb(Rgb::new(0.86, 0.34, 0.34, None)),
                        });
                        ops.push(Op::SetOutlineThickness { pt: Pt(0.48) });
                        push_pdf_dashed_line(ops, c0, c1, 2.4, 1.8);
                    }
                }
            }
        }

        for prop in &plan.props {
            if is_arch(prop.kind) != arch_pass {
                continue;
            }
            let rad = prop.angle_deg.to_radians();
            let hw = prop.w * 0.5;
            let hh = prop.h * 0.5;
            let pt = |lx: f32, ly: f32| -> (f32, f32) {
                let (rx, ry) = pdf_rot2(lx, ly, rad);
                to_mm(prop.x + rx, prop.y + ry)
            };
            let corners = [pt(-hw, -hh), pt(hw, -hh), pt(hw, hh), pt(-hw, hh)];
            let bx0 = corners.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
            let bx1 = corners
                .iter()
                .map(|p| p.0)
                .fold(f32::NEG_INFINITY, f32::max);
            let by0 = corners.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
            let by1 = corners
                .iter()
                .map(|p| p.1)
                .fold(f32::NEG_INFINITY, f32::max);
            let obs_weight = if prop.kind == PropKind::Wall {
                6.5
            } else {
                4.2
            };
            obstacles.push((bx0, by0, bx1, by1, obs_weight));

            ops.push(Op::SetOutlineColor {
                col: Color::Greyscale(Greyscale::new(0.36, None)),
            });
            ops.push(Op::SetOutlineThickness { pt: Pt(0.65) });
            match prop.kind {
                PropKind::Wall => {
                    ops.push(Op::SetFillColor {
                        col: Color::Greyscale(Greyscale::new(0.38, None)),
                    });
                    ops.push(Op::SetOutlineColor {
                        col: Color::Greyscale(Greyscale::new(0.28, None)),
                    });
                    ops.push(Op::SetOutlineThickness { pt: Pt(0.45) });
                    let clipped = clip_polygon_to_rect(
                        &corners,
                        inner_min_x,
                        inner_min_y,
                        inner_max_x,
                        inner_max_y,
                    );
                    if clipped.len() >= 3 {
                        push_pdf_polygon(ops, &clipped, PaintMode::FillStroke);
                    }
                }
                PropKind::Window => {
                    ops.push(Op::SetFillColor {
                        col: Color::Rgb(Rgb::new(0.84, 0.93, 0.99, None)),
                    });
                    let clipped = clip_polygon_to_rect(
                        &corners,
                        inner_min_x,
                        inner_min_y,
                        inner_max_x,
                        inner_max_y,
                    );
                    if clipped.len() >= 3 {
                        push_pdf_polygon(ops, &clipped, PaintMode::FillStroke);
                    }
                    ops.push(Op::SetOutlineColor {
                        col: Color::Rgb(Rgb::new(0.28, 0.62, 0.90, None)),
                    });
                    if let Some((a, b)) = clip_segment_to_rect(
                        pt(-hw, 0.0),
                        pt(hw, 0.0),
                        inner_min_x,
                        inner_min_y,
                        inner_max_x,
                        inner_max_y,
                    ) {
                        push_pdf_polyline(ops, &[a, b], false);
                    }
                }
                PropKind::Door => {
                    for (a, b) in [(pt(-hw, -hh), pt(-hw, hh)), (pt(hw, -hh), pt(hw, hh))] {
                        if let Some((ca, cb)) = clip_segment_to_rect(
                            a,
                            b,
                            inner_min_x,
                            inner_min_y,
                            inner_max_x,
                            inner_max_y,
                        ) {
                            push_pdf_polyline(ops, &[ca, cb], false);
                        }
                    }
                    let hinge = pt(-hw, 0.0);
                    let swing_rad = rad - 65.0_f32.to_radians();
                    let leaf_end = to_mm(
                        prop.x - hw * rad.cos() + prop.w * swing_rad.cos(),
                        prop.y - hw * rad.sin() + prop.w * swing_rad.sin(),
                    );
                    ops.push(Op::SetOutlineThickness { pt: Pt(1.0) });
                    if let Some((ca, cb)) = clip_segment_to_rect(
                        hinge,
                        leaf_end,
                        inner_min_x,
                        inner_min_y,
                        inner_max_x,
                        inner_max_y,
                    ) {
                        push_pdf_polyline(ops, &[ca, cb], false);
                    }
                    ops.push(Op::SetOutlineThickness { pt: Pt(0.42) });
                    let segs = 20usize;
                    let mut prev = to_mm(
                        prop.x - hw * rad.cos() + prop.w * rad.cos(),
                        prop.y - hw * rad.sin() + prop.w * rad.sin(),
                    );
                    for k in 1..=segs {
                        let a = rad - (65.0_f32 * k as f32 / segs as f32).to_radians();
                        let cur = to_mm(
                            prop.x - hw * rad.cos() + prop.w * a.cos(),
                            prop.y - hw * rad.sin() + prop.w * a.sin(),
                        );
                        if let Some((ca, cb)) = clip_segment_to_rect(
                            prev,
                            cur,
                            inner_min_x,
                            inner_min_y,
                            inner_max_x,
                            inner_max_y,
                        ) {
                            push_pdf_polyline(ops, &[ca, cb], false);
                        }
                        prev = cur;
                    }
                }
                PropKind::Circle => {
                    ops.push(Op::SetFillColor {
                        col: Color::Greyscale(Greyscale::new(0.94, None)),
                    });
                    let segs = 48usize;
                    let pts: Vec<(f32, f32)> = (0..segs)
                        .map(|k| {
                            let a = std::f32::consts::TAU * (k as f32 / segs as f32);
                            pt(hw * a.cos(), hh * a.sin())
                        })
                        .collect();
                    let clipped = clip_polygon_to_rect(
                        &pts,
                        inner_min_x,
                        inner_min_y,
                        inner_max_x,
                        inner_max_y,
                    );
                    if clipped.len() >= 3 {
                        push_pdf_polygon(ops, &clipped, PaintMode::FillStroke);
                    }
                }
                PropKind::Triangle => {
                    ops.push(Op::SetFillColor {
                        col: Color::Greyscale(Greyscale::new(0.94, None)),
                    });
                    let clipped = clip_polygon_to_rect(
                        &[pt(0.0, -hh), pt(hw, hh), pt(-hw, hh)],
                        inner_min_x,
                        inner_min_y,
                        inner_max_x,
                        inner_max_y,
                    );
                    if clipped.len() >= 3 {
                        push_pdf_polygon(ops, &clipped, PaintMode::FillStroke);
                    }
                }
                PropKind::Light => {
                    ops.push(Op::SetFillColor {
                        col: Color::Rgb(Rgb::new(0.99, 0.90, 0.52, None)),
                    });
                    ops.push(Op::SetOutlineColor {
                        col: Color::Rgb(Rgb::new(0.76, 0.52, 0.08, None)),
                    });
                    let r = hw.min(hh).max(0.18);
                    let pts: Vec<(f32, f32)> = (0..24)
                        .map(|k| {
                            let a = std::f32::consts::TAU * (k as f32 / 24.0);
                            pt(r * a.cos(), r * a.sin())
                        })
                        .collect();
                    let clipped = clip_polygon_to_rect(
                        &pts,
                        inner_min_x,
                        inner_min_y,
                        inner_max_x,
                        inner_max_y,
                    );
                    if clipped.len() >= 3 {
                        push_pdf_polygon(ops, &clipped, PaintMode::FillStroke);
                    }
                    for ang in [-28.0_f32, 0.0, 28.0] {
                        let ar = ang.to_radians();
                        if let Some((ca, cb)) = clip_segment_to_rect(
                            pt(r * ar.cos(), r * ar.sin()),
                            pt((r + 0.35) * ar.cos(), (r + 0.35) * ar.sin()),
                            inner_min_x,
                            inner_min_y,
                            inner_max_x,
                            inner_max_y,
                        ) {
                            push_pdf_polyline(ops, &[ca, cb], false);
                        }
                    }
                }
                PropKind::Car => {
                    ops.push(Op::SetFillColor {
                        col: Color::Greyscale(Greyscale::new(0.92, None)),
                    });
                    let clipped = clip_polygon_to_rect(
                        &corners,
                        inner_min_x,
                        inner_min_y,
                        inner_max_x,
                        inner_max_y,
                    );
                    if clipped.len() >= 3 {
                        push_pdf_polygon(ops, &clipped, PaintMode::FillStroke);
                    }
                    ops.push(Op::SetFillColor {
                        col: Color::Rgb(Rgb::new(0.84, 0.93, 0.99, None)),
                    });
                    let ws = clip_polygon_to_rect(
                        &[
                            pt(hw * 0.35, -hh * 0.76),
                            pt(hw * 0.58, -hh * 0.66),
                            pt(hw * 0.58, hh * 0.66),
                            pt(hw * 0.35, hh * 0.76),
                        ],
                        inner_min_x,
                        inner_min_y,
                        inner_max_x,
                        inner_max_y,
                    );
                    if ws.len() >= 3 {
                        push_pdf_polygon(ops, &ws, PaintMode::FillStroke);
                    }
                }
                PropKind::Chair => {
                    ops.push(Op::SetFillColor {
                        col: Color::Greyscale(Greyscale::new(0.95, None)),
                    });
                    let clipped = clip_polygon_to_rect(
                        &corners,
                        inner_min_x,
                        inner_min_y,
                        inner_max_x,
                        inner_max_y,
                    );
                    if clipped.len() >= 3 {
                        push_pdf_polygon(ops, &clipped, PaintMode::FillStroke);
                    }
                    ops.push(Op::SetOutlineThickness { pt: Pt(1.4) });
                    if let Some((ca, cb)) = clip_segment_to_rect(
                        pt(-hw, -hh),
                        pt(-hw, hh),
                        inner_min_x,
                        inner_min_y,
                        inner_max_x,
                        inner_max_y,
                    ) {
                        push_pdf_polyline(ops, &[ca, cb], false);
                    }
                }
                PropKind::Bed => {
                    ops.push(Op::SetFillColor {
                        col: Color::Greyscale(Greyscale::new(0.95, None)),
                    });
                    let clipped = clip_polygon_to_rect(
                        &corners,
                        inner_min_x,
                        inner_min_y,
                        inner_max_x,
                        inner_max_y,
                    );
                    if clipped.len() >= 3 {
                        push_pdf_polygon(ops, &clipped, PaintMode::FillStroke);
                    }
                    for py in [-hh * 0.45, hh * 0.45] {
                        let pil = clip_polygon_to_rect(
                            &[
                                pt(-hw * 0.85, py - hh * 0.30),
                                pt(-hw * 0.45, py - hh * 0.30),
                                pt(-hw * 0.45, py + hh * 0.30),
                                pt(-hw * 0.85, py + hh * 0.30),
                            ],
                            inner_min_x,
                            inner_min_y,
                            inner_max_x,
                            inner_max_y,
                        );
                        if pil.len() >= 3 {
                            push_pdf_polygon(ops, &pil, PaintMode::FillStroke);
                        }
                    }
                }
                PropKind::Table | PropKind::Square | PropKind::Custom => {
                    ops.push(Op::SetFillColor {
                        col: Color::Greyscale(Greyscale::new(0.94, None)),
                    });
                    let clipped = clip_polygon_to_rect(
                        &corners,
                        inner_min_x,
                        inner_min_y,
                        inner_max_x,
                        inner_max_y,
                    );
                    if clipped.len() >= 3 {
                        push_pdf_polygon(ops, &clipped, PaintMode::FillStroke);
                    }
                }
            }
            if !prop.label.trim().is_empty() {
                let lbl = prop.label.trim();
                let c = to_mm(prop.x, prop.y);
                // Skip prop center label when a camera sits right over it
                let obscured_by_cam = prop.kind != PropKind::Wall
                    && cam_positions_mm
                        .iter()
                        .any(|&(cx, cy)| (cx - c.0).hypot(cy - c.1) < 5.0);
                if !obscured_by_cam {
                    let pt_sz = 5.8_f32;
                    let lw = lbl.chars().count() as f32 * CHAR_MM * (pt_sz / FONT_PT);
                    let (label_col, label_y) = if prop.kind == PropKind::Wall {
                        (
                            Color::Greyscale(Greyscale::new(0.42, None)),
                            c.1 + (prop.h * 0.5 * scale) + 1.4,
                        )
                    } else {
                        (Color::Greyscale(Greyscale::new(0.48, None)), c.1 - 0.7)
                    };
                    ops.push(Op::SetFillColor { col: label_col });
                    draw_summary_text(
                        ops,
                        regular,
                        fallback_regular,
                        pt_sz,
                        (c.0 - lw * 0.5).clamp(box_x + 1.5, box_x + box_w - lw - 1.5),
                        label_y.clamp(box_y + 1.5, box_y + box_h - 3.0),
                        lbl,
                    );
                }
            }
        }
    }

    // Layer 2: Actors (true-to-scale silhouettes; name pills painted cleanly in Layer 4)
    struct ActorLabelSpec<'a> {
        ap: (f32, f32),
        fwd: (f32, f32),
        sh_half: f32,
        sh_thick: f32,
        name: &'a str,
    }
    let mut actor_labels: Vec<ActorLabelSpec<'_>> = Vec::with_capacity(plan.actors.len());

    for actor in &plan.actors {
        let ap = to_mm(actor.x, actor.y);
        let rad = actor.angle_deg.to_radians();
        let (r, g, b) = actor.color.rgb();
        let acol = Color::Rgb(Rgb::new(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            None,
        ));
        if let Some((mx, my)) = actor.move_to {
            let mp = to_mm(mx, my);
            let dir = (mp.0 - ap.0, mp.1 - ap.1);
            let dist = dir.0.hypot(dir.1);
            if dist > 2.5 {
                let ux = dir.0 / dist;
                let uy = dir.1 / dist;
                let line_end = (mp.0 - ux * 1.6, mp.1 - uy * 1.6);
                if let Some((ca, cb)) = clip_segment_to_rect(
                    ap,
                    line_end,
                    inner_min_x,
                    inner_min_y,
                    inner_max_x,
                    inner_max_y,
                ) {
                    ops.push(Op::SetOutlineColor { col: acol.clone() });
                    ops.push(Op::SetOutlineThickness { pt: Pt(0.75) });
                    push_pdf_dashed_line(ops, ca, cb, 1.7, 1.3);
                }
                if mp.0 >= inner_min_x
                    && mp.0 <= inner_max_x
                    && mp.1 >= inner_min_y
                    && mp.1 <= inner_max_y
                {
                    ops.push(Op::SetFillColor { col: acol.clone() });
                    push_pdf_arrow_head(ops, (mp.0 - ux * 1.1, mp.1 - uy * 1.1), dir, 1.8);
                    let ring: Vec<(f32, f32)> = (0..16)
                        .map(|k| {
                            let a = std::f32::consts::TAU * (k as f32 / 16.0);
                            (mp.0 + 1.4 * a.cos(), mp.1 + 1.4 * a.sin())
                        })
                        .collect();
                    ops.push(Op::SetOutlineColor { col: acol.clone() });
                    ops.push(Op::SetOutlineThickness { pt: Pt(0.65) });
                    push_pdf_polyline(ops, &ring, true);
                }
            }
        }
        // Shoulders + head (true-to-scale ~0.60m overhead figure matching metric scale bar)
        let fwd = (rad.cos(), -rad.sin()); // PDF +Y is up
        let right = (-fwd.1, fwd.0);
        let sh_half = (0.30 * scale).clamp(2.2, 5.2);
        let sh_thick = (0.14 * scale).clamp(1.1, 2.5);
        let arad = sh_half.max(sh_thick * 1.5);
        obstacles.push((ap.0 - arad, ap.1 - arad, ap.0 + arad, ap.1 + arad, 11.0));

        ops.push(Op::SetFillColor {
            col: Color::Rgb(Rgb::new(
                0.82 + 0.18 * (r as f32 / 255.0),
                0.82 + 0.18 * (g as f32 / 255.0),
                0.82 + 0.18 * (b as f32 / 255.0),
                None,
            )),
        });
        ops.push(Op::SetOutlineColor { col: acol.clone() });
        ops.push(Op::SetOutlineThickness { pt: Pt(0.85) });
        push_pdf_polygon(
            ops,
            &[
                (
                    ap.0 - right.0 * sh_half - fwd.0 * sh_thick * 0.72,
                    ap.1 - right.1 * sh_half - fwd.1 * sh_thick * 0.72,
                ),
                (
                    ap.0 + right.0 * sh_half - fwd.0 * sh_thick * 0.72,
                    ap.1 + right.1 * sh_half - fwd.1 * sh_thick * 0.72,
                ),
                (
                    ap.0 + right.0 * sh_half * 0.85 + fwd.0 * sh_thick * 0.72,
                    ap.1 + right.1 * sh_half * 0.85 + fwd.1 * sh_thick * 0.72,
                ),
                (
                    ap.0 - right.0 * sh_half * 0.85 + fwd.0 * sh_thick * 0.72,
                    ap.1 - right.1 * sh_half * 0.85 + fwd.1 * sh_thick * 0.72,
                ),
            ],
            PaintMode::FillStroke,
        );
        // Gaze nose triangle
        let nose_tip = (
            ap.0 + fwd.0 * sh_thick * 1.75,
            ap.1 + fwd.1 * sh_thick * 1.75,
        );
        let nose_base = (
            ap.0 + fwd.0 * sh_thick * 0.68,
            ap.1 + fwd.1 * sh_thick * 0.68,
        );
        ops.push(Op::SetFillColor { col: acol.clone() });
        push_pdf_polygon(
            ops,
            &[
                nose_tip,
                (
                    nose_base.0 + right.0 * sh_thick * 0.62,
                    nose_base.1 + right.1 * sh_thick * 0.62,
                ),
                (
                    nose_base.0 - right.0 * sh_thick * 0.62,
                    nose_base.1 - right.1 * sh_thick * 0.62,
                ),
            ],
            PaintMode::Fill,
        );
        // Smooth 24-segment circular head with crisp white halo outline
        let hr = (0.125 * scale).clamp(1.2, 2.4);
        let head_pts: Vec<(f32, f32)> = (0..24)
            .map(|k| {
                let a = std::f32::consts::TAU * (k as f32 / 24.0);
                (ap.0 + hr * a.cos(), ap.1 + hr * a.sin())
            })
            .collect();
        ops.push(Op::SetFillColor {
            col: Color::Greyscale(Greyscale::new(0.20, None)),
        });
        ops.push(Op::SetOutlineColor {
            col: Color::Greyscale(Greyscale::new(1.0, None)),
        });
        ops.push(Op::SetOutlineThickness { pt: Pt(0.65) });
        push_pdf_polygon(ops, &head_pts, PaintMode::FillStroke);

        let name = actor.name.trim();
        if !name.is_empty() {
            actor_labels.push(ActorLabelSpec {
                ap,
                fwd,
                sh_half,
                sh_thick,
                name,
            });
        }
    }

    // Layer 3: Proportional camera bodies, twin dolly rails, and centered
    // shot index badges (matched to the Camera Key panel beside/below)
    let bw = (0.135 * scale).clamp(1.65, 2.45);
    let bh = (0.105 * scale).clamp(1.25, 1.85);
    let hood_len = (0.075 * scale).clamp(0.95, 1.45);
    let hood_base = bw * 0.48;
    let hood_tip = bw * 0.82;

    for (ci, cam) in plan.cameras.iter().enumerate() {
        let Some(shot) = p.shots.get(cam.shot_id) else {
            continue;
        };
        let cp = to_mm(cam.x, cam.y);
        let rad = cam.angle_deg.to_radians();
        let (r, g, b) = shot.color.rgb();
        let col = Color::Rgb(Rgb::new(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            None,
        ));
        let cam_r = (bw.max(bh + hood_len)) + 0.6;
        obstacles.push((cp.0 - cam_r, cp.1 - cam_r, cp.0 + cam_r, cp.1 + cam_r, 14.0));

        // Twin dashed dolly rails + destination ring
        if let Some((tx, ty)) = cam.track_to {
            let tp = to_mm(tx, ty);
            let dir = (tp.0 - cp.0, tp.1 - cp.1);
            let dist = dir.0.hypot(dir.1);
            if dist > 2.5 {
                let ux = dir.0 / dist;
                let uy = dir.1 / dist;
                let px = -uy * 0.85;
                let py = ux * 0.85;
                ops.push(Op::SetOutlineColor { col: col.clone() });
                ops.push(Op::SetOutlineThickness { pt: Pt(0.55) });
                for sign in [-1.0_f32, 1.0_f32] {
                    let s0 = (cp.0 + px * sign, cp.1 + py * sign);
                    let s1 = (tp.0 + px * sign, tp.1 + py * sign);
                    if let Some((ca, cb)) = clip_segment_to_rect(
                        s0,
                        s1,
                        inner_min_x,
                        inner_min_y,
                        inner_max_x,
                        inner_max_y,
                    ) {
                        push_pdf_dashed_line(ops, ca, cb, 1.7, 1.3);
                    }
                }
                if tp.0 >= inner_min_x
                    && tp.0 <= inner_max_x
                    && tp.1 >= inner_min_y
                    && tp.1 <= inner_max_y
                {
                    ops.push(Op::SetFillColor { col: col.clone() });
                    push_pdf_arrow_head(ops, tp, dir, 1.9);
                    let ring: Vec<(f32, f32)> = (0..16)
                        .map(|k| {
                            let a = std::f32::consts::TAU * (k as f32 / 16.0);
                            (tp.0 + 1.45 * a.cos(), tp.1 + 1.45 * a.sin())
                        })
                        .collect();
                    ops.push(Op::SetOutlineColor { col: col.clone() });
                    ops.push(Op::SetOutlineThickness { pt: Pt(0.6) });
                    push_pdf_polyline(ops, &ring, true);
                }
            }
        }

        // Camera body + matte box
        let fwd = (rad.cos(), -rad.sin());
        let right = (-fwd.1, fwd.0);
        ops.push(Op::SetFillColor { col: col.clone() });
        ops.push(Op::SetOutlineColor {
            col: Color::Greyscale(Greyscale::new(1.0, None)),
        });
        ops.push(Op::SetOutlineThickness { pt: Pt(0.55) });
        push_pdf_polygon(
            ops,
            &[
                (
                    cp.0 + fwd.0 * bh - right.0 * hood_base,
                    cp.1 + fwd.1 * bh - right.1 * hood_base,
                ),
                (
                    cp.0 + fwd.0 * (bh + hood_len) - right.0 * hood_tip,
                    cp.1 + fwd.1 * (bh + hood_len) - right.1 * hood_tip,
                ),
                (
                    cp.0 + fwd.0 * (bh + hood_len) + right.0 * hood_tip,
                    cp.1 + fwd.1 * (bh + hood_len) + right.1 * hood_tip,
                ),
                (
                    cp.0 + fwd.0 * bh + right.0 * hood_base,
                    cp.1 + fwd.1 * bh + right.1 * hood_base,
                ),
            ],
            PaintMode::FillStroke,
        );
        push_pdf_polygon(
            ops,
            &[
                (
                    cp.0 - fwd.0 * bh - right.0 * bw,
                    cp.1 - fwd.1 * bh - right.1 * bw,
                ),
                (
                    cp.0 + fwd.0 * bh - right.0 * bw,
                    cp.1 + fwd.1 * bh - right.1 * bw,
                ),
                (
                    cp.0 + fwd.0 * bh + right.0 * bw,
                    cp.1 + fwd.1 * bh + right.1 * bw,
                ),
                (
                    cp.0 - fwd.0 * bh + right.0 * bw,
                    cp.1 - fwd.1 * bh + right.1 * bw,
                ),
            ],
            PaintMode::FillStroke,
        );

        // Crisp centered camera index number inside body
        let num = shot_numbers.get(&cam.shot_id).copied().unwrap_or(ci + 1);
        let badge = num.to_string();
        let pt_sz = if badge.len() >= 2 { 4.8_f32 } else { 5.6_f32 };
        let bw_txt = badge.chars().count() as f32 * CHAR_MM * (pt_sz / FONT_PT);
        let cap_h = 0.62 * pt_sz / MM_TO_PT;
        ops.push(Op::SetFillColor {
            col: ink_contrast_on_rgb(r, g, b),
        });
        draw_summary_text(
            ops,
            bold,
            fallback_bold,
            pt_sz,
            cp.0 - bw_txt * 0.5,
            cp.1 - cap_h * 0.5,
            &badge,
        );
    }

    // Reserve scale bar area so actor labels never overlap it
    let (scale_m, scale_lbl) = if scale >= 24.0 {
        (0.5_f32, "50 cm")
    } else if scale >= 6.5 {
        (1.0_f32, "1 m")
    } else {
        (2.0_f32, "2 m")
    };
    let bar_w = scale_m * scale;
    let sx0 = box_x + 3.6;
    let sy0 = box_y + 3.0;
    obstacles.push((
        sx0 - 1.5,
        box_y + 0.8,
        sx0 + bar_w.max(10.0) + 2.0,
        sy0 + 4.5,
        16.0,
    ));

    // Layer 4: Smart collision-free actor name pills (behind actor's back first,
    // falling back to clearest open spot so names never cross tables or cameras)
    let mut placed_pills: Vec<(f32, f32, f32, f32)> = Vec::with_capacity(actor_labels.len());
    let name_pt = 5.8_f32;
    let name_cap_h = 0.62 * name_pt / MM_TO_PT;
    let pill_h = 3.2_f32;
    let margin_in = 1.6_f32;

    for spec in &actor_labels {
        let lw = spec.name.chars().count() as f32 * CHAR_MM * (name_pt / FONT_PT);
        let pw = (lw + 2.2).min(box_w - 4.0);
        let back_dist = spec.sh_thick + pill_h * 0.5 + 1.7;
        let side_dist_y = spec.sh_half.max(spec.sh_thick) + pill_h * 0.5 + 1.5;
        let side_dist_x = spec.sh_half + pw * 0.5 + 1.5;

        let candidates = [
            // 0. Behind the actor's shoulders (away from gaze / table)
            (
                spec.ap.0 - spec.fwd.0 * back_dist,
                spec.ap.1 - spec.fwd.1 * back_dist,
            ),
            // 1. Below actor
            (spec.ap.0, spec.ap.1 - side_dist_y),
            // 2. Above actor
            (spec.ap.0, spec.ap.1 + side_dist_y),
            // 3. Left of actor
            (spec.ap.0 - side_dist_x, spec.ap.1),
            // 4. Right of actor
            (spec.ap.0 + side_dist_x, spec.ap.1),
            // 5. Behind shoulders slightly further out
            (
                spec.ap.0 - spec.fwd.0 * (back_dist + 2.2),
                spec.ap.1 - spec.fwd.1 * (back_dist + 2.2),
            ),
        ];

        let mut best_rect = (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
        let mut best_score = f32::INFINITY;
        for (ci, &(cx, cy)) in candidates.iter().enumerate() {
            let x0 = (cx - pw * 0.5).clamp(box_x + margin_in, box_x + box_w - margin_in - pw);
            let y0 =
                (cy - pill_h * 0.5).clamp(box_y + margin_in, box_y + box_h - margin_in - pill_h);
            let r = (x0, y0, x0 + pw, y0 + pill_h);
            let mut score = (ci as f32) * 0.25;
            // Small penalty if clamping pushed the pill far from intended candidate
            let clamped_cx = (r.0 + r.2) * 0.5;
            let clamped_cy = (r.1 + r.3) * 0.5;
            score += (clamped_cx - cx).hypot(clamped_cy - cy) * 0.4;
            for &pr in &placed_pills {
                score += rect_overlap_area(r, pr) * 24.0;
            }
            for &(ox0, oy0, ox1, oy1, w) in &obstacles {
                score += rect_overlap_area(r, (ox0, oy0, ox1, oy1)) * w;
            }
            if score < best_score {
                best_score = score;
                best_rect = r;
            }
        }
        placed_pills.push(best_rect);

        // Crisp white pill + subtle border + bold actor name
        ops.push(Op::SetFillColor {
            col: Color::Greyscale(Greyscale::new(0.995, None)),
        });
        ops.push(Op::SetOutlineColor {
            col: Color::Greyscale(Greyscale::new(0.76, None)),
        });
        ops.push(Op::SetOutlineThickness { pt: Pt(0.38) });
        push_pdf_rounded_rect(
            ops,
            best_rect.0,
            best_rect.1,
            best_rect.2 - best_rect.0,
            best_rect.3 - best_rect.1,
            0.85,
            PaintMode::FillStroke,
        );
        ops.push(Op::SetFillColor {
            col: Color::Greyscale(Greyscale::new(0.10, None)),
        });
        let pill_cx = (best_rect.0 + best_rect.2) * 0.5;
        let pill_cy = (best_rect.1 + best_rect.3) * 0.5;
        draw_summary_text(
            ops,
            bold,
            fallback_bold,
            name_pt,
            pill_cx - lw * 0.5,
            pill_cy - name_cap_h * 0.5,
            spec.name,
        );
    }

    // Layer 5: True-to-scale metric architectural scale bar in bottom-left corner
    let lbl_w = scale_lbl.chars().count() as f32 * CHAR_MM * (6.0 / FONT_PT);
    let badge_w = bar_w.max(lbl_w) + 3.0;
    ops.push(Op::SetFillColor {
        col: Color::Greyscale(Greyscale::new(0.988, None)),
    });
    push_pdf_rounded_rect(
        ops,
        sx0 - 1.5,
        sy0 - 1.3,
        badge_w,
        5.1,
        0.8,
        PaintMode::Fill,
    );
    ops.push(Op::SetOutlineColor {
        col: Color::Greyscale(Greyscale::new(0.40, None)),
    });
    ops.push(Op::SetOutlineThickness { pt: Pt(0.55) });
    push_pdf_polyline(ops, &[(sx0, sy0), (sx0 + bar_w, sy0)], false);
    push_pdf_polyline(ops, &[(sx0, sy0 - 0.7), (sx0, sy0 + 0.7)], false);
    push_pdf_polyline(
        ops,
        &[(sx0 + bar_w, sy0 - 0.7), (sx0 + bar_w, sy0 + 0.7)],
        false,
    );
    ops.push(Op::SetFillColor {
        col: Color::Greyscale(Greyscale::new(0.38, None)),
    });
    draw_summary_text(
        ops,
        regular,
        fallback_regular,
        6.0,
        (sx0 + (bar_w - lbl_w) * 0.5).max(box_x + 2.0),
        sy0 + 1.15,
        scale_lbl,
    );

    // Outer crisp diagram frame border painted last over any clipped edges
    ops.push(Op::SetOutlineColor {
        col: Color::Greyscale(Greyscale::new(0.76, None)),
    });
    ops.push(Op::SetOutlineThickness { pt: Pt(0.6) });
    push_pdf_polyline(
        ops,
        &[
            (box_x, box_y),
            (box_x + box_w, box_y),
            (box_x + box_w, box_y + box_h),
            (box_x, box_y + box_h),
        ],
        true,
    );
}

/// Draw a clean numbered Camera Key legend panel beside (side panel) or below
/// (multi-column strip) the vector floor plan.
#[allow(clippy::too_many_arguments)]
fn draw_camera_legend_panel(
    ops: &mut Vec<Op>,
    items: &[(usize, &scriptcraft_core::shots::Shot)],
    box_x: f32,
    box_y: f32,
    box_w: f32,
    box_h: f32,
    cols: usize,
    regular: &Face,
    bold: &Face,
    fallback_regular: &Face,
    fallback_bold: &Face,
) {
    if items.is_empty() {
        return;
    }
    let cols = cols.max(1);
    // Subtle card frame
    ops.push(Op::SetFillColor {
        col: Color::Greyscale(Greyscale::new(0.975, None)),
    });
    ops.push(Op::SetOutlineColor {
        col: Color::Greyscale(Greyscale::new(0.80, None)),
    });
    ops.push(Op::SetOutlineThickness { pt: Pt(0.5) });
    push_pdf_polygon(
        ops,
        &[
            (box_x, box_y),
            (box_x + box_w, box_y),
            (box_x + box_w, box_y + box_h),
            (box_x, box_y + box_h),
        ],
        PaintMode::FillStroke,
    );

    let pad = 3.0_f32;
    let header_y = box_y + box_h - 4.8;
    ops.push(Op::SetFillColor {
        col: Color::Greyscale(Greyscale::new(0.38, None)),
    });
    draw_summary_text(
        ops,
        bold,
        fallback_bold,
        6.8,
        box_x + pad,
        header_y,
        "CAMERA KEY",
    );
    let div_y = header_y - 1.8;
    ops.push(Op::SetOutlineColor {
        col: Color::Greyscale(Greyscale::new(0.85, None)),
    });
    ops.push(Op::SetOutlineThickness { pt: Pt(0.35) });
    push_pdf_polyline(
        ops,
        &[(box_x + pad, div_y), (box_x + box_w - pad, div_y)],
        false,
    );

    let rows = items.len().div_ceil(cols).max(1);
    let avail_h = (div_y - (box_y + 2.0)).max(8.0);
    let row_step = (avail_h / rows as f32).min(if cols == 1 { 10.5 } else { 8.5 });
    let compact = row_step < 7.8;
    let col_w = (box_w - 2.0 * pad) / cols as f32;
    let title_pt = if compact { 6.2_f32 } else { 6.8_f32 };
    let meta_pt = 5.8_f32;
    let title_char_w = CHAR_MM * (title_pt / FONT_PT);
    let badge_w = 4.6_f32;
    let badge_h = 3.6_f32;

    for (idx, &(num, shot)) in items.iter().enumerate() {
        let (col_i, row_i) = if cols == 1 {
            (0usize, idx)
        } else {
            (idx % cols, idx / cols)
        };
        let rx = box_x + pad + (col_i as f32) * col_w;
        let row_top = div_y - 2.2 - (row_i as f32) * row_step;
        if row_top - badge_h < box_y + 1.0 {
            break;
        }
        let (r, g, b) = shot.color.rgb();
        let scol = Color::Rgb(Rgb::new(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            None,
        ));

        // Color-coded numbered badge pill
        let by = row_top - badge_h;
        ops.push(Op::SetFillColor { col: scol });
        push_pdf_rounded_rect(ops, rx, by, badge_w, badge_h, 0.8, PaintMode::Fill);
        let num_str = num.to_string();
        let n_pt = if num_str.len() >= 2 { 5.4_f32 } else { 6.0_f32 };
        let nw = num_str.chars().count() as f32 * CHAR_MM * (n_pt / FONT_PT);
        let n_cap = 0.62 * n_pt / MM_TO_PT;
        ops.push(Op::SetFillColor {
            col: ink_contrast_on_rgb(r, g, b),
        });
        draw_summary_text(
            ops,
            bold,
            fallback_bold,
            n_pt,
            rx + (badge_w - nw) * 0.5,
            by + (badge_h - n_cap) * 0.5,
            &num_str,
        );

        // Shot title + lens/size meta
        let tx = rx + badge_w + 1.8;
        let text_avail = (col_w - badge_w - 3.2).max(12.0);
        let max_chars = (text_avail / title_char_w).floor().max(8.0) as usize;
        let code_clean = shot.code.trim();
        let lens = shot.lens.trim();
        let meta_str = if lens.is_empty() {
            shot.size.short().to_string()
        } else {
            format!("{lens} · {}", shot.size.short())
        };

        if compact {
            let combined = format!("{code_clean} ({meta_str})");
            let line = if combined.chars().count() > max_chars {
                let mut s: String = combined.chars().take(max_chars.saturating_sub(1)).collect();
                s.push('…');
                s
            } else {
                combined
            };
            ops.push(Op::SetFillColor {
                col: Color::Greyscale(Greyscale::new(0.12, None)),
            });
            draw_summary_text(ops, bold, fallback_bold, title_pt, tx, by + 1.0, &line);
        } else {
            let title_line = if code_clean.chars().count() > max_chars {
                let mut s: String = code_clean
                    .chars()
                    .take(max_chars.saturating_sub(1))
                    .collect();
                s.push('…');
                s
            } else {
                code_clean.to_string()
            };
            ops.push(Op::SetFillColor {
                col: Color::Greyscale(Greyscale::new(0.12, None)),
            });
            draw_summary_text(
                ops,
                bold,
                fallback_bold,
                title_pt,
                tx,
                row_top - 2.4,
                &title_line,
            );
            ops.push(Op::SetFillColor {
                col: Color::Greyscale(Greyscale::new(0.45, None)),
            });
            draw_summary_text(
                ops,
                regular,
                fallback_regular,
                meta_pt,
                tx,
                row_top - 5.4,
                &meta_str,
            );
        }
    }
}

fn draw_shot_note_ops(
    ops: &mut Vec<Op>,
    line: &PrintLine,
    yy: f32,
    text_right: f32,
    page_w: f32,
    regular: &Face,
    fallback_regular: &Face,
) {
    let base_x = if line.right_aligned {
        line.x_mm - line.text.chars().count() as f32 * CHAR_MM
    } else {
        line.x_mm
    };
    let amber = Color::Rgb(Rgb::new(0.82, 0.48, 0.04, None));
    ops.push(Op::SetOutlineColor { col: amber.clone() });
    ops.push(Op::SetOutlineThickness { pt: Pt(0.9) });
    let mut callouts: Vec<&str> = Vec::new();
    for (c0, c1, note_text) in &line.shot_notes {
        if c0 < c1 {
            let ux0 = base_x + *c0 as f32 * CHAR_MM;
            let ux1 = base_x + *c1 as f32 * CHAR_MM;
            let uy = yy - 0.55;
            push_pdf_polyline(ops, &[(ux0, uy), (ux1, uy)], false);
        }
        if !note_text.trim().is_empty() {
            callouts.push(note_text.trim());
        }
    }
    if !callouts.is_empty() {
        let note_pt = 6.0_f32;
        let note_char_w = CHAR_MM * (note_pt / FONT_PT);
        let rx = text_right + 2.2;
        let avail_w = (page_w - 3.5 - rx).max(12.0);
        let max_chars = (avail_w / note_char_w).floor().max(8.0) as usize;
        let joined = format!("* {}", callouts.join("; "));
        let mut wrapped = wrap_title_line(&joined, max_chars);
        if wrapped.len() > 4 {
            wrapped.truncate(4);
            if let Some(last) = wrapped.last_mut() {
                let take_n = max_chars.saturating_sub(1);
                let mut s: String = last.chars().take(take_n).collect();
                s.push('…');
                *last = s;
            }
        }
        ops.push(Op::SetFillColor { col: amber });
        for (ri, part) in wrapped.iter().enumerate() {
            draw_summary_text(
                ops,
                regular,
                fallback_regular,
                note_pt,
                rx,
                yy + 0.3 - (ri as f32) * 2.8,
                part,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_directors_breakdown_pages(
    p: &Project,
    meta: &Metadata,
    title: &str,
    page_w: f32,
    page_h: f32,
    regular: &Face,
    bold: &Face,
    fallback_regular: &Face,
    fallback_bold: &Face,
    xobjects: &HashMap<uuid::Uuid, (XObjectId, usize, usize)>,
) -> Vec<Vec<Op>> {
    let margin = 14.0_f32;
    let bottom = 20.0_f32;
    let content_w = page_w - 2.0 * margin;
    let black = Color::Greyscale(Greyscale::new(0.0, None));
    let muted = Color::Greyscale(Greyscale::new(0.42, None));
    let rule_col = Color::Greyscale(Greyscale::new(0.86, None));

    let scenes = p.scenes();
    let locked = locked_scene_labels(meta);
    let prefix = meta.scene_number_prefix.trim();
    let start_num: u32 = meta.scene_number_start.trim().parse().unwrap_or(1);
    let scene_labels: Vec<String> = (0..scenes.len())
        .map(|i| {
            scene_label(&locked, prefix, start_num, i)
                .trim_end_matches('.')
                .to_string()
        })
        .collect();

    let mut pages: Vec<Vec<Op>> = Vec::new();

    for (si, scene) in scenes.iter().enumerate() {
        let shot_indices = p.shots.shots_for_scene(scene, &p.blocks);
        let plan = p.shots.plan_for_scene(scene.id).filter(|pl| !pl.is_empty());
        if shot_indices.is_empty() && plan.is_none() {
            continue;
        }

        let mut ops: Vec<Op> = vec![Op::SetOutlineThickness { pt: Pt(0.5) }];
        let mut y = page_h - margin;

        // Top running label
        ops.push(Op::SetFillColor { col: muted.clone() });
        draw_summary_text(
            &mut ops,
            bold,
            fallback_bold,
            8.0,
            margin,
            y,
            &format!("DIRECTOR'S BREAKDOWN  |  {title}"),
        );
        y -= 6.0;

        // Scene header banner
        let banner_label = format!("SCENE {} - {}", scene_labels[si], scene.heading);
        let max_banner = ((content_w - 4.0) / (CHAR_MM * (9.5 / FONT_PT)))
            .floor()
            .max(24.0) as usize;
        let banner_lines = wrap_title_line(&banner_label, max_banner);
        let banner_h = 2.4 + (banner_lines.len().max(1) as f32) * 4.5;
        ops.push(Op::SetFillColor {
            col: Color::Greyscale(Greyscale::new(0.93, None)),
        });
        let mut banner = PdfRect::from_wh(Pt(content_w * MM_TO_PT), Pt(banner_h * MM_TO_PT));
        banner.x = Pt(margin * MM_TO_PT);
        banner.y = Pt((y - banner_h + 4.5) * MM_TO_PT);
        ops.push(Op::DrawPolygon {
            polygon: banner.to_polygon(),
        });
        ops.push(Op::SetFillColor { col: black.clone() });
        for (bi, bl) in banner_lines.iter().enumerate() {
            if bi > 0 {
                y -= 4.5;
            }
            draw_summary_text(&mut ops, bold, fallback_bold, 9.5, margin + 2.0, y, bl);
        }
        y -= 5.8;

        let scene_cuts = p.shots.scene_cut_stats(scene, &p.blocks);
        let total_scene_s: f64 = shot_indices
            .iter()
            .filter_map(|&idx| p.shots.shots.get(idx))
            .map(|s| p.shots.shot_duration_secs(s, &p.blocks))
            .sum();
        let scene_meta_line = if scene_cuts.cut_count > 0 {
            format!(
                "{} {}  |  {} {}  |  avg {:.1}s/cut  |  est. ~{}s",
                shot_indices.len(),
                if shot_indices.len() == 1 {
                    "shot"
                } else {
                    "shots"
                },
                scene_cuts.cut_count,
                if scene_cuts.cut_count == 1 {
                    "cut"
                } else {
                    "cuts"
                },
                scene_cuts.avg_cut_secs,
                total_scene_s.round() as usize
            )
        } else {
            format!(
                "{} {}  |  est. ~{}s",
                shot_indices.len(),
                if shot_indices.len() == 1 {
                    "shot"
                } else {
                    "shots"
                },
                total_scene_s.round() as usize
            )
        };
        ops.push(Op::SetFillColor { col: muted.clone() });
        draw_summary_text(
            &mut ops,
            regular,
            fallback_regular,
            8.5,
            margin,
            y,
            &scene_meta_line,
        );
        y -= 4.5;

        // Assign consistent 1-based camera/shot numbers across floor plan,
        // Camera Key legend, and Shot Breakdown Cards.
        let mut shot_numbers: HashMap<uuid::Uuid, usize> = HashMap::new();
        for &idx in &shot_indices {
            if let Some(shot) = p.shots.shots.get(idx) {
                let next = shot_numbers.len() + 1;
                shot_numbers.entry(shot.id).or_insert(next);
            }
        }
        let mut plan_has_cameras = false;

        // Vector 2D Overhead Floor Plan + crisp numbered Camera Key
        if let Some(pl) = plan {
            for cam in &pl.cameras {
                if p.shots.get(cam.shot_id).is_some() {
                    let next = shot_numbers.len() + 1;
                    shot_numbers.entry(cam.shot_id).or_insert(next);
                }
            }
            let mut cam_legend: Vec<(usize, &scriptcraft_core::shots::Shot)> = pl
                .cameras
                .iter()
                .filter_map(|c| {
                    let shot = p.shots.get(c.shot_id)?;
                    let num = shot_numbers.get(&c.shot_id).copied()?;
                    Some((num, shot))
                })
                .collect();
            cam_legend.sort_by_key(|(num, _)| *num);
            cam_legend.dedup_by_key(|(num, _)| *num);
            plan_has_cameras = !cam_legend.is_empty();

            let (min_x, max_x, min_y, max_y) = floor_plan_world_bounds(pl);
            let world_aspect = (max_x - min_x).max(3.2) / (max_y - min_y).max(2.6);

            if plan_has_cameras && world_aspect <= 1.38 {
                // Square/vertical room: generous 122.5 × 114 mm floor plan on
                // the left + dedicated Camera Key panel on the right.
                let legend_w = 56.0_f32;
                let gap = 3.5_f32;
                let plan_w = (content_w - legend_w - gap).max(90.0);
                let plan_h = 114.0_f32;
                let plan_y = y - plan_h;
                draw_floor_plan_vector(
                    &mut ops,
                    p,
                    pl,
                    &shot_numbers,
                    margin,
                    plan_y,
                    plan_w,
                    plan_h,
                    regular,
                    bold,
                    fallback_regular,
                    fallback_bold,
                );
                draw_camera_legend_panel(
                    &mut ops,
                    &cam_legend,
                    margin + plan_w + gap,
                    plan_y,
                    legend_w,
                    plan_h,
                    1,
                    regular,
                    bold,
                    fallback_regular,
                    fallback_bold,
                );
                y = plan_y - 6.5;
            } else if plan_has_cameras {
                // Wide room / street: full-width floor plan + compact 3-column
                // Camera Key strip directly underneath.
                let plan_h = 92.0_f32;
                let plan_y = y - plan_h;
                draw_floor_plan_vector(
                    &mut ops,
                    p,
                    pl,
                    &shot_numbers,
                    margin,
                    plan_y,
                    content_w,
                    plan_h,
                    regular,
                    bold,
                    fallback_regular,
                    fallback_bold,
                );
                let cols = if cam_legend.len() <= 2 {
                    cam_legend.len().max(1)
                } else {
                    3
                };
                let rows = cam_legend.len().div_ceil(cols).max(1);
                let legend_h = (9.5 + (rows as f32) * 8.2).min(46.0);
                let legend_y = plan_y - 2.5 - legend_h;
                draw_camera_legend_panel(
                    &mut ops,
                    &cam_legend,
                    margin,
                    legend_y,
                    content_w,
                    legend_h,
                    cols,
                    regular,
                    bold,
                    fallback_regular,
                    fallback_bold,
                );
                y = legend_y - 6.0;
            } else {
                let plan_h = 96.0_f32;
                let plan_y = y - plan_h;
                draw_floor_plan_vector(
                    &mut ops,
                    p,
                    pl,
                    &shot_numbers,
                    margin,
                    plan_y,
                    content_w,
                    plan_h,
                    regular,
                    bold,
                    fallback_regular,
                    fallback_bold,
                );
                y = plan_y - 6.5;
            }
        }

        // Shot Breakdown Cards (with optional embedded storyboard frame thumbnails)
        let body_pt = 8.2_f32;
        let char_w = CHAR_MM * (body_pt / FONT_PT);
        let thumb_w_mm = 38.0_f32;
        let thumb_h_mm = 22.0_f32;

        for idx in shot_indices {
            let Some(shot) = p.shots.shots.get(idx) else {
                continue;
            };
            let img_entry = xobjects.get(&shot.id).cloned();
            let text_x = if img_entry.is_some() {
                margin + thumb_w_mm + 4.5
            } else {
                margin + 5.5
            };
            let text_w = (page_w - margin - text_x).max(50.0);
            let max_chars = (text_w / char_w).floor().max(24.0) as usize;
            let dur_s = p.shots.shot_duration_secs(shot, &p.blocks).round().max(1.0) as usize;
            let lens_str = if shot.lens.trim().is_empty() {
                "-"
            } else {
                shot.lens.trim()
            };
            let title_prefix = if plan_has_cameras {
                shot_numbers
                    .get(&shot.id)
                    .map(|n| format!("#{n} "))
                    .unwrap_or_default()
            } else {
                String::new()
            };
            let header_line = format!(
                "{title_prefix}{}   |   {}  ·  {}  ·  {}  ·  ~{}s",
                shot.code.trim(),
                shot.size.short(),
                lens_str,
                shot.movement.label(),
                dur_s
            );
            let hdr_wrapped = wrap_title_line(&header_line, max_chars);
            let desc_wrapped = if !shot.description.trim().is_empty() {
                wrap_title_line(shot.description.trim(), max_chars)
            } else {
                Vec::new()
            };
            let covered = p.shots.covered_excerpt(shot, &p.blocks);
            let cov_wrapped = if !covered.is_empty() {
                wrap_title_line(&covered, max_chars)
            } else {
                Vec::new()
            };
            let note_index = scriptcraft_core::shots::block_index_map(&p.blocks);
            let mut note_wrapped: Vec<String> = Vec::new();
            for note in &shot.notes {
                let nt = note.text.trim();
                if nt.is_empty() {
                    continue;
                }
                let ex = note.range.excerpt(&note_index, &p.blocks);
                let ex_short = if ex.chars().count() > 30 {
                    let mut s: String = ex.chars().take(29).collect();
                    s.push('…');
                    s
                } else {
                    ex
                };
                let cue_line = if ex_short.is_empty() {
                    format!("* Cue: {nt}")
                } else {
                    format!("* \"{ex_short}\" - {nt}")
                };
                note_wrapped.extend(wrap_title_line(&cue_line, max_chars));
            }

            let text_h = (hdr_wrapped.len() as f32) * 4.3
                + (desc_wrapped.len() as f32) * 4.0
                + (note_wrapped.len() as f32) * 3.8
                + (cov_wrapped.len() as f32) * 3.8
                + 4.0;
            let card_h = if img_entry.is_some() {
                text_h.max(thumb_h_mm + 4.0)
            } else {
                text_h.max(10.0)
            };

            if y - card_h < bottom {
                pages.push(ops);
                ops = vec![Op::SetOutlineThickness { pt: Pt(0.5) }];
                y = page_h - margin;
                ops.push(Op::SetFillColor { col: muted.clone() });
                draw_summary_text(
                    &mut ops,
                    bold,
                    fallback_bold,
                    8.5,
                    margin,
                    y,
                    &format!("SCENE {} - {} (continued)", scene_labels[si], scene.heading),
                );
                y -= 7.0;
            }

            let card_top = y;
            // Left color accent bar for this shot card
            let (r, g, b) = shot.color.rgb();
            ops.push(Op::SetFillColor {
                col: Color::Rgb(Rgb::new(
                    r as f32 / 255.0,
                    g as f32 / 255.0,
                    b as f32 / 255.0,
                    None,
                )),
            });
            let mut bar = PdfRect::from_wh(Pt(2.4 * MM_TO_PT), Pt((card_h - 2.0) * MM_TO_PT));
            bar.x = Pt(margin * MM_TO_PT);
            bar.y = Pt((card_top - card_h + 2.0) * MM_TO_PT);
            ops.push(Op::DrawPolygon {
                polygon: bar.to_polygon(),
            });

            // Optional storyboard / reference image thumbnail
            if let Some((xid, iw, ih)) = img_entry {
                let aspect = (iw as f32 / ih.max(1) as f32).clamp(0.4, 3.0);
                let (dw, dh) = if thumb_w_mm / thumb_h_mm > aspect {
                    (thumb_h_mm * aspect, thumb_h_mm)
                } else {
                    (thumb_w_mm, thumb_w_mm / aspect)
                };
                let ix = margin + 3.6 + (thumb_w_mm - dw) * 0.5;
                let iy = card_top - dh - 1.0;
                ops.push(Op::UseXobject {
                    id: xid,
                    transform: XObjectTransform {
                        translate_x: Some(Pt(ix * MM_TO_PT)),
                        translate_y: Some(Pt(iy * MM_TO_PT)),
                        scale_x: Some(dw * MM_TO_PT),
                        scale_y: Some(dh * MM_TO_PT),
                        no_auto_scale: true,
                        ..Default::default()
                    },
                });
                ops.push(Op::SetOutlineColor {
                    col: rule_col.clone(),
                });
                ops.push(Op::SetOutlineThickness { pt: Pt(0.4) });
                push_pdf_polyline(
                    &mut ops,
                    &[(ix, iy), (ix + dw, iy), (ix + dw, iy + dh), (ix, iy + dh)],
                    true,
                );
            }

            // Text block (header + description + covered excerpt)
            let mut ty = card_top - 3.2;
            ops.push(Op::SetFillColor { col: black.clone() });
            for hl in &hdr_wrapped {
                draw_summary_text(&mut ops, bold, fallback_bold, body_pt, text_x, ty, hl);
                ty -= 4.3;
            }
            for dl in &desc_wrapped {
                draw_summary_text(&mut ops, regular, fallback_regular, body_pt, text_x, ty, dl);
                ty -= 4.0;
            }
            if !note_wrapped.is_empty() {
                ops.push(Op::SetFillColor {
                    col: Color::Rgb(Rgb::new(0.74, 0.42, 0.04, None)),
                });
                for nl in &note_wrapped {
                    draw_summary_text(&mut ops, bold, fallback_bold, 7.6, text_x, ty, nl);
                    ty -= 3.8;
                }
            }
            if !cov_wrapped.is_empty() {
                ops.push(Op::SetFillColor { col: muted.clone() });
                for cl in &cov_wrapped {
                    draw_summary_text(&mut ops, regular, fallback_regular, 7.6, text_x, ty, cl);
                    ty -= 3.8;
                }
            }

            // Subtle divider line below card
            ops.push(Op::SetOutlineColor {
                col: rule_col.clone(),
            });
            ops.push(Op::SetOutlineThickness { pt: Pt(0.35) });
            push_pdf_polyline(
                &mut ops,
                &[
                    (margin, card_top - card_h),
                    (page_w - margin, card_top - card_h),
                ],
                false,
            );
            y = card_top - card_h - 3.0;
        }

        pages.push(ops);
    }

    // Stamp "Breakdown Page X of N" footer
    let total = pages.len();
    for (idx, page_ops) in pages.iter_mut().enumerate() {
        let label = format!("Breakdown {} of {}", idx + 1, total);
        let fx = right_aligned_x(&label, 8.2, page_w - margin);
        page_ops.push(Op::SetFillColor { col: muted.clone() });
        draw_summary_text(page_ops, regular, fallback_regular, 8.2, fx, 11.5, &label);
    }

    pages
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

    #[test]
    fn bold_character_cues_only_bolds_speaking_cues() {
        let p = Project {
            title: "Cues".into(),
            blocks: vec![
                ScriptBlock::new(ElementType::SceneHeading, "INT. ROOM - DAY".into()),
                ScriptBlock::new(ElementType::Action, "IVAN enters the room.".into()),
                ScriptBlock::new(ElementType::Character, "IVAN".into()),
                ScriptBlock::new(ElementType::Dialogue, "Hello there.".into()),
            ],
            ..Default::default()
        };
        let mut opts = default_opts();
        assert!(!opts.bold_character_cues, "off by default");
        let regular_pages = layout_pages(&p, &Default::default(), &opts);
        let cue_regular = regular_pages[0]
            .iter()
            .find(|l| l.text == "IVAN")
            .expect("cue line");
        assert!(!cue_regular.bold);

        opts.bold_character_cues = true;
        let bold_pages = layout_pages(&p, &Default::default(), &opts);
        let action_line = bold_pages[0]
            .iter()
            .find(|l| l.text == "IVAN enters the room.")
            .expect("action line");
        let cue_line = bold_pages[0]
            .iter()
            .find(|l| l.text == "IVAN")
            .expect("cue line");
        let dlg_line = bold_pages[0]
            .iter()
            .find(|l| l.text == "Hello there.")
            .expect("dialogue line");
        assert!(cue_line.bold, "speaking character cue must be bold");
        assert!(
            !action_line.bold,
            "action mentioning character must not be bold"
        );
        assert!(!dlg_line.bold, "dialogue must not be bold");
    }

    #[test]
    fn colored_script_highlights_exact_character_substrings() {
        use scriptcraft_core::shots::{ShotMovement, ShotRange, ShotSize};

        let mut p = Project {
            title: "CharHL".into(),
            blocks: vec![
                ScriptBlock::new(ElementType::SceneHeading, "INT. ROOM - DAY".into()),
                ScriptBlock::new(
                    ElementType::Action,
                    "Ivan stares at the flickering neon sign.".into(),
                ),
            ],
            ..Default::default()
        };
        let b1 = p.blocks[1].id;
        p.shots.add_shot(
            Some(p.blocks[0].id),
            "1",
            ShotSize::Insert,
            ShotMovement::Static,
            "85mm".into(),
            "Neon sign".into(),
            ShotRange::chars(b1, 19, b1, 39),
        );
        let pages = layout_pages_inner(&p, &Default::default(), &default_opts(), &[], None, true);
        let line = pages[0]
            .iter()
            .find(|l| l.text.contains("flickering"))
            .expect("action line");
        assert_eq!(line.shot_highlights.len(), 1);
        assert_eq!(line.shot_highlights[0].0, 19);
        assert_eq!(line.shot_highlights[0].1, 39);
        assert_eq!(line.shot_tag.as_ref().map(|(t, _)| t.as_str()), Some("1A"));
    }
}
