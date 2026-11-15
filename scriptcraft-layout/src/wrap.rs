//! Greedy word wrap with source spans.
//!
//! The display-text algorithm is a 1:1 port of `scriptcraft-core::editing`'s
//! `wrap_lines` / `push_word` / `hyphen_pieces` (KIT/Qt-style: a word that
//! doesn't fit the remaining space breaks after its last internal hyphen
//! instead of moving whole). `tests/parity.rs` locks the port to the original
//! on the A95 fixture and a synthetic corpus, so the editor can never drift
//! from the pagination budgets.
//!
//! On top of the port, every wrapped line carries the char range of the source
//! paragraph that produced it. Those ranges are what the future `ScriptView`
//! selection, caret placement and hit-testing will map through.

/// One wrapped display line plus the char range of the source paragraph that
/// produced it.
///
/// Ranges **tile** the paragraph exactly: `lines[0].start == 0`, every
/// `lines[i].end == lines[i + 1].start`, and `lines.last().end` equals the
/// paragraph's char count. Whitespace at a wrap boundary belongs to the line
/// before it (so a caret at the start of the next line can still be expressed
/// as the previous line's end).
///
/// Display text is the source text: every space the writer typed occupies its
/// own cell. Only the whitespace run that *causes* a wrap is left out of the
/// display (it stays inside the previous line's span), and that run is not
/// selectable because the line break already separates the two rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WrappedLine {
    /// Display text: a slice of the source paragraph, one cell per character,
    /// including every space. Blank paragraphs render as one empty line.
    pub text: String,
    /// Char offset (not bytes) into the paragraph where this line starts.
    pub start: usize,
    /// Char offset one past the last source char covered by this line.
    pub end: usize,
    /// Source char offset for every display boundary, `text.chars().count() + 1`
    /// entries.
    ///
    /// Display columns are source columns — `cols[col] == start + col` — since
    /// nothing is collapsed. The map stays on the public line type because
    /// hit-testing, the caret and selection painting read it; the last entry
    /// is the offset after the last displayed character (before any trailing
    /// wrap whitespace that the line break swallowed).
    pub cols: Vec<usize>,
}

/// Wrap one paragraph (no `\n` expected) into display lines.
///
/// Spaces are content: a run of spaces is kept and displayed as one cell per
/// space. The only whitespace that is not displayed is the run at which a
/// line breaks, because that break is already visible as the row change.
pub fn wrap_paragraph(para: &str, chars_per_line: usize) -> Vec<WrappedLine> {
    let cpl = chars_per_line.max(1);
    let total = para.chars().count();
    if total == 0 {
        return vec![WrappedLine {
            text: String::new(),
            start: 0,
            end: 0,
            cols: vec![0],
        }];
    }
    let tokens = tokens_with_spans(para);
    let mut out: Vec<WrappedLine> = Vec::new();
    let mut line = String::new();
    let mut len = 0usize;
    let mut start = 0usize;
    let mut end = 0usize;

    // The paragraph's leading whitespace is content too (a writer can indent
    // with spaces). Keep it even if the line is already at the limit.
    if let Some(first) = tokens.first() {
        if first.ws_start == 0 && first.word_start > 0 {
            line.extend(para.chars().take(first.word_start));
            len = first.word_start;
            end = first.word_start;
        }
    }

    for token in &tokens {
        // The paragraph's leading run was already placed above.
        let ws = if token.ws_start == 0 && start == 0 {
            ""
        } else {
            token.ws
        };
        place_token(
            &mut out, &mut line, &mut len, &mut start, &mut end, ws, token, cpl,
        );
    }

    // Trailing whitespace is content too: the writer may be about to type
    // there, and the caret must be able to sit after it.
    if end < total {
        let tail: String = para.chars().skip(end).take(total - end).collect();
        line.push_str(&tail);
        end = total;
    }
    out.push(WrappedLine {
        text: line,
        start,
        end: total,
        cols: Vec::new(),
    });
    for line in &mut out {
        let shown = line.text.chars().count();
        line.cols = (line.start..=line.start + shown).collect();
    }
    out
}

