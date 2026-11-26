//! PDF export golden checks on the A95 fixture (moved with the exporter from
//! `scriptcraft-core`, now that the exporter shares the layout engine).

use scriptcraft_core::kitsp;
use scriptcraft_core::revision::RevisionHistory;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/A95.kitsp");

#[test]
fn plex_primary_exports_the_same_page_count_and_both_scripts() {
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
    // IBM Plex Mono is the default script face: it carries Latin and Cyrillic
    // in one design, so the Cyrillic no longer comes from a fallback.
    let fonts = pdf::ExportFonts {
        regular: &read("IBMPlexMono-Regular.ttf"),
        bold: &read("IBMPlexMono-Bold.ttf"),
        italic: Some(&read("CourierPrime-Italic.ttf")),
        fallback_regular: &read("CourierPrime-Regular.ttf"),
        fallback_bold: &read("CourierPrime-Bold.ttf"),
        fallback_italic: Some(&read("LiberationMono-Italic.ttf")),
    };
    let meta = kitsp::load_metadata(FIXTURE).expect("load metadata");
    let opts = pdf::ExportOptions::default();
    let bytes = pdf::to_pdf_bytes(&p, &meta, &opts, &fonts).expect("pdf renders");
    let doc = lopdf::Document::load_mem(&bytes).expect("generated PDF must parse");
    assert_eq!(
        doc.get_pages().len(),
        pdf::layout_pages(&p, &meta, &opts).len() + 1,
        "choosing a face must never move a page break"
    );
    let mut all = String::new();
    for page_num in doc.get_pages().keys() {
        all.push_str(&doc.extract_text(&[*page_num]).unwrap_or_default());
        all.push('\n');
    }
    assert!(all.contains("A95"), "title text missing from PDF");
    assert!(all.contains("ПАРКИНГ"), "Cyrillic heading text missing");
}

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
        italic: Some(&read("CourierPrime-Italic.ttf")),
        fallback_regular: &read("IBMPlexMono-Regular.ttf"),
        fallback_bold: &read("IBMPlexMono-Bold.ttf"),
        fallback_italic: Some(&read("LiberationMono-Italic.ttf")),
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
    use scriptcraft_core::editing::FINAL_DRAFT_A4;
    use scriptcraft_layout::pdf;
    use scriptcraft_layout::place::{layout, FINAL_DRAFT_A4_GEOMETRY, FINAL_DRAFT_A4_TEXT_W_MM};

    const ML: f32 = 38.0;
    const CELL: f32 = 2.54;

    let p = kitsp::load_file(FIXTURE).expect("fixture must load");
    let meta = kitsp::load_metadata(FIXTURE).expect("load metadata");
    let labels: Vec<String> = (0..p.scenes().len())
        .map(|i| format!("{}.", i + 1))
        .collect();
    let editor = layout(
        &p.blocks,
        &FINAL_DRAFT_A4,
        &FINAL_DRAFT_A4_GEOMETRY,
        &labels,
    );
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
                        ML + FINAL_DRAFT_A4_TEXT_W_MM - w
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
        italic: Some(&read("CourierPrime-Italic.ttf")),
        fallback_regular: &read("IBMPlexMono-Regular.ttf"),
        fallback_bold: &read("IBMPlexMono-Bold.ttf"),
        fallback_italic: Some(&read("LiberationMono-Italic.ttf")),
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
        italic: Some(&read("CourierPrime-Italic.ttf")),
        fallback_regular: &read("IBMPlexMono-Regular.ttf"),
        fallback_bold: &read("IBMPlexMono-Bold.ttf"),
        fallback_italic: Some(&read("LiberationMono-Italic.ttf")),
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
        italic: Some(&read("CourierPrime-Italic.ttf")),
        fallback_regular: &read("IBMPlexMono-Regular.ttf"),
        fallback_bold: &read("IBMPlexMono-Bold.ttf"),
        fallback_italic: Some(&read("LiberationMono-Italic.ttf")),
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

