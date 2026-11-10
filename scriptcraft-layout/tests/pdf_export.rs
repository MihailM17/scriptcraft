//! PDF export golden checks on the A95 fixture (moved with the exporter from
//! `scriptcraft-core`, now that the exporter shares the layout engine).

use scriptcraft_core::kitsp;
use scriptcraft_core::revision::RevisionHistory;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/A95.kitsp");

#[test]
fn a95_exports_valid_pdf() {
    use scriptcraft_layout::pdf;
    let p = kitsp::load_file(FIXTURE).expect("fixture must load");
    let read = |name: &str| {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../scriptcraft-app/assets/fonts")
                .join(name),
        )
        .expect("bundled font")
    };
    let fonts = pdf::ExportFonts {
        regular: &read("CourierPrime-Regular.ttf"),
        bold: &read("CourierPrime-Bold.ttf"),
        fallback_regular: &read("IBMPlexMono-Regular.ttf"),
        fallback_bold: &read("IBMPlexMono-Bold.ttf"),
    };
    let meta = kitsp::load_metadata(FIXTURE).expect("load metadata");
    let bytes =
        pdf::to_pdf_bytes(&p, &meta, &pdf::ExportOptions::default(), &fonts).expect("pdf renders");
    assert!(bytes.starts_with(b"%PDF-"), "not a PDF");
    assert!(
        bytes.len() > 20_000,
        "suspiciously small PDF: {}",
        bytes.len()
    );

    let doc = lopdf::Document::load_mem(&bytes).expect("generated PDF must parse");
    let expected = pdf::layout_pages(&p, &meta, &pdf::ExportOptions::default()).len() + 1;
    assert_eq!(doc.get_pages().len(), expected, "PDF page count mismatch");

    // Extraction goes through the embedded fonts' ToUnicode data, so the
    // Cyrillic match also proves the fallback face was embedded with a
    // working cmap (Courier Prime itself has no Cyrillic).
    let mut all = String::new();
    for page_num in doc.get_pages().keys() {
        all.push_str(&doc.extract_text(&[*page_num]).unwrap_or_default());
        all.push('\n');
    }
    assert!(all.contains("A95"), "title text missing from PDF");
    assert!(
        all.contains("ПАРКИНГ"),
        "Cyrillic heading text missing from PDF (font fallback broken)"
    );

    let mut revised = p.clone();
    let mut history = RevisionHistory::default();
    history.start(&revised.blocks, "Blue".into(), "today".into(), 0);
    revised.blocks.last_mut().unwrap().text.push_str(" REVISED");
    let marks = history.latest_marks(&revised.blocks).colors;
    let full_script_pages = expected - 1;
    let marked_opts = pdf::ExportOptions {
        revision_marks: true,
        ..pdf::ExportOptions::default()
    };
    let bytes = pdf::to_pdf_bytes_with_marks(&revised, &meta, &marked_opts, &marks, &fonts)
        .expect("marked full script renders");
    let marked_doc = lopdf::Document::load_mem(&bytes).expect("marked PDF must parse");
    assert_eq!(marked_doc.get_pages().len(), expected);

    let opts = pdf::ExportOptions {
        revision_pages_only: true,
        ..pdf::ExportOptions::default()
    };
    let bytes = pdf::to_pdf_bytes_with_marks(&revised, &meta, &opts, &marks, &fonts)
        .expect("revision pages render");
    let doc = lopdf::Document::load_mem(&bytes).expect("revision PDF must parse");
    let marked_pages = pdf::layout_pages_with_marks(&revised, &meta, &opts, &marks)
        .iter()
        .filter(|page| page.iter().any(|line| line.revision.is_some()))
        .count();
    assert!(marked_pages > 0 && marked_pages < full_script_pages);
    assert_eq!(doc.get_pages().len(), marked_pages);
    assert!(!doc.extract_text(&[1]).unwrap_or_default().contains("A95"));
}

