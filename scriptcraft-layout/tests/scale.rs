//! Scale smoke check: pagination + placement of a ~500-page script.
//!
//! Prints engine timings (run with `--release -- --nocapture` for
//! representative numbers) and asserts the structure stays coherent: the
//! editor re-runs this layout on every keystroke, so it must stay in the
//! low-millisecond range for feature-length scripts.

use scriptcraft_core::edit::{EditCommand, Pos};
use scriptcraft_core::editing::RUSSIAN;
use scriptcraft_core::model::{ElementType, Project, ScriptBlock};
use scriptcraft_layout::{layout, paginate, relayout_from, LineRole, RUSSIAN_GEOMETRY};
use std::time::Instant;

/// Deterministic ~500-page script: uniform scenes of one heading plus a
/// 12-paragraph action block (≈14 grid rows per scene at 54 rows/page).
fn big_script(scenes: usize) -> Vec<ScriptBlock> {
    let mut blocks = Vec::with_capacity(scenes * 2);
    for i in 0..scenes {
        blocks.push(ScriptBlock::new(
            ElementType::SceneHeading,
            format!("INT. СТАЯ НОМЕР {i} - ДЕН"),
        ));
        let action = (0..12)
            .map(|p| format!("Ред {p} в сцена {i} от дългия тестов сценарий."))
            .collect::<Vec<_>>()
            .join("\n");
        blocks.push(ScriptBlock::new(ElementType::Action, action));
    }
    blocks
}

fn labels(blocks: &[ScriptBlock]) -> Vec<String> {
    let n = blocks
        .iter()
        .filter(|b| b.element_type == ElementType::SceneHeading)
        .count();
    (1..=n).map(|i| format!("{i}.")).collect()
}

#[test]
fn five_hundred_pages_paginate_and_place() {
    let blocks = big_script(1750);
    let labels = labels(&blocks);

    let t0 = Instant::now();
    let pages = paginate(&blocks, &RUSSIAN);
    let t_paginate = t0.elapsed();

    let t1 = Instant::now();
    let doc = layout(&blocks, &RUSSIAN, &RUSSIAN_GEOMETRY, &labels);
    let t_layout = t1.elapsed();

    let lines: usize = doc.pages.iter().map(|p| p.lines.len()).sum();
    eprintln!(
        "scale: {} blocks → {} pages, {lines} placed lines; \
         paginate {t_paginate:?}, place {t_layout:?} ({} profile)",
        blocks.len(),
        pages.len(),
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );

    assert!(
        (470..=530).contains(&pages.len()),
        "expected ~500 pages, got {}",
        pages.len()
    );
    assert_eq!(doc.pages.len(), pages.len(), "place must keep the pages");
    // Every placed line must sit inside its page's grid.
    for (pi, page) in doc.pages.iter().enumerate() {
        for line in &page.lines {
            assert!(
                line.row < RUSSIAN.lines_per_page,
                "page {pi}: row {} overflows the grid",
                line.row
            );
        }
        // Scene numbers only exist where a heading is placed.
        if page.lines.iter().any(|l| l.role == LineRole::SceneNumber) {
            assert!(
                page.lines.iter().any(|l| l.role == LineRole::Body),
                "page {pi}: scene number with no body lines"
            );
        }
    }
}

#[test]
fn typing_in_a_big_script_relayouts_incrementally() {
    // The keystroke path: 40 single-char inserts into one mid-document
    // action block, relayouted through `relayout_from` (in-place text hint).
    let mut blocks = big_script(1750);
    let labels = labels(&blocks);
    let t_full = Instant::now();
    let mut doc = layout(&blocks, &RUSSIAN, &RUSSIAN_GEOMETRY, &labels);
    let full = t_full.elapsed();

    // Pairs are (heading, action), so odd indices are the action blocks.
    let bi = (blocks.len() / 2) | 1;
    let mut total = std::time::Duration::ZERO;
    for k in 0..40usize {
        let at = Pos {
            bi,
            para: 3,
            ch: 10 + k % 20,
        };
        EditCommand::InsertText {
            at,
            text: "x".into(),
        }
        .apply(&mut blocks)
        .expect("insert must apply");
        let t = Instant::now();
        let new = relayout_from(
            &blocks,
            &RUSSIAN,
            &RUSSIAN_GEOMETRY,
            &labels,
            &doc,
            bi,
            Some(bi),
        )
        .expect("incremental relayout must resume");
        total += t.elapsed();
        doc = new;
    }
    let per = total / 40;
    eprintln!(
        "incremental typing: 40 keystrokes in {total:?} ({per:?} per keystroke); \
         full relayout {full:?} ({:.1}x faster) ({} profile)",
        full.as_secs_f64() / per.as_secs_f64().max(f64::EPSILON),
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );

    // The widget builds a per-input-frame working copy (`Editor::new` clones
    // the block list); time that too so the keystroke budget is complete.
    let t_clone = Instant::now();
    for _ in 0..40 {
        let _ = blocks.clone();
    }
    let clone_per = t_clone.elapsed() / 40;
    eprintln!("working-copy clone: {clone_per:?} per input frame");

    // Correctness at scale: the incremental result must equal a full relayout.
    let expect = layout(&blocks, &RUSSIAN, &RUSSIAN_GEOMETRY, &labels);
    assert_eq!(doc, expect, "incremental layout drifted at scale");
    // And it must actually be incremental (a page or two, not the whole doc).
    assert!(
        per.as_secs_f64() * 3.0 < full.as_secs_f64(),
        "incremental relayout is not meaningfully faster than a full one"
    );
}

#[test]
fn project_scene_index_round_trips_at_scale() {
    // The scale script must also survive the model's scene indexing (the app
    // runs it on every navigator refresh; guards against accidental O(n²)).
    let blocks = big_script(400);
    let p = Project {
        title: "Perf".to_string(),
        blocks,
    };
    let t = Instant::now();
    let scenes = p.scenes();
    assert_eq!(scenes.len(), 400, "one scene per heading");
    let total: usize = scenes.iter().map(|s| s.blocks.len()).sum();
    assert_eq!(total, p.blocks.len(), "every block indexed exactly once");
    eprintln!("scene index: {} scenes in {:?}", scenes.len(), t.elapsed());
}
