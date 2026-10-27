//! Fixed-page pagination: 1:1 port of `scriptcraft-core::editing`'s
//! `paginate_m` (and its helpers `is_atomic`, `is_anchor`, `following_lines`,
//! `needs_contd_cue`).
//!
//! Blocks become `Chunk`s (paragraph slices of one block); long
//! Action/Shot/Dialogue/Lyrics blocks split at paragraph boundaries with
//! (MORE)/(CONT'D) markers budgeted as content rows, anchors never strand at
//! page bottoms, and template top-space rows are only counted when the block
//! lands below other content. `tests/parity.rs` asserts chunk-for-chunk
//! equality with the old engine on the A95 fixture and 200 synthetic
//! documents under both templates; the editor must never paginate differently
//! than it used to.
//!
//! The engine is a stepwise state machine ([`Pager`]): one `step` places one
//! chunk. `place::relayout_from` drives it page by page after an edit and
//! stops as soon as a freshly built page equals the old layout's page — from
//! there on the old pages are provably unchanged.

use crate::wrap;
use scriptcraft_core::editing::TemplateMetrics;
use scriptcraft_core::model::{ElementType, ScriptBlock};
use scriptcraft_core::page_lock::ResolvedAnchor;

/// One rendered slice of a block: paragraphs `[start..end]`.
/// `more` marks a non-final chunk ((MORE) follows it);
/// `contd` marks a non-first dialogue/lyrics chunk (cue + (CONT'D) precedes it).
/// `top` is the number of template blank rows that precede the block
/// (only reported when the block starts on a non-empty page).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub bi: usize,
    pub start: usize,
    pub end: usize,
    pub more: bool,
    pub contd: bool,
    pub top: usize,
}

/// One fixed-size page: the chunks that landed on it, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Page {
    pub chunks: Vec<Chunk>,
}

/// Blocks that must stay whole on one page (never split mid-block).
fn is_atomic(t: ElementType) -> bool {
    matches!(
        t,
        ElementType::SceneHeading
            | ElementType::SceneCharacters
            | ElementType::Character
            | ElementType::Parenthetical
            | ElementType::Transition
            | ElementType::NonPrintable
    )
}

/// Anchors that may not strand at the bottom: they need the block plus at
/// least 2 following lines on the same page (cue + dialogue kept together).
fn is_anchor(t: ElementType) -> bool {
    matches!(
        t,
        ElementType::SceneHeading
            | ElementType::SceneCharacters
            | ElementType::Character
            | ElementType::Parenthetical
    )
}

/// Lines that must accompany an anchor on its page: the following block's
/// top space plus its first content unit — the whole block for atomic
/// types, the first paragraph for splittable ones (≥2 lines). A following
/// parenthetical additionally holds back the head of its dialogue so the
/// cue never strands alone at a page bottom.
fn following_lines(blocks: &[ScriptBlock], bi: usize, m: &TemplateMetrics) -> usize {
    let Some(next) = blocks.get(bi + 1) else {
        return 2;
    };
    fn unit(b: &ScriptBlock, m: &TemplateMetrics) -> usize {
        let cpl = m.col_chars(b.element_type);
        if is_atomic(b.element_type) {
            b.text
                .split('\n')
                .map(|p| wrap::row_count(p, cpl).max(1))
                .sum()
        } else {
            let first = b.text.split('\n').next().unwrap_or("");
            wrap::row_count(first, cpl).max(2)
        }
    }
    let mut need = m.top_space_lines(next.element_type) + unit(next, m);
    if next.element_type == ElementType::Parenthetical {
        if let Some(d) = blocks.get(bi + 2) {
            if matches!(d.element_type, ElementType::Dialogue | ElementType::Lyrics) {
                need += m.top_space_lines(d.element_type) + unit(d, m);
            }
        }
    }
    need.max(2)
}

/// Whether a continued chunk gets a "(CONT'D)" character cue above it.
fn needs_contd_cue(t: ElementType) -> bool {
    matches!(t, ElementType::Dialogue | ElementType::Lyrics)
}

/// [`paginate`] with the Russian template's line capacity.
pub fn paginate_lines(blocks: &[ScriptBlock], lines_per_page: usize) -> Vec<Page> {
    let m = TemplateMetrics {
        lines_per_page,
        ..scriptcraft_core::editing::RUSSIAN
    };
    paginate(blocks, &m)
}

