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
    let mut pager = Pager::new(blocks, m);
    while pager.step() {}
    pager.finish()
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
