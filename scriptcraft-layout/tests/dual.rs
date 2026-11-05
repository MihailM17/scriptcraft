//! Dual-dialogue placement: two columns share one row grid, split across
//! pages with `(MORE)`/`(CONT'D)`, and the editor and PDF must produce the
//! same lines in the same order.

use scriptcraft_core::editing::RUSSIAN;
use scriptcraft_core::kitsp::Metadata;
use scriptcraft_core::model::{DualMark, DualSide, ElementType, Project, ScriptBlock};
use scriptcraft_layout::dual::{DUAL_COL_W_MM, DUAL_GAP_MM};
use scriptcraft_layout::pdf::{layout_pages, ExportOptions};
use scriptcraft_layout::{layout, paginate, relayout_from, DocLayout, LineRole, RUSSIAN_GEOMETRY};
use uuid::Uuid;

fn block(t: ElementType, text: impl Into<String>) -> ScriptBlock {
    ScriptBlock::new(t, text.into())
}

fn pair(p: &mut [ScriptBlock], left: usize, right: usize) {
    let group = Uuid::new_v4();
    p[left].dual = Some(DualMark {
        group,
        side: DualSide::Left,
    });
    p[right].dual = Some(DualMark {
        group,
        side: DualSide::Right,
    });
}

fn paragraphs(prefix: &str, count: usize) -> String {
    (0..count)
        .map(|i| format!("{prefix} ред {i}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn project(blocks: Vec<ScriptBlock>) -> Project {
    Project {
        title: "Dual".into(),
        blocks,
        ..Default::default()
    }
}

fn editor_lines(doc: &DocLayout) -> Vec<(usize, f32, String, LineRole)> {
    doc.pages
        .iter()
        .enumerate()
        .flat_map(|(pi, page)| {
            page.lines
                .iter()
                .filter(|l| matches!(l.role, LineRole::Body | LineRole::ContdCue | LineRole::More))
                .map(move |l| (pi, l.x_mm, l.text.clone(), l.role))
        })
        .collect()
}

#[test]
fn side_by_side_rows_align_and_pdf_matches_editor() {
    let mut blocks = vec![
        block(ElementType::SceneHeading, "INT. CONTROL ROOM - NIGHT"),
        block(ElementType::Action, "Two operators watch the same screen."),
        block(ElementType::Character, "ADA"),
        block(ElementType::Parenthetical, "(quietly)"),
        block(
            ElementType::Dialogue,
            "It is still moving.\nSecond left line.\nThird left line.",
        ),
        block(ElementType::Character, "GRACE"),
        block(ElementType::Dialogue, "Then we stop it together."),
        block(ElementType::Action, "They reach for the same switch."),
    ];
    pair(&mut blocks, 2, 5);

    let pages = paginate::paginate(&blocks, &RUSSIAN);
    let dual_chunks: Vec<_> = pages
        .iter()
        .flat_map(|p| &p.chunks)
        .filter_map(|c| c.dual.as_ref())
        .collect();
    assert_eq!(dual_chunks.len(), 1, "short pair stays on one page");
    assert_eq!(dual_chunks[0].start_row, 0);
    assert_eq!(dual_chunks[0].end_row, 5);
    assert!(!dual_chunks[0].contd_left && !dual_chunks[0].contd_right);
    assert!(!dual_chunks[0].more_left && !dual_chunks[0].more_right);

    let doc = layout(&blocks, &RUSSIAN, &RUSSIAN_GEOMETRY, &["1.".to_string()]);
    let lines = doc.pages[0].lines.iter().collect::<Vec<_>>();
    let ada = lines.iter().find(|l| l.text == "ADA").expect("left cue");
    let grace = lines.iter().find(|l| l.text == "GRACE").expect("right cue");
    assert_eq!(ada.row, grace.row, "cues share the first dual row");
    assert!(ada.x_mm < DUAL_COL_W_MM);
    assert!(grace.x_mm >= DUAL_COL_W_MM + DUAL_GAP_MM);

    // The left parenthetical and the right dialogue share row 1: both columns
    // align at their top, not by line count.
    let paren = lines
        .iter()
        .find(|l| l.text == "(quietly)")
        .expect("parenthetical");
    let right_line = lines
        .iter()
        .find(|l| l.text.starts_with("Then we stop it"))
        .expect("right dialogue");
    assert_eq!(paren.row, right_line.row);
    assert_eq!(paren.row, ada.row + 1);

    // PDF uses the same columns and the same line order.
    let p = project(blocks);
    let pdf = layout_pages(&p, &Metadata::default(), &ExportOptions::default());
    let pdf_page = &pdf[0];
    let body: Vec<_> = pdf_page.iter().filter(|l| !l.text.is_empty()).collect();
    let pdf_ada = body.iter().find(|l| l.text == "ADA").expect("pdf left cue");
    let pdf_grace = body
        .iter()
        .find(|l| l.text == "GRACE")
        .expect("pdf right cue");
    assert!(pdf_ada.x_mm < pdf_grace.x_mm);
    assert!(
        (pdf_ada.x_mm - pdf_grace.x_mm - (ada.x_mm - grace.x_mm)).abs() < 1e-3,
        "PDF keeps the editor's column geometry"
    );

    let editor = editor_lines(&doc);
    let expected: Vec<_> = editor
        .iter()
        .filter(|(pi, ..)| *pi == 0)
        .map(|(_, x, text, _)| (text.clone(), *x))
        .collect();
    let actual: Vec<_> = body
        .iter()
        .filter(|l| !l.is_page_number && l.x_mm >= 37.5)
        .map(|l| (l.text.clone(), l.x_mm - 37.5))
        .collect();
    assert_eq!(actual.len(), expected.len(), "PDF line count");
    for ((atext, ax), (etext, ex)) in actual.iter().zip(&expected) {
        assert_eq!(atext, etext, "PDF line order");
        assert!((ax - ex).abs() < 1e-3, "PDF x for {atext:?}: {ax} vs {ex}");
    }
}

#[test]
fn long_dual_pair_splits_with_more_and_contd_then_drops_finished_column() {
    let mut blocks = vec![
        block(ElementType::Action, "They speak over each other."),
        block(ElementType::Character, "ADA"),
        block(ElementType::Dialogue, paragraphs("ляво", 300)),
        block(ElementType::Character, "GRACE"),
        block(ElementType::Dialogue, paragraphs("дясно", 55)),
        block(ElementType::Action, "Silence."),
    ];
    pair(&mut blocks, 1, 3);

    let pages = paginate::paginate(&blocks, &RUSSIAN);
    assert!(pages.len() >= 3, "long pair must span pages");
    let doc = layout(&blocks, &RUSSIAN, &RUSSIAN_GEOMETRY, &[]);
    assert_eq!(doc.pages.len(), pages.len());

    let mut next_row = 0usize;
    let mut total_rows = 0usize;
    let mut saw_right_more = false;
    let mut saw_left_more = false;
    for page in &pages {
        for chunk in page.chunks.iter().filter(|c| c.dual.is_some()) {
            let dual = chunk.dual.as_ref().unwrap();
            assert_eq!(dual.start_row, next_row, "dual rows are contiguous");
            assert!(dual.end_row > dual.start_row);
            next_row = dual.end_row;
            total_rows = total_rows.max(dual.end_row);
            saw_left_more |= dual.more_left;
            saw_right_more |= dual.more_right;
        }
    }
    assert_eq!(next_row, total_rows);
    assert!(saw_left_more, "the long left column must split");
    assert!(saw_right_more, "the short right column also splits once");

    let more: Vec<_> = doc
        .pages
        .iter()
        .flat_map(|p| &p.lines)
        .filter(|l| l.role == LineRole::More)
        .collect();
    assert!(!more.is_empty());
    for l in &more {
        assert_eq!(l.text, "(MORE)");
        assert!(l.right_aligned);
        assert!(l.x_mm == DUAL_COL_W_MM - 1.0 || l.x_mm == 2.0 * DUAL_COL_W_MM + DUAL_GAP_MM - 1.0);
    }

    // The right column is 120 rows shorter, so later continuation pages must
    // not print "GRACE (CONT'D)" over an empty column.
    let contd: Vec<_> = doc
        .pages
        .iter()
        .flat_map(|p| &p.lines)
        .filter(|l| l.role == LineRole::ContdCue)
        .collect();
    assert!(contd.iter().any(|l| l.text == "ADA (CONT'D)"));
    let right_contd = contd.iter().filter(|l| l.text == "GRACE (CONT'D)").count();
    assert!(right_contd >= 1, "early splits repeat both cues");
    assert!(
        right_contd < contd.len() / 2,
        "finished right column must stop getting cues"
    );
    let last_page_contd: Vec<_> = doc
        .pages
        .last()
        .unwrap()
        .lines
        .iter()
        .filter(|l| l.role == LineRole::ContdCue)
        .collect();
    assert!(last_page_contd.iter().all(|l| l.text == "ADA (CONT'D)"));

    // PDF and editor agree page by page, including MORE/CONT'D rows.
    let p = project(blocks);
    let pdf = layout_pages(&p, &Metadata::default(), &ExportOptions::default());
    assert_eq!(pdf.len(), doc.pages.len());
    for (pi, page) in doc.pages.iter().enumerate() {
        let expected: Vec<_> = page
            .lines
            .iter()
            .filter(|l| matches!(l.role, LineRole::Body | LineRole::ContdCue | LineRole::More))
            .map(|l| {
                let x = if l.right_aligned {
                    l.x_mm - l.text.chars().count() as f32 * 2.54
                } else {
                    l.x_mm
                };
                (l.text.clone(), x)
            })
            .collect();
        let actual: Vec<_> = pdf[pi]
            .iter()
            .filter(|l| !l.text.is_empty() && !l.is_page_number)
            .map(|l| (l.text.clone(), l.x_mm - 37.5))
            .collect();
        assert_eq!(actual.len(), expected.len(), "page {} line count", pi + 1);
        for ((atext, ax), (etext, ex)) in actual.iter().zip(&expected) {
            assert_eq!(atext, etext, "page {} line order", pi + 1);
            assert!(
                (ax - ex).abs() < 1e-3,
                "page {} x for {atext:?}: {ax} vs {ex}",
                pi + 1
            );
        }
    }
}

#[test]
fn incremental_relayout_of_the_block_after_a_split_pair_keeps_the_pair() {
    let mut blocks = vec![
        block(ElementType::SceneHeading, "INT. HALL - NIGHT"),
        block(ElementType::Action, "They start together."),
        block(ElementType::Character, "ADA"),
        block(ElementType::Dialogue, paragraphs("ляво", 300)),
        block(ElementType::Character, "GRACE"),
        block(ElementType::Dialogue, paragraphs("дясно", 55)),
        block(ElementType::Action, "A beat after the pair."),
        block(ElementType::Action, "Another beat."),
        block(ElementType::Action, "Final action to edit."),
    ];
    pair(&mut blocks, 2, 4);
    let labels = vec!["1.".to_string()];
    let old = layout(&blocks, &RUSSIAN, &RUSSIAN_GEOMETRY, &labels);
    assert!(old.pages.len() >= 3);

    let last = blocks.len() - 1;
    blocks[last].text = "Final action, now edited.".to_string();
    let incremental = relayout_from(
        &blocks,
        &RUSSIAN,
        &RUSSIAN_GEOMETRY,
        &labels,
        &old,
        last,
        Some(last),
    )
    .expect("relayout after the pair must resume");
    let full = layout(&blocks, &RUSSIAN, &RUSSIAN_GEOMETRY, &labels);
    assert_eq!(incremental, full);
    assert!(
        incremental
            .pages
            .iter()
            .flat_map(|p| &p.lines)
            .any(|l| l.text == "GRACE" && l.role == LineRole::Body),
        "the rebuilt tail must still render both columns"
    );
}

#[test]
fn incremental_relayout_matches_a_full_layout_of_a_dual_edit() {
    let mut blocks = vec![block(ElementType::SceneHeading, "INT. WRITER'S ROOM - DAY")];
    for i in 0..20 {
        blocks.push(block(
            ElementType::Action,
            format!("Filler action {i} keeps the page moving."),
        ));
    }
    blocks.push(block(ElementType::Character, "ADA"));
    blocks.push(block(ElementType::Dialogue, paragraphs("ляво", 70)));
    blocks.push(block(ElementType::Character, "GRACE"));
    blocks.push(block(ElementType::Dialogue, paragraphs("дясно", 35)));
    blocks.push(block(ElementType::Action, "End."));
    let left = blocks.len() - 5;
    let right_line = left + 3;
    pair(&mut blocks, left, left + 2);

    let labels = vec!["1.".to_string()];
    let old = layout(&blocks, &RUSSIAN, &RUSSIAN_GEOMETRY, &labels);
    assert!(old.pages.len() >= 3, "pair must reach a page boundary");

    blocks[right_line].text = format!("{}\nAn inserted right line.", paragraphs("дясно", 35));
    let incremental = relayout_from(
        &blocks,
        &RUSSIAN,
        &RUSSIAN_GEOMETRY,
        &labels,
        &old,
        right_line,
        Some(right_line),
    )
    .expect("dual edit can be relaid out incrementally");
    let full = layout(&blocks, &RUSSIAN, &RUSSIAN_GEOMETRY, &labels);
    assert_eq!(incremental, full);
}