/// Paginate blocks into fixed-size pages, splitting long Action/Shot/
/// Dialogue/Lyrics blocks at paragraph boundaries with (MORE)/(CONT'D)
/// markers. Anchors never strand at page bottoms.
pub fn paginate(blocks: &[ScriptBlock], m: &TemplateMetrics) -> Vec<Page> {
    paginate_with_breaks(blocks, m, &[])
}

/// Like [`paginate`], but force a fresh page before each
/// `(block index, paragraph offset)` in `breaks`. Breaks already at a page
/// start are no-ops (they never create blank pages).
pub fn paginate_with_breaks(
    blocks: &[ScriptBlock],
    m: &TemplateMetrics,
    breaks: &[(usize, usize)],
) -> Vec<Page> {
    let mut pager = Pager::new(blocks, m);
    pager.breaks = breaks.to_vec();
    while pager.step() {}
    pager.finish()
}

/// Page starts as `(block index, paragraph offset)`; used to capture locks.
pub fn page_starts(blocks: &[ScriptBlock], m: &TemplateMetrics) -> Vec<(usize, usize)> {
    paginate(blocks, m)
        .iter()
        .filter_map(|p| p.chunks.first().map(|c| (c.bi, c.start)))
        .collect()
}

/// Labels for physical pages under a page lock. Anchors that fell on a page
/// keep their original number; pages inserted before one get suffixes
/// (`42A`, `42B`, …); pages after the last anchor continue numbering.
pub fn page_labels(pages: &[Page], anchors: &[ResolvedAnchor]) -> Vec<String> {
    let mut labels = vec![String::new(); pages.len()];
    let mut assigned: Vec<(usize, u32)> = Vec::new();
    for a in anchors {
        let Some(pi) = page_of(pages, a.bi, a.para) else {
            continue;
        };
        if labels[pi].is_empty() {
            labels[pi] = a.page.to_string();
            assigned.push((pi, a.page));
        }
    }
    if assigned.is_empty() {
        return (1..=pages.len()).map(|n| n.to_string()).collect();
    }
    let (first_pi, first_base) = assigned[0];
    for (pi, label) in labels.iter_mut().enumerate().take(first_pi) {
        *label = format!("{first_base}{}", suffix(pi));
    }
    let mut prev_pi = first_pi;
    let mut prev_base = first_base;
    for &(pi, base) in assigned.iter().skip(1) {
        if pi <= prev_pi {
            continue;
        }
        for k in 0..(pi - prev_pi - 1) {
            labels[prev_pi + 1 + k] = format!("{prev_base}{}", suffix(k));
        }
        labels[pi] = base.to_string();
        prev_pi = pi;
        prev_base = base;
    }
    for k in 1..(pages.len() - prev_pi) {
        labels[prev_pi + k] = (prev_base as usize + k).to_string();
    }
    labels
}

fn page_of(pages: &[Page], bi: usize, para: usize) -> Option<usize> {
    pages.iter().position(|p| {
        p.chunks
            .iter()
            .any(|c| c.bi == bi && c.start <= para && para < c.end)
    })
}

/// A, B, … Z, AA, AB, …
fn suffix(mut n: usize) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (n % 26) as u8) as char);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    s
}

/// Stepwise pagination: `step` places one chunk (possibly after starting a
/// fresh page) and returns `false` once every block is consumed.
pub(crate) struct Pager<'a> {
    blocks: &'a [ScriptBlock],
    m: &'a TemplateMetrics,
    pages: Vec<Page>,
    used: usize,
    bi: usize,
    /// Paragraph cursor within `blocks[bi]` (splittable blocks only).
    start: usize,
    /// Whether the next chunk of `blocks[bi]` is its first.
    first_chunk: bool,
    /// Cached paragraph row counts for `blocks[bi]`.
    rows: Vec<usize>,
    /// Forced page starts `(block index, paragraph offset)`.
    breaks: Vec<(usize, usize)>,
    break_i: usize,
}

impl<'a> Pager<'a> {
    pub(crate) fn new(blocks: &'a [ScriptBlock], m: &'a TemplateMetrics) -> Self {
        Self {
            blocks,
            m,
            pages: vec![Page::default()],
            used: 0,
            bi: 0,
            start: 0,
            first_chunk: true,
            rows: Vec::new(),
            breaks: Vec::new(),
            break_i: 0,
        }
    }

    pub(crate) fn pages(&self) -> &[Page] {
        &self.pages
    }

