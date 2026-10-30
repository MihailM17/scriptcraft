//! Plain-text screenplay recognition for pasting from chat or text editors.
//!
//! KIT Scenarist formats pasted screenplay prose into scene headings, action,
//! cues, parentheticals, dialogue and transitions. ScriptCraft's editor only
//! receives plain text from the system clipboard, so this module applies the
//! same kind of recognition before the blocks reach the document. Ordinary
//! multi-line prose is rejected and keeps the editor's previous insertion
//! behaviour.

use crate::editing::auto_upper;
use crate::model::{ElementType, ScriptBlock};

/// Parse clipboard text that looks like a screenplay into typed blocks.
///
/// Returns `None` when the text has no scene heading or character cue, so a
/// pasted address, note or action paragraph is not split unexpectedly.
pub fn parse(text: &str) -> Option<Vec<ScriptBlock>> {
    let lines: Vec<String> = text.lines().map(clean_line).collect();
    let nonblank = lines.iter().filter(|l| !l.trim().is_empty()).count();
    if nonblank < 2 {
        return None;
    }

    let mut blocks = Vec::new();
    let mut in_dialogue = false;
    for (i, line) in lines.iter().enumerate() {
        let text = line.trim();
        if text.is_empty() {
            in_dialogue = false;
            continue;
        }

        let element_type = if is_scene_heading(text) {
            in_dialogue = false;
            ElementType::SceneHeading
        } else if is_transition(text) {
            in_dialogue = false;
            ElementType::Transition
        } else if in_dialogue && is_parenthetical(text) {
            ElementType::Parenthetical
        } else if in_dialogue {
            ElementType::Dialogue
        } else if looks_like_character(text)
            && (i == 0 || lines[i - 1].trim().is_empty())
            && followed_by_speech(&lines, i)
        {
            in_dialogue = true;
            ElementType::Character
        } else {
            in_dialogue = false;
            ElementType::Action
        };

        let text = if auto_upper(element_type) {
            text.to_uppercase()
        } else {
            text.to_string()
        };
        blocks.push(ScriptBlock::new(element_type, text));
    }

    let looks_like_screenplay = blocks
        .iter()
        .any(|b| b.element_type == ElementType::SceneHeading)
        || blocks
            .iter()
            .filter(|b| b.element_type == ElementType::Character)
            .count()
            >= 2;
    if !looks_like_screenplay {
        return None;
    }
    Some(blocks)
}

/// Normalise one pasted line: strip markdown wrappers, code-fence markers and
/// scene numbers so the content rules see the screenplay text.
fn clean_line(raw: &str) -> String {
    let mut line = raw.replace('\u{00a0}', " ").trim().to_string();
    if line.starts_with("```") {
        return String::new();
    }
    if is_markdown_rule(&line) {
        return String::new();
    }
    while let Some(rest) = line.strip_prefix('#') {
        line = rest.trim_start().to_string();
    }
    if let Some(rest) = line.strip_prefix('>') {
        line = rest.trim_start().to_string();
    }
    line = line.replace("**", "").replace("__", "");
    strip_scene_number(&line)
}

fn is_markdown_rule(line: &str) -> bool {
    let t: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    t.len() >= 3
        && (t.chars().all(|c| c == '-')
            || t.chars().all(|c| c == '*')
            || t.chars().all(|c| c == '_'))
}

/// Drop a leading scene number only when the rest is a scene heading, e.g.
/// `1. INT. ROOM - DAY`, `1A INT. ROOM - DAY`, `Scene 12: INT. ROOM - DAY`.
fn strip_scene_number(line: &str) -> String {
    let t = line.trim_start();

    if let Some(prefix) = t.get(..6) {
        if prefix.eq_ignore_ascii_case("scene ") {
            if let Some(end) = t[6..].find([':', '.', '-']) {
                let candidate = t[6 + end + 1..].trim_start();
                if is_scene_heading(candidate) {
                    return candidate.to_string();
                }
            }
        }
    }

    let chars: Vec<char> = t.chars().collect();
    let mut end = 0;
    while end < chars.len() && chars[end].is_ascii_digit() {
        end += 1;
    }
    if end == 0 {
        return t.to_string();
    }
    if end < chars.len() && chars[end].is_ascii_alphabetic() {
        end += 1;
    }
    if end < chars.len() && (chars[end] == '.' || chars[end] == ')') {
        end += 1;
    }
    if end < chars.len() && chars[end].is_whitespace() {
        let candidate: String = chars[end..].iter().collect();
        let candidate = candidate.trim_start();
        if is_scene_heading(candidate) {
            return candidate.to_string();
        }
    }
    t.to_string()
}

fn is_scene_heading(text: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "INT.",
        "EXT.",
        "EST.",
        "INT ",
        "EXT ",
        "EST ",
        "I/E.",
        "I/E ",
        "ИНТ.",
        "ЕКСТ.",
        "НАТ.",
        "ИНТ ",
        "ЕКСТ ",
        "НАТ ",
    ];
    let upper = text.trim_start().to_uppercase();
    PREFIXES.iter().any(|prefix| upper.starts_with(prefix))
}

fn is_transition(text: &str) -> bool {
    const KNOWN: &[&str] = &[
        "FADE IN:",
        "FADE IN.",
        "FADE OUT:",
        "FADE OUT.",
        "FADE TO:",
        "FADE TO BLACK.",
        "CUT TO:",
        "CUT TO BLACK.",
        "SMASH CUT TO:",
        "MATCH CUT TO:",
        "JUMP CUT TO:",
        "DISSOLVE TO:",
        "WIPE TO:",
        "BACK TO:",
        "IRIS OUT.",
        "END.",
        "THE END.",
    ];
    let upper = text.trim().to_uppercase();
    if KNOWN.contains(&upper.as_str()) {
        return true;
    }
    let has_lower = text.chars().any(|c| c.is_lowercase());
    upper.len() > 3 && upper.len() < 50 && upper.ends_with(" TO:") && !has_lower
}