/// How many grid rows a text (paragraphs split on `\n`) needs in a column of
/// `chars_per_line` characters. Never zero — blank lines still occupy a row.
pub fn wrap_rows(text: &str, chars_per_line: usize) -> usize {
    text.split('\n')
        .map(|p| row_count(p, chars_per_line))
        .sum::<usize>()
        .max(1)
}

/// Count-only twin of [`wrap_paragraph`] (no string building); the paginator
/// runs this on every block of every relayout, so it must never allocate.
pub fn row_count(para: &str, chars_per_line: usize) -> usize {
    let cpl = chars_per_line.max(1);
    if para.is_empty() {
        return 1;
    }
    let tokens = tokens_with_spans(para);
    let mut rows = 1usize;
    let mut len = 0usize;
    if let Some(first) = tokens.first() {
        if first.ws_start == 0 && first.word_start > 0 {
            len = first.word_start;
        }
    }
    for (i, tok) in tokens.iter().enumerate() {
        let wsl = if i == 0 { 0 } else { tok.ws_chars };
        let wl = tok.word.chars().count();
        if len + wsl + wl <= cpl {
            len += wsl + wl;
            continue;
        }
        let pieces = hyphen_pieces(tok.word);
        if pieces.len() == 1 {
            if len > 0 {
                rows += 1;
            }
            len = wl;
            continue;
        }
        for (k, piece) in pieces.iter().enumerate() {
            let lead = if k == 0 { wsl } else { 0 };
            let pl = piece.chars().count();
            if len + lead + pl <= cpl {
                len += lead + pl;
            } else {
                if len > 0 {
                    rows += 1;
                }
                len = pl;
            }
        }
    }
    rows
}

/// One word plus the exact whitespace run before it.
#[derive(Clone, Copy, Debug)]
struct Token<'a> {
    ws: &'a str,
    ws_chars: usize,
    ws_start: usize,
    word: &'a str,
    word_start: usize,
    word_end: usize,
}

/// The paragraph's words with the whitespace that precedes each of them.
fn tokens_with_spans(para: &str) -> Vec<Token<'_>> {
    let chars: Vec<(usize, char)> = para.char_indices().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let ws_start = i;
        while i < chars.len() && chars[i].1.is_whitespace() {
            i += 1;
        }
        let ws_end = i;
        let word_start = i;
        while i < chars.len() && !chars[i].1.is_whitespace() {
            i += 1;
        }
        if i > word_start {
            let ws_bs = chars
                .get(ws_start)
                .map(|(b, _)| *b)
                .unwrap_or_else(|| para.len());
            let word_bs = chars[word_start].0;
            let word_be = if i < chars.len() {
                chars[i].0
            } else {
                para.len()
            };
            out.push(Token {
                ws: &para[ws_bs..word_bs],
                ws_chars: ws_end - ws_start,
                ws_start,
                word: &para[word_bs..word_be],
                word_start,
                word_end: i,
            });
        }
    }
    out
}