    pub(crate) fn is_done(&self) -> bool {
        self.bi >= self.blocks.len()
    }

    /// Run until `limit` pages exist or the document is exhausted. A page is
    /// closed as soon as a later page exists, so callers compare complete
    /// pages once `pages().len() > index + 1` (or the pager is done).
    pub(crate) fn run_until(&mut self, limit: usize) {
        while !self.is_done() && self.pages.len() < limit {
            self.step();
        }
    }

    /// Pop the trailing empty page (only created while content was pending).
    pub(crate) fn trim_final_empty(&mut self) {
        if self.pages.len() > 1 && self.pages.last().map(|p| p.chunks.is_empty()) == Some(true) {
            self.pages.pop();
        }
    }

    pub(crate) fn finish(mut self) -> Vec<Page> {
        self.trim_final_empty();
        self.pages
    }

    fn page_empty(&self) -> bool {
        self.pages
            .last()
            .map(|p| p.chunks.is_empty())
            .unwrap_or(true)
    }

    /// Place one chunk; `false` when the document is exhausted.
    pub(crate) fn step(&mut self) -> bool {
        let Some(b) = self.blocks.get(self.bi) else {
            return false;
        };
        while self.break_i < self.breaks.len() && self.breaks[self.break_i] < (self.bi, self.start)
        {
            self.break_i += 1;
        }
        if self.break_i < self.breaks.len() && self.breaks[self.break_i] == (self.bi, self.start) {
            self.break_i += 1;
            if !self.page_empty() {
                self.pages.push(Page::default());
                self.used = 0;
            }
        }
        let t = b.element_type;
        if self.rows.is_empty() || self.start == 0 && self.first_chunk {
            let cpl = self.m.col_chars(t);
            self.rows = b
                .text
                .split('\n')
                .map(|p| wrap::row_count(p, cpl).max(1))
                .collect();
        }
        if is_atomic(t) {
            self.step_atomic(t);
        } else {
            self.step_splittable(t);
        }
        true
    }

    fn step_atomic(&mut self, t: ElementType) {
        let m = self.m;
        let total: usize = self.rows.iter().sum();
        if !self.page_empty() && self.used + m.top_space_lines(t) + total > m.lines_per_page {
            self.pages.push(Page::default());
            self.used = 0;
        }
        // Orphan guard: anchors keep the following atomic blocks plus
        // the first paragraph of the next splittable block on their
        // page — unless this is the last block (nothing to strand).
        let page_empty = self.page_empty();
        let need_here = if page_empty {
            total
        } else {
            m.top_space_lines(t) + total
        };
        let following = following_lines(self.blocks, self.bi, m);
        if is_anchor(t)
            && self.bi + 1 < self.blocks.len()
            && !page_empty
            && self.used + need_here + following > m.lines_per_page
        {
            self.pages.push(Page::default());
            self.used = 0;
        }
        let skip_top = self.page_empty();
        self.used += if skip_top {
            total
        } else {
            m.top_space_lines(t) + total
        };
        self.pages.last_mut().unwrap().chunks.push(Chunk {
            bi: self.bi,
            start: 0,
            end: self.rows.len(),
            more: false,
            contd: false,
            top: if skip_top { 0 } else { m.top_space_lines(t) },
        });
        self.bi += 1;
        self.start = 0;
        self.first_chunk = true;
        self.rows.clear();
    }