/// Liberation Mono is bundled with Latin and Cyrillic in one design, so the
/// same A95 fixture must export without falling back for either script and
/// must keep the editor's page count.
#[test]
fn liberation_mono_exports_latin_and_cyrillic() {
    use scriptcraft_layout::pdf;

    let p = kitsp::load_file(FIXTURE).expect("fixture must load");
    let meta = kitsp::load_metadata(FIXTURE).expect("load metadata");
    let read = |name: &str| {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../scriptcraft-app/assets/fonts")
                .join(name),
        )
        .expect("bundled font")
    };
    let fonts = pdf::ExportFonts {
        regular: &read("LiberationMono-Regular.ttf"),
        bold: &read("LiberationMono-Bold.ttf"),
        italic: Some(&read("LiberationMono-Italic.ttf")),
        fallback_regular: &read("IBMPlexMono-Regular.ttf"),
        fallback_bold: &read("IBMPlexMono-Bold.ttf"),
        fallback_italic: Some(&read("LiberationMono-Italic.ttf")),
    };
    let opts = pdf::ExportOptions::default();
    let bytes = pdf::to_pdf_bytes(&p, &meta, &opts, &fonts).expect("pdf renders");
    let doc = lopdf::Document::load_mem(&bytes).expect("generated PDF must parse");
    assert_eq!(
        doc.get_pages().len(),
        pdf::layout_pages(&p, &meta, &opts).len() + 1,
        "Liberation Mono must not move a page break"
    );
    let mut all = String::new();
    for page_num in doc.get_pages().keys() {
        all.push_str(&doc.extract_text(&[*page_num]).unwrap_or_default());
        all.push('\n');
    }
    assert!(all.contains("A95"), "title text missing from PDF");
    assert!(all.contains("ПАРКИНГ"), "Cyrillic heading text missing");
    assert!(
        String::from_utf8_lossy(&bytes).contains("LiberationMono"),
        "the PDF must embed Liberation Mono"
    );
}

#[test]
fn colored_script_and_shot_list_pdfs_render_with_legend() {
    use scriptcraft_core::shots::{ShotMovement, ShotRange, ShotSize};

    let mut p = kitsp::load_file(FIXTURE).expect("load A95");
    let meta = kitsp::load_metadata(FIXTURE).expect("load metadata");
    let scenes = p.scenes();
    let s0 = &scenes[0];
    let start_id = p.blocks[s0.blocks[1]].id;
    let end_id = p.blocks[s0.blocks[3.min(s0.blocks.len() - 1)]].id;
    p.shots.add_shot(
        Some(s0.id),
        "1",
        ShotSize::Ws,
        ShotMovement::Dolly,
        "28mm".into(),
        "Wide establishing master".into(),
        ShotRange::blocks(start_id, end_id),
    );
    p.shots.add_shot(
        Some(s0.id),
        "1",
        ShotSize::Cu,
        ShotMovement::Static,
        "85mm".into(),
        "Close on Ivan".into(),
        ShotRange::blocks(end_id, end_id),
    );

    let read = |name: &str| {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../scriptcraft-app/assets/fonts")
                .join(name),
        )
        .expect("font")
    };
    let fonts = scriptcraft_layout::pdf::ExportFonts {
        regular: &read("CourierPrime-Regular.ttf"),
        bold: &read("CourierPrime-Bold.ttf"),
        italic: Some(&read("CourierPrime-Italic.ttf")),
        fallback_regular: &read("IBMPlexMono-Regular.ttf"),
        fallback_bold: &read("IBMPlexMono-Bold.ttf"),
        fallback_italic: Some(&read("LiberationMono-Italic.ttf")),
    };
    let opts = scriptcraft_layout::pdf::ExportOptions::default();

    let colored = scriptcraft_layout::pdf::to_colored_script_pdf_bytes(&p, &meta, &opts, &fonts)
        .expect("colored script PDF renders");
    let doc = lopdf::Document::load_mem(&colored).expect("colored PDF parses");
    // Title page + 1 Shot Legend page + all script pages
    assert_eq!(
        doc.get_pages().len(),
        scriptcraft_layout::pdf::layout_pages(&p, &meta, &opts).len() + 2
    );
    let page1_text = doc.extract_text(&[1]).unwrap_or_default();
    assert!(
        page1_text.contains("A95"),
        "title page must come first: {page1_text}"
    );
    let page2_text = doc.extract_text(&[2]).unwrap_or_default();
    assert!(
        page2_text.contains("SHOT LEGEND") && page2_text.contains("1A"),
        "legend page must follow title page and list shot 1A: {page2_text}"
    );

    let list_pdf = scriptcraft_layout::pdf::to_shot_list_pdf_bytes(&p, &meta, &opts, &fonts)
        .expect("shot list PDF renders");
    let list_doc = lopdf::Document::load_mem(&list_pdf).expect("shot list PDF parses");
    assert!(!list_doc.get_pages().is_empty());
    let list_text = list_doc.extract_text(&[1]).unwrap_or_default();
    assert!(
        list_text.contains("SHOT LIST") && list_text.contains("28mm"),
        "shot list PDF must contain shots and lens: {list_text}"
    );
}

