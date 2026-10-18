use crate::model::{ElementType, Project};

/// One formatting problem found by validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    pub block: usize,
    pub code: &'static str,
    pub message: String,
}

/// Shot/SFX labels that must never be `<character>`.
const MISUSED_CHARACTER_MARKERS: &[&str] = &[
    "БЛИЗЪК ПЛАН",
    "БУУУМ!",
    "ВЪРТ-ВЪРТ-ВЪРТ!",
    "ЦЪК-ЦЪК",
    "А) ",
    "Б) ",
    "В) ",
    "Г) ",
];

/// Validate formatting rules (the ones we fixed in A95).
/// Returns empty vec = clean.
pub fn validate(p: &Project) -> Vec<Issue> {
    let mut issues = Vec::new();

    for (i, b) in p.blocks.iter().enumerate() {
        match b.element_type {
            ElementType::SceneHeading => {
                if b.text != b.text.trim() {
                    issues.push(issue(
                        i,
                        "heading-space",
                        "heading has leading/trailing space",
                    ));
                }
                if b.text.ends_with('.') && !b.text.ends_with("...") {
                    issues.push(issue(i, "heading-period", "heading ends with '.'"));
                }
                // lowercase time/prefix markers (latin) = not normalized
                for bad in [
                    "morning",
                    "night",
                    "day",
                    "moments",
                    "continuous",
                    "Int.",
                    "Ext.",
                    "the fight",
                    "conclusion",
                    "explosion",
                    "НОЩ",
                ] {
                    if b.text.contains(bad) {
                        issues.push(issue(
                            i,
                            "heading-case",
                            format!("heading contains non-standard {:?}", bad),
                        ));
                        break;
                    }
                }
            }
            ElementType::Character => {
                if b.text != b.text.trim() {
                    issues.push(issue(
                        i,
                        "char-space",
                        "character has leading/trailing space",
                    ));
                }
                for m in MISUSED_CHARACTER_MARKERS {
                    if b.text == *m
                        || b.text.starts_with(m) && b.text.contains("БЛИЗЪК")
                        || b.text.starts_with("А) ")
                        || b.text.starts_with("Б) ")
                        || b.text.starts_with("В) ")
                        || b.text.starts_with("Г) ")
                    {
                        // precise: exact SFX or starts with montage letter / shot label
                        if b.text.starts_with("БЛИЗЪК ПЛАН")
                            || b.text == "БУУУМ!"
                            || b.text == "ВЪРТ-ВЪРТ-ВЪРТ!"
                            || b.text.starts_with("ЦЪК-ЦЪК")
                            || ["А) ", "Б) ", "В) ", "Г) "]
                                .iter()
                                .any(|x| b.text.starts_with(x))
                        {
                            issues.push(issue(
                                i,
                                "char-misused",
                                format!("{:?} must be Action, not Character", b.text),
                            ));
                            break;
                        }
                    }
                }
            }
            ElementType::Dialogue => {
                if b.text.contains("&quot;") {
                    issues.push(issue(i, "entity", "dialog contains literal &quot;"));
                }
                // orphan check: previous non-dialog block must be character/parenthetical
                if i == 0 {
                    issues.push(issue(i, "orphan-dialog", "dialog without character cue"));
                } else {
                    let mut j = i;
                    loop {
                        j -= 1;
                        match p.blocks[j].element_type {
                            ElementType::Dialogue => {
                                if j == 0 {
                                    issues.push(issue(
                                        i,
                                        "orphan-dialog",
                                        "dialog without character cue",
                                    ));
                                    break;
                                }
                                continue;
                            }
                            ElementType::Character | ElementType::Parenthetical => break,
                            _ => {
                                issues.push(issue(
                                    i,
                                    "orphan-dialog",
                                    "dialog without character cue",
                                ));
                                break;
                            }
                        }
                    }
                }
            }
            ElementType::Parenthetical => {
                if b.text == "(ПРОД.)" {
                    issues.push(issue(
                        i,
                        "prod-paren",
                        "(ПРОД.) should be character extension, not parenthetical",
                    ));
                }
            }
            ElementType::Action
            | ElementType::Transition
            | ElementType::SceneCharacters
            | ElementType::Shot
            | ElementType::Lyrics
            | ElementType::NonPrintable => {
                if b.text.contains("&quot;") {
                    issues.push(issue(i, "entity", "block contains literal &quot;"));
                }
            }
        }
        if b.text.contains('—') {
            issues.push(issue(i, "em-dash", "block contains em-dash — (use –)"));
        }
    }

    issues
}

fn issue(block: usize, code: &'static str, message: impl Into<String>) -> Issue {
    Issue {
        block,
        code,
        message: message.into(),
    }
}