    fn step_splittable(&mut self, t: ElementType) {
        let m = self.m;
        let para_start = self.start;
        let page_empty = self.page_empty();
        let top_here = if self.first_chunk && !page_empty {
            m.top_space_lines(t)
        } else {
            0
        };
        let cue_here = if !self.first_chunk && needs_contd_cue(t) {
            1
        } else {
            0
        };
        let budget = m
            .lines_per_page
            .saturating_sub(self.used + top_here + cue_here);
        // Fit whole paragraphs.
        let mut end = para_start;
        let mut chunk_lines = 0usize;
        while end < self.rows.len() && chunk_lines + self.rows[end] <= budget {
            chunk_lines += self.rows[end];
            end += 1;
        }
        if end == para_start {
            if !page_empty {
                // Nothing fits here (markers included): fresh page, retry.
                self.pages.push(Page::default());
                self.used = 0;
                return;
            }
            // Oversized paragraph on an empty page: place it anyway.
            end = para_start + 1;
            chunk_lines = self.rows[para_start];
        }
        let needs_more = end < self.rows.len();
        if needs_more {
            // Reserve the (MORE) line, shedding trailing paragraphs first.
            // Budget already excludes top + cue, so content + 1 must fit.
            while end > para_start && chunk_lines + 1 > budget {
                end -= 1;
                chunk_lines -= self.rows[end];
            }
            if end == para_start {
                // Even one paragraph plus markers doesn't fit: if the
                // page already holds content, try the next page; else
                // place the paragraph anyway and let markers overflow.
                if !page_empty {
                    self.pages.push(Page::default());
                    self.used = 0;
                    return;
                }
                end = para_start + 1;
                chunk_lines = self.rows[para_start];
            }
        }
        self.used += top_here + cue_here + chunk_lines + usize::from(needs_more);
        let last_chunk = end == self.rows.len();
        // `more` is only true when content really continues.
        let more = needs_more && !last_chunk;
        // Top space counted iff this chunk landed on a non-empty page.
        let landed_on_empty = self.page_empty();
        self.pages.last_mut().unwrap().chunks.push(Chunk {
            bi: self.bi,
            start: para_start,
            end,
            more,
            contd: !self.first_chunk && needs_contd_cue(t),
            top: if self.first_chunk && !landed_on_empty {
                m.top_space_lines(t)
            } else {
                0
            },
        });
        self.start = end;
        if end == self.rows.len() {
            self.bi += 1;
            self.start = 0;
            self.first_chunk = true;
            self.rows.clear();
        } else {
            self.first_chunk = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scriptcraft_core::model::ScriptBlock;

    fn blocks(n: usize) -> Vec<ScriptBlock> {
        (0..n)
            .map(|i| ScriptBlock::new(ElementType::Action, format!("Paragraph {i}.")))
            .collect()
    }

    #[test]
    fn forced_break_starts_page_at_anchor() {
        let b = blocks(12);
        let m = TemplateMetrics {
            lines_per_page: 100,
            ..scriptcraft_core::editing::RUSSIAN
        };
        let plain = paginate(&b, &m);
        assert_eq!(plain.len(), 1);
        let pages = paginate_with_breaks(&b, &m, &[(6, 0)]);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[1].chunks[0].bi, 6);
    }

    #[test]
    fn break_at_existing_page_start_adds_no_blank_page() {
        let b = blocks(20);
        let m = TemplateMetrics {
            lines_per_page: 8,
            ..scriptcraft_core::editing::RUSSIAN
        };
        let plain = paginate(&b, &m);
        let second = plain[1].chunks[0].bi;
        let locked = paginate_with_breaks(&b, &m, &[(second, 0)]);
        assert_eq!(locked, plain);
    }

    #[test]
    fn page_starts_match_captured_chunks() {
        let b = blocks(30);
        let m = TemplateMetrics {
            lines_per_page: 6,
            ..scriptcraft_core::editing::RUSSIAN
        };
        let pages = paginate(&b, &m);
        let starts = page_starts(&b, &m);
        assert_eq!(starts.len(), pages.len());
        for (start, page) in starts.iter().zip(&pages) {
            assert_eq!(page.chunks[0].bi, start.0);
        }
    }

    #[test]
    fn labels_insert_a_b_after_locked_pages() {
        let b = blocks(30);
        let m = TemplateMetrics {
            lines_per_page: 4,
            ..scriptcraft_core::editing::RUSSIAN
        };
        let pages = paginate(&b, &m);
        assert!(pages.len() >= 4);
        let anchors = vec![
            ResolvedAnchor {
                bi: 0,
                para: 0,
                page: 1,
            },
            ResolvedAnchor {
                bi: pages[1].chunks[0].bi,
                para: 0,
                page: 2,
            },
            ResolvedAnchor {
                bi: pages[3].chunks[0].bi,
                para: 0,
                page: 3,
            },
        ];
        let labels = page_labels(&pages, &anchors);
        assert_eq!(labels[0], "1");
        assert_eq!(labels[1], "2");
        assert_eq!(labels[2], "2A");
        assert_eq!(labels[3], "3");
        if pages.len() > 4 {
            assert_eq!(labels[4], "4");
        }
    }

    #[test]
    fn suffix_rolls_over() {
        assert_eq!(suffix(0), "A");
        assert_eq!(suffix(1), "B");
        assert_eq!(suffix(25), "Z");
        assert_eq!(suffix(26), "AA");
        assert_eq!(suffix(27), "AB");
    }
}
