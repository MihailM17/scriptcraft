use crate::model::{ElementType, Project, ScriptBlock};

/// Export a project to Fountain plain text (`.fountain`).
///
/// Layout mirrors KIT Scenarist conventions: title page keys, blank line
/// between blocks, `> ` forced transitions, `!` escape for action lines
/// that would otherwise parse as scene headings.
pub fn to_fountain(p: &Project, meta: &crate::kitsp::Metadata) -> String {
    let mut out = String::new();
    out.push_str(&format!("Title: {}\n", p.title));
    if !meta.genre.trim().is_empty() {
        out.push_str(&format!("Credit: {}\n", meta.genre.trim()));
    }
    if !meta.author.trim().is_empty() {
        out.push_str(&format!("Author: {}\n", meta.author.trim()));
    }
    if !meta.year.trim().is_empty() {
        out.push_str(&format!("Draft date: {}\n", meta.year.trim()));
    }
    if !meta.contacts.trim().is_empty() {
        for line in meta.contacts.lines() {
            let line = line.trim();
            if !line.is_empty() {
                out.push_str(&format!("Contact: {line}\n"));
            }
        }
    }
    out.push('\n');
    for b in &p.blocks {
        let text = b.text.trim();
        if text.is_empty() {
            continue;
        }
        match b.element_type {
            ElementType::SceneHeading
            | ElementType::SceneCharacters
            | ElementType::Shot
            | ElementType::Character
            | ElementType::Parenthetical
            | ElementType::Dialogue => {
                out.push_str(text);
                out.push('\n');
            }
            ElementType::Lyrics => {
                if text.starts_with('~') {
                    out.push_str(text);
                } else {
                    out.push('~');
                    out.push_str(text);
                }
                out.push('\n');
            }
            ElementType::NonPrintable => {
                out.push_str("[[");
                out.push_str(&text.replace('\n', " "));
                out.push_str("]]\n");
            }
            ElementType::Action => {
                // Escape action lines that Fountain would read as scenes.
                let up = text.to_uppercase();
                let looks_like_scene = ["INT.", "EXT.", "EST.", "INT ", "EXT ", "EST "]
                    .iter()
                    .any(|pre| up.starts_with(pre));
                if looks_like_scene {
                    out.push('!');
                }
                out.push_str(text);
                out.push('\n');
            }
            ElementType::Transition => {
                if text.starts_with('>') {
                    out.push_str(text);
                } else {
                    out.push_str("> ");
                    out.push_str(text);
                }
                out.push('\n');
            }
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ScriptBlock;

    fn meta(author: &str) -> crate::kitsp::Metadata {
        crate::kitsp::Metadata {
            author: author.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn exports_blocks_with_forced_transitions() {
        let mut p = Project {
            title: "T".into(),
            blocks: vec![],
            ..Default::default()
        };
        for (t, text) in [
            (ElementType::SceneHeading, "EXT. PARK - DAY"),
            (ElementType::Action, "A man walks."),
            (ElementType::Character, "IVAN"),
            (ElementType::Dialogue, "Hi."),
            (ElementType::Transition, "FADE OUT:"),
        ] {
            p.blocks.push(ScriptBlock::new(t, text.into()));
        }
        let f = to_fountain(&p, &meta("Anon"));
        assert!(f.starts_with("Title: T\nAuthor: Anon\n"));
        assert!(f.contains("EXT. PARK - DAY\n"));
        assert!(f.contains("> FADE OUT:\n"));
    }

    #[test]
    fn escapes_action_lines_looking_like_scenes() {
        let p = Project {
            title: "T".into(),
            blocks: vec![ScriptBlock::new(
                ElementType::Action,
                "INT. something happened, he thought.".into(),
            )],
            ..Default::default()
        };
        let f = to_fountain(&p, &meta(""));
        assert!(f.contains("!INT. something"));
    }

    #[test]
    fn parses_full_fountain_document() {
        let text = "Title: Test\nAuthor: Me\n\n# Act One\n\n.EXT. PARK - DAY\n\nA man walks in.\n\nIVAN\n\nHello there.\n\n(to himself)\n\nAgain.\n\n~La la la\n\n[[a quiet note]]\n\n> CUT TO:\n\nFADE OUT:\n";
        let (p, meta) = parse_fountain(text);
        assert_eq!(meta.title, "Test");
        assert_eq!(meta.author, "Me");
        let kinds: Vec<(ElementType, &str)> = p
            .blocks
            .iter()
            .map(|b| (b.element_type, b.text.as_str()))
            .collect();
        use ElementType::*;
        assert_eq!(
            kinds,
            vec![
                (NonPrintable, "Act One"),
                (SceneHeading, "EXT. PARK - DAY"),
                (Action, "A man walks in."),
                (Character, "IVAN"),
                (Dialogue, "Hello there."),
                (Parenthetical, "(to himself)"),
                (Dialogue, "Again."),
                (Lyrics, "La la la"),
                (NonPrintable, "a quiet note"),
                (Transition, "CUT TO:"),
                (Transition, "FADE OUT:"),
            ]
        );
        assert_eq!(p.title, "Test");
    }

    #[test]
    fn uppercase_action_without_dialogue_stays_action() {
        // An all-caps line followed by a scene can't be a cue (Fountain is
        // positional: cues must be followed by spoken lines).
        let (p, _) =
            parse_fountain("INT. HALL - DAY\n\nПРЕВОЗ НА БАГАЖ И ХОРА\n\nEXT. STREET - DAY\n");
        let kinds: Vec<ElementType> = p.blocks.iter().map(|b| b.element_type).collect();
        use ElementType::*;
        assert_eq!(kinds, vec![SceneHeading, Action, SceneHeading]);
    }

    #[test]
    fn export_import_round_trips_clean_documents() {
        let mut p = Project {
            title: "RT".into(),
            blocks: vec![],
            ..Default::default()
        };
        for (t, text) in [
            (ElementType::SceneHeading, "EXT. PARK - DAY"),
            (ElementType::Action, "A man walks in."),
            (ElementType::Character, "IVAN (ПРОД.)"),
            (ElementType::Dialogue, "Hi there."),
            (ElementType::Parenthetical, "(softly)"),
            (ElementType::Dialogue, "Again."),
            (ElementType::Lyrics, "La la la"),
            (ElementType::NonPrintable, "a note"),
            (ElementType::Transition, "CUT TO:"),
            (ElementType::SceneHeading, "INT. CAR - NIGHT"),
            (ElementType::Transition, "FADE OUT:"),
        ] {
            p.blocks.push(ScriptBlock::new(t, text.into()));
        }
        let (q, meta) = parse_fountain(&to_fountain(&p, &meta("Me")));
        assert_eq!(meta.title, "RT");
        assert_eq!(q.blocks.len(), p.blocks.len());
        for (a, b) in p.blocks.iter().zip(q.blocks.iter()) {
            assert_eq!(a.element_type, b.element_type, "type for {:?}", a.text);
            assert_eq!(a.text, b.text);
        }
    }
}

/// Imported title page (Fountain `Key: value` header).
#[derive(Debug, Clone, Default)]
pub struct FountainMeta {
    pub title: String,
    pub credit: String,
    pub author: String,
    pub contact: String,
    pub draft_date: String,
}

/// Standard transition vocabulary (matches KIT's own transition list):
/// bare uppercase lines like these are transitions, never cues.
const KNOWN_TRANSITIONS: &[&str] = &[
    "FADE IN:",
    "FADE OUT:",
    "CUT TO:",
    "SMASH CUT TO:",
    "DISSOLVE TO:",
    "MATCH CUT TO:",
    "JUMP CUT TO:",
    "FADE TO:",
    "FADE TO BLACK",
    "BACK TO:",
];

fn is_scene_text(s: &str) -> bool {
    let up = s.to_uppercase();
    [
        "INT.", "EXT.", "EST.", "INT ", "EXT ", "EST ", "INT./EXT", "INT/EXT", "I/E",
    ]
    .iter()
    .any(|pre| up.starts_with(pre))
}

/// A line counts as a character cue when it holds no lowercase letters
/// (digits/punctuation/space allowed) and isn't a transition/scene.
fn looks_like_character(s: &str) -> bool {
    if s.is_empty() || s.len() > 60 {
        return false;
    }
    let has_letter = s.chars().any(|c| c.is_alphabetic());
    let has_lower = s.chars().any(|c| c.is_lowercase());
    has_letter && !has_lower && !s.to_uppercase().ends_with(" TO:")
}

/// Parse Fountain plain text into a project plus title-page metadata.
/// Inverse of [`to_fountain`] for everything it emits, plus the common
/// Fountain markers (`#` sections as notes, `~` lyrics, `[[notes]]`,
/// `>`, `!`, `.` escapes, `===` page breaks).
pub fn parse_fountain(text: &str) -> (Project, FountainMeta) {
    let mut meta = FountainMeta::default();
    let mut blocks: Vec<ScriptBlock> = Vec::new();

    // Title page: leading `Key: value` lines until the first blank line.
    let mut lines = text.lines().peekable();
    while let Some(&line) = lines.peek() {
        let t = line.trim();
        if t.is_empty() {
            lines.next();
            break;
        }
        if let Some((k, v)) = t.split_once(':') {
            match k.trim().to_lowercase().as_str() {
                "title" => meta.title = v.trim().to_string(),
                "credit" => meta.credit = v.trim().to_string(),
                "author" | "authors" => meta.author = v.trim().to_string(),
                "contact" | "contacts" => {
                    if meta.contact.is_empty() {
                        meta.contact = v.trim().to_string();
                    } else {
                        meta.contact.push('\n');
                        meta.contact.push_str(v.trim());
                    }
                }
                "draft date" => meta.draft_date = v.trim().to_string(),
                _ => {}
            }
            lines.next();
        } else {
            break;
        }
    }

    let rest: Vec<&str> = lines.collect();
    let mut i = 0usize;
    // Previous significant block type (drives dialogue continuation).
    let mut prev: Option<ElementType> = None;
    // Multi-line [[ note ]] accumulator.
    let mut open_note: Option<String> = None;

    // Look ahead to the next non-blank line.
    fn next_sig<'a>(mut j: usize, rest: &[&'a str]) -> Option<&'a str> {
        while j < rest.len() {
            let t = rest[j].trim();
            if !t.is_empty() {
                return Some(t);
            }
            j += 1;
        }
        None
    }

    let push = |blocks: &mut Vec<ScriptBlock>,
                prev: &mut Option<ElementType>,
                t: ElementType,
                s: String| {
        blocks.push(ScriptBlock::new(t, s));
        *prev = Some(t);
    };

    while i < rest.len() {
        let raw = rest[i];
        let t = raw.trim();
        i += 1;
        if t.is_empty() {
            continue;
        }

        if let Some(note) = open_note.take() {
            if let Some(end) = t.find("]]") {
                let mut full = note;
                full.push(' ');
                full.push_str(&t[..end]);
                push(
                    &mut blocks,
                    &mut prev,
                    ElementType::NonPrintable,
                    full.trim().to_string(),
                );
            } else {
                open_note = Some(format!("{note} {}", t.trim()));
            }
            continue;
        }

        if t == "===" {
            continue; // page break: layout is ours
        }
        if t.starts_with('#') {
            // Sections/outline become editor notes (non-printing).
            let s = t.trim_start_matches('#').trim();
            if !s.is_empty() {
                push(
                    &mut blocks,
                    &mut prev,
                    ElementType::NonPrintable,
                    s.to_string(),
                );
            }
            continue;
        }
        if let Some(stripped) = t.strip_prefix("[[") {
            if let Some(end) = stripped.find("]]") {
                let inner = stripped[..end].trim();
                push(
                    &mut blocks,
                    &mut prev,
                    ElementType::NonPrintable,
                    inner.to_string(),
                );
            } else {
                open_note = Some(stripped.trim().to_string());
            }
            continue;
        }
        if let Some(body) = t.strip_prefix('~') {
            push(
                &mut blocks,
                &mut prev,
                ElementType::Lyrics,
                body.trim().to_string(),
            );
            continue;
        }
        if let Some(body) = t.strip_prefix('>') {
            let body = body.trim().trim_end_matches('<').trim();
            push(
                &mut blocks,
                &mut prev,
                ElementType::Transition,
                body.to_string(),
            );
            continue;
        }
        if let Some(body) = t.strip_prefix('!') {
            push(
                &mut blocks,
                &mut prev,
                ElementType::Action,
                body.to_string(),
            );
            continue;
        }
        if let Some(body) = t.strip_prefix('.') {
            if !body.trim().is_empty() {
                push(
                    &mut blocks,
                    &mut prev,
                    ElementType::SceneHeading,
                    body.trim().to_string(),
                );
                continue;
            }
        }
        let up = t.to_uppercase();
        if KNOWN_TRANSITIONS.contains(&up.as_str()) {
            push(
                &mut blocks,
                &mut prev,
                ElementType::Transition,
                t.to_string(),
            );
            continue;
        }
        if t.len() > 3
            && t.len() < 40
            && t.to_uppercase().ends_with(" TO:")
            && !t.chars().any(|c| c.is_lowercase())
        {
            // Un-prefixed transitions like CUT TO: (uppercase, short).
            push(
                &mut blocks,
                &mut prev,
                ElementType::Transition,
                t.to_string(),
            );
            continue;
        }
        if is_scene_text(t) {
            push(
                &mut blocks,
                &mut prev,
                ElementType::SceneHeading,
                t.to_string(),
            );
            continue;
        }
        if t.starts_with('(') && t.ends_with(')') {
            push(
                &mut blocks,
                &mut prev,
                ElementType::Parenthetical,
                t.to_string(),
            );
            continue;
        }
        if looks_like_character(t) {
            // Character cue only when followed by dialogue-ish content;
            // otherwise it's an uppercase action line (e.g. a sign).
            let follows_dialog = matches!(next_sig(i, &rest), Some(n)
                if n.starts_with('(') && n.ends_with(')')
                    || (!looks_like_character(n) && !is_scene_text(n) && !n.ends_with(" TO:")));
            if follows_dialog || matches!(prev, Some(ElementType::Parenthetical)) {
                push(
                    &mut blocks,
                    &mut prev,
                    ElementType::Character,
                    t.to_string(),
                );
                continue;
            }
        }
        // Dialogue continues after a cue / parenthetical / dialogue line.
        if matches!(
            prev,
            Some(ElementType::Character)
                | Some(ElementType::Parenthetical)
                | Some(ElementType::Dialogue)
        ) {
            push(&mut blocks, &mut prev, ElementType::Dialogue, t.to_string());
            continue;
        }
        push(&mut blocks, &mut prev, ElementType::Action, t.to_string());
    }
    if let Some(note) = open_note.take() {
        if !note.trim().is_empty() {
            push(
                &mut blocks,
                &mut prev,
                ElementType::NonPrintable,
                note.trim().to_string(),
            );
        }
    }

    let title = if meta.title.is_empty() {
        "Untitled".to_string()
    } else {
        meta.title.clone()
    };
    (
        Project {
            title,
            blocks,
            ..Default::default()
        },
        meta,
    )
}
