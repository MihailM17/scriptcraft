use crate::model::{ElementType, ScriptBlock};

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

/// Uppercase-on-type element kinds (Russian template `uppercase="true"`).
pub fn auto_upper(t: ElementType) -> bool {
    matches!(
        t,
        ElementType::SceneHeading
            | ElementType::SceneCharacters
            | ElementType::Character
            | ElementType::Transition
    )
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

/// Locations seen in existing scene headings, e.g. `EXT. ПАРКИНГ - NIGHT`
/// yields `ПАРКИНГ`. Order of first appearance, deduplicated.
pub fn known_locations(blocks: &[ScriptBlock]) -> Vec<String> {
    let mut out = Vec::new();
    for b in blocks
        .iter()
        .filter(|b| b.element_type == ElementType::SceneHeading)
    {
        let mut h = b.text.as_str();
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

/// Standard scene-heading times: KIT's seeded `times` list plus the common
/// "A FEW MOMENTS LATER" variant.
pub const TIME_OPTIONS: &[&str] = &[
    "DAY",
    "NIGHT",
    "MORNING",
    "AFTERNOON",
    "EVENING",
    "LATER",
    "MOMENTS LATER",
    "A FEW MOMENTS LATER",
    "CONTINUOUS",
    "THE NEXT DAY",
];

/// Times of day already used in the script, in order of first appearance.
pub fn known_times(blocks: &[ScriptBlock]) -> Vec<String> {
    let mut out = Vec::new();
    for b in blocks
        .iter()
        .filter(|b| b.element_type == ElementType::SceneHeading)
    {
        if let Some((_, time)) = b.text.rsplit_once(" - ") {
            let t = time.trim();
            if !t.is_empty() && !out.iter().any(|s: &String| s.eq_ignore_ascii_case(t)) {
                out.push(t.to_string());
            }
        }
    }
    out
}

/// Autocomplete candidates for `element_type` given the current `prefix`.
/// Capped, most useful first.
pub fn candidates(blocks: &[ScriptBlock], element_type: ElementType, prefix: &str) -> Vec<String> {
    const CAP: usize = 8;
    match element_type {
        ElementType::Character => {
            let up = prefix.trim().to_uppercase();
            let mut cues = Vec::new();
            for b in blocks {
                if b.element_type == ElementType::Character && !cues.contains(&b.text) {
                    cues.push(b.text.clone());
                }
            }
            cues.sort();
            let mut out = Vec::new();
            for cue in &cues {
                let base = base_character_name(cue).to_string();
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
                    for loc in known_locations(blocks) {
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
            // Once the " - " separator is typed, suggest times of day so the
            // writer can complete "INT. PARK - " in one pick.
            if let Some((_, typed_time)) = prefix.rsplit_once(" - ") {
                let head = &prefix[..prefix.len() - typed_time.len()];
                let typed_up = typed_time.trim_start().to_uppercase();
                let mut times: Vec<String> = TIME_OPTIONS.iter().map(|s| s.to_string()).collect();
                for t in known_times(blocks) {
                    if !times.iter().any(|x| x.eq_ignore_ascii_case(&t)) {
                        times.push(t);
                    }
                }
                for t in times {
                    if t.len() > typed_up.len() && t.starts_with(&typed_up) {
                        let suggestion = format!("{head}{t}");
                        if !out.iter().any(|s| s.eq_ignore_ascii_case(&suggestion)) {
                            out.push(suggestion);
                        }
                        if out.len() >= 16 {
                            break;
                        }
                    }
                }
            }
            // Existing headings starting with the prefix.
            if out.len() < CAP {
                for b in blocks
                    .iter()
                    .filter(|b| b.element_type == ElementType::SceneHeading)
                {
                    if b.text.to_uppercase().starts_with(&up as &str)
                        && b.text.len() > prefix.len()
                        && !out.iter().any(|s| s.eq_ignore_ascii_case(&b.text))
                    {
                        out.push(b.text.clone());
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
            for b in blocks {
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

/// How many editor rows a text needs inside a column of `col_width_px`
/// at `font_pt`. Greedy word wrap, matching the on-screen layout.
pub fn wrap_rows(text: &str, col_width_px: f32, font_pt: f32) -> usize {
    let per_line = (col_width_px / (MONO_ADVANCE * font_pt).max(1.0)).floor() as usize;
    wrap_rows_chars(text, per_line.max(8))
}

/// Same as [`wrap_rows`] but with a fixed character budget (the coach's
/// line estimate; mirrors the layout engine's wrap breaks without building
/// the display strings).
pub fn wrap_rows_chars(text: &str, chars_per_line: usize) -> usize {
    text.split('\n')
        .map(|p| count_rows(p, chars_per_line.max(1)))
        .sum::<usize>()
        .max(1)
}

/// Rows one paragraph needs under the same greedy/hyphen rules as
/// `scriptcraft-layout`'s wrap (blank paragraphs still occupy a row).
fn count_rows(para: &str, cpl: usize) -> usize {
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

/// Format seconds as KIT does in the navigator: `(m:ss)`.
pub fn format_duration(total_secs: f64) -> String {
    let s = total_secs.max(0.0).round() as u64;
    format!("({}:{:02})", s / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Project;
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
        let c = candidates(&p.blocks, ElementType::Character, "");
        assert_eq!(c, vec!["IVAN".to_string()]);
        assert!(candidates(&p.blocks, ElementType::Character, "IVA").contains(&"IVAN".to_string()));
        assert!(candidates(&p.blocks, ElementType::Character, "X").is_empty());
    }

    #[test]
    fn heading_candidates_prefixes_and_locations() {
        let p = sample();
        let c = candidates(&p.blocks, ElementType::SceneHeading, "");
        assert!(c.contains(&"INT. ".to_string()));
        assert!(c.contains(&"EXT. ".to_string()));
        let loc = candidates(&p.blocks, ElementType::SceneHeading, "EXT. ");
        assert!(loc.iter().any(|s| s.contains("PARK")));
    }

    #[test]
    fn scene_heading_location_pick_does_not_duplicate_typed_text() {
        let p = sample();
        let sugg = candidates(&p.blocks, ElementType::SceneHeading, "EXT. P");
        assert_eq!(sugg.first().map(String::as_str), Some("EXT. PARK"));
        assert!(
            sugg.iter().all(|s| !s.contains("PPARK")),
            "suggestion must not repeat the typed location prefix: {sugg:?}"
        );
        let done = complete_line_at("EXT. P", 6, &sugg[0]);
        assert_eq!(done, "EXT. PARK");
        assert_eq!(done.matches("EXT.").count(), 1);
        // No location suggestions once the " - TIME" part began.
        let time = candidates(&p.blocks, ElementType::SceneHeading, "EXT. PARK - D");
        assert!(time.iter().all(|s| s.starts_with("EXT. PARK - D")));
        assert!(time.iter().all(|s| !s.contains("EXT. EXT.")));
    }

    #[test]
    fn known_locations_parses() {
        let p = sample();
        assert_eq!(
            known_locations(&p.blocks),
            vec!["PARK".to_string(), "CAR".to_string()]
        );
    }

    #[test]
    fn scene_heading_times_are_suggested() {
        let p = sample();
        let all = candidates(&p.blocks, ElementType::SceneHeading, "EXT. PARK - ");
        assert!(all.iter().any(|s| s == "EXT. PARK - DAY"));
        assert!(all.iter().any(|s| s == "EXT. PARK - NIGHT"));
        assert!(all.iter().any(|s| s == "EXT. PARK - A FEW MOMENTS LATER"));
        assert!(all.iter().all(|s| s.starts_with("EXT. PARK - ")));

        let m = candidates(&p.blocks, ElementType::SceneHeading, "EXT. PARK - M");
        assert!(m.iter().any(|s| s == "EXT. PARK - MORNING"));
        assert!(m.iter().any(|s| s == "EXT. PARK - MOMENTS LATER"));
        assert!(!m.iter().any(|s| s == "EXT. PARK - DAY"));

        assert!(
            candidates(&p.blocks, ElementType::SceneHeading, "EXT. PARK - DAY").is_empty(),
            "a complete time needs no further suggestion"
        );
    }

    #[test]
    fn known_times_parses_headings() {
        let p = sample();
        assert_eq!(
            known_times(&p.blocks),
            vec!["DAY".to_string(), "NIGHT".to_string()]
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
    fn cue_name_for_finds_the_speaker() {
        use ElementType::*;
        let blocks = vec![
            ScriptBlock::new(Character, "IVAN".into()),
            ScriptBlock::new(Dialogue, "Hi.".into()),
        ];
        assert_eq!(cue_name_for(&blocks, 1), Some("IVAN".to_string()));
        assert_eq!(cue_name_for(&blocks, 0), None);
    }

    #[test]
    fn wrap_rows_chars_counts_kit_breaks() {
        // Hyphen breaks count as row breaks (same cases as layout's wrap).
        assert_eq!(
            wrap_rows_chars("Осем часа сутринта. Стотинка по-малко да е", 35),
            2
        );
        assert_eq!(wrap_rows_chars("xxxxxxxxxxxxxxxxxx 10-литровите", 24), 2);
        assert_eq!(wrap_rows_chars("aaa bbbbbb", 6), 2);
        assert_eq!(wrap_rows_chars("aaa b-c", 7), 1);
    }
}
