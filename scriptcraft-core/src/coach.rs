use crate::editing::{base_character_name, col_chars, wrap_rows_chars};
use crate::model::{ElementType, Project};

/// Screenwriting coaching notes (new-writer lints). Unlike [`crate::format`]
/// issues (objective formatting errors), these are craft suggestions with
/// the same shape so the Tools tab can render them together.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    Hint,
    Warn,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub block: usize,
    pub code: &'static str,
    pub message: String,
    pub severity: Severity,
}

fn note(block: usize, code: &'static str, severity: Severity, message: impl Into<String>) -> Note {
    Note {
        block,
        code,
        message: message.into(),
        severity,
    }
}

/// Unfilmable camera-direction verbs (EN + BG).
const UNFILMABLE: &[&str] = &[
    "we see",
    "we hear",
    "we notice",
    "виждаме",
    "чуваме",
    "забелязваме",
];

/// Lint a project. Pure function of the blocks — fully unit-testable.
pub fn lint(p: &Project) -> Vec<Note> {
    let mut notes = Vec::new();
    let mut introduced: Vec<String> = Vec::new();
    let mut talk_run_start: Option<usize> = None;
    let mut talk_run_len = 0usize;
    let mut scene_parens = 0usize;
    let mut scene_lines = 0usize;
    let mut scene_head: Option<usize> = None;

    let flush_talk = |notes: &mut Vec<Note>, start: Option<usize>, len: usize| {
        if len >= 4 {
            if let Some(bi) = start {
                notes.push(note(
                    bi,
                    "talking-heads",
                    Severity::Warn,
                    format!(
                        "{len} lines of uninterrupted dialogue — give the eye something to see"
                    ),
                ));
            }
        }
    };
    let flush_scene = |notes: &mut Vec<Note>, head: Option<usize>, parens: usize, lines: usize| {
        if let Some(h) = head {
            if parens > 3 {
                notes.push(note(
                    h,
                    "parenthetical-heavy",
                    Severity::Hint,
                    format!(
                        "{parens} parentheticals in one scene — actors hate these; act it instead"
                    ),
                ));
            }
            if lines > 55 {
                notes.push(note(
                    h,
                    "long-scene",
                    Severity::Warn,
                    format!(
                        "scene runs ~{} pages — consider splitting it",
                        lines / 55 + 1
                    ),
                ));
            }
        }
    };

    for (bi, b) in p.blocks.iter().enumerate() {
        if matches!(b.element_type, ElementType::SceneHeading) {
            flush_talk(&mut notes, talk_run_start.take(), talk_run_len);
            talk_run_len = 0;
            flush_scene(&mut notes, scene_head.take(), scene_parens, scene_lines);
            scene_parens = 0;
            scene_lines = 0;
            scene_head = Some(bi);
        }
        let lines = wrap_rows_chars(&b.text, col_chars(b.element_type)).max(1);
        scene_lines += lines;

        match b.element_type {
            ElementType::Dialogue | ElementType::Parenthetical => {
                if talk_run_len == 0 {
                    talk_run_start = Some(bi);
                }
                talk_run_len += 1;
                if matches!(b.element_type, ElementType::Parenthetical) {
                    scene_parens += 1;
                }
                if matches!(b.element_type, ElementType::Dialogue) {
                    let alpha: String = b.text.chars().filter(|c| c.is_alphabetic()).collect();
                    if alpha.chars().count() > 20
                        && alpha == alpha.to_uppercase()
                        && alpha != alpha.to_lowercase()
                    {
                        notes.push(note(
                            bi,
                            "shouting",
                            Severity::Hint,
                            "all-caps dialogue reads as shouting — use sparingly",
                        ));
                    }
                    if b.text.chars().any(|c| c.is_ascii_digit()) {
                        notes.push(note(
                            bi,
                            "digits-in-dialogue",
                            Severity::Hint,
                            "write out numbers in dialogue (actors speak words, not digits)",
                        ));
                    }
                }
            }
            ElementType::Character => {
                let base = base_character_name(&b.text).trim().to_string();
                let known = introduced.iter().any(|n| n == &base);
                if !known {
                    let tokens: Vec<String> = base
                        .split_whitespace()
                        .filter(|t| t.chars().count() >= 3)
                        .map(|t| t.to_lowercase())
                        .collect();
                    let mentioned = p.blocks[..bi].iter().any(|prev| {
                        matches!(prev.element_type, ElementType::Action | ElementType::Shot) && {
                            let hay = prev.text.to_lowercase();
                            tokens.iter().any(|t| hay.contains(t.as_str()))
                        }
                    });
                    if !mentioned && !tokens.is_empty() {
                        notes.push(note(
                            bi,
                            "unintroduced-speaker",
                            Severity::Hint,
                            format!("{base} speaks with no prior introduction — name them in action first"),
                        ));
                    }
                    introduced.push(base);
                }
                if talk_run_len == 0 {
                    talk_run_start = Some(bi);
                }
            }
            ElementType::Action | ElementType::Shot => {
                flush_talk(&mut notes, talk_run_start.take(), talk_run_len);
                talk_run_len = 0;
                if lines > 4 {
                    notes.push(note(
                        bi,
                        "long-action",
                        Severity::Hint,
                        format!("{lines}-line action block — consider trimming to 3–4 lines"),
                    ));
                }
                let low = b.text.to_lowercase();
                if UNFILMABLE.iter().any(|w| low.contains(w)) {
                    notes.push(note(
                        bi,
                        "unfilmable",
                        Severity::Hint,
                        "unfilmable direction (we see / we hear) — show it instead",
                    ));
                }
            }
            _ => {
                flush_talk(&mut notes, talk_run_start.take(), talk_run_len);
                talk_run_len = 0;
            }
        }
    }
    flush_talk(&mut notes, talk_run_start, talk_run_len);
    flush_scene(&mut notes, scene_head, scene_parens, scene_lines);
    notes.sort_by_key(|n| n.block);
    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ScriptBlock;

    fn project(parts: Vec<(ElementType, &str)>) -> Project {
        let mut p = Project {
            title: "T".into(),
            blocks: vec![],
        };
        for (t, s) in parts {
            p.blocks.push(ScriptBlock::new(t, s.into()));
        }
        p
    }

    #[test]
    fn flags_long_action_and_unfilmable() {
        use ElementType::*;
        let long: String = (0..60)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let p = project(vec![
            (SceneHeading, "EXT. PARK - DAY"),
            (Action, &long),
            (
                Action,
                "We see a man walking slowly across the empty parking lot.",
            ),
        ]);
        let codes: Vec<&str> = lint(&p).iter().map(|n| n.code).collect();
        assert!(codes.contains(&"long-action"), "{codes:?}");
        assert!(codes.contains(&"unfilmable"), "{codes:?}");
    }

    #[test]
    fn flags_talking_heads_runs() {
        use ElementType::*;
        let p = project(vec![
            (SceneHeading, "INT. ROOM - DAY"),
            (Character, "IVAN"),
            (Dialogue, "One."),
            (Dialogue, "Two."),
            (Parenthetical, "(beat)"),
            (Dialogue, "Three."),
            (Dialogue, "Four."),
        ]);
        // "IVAN" cue: never introduced in action → also flagged.
        let codes: Vec<&str> = lint(&p).iter().map(|n| n.code).collect();
        assert!(codes.contains(&"talking-heads"), "{codes:?}");
    }

    #[test]
    fn dialogue_after_cue_does_not_trigger_unintroduced_when_named() {
        use ElementType::*;
        let p = project(vec![
            (SceneHeading, "EXT. PARK - DAY"),
            (Action, "Ivan waits by the gate."),
            (Character, "IVAN"),
            (Dialogue, "Hi."),
        ]);
        let codes: Vec<&str> = lint(&p).iter().map(|n| n.code).collect();
        assert!(!codes.contains(&"unintroduced-speaker"), "{codes:?}");
        assert!(!codes.contains(&"talking-heads"), "{codes:?}");
    }

    #[test]
    fn flags_shouting_digits_and_heavy_parens() {
        use ElementType::*;
        let p = project(vec![
            (SceneHeading, "INT. ROOM - DAY"),
            (Character, "IVAN"),
            (Dialogue, "I TOLD YOU THREE TIMES ALREADY TO KNOCK FIRST"),
            (Parenthetical, "(beat)"),
            (Parenthetical, "(pause)"),
            (Parenthetical, "(sighs)"),
            (Parenthetical, "(looks away)"),
            (Dialogue, "Call me at 555 tomorrow."),
        ]);
        let codes: Vec<&str> = lint(&p).iter().map(|n| n.code).collect();
        assert!(codes.contains(&"shouting"), "{codes:?}");
        assert!(codes.contains(&"digits-in-dialogue"), "{codes:?}");
        assert!(codes.contains(&"parenthetical-heavy"), "{codes:?}");
    }
}
