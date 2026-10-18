use crate::model::{ElementType, Project, ScriptBlock};

/// KIT-style Enter behavior: pressing Enter at the end of a block of the
/// given type starts a new block of the returned type.
pub fn enter_next(t: ElementType) -> ElementType {
    match t {
        ElementType::SceneHeading => ElementType::Action,
        ElementType::SceneCharacters => ElementType::Action,
        ElementType::Action => ElementType::Action,
        ElementType::Character => ElementType::Dialogue,
        ElementType::Parenthetical => ElementType::Dialogue,
        ElementType::Dialogue => ElementType::Action,
        ElementType::Shot => ElementType::Action,
        ElementType::Transition => ElementType::SceneHeading,
        ElementType::NonPrintable => ElementType::Action,
        ElementType::Lyrics => ElementType::Lyrics,
    }
}

/// Tab cycles forward through the main writing order (side buttons reach
/// the secondary types: scene characters, shot, non-printable, lyrics).
pub fn tab_next(t: ElementType) -> ElementType {
    match t {
        ElementType::SceneHeading => ElementType::Action,
        ElementType::Action => ElementType::Character,
        ElementType::Character => ElementType::Dialogue,
        ElementType::Dialogue => ElementType::Parenthetical,
        ElementType::Parenthetical => ElementType::Transition,
        ElementType::Transition => ElementType::SceneHeading,
        ElementType::SceneCharacters
        | ElementType::Shot
        | ElementType::NonPrintable
        | ElementType::Lyrics => ElementType::Action,
    }
}

/// Shift+Tab cycles backward.
pub fn tab_prev(t: ElementType) -> ElementType {
    match t {
        ElementType::SceneHeading => ElementType::Transition,
        ElementType::Action => ElementType::SceneHeading,
        ElementType::Character => ElementType::Action,
        ElementType::Dialogue => ElementType::Character,
        ElementType::Parenthetical => ElementType::Dialogue,
        ElementType::Transition => ElementType::Parenthetical,
        ElementType::SceneCharacters
        | ElementType::Shot
        | ElementType::NonPrintable
        | ElementType::Lyrics => ElementType::Action,
    }
}

/// Character cue without the `(ПРОД.)` continuation extension.
pub fn base_character_name(cue: &str) -> &str {
    cue.strip_suffix(" (ПРОД.)").unwrap_or(cue)
}

/// Apply an autocomplete pick: replace the partial token before `caret`
/// (a char index into `text`) with the full suggestion. Everything after
/// the caret is preserved.
pub fn complete_at(text: &str, caret: usize, ins: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let caret = caret.min(chars.len());
    // Token start: after the last whitespace or line start.
    let mut tok = caret;
    while tok > 0 && !chars[tok - 1].is_whitespace() {
        tok -= 1;
    }
    let mut out: String = chars[..tok].iter().collect();
    out.push_str(ins);
    out.extend(chars[caret..].iter());
    out
}

