use crate::paginate::{page_labels, paginate, paginate_with_breaks};
use crate::place::{ROW_MM, RUSSIAN_GEOMETRY, TEXT_W_MM};
use crate::wrap::wrap_lines;
use printpdf::{
    Color, Greyscale, Line, LinePoint, Mm, Op, ParsedFont, PdfDocument, PdfFontHandle, PdfPage,
    PdfSaveOptions, Point, Pt, Rect as PdfRect, Rgb, TextItem, TextMatrix,
};
use scriptcraft_core::editing::{cue_name_for, RUSSIAN};
use scriptcraft_core::kitsp::Metadata;
use scriptcraft_core::model::{ElementType, Project};
use scriptcraft_core::page_lock::PageLock;
use scriptcraft_core::revision::{RevisionColor, RevisionSet};
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

/// A4 page geometry (mm) for KIT's Russian editor/export template:
/// margins 37.5/25/25/25, 147.5mm text column (58 chars at 10cpi Courier).
const PAGE_W: f32 = 210.0;
const PAGE_H: f32 = 297.0;
const MARGIN_L: f32 = 37.5;
const MARGIN_T: f32 = 25.0;
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
/// Right edge of the text column.
const TEXT_RIGHT: f32 = MARGIN_L + TEXT_W_MM;
/// Block columns in from the left margin: character 67.5mm, parenthetical
/// 55mm, dialogue 37.5mm — the editor's grid.
const X_CUE: f32 = MARGIN_L + RUSSIAN_GEOMETRY.cue_mm;
const X_PAREN: f32 = MARGIN_L + RUSSIAN_GEOMETRY.paren_mm;
const X_DLG: f32 = MARGIN_L + RUSSIAN_GEOMETRY.dialog_mm;
/// Baseline of the top-right page number: the editor bottom-aligns it 0.4
/// rows above the first content row.
const PAGE_NUM_BASELINE: f32 = MARGIN_T - 0.4 * PITCH_MM - DESCENT_MM;
/// Scene numbers sit 4mm left of the text margin (measured from KIT).
const SCENE_NUM_GAP: f32 = RUSSIAN_GEOMETRY.scene_num_gap_mm;
/// Footer baseline, inside the 25 mm bottom margin.
const FOOTER_BASELINE_MM: f32 = 12.0;
/// Revision margin bars shared with the editor (mm from the page edge).
pub const REVISION_BAR_X_MM: f32 = 10.0;
pub const REVISION_BAR_W_MM: f32 = 3.0;
/// Title-page nudges measured from KIT's export (mm below the slot row).
const TITLE_DY: f32 = 1.4;
const TITLE_VALUE_DY: f32 = 1.5;
const TITLE_CONTACT_DY: f32 = 1.8;
const TITLE_YEAR_DY: f32 = 2.6;

/// Toggles mirrored by KIT's Export Script dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportOptions {
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
        }
    }

    fn blank() -> Self {
        Self::text_line(MARGIN_L, String::new(), false)
    }
}

/// The number shown ahead of scene `i`: locked label when numbering is
/// frozen, else `{prefix}{start + i}.`
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

