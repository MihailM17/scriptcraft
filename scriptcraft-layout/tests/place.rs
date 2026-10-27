//! Placed-line invariants: `place::layout` must resolve the paginator's chunks
//! into grid rows that never overlap or leak off their page, and every body
//! line must carry a source span that tiles its paragraph and reproduces the
//! display text. (The original A/B check against the legacy core paginator
//! was retired together with that engine; `parity.rs` freezes its exact
//! output as golden hashes.)

use scriptcraft_core::editing::RUSSIAN;
use scriptcraft_core::kitsp;
use scriptcraft_core::model::{ElementType, ScriptBlock};
use scriptcraft_core::page_lock::PageLock;
use scriptcraft_layout::{layout, layout_with_page_lock, LineRole, RUSSIAN_GEOMETRY, TEXT_W_MM};
use std::collections::HashMap;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/A95.kitsp");

fn scene_labels(blocks: &[ScriptBlock]) -> Vec<String> {
    let count = blocks
        .iter()
        .filter(|b| b.element_type == ElementType::SceneHeading)
        .count();
    (1..=count).map(|i| format!("{i}.")).collect()
}

#[test]
fn a95_scene_numbers_align_with_headings() {
    let p = kitsp::load_file(FIXTURE).expect("fixture must load");
    let labels = scene_labels(&p.blocks);
    let doc = layout(&p.blocks, &RUSSIAN, &RUSSIAN_GEOMETRY, &labels);
    assert_eq!(doc.pages[0].number, None, "KIT leaves page one unnumbered");
    for (pi, page) in doc.pages.iter().enumerate() {
        assert_eq!(
            page.number,
            if pi > 0 {
                Some((pi + 1).to_string())
            } else {
                None
            }
        );
    }

    let numbers: Vec<_> = doc
        .pages
        .iter()
        .flat_map(|pg| &pg.lines)
        .filter(|l| l.role == LineRole::SceneNumber)
        .collect();
    assert_eq!(numbers.len(), labels.len(), "one number per scene");
    for (i, l) in numbers.iter().enumerate() {
        assert_eq!(l.text, format!("{}.", i + 1));
        assert!(l.right_aligned && l.bold);
        assert_eq!(l.x_mm, -RUSSIAN_GEOMETRY.scene_num_gap_mm);
        assert!(l.span.is_none());
    }
    // Each number sits on the same grid row as its heading's first body line.
    for l in &numbers {
        let heading_on = doc.pages.iter().flat_map(|pg| &pg.lines).any(|b| {
            b.role == LineRole::Body
                && b.row == l.row
                && b.span
                    .as_ref()
                    .is_some_and(|s| p.blocks[s.bi].element_type == ElementType::SceneHeading)
        });
        assert!(heading_on, "number {} floats with no heading", l.text);
    }
}

#[test]
fn body_spans_tile_paragraphs_and_reproduce_text() {
    let p = kitsp::load_file(FIXTURE).expect("fixture must load");
    let labels = scene_labels(&p.blocks);
    let doc = layout(&p.blocks, &RUSSIAN, &RUSSIAN_GEOMETRY, &labels);

    let mut spans: HashMap<(usize, usize), Vec<(usize, usize)>> = HashMap::new();
    for line in doc.pages.iter().flat_map(|pg| &pg.lines) {
        let Some(span) = line.span else { continue };
        assert_eq!(line.role, LineRole::Body);
        let para = p
            .blocks
            .get(span.bi)
            .and_then(|b| b.text.split('\n').nth(span.para))
            .expect("span points at a real paragraph");
        let sub: String = para
            .chars()
            .skip(span.start)
            .take(span.end.saturating_sub(span.start))
            .collect();
        assert_eq!(
            sub.split_whitespace().collect::<Vec<_>>().join(" "),
            line.text,
            "display text ≠ source span in block {} para {}",
            span.bi,
            span.para
        );
        spans
            .entry((span.bi, span.para))
            .or_default()
            .push((span.start, span.end));
    }

    // Every visible paragraph of every chunk must be covered exactly once.
    let total_paras: usize = p.blocks.iter().map(|b| b.text.split('\n').count()).sum();
    assert_eq!(spans.len(), total_paras, "paragraph coverage");
    for ((bi, para), ranges) in spans {
        let mut ranges = ranges;
        ranges.sort_unstable();
        let len = p.blocks[bi]
            .text
            .split('\n')
            .nth(para)
            .unwrap()
            .chars()
            .count();
        assert_eq!(ranges.first().map(|r| r.0), Some(0), "bi {bi} para {para}");
        assert_eq!(ranges.last().map(|r| r.1), Some(len), "bi {bi} para {para}");
        for w in ranges.windows(2) {
            assert_eq!(w[0].1, w[1].0, "gap/overlap bi {bi} para {para}");
        }
    }
}

