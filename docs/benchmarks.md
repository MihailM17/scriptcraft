# Benchmarks

ScriptCraft keeps a small Criterion harness for the paths that affect typing
and export latency. Run it in release mode before a release:

```sh
cargo bench -p scriptcraft-core --bench engine -- --quick
cargo bench -p scriptcraft-layout --bench layout -- --quick
```

Remove `-- --quick` for a full sample when investigating a regression.

## Baseline — A95, 21 scenes (2026-11-10)

MacBook (Apple silicon, macOS 26), Rust stable, release bench profile:

| Benchmark | Result |
|---|---|
| `kitsp_load_a95` | 0.66 ms |
| `draft_diff_a95` | 44 µs |
| `compare_latest_a95` | 44 µs |
| `revision_marks_a95` | 3.1 µs |
| `layout_a95` | 0.76 ms |
| `relayout_one_block_a95` | 92 µs |
| `pdf_a95` | 30 ms |

These are smoke budgets, not CI assertions: shared runners are too noisy for
hard thresholds. Treat a 2× regression on a quiet machine as a release blocker
until explained. The typing path must stay under 16 ms per edit; the relayout
benchmark is the relevant signal.

## What each bench covers

- `kitsp_load_a95` — SQLite open, script XML parse, metadata and sidecars.
- `draft_diff_a95` / `compare_latest_a95` — the shared revision diff used by
  compare and the revision summary.
- `revision_marks_a95` — per-block revision colours.
- `layout_a95` — full pagination and placed-line resolution.
- `relayout_one_block_a95` — incremental relayout after one edited block.
- `pdf_a95` — fonts, A4 pages and embedded PDF output.
