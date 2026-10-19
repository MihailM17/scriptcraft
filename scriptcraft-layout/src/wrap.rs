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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WrappedLine {
    /// Display text: source words joined by single spaces (hyphen pieces
    /// rejoin without one). Blank paragraphs render as one empty line.
    pub text: String,
    /// Char offset (not bytes) into the paragraph where this line starts.
    pub start: usize,
    /// Char offset one past the last source char covered by this line.
    pub end: usize,
}

/// Wrap one paragraph (no `\n` expected) into display lines.
pub fn wrap_paragraph(para: &str, chars_per_line: usize) -> Vec<WrappedLine> {
    let cpl = chars_per_line.max(1);
    let total = para.chars().count();
    if para.trim().is_empty() {
        return vec![WrappedLine {
            text: String::new(),
            start: 0,
            end: total,
        }];
    }
    let mut out: Vec<WrappedLine> = Vec::new();
    let mut line = String::new();
    let mut len = 0usize;
    let mut start = 0usize;
    let mut end = 0usize;
    for (word, w_start, w_end) in words_with_spans(para) {
        push_word(
            &mut out, &mut line, &mut len, &mut start, &mut end, word, w_start, w_end, cpl,
        );
    }
    out.push(WrappedLine {
        text: line,
        start,
        end: total,
    });
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
    if para.trim().is_empty() {
        return 1;
    }
    let mut rows = 1usize;
    let mut len = 0usize;
    for w in para.split_whitespace() {
        let wl = w.chars().count();
        let space = usize::from(len > 0);
        if len + space + wl <= cpl {
            len += space + wl;
            continue;
        }
        let pieces = hyphen_pieces(w);
        if pieces.len() > 1 {
            for (k, piece) in pieces.iter().enumerate() {
                let space = if k == 0 { usize::from(len > 0) } else { 0 };
                let pl = piece.chars().count();
                if len + space + pl <= cpl {
                    len += space + pl;
                } else {
                    if len > 0 {
                        rows += 1;
                    }
                    len = pl;
                }
            }
        } else {
            if len > 0 {
                rows += 1;
            }
            len = wl;
        }
    }
    rows
}

/// The paragraph's words as `(slice, char_start, char_end)`.
fn words_with_spans(para: &str) -> Vec<(&str, usize, usize)> {
    let chars: Vec<(usize, char)> = para.char_indices().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        while i < chars.len() && chars[i].1.is_whitespace() {
            i += 1;
        }
        let start = i;
        while i < chars.len() && !chars[i].1.is_whitespace() {
            i += 1;
        }
        if i > start {
            let bs = chars[start].0;
            let be = if i < chars.len() {
                chars[i].0
            } else {
                para.len()
            };
            out.push((&para[bs..be], start, i));
        }
    }
    out
}

/// Greedy placement of one word, breaking at internal hyphens (Qt/KIT break
/// "по-малко" as "по-" + "малко" when the whole word doesn't fit).
#[allow(clippy::too_many_arguments)]
fn push_word(
    out: &mut Vec<WrappedLine>,
    line: &mut String,
    len: &mut usize,
    start: &mut usize,
    end: &mut usize,
    w: &str,
    w_start: usize,
    w_end: usize,
    cpl: usize,
) {
    let wl = w.chars().count();
    let space = usize::from(!line.is_empty());
    if *len + space + wl <= cpl {
        if space == 1 {
            line.push(' ');
        }
        line.push_str(w);
        *len += space + wl;
        *end = w_end;
        return;
    }
    let pieces = hyphen_pieces(w);
    if pieces.len() > 1 {
        let mut off = w_start;
        for (k, piece) in pieces.iter().enumerate() {
            if k > 0 {
                off += pieces[k - 1].chars().count();
            }
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
                *end = off + pl;
            } else {
                if !line.is_empty() {
                    *end = off;
                    out.push(WrappedLine {
                        text: std::mem::take(line),
                        start: *start,
                        end: *end,
                    });
                    *start = off;
                }
                line.push_str(piece);
                *len = pl;
                *end = off + pl;
            }
        }
        return;
    }
    // No break opportunity: move the word to the next line whole.
    if !line.is_empty() {
        *end = w_start;
        out.push(WrappedLine {
            text: std::mem::take(line),
            start: *start,
            end: *end,
        });
        *start = w_start;
        *len = 0;
    }
    line.push_str(w);
    *len = wl;
    *end = w_end;
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

    /// Join a span's source chars the way the display text is produced.
    fn joined_source(para: &str, start: usize, end: usize) -> String {
        para.chars()
            .skip(start)
            .take(end.saturating_sub(start))
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
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
            assert_eq!(l.text, joined_source(para, l.start, l.end), "para {para:?}");
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
    fn blank_paragraph_is_one_spanning_line() {
        let empty = wrap_paragraph("", 10);
        assert_eq!(empty.len(), 1);
        assert_eq!(
            (empty[0].text.as_str(), empty[0].start, empty[0].end),
            ("", 0, 0)
        );
        let spaces = wrap_paragraph("   ", 10);
        assert_eq!(spaces.len(), 1);
        assert_eq!(
            (spaces[0].text.as_str(), spaces[0].start, spaces[0].end),
            ("", 0, 3)
        );
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