#[test]
fn split_dialogue_places_cues_and_more_rows() {
    let text = (0..140)
        .map(|i| format!("ред номер {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let blocks = vec![
        ScriptBlock::new(ElementType::Character, "ИВАН".into()),
        ScriptBlock::new(ElementType::Dialogue, text),
    ];
    let doc = layout(&blocks, &RUSSIAN, &RUSSIAN_GEOMETRY, &[]);
    assert!(doc.pages.len() >= 3, "140 rows must span pages");

    let lines: Vec<_> = doc.pages.iter().flat_map(|pg| &pg.lines).collect();
    let more = lines
        .iter()
        .filter(|l| l.role == LineRole::More)
        .collect::<Vec<_>>();
    let contd = lines
        .iter()
        .filter(|l| l.role == LineRole::ContdCue)
        .collect::<Vec<_>>();
    assert!(!more.is_empty(), "long dialogue must split");
    assert_eq!(more.len(), contd.len(), "every split continues with a cue");
    for l in &more {
        assert_eq!(l.text, "(MORE)");
        assert!(l.right_aligned);
        assert_eq!(l.x_mm, TEXT_W_MM);
    }
    for l in &contd {
        assert_eq!(l.text, "ИВАН (CONT'D)");
        assert!(l.bold);
        assert_eq!(l.x_mm, RUSSIAN_GEOMETRY.cue_mm);
    }
    // Uniform one-row paragraphs always fill whole pages, so each continued
    // chunk's (CONT'D) cue opens its page.
    for page in &doc.pages {
        let first = page
            .lines
            .iter()
            .filter(|x| x.role != LineRole::SceneNumber)
            .min_by_key(|x| x.row);
        if let Some(cue) = page.lines.iter().find(|l| l.role == LineRole::ContdCue) {
            assert_eq!(
                first.map(|f| f.row),
                Some(cue.row),
                "CONT'D must open its page"
            );
        }
    }
}

#[test]
fn page_lock_labels_editor_pages_with_a_suffix() {
    let mut p = kitsp::load_file(FIXTURE).expect("fixture must load");
    for i in 0..30 {
        p.blocks.push(ScriptBlock::new(
            ElementType::Action,
            format!("Original action {i}."),
        ));
    }
    let starts = scriptcraft_layout::paginate::page_starts(&p.blocks, &RUSSIAN);
    assert!(starts.len() >= 2, "fixture needs multiple pages");
    let lock = PageLock::capture(&p.blocks, &starts);
    let at = starts[1].0;
    for i in 0..40 {
        p.blocks.insert(
            at,
            ScriptBlock::new(ElementType::Action, format!("Inserted action {i}.")),
        );
    }
    let labels = scene_labels(&p.blocks);
    let doc = layout_with_page_lock(&p.blocks, &RUSSIAN, &RUSSIAN_GEOMETRY, &labels, Some(&lock));
    assert_eq!(doc.pages[0].number, None);
    assert_eq!(doc.pages[1].number.as_deref(), Some("1A"));
    assert!(doc.pages.iter().any(|p| p.number.as_deref() == Some("2")));
}
