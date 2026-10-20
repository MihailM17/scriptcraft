//! PDF export golden checks on the A95 fixture (moved with the exporter from
//! `scriptcraft-core`, now that the exporter shares the layout engine).

use scriptcraft_core::kitsp;

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
