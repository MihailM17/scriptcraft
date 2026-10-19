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
