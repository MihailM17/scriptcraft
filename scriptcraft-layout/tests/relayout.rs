//! Differential fuzz for incremental relayout.
//!
//! `place::relayout_from` must reproduce `place::layout` exactly, for every
//! kind of edit, on both a synthetic corpus of realistic speech runs and the
//! live A95 fixture. Each case mutates a document with a real
//! [`EditCommand`] (the same one the editor would record), derives the
//! relayout hint exactly like `scriptcraft-app` does, and compares the
//! incremental result against a full relayout of the mutated blocks.

use scriptcraft_core::edit::{EditCommand, Pos};
use scriptcraft_core::editing::RUSSIAN;
use scriptcraft_core::kitsp;
use scriptcraft_core::model::{ElementType, ScriptBlock};
use scriptcraft_layout::{layout, relayout_from, DocLayout, RUSSIAN_GEOMETRY};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/A95.kitsp");

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn scene_labels(blocks: &[ScriptBlock]) -> Vec<String> {
    let count = blocks
        .iter()
        .filter(|b| b.element_type == ElementType::SceneHeading)
        .count();
    (1..=count).map(|i| format!("{i}.")).collect()
}

fn full(blocks: &[ScriptBlock]) -> DocLayout {
    layout(blocks, &RUSSIAN, &RUSSIAN_GEOMETRY, &scene_labels(blocks))
}

/// The relayout hint `scriptcraft-app` would pass for this command.
fn hint_for(cmd: &EditCommand) -> (usize, Option<usize>) {
    match *cmd {
        EditCommand::InsertText { at, .. } => (at.bi, Some(at.bi)),
        EditCommand::DeleteRange { start, .. } => (start.bi, Some(start.bi)),
        EditCommand::SetText { bi, .. } => (bi, Some(bi)),
        EditCommand::SetType { bi, .. } => (bi, None),
        EditCommand::SplitBlock { at, .. } => (at.bi, None),
        EditCommand::MergeBlocks { bi, .. } => (bi, None),
        EditCommand::InsertBlock { at, .. } => (at.saturating_sub(1), None),
        EditCommand::DeleteBlock { bi } => (bi, None),
        EditCommand::ReplaceAll { .. } => (0, None),
    }
}

// ---------------------------------------------------------------------------
// Synthetic corpus: scenes of speech runs, long enough that dialogues split
// across pages (the hard continuation cases).
// ---------------------------------------------------------------------------

const WORDS: &[&str] = &[
    "дом",
    "слънце",
    "по-малко",
    "10-литровите",
    "на",
    "и",
    "много-дълга-дума-тук",
    "Тест",
    "що-годе",
    "ето",
    "значи",
    "тихо",
];
const NAMES: &[&str] = &["ИВАН", "МАРИЯ", "Д-Р ПЕТРОВ", "БАБА", "СТРАЖАРЯТ"];

fn words(rng: &mut Rng, n: usize) -> String {
    let mut out = String::new();
    for w in 0..n {
        if w > 0 {
            out.push(' ');
        }
        out.push_str(WORDS[rng.below(WORDS.len())]);
    }
    out
}

fn paragraphs(rng: &mut Rng, max: usize) -> String {
    let n = 1 + rng.below(max);
    let mut out = String::new();
    for p in 0..n {
        if p > 0 {
            out.push('\n');
        }
        let n = 1 + rng.below(14);
        out.push_str(&words(rng, n));
    }
    out
}

fn document(rng: &mut Rng) -> Vec<ScriptBlock> {
    let scenes = 3 + rng.below(6);
    let mut blocks = Vec::new();
    for _ in 0..scenes {
        blocks.push(ScriptBlock::new(
            ElementType::SceneHeading,
            format!("НА ВЪТРЕ — {}", WORDS[rng.below(WORDS.len())]),
        ));
        blocks.push(ScriptBlock::new(ElementType::Action, paragraphs(rng, 6)));
        if rng.below(2) == 0 {
            blocks.push(ScriptBlock::new(
                ElementType::SceneCharacters,
                NAMES[rng.below(NAMES.len())].to_string(),
            ));
        }
        let runs = 1 + rng.below(3);
        for _ in 0..runs {
            blocks.push(ScriptBlock::new(
                ElementType::Character,
                NAMES[rng.below(NAMES.len())].to_string(),
            ));
            if rng.below(3) == 0 {
                let n = 1 + rng.below(4);
                blocks.push(ScriptBlock::new(
                    ElementType::Parenthetical,
                    format!("({})", words(rng, n)),
                ));
            }
            blocks.push(ScriptBlock::new(ElementType::Dialogue, paragraphs(rng, 45)));
        }
        if rng.below(4) == 0 {
            blocks.push(ScriptBlock::new(ElementType::Lyrics, paragraphs(rng, 8)));
        }
        if rng.below(3) == 0 {
            blocks.push(ScriptBlock::new(
                ElementType::Transition,
                "ПРЕМИНАВАМЕ КЪМ".to_string(),
            ));
        }
    }
    blocks
}

