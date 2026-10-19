use crate::paginate::paginate;
use crate::wrap::wrap_lines;
use printpdf::{
    Color, Greyscale, Line, LinePoint, Mm, Op, ParsedFont, PdfDocument, PdfFontHandle, PdfPage,
    PdfSaveOptions, Point, Pt, TextItem, TextMatrix,
};
use scriptcraft_core::editing::{cue_name_for, FINAL_DRAFT_A4};
use scriptcraft_core::kitsp::Metadata;
use scriptcraft_core::model::{ElementType, Project};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PdfError {
    #[error("font could not be parsed")]
    Font,
    #[error("no printable content")]
    Empty,
}

/// A4 page geometry (mm) for KIT's `final_draft_a4.kitss` export template:
/// margins 38/24/17/24, 155mm text column (61 chars at 10cpi Courier).
const PAGE_W: f32 = 210.0;
const PAGE_H: f32 = 297.0;
const MARGIN_L: f32 = 38.0;
const MARGIN_T: f32 = 24.0;
const MARGIN_R: f32 = 17.0;
const FONT_PT: f32 = 12.0;
/// 0.6em advance at 12pt = 7.2pt = 2.54mm per column.
const CHAR_MM: f32 = 2.54;
/// Template line pitch: single-spaced 12pt = 4.2333mm.
const PITCH_MM: f32 = FONT_PT / 72.0 * 25.4;
/// Millimetres per PDF point (TextMatrix offsets are in points).
const MM_TO_PT: f32 = 72.0 / 25.4;
/// Baseline offset below the slot top under the top margin.
const BASELINE_OFF: f32 = 3.0;
/// Right edge of the text column.
const TEXT_RIGHT: f32 = PAGE_W - MARGIN_R;
/// Block columns: character 51mm, parenthetical 36mm, dialogue 26mm in from
/// the left margin; dialogue right margin 40mm.
const X_CUE: f32 = MARGIN_L + 51.0;
const X_PAREN: f32 = MARGIN_L + 36.0;
const X_DLG: f32 = MARGIN_L + 26.0;
const DLG_RIGHT: f32 = TEXT_RIGHT - 40.0;
/// Baseline of the top-right page number, measured from the page top.
const PAGE_NUM_BASELINE: f32 = 16.4;
/// Scene numbers sit 4mm left of the text margin (measured from KIT).
const SCENE_NUM_GAP: f32 = 4.0;
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
    pub watermark: Option<String>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            title_page: true,
            page_numbers: true,
            scene_numbers: true,
            speaking_parts_count: false,
            watermark: None,
        }
    }
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

/// Typeset a project into positioned lines per page (pure function of the
/// document — unit-testable without fonts). Non-printable blocks are
/// excluded, matching KIT's print behavior. Geometry follows KIT's default
/// export template `Final Draft screenplay (A4)`.
pub fn layout_pages(p: &Project, meta: &Metadata, opts: &ExportOptions) -> Vec<Vec<PrintLine>> {
    let blocks: Vec<usize> = (0..p.blocks.len())
        .filter(|&i| p.blocks[i].element_type.is_printable())
        .collect();
    let renum: Vec<scriptcraft_core::model::ScriptBlock> =
        blocks.iter().map(|&i| p.blocks[i].clone()).collect();
    let pages = paginate(&renum, &FINAL_DRAFT_A4);
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
        // KIT numbers script pages starting from the second one ("2."…),
        // since the title page and first script page stay unnumbered.
        if opts.page_numbers && pi + 1 >= 2 {
            let pg = format!("{}.", pi + 1);
            lines.push(PrintLine {
                text: pg,
                is_page_number: true,
                bold: true,
                right_aligned: true,
                ..PrintLine::blank()
            });
        }
        for chunk in &page.chunks {
            let b = &renum[chunk.bi];
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
                ElementType::Transition => (MARGIN_L, false),
                ElementType::NonPrintable => (X_CUE, false),
            };
            if chunk.contd {
                if let Some(cue) = cue_name_for(&renum, chunk.bi) {
                    lines.push(PrintLine::text_line(X_CUE, format!("{cue} (CONT'D)"), true));
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
            let wrapped = wrap_lines(&text, FINAL_DRAFT_A4.col_chars(b.element_type));
            for (li, line) in wrapped.into_iter().enumerate() {
                let mut l = PrintLine::text_line(x, line, bold);
                l.right_aligned = matches!(b.element_type, ElementType::Transition);
                l.share_row = li == 0 && number_pushed;
                lines.push(l);
            }
            if chunk.more {
                lines.push(PrintLine {
                    right_aligned: true,
                    ..PrintLine::text_line(0.0, "(MORE)".to_string(), false)
                });
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
        pages.push(layout_title_page(meta));
    }
    pages.extend(layout_pages(p, meta, opts));
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
            if !line.text.is_empty() {
                let w = line.text.chars().count() as f32 * CHAR_MM;
                let x = if line.right_aligned {
                    let right = if line.text == "(MORE)" {
                        // (MORE) hugs the dialogue column's right edge.
                        DLG_RIGHT
                    } else {
                        TEXT_RIGHT
                    };
                    (right - w).max(MARGIN_L)
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
            let banded = lines.iter().filter(|l| l.is_page_number).count();
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
        // Character cue at 38 + 51.
        let cue = page.iter().find(|l| l.text == "IVAN").unwrap();
        assert!((cue.x_mm - X_CUE).abs() < 0.01);
        assert!(cue.bold);
        // Dialogue at 38 + 26.
        let dlg = page.iter().find(|l| l.text.starts_with("Hello")).unwrap();
        assert!((dlg.x_mm - X_DLG).abs() < 0.01);
        assert!(!dlg.bold);
        // Parenthetical at 38 + 36.
        let par = page.iter().find(|l| l.text == "(softly)").unwrap();
        assert!((par.x_mm - X_PAREN).abs() < 0.01);
        // Transition is right-aligned and not bold (FD template).
        let tr = page.iter().find(|l| l.text == "FADE OUT:").unwrap();
        assert!(tr.right_aligned && !tr.bold);
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
