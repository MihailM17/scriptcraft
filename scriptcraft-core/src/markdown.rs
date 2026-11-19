//! Markdown export (one-way, for notes, wikis and plain-text diffing).
//!
//! Screenplay structure is expressed with Markdown emphasis: scene headings
//! and character cues are bold, transitions and lyrics italic, and
//! non-printing notes become blockquotes. Text is escaped so a line that
//! happens to start with `#`, `>` or `-` still renders as the script wrote
//! it.

use crate::kitsp::Metadata;
use crate::model::{ElementType, Project};

/// Escape inline Markdown characters and line-leading block markers.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' | '*' | '_' | '`' | '[' | ']' | '<' | '>' => {
                out.push('\\');
                out.push(ch);
            }
            '\r' | '\n' | '\t' => out.push(' '),
            _ => out.push(ch),
        }
    }
    let needs_escape = out.starts_with('#')
        || out.starts_with('-')
        || out.starts_with('+')
        || out.starts_with('=')
        || out.starts_with('!')
        || out.starts_with('|')
        || out
            .split_once('.')
            .or_else(|| out.split_once(')'))
            .is_some_and(|(head, _)| !head.is_empty() && head.chars().all(|c| c.is_ascii_digit()));
    if needs_escape {
        out.insert(0, '\\');
    }
    out
}

/// Wrap each line of a contact block so Markdown keeps the line breaks.
fn contact_lines(contacts: &str) -> String {
    contacts
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(escape)
        .collect::<Vec<_>>()
        .join("  \n")
}

/// Render a project as a Markdown document.
pub fn to_markdown(p: &Project, meta: &Metadata) -> String {
    let title = if meta.title.trim().is_empty() {
        p.title.trim()
    } else {
        meta.title.trim()
    };
    let mut out = String::new();
    out.push_str("# ");
    out.push_str(&escape(if title.is_empty() { "Untitled" } else { title }));
    out.push_str("\n\n");

    if !meta.genre.trim().is_empty() {
        out.push('*');
        out.push_str(&escape(meta.genre.trim()));
        out.push_str("*\n\n");
    }
    if !meta.author.trim().is_empty() {
        out.push_str("**Written by ");
        out.push_str(&escape(meta.author.trim()));
        out.push_str("**\n\n");
    }
    if !meta.additional_info.trim().is_empty() {
        out.push_str("Source: ");
        out.push_str(&escape(meta.additional_info.trim()));
        out.push_str("\n\n");
    }
    if !meta.year.trim().is_empty() {
        out.push_str("Draft date: ");
        out.push_str(&escape(meta.year.trim()));
        out.push_str("\n\n");
    }
    if !meta.contacts.trim().is_empty() {
        out.push_str("Contact:\n");
        out.push_str(&contact_lines(&meta.contacts));
        out.push_str("\n\n");
    }
    out.push_str("---\n\n");

    let mut wrote = false;
    for b in &p.blocks {
        let text = b.text.trim();
        if text.is_empty() {
            continue;
        }
        if wrote {
            out.push('\n');
        }
        let body = escape(text);
        match b.element_type {
            ElementType::SceneHeading | ElementType::SceneCharacters | ElementType::Character => {
                out.push_str("**");
                out.push_str(&body);
                out.push_str("**\n");
            }
            ElementType::Parenthetical => {
                out.push('*');
                out.push_str(&body);
                out.push_str("*\n");
            }
            ElementType::Transition | ElementType::Lyrics => {
                out.push('*');
                out.push_str(&body);
                out.push_str("*\n");
            }
            ElementType::NonPrintable => {
                out.push_str("> **Note:** ");
                out.push_str(&body);
                out.push('\n');
            }
            ElementType::Action | ElementType::Dialogue | ElementType::Shot => {
                out.push_str(&body);
                out.push('\n');
            }
        }
        wrote = true;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ElementType::*;
    use crate::model::ScriptBlock;

    fn project() -> Project {
        let mut p = Project {
            title: "The Morning Ritual".into(),
            ..Default::default()
        };
        for (t, text) in [
            (SceneHeading, "INT. COFFEE SHOP - MORNING"),
            (Action, "The room hums with morning chatter."),
            (Character, "MIRA"),
            (Parenthetical, "(over the noise)"),
            (Dialogue, "One espresso, please."),
            (Shot, "CLOSE ON THE ESPRESSO MACHINE"),
            (Transition, "CUT TO:"),
            (Lyrics, "Raindrops keep falling..."),
            (NonPrintable, "Check with production."),
        ] {
            p.blocks.push(ScriptBlock::new(t, text.into()));
        }
        p
    }

    fn meta() -> Metadata {
        Metadata {
            title: "The Morning Ritual".into(),
            genre: "Drama".into(),
            author: "M. Hayll".into(),
            contacts: "mihail@example.com\n+1 555 0100".into(),
            additional_info: "Based on a short story".into(),
            year: "2026".into(),
            ..Default::default()
        }
    }

    #[test]
    fn renders_title_page_and_screenplay_structure() {
        let md = to_markdown(&project(), &meta());
        assert!(md.starts_with("# The Morning Ritual\n\n"));
        assert!(md.contains("*Drama*"));
        assert!(md.contains("**Written by M. Hayll**"));
        assert!(md.contains("Source: Based on a short story"));
        assert!(md.contains("Draft date: 2026"));
        assert!(md.contains("Contact:\nmihail@example.com  \n\\+1 555 0100"));
        assert!(md.contains("---\n\n"));
        assert!(md.contains("**INT. COFFEE SHOP - MORNING**"));
        assert!(md.contains("**MIRA**"));
        assert!(md.contains("*(over the noise)*"));
        assert!(md.contains("*CUT TO:*"));
        assert!(md.contains("*Raindrops keep falling...*"));
        assert!(md.contains("> **Note:** Check with production."));
        assert!(md.contains("CLOSE ON THE ESPRESSO MACHINE"));
    }

    #[test]
    fn escapes_markdown_that_would_change_the_script() {
        let mut p = project();
        p.blocks
            .push(ScriptBlock::new(Action, "# not a heading".into()));
        p.blocks.push(ScriptBlock::new(
            Action,
            "- not a list, *not italic*".into(),
        ));
        p.blocks
            .push(ScriptBlock::new(Action, "1. not an ordered list".into()));
        let md = to_markdown(&p, &meta());
        assert!(md.contains("\\# not a heading"));
        assert!(md.contains("\\- not a list, \\*not italic\\*"));
        assert!(md.contains("\\1. not an ordered list"));
    }

    #[test]
    fn empty_project_still_produces_a_document() {
        let md = to_markdown(&Project::default(), &Metadata::default());
        assert_eq!(md, "# Untitled\n\n---\n\n");
    }
}