/// The editor and the PDF share one engine: for the A95 fixture, every page's
/// rows, texts, weights and horizontal anchors must be identical.
#[test]
fn a95_pdf_matches_editor_layout() {
    use scriptcraft_core::editing::RUSSIAN;
    use scriptcraft_layout::pdf;
    use scriptcraft_layout::place::{layout, RUSSIAN_GEOMETRY, TEXT_W_MM};

    const ML: f32 = 37.5;
    const CELL: f32 = 2.54;

    let p = kitsp::load_file(FIXTURE).expect("fixture must load");
    let meta = kitsp::load_metadata(FIXTURE).expect("load metadata");
    let labels: Vec<String> = (0..p.scenes().len())
        .map(|i| format!("{}.", i + 1))
        .collect();
    let editor = layout(&p.blocks, &RUSSIAN, &RUSSIAN_GEOMETRY, &labels);
    let pages = pdf::layout_pages(&p, &meta, &pdf::ExportOptions::default());

    assert_eq!(editor.pages.len(), pages.len(), "page count");
    for (pi, (epage, plines)) in editor.pages.iter().zip(&pages).enumerate() {
        let mut want: Vec<(usize, String, f32, f32, bool)> = epage
            .lines
            .iter()
            .map(|l| {
                let w = l.text.chars().count() as f32 * CELL;
                let left = if l.right_aligned {
                    ML + l.x_mm - w
                } else {
                    ML + l.x_mm
                };
                (l.row, l.text.clone(), left, w, l.bold)
            })
            .collect();
        let mut got: Vec<(usize, String, f32, f32, bool)> = Vec::new();
        let mut row = 0usize;
        for line in plines {
            if line.share_row {
                row = row.saturating_sub(1);
            }
            if !line.is_page_number {
                if !line.text.is_empty() {
                    let w = line.text.chars().count() as f32 * CELL;
                    let left = if line.right_aligned {
                        ML + TEXT_W_MM - w
                    } else {
                        line.x_mm
                    };
                    got.push((row, line.text.clone(), left, w, line.bold));
                }
                row += 1;
            }
        }
        want.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        got.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        assert_eq!(want, got, "page {} differs from the editor", pi + 1);
    }
}

/// The title-page preview is the PDF exporter's own line list: every visible
/// line (Cyrillic, wrapped title, split contacts, Revised block) must appear
/// on exported page 1 in the same order.
#[test]
fn title_page_preview_matches_exported_first_page() {
    use scriptcraft_core::revision::RevisionHistory;
    use scriptcraft_layout::pdf;

    let p = kitsp::load_file(FIXTURE).expect("fixture must load");
    let mut meta = kitsp::load_metadata(FIXTURE).expect("load metadata");
    meta.title = "ЗАГЛАВИЕ НА ЕДИН МНОГО ДЪЛЪГ ФИЛМ, КОЙТО ПРОДЪЛЖАВА И ПРОДЪЛЖАВА".into();
    meta.contacts = "ул. Примерна 1\nСофия 1000".into();
    meta.year = "09.11.2026".into();

    let read = |name: &str| {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../scriptcraft-app/assets/fonts")
                .join(name),
        )
        .expect("bundled font")
    };
    let fonts = pdf::ExportFonts {
        regular: &read("CourierPrime-Regular.ttf"),
        bold: &read("CourierPrime-Bold.ttf"),
        fallback_regular: &read("IBMPlexMono-Regular.ttf"),
        fallback_bold: &read("IBMPlexMono-Bold.ttf"),
    };

    let mut history = RevisionHistory::default();
    history.start(&p.blocks, "Blue".into(), "09.11.2026".into(), 0);
    let latest = history.latest();
    let preview = pdf::layout_title_page_with_latest(&meta, latest);
    let underlined = preview.iter().filter(|l| l.underline).count();
    assert_eq!(underlined, 2, "the long title must wrap");

    let bytes = pdf::to_pdf_bytes_with_marks_and_latest(
        &p,
        &meta,
        &pdf::ExportOptions::default(),
        &[],
        latest,
        &fonts,
    )
    .expect("pdf renders");
    let doc = lopdf::Document::load_mem(&bytes).expect("PDF must parse");
    let first = *doc.get_pages().keys().next().expect("title page");
    let text = doc.extract_text(&[first]).unwrap_or_default();

    // printpdf splits a line into one text op per font run, and the
    // fallback run boundaries show up as whitespace in extraction. Compare
    // the characters in order, ignoring whitespace, so Cyrillic fallback
    // text is checked just like the Latin lines.
    let squeeze = |s: &str| -> String { s.chars().filter(|c| !c.is_whitespace()).collect() };
    let squash = squeeze(&text);
    let mut cursor = 0;
    for line in preview.iter().filter(|l| !l.text.is_empty()) {
        let want = squeeze(&line.text);
        if want.is_empty() {
            continue;
        }
        let found = squash[cursor..].find(&want).unwrap_or_else(|| {
            panic!(
                "page 1 is missing {:?} after offset {cursor}; page text: {squash:?}",
                line.text
            )
        });
        cursor += found + want.len();
    }
    assert!(squash.contains("REVISED"), "revision block missing");
}

