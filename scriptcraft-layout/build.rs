//! Generate the line-grid pitch from the bundled Courier Prime font.
//!
//! `ROW_MM` used to be a hand-copied hhea ratio. Parsing the actual TTF at
//! build time means a font swap cannot silently print on the wrong rhythm: the
//! value changes with the asset and both the editor and the PDF pick it up.

use std::path::PathBuf;

fn main() {
    let font = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../scriptcraft-app/assets/fonts/CourierPrime-Regular.ttf");
    println!("cargo:rerun-if-changed={}", font.display());
    let data = std::fs::read(&font)
        .unwrap_or_else(|e| panic!("cannot read bundled font {}: {e}", font.display()));
    let face = ttf_parser::Face::parse(&data, 0)
        .unwrap_or_else(|e| panic!("cannot parse bundled font {}: {e}", font.display()));
    let upem = f32::from(face.units_per_em());
    let height =
        f32::from(face.ascender()) - f32::from(face.descender()) + f32::from(face.line_gap());
    let row_mm = 12.0 * height / upem * (25.4 / 72.0);
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("font_metrics.rs");
    std::fs::write(
        out,
        format!(
            "/// Height of one 12 pt line-grid row, generated from Courier Prime's\n\
             /// hhea ascender/descender/line-gap in `scriptcraft-layout/build.rs`.\n\
             pub const ROW_MM: f32 = {row_mm:?};\n"
        ),
    )
    .expect("write generated font metrics");
}