#[test]
fn normal_export_color_by_shots_and_untruncated_shot_list_pdf() {
    use scriptcraft_core::model::{ElementType, Project, ScriptBlock};
    use scriptcraft_core::shots::{ShotMovement, ShotRange, ShotSize};
    use scriptcraft_layout::pdf;

    let long_desc = "Slow push-in from wide master through the doorway framing Ivan at the workbench with rain streaking down the glass behind him, holding beat until he turns to face Elena.";
    let mut p = Project {
        title: "ExportShots".into(),
        blocks: vec![
            ScriptBlock::new(
                ElementType::SceneHeading,
                "INT. ABANDONED ROOFTOP OBSERVATORY CONTROL ROOM - CONTINUOUS NIGHT".into(),
            ),
            ScriptBlock::new(
                ElementType::Action,
                "Ivan stares at the flickering neon sign while thunder rolls across the wet concrete courtyard outside.".into(),
            ),
        ],
        ..Default::default()
    };
    let b1 = p.blocks[1].id;
    let sid = p.shots.add_shot(
        Some(p.blocks[0].id),
        "1",
        ShotSize::Ws,
        ShotMovement::Dolly,
        "28mm anamorphic".into(),
        long_desc.into(),
        ShotRange::blocks(b1, b1),
    );
    p.shots.get_mut(sid).unwrap().code = "ROOFTOP-MASTER-WIDE-01A".into();

    let read = |name: &str| {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../scriptcraft-app/assets/fonts")
                .join(name),
        )
        .expect("font")
    };
    let fonts = pdf::ExportFonts {
        regular: &read("CourierPrime-Regular.ttf"),
        bold: &read("CourierPrime-Bold.ttf"),
        italic: Some(&read("CourierPrime-Italic.ttf")),
        fallback_regular: &read("IBMPlexMono-Regular.ttf"),
        fallback_bold: &read("IBMPlexMono-Bold.ttf"),
        fallback_italic: Some(&read("LiberationMono-Italic.ttf")),
    };
    let meta = kitsp::Metadata::default();

    // 1. Normal export colored by shots WITHOUT legend page
    let opts_no_legend = pdf::ExportOptions {
        title_page: false,
        color_by_shots: true,
        include_shot_legend: false,
        ..Default::default()
    };
    let pdf_no =
        pdf::to_pdf_bytes(&p, &meta, &opts_no_legend, &fonts).expect("colored without legend");
    let doc_no = lopdf::Document::load_mem(&pdf_no).expect("parse no-legend pdf");

    // 2. Normal export colored by shots WITH legend page
    let opts_with_legend = pdf::ExportOptions {
        include_shot_legend: true,
        ..opts_no_legend.clone()
    };
    let pdf_with =
        pdf::to_pdf_bytes(&p, &meta, &opts_with_legend, &fonts).expect("colored with legend");
    let doc_with = lopdf::Document::load_mem(&pdf_with).expect("parse with-legend pdf");
    assert_eq!(
        doc_with.get_pages().len(),
        doc_no.get_pages().len() + 1,
        "include_shot_legend must add the Shot Legend page"
    );

    // 3. Dedicated Shot List PDF preserves wrapped long description and long custom shot code
    let list_pdf = pdf::to_shot_list_pdf_bytes(&p, &meta, &pdf::ExportOptions::default(), &fonts)
        .expect("shot list pdf");
    let list_doc = lopdf::Document::load_mem(&list_pdf).expect("parse shot list pdf");
    let mut all_text = String::new();
    for pg in list_doc.get_pages().keys() {
        all_text.push_str(&list_doc.extract_text(&[*pg]).unwrap_or_default());
    }
    assert!(
        all_text.contains("Elena") && all_text.contains("ROOFTOP-"),
        "long description end and long code must wrap instead of being cut off: {all_text}"
    );
}