/// The printable revision summary is a real PDF: revision metadata, the
/// changed scene (Cyrillic heading), counts and the totals row all extract.
#[test]
fn summary_pdf_is_printable_and_cyrillic_safe() {
    use scriptcraft_core::model::ElementType;
    use scriptcraft_core::revision::{DraftDiff, RevisionHistory};
    use scriptcraft_core::summary::RevisionSummary;
    use scriptcraft_layout::pdf;

    let mut p = kitsp::load_file(FIXTURE).expect("fixture must load");
    let mut history = RevisionHistory::default();
    history.start(&p.blocks, "Синя".into(), "09.11.2026 10:00".into(), 342);
    let set = history.latest().unwrap().clone();
    let bi = p
        .blocks
        .iter()
        .position(|b| b.element_type == ElementType::Action)
        .expect("fixture has action blocks");
    p.blocks[bi].text.push_str(" — промяна");
    let diff = DraftDiff::against_current(&set, &p.blocks);
    let summary = RevisionSummary::from_diff(&set, &diff, 343, p.scenes().len());
    assert_eq!(summary.changed, 1);

    let read = |name: &str| {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../scriptcraft-app/assets/fonts")
                .join(name),
        )
        .expect("bundled font")
    };
    let fonts = pdf::ExportFonts {
        regular: &read("CourierPrime-Regular.ttf"),
        bold: &read("CourierPrime-Bold.ttf"),
        fallback_regular: &read("IBMPlexMono-Regular.ttf"),
        fallback_bold: &read("IBMPlexMono-Bold.ttf"),
    };
    let bytes = pdf::to_summary_pdf_bytes(&summary, "A95", &fonts).expect("summary renders");
    assert!(bytes.starts_with(b"%PDF-"));
    let doc = lopdf::Document::load_mem(&bytes).expect("summary PDF must parse");
    assert_eq!(doc.get_pages().len(), 1);
    let text = doc.extract_text(&[1]).unwrap_or_default();
    let squash: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    for want in [
        "REVISIONSUMMARY",
        "BlueRevision-Синя",
        "ПАРКИНГ",
        "TOTAL",
        "Page1of1",
    ] {
        let want: String = want.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(squash.contains(&want), "missing {want:?}: {squash:?}");
    }
    assert!(
        squash.contains("342pagesto343pages(+1)"),
        "page baseline is missing: {squash:?}"
    );
}

/// Long summaries paginate instead of running past A4, and the totals row
/// stays on the last page.
#[test]
fn summary_pdf_paginates() {
    use scriptcraft_core::revision::RevisionColor;
    use scriptcraft_core::summary::{RevisionSummary, SceneRevisionSummary};
    use scriptcraft_layout::pdf;

    let scenes: Vec<SceneRevisionSummary> = (0..80)
        .map(|i| SceneRevisionSummary {
            scene_index: i,
            heading: format!("INT. СЦЕНА {i} - ДЕН"),
            added: 1,
            removed: 0,
            changed: 2,
        })
        .collect();
    let summary = RevisionSummary {
        color: RevisionColor::Blue,
        name: "Blue".into(),
        date: "09.11.2026".into(),
        base_pages: 120,
        current_pages: 121,
        added: 80,
        removed: 0,
        changed: 160,
        target_scenes: 80,
        scenes,
    };

    let read = |name: &str| {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../scriptcraft-app/assets/fonts")
                .join(name),
        )
        .expect("bundled font")
    };
    let fonts = pdf::ExportFonts {
        regular: &read("CourierPrime-Regular.ttf"),
        bold: &read("CourierPrime-Bold.ttf"),
        fallback_regular: &read("IBMPlexMono-Regular.ttf"),
        fallback_bold: &read("IBMPlexMono-Bold.ttf"),
    };
    let bytes = pdf::to_summary_pdf_bytes(&summary, "A95", &fonts).expect("summary renders");
    let doc = lopdf::Document::load_mem(&bytes).expect("summary PDF must parse");
    let count = doc.get_pages().len();
    assert!(count > 1, "80 scenes must paginate, got {count} page(s)");
    let last = *doc.get_pages().keys().last().unwrap();
    let text = doc.extract_text(&[last]).unwrap_or_default();
    let squash: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(squash.contains("TOTAL"), "totals row missing: {squash:?}");
    assert!(squash.contains("СЦЕНА79"), "last scene missing: {squash:?}");
}
