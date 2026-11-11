//! Core engine benchmarks: load, revision diff and mark computation.
//!
//! Run with `cargo bench -p scriptcraft-core`; the A95 fixture is read-only.

use criterion::{criterion_group, criterion_main, Criterion};
use scriptcraft_core::kitsp;
use scriptcraft_core::revision::{DraftDiff, RevisionHistory};
use std::path::PathBuf;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/A95.kitsp")
}

fn benches(c: &mut Criterion) {
    let path = fixture();
    c.bench_function("kitsp_load_a95", |b| {
        b.iter(|| kitsp::load_file(&path).expect("fixture loads"))
    });

    let project = kitsp::load_file(&path).expect("fixture loads");
    let mut current = project.blocks.clone();
    current[10].text.push_str(" — revised");
    let mut history = RevisionHistory::default();
    history.start(
        &project.blocks,
        "Blue Revision".into(),
        "10.11.2026 10:00".into(),
        342,
    );
    let baseline = history.latest().expect("baseline").clone();

    c.bench_function("draft_diff_a95", |b| {
        b.iter(|| DraftDiff::against_current(&baseline, &current))
    });
    c.bench_function("compare_latest_a95", |b| {
        b.iter(|| history.sets[0].diff_against_current(&current))
    });
    c.bench_function("revision_marks_a95", |b| {
        b.iter(|| history.latest_marks(&current))
    });
}

criterion_group!(engine, benches);
criterion_main!(engine);