fn fit_margin_text(text: String, reserved: usize) -> Option<String> {
    let max = ((TEXT_W_MM / CHAR_MM) as usize).saturating_sub(reserved);
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

fn margin_line(text: String, band: MarginBand) -> PrintLine {
    PrintLine {
        margin: Some(band),
        ..PrintLine::text_line(MARGIN_L, text, false)
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
    fit_margin_text(text, 8)
}

fn page_footer(meta: &Metadata, latest: Option<&RevisionSet>, page: &str) -> Option<String> {
    let raw = meta.footer.trim();
    if raw.is_empty() {
        return None;
    }
    fit_margin_text(expand_margin_text(raw, latest, page), 0)
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
    let breaks: Vec<(usize, usize)> = page_anchors.iter().map(|a| (a.bi, a.para)).collect();
    let pages = if page_lock.is_some() {
        paginate_with_breaks(&renum, &RUSSIAN, &breaks)
    } else {
        paginate(&renum, &RUSSIAN)
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
                ..PrintLine::blank()
            });
        }
        let show_revision = opts.revision_marks || opts.revision_pages_only;
        if let Some(header) = page_header(meta, latest, show_revision, label) {
            lines.push(margin_line(header, MarginBand::Header));
        }
        if let Some(footer) = page_footer(meta, latest, label) {
            lines.push(margin_line(footer, MarginBand::Footer));
        }
        for chunk in &page.chunks {
            let b = &renum[chunk.bi];
            let revision = revisions[chunk.bi];
            for _ in 0..chunk.top {
                lines.push(PrintLine::blank());
            }
            let (x, bold) = match b.element_type {
                ElementType::SceneHeading => (MARGIN_L, true),
                ElementType::SceneCharacters => (MARGIN_L, false),
                ElementType::Action | ElementType::Shot => (MARGIN_L, false),
                ElementType::Character => (X_CUE, true),
                ElementType::Parenthetical => (X_PAREN, false),
                ElementType::Dialogue => (X_DLG, false),
                ElementType::Lyrics => (X_DLG, false),
                ElementType::Transition => (MARGIN_L, true),
                ElementType::NonPrintable => (X_CUE, false),
            };
            if chunk.contd {
                if let Some(cue) = cue_name_for(&renum, chunk.bi) {
                    let mut line = PrintLine::text_line(X_CUE, format!("{cue} (CONT'D)"), true);
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
                    MARGIN_L - SCENE_NUM_GAP - w,
                    label,
                    true,
                ));
                number_pushed = true;
            }
            if b.element_type == ElementType::SceneHeading {
                scene_i += 1;
            }
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
            let wrapped = wrap_lines(&text, RUSSIAN.col_chars(b.element_type));
            for (li, line) in wrapped.into_iter().enumerate() {
                let mut l = PrintLine::text_line(x, line, bold);
                l.share_row = li == 0 && number_pushed;
                l.revision = revision;
                lines.push(l);
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

/// Lay out the title page exactly like KIT's exporter: uppercase underlined
/// title on slot 16, genre / author / director / additional info from slot
/// 20 (every other slot, empties skipped), contacts on slot 39, year on
/// slot 57 (measured nudges below used for the last two).
pub fn layout_title_page(meta: &Metadata) -> Vec<PrintLine> {
    layout_title_page_with_latest(meta, None)
}

pub fn layout_title_page_with_latest(
    meta: &Metadata,
    latest: Option<&RevisionSet>,
) -> Vec<PrintLine> {
    fn centered(text: String, bold: bool, underline: bool, dy_mm: f32) -> PrintLine {
        let w = text.chars().count() as f32 * CHAR_MM;
        PrintLine {
            x_mm: (PAGE_W - w) / 2.0,
            text,
            bold,
            right_aligned: false,
            is_page_number: false,
            underline,
            dy_mm,
            share_row: false,
            revision: None,
            margin: None,
        }
    }
    let mut lines: Vec<PrintLine> = Vec::new();
    while lines.len() < 16 {
        lines.push(PrintLine::blank());
    }
    lines.push(centered(meta.title.to_uppercase(), true, true, TITLE_DY));
    let mut slot = 20usize;
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
        while lines.len() < slot {
            lines.push(PrintLine::blank());
        }
        lines.push(centered(v.to_string(), false, false, TITLE_VALUE_DY));
        slot += 2;
    }
    while lines.len() < 39 {
        lines.push(PrintLine::blank());
    }
    lines.push(PrintLine {
        text: meta.contacts.trim().to_string(),
        dy_mm: TITLE_CONTACT_DY,
        ..PrintLine::blank()
    });
    while lines.len() < 57 {
        lines.push(PrintLine::blank());
    }
    lines.push(centered(
        meta.year.trim().to_string(),
        false,
        false,
        TITLE_YEAR_DY,
    ));
    if let Some(set) = latest {
        while lines.len() < 60 {
            lines.push(PrintLine::blank());
        }
        lines.push(centered("REVISED".to_string(), true, false, TITLE_DY));
        let name = set.name.trim();
        if !name.is_empty() {
            lines.push(centered(name.to_string(), false, false, TITLE_VALUE_DY));
        }
        let date = revision_date(set);
        if !date.is_empty() {
            lines.push(centered(date, false, false, TITLE_VALUE_DY));
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
    pub fallback_regular: &'a [u8],
    pub fallback_bold: &'a [u8],
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
    let mut pages: Vec<Vec<PrintLine>> = Vec::new();
    if opts.title_page {
        pages.push(layout_title_page_with_latest(meta, latest));
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
    let show =
        |ops: &mut Vec<Op>, font: &printpdf::FontId, size: f32, x: f32, y: f32, text: &str| {
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
        };

    let mut doc_pages: Vec<PdfPage> = Vec::new();
    for lines in pages.iter() {
        let mut ops: Vec<Op> = vec![Op::SetOutlineThickness { pt: Pt(0.72) }];
        if let Some(wm) = watermark {
            let size = 40.0;
            let w = wm.chars().count() as f32 * CHAR_MM * (size / FONT_PT);
            ops.push(Op::SetFillColor {
                col: Color::Greyscale(Greyscale::new(0.82, None)),
            });
            let mut cx = ((PAGE_W - w) / 2.0).max(0.0);
            for (use_fb, run) in split_runs(wm, &regular.font, &fallback_regular.font) {
                let id = if use_fb {
                    &fallback_regular.id
                } else {
                    &regular.id
                };
                show(&mut ops, id, size, cx, PAGE_H / 2.0, run);
                cx += run.chars().count() as f32 * CHAR_MM * (size / FONT_PT);
            }
        }
        let mut y = PAGE_H - MARGIN_T - BASELINE_OFF;
        for line in lines.iter() {
            if line.share_row {
                y += PITCH_MM;
            }
            ops.push(Op::SetFillColor { col: black.clone() });
            if let Some(band) = line.margin {
                let yy = match band {
                    MarginBand::Header => PAGE_H - PAGE_NUM_BASELINE,
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
                let mut cx = TEXT_RIGHT - w;
                let (face, fb_face) = if line.bold {
                    (&bold, &fallback_bold)
                } else {
                    (&regular, &fallback_regular)
                };
                for (use_fb, run) in split_runs(&line.text, &face.font, &fb_face.font) {
                    let id = if use_fb { &fb_face.id } else { &face.id };
                    show(&mut ops, id, FONT_PT, cx, PAGE_H - PAGE_NUM_BASELINE, run);
                    cx += run.chars().count() as f32 * CHAR_MM;
                }
                continue;
            }
            let yy = y - line.dy_mm;
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
                    (TEXT_RIGHT - w).max(MARGIN_L)
                } else {
                    line.x_mm
                };
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
            }
            y -= PITCH_MM;
        }
        doc_pages.push(PdfPage::new(Mm(PAGE_W), Mm(PAGE_H), ops));
    }
    doc.with_pages(doc_pages);
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
        let pages = paginate(&p.blocks, &RUSSIAN);
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
                rows += wrap_lines(&slice, RUSSIAN.col_chars(b.element_type))
                    .len()
                    .max(1);
                if c.contd {
                    rows += 1;
                }
                if c.more {
                    rows += 1;
                }
            }
            assert!(rows <= RUSSIAN.lines_per_page, "page {pi} overflow");
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
        assert!(page[num].bold && !page[num].share_row);
        let head = &page[num + 1];
        assert_eq!(head.text, "EXT. PARK - DAY");
        assert_eq!(head.x_mm, MARGIN_L);
        assert!(head.bold && head.share_row);
        // Character cue at 37.5 + 67.5.
        let cue = page.iter().find(|l| l.text == "IVAN").unwrap();
        assert!((cue.x_mm - X_CUE).abs() < 0.01);
        assert!(cue.bold);
        // Dialogue at 37.5 + 37.5.
        let dlg = page.iter().find(|l| l.text.starts_with("Hello")).unwrap();
        assert!((dlg.x_mm - X_DLG).abs() < 0.01);
        assert!(!dlg.bold);
        // Parenthetical at 37.5 + 55.
        let par = page.iter().find(|l| l.text == "(softly)").unwrap();
        assert!((par.x_mm - X_PAREN).abs() < 0.01);
        // Transition keeps the editor's left-aligned bold style.
        let tr = page.iter().find(|l| l.text == "FADE OUT:").unwrap();
        assert!(!tr.right_aligned && tr.bold);
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

    #[test]
    fn page_lock_inserts_a_labels() {
        let mut p = sample();
        for i in 0..30 {
            p.blocks.push(ScriptBlock::new(
                ElementType::Action,
                format!("Original action {i}."),
            ));
        }
        let starts = crate::paginate::page_starts(&p.blocks, &RUSSIAN);
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
}