/// Apply an autocomplete pick to a whole line: replace the text between the
/// start of the caret's line and `caret` with `ins`, preserving everything
/// after the caret. Scene-heading suggestions are full lines (cue +
/// location), so they must replace the line rather than the last token.
pub fn complete_line_at(text: &str, caret: usize, ins: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let caret = caret.min(chars.len());
    let line_start = chars[..caret]
        .iter()
        .rposition(|&c| c == '\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let mut out: String = chars[..line_start].iter().collect();
    out.push_str(ins);
    out.extend(chars[caret..].iter());
    out
}

/// Character cue owning a dialogue/lyrics block: nearest Character above it
/// (skipping parentheticals/dialogue), without a `(ПРОД.)` extension.
pub fn cue_name_for(blocks: &[ScriptBlock], bi: usize) -> Option<String> {
    let mut j = bi;
    while j > 0 {
        j -= 1;
        match blocks.get(j)?.element_type {
            ElementType::Character => {
                return Some(base_character_name(&blocks[j].text).to_string())
            }
            ElementType::Parenthetical | ElementType::Dialogue | ElementType::Lyrics => continue,
            _ => return None,
        }
    }
    None
}

/// The visible text of a page chunk (paragraph slice joined back up).
pub fn chunk_text(blocks: &[ScriptBlock], bi: usize, start: usize, end: usize) -> String {
    blocks
        .get(bi)
        .map(|b| {
            b.text
                .split('\n')
                .skip(start)
                .take(end.saturating_sub(start))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// Splice edited chunk text back into the block's paragraphs.
pub fn replace_chunk(
    blocks: &mut [ScriptBlock],
    bi: usize,
    start: usize,
    end: usize,
    new_text: &str,
) -> bool {
    let Some(b) = blocks.get_mut(bi) else {
        return false;
    };
    let mut paras: Vec<String> = b.text.split('\n').map(|s| s.to_string()).collect();
    let end = end.min(paras.len());
    let start = start.min(end);
    let fresh: Vec<String> = new_text.split('\n').map(|s| s.to_string()).collect();
    paras.splice(start..end, fresh);
    // Trim a single trailing empty paragraph KIT would never store.
    while paras.len() > 1 && paras.last().map(|s| s.is_empty()).unwrap_or(false) {
        paras.pop();
    }
    let joined = paras.join("\n");
    if joined == b.text {
        return false;
    }
    b.text = joined;
    true
}

/// Locations seen in existing scene headings, e.g. `EXT. ПАРКИНГ - NIGHT`
/// yields `ПАРКИНГ`. Order of first appearance, deduplicated.
pub fn known_locations(p: &Project) -> Vec<String> {
    let mut out = Vec::new();
    for scene in p.scenes() {
        let mut h = scene.heading.as_str();
        for prefix in ["INT./EXT. ", "INT. ", "EXT. "] {
            if let Some(rest) = h.strip_prefix(prefix) {
                h = rest;
                break;
            }
        }
        let loc = match h.rsplit_once(" - ") {
            Some((loc, _)) => loc.trim(),
            None => h.trim(),
        };
        if !loc.is_empty() && !out.iter().any(|s| s == loc) {
            out.push(loc.to_string());
        }
    }
    out
}

const TRANSITIONS: &[&str] = &[
    "CUT TO:",
    "FADE IN:",
    "FADE OUT:",
    "DISSOLVE TO:",
    "FADE TO BLACK",
];

/// Autocomplete candidates for `element_type` given the current `prefix`.
/// Capped, most useful first.
pub fn candidates(p: &Project, element_type: ElementType, prefix: &str) -> Vec<String> {
    const CAP: usize = 8;
    match element_type {
        ElementType::Character => {
            let up = prefix.trim().to_uppercase();
            let mut out = Vec::new();
            for cue in p.characters() {
                let base = base_character_name(&cue).to_string();
                if !out.contains(&base) && (up.is_empty() || base.starts_with(&up as &str)) {
                    out.push(base);
                }
                if out.len() >= CAP {
                    break;
                }
            }
            out
        }
        ElementType::SceneHeading => {
            let up = prefix.to_uppercase();
            let mut out = Vec::new();
            for pre in ["INT. ", "EXT. ", "INT./EXT. "] {
                if pre.starts_with(&up as &str) {
                    out.push(pre.to_string());
                }
            }
            // Complete the location after a cue. Longest cue first so
            // "INT./EXT. " is not mistaken for "INT. ". Match only the text
            // already typed after the cue, never re-emit the whole line —
            // that is what used to produce "EXT. ПАРКИНГПАРКИНГ…".
            if let Some(cue) = ["INT./EXT. ", "INT. ", "EXT. "]
                .iter()
                .find(|c| up.starts_with(**c))
            {
                let typed = &up[cue.len()..];
                // The " - TIME" part begins: no location suggestions.
                if !typed.contains(" - ") {
                    for loc in known_locations(p) {
                        let loc_up = loc.to_uppercase();
                        if loc_up.len() > typed.len() && loc_up.starts_with(typed) {
                            let suggestion = format!("{cue}{loc}");
                            if !out.iter().any(|s| s.eq_ignore_ascii_case(&suggestion)) {
                                out.push(suggestion);
                            }
                            if out.len() >= CAP {
                                break;
                            }
                        }
                    }
                }
            }
            // Existing headings starting with the prefix.
            if out.len() < CAP {
                for scene in p.scenes() {
                    if scene.heading.to_uppercase().starts_with(&up as &str)
                        && scene.heading.len() > prefix.len()
                        && !out.iter().any(|s| s.eq_ignore_ascii_case(&scene.heading))
                    {
                        out.push(scene.heading.clone());
                    }
                    if out.len() >= CAP {
                        break;
                    }
                }
            }
            out
        }
        ElementType::Transition => TRANSITIONS
            .iter()
            .filter(|t| t.starts_with(&prefix.to_uppercase() as &str))
            .take(CAP)
            .map(|s| s.to_string())
            .collect(),
        ElementType::Parenthetical => {
            let mut out = Vec::new();
            for b in &p.blocks {
                if b.element_type == ElementType::Parenthetical
                    && b.text.starts_with(prefix)
                    && !out.contains(&b.text)
                {
                    out.push(b.text.clone());
                }
                if out.len() >= CAP {
                    break;
                }
            }
            out
        }
        ElementType::Action | ElementType::Dialogue => Vec::new(),
        ElementType::Shot
        | ElementType::SceneCharacters
        | ElementType::NonPrintable
        | ElementType::Lyrics => Vec::new(),
    }
}

/// Monospace advance factor: Courier-class faces are 0.6em.
pub const MONO_ADVANCE: f32 = 0.6;

/// Greedy word wrap into concrete lines (blank paragraphs stay blank).
/// Single source of truth for both the paginator and the PDF renderer —
/// and, by matching column widths, for the on-screen editor.
pub fn wrap_lines(text: &str, chars_per_line: usize) -> Vec<String> {
    let cpl = chars_per_line.max(1);
    let mut out = Vec::new();
    for para in text.split('\n') {
        if para.trim().is_empty() {
            out.push(String::new());
            continue;
        }
        let mut line = String::new();
        let mut len = 0usize;
        for w in para.split_whitespace() {
            push_word(&mut out, &mut line, &mut len, w, cpl);
        }
        out.push(line);
    }
    out
}

/// Greedy placement of one word, breaking at internal hyphens (Qt/KIT
/// break "по-малко" as "по-" + "малко" when the whole word doesn't fit).
fn push_word(out: &mut Vec<String>, line: &mut String, len: &mut usize, w: &str, cpl: usize) {
    let wl = w.chars().count();
    let space = usize::from(!line.is_empty());
    if *len + space + wl <= cpl {
        if space == 1 {
            line.push(' ');
        }
        line.push_str(w);
        *len += space + wl;
        return;
    }
    let pieces = hyphen_pieces(w);
    if pieces.len() > 1 {
        for (k, piece) in pieces.iter().enumerate() {
            // Only the first piece takes a leading space; the rest rejoin
            // directly after the hyphen ("10-" + "литровите").
            let space = if k == 0 {
                usize::from(!line.is_empty())
            } else {
                0
            };
            let pl = piece.chars().count();
            if *len + space + pl <= cpl {
                if space == 1 {
                    line.push(' ');
                }
                line.push_str(piece);
                *len += space + pl;
            } else {
                if !line.is_empty() {
                    out.push(std::mem::take(line));
                }
                line.push_str(piece);
                *len = pl;
            }
        }
        return;
    }
    // No break opportunity: move the word to the next line whole.
    if !line.is_empty() {
        out.push(std::mem::take(line));
        *len = 0;
    }
    line.push_str(w);
    *len = wl;
}

/// Split a word after each internal hyphen: "по-малко" -> ["по-", "малко"].
fn hyphen_pieces(w: &str) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut cur = String::new();
    for ch in w.chars() {
        cur.push(ch);
        if ch == '-' {
            pieces.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        pieces.push(cur);
    }
    if pieces.is_empty() {
        pieces.push(w.to_string());
    }
    pieces
}

/// Russian-template column metrics as fractions of the text width
/// (text = A4 210 − left 37.5 − right 25 = 147.5mm). The app renders
/// pixel columns from these same fractions, so layout always matches.
pub const FRAC_CHARACTER_OFF: f32 = 67.5 / 147.5;
pub const FRAC_PAREN_OFF: f32 = 55.0 / 147.5;
pub const FRAC_PAREN_W: f32 = 47.5 / 147.5;
pub const FRAC_DIALOG_W: f32 = 72.5 / 147.5;
pub const FRAC_DIALOG_OFF: f32 = (1.0 - FRAC_DIALOG_W) / 2.0;
pub const FRAC_LYRICS_OFF: f32 = 37.5 / 147.5;
pub const FRAC_LYRICS_W: f32 = 110.0 / 147.5;

/// Column width in characters (58 = full 147.5mm line at 10cpi Courier).
pub fn col_chars(t: ElementType) -> usize {
    RUSSIAN.col_chars(t)
}

/// Blank lines before a block (template `top_space`; suppressed at page top).
pub fn top_space_lines(t: ElementType) -> usize {
    RUSSIAN.top_space_lines(t)
}

/// Pagination metrics of one KIT `.kitss` template: column budgets in
/// characters, per-block blank lines and the page line capacity.
#[derive(Clone, Copy, Debug)]
pub struct TemplateMetrics {
    pub full: usize,
    pub cue: usize,
    pub paren: usize,
    pub dialog: usize,
    pub lyrics: usize,
    pub head_space: usize,
    pub scenechars_space: usize,
    pub action_space: usize,
    pub char_space: usize,
    pub trans_space: usize,
    pub note_space: usize,
    pub lines_per_page: usize,
}

impl TemplateMetrics {
    pub fn col_chars(&self, t: ElementType) -> usize {
        match t {
            ElementType::SceneHeading
            | ElementType::SceneCharacters
            | ElementType::Action
            | ElementType::Shot
            | ElementType::Transition => self.full,
            ElementType::Character | ElementType::NonPrintable => self.cue,
            ElementType::Parenthetical => self.paren,
            ElementType::Dialogue => self.dialog,
            ElementType::Lyrics => self.lyrics,
        }
    }

    pub fn top_space_lines(&self, t: ElementType) -> usize {
        match t {
            ElementType::SceneHeading => self.head_space,
            ElementType::SceneCharacters => self.scenechars_space,
            ElementType::Action | ElementType::Shot => self.action_space,
            ElementType::Character => self.char_space,
            ElementType::Transition => self.trans_space,
            ElementType::NonPrintable => self.note_space,
            ElementType::Parenthetical | ElementType::Dialogue | ElementType::Lyrics => 0,
        }
    }
}

/// The Russian template the editor renders (147.5mm column, 54 lines).
pub const RUSSIAN: TemplateMetrics = TemplateMetrics {
    full: 58,
    cue: 31,
    paren: 18,
    dialog: 28,
    lyrics: 43,
    head_space: 1,
    scenechars_space: 1,
    action_space: 1,
    char_space: 1,
    trans_space: 1,
    note_space: 1,
    lines_per_page: 54,
};

/// KIT's default export template `final_draft_a4.kitss`: 38/24/17/24mm
/// margins (155mm column = 61 chars), 2 blank lines before scene
/// headings, no blank before scene characters, 58 lines per page.
pub const FINAL_DRAFT_A4: TemplateMetrics = TemplateMetrics {
    full: 61,
    cue: 38,
    paren: 26,
    dialog: 35,
    lyrics: 50,
    head_space: 2,
    scenechars_space: 0,
    action_space: 1,
    char_space: 1,
    trans_space: 1,
    note_space: 1,
    lines_per_page: 58,
};

/// A4 lines-per-page at 12pt Courier, derived from the template geometry
/// (259.5mm text height, 0.6em advance, measured row factor 2320/2048):
/// floor(259.5/147.5 × 58×0.6×2048/2320) = 54.
pub const A4_LINES_PER_PAGE: usize = 54;

/// One rendered slice of a block: paragraphs `[start..end]`.
/// `more` marks a non-final chunk ((MORE) follows it);
/// `contd` marks a non-first dialogue/lyrics chunk (cue + (CONT'D) precedes it).
/// `top` is the number of template blank rows that precede the block
/// (only reported when the block starts on a non-empty page).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageChunk {
    pub bi: usize,
    pub start: usize,
    pub end: usize,
    pub more: bool,
    pub contd: bool,
    pub top: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Page {
    pub chunks: Vec<PageChunk>,
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
            | ElementType::Character
            | ElementType::SceneCharacters
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
                .map(|p| wrap_rows_chars(p, cpl).max(1))
                .sum()
        } else {
            let first = b.text.split('\n').next().unwrap_or("");
            wrap_rows_chars(first, cpl).max(2)
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

/// Paginate blocks into fixed-size pages, splitting long Action/Shot/
/// Dialogue/Lyrics blocks at paragraph boundaries with (MORE)/(CONT'D)
/// markers. Anchors never strand at page bottoms.
pub fn paginate(blocks: &[ScriptBlock], lines_per_page: usize) -> Vec<Page> {
    let m = TemplateMetrics {
        lines_per_page,
        ..RUSSIAN
    };
    paginate_m(blocks, &m)
}

/// [`paginate`] with explicit template metrics (used by PDF export).
pub fn paginate_m(blocks: &[ScriptBlock], m: &TemplateMetrics) -> Vec<Page> {
    let mut pages: Vec<Page> = vec![Page::default()];
    let mut used = 0usize;
    let n = blocks.len();

    for (bi, b) in blocks.iter().enumerate() {
        let t = b.element_type;
        let paras: Vec<&str> = b.text.split('\n').collect();
        let cpl = m.col_chars(t);
        let para_lines: Vec<usize> = paras
            .iter()
            .map(|p| wrap_rows_chars(p, cpl).max(1))
            .collect();

        if is_atomic(t) {
            let page_empty = pages.last().map(|p| p.chunks.is_empty()).unwrap_or(true);
            let total: usize = para_lines.iter().sum();
            if !page_empty && used + m.top_space_lines(t) + total > m.lines_per_page {
                pages.push(Page::default());
                used = 0;
            }
            // Orphan guard: anchors keep the following atomic blocks plus
            // the first paragraph of the next splittable block on their
            // page — unless this is the last block (nothing to strand).
            let page_empty = pages.last().map(|p| p.chunks.is_empty()).unwrap_or(true);
            let need_here = if page_empty {
                total
            } else {
                m.top_space_lines(t) + total
            };
            let following = following_lines(blocks, bi, m);
            if is_anchor(t)
                && bi + 1 < n
                && !page_empty
                && used + need_here + following > m.lines_per_page
            {
                pages.push(Page::default());
                used = 0;
            }
            let skip_top = pages.last().map(|p| p.chunks.is_empty()).unwrap_or(true);
            used += if skip_top {
                total
            } else {
                m.top_space_lines(t) + total
            };
            pages.last_mut().unwrap().chunks.push(PageChunk {
                bi,
                start: 0,
                end: paras.len(),
                more: false,
                contd: false,
                top: if skip_top { 0 } else { m.top_space_lines(t) },
            });
            continue;
        }

        // Splittable block: lay out paragraph by paragraph. (MORE) and
        // (CONT'D) cue lines are budgeted like content lines so rendered
        // pages never overflow their fixed size.
        let mut start = 0usize;
        let mut first_chunk = true;
        while start < paras.len() {
            let page_empty = pages.last().map(|p| p.chunks.is_empty()).unwrap_or(true);
            let top_here = if first_chunk && !page_empty {
                m.top_space_lines(t)
            } else {
                0
            };
            let cue_here = if !first_chunk && needs_contd_cue(t) {
                1
            } else {
                0
            };
            let mut budget = m.lines_per_page.saturating_sub(used + top_here + cue_here);
            // Fit whole paragraphs.
            let mut end = start;
            let mut chunk_lines = 0usize;
            while end < paras.len() && chunk_lines + para_lines[end] <= budget {
                chunk_lines += para_lines[end];
                end += 1;
            }
            if end == start {
                if !page_empty {
                    // Nothing fits here (markers included): fresh page, retry.
                    pages.push(Page::default());
                    used = 0;
                    continue;
                }
                // Oversized paragraph on an empty page: place it anyway.
                budget = budget.max(chunk_lines + para_lines[start]);
                end = start + 1;
                chunk_lines = para_lines[start];
            }
            let needs_more = end < paras.len();
            if needs_more {
                // Reserve the (MORE) line, shedding trailing paragraphs first.
                // Budget already excludes top + cue, so content + 1 must fit.
                while end > start && chunk_lines + 1 > budget {
                    end -= 1;
                    chunk_lines -= para_lines[end];
                }
                if end == start {
                    // Even one paragraph plus markers doesn't fit: if the
                    // page already holds content, try the next page; else
                    // place the paragraph anyway and let markers overflow.
                    if !page_empty {
                        pages.push(Page::default());
                        used = 0;
                        continue;
                    }
                    end = start + 1;
                    chunk_lines = para_lines[start];
                }
            }
            used += top_here + cue_here + chunk_lines + usize::from(needs_more);
            let last_chunk = end == paras.len();
            // `more` is only true when content really continues.
            let more = needs_more && !last_chunk;
            // Top space counted iff this chunk landed on a non-empty page.
            let landed_on_empty = pages.last().map(|p| p.chunks.is_empty()).unwrap_or(true);
            pages.last_mut().unwrap().chunks.push(PageChunk {
                bi,
                start,
                end,
                more,
                contd: !first_chunk && needs_contd_cue(t),
                top: if first_chunk && !landed_on_empty {
                    m.top_space_lines(t)
                } else {
                    0
                },
            });
            start = end;
            first_chunk = false;
        }
    }

    if pages.len() > 1 && pages.last().map(|p| p.chunks.is_empty()).unwrap_or(false) {
        pages.pop();
    }
    pages
}

/// How many editor rows a text needs inside a column of `col_width_px`
/// at `font_pt`. Greedy word wrap, matching the on-screen layout.
pub fn wrap_rows(text: &str, col_width_px: f32, font_pt: f32) -> usize {
    let per_line = (col_width_px / (MONO_ADVANCE * font_pt).max(1.0)).floor() as usize;
    wrap_rows_chars(text, per_line.max(8))
}

/// Same as [`wrap_rows`] but with a fixed character budget (used by the
/// paginator so layout never depends on zoom or pixels).
pub fn wrap_rows_chars(text: &str, chars_per_line: usize) -> usize {
    wrap_lines(text, chars_per_line).len().max(1)
}

/// Format seconds as KIT does in the navigator: `(m:ss)`.
pub fn format_duration(total_secs: f64) -> String {
    let s = total_secs.max(0.0).round() as u64;
    format!("({}:{:02})", s / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ScriptBlock;

    fn sample() -> Project {
        let mut p = Project {
            title: "T".into(),
            blocks: vec![],
        };
        for (t, text) in [
            (ElementType::SceneHeading, "EXT. PARK - DAY"),
            (ElementType::Character, "IVAN"),
            (ElementType::Dialogue, "Hi."),
            (ElementType::Character, "IVAN (ПРОД.)"),
            (ElementType::Dialogue, "Again."),
            (ElementType::SceneHeading, "INT. CAR - NIGHT"),
        ] {
            p.blocks.push(ScriptBlock::new(t, text.into()));
        }
        p
    }

    #[test]
    fn enter_transitions() {
        use ElementType::*;
        assert_eq!(enter_next(SceneHeading), Action);
        assert_eq!(enter_next(Character), Dialogue);
        assert_eq!(enter_next(Parenthetical), Dialogue);
        assert_eq!(enter_next(Dialogue), Action);
        assert_eq!(enter_next(Transition), SceneHeading);
        assert_eq!(enter_next(Lyrics), Lyrics);
        assert_eq!(enter_next(Shot), Action);
        assert_eq!(enter_next(SceneCharacters), Action);
        assert_eq!(enter_next(NonPrintable), Action);
    }

    #[test]
    fn tab_cycles_both_ways() {
        use ElementType::*;
        let order = [
            SceneHeading,
            Action,
            Character,
            Dialogue,
            Parenthetical,
            Transition,
        ];
        for w in order.windows(2) {
            assert_eq!(tab_next(w[0]), w[1]);
            assert_eq!(tab_prev(w[1]), w[0]);
        }
        assert_eq!(tab_next(Transition), SceneHeading);
        assert_eq!(tab_prev(SceneHeading), Transition);
    }

    #[test]
    fn character_candidates_dedup_prolongation() {
        let p = sample();
        let c = candidates(&p, ElementType::Character, "");
        assert_eq!(c, vec!["IVAN".to_string()]);
        assert!(candidates(&p, ElementType::Character, "IVA").contains(&"IVAN".to_string()));
        assert!(candidates(&p, ElementType::Character, "X").is_empty());
    }

    #[test]
    fn heading_candidates_prefixes_and_locations() {
        let p = sample();
        let c = candidates(&p, ElementType::SceneHeading, "");
        assert!(c.contains(&"INT. ".to_string()));
        assert!(c.contains(&"EXT. ".to_string()));
        let loc = candidates(&p, ElementType::SceneHeading, "EXT. ");
        assert!(loc.iter().any(|s| s.contains("PARK")));
    }

    #[test]
    fn scene_heading_location_pick_does_not_duplicate_typed_text() {
        let p = sample();
        let sugg = candidates(&p, ElementType::SceneHeading, "EXT. P");
        assert_eq!(sugg.first().map(String::as_str), Some("EXT. PARK"));
        assert!(
            sugg.iter().all(|s| !s.contains("PPARK")),
            "suggestion must not repeat the typed location prefix: {sugg:?}"
        );
        let done = complete_line_at("EXT. P", 6, &sugg[0]);
        assert_eq!(done, "EXT. PARK");
        assert_eq!(done.matches("EXT.").count(), 1);
        // No location suggestions once the " - TIME" part began.
        let time = candidates(&p, ElementType::SceneHeading, "EXT. PARK - D");
        assert!(time.iter().all(|s| s.starts_with("EXT. PARK - D")));
        assert!(time.iter().all(|s| !s.contains("EXT. EXT.")));
    }

    #[test]
    fn known_locations_parses() {
        let p = sample();
        assert_eq!(
            known_locations(&p),
            vec!["PARK".to_string(), "CAR".to_string()]
        );
    }

    #[test]
    fn wrap_rows_counts_wrapped_lines() {
        // 0.6em advance at 10pt → 6px/char; 60px column → 10 chars/line.
        assert_eq!(wrap_rows("hello world", 60.0, 10.0), 2);
        assert_eq!(wrap_rows("hi", 60.0, 10.0), 1);
        assert_eq!(wrap_rows("a\n\nb", 60.0, 10.0), 3);
        assert_eq!(wrap_rows("", 60.0, 10.0), 1);
    }

    #[test]
    fn wrap_breaks_at_internal_hyphens_like_qt() {
        // KIT (Qt) fills the line up to the last hyphen that fits.
        assert_eq!(
            wrap_lines("Осем часа сутринта. Стотинка по-малко да е", 35),
            vec!["Осем часа сутринта. Стотинка по-", "малко да е"]
        );
        // "10-литровите" as the 10-char tail of a dialogue line.
        assert_eq!(
            wrap_lines("xxxxxxxxxxxxxxxxxx 10-литровите", 24),
            vec!["xxxxxxxxxxxxxxxxxx 10-", "литровите"]
        );
        // A word without hyphens moves whole to the next line.
        assert_eq!(wrap_lines("aaa bbbbbb", 6), vec!["aaa", "bbbbbb"]);
        // Hyphenated word that fits whole is never split.
        assert_eq!(wrap_lines("aaa b-c", 7), vec!["aaa b-c"]);
        // Pieces rejoin without a space when the line breaks before them.
        assert_eq!(
            wrap_lines("aaaaaaaa 10-литрови", 10),
            vec!["aaaaaaaa", "10-литрови"]
        );
    }

    #[test]
    fn duration_formats_like_kit() {
        assert_eq!(format_duration(65.0), "(1:05)");
        assert_eq!(format_duration(20.0), "(0:20)");
        assert_eq!(format_duration(1074.0), "(17:54)");
    }

    #[test]
    fn column_widths_match_template() {
        use ElementType::*;
        assert_eq!(col_chars(SceneHeading), 58);
        assert_eq!(col_chars(Character), 31);
        assert_eq!(col_chars(Parenthetical), 18);
        assert_eq!(col_chars(Dialogue), 28);
        assert_eq!(col_chars(Lyrics), 43);
    }

    fn flat_indexes(pages: &[Page]) -> Vec<usize> {
        pages
            .iter()
            .flat_map(|p| p.chunks.iter().map(|c| c.bi))
            .collect()
    }

    #[test]
    fn paginate_keeps_anchors_off_page_bottom() {
        use ElementType::*;
        let mut blocks = Vec::new();
        // 52 zero-top-space one-liners leave exactly 2 lines spare…
        for i in 0..52 {
            blocks.push(ScriptBlock::new(Parenthetical, format!("(beat {i})")));
        }
        // …so a heading needing the 53rd line is pushed to page two
        // (anchors require ≥2 spare lines: themselves + one content line).
        blocks.push(ScriptBlock::new(
            SceneHeading,
            "EXT. ELSEWHERE - DAY".into(),
        ));
        blocks.push(ScriptBlock::new(Action, "More.".into()));
        let pages = paginate(&blocks, 54);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].chunks.len(), 52);
        assert_eq!(flat_indexes(&pages), (0..54).collect::<Vec<_>>());
        assert_eq!(pages[1].chunks[0].bi, 52);
        assert!(!pages[1].chunks[0].contd && !pages[1].chunks[0].more);
    }

    #[test]
    fn paginate_overflow_breaks_pages() {
        use ElementType::*;
        // Actions cost top_space 1 + 1 text line each: 27 fill a 54-line page.
        let blocks: Vec<ScriptBlock> = (0..30)
            .map(|i| ScriptBlock::new(Action, format!("line {i}")))
            .collect();
        let pages = paginate(&blocks, 54);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].chunks.len(), 27);
        assert_eq!(pages[1].chunks.len(), 3);
    }

    #[test]
    fn complete_at_replaces_partial_token() {
        assert_eq!(complete_at("ИВА", 3, "ИВАН"), "ИВАН");
        assert_eq!(complete_at("hello ВА", 8, "ВАНКО"), "hello ВАНКО");
        assert_eq!(complete_at("AB XCD", 4, "XYZ"), "AB XYZCD");
        // Caret mid-word keeps the tail.
        assert_eq!(complete_at("helo world", 4, "hello"), "hello world");
    }

    #[test]
    fn complete_line_at_replaces_whole_line() {
        assert_eq!(complete_line_at("EXT. P", 6, "EXT. PARK"), "EXT. PARK");
        // Text on later lines and after the caret survives.
        assert_eq!(
            complete_line_at("EXT. P\nAction.", 6, "EXT. PARK"),
            "EXT. PARK\nAction."
        );
        assert_eq!(
            complete_line_at("Intro.\nEXT. P", 13, "EXT. PARK"),
            "Intro.\nEXT. PARK"
        );
    }

    #[test]
    fn chunk_text_and_replace_round_trip() {
        use ElementType::*;
        let mut blocks = vec![
            ScriptBlock::new(SceneHeading, "EXT. PARK - DAY".into()),
            ScriptBlock::new(Action, "A.\nB.\nC.".into()),
            ScriptBlock::new(Character, "IVAN".into()),
            ScriptBlock::new(Dialogue, "Hi.".into()),
        ];
        assert_eq!(chunk_text(&blocks, 1, 1, 3), "B.\nC.");
        assert!(replace_chunk(&mut blocks, 1, 1, 3, "B2.\nC2.\nD2."));
        assert_eq!(blocks[1].text, "A.\nB2.\nC2.\nD2.");
        assert!(!replace_chunk(&mut blocks, 3, 0, 1, "Hi."));
        assert_eq!(cue_name_for(&blocks, 3), Some("IVAN".to_string()));
        assert_eq!(cue_name_for(&blocks, 0), None);
    }

    #[test]
    fn paginate_splits_long_dialogue_with_markers() {
        use ElementType::*;
        // 60 one-line paragraphs: 53 + (MORE) fill page one, the rest
        // follows with a (CONT'D) cue on page two.
        let text = (0..60)
            .map(|i| format!("w{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let blocks = vec![ScriptBlock::new(Dialogue, text)];
        let pages = paginate(&blocks, 54);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].chunks.len(), 1);
        assert_eq!(pages[1].chunks.len(), 1);
        let (c0, c1) = (&pages[0].chunks[0], &pages[1].chunks[0]);
        assert_eq!((c0.start, c0.end), (0, 53));
        assert_eq!((c1.start, c1.end), (53, 60));
        assert!(c0.more && !c0.contd);
        assert!(!c1.more && c1.contd);
    }

    #[test]
    fn paginate_keeps_parenthetical_with_dialogue() {
        use ElementType::*;
        // Fill 53 lines, then a parenthetical: only 1 line would remain,
        // so it (and its dialogue) move to the next page together.
        // (Parentheticals are anchors too: they need ≥2 spare lines.)
        let mut blocks = Vec::new();
        for i in 0..53 {
            blocks.push(ScriptBlock::new(Parenthetical, format!("(b{i})")));
        }
        blocks.push(ScriptBlock::new(Character, "IVAN".into()));
        blocks.push(ScriptBlock::new(Parenthetical, "(softly)".into()));
        blocks.push(ScriptBlock::new(Dialogue, "Hi.".into()));
        let pages = paginate(&blocks, 54);
        assert_eq!(pages[0].chunks.len(), 52);
        let ids: Vec<usize> = pages[1].chunks.iter().map(|c| c.bi).collect();
        assert_eq!(ids, vec![52, 53, 54, 55]);
    }

    #[test]
    fn paginate_splits_action_at_paragraphs_without_markers() {
        use ElementType::*;
        let paras: Vec<String> = (0..10).map(|i| format!("para number {i} here")).collect();
        let blocks = vec![ScriptBlock::new(Action, paras.join("\n"))];
        // Force tiny pages: top(1) + 10 lines needs 2 pages of 6.
        let pages = paginate(&blocks, 6);
        assert_eq!(pages.len(), 2);
        // First chunk carries paragraphs, markers only on continuation.
        assert!(!pages[0].chunks[0].contd);
        assert!(pages[0].chunks[0].more);
        assert!(!pages[1].chunks[0].more);
        // Paragraph ranges tile the block without gaps.
        let mut covered = vec![false; 10];
        for p in &pages {
            for c in &p.chunks {
                assert_eq!(c.bi, 0);
                for (i, slot) in covered.iter_mut().enumerate().take(c.end).skip(c.start) {
                    assert!(!*slot, "paragraph {i} covered twice");
                    *slot = true;
                }
                assert!(!c.contd, "action chunks get no CONT'D");
            }
        }
        assert!(covered.iter().all(|&c| c));
    }

    #[test]
    fn paginate_keeps_cue_with_following_parenthetical_and_dialogue() {
        use ElementType::*;
        // 11 one-line actions fill 21 of 26 rows (1 + 10×2), so the cue
        // would fit alone but not together with its speech head.
        let mut blocks: Vec<ScriptBlock> = (0..11)
            .map(|_| ScriptBlock::new(Action, "one line here".into()))
            .collect();
        let cue_bi = blocks.len();
        blocks.push(ScriptBlock::new(Character, "IVAN".into()));
        blocks.push(ScriptBlock::new(
            Parenthetical,
            "(whispering very quietly indeed my friend)".into(),
        ));
        blocks.push(ScriptBlock::new(Dialogue, "Hello there.".into()));
        let pages = paginate(&blocks, 26);
        // The cue must not strand: it moves with its speech to page 2.
        let page1_ids: Vec<usize> = pages[0].chunks.iter().map(|c| c.bi).collect();
        assert!(!page1_ids.contains(&cue_bi), "cue stranded at page bottom");
        let page2_ids: Vec<usize> = pages[1].chunks.iter().map(|c| c.bi).collect();
        assert_eq!(
            &page2_ids[..3],
            &[cue_bi, cue_bi + 1, cue_bi + 2],
            "cue, parenthetical and dialogue stay together"
        );
    }
}