#[test]
fn cyrillic_descriptive_shot_names_render_cleanly_without_collisions() {
    use scriptcraft_core::model::{ElementType, Project, ScriptBlock};
    use scriptcraft_core::shots::{ShotColor, ShotMovement, ShotRange, ShotSize};
    use scriptcraft_layout::pdf;

    let mut p = Project {
        title: "Покер".into(),
        blocks: vec![
            ScriptBlock::new(
                ElementType::SceneHeading,
                "ИНТ. СТАЯТА ЗА ПОЧИВКА. НОЩ".into(),
            ),
            ScriptBlock::new(
                ElementType::Action,
                "Групата изследователи се е наредила около бюрото отрупано с карти, чаши и бутилки с алкохол и чипове за покер. Петимата мъже са се наредили в полукръг.".into(),
            ),
            ScriptBlock::new(ElementType::Character, "КОПИНОВ".into()),
            ScriptBlock::new(
                ElementType::Dialogue,
                "Момчета, време е да се поразтъпчем.".into(),
            ),
            ScriptBlock::new(
                ElementType::Action,
                "КОПИНОВ вдига залогът на масата. Всички възкликват негативно. Плащат и взимат картите пред тях.".into(),
            ),
            ScriptBlock::new(
                ElementType::Action,
                "КОПИНОВ отпива от уискито си. Мъжете се засмиват. Исаков оглежда стаята и после връща погледа си на усмихнатото лице на КОПИНОВ.".into(),
            ),
        ],
        ..Default::default()
    };
    let s0 = p.blocks[0].id;
    let b0 = p.blocks[0].id;
    let b1 = p.blocks[1].id;
    let b2 = p.blocks[2].id;
    let b3 = p.blocks[3].id;
    let b4 = p.blocks[4].id;
    let b5 = p.blocks[5].id;

    let id1 = p.shots.add_shot(
        Some(s0),
        "1",
        ShotSize::Ews,
        ShotMovement::Static,
        "24mm".into(),
        "Общ план на цялата стая и масата за покер".into(),
        ShotRange::blocks(b0, b1),
    );
    p.shots.get_mut(id1).unwrap().code = "Най-общ план".into();
    p.shots.get_mut(id1).unwrap().color = ShotColor::Amber;

    let id2 = p.shots.add_shot(
        Some(s0),
        "1",
        ShotSize::Cu,
        ShotMovement::Static,
        "85mm".into(),
        "Близък план на Копинов".into(),
        ShotRange::blocks(b2, b3),
    );
    p.shots.get_mut(id2).unwrap().code = "Копинов близък".into();
    p.shots.get_mut(id2).unwrap().color = ShotColor::Crimson;
    p.shots
        .assign_range_merged(id2, ShotRange::chars(b5, 0, b5, 49), &p.blocks);

    let id3 = p.shots.add_shot(
        Some(s0),
        "1",
        ShotSize::Ws,
        ShotMovement::Dolly,
        "35mm".into(),
        "Цялата маса при залога".into(),
        ShotRange::blocks(b4, b4),
    );
    p.shots.get_mut(id3).unwrap().code = "Цялата маса".into();
    p.shots.get_mut(id3).unwrap().color = ShotColor::Coral;

    let id4 = p.shots.add_shot(
        Some(s0),
        "1",
        ShotSize::Mcu,
        ShotMovement::Static,
        "50mm".into(),
        "Исаков среден план".into(),
        ShotRange::chars(b5, 50, b5, p.blocks[5].text.chars().count()),
    );
    p.shots.get_mut(id4).unwrap().code = "Исаков среден".into();
    p.shots.get_mut(id4).unwrap().color = ShotColor::Navy;

    let read = |name: &str| {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../scriptcraft-app/assets/fonts")
                .join(name),
        )
        .expect("font")
    };
    let fonts = pdf::ExportFonts {
        regular: &read("IBMPlexMono-Regular.ttf"),
        bold: &read("IBMPlexMono-Bold.ttf"),
        italic: Some(&read("CourierPrime-Italic.ttf")),
        fallback_regular: &read("CourierPrime-Regular.ttf"),
        fallback_bold: &read("CourierPrime-Bold.ttf"),
        fallback_italic: Some(&read("LiberationMono-Italic.ttf")),
    };
    let meta = kitsp::Metadata::default();
    let opts = pdf::ExportOptions {
        title_page: false,
        bold_character_cues: true,
        color_by_shots: true,
        include_shot_legend: true,
        ..Default::default()
    };
    let colored = pdf::to_pdf_bytes(&p, &meta, &opts, &fonts).expect("colored cyrillic pdf");
    let shotlist =
        pdf::to_shot_list_pdf_bytes(&p, &meta, &opts, &fonts).expect("shotlist cyrillic pdf");
    assert!(shotlist.starts_with(b"%PDF-"));
    let doc = lopdf::Document::load_mem(&colored).expect("parse cyrillic colored");
    assert_eq!(doc.get_pages().len(), 2, "legend + 1 script page");
    let script_txt = doc.extract_text(&[2]).unwrap_or_default();
    assert!(
        script_txt.contains("Копинов близък") && script_txt.contains("Цялата маса"),
        "full Cyrillic shot names must appear untruncated in left margin: {script_txt}"
    );
}

