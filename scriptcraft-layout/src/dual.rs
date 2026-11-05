//! Two-column dual-dialogue placement shared by the editor and PDF.
//!
//! A pair is two adjacent speech units (character cue plus parenthetical /
//! dialogue / lyrics). Each unit wraps inside its own column; rows are aligned
//! at the top and both columns share the same grid. Pagination consumes the
//! row list from here, so page breaks never disagree between editor and PDF.

use crate::wrap;
use scriptcraft_core::editing::TemplateMetrics;
use scriptcraft_core::model::{DualPair, ElementType, ScriptBlock};

/// Width of one dual-dialogue column. Two columns plus [`DUAL_GAP_MM`] equal
/// the 147.5 mm Russian text column.
pub const DUAL_COL_W_MM: f32 = 67.5;
/// Gap between the columns.
pub const DUAL_GAP_MM: f32 = 12.5;
/// Left inset of dialogue text inside its column.
const DIALOGUE_INSET_MM: f32 = 4.5;
/// Left inset of parentheticals inside their column.
const PARENTHETICAL_INSET_MM: f32 = 3.0;
/// Millimetres per character cell (0.6 em at 12 pt), mirroring `place`.
pub const DUAL_MM_PER_CELL: f32 = 2.54;

/// Char range of a body line within its source paragraph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceSpan {
    pub bi: usize,
    pub para: usize,
    /// Char offset (not bytes) into the paragraph.
    pub start: usize,
    pub end: usize,
}

/// One painted cell in a dual row.
#[derive(Clone, Debug, PartialEq)]
pub struct DualLine {
    pub text: String,
    /// Horizontal anchor in document millimetres from the text-column edge.
    pub x_mm: f32,
    pub span: Option<SourceSpan>,
    pub bold: bool,
    pub italic: bool,
}

/// One aligned row: left cell, right cell, or both.
#[derive(Clone, Debug, PartialEq)]
pub struct DualRow {
    pub left: Option<DualLine>,
    pub right: Option<DualLine>,
}

/// All aligned rows of a pair, plus each column's own line count.
#[derive(Clone, Debug, PartialEq)]
pub struct DualRows {
    pub rows: Vec<DualRow>,
    pub left_len: usize,
    pub right_len: usize,
}

/// Build the aligned rows for a validated pair.
pub fn build_rows(blocks: &[ScriptBlock], pair: DualPair, m: &TemplateMetrics) -> DualRows {
    let left = column_lines(blocks, pair.left, pair.left_end, 0.0, m);
    let right = column_lines(
        blocks,
        pair.right,
        pair.right_end,
        DUAL_COL_W_MM + DUAL_GAP_MM,
        m,
    );
    let left_len = left.len();
    let right_len = right.len();
    let rows = (0..left_len.max(right_len))
        .map(|i| DualRow {
            left: left.get(i).cloned(),
            right: right.get(i).cloned(),
        })
        .collect();
    DualRows {
        rows,
        left_len,
        right_len,
    }
}

fn column_lines(
    blocks: &[ScriptBlock],
    start: usize,
    end: usize,
    col_x: f32,
    m: &TemplateMetrics,
) -> Vec<DualLine> {
    let mut out = Vec::new();
    for bi in start..end {
        let Some(block) = blocks.get(bi) else {
            continue;
        };
        let (inset, italic) = match block.element_type {
            ElementType::Character => (0.0, false),
            ElementType::Parenthetical => (PARENTHETICAL_INSET_MM, true),
            ElementType::Lyrics => (DIALOGUE_INSET_MM, true),
            _ => (DIALOGUE_INSET_MM, false),
        };
        let cpl = (((DUAL_COL_W_MM - inset) / DUAL_MM_PER_CELL).floor() as usize).max(1);
        let bold = block.element_type == ElementType::Character;
        for (para, text) in block.text.split('\n').enumerate() {
            for line in wrap::wrap_paragraph(text, cpl) {
                let x_mm = if block.element_type == ElementType::Character {
                    // Centre the cue over its column, like a normal cue.
                    let width = line.text.chars().count() as f32 * DUAL_MM_PER_CELL;
                    col_x + ((DUAL_COL_W_MM - width) / 2.0).max(0.0)
                } else {
                    col_x + inset
                };
                out.push(DualLine {
                    text: line.text,
                    x_mm,
                    span: Some(SourceSpan {
                        bi,
                        para,
                        start: line.start,
                        end: line.end,
                    }),
                    bold,
                    italic,
                });
            }
        }
        // `m` is accepted for symmetry with the single-column planner and so
        // future per-template dual metrics can land here without changing the
        // paginator call site.
        let _ = m;
    }
    out
}

/// Centred x for a `(CONT'D)` cue in the left or right column.
pub fn cue_x_mm(text: &str, right: bool) -> f32 {
    let col_x = if right {
        DUAL_COL_W_MM + DUAL_GAP_MM
    } else {
        0.0
    };
    let width = text.chars().count() as f32 * DUAL_MM_PER_CELL;
    col_x + ((DUAL_COL_W_MM - width) / 2.0).max(0.0)
}

/// Right edge used by `(MORE)` in the left or right column.
pub fn more_x_mm(right: bool) -> f32 {
    if right {
        DUAL_COL_W_MM + DUAL_GAP_MM + DUAL_COL_W_MM - 1.0
    } else {
        DUAL_COL_W_MM - 1.0
    }
}