fn is_parenthetical(text: &str) -> bool {
    let t = text.trim();
    t.starts_with('(') && t.ends_with(')')
}

fn looks_like_character(text: &str) -> bool {
    let t = text.trim();
    let len = t.chars().count();
    if len == 0 || len > 60 {
        return false;
    }
    let has_letter = t.chars().any(|c| c.is_alphabetic());
    let has_lower = t.chars().any(|c| c.is_lowercase());
    if !has_letter || has_lower {
        return false;
    }
    if is_scene_heading(t) || is_transition(t) || t.starts_with('(') {
        return false;
    }
    // All-caps action usually ends a sentence; character cues do not.
    if t.ends_with('.') {
        return false;
    }
    let upper = t.to_uppercase();
    !["SUPER:", "TITLE:", "INSERT:", "INTERCUT"]
        .iter()
        .any(|prefix| upper.starts_with(prefix))
}

/// A cue is only a cue when the very next line (no blank line between)
/// starts the speech. That keeps isolated all-caps action from matching.
fn followed_by_speech(lines: &[String], i: usize) -> bool {
    let Some(next) = lines.get(i + 1) else {
        return false;
    };
    let next = next.trim();
    if next.is_empty() || is_scene_heading(next) || is_transition(next) {
        return false;
    }
    // Another all-caps line directly below is more likely a stacked action
    // line than dialogue.
    let has_lower = next.chars().any(|c| c.is_lowercase());
    if !has_lower && !is_parenthetical(next) {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<(ElementType, String)> {
        parse(text)
            .expect("screenplay")
            .into_iter()
            .map(|b| (b.element_type, b.text))
            .collect()
    }

    #[test]
    fn recognises_a_plain_screenplay() {
        let text = "\
INT. COFFEE SHOP - DAY

Sunlight cuts through the blinds.

MAYA
(to herself)
Just one more page.

She opens her laptop.

CUT TO:

EXT. STREET - NIGHT

Rain falls.";
        let blocks = kinds(text);
        assert_eq!(
            blocks,
            vec![
                (ElementType::SceneHeading, "INT. COFFEE SHOP - DAY".into()),
                (
                    ElementType::Action,
                    "Sunlight cuts through the blinds.".into()
                ),
                (ElementType::Character, "MAYA".into()),
                (ElementType::Parenthetical, "(to herself)".into()),
                (ElementType::Dialogue, "Just one more page.".into()),
                (ElementType::Action, "She opens her laptop.".into()),
                (ElementType::Transition, "CUT TO:".into()),
                (ElementType::SceneHeading, "EXT. STREET - NIGHT".into()),
                (ElementType::Action, "Rain falls.".into()),
            ]
        );
    }

    #[test]
    fn rejects_ordinary_multiline_text() {
        assert!(parse("Meet me at 10.\nBring the notes.").is_none());
        assert!(parse("just one line").is_none());
        assert!(parse("MEETING NOTES\nAttendees: Maya and John.").is_none());
    }

    #[test]
    fn strips_markdown_and_scene_numbers() {
        let blocks = kinds(
            "### **1. INT. OFFICE - DAY**\n\n\
             > **MAYA** (V.O.)\n\
             (quietly)\n\
             We start now.",
        );
        assert_eq!(blocks[0].0, ElementType::SceneHeading);
        assert_eq!(blocks[0].1, "INT. OFFICE - DAY");
        assert_eq!(blocks[1], (ElementType::Character, "MAYA (V.O.)".into()));
        assert_eq!(blocks[2], (ElementType::Parenthetical, "(quietly)".into()));
        assert_eq!(blocks[3], (ElementType::Dialogue, "We start now.".into()));
    }

    #[test]
    fn blank_line_ends_dialogue() {
        let blocks = kinds("MAYA\nHello.\n\nShe leaves.\n\nINT. HALL - DAY");
        assert_eq!(blocks[0].0, ElementType::Character);
        assert_eq!(blocks[1], (ElementType::Dialogue, "Hello.".into()));
        assert_eq!(blocks[2], (ElementType::Action, "She leaves.".into()));
        assert_eq!(blocks[3].0, ElementType::SceneHeading);
    }

    #[test]
    fn all_caps_action_stays_action() {
        let blocks = kinds("INT. STREET - NIGHT\n\nA GUNSHOT.\n\nMaya runs.");
        assert_eq!(blocks[1], (ElementType::Action, "A GUNSHOT.".into()));
        assert_eq!(blocks[2], (ElementType::Action, "Maya runs.".into()));
    }

    #[test]
    fn parenthetical_outside_dialogue_is_action() {
        let blocks = kinds("INT. ROOM - DAY\n\n(An empty chair.)");
        assert_eq!(blocks[1], (ElementType::Action, "(An empty chair.)".into()));
    }

    #[test]
    fn recognises_cyrillic_headings_and_mixed_case() {
        let blocks = kinds("Инт. КУХНЯ – ДЕН\n\nТя се обръща.");
        assert_eq!(blocks[0].0, ElementType::SceneHeading);
        assert_eq!(blocks[0].1, "ИНТ. КУХНЯ – ДЕН");
    }
}