/// Place one token (its whitespace run plus the word) on the current line,
/// wrapping first at word boundaries and then at internal hyphens.
fn place_token(
    out: &mut Vec<WrappedLine>,
    line: &mut String,
    len: &mut usize,
    start: &mut usize,
    end: &mut usize,
    ws: &str,
    tok: &Token<'_>,
    cpl: usize,
) {
    let wsl = ws.chars().count();
    let wl = tok.word.chars().count();
    if *len + wsl + wl <= cpl {
        line.push_str(ws);
        line.push_str(tok.word);
        *len += wsl + wl;
        *end = tok.word_end;
        return;
    }
    let pieces = hyphen_pieces(tok.word);
    if pieces.len() == 1 {
        // No break opportunity: move the word to the next line whole.
        if *len > 0 {
            // The whitespace run that forced the break stays inside the
            // previous line's source span; the row break separates the lines.
            *end = tok.word_start;
            out.push(WrappedLine {
                text: std::mem::take(line),
                start: *start,
                end: *end,
                cols: Vec::new(),
            });
            *start = tok.word_start;
        }
        line.push_str(tok.word);
        *len = wl;
        *end = tok.word_end;
        return;
    }
    // Break after internal hyphens (Qt/KIT rule), even when the whole word
    // would fit on a fresh line on its own.
    let mut off = tok.word_start;
    for (k, piece) in pieces.iter().enumerate() {
        if k > 0 {
            off += pieces[k - 1].chars().count();
        }
        let lead = if k == 0 { wsl } else { 0 };
        let lead_str = if k == 0 { ws } else { "" };
        let pl = piece.chars().count();
        if *len + lead + pl <= cpl {
            line.push_str(lead_str);
            line.push_str(piece);
            *len += lead + pl;
            *end = off + pl;
        } else {
            if *len > 0 {
                *end = off;
                out.push(WrappedLine {
                    text: std::mem::take(line),
                    start: *start,
                    end: *end,
                    cols: Vec::new(),
                });
                *start = off;
            }
            line.push_str(piece);
            *len = pl;
            *end = off + pl;
        }
    }
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

/// Greedy wrap into plain line strings (the printer's view of
/// [`wrap_paragraph`]; same line breaks as the paginator uses).
pub fn wrap_lines(text: &str, chars_per_line: usize) -> Vec<String> {
    text.split('\n')
        .flat_map(|para| wrap_paragraph(para, chars_per_line))
        .map(|line| line.text)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The display must be the source span itself, minus only the whitespace
    /// run that the line break swallowed at the end.
    fn joined_source<'a>(para: &'a str, start: usize, end: usize) -> String {
        para.chars()
            .skip(start)
            .take(end.saturating_sub(start))
            .collect()
    }

    fn assert_tiles(para: &str, cpl: usize) {
        let lines = wrap_paragraph(para, cpl);
        let total = para.chars().count();
        assert_eq!(lines.first().map(|l| l.start), Some(0), "para {para:?}");
        assert_eq!(lines.last().map(|l| l.end), Some(total), "para {para:?}");
        for w in lines.windows(2) {
            assert_eq!(w[0].end, w[1].start, "gap/overlap in {para:?}");
        }
        for l in &lines {
            let span = joined_source(para, l.start, l.end);
            let shown: String = span.chars().take(l.text.chars().count()).collect();
            assert_eq!(l.text, shown, "para {para:?}");
            let swallowed: String = span.chars().skip(l.text.chars().count()).collect();
            assert!(
                swallowed.chars().all(char::is_whitespace),
                "only a wrap gap may be undisplayed in {para:?}: {l:?}"
            );
            // Display columns are source columns: nothing collapses.
            let disp = l.text.chars().count();
            let expect: Vec<usize> = (l.start..=l.start + disp).collect();
            assert_eq!(l.cols, expect, "para {para:?} line {l:?}");
            assert!(*l.cols.last().unwrap() <= l.end, "para {para:?} line {l:?}");
        }
    }

    #[test]
    fn breaks_at_internal_hyphens_like_kit() {
        let text = |lines: Vec<WrappedLine>| lines.into_iter().map(|l| l.text).collect::<Vec<_>>();
        // KIT (Qt) fills the line up to the last hyphen that fits.
        assert_eq!(
            text(wrap_paragraph(
                "Осем часа сутринта. Стотинка по-малко да е",
                35
            )),
            vec!["Осем часа сутринта. Стотинка по-", "малко да е"]
        );
        // "10-литровите" as the 10-char tail of a dialogue line.
        assert_eq!(
            text(wrap_paragraph("xxxxxxxxxxxxxxxxxx 10-литровите", 24)),
            vec!["xxxxxxxxxxxxxxxxxx 10-", "литровите"]
        );
        // A word without hyphens moves whole to the next line.
        assert_eq!(text(wrap_paragraph("aaa bbbbbb", 6)), vec!["aaa", "bbbbbb"]);
        // Hyphenated word that fits whole is never split.
        assert_eq!(text(wrap_paragraph("aaa b-c", 7)), vec!["aaa b-c"]);
        // Pieces rejoin without a space when the line breaks before them.
        assert_eq!(
            text(wrap_paragraph("aaaaaaaa 10-литрови", 10)),
            vec!["aaaaaaaa", "10-литрови"]
        );
    }

    #[test]
    fn every_space_keeps_its_own_cell() {
        // A writer typed two spaces after the period; both must survive, and
        // the map must be the identity so a click on "cd" lands on "c".
        let lines = wrap_paragraph("ab  cd", 10);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "ab  cd");
        assert_eq!(lines[0].cols, vec![0, 1, 2, 3, 4, 5, 6]);

        // Leading spaces are content, not indentation to be dropped.
        let lines = wrap_paragraph("   abc", 10);
        assert_eq!(lines[0].text, "   abc");
        assert_eq!(lines[0].cols, vec![0, 1, 2, 3, 4, 5, 6]);

        // Trailing spaces stay reachable by the caret.
        let lines = wrap_paragraph("abc   ", 10);
        assert_eq!(lines[0].text, "abc   ");
        assert_eq!(lines[0].cols, vec![0, 1, 2, 3, 4, 5, 6]);

        // Only the run that causes a wrap is swallowed by the row break.
        let lines = wrap_paragraph("aaa bbb", 3);
        assert_eq!(lines[0].text, "aaa");
        assert_eq!(lines[0].cols, vec![0, 1, 2, 3]);
        assert_eq!(lines[1].text, "bbb");
        assert_eq!(lines[1].cols, vec![4, 5, 6, 7]);

        // A multi-space run that fits is kept whole; when it cannot fit, the
        // break swallows it exactly like a single space.
        let lines = wrap_paragraph("aa    bb", 8);
        assert_eq!(lines[0].text, "aa    bb");
        let lines = wrap_paragraph("aaaa    bb", 9);
        assert_eq!(lines[0].text, "aaaa");
        assert_eq!(lines[1].text, "bb");
        assert_eq!((lines[0].start, lines[0].end), (0, 8));
        assert_eq!((lines[1].start, lines[1].end), (8, 10));
    }

    #[test]
    fn blank_paragraph_is_one_spanning_line() {
        let empty = wrap_paragraph("", 10);
        assert_eq!(empty.len(), 1);
        assert_eq!(
            (empty[0].text.as_str(), empty[0].start, empty[0].end),
            ("", 0, 0)
        );
        // A line holding only spaces still carries them, so the caret can sit
        // at the end of the run.
        let spaces = wrap_paragraph("   ", 10);
        assert_eq!(spaces.len(), 1);
        assert_eq!(
            (spaces[0].text.as_str(), spaces[0].start, spaces[0].end),
            ("   ", 0, 3)
        );
        assert_eq!(spaces[0].cols, vec![0, 1, 2, 3]);
    }

    #[test]
    fn spans_tile_tricky_paragraphs() {
        for para in [
            "aa bb cc dd",
            "  ведущи интервали и думи",
            "дума-с-много-тирета накрая-",
            "-започва с тире",
            "много      вътрешни      интервали",
            "единственадумабезинтервали",
            "x",
            "край-",
            "две  интервала   тук",
            "   ",
            "a\t\tb",
        ] {
            for cpl in [1usize, 2, 3, 5, 7, 10, 18, 28, 58] {
                assert_tiles(para, cpl);
            }
        }
    }

    #[test]
    fn wrap_rows_never_zero() {
        assert_eq!(wrap_rows("", 10), 1);
        assert_eq!(wrap_rows("a\n\nb", 10), 3);
        assert_eq!(wrap_rows("hello world", 5), 2);
    }

    /// `row_count` must stay an exact twin of the string-building wrap: any
    /// drift would silently change pagination.
    #[test]
    fn row_count_matches_wrap_paragraph() {
        for para in [
            "aa bb cc dd",
            "Осем часа сутринта. Стотинка по-малко да е",
            "xxxxxxxxxxxxxxxxxx 10-литровите",
            "  ведущи интервали и думи",
            "дума-с-много-тирета накрая-",
            "-започва с тире",
            "много      вътрешни      интервали",
            "единственадумабезинтервали",
            "aaaaaaaa 10-литрови",
            "aaa b-c",
            "x",
            "край-",
            "",
            "   ",
            "ab  cd",
            "aaaa    bb",
            "   leading",
            "trailing   ",
            "a\t\tb",
        ] {
            for cpl in [1usize, 2, 3, 5, 7, 10, 18, 24, 28, 35, 58] {
                assert_eq!(
                    row_count(para, cpl),
                    wrap_paragraph(para, cpl).len(),
                    "para {para:?} at {cpl}"
                );
            }
        }
    }
}
