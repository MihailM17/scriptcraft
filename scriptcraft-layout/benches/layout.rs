//! Layout and PDF benchmarks on the 21-scene A95 fixture.
//!
//! Run with `cargo bench -p scriptcraft-layout`.

use criterion::{criterion_group, criterion_main, Criterion};
use scriptcraft_core::editing::RUSSIAN;
use scriptcraft_core::kitsp;
use scriptcraft_layout::pdf::{self, ExportFonts, ExportOptions};
use scriptcraft_layout::{layout, relayout_from, RUSSIAN_GEOMETRY};
use std::path::{Path, PathBuf};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/A95.kitsp")
}

fn font(name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../scriptcraft-app/assets/fonts")
            .join(name),
    )
    .expect("bundled font")
}

fn benches(c: &mut Criterion) {
    let path = fixture();
    let project = kitsp::load_file(&path).expect("fixture loads");
    let meta = kitsp::load_metadata(&path).expect("fixture metadata");
    let labels: Vec<String> = (1..=project.scenes().len())
        .map(|n| format!("{n}."))
        .collect();

    c.bench_function("layout_a95", |b| {
        b.iter(|| layout(&project.blocks, &RUSSIAN, &RUSSIAN_GEOMETRY, &labels))
    });

    let laid_out = layout(&project.blocks, &RUSSIAN, &RUSSIAN_GEOMETRY, &labels);
    let mut edited = project.blocks.clone();
    edited[10].text.push_str(" — revised");
    c.bench_function("relayout_one_block_a95", |b| {
        b.iter(|| {
            relayout_from(
                &edited,
                &RUSSIAN,
                &RUSSIAN_GEOMETRY,
                &labels,
                &laid_out,
                10,
                Some(10),
            )
        })
    });

    let regular = font("CourierPrime-Regular.ttf");
    let bold = font("CourierPrime-Bold.ttf");
    let fallback_regular = font("IBMPlexMono-Regular.ttf");
    let fallback_bold = font("IBMPlexMono-Bold.ttf");
    let fonts = ExportFonts {
        regular: &regular,
        bold: &bold,
        fallback_regular: &fallback_regular,
        fallback_bold: &fallback_bold,
    };
    let options = ExportOptions::default();
    c.bench_function("pdf_a95", |b| {
        b.iter(|| pdf::to_pdf_bytes(&project, &meta, &options, &fonts).expect("PDF renders"))
    });
}

criterion_group!(layout_bench, benches);
criterion_main!(layout_bench);