#[test]
fn directors_breakdown_pdf_renders_floor_plan_shapes_and_storyboard_images() {
    use scriptcraft_core::shots::{
        PlanActor, PlanCamera, PlanProp, PropKind, ShotColor, ShotMovement, ShotRange, ShotSize,
    };
    use scriptcraft_core::{kitsp, ElementType, Project, ScriptBlock};
    use scriptcraft_layout::pdf;
    use std::collections::HashMap;

    let mut p = Project {
        title: "Midnight Diner".into(),
        blocks: vec![
            ScriptBlock::new(ElementType::SceneHeading, "INT. DINER - NIGHT".into()),
            ScriptBlock::new(
                ElementType::Action,
                "Rain streaks the neon-lit glass. ELENA sits across from MARCO in a red vinyl booth, sliding a battered envelope across the table.".into(),
            ),
            ScriptBlock::new(ElementType::Character, "ELENA".into()),
            ScriptBlock::new(
                ElementType::Dialogue,
                "You said nobody followed you here. Look across the street.".into(),
            ),
            ScriptBlock::new(ElementType::Character, "MARCO".into()),
            ScriptBlock::new(
                ElementType::Dialogue,
                "That's just a cab waiting for a fare.".into(),
            ),
        ],
        ..Default::default()
    };
    let s0 = p.blocks[0].id;
    let b0 = p.blocks[0].id;
    let b1 = p.blocks[1].id;
    let b2 = p.blocks[2].id;
    let b3 = p.blocks[3].id;
    let b4 = p.blocks[4].id;
    let b5 = p.blocks[5].id;

    let id1 = p.shots.add_shot(
        Some(s0),
        "1",
        ShotSize::Ws,
        ShotMovement::Dolly,
        "28mm".into(),
        "Wide establishing master tracking gently along the booth aisle with neon window reflections.".into(),
        ShotRange::blocks(b0, b1),
    );
    p.shots.get_mut(id1).unwrap().color = ShotColor::Cobalt;

    let id2 = p.shots.add_shot(
        Some(s0),
        "1",
        ShotSize::Cu,
        ShotMovement::Static,
        "85mm".into(),
        "Tight single on Elena as she glances out the rain-streaked glass.".into(),
        ShotRange::blocks(b2, b3),
    );
    p.shots.get_mut(id2).unwrap().color = ShotColor::Coral;

    let id3 = p.shots.add_shot(
        Some(s0),
        "1",
        ShotSize::Ots,
        ShotMovement::Static,
        "50mm".into(),
        "Over-the-shoulder on Marco holding steady eye contact.".into(),
        ShotRange::blocks(b4, b5),
    );
    p.shots.get_mut(id3).unwrap().color = ShotColor::Emerald;

    // Populate a rich 2D Floor Plan with basic shapes (Circle, Triangle, Square, Light) + actors + cameras
    p.shots
        .scene_plans
        .push(scriptcraft_core::shots::SceneFloorPlan::empty(s0));
    let plan = p.shots.plan_for_scene_mut(s0).unwrap();
    plan.props.push(PlanProp {
        id: uuid::Uuid::new_v4(),
        kind: PropKind::Wall,
        label: "Front Window".into(),
        x: 0.0,
        y: -2.6,
        w: 6.4,
        h: 0.25,
        angle_deg: 0.0,
    });
    plan.props.push(PlanProp {
        id: uuid::Uuid::new_v4(),
        kind: PropKind::Table,
        label: "Booth Table".into(),
        x: 0.0,
        y: -0.4,
        w: 1.4,
        h: 0.85,
        angle_deg: 0.0,
    });
    plan.props.push(PlanProp {
        id: uuid::Uuid::new_v4(),
        kind: PropKind::Circle,
        label: "Pillar".into(),
        x: -2.3,
        y: 0.6,
        w: 0.9,
        h: 0.9,
        angle_deg: 0.0,
    });
    plan.props.push(PlanProp {
        id: uuid::Uuid::new_v4(),
        kind: PropKind::Triangle,
        label: "Neon Sign".into(),
        x: 2.3,
        y: -1.6,
        w: 0.9,
        h: 0.8,
        angle_deg: -15.0,
    });
    plan.props.push(PlanProp {
        id: uuid::Uuid::new_v4(),
        kind: PropKind::Square,
        label: "Jukebox".into(),
        x: 2.5,
        y: 0.9,
        w: 1.0,
        h: 0.8,
        angle_deg: 0.0,
    });
    plan.props.push(PlanProp {
        id: uuid::Uuid::new_v4(),
        kind: PropKind::Light,
        label: "Key 1K".into(),
        x: -2.4,
        y: -1.8,
        w: 0.7,
        h: 0.7,
        angle_deg: 35.0,
    });
    plan.actors.push(PlanActor {
        id: uuid::Uuid::new_v4(),
        name: "ELENA".into(),
        x: -1.0,
        y: -0.4,
        angle_deg: 0.0,
        color: ShotColor::Coral,
        move_to: Some((-0.8, 0.3)),
    });
    plan.actors.push(PlanActor {
        id: uuid::Uuid::new_v4(),
        name: "MARCO".into(),
        x: 1.0,
        y: -0.4,
        angle_deg: 180.0,
        color: ShotColor::Emerald,
        move_to: None,
    });
    plan.cameras.push(PlanCamera {
        shot_id: id1,
        x: -1.8,
        y: 2.2,
        angle_deg: -65.0,
        fov_deg: Some(65.0),
        cone_length: Some(3.2),
        track_to: Some((1.2, 2.2)),
    });
    plan.cameras.push(PlanCamera {
        shot_id: id2,
        x: 0.9,
        y: 1.4,
        angle_deg: -135.0,
        fov_deg: Some(28.0),
        cone_length: Some(2.6),
        track_to: None,
    });
    plan.cameras.push(PlanCamera {
        shot_id: id3,
        x: -0.9,
        y: 1.4,
        angle_deg: -45.0,
        fov_deg: Some(40.0),
        cone_length: Some(2.6),
        track_to: None,
    });

    // Synthetic 160x90 storyboard thumbnail image for Shot 1A
    let (w, h) = (160usize, 90usize);
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            let r = (24 + (x * 180 / w)) as u8;
            let g = (48 + (y * 120 / h)) as u8;
            let b = (110 + ((w - x) * 110 / w)) as u8;
            rgb.extend_from_slice(&[r, g, b]);
        }
    }
    let mut shot_images = HashMap::new();
    shot_images.insert(
        id1,
        pdf::DecodedShotImage {
            width: w,
            height: h,
            rgb,
        },
    );

    let read = |name: &str| {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../scriptcraft-app/assets/fonts")
                .join(name),
        )
        .expect("font")
    };
    let fonts = pdf::ExportFonts {
        regular: &read("IBMPlexMono-Regular.ttf"),
        bold: &read("IBMPlexMono-Bold.ttf"),
        italic: Some(&read("CourierPrime-Italic.ttf")),
        fallback_regular: &read("CourierPrime-Regular.ttf"),
        fallback_bold: &read("CourierPrime-Bold.ttf"),
        fallback_italic: Some(&read("LiberationMono-Italic.ttf")),
    };
    let meta = kitsp::Metadata {
        title: "MIDNIGHT DINER".into(),
        author: "Alex Mercer".into(),
        ..Default::default()
    };
    let opts = pdf::ExportOptions {
        title_page: true,
        bold_character_cues: true,
        color_by_shots: true,
        include_shot_legend: false,
        ..Default::default()
    };

    let bytes =
        pdf::to_directors_breakdown_pdf_bytes(&p, &meta, &opts, &[], None, &fonts, &shot_images)
            .expect("director's breakdown pdf");
    assert!(bytes.starts_with(b"%PDF-"));

    if let Ok(dump_path) = std::env::var("DUMP_BREAKDOWN_PDF") {
        if let Some(parent) = std::path::Path::new(&dump_path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&dump_path, &bytes).expect("dump breakdown pdf");
    }

    let doc = lopdf::Document::load_mem(&bytes).expect("parse breakdown pdf");
    assert!(
        doc.get_pages().len() >= 3,
        "expected title + breakdown sheet + colored script pages, got {}",
        doc.get_pages().len()
    );
    let breakdown_txt = doc.extract_text(&[2]).unwrap_or_default();
    assert!(
        breakdown_txt.contains("DIRECTOR'S BREAKDOWN") && breakdown_txt.contains("1A"),
        "breakdown sheet must show header and shot cards: {breakdown_txt}"
    );
}