fn random_pos(rng: &mut Rng, blocks: &[ScriptBlock]) -> Pos {
    let bi = rng.below(blocks.len());
    let paras: Vec<&str> = blocks[bi].text.split('\n').collect();
    let para = rng.below(paras.len());
    let ch = rng.below(paras[para].chars().count() + 1);
    Pos { bi, para, ch }
}

fn random_edit(rng: &mut Rng, blocks: &[ScriptBlock]) -> EditCommand {
    match rng.below(8) {
        0 => {
            let at = random_pos(rng, blocks);
            let n = 1 + rng.below(3);
            let mut text = words(rng, n);
            if rng.below(4) == 0 {
                text.push('\n');
                let m = 1 + rng.below(6);
                text.push_str(&words(rng, m));
            }
            EditCommand::InsertText { at, text }
        }
        1 => {
            let bi = rng.below(blocks.len());
            let paras: Vec<&str> = blocks[bi].text.split('\n').collect();
            let para = rng.below(paras.len());
            let len = paras[para].chars().count();
            if len < 2 {
                return random_edit(rng, blocks);
            }
            let a = rng.below(len - 1);
            let b = a + 1 + rng.below(len - a - 1);
            EditCommand::DeleteRange {
                start: Pos { bi, para, ch: a },
                end: Pos { bi, para, ch: b },
            }
        }
        2 => {
            let bi = rng.below(blocks.len());
            EditCommand::SetText {
                bi,
                text: paragraphs(rng, 30),
            }
        }
        3 => {
            let at = random_pos(rng, blocks);
            EditCommand::SplitBlock {
                at,
                new_type: if rng.below(2) == 0 {
                    ElementType::Dialogue
                } else {
                    ElementType::Action
                },
                fuse: rng.below(2) == 0,
            }
        }
        4 => {
            if blocks.len() < 2 {
                return random_edit(rng, blocks);
            }
            EditCommand::MergeBlocks {
                bi: 1 + rng.below(blocks.len() - 1),
                fuse: rng.below(2) == 0,
            }
        }
        5 => {
            let at = rng.below(blocks.len() + 1);
            let block = if rng.below(2) == 0 {
                ScriptBlock::new(ElementType::Action, paragraphs(rng, 6))
            } else {
                ScriptBlock::new(ElementType::Dialogue, paragraphs(rng, 30))
            };
            EditCommand::InsertBlock { at, block }
        }
        6 => {
            if blocks.len() < 2 {
                return random_edit(rng, blocks);
            }
            EditCommand::DeleteBlock {
                bi: rng.below(blocks.len()),
            }
        }
        _ => {
            let bi = rng.below(blocks.len());
            let t = [
                ElementType::Action,
                ElementType::Dialogue,
                ElementType::Lyrics,
            ][rng.below(3)];
            EditCommand::SetType { bi, t }
        }
    }
}

fn first_diff(a: &DocLayout, b: &DocLayout) -> String {
    for (i, (pa, pb)) in a.pages.iter().zip(b.pages.iter()).enumerate() {
        if pa == pb {
            continue;
        }
        let mut msg = format!(
            "first drifting page: {i} (got {} lines, expected {})\n",
            pa.lines.len(),
            pb.lines.len()
        );
        for (la, lb) in pa.lines.iter().zip(pb.lines.iter()) {
            if la != lb {
                msg.push_str(&format!("  got: {la:?}\n  exp: {lb:?}\n"));
                break;
            }
        }
        return msg;
    }
    format!(
        "page count: got {}, expected {}",
        a.pages.len(),
        b.pages.len()
    )
}

