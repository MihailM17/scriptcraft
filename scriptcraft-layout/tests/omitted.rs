//! Omitted-scene layout: retain heading + number, print one OMITTED row, drop
//! the body and keep production page labels stable around the omission.

use scriptcraft_core::editing::{TemplateMetrics, RUSSIAN};
use scriptcraft_core::kitsp::Metadata;
use scriptcraft_core::model::{ElementType, Project, ScriptBlock};
use scriptcraft_core::page_lock::PageLock;
use scriptcraft_layout::paginate::page_starts;
use scriptcraft_layout::pdf::{layout_pages, ExportOptions};
use scriptcraft_layout::place::{layout_document_view, layout_with_page_lock, RUSSIAN_GEOMETRY};
use scriptcraft_layout::LineRole;

fn metrics() -> TemplateMetrics {
    TemplateMetrics {
        lines_per_page: 10,
        ..RUSSIAN
    }
}

fn script() -> Project {
    let mut blocks = Vec::new();
    for scene in 1..=4 {
        blocks.push(ScriptBlock::new(
            ElementType::SceneHeading,
            format!("INT. ROOM {scene} - DAY"),
        ));
        for line in 0..20 {
            blocks.push(ScriptBlock::new(
                ElementType::Action,
                format!("Scene {scene} body line {line}."),
            ));
        }
    }
    Project {
        title: "Omitted layout".into(),
        blocks,
        scene_meta: Vec::new(),
    }
}

fn labels(p: &Project) -> Vec<String> {
    (0..p.scenes().len())
        .map(|i| format!("{}.", i + 1))
        .collect()
}

fn heading_pages(doc: &scriptcraft_layout::place::DocLayout) -> Vec<(String, Option<String>)> {
    doc.pages
        .iter()
        .flat_map(|page| {
            page.lines
                .iter()
                .filter(|line| line.role == LineRole::Body && line.text.starts_with("INT. ROOM"))
                .map(move |line| (line.text.clone(), page.number.clone()))
        })
        .collect()
}

#[test]
fn omitted_body_disappears_and_prints_one_marker() {
    let mut p = script();
    let full_labels = labels(&p);
    let full = layout_with_page_lock(&p.blocks, &metrics(), &RUSSIAN_GEOMETRY, &full_labels, None);
    let id = p.scenes()[1].id;
    p.set_scene_omitted(id, true);
    let view = p.document_view();
    let omitted = layout_document_view(
        &view,
        &p.blocks,
        &metrics(),
        &RUSSIAN_GEOMETRY,
        &full_labels,
        None,
    );

    assert!(
        omitted.pages.len() < full.pages.len(),
        "omitted body must not consume pages: {} vs {}",
        omitted.pages.len(),
        full.pages.len()
    );
    let lines: Vec<_> = omitted.pages.iter().flat_map(|p| &p.lines).collect();
    let marker = lines
        .iter()
        .find(|line| line.role == LineRole::Omitted)
        .expect("OMITTED row");
    assert_eq!(marker.text, "OMITTED");
    assert!(marker.span.is_none(), "the marker is not editable text");
    assert_eq!(
        marker.source_bi,
        Some(p.scenes()[1].blocks[0]),
        "marker carries the heading's identity for revision colour"
    );
    assert!(
        !lines
            .iter()
            .any(|line| line.text.starts_with("Scene 2 body")),
        "hidden body must not be placed"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.text.starts_with("Scene 3 body")),
        "later scenes must still print"
    );

    // Scene numbers and identities survive the omission.
    let numbers: Vec<_> = lines
        .iter()
        .filter(|line| line.role == LineRole::SceneNumber)
        .map(|line| line.text.as_str())
        .collect();
    assert_eq!(numbers, vec!["1.", "2.", "3.", "4."]);
    let headings: Vec<_> = lines
        .iter()
        .filter(|line| line.role == LineRole::Body && line.text.starts_with("INT. ROOM"))
        .map(|line| line.text.as_str())
        .collect();
    assert_eq!(
        headings,
        vec![
            "INT. ROOM 1 - DAY",
            "INT. ROOM 2 - DAY",
            "INT. ROOM 3 - DAY",
            "INT. ROOM 4 - DAY"
        ]
    );
}

#[test]
fn page_locks_survive_omission_and_keep_heading_numbers() {
    let mut p = script();
    let m = metrics();
    let full_labels = labels(&p);
    let starts = page_starts(&p.blocks, &m);
    let lock = PageLock::capture(&p.blocks, &starts);
    let full = layout_with_page_lock(&p.blocks, &m, &RUSSIAN_GEOMETRY, &full_labels, Some(&lock));
    let before = heading_pages(&full);

    let id = p.scenes()[1].id;
    p.set_scene_omitted(id, true);
    let view = p.document_view();
    let omitted = layout_document_view(
        &view,
        &p.blocks,
        &m,
        &RUSSIAN_GEOMETRY,
        &full_labels,
        Some(&lock),
    );
    let after = heading_pages(&omitted);
    assert_eq!(
        after, before,
        "every scene heading keeps its locked page number across omission"
    );
    assert!(
        omitted.pages.len() < full.pages.len(),
        "the omitted body's pages are gone"
    );
}

fn pdf_heading_pages(p: &Project, lock: &PageLock) -> (Vec<(String, Option<String>)>, usize) {
    let meta = Metadata {
        title: p.title.clone(),
        locked_pages: lock.to_json().expect("lock json"),
        ..Metadata::default()
    };
    let opts = ExportOptions {
        title_page: false,
        page_numbers: true,
        scene_numbers: true,
        ..ExportOptions::default()
    };
    let pages = layout_pages(p, &meta, &opts);
    let headings = (1..=4)
        .map(|scene| {
            let heading = format!("INT. ROOM {scene} - DAY");
            let lines = pages
                .iter()
                .find(|lines| {
                    lines
                        .iter()
                        .any(|line| !line.is_page_number && line.text == heading)
                })
                .unwrap_or_else(|| panic!("PDF must print heading {scene}"));
            let label = lines
                .iter()
                .find(|line| line.is_page_number)
                .map(|line| line.text.trim_end_matches('.').to_string());
            (heading, label)
        })
        .collect();
    (headings, pages.len())
}

#[test]
fn pdf_keeps_locked_heading_pages_across_omission() {
    // Enough body per scene to span several Russian-template pages.
    let mut p = Project {
        title: "PDF omission".into(),
        blocks: Vec::new(),
        scene_meta: Vec::new(),
    };
    for scene in 1..=4 {
        p.blocks.push(ScriptBlock::new(
            ElementType::SceneHeading,
            format!("INT. ROOM {scene} - DAY"),
        ));
        for line in 0..120 {
            p.blocks.push(ScriptBlock::new(
                ElementType::Action,
                format!("Scene {scene} body line {line}."),
            ));
        }
    }
    let starts = page_starts(&p.blocks, &RUSSIAN);
    let lock = PageLock::capture(&p.blocks, &starts);
    let (before, pages_before) = pdf_heading_pages(&p, &lock);
    assert!(pages_before >= 8, "the PDF must span several pages");

    let id = p.scenes()[1].id;
    p.set_scene_omitted(id, true);
    let (after, pages_after) = pdf_heading_pages(&p, &lock);
    assert!(
        pages_after < pages_before,
        "the omitted body's PDF pages are gone"
    );
    assert_eq!(
        after, before,
        "the PDF keeps every locked heading page across omission"
    );
}