fn assert_case(blocks: &mut Vec<ScriptBlock>, cmd: EditCommand, what: &str) {
    let old = full(blocks);
    let (first, changed_upto) = hint_for(&cmd);
    cmd.apply(blocks).expect("command must apply");
    let expect = full(blocks);
    let got = relayout_from(
        blocks,
        &RUSSIAN,
        &RUSSIAN_GEOMETRY,
        &scene_labels(blocks),
        &old,
        first,
        changed_upto,
    )
    .unwrap_or_else(|| panic!("{what}: relayout must resume, not bail"));
    assert_eq!(
        got,
        expect,
        "{what}: incremental layout drifted\n{}",
        first_diff(&got, &expect)
    );
}

#[test]
fn relayout_matches_full_layout_on_synthetic_corpus() {
    for case in 0..240u64 {
        let mut rng = Rng(0x5EED_2026 ^ (case.wrapping_mul(0x9E37_79B9_7F4A_7C15)));
        let mut blocks = document(&mut rng);
        let cmd = random_edit(&mut rng, &blocks);
        assert_case(&mut blocks, cmd.clone(), &format!("case {case}: {cmd:?}"));
    }
}

#[test]
fn relayout_matches_full_layout_on_a95() {
    let p = kitsp::load_file(FIXTURE).expect("fixture must load");
    let base = p.blocks;

    // Pure text edit inside a long dialogue (mid-paragraph, mid-document).
    let mut blocks = base.clone();
    let bi = blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.element_type == ElementType::Dialogue)
        .max_by_key(|(_, b)| b.text.split('\n').count())
        .map(|(i, _)| i)
        .expect("A95 has dialogue");
    let last_para = blocks[bi].text.split('\n').count().saturating_sub(1);
    let ch = blocks[bi]
        .text
        .split('\n')
        .nth(last_para)
        .map_or(0, |p| 4.min(p.chars().count()));
    assert_case(
        &mut blocks,
        EditCommand::InsertText {
            at: Pos {
                bi,
                para: last_para,
                ch,
            },
            text: " ново".into(),
        },
        "A95 insert in the longest dialogue",
    );

    // Text edit right at the start of that dialogue's first paragraph.
    let mut blocks = base.clone();
    assert_case(
        &mut blocks,
        EditCommand::DeleteRange {
            start: Pos { bi, para: 0, ch: 0 },
            end: Pos { bi, para: 0, ch: 3 },
        },
        "A95 delete at dialogue head",
    );

    // Whole-block replacement (splittable type).
    let mut blocks = base.clone();
    assert_case(
        &mut blocks,
        EditCommand::SetText {
            bi,
            text: (0..90)
                .map(|i| format!("променен ред {i}"))
                .collect::<Vec<_>>()
                .join("\n"),
        },
        "A95 set long dialogue text",
    );

    // Type change on a heading and mid-document action.
    let heading = base
        .iter()
        .position(|b| b.element_type == ElementType::SceneHeading)
        .unwrap();
    let mut blocks = base.clone();
    assert_case(
        &mut blocks,
        EditCommand::SetType {
            bi: heading,
            t: ElementType::Action,
        },
        "A95 heading retyped",
    );

    // Structural edits.
    let mut blocks = base.clone();
    assert_case(
        &mut blocks,
        EditCommand::InsertBlock {
            at: heading + 1,
            block: ScriptBlock::new(ElementType::Action, "Вмъкнато действие.".into()),
        },
        "A95 insert action after first heading",
    );

    let mut blocks = base.clone();
    assert_case(
        &mut blocks,
        EditCommand::DeleteBlock { bi: heading },
        "A95 delete first heading",
    );

    let mut blocks = base.clone();
    assert_case(
        &mut blocks,
        EditCommand::SplitBlock {
            at: random_pos(&mut Rng(7), &base),
            new_type: ElementType::Action,
            fuse: true,
        },
        "A95 split mid-block",
    );

    let mut blocks = base.clone();
    let second = base
        .iter()
        .enumerate()
        .filter(|(i, b)| *i > heading && b.element_type == ElementType::SceneHeading)
        .map(|(i, _)| i)
        .next()
        .unwrap();
    assert_case(
        &mut blocks,
        EditCommand::MergeBlocks {
            bi: second,
            fuse: false,
        },
        "A95 merge second heading into previous block",
    );
}
