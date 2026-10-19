# ScriptCraft — Living Roadmap & Work Log

> **Purpose.** This is the single source of truth for what has been done, what is
> being done, and what comes next. Upload this file to a new chat to restore
> full project context, then ask the assistant to append a session-log entry and
> tick off checkboxes as work lands.
>
> **Rule for assistants:** before doing anything in a session, read this file.
> After every user prompt that changes work, update the "Session log", the
> status markers below, and "Next actions". Keep edits small and factual.
> Do not delete historical log entries; append.

Status legend: `[ ]` to do `[-]` in progress `[x]` done `[!]` blocked

---

## 1. Snapshot

| Field | Value |
|---|---|
| Date | 2026-10-19 |
| Project | ScriptCraft — open-source, local-first screenwriting studio |
| Repo | `/Users/mihailmihaylov/Documents/Dev_work/scriptcraft` |
| Branch / commit | `main`; initial commit `48e2fed` "Initial commit: ScriptCraft 100% Rust screenwriting studio" |
| Rust workspace | `scriptcraft-core`, `scriptcraft-app`, `scriptcraft-layout` (`Cargo.toml`, resolver 2) |
| App crate version | `scriptcraft-app 0.1.0`, `scriptcraft-core 0.1.0` |
| Distribution | `dist/ScriptCraft-0.4.0-macos-arm64.zip`, `dist/ScriptCraft.app` (unsigned, manual builds) |
| Current one-liner | KIT-compatible prototype with one screenplay engine (Editor 2.0), crash-safe saves + recovery, OFL fonts, FDX import/export; editor/PDF templates still diverge. |

**Positioning (locked):** the pro-grade, open-source, local-first screenplay
studio for working writers. Moat = lossless KIT `.kitsp` + Final Draft `.fdx`
interoperability, native Rust speed, and a writing surface better than Final
Draft — no account, no subscription, no cloud dependency.

**Locked decisions (2026-10-19, user answers):**

1. Product model: open-source, local-first. No subscription. Optional paid
   sync/services much later. License target: Apache-2.0 (same as ArtCraft).
2. Primary user for v1: professional film/TV writers. FDX, revisions,
   production drafts, dual dialogue, writers-room reliability.
3. Stack: evolve the existing egui/eframe app. No GPUI rewrite, no web frontend.
4. Formats: keep reading/writing `.kitsp`; add `.fdx`; introduce a better native
   format only if/when KIT's schema blocks pro features.

**Non-negotiables:** no data loss; editor pagination == PDF pagination; FDX/KIT
round-trip fidelity; <16 ms typing latency; local-first/offline; Apache-2.0 with
no proprietary assets; keyboard-first and accessible; every UI claim must ship.

---

## 2. Repo map & commands

- `scriptcraft-core/src/`
  - `model.rs` — `ElementType`, `ScriptBlock`, `Scene`, `Project`, `Stats`, KIT chronometry.
  - `kitsp.rs` — `.kitsp` SQLite reader/writer, metadata, research, cards, versions, HTML helpers.
  - `editing.rs` — element transitions, autocomplete candidates, template metrics (`RUSSIAN`, `FINAL_DRAFT_A4`), text helpers for coach/duration. The wrap/paginator were deleted at M5; layout lives in `scriptcraft-layout`.
  - `edit.rs` — `EditCommand` (insert/delete/set/split/merge) with exact inverse sequences + `History` undo/redo (Editor 2.0, M2 done).
  - `fountain.rs` — Fountain export/import.
  - `fdx.rs` — Final Draft `.fdx` XML export/import (paragraphs, title page, scene numbers, revision scaffold).
  - `format.rs`, `coach.rs` — validation and coaching lints.
- `scriptcraft-layout/src/` — the layout engine (Editor 2.0, M5 done):
  - `wrap.rs` — greedy wrap ported 1:1 from `editing` + source char spans (ranges tile each paragraph); `row_count` is the count-only twin used on hot paths.
  - `paginate.rs` — page breaker ported 1:1 from `editing::paginate_m`; `Chunk`/`Page`; stepwise `Pager` state machine (M5 perf) so `place::relayout_from` can replay pagination from any page.
  - `place.rs` — `PlacedLine`s in document units (rows × `ROW_MM`; x in mm): body lines with `SourceSpan`, `(CONT'D)` cues, `(MORE)` rows, margin scene numbers, page numbers. `relayout_from` rebuilds only the pages an edit can have affected and splices the rest (differentially fuzzed against `layout` in `tests/relayout.rs`).
  - `pdf.rs` — print layout + `printpdf` renderer (title page, scene/page numbers, watermarks).
  - `tests/` — `parity.rs` (golden hashes), `pagination.rs`, `place.rs`, `pdf_export.rs`, `scale.rs` (500-page timings).
- `scriptcraft-app/src/`
  - `app.rs` — 6,077-line monolith: all state, IO, views, editor, export dialog.
  - `script_view.rs` — Editor 2.0 `ScriptView` (the only editor since M5): paints `DocLayout` pages (visible pages only; zoom scales geometry, never breaks) and edits by emitting `core::edit` commands — selection/caret, keyboard/pointer, cross-block clipboard, IME, command-history undo/redo.
  - `theme.rs` — palette + egui style. `menu.rs` — native macOS menu. `main.rs`, `lib.rs`.
  - `assets/fonts/` — Courier Prime (Regular/Bold/Italic), PT Sans, IBM Plex Mono; all OFL, license texts in `assets/fonts/licenses/`.
- `scriptcraft-app/tests/` — `ui_snapshots.rs` (kittest snapshots), `interaction.rs` (end-to-end clicks/keys), `snapshots/*.png`.
- `scriptcraft-core/tests/test_a95.rs` — fixture tests (21 scenes, 352 blocks).
- `fixtures/A95.kitsp` — 21-scene Bulgarian project, the compatibility fixture; `fixtures/example.fdx` — small Final Draft sample for the FDX tests.
- `docs/kit-design.md`, `docs/kitsp-format.md` — reverse-engineered KIT notes.

Commands:

```sh
cargo run -p scriptcraft-app
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
UPDATE_SNAPSHOTS=1 cargo test -p scriptcraft-app --test ui_snapshots
cargo run -p scriptcraft-core --example export_pdf -- <file.kitsp> <out.pdf> [--no-title]
cargo build --release -p scriptcraft-app
```

CI lives in `.github/workflows/ci.yml`: fmt + clippy on Linux, full workspace
tests (incl. snapshots) on macOS, core tests + app compile on Linux/Windows,
`cargo-deny-action`. `deny.toml` allows permissive licenses plus MPL-2.0
(azul-simplecss via printpdf) and only flags unmaintained crates that are
direct workspace dependencies. There is still no benchmark harness.

---

## 3. Verified known issues (fix these before feature work)

- [x] **Metadata key mismatch / scene-numbering loss.** Fixed 2026-10-19:
  `load_metadata` reads KIT's canonical `scene_numbers_prefix` /
  `scene_start_number` first with legacy singular fallback; `save_metadata`
  writes canonical keys and keeps an existing legacy row in sync; `create_file`
  seeds canonical keys. Covered by `test_a95.rs` tests incl. legacy migration.
- [x] **Metadata not saved by the normal save path.** Fixed 2026-10-19:
  `do_save` now calls `write_metadata` (only when title-page edits are dirty,
  so untouched KIT HTML is never rewritten); `flush_meta` keeps edits pending
  for read-only/unsaved projects so Save As carries them; Fountain import marks
  metadata dirty so imported title-page fields persist on first save.
- [x] **Non-atomic saves.** Fixed 2026-10-19: `save_file` keeps the `.bak`
  copy, then builds the new file at `<name>.tmp.<pid>` in the same directory,
  fsyncs it, renames it over the target, and fsyncs the parent dir
  (best-effort). Temp file is removed on any failure, so a crash mid-save can
  never leave a half-written project (`kitsp.rs:109`).
- [x] **No crash recovery.** Fixed 2026-10-19: while a project is dirty the app
  snapshots it (script + title page) to `~/Library/Application Support/
  ScriptCraft/recovery/` — immediately on the first dirty frame, then every
  10 s; copies are removed on every successful save; the launch banner offers
  Restore (keeps the original path if it still exists, re-marks dirty) or
  Discard (`app.rs` `write_recovery`, `recovery_bar`, `restore_recovery`).
- [x] **Editor is per-chunk `TextEdit`.** Fixed 2026-10-19 (M5): the chunk
  editor and old paginator were deleted; `ScriptView` (Editor 2.0) is the only
  editor — one layout engine for pages, commands via `core::edit::History`.
- [-] **Editor and PDF template divergence.** Both now render through
  `scriptcraft-layout`, but the editor shows the `RUSSIAN` (54-row) template
  while the PDF export renders `FINAL_DRAFT_A4` (58-row); breaks are
  consistent within each template, yet WYSIWYG (same template in editor and
  PDF) is still open. (`editing.rs` templates; `scriptcraft-layout/src/pdf.rs`.)
- [ ] **Snapshot tests on CI GPUs unverified.** `ui_snapshots.rs` renders with
  wgpu/Metal; locally green, but GitHub's macOS runner GPU may produce pixel
  diffs. If CI goes red on snapshots only, gate them behind an env var.
- [ ] **`block v0.1.6` is future-incompatible** (transitive via `metal`/
  `wgpu-hal`, macOS-only). Not a build failure; disappears when wgpu updates.
- [-] **Unimplemented formats exposed in UI.** Fixed 2026-10-19: FDX shipped,
  the DOCX radio is hidden, README claims corrected. Remaining: DOCX export
  itself (Phase 2).
- [x] **Proprietary fonts bundled.** Fixed 2026-10-19: `cour*.ttf` removed;
  editor and PDF now use Courier Prime (OFL), OFL license texts added for all
  bundled fonts, root `LICENSE` (Apache-2.0) and `NOTICE` added.
- [ ] **Monolith.** `app.rs` is 6,077 lines; state, IO, layout and UI are entangled.
- [ ] **No production/revision layer.** No revision sets/colors, A/B pages,
  locked pages, dual dialogue, omitted scenes, title-page editor, scene
  metadata, spellcheck, or autosave recovery.
- [ ] **Hardcoded font metrics.** `ROW_MM` is the measured Courier Prime value
  (`scriptcraft-layout/src/place.rs:24`), but is still a constant. Must derive
  from the TTF at runtime (e.g. `ttf-parser`) so swapping fonts cannot break
  pagination.

---

## 4. Roadmap

### Phase 0 — Trust (week 1)

- [x] Apache-2.0 `LICENSE`, `NOTICE`, font license files; remove `cour*.ttf`.
- [-] Switch editor + PDF to Courier Prime (done); derive line metrics from the
  font file (pending — constant updated to the measured 2300/2048 for now).
- [x] Fix metadata key aliases (accept both spellings); persist metadata in `do_save`.
- [x] Atomic saves: write temp in same dir → fsync → rename; keep `.bak`.
- [x] Recovery file + launch-time "restore unsaved changes" prompt.
- [x] A95 round-trip test: untouched tables/rows compared (deterministic dump,
  BLOBs hex-encoded) before and after `save_file`.
- [x] CI: fmt, clippy `-D warnings`, tests on macOS/Windows/Linux; `cargo deny`
  (RustSec audit included). Workflow added 2026-10-19; first green run pending
  the next push. (cargo-audit itself is redundant next to `cargo deny advisories`.)
- [x] Hide DOCX/FDX in the UI until implemented; correct README claims. Done 2026-10-19: FDX implemented, DOCX radio hidden, README updated.

Exit: no known data-loss path; license surface clean; CI green.

### Phase 1 — Pro writing core (weeks 2–6)

- [ ] New crate `scriptcraft-layout`: one pagination/wrap engine shared by editor and PDF; golden tests.
- [ ] `scriptcraft-core`: format-agnostic document model, stable IDs, `EditCommand` + inverse ops.
- [ ] Custom `ScriptView` widget: global `Selection{anchor,focus}`, cross-block selection, multi-block delete/copy/paste, smart paste (FDX/Fountain/text), IME/accessibility baseline.
- [ ] Command-based undo/redo (replace full-snapshot history in `app.rs:429`).
- [-] FDX import/export: paragraphs, title page, scene numbers, revisions scaffold + sample fixture + round-trip tests shipped 2026-10-19. Remaining: FDX clipboard smart paste.
- [ ] Fountain 1.1 completion: sections, synopsis, notes, boneyard, dual dialogue, page breaks, emphasis; round-trip tests.
- [ ] PDF export with embedded/subset Courier Prime; title page, scene/page numbers, watermarks; in-app print preview from the shared layout.
- [ ] Split `app.rs` into `ui/state.rs`, `ui/toolbar.rs`, `ui/editor.rs`, `ui/navigator.rs`, `ui/modules/*`, `ui/dialogs/*` (no behavior change).

Exit: open KIT or FDX, edit, save KIT losslessly, deliver page-exact PDF/FDX/Fountain.

### Phase 2 — Production & revisions (weeks 6–12)

- [ ] Revision sets/colors, revision marks, omitted scenes, A/B pages, locked page/scene numbers, version compare/restore.
- [ ] Dual dialogue; more/cont'd edge cases; page-break controls.
- [ ] Title-page WYSIWYG editor; templates (Final Draft A4/Letter, KIT, custom) as data.
- [ ] Production reports: scene/location/cast/structure/activity, CSV + HTML export.
- [ ] Spellcheck (pure-Rust, e.g. `zspell`); batch watermark; DOCX export.

### Phase 3 — Story & research (weeks 12–20)

- [ ] Beat board/cards/folders/colors/tags, drag/drop, outline templates.
- [ ] Character/location DB: photos, relationships, global search.
- [ ] Statistics charts (duration, dialogue balance, pacing); onboarding, project templates, sample projects.

### Phase 4 — Polish & scale (weeks 20–30)

- [ ] Design system 2.0 (motion, spacing, high contrast); command palette; customizable shortcuts; EN/BG localization.
- [-] 500-page performance pass + incremental layout done 2026-10-19 (M5):
  full relayout ≈ 44–54 ms release, incremental typing ≈ 0.7 ms/keystroke
  (`scriptcraft-layout/tests/scale.rs`); background IO still open.
- [ ] Signed/notarized macOS builds, auto-update, Windows/Linux parity, file associations.
- [ ] Docs site, user guide, contributing guide, beta program.

### Phase 5 — Optional (later)

- [ ] CRDT sync/collaboration, comments, writer's-room features.
- [ ] Plugin/WASM API; BYO-key AI assistance. Only after the craft core is excellent.

---

## 5. Current sprint (two weeks)

Order matters; do not start a later item before the earlier one is verified.

1. [x] Legal/asset cleanup (LICENSE, fonts, NOTICE).
2. [x] Metadata fix + A95 round-trip test.
3. [x] Atomic save + recovery file.
4. [x] CI + `cargo deny`/`audit`.
5. [-] **Editor 2.0** — merges the former items 5 + 7 (user decision
   2026-10-19): one shared layout engine + a custom `ScriptView` widget, before
   FDX. Zoom scales/pans the page only (never reflows; Fit width/Fit page/
   100% shipped 2026-10-19; continuous pointer-anchored zoom + middle-drag
   pan shipped 2026-10-19); the old chunk editor gets no more patches and is
   deleted at M5.
   - [x] **M1** `scriptcraft-layout` crate: ported wrap + paginator + placed
     lines in document units, golden-parity-tested against `editing`. Done
     2026-10-19.
   - [x] **M2** `EditCommand` + exact inverse sequences in core
     (`scriptcraft-core/src/edit.rs`), plus `History` undo/redo; the app
     switches to command undo at M5. Done 2026-10-19.
   - [x] **M3** read-only `ScriptView` (renders `DocLayout`, visible pages
     only); zoom-invariant tests (identical breaks at 95/100/150/200%). Done
     2026-10-19.
   - [x] **M4** selection/caret/clipboard/IME behind a flag:
     `Selection{anchor,focus}`, cross-block copy/cut/paste, split/merge via
     commands. Done 2026-10-19.
   - [x] **M5** switchover done 2026-10-19: old chunk editor + old paginator
     deleted; `ScriptView` is the only editor; find/autocomplete/navigator/
     cards/typewriter/stats rewired; perf pass — incremental relayout via
     `place::relayout_from` (differentially fuzzed) keeps typing at ≈
     0.7 ms/keystroke on a 500-page script (release; full relayout ≈ 50 ms).
6. [x] FDX import/export with sample fixtures (moved after Editor 2.0). Done
   2026-10-19: `core::fdx` + `fixtures/example.fdx`, A95 round-trip, menu
   import, Export ▸ FDX.

---

## 6. Session log

Newest entries at the bottom. Each entry: date — what changed / what was found.

- **2026-10-19** — Deep read-only audit of the repo. Produced architectural
  assessment and the phased roadmap above. User locked product decisions
  (open-source local-first, pro writers, evolve egui, keep KIT + add FDX).
  Created this living document. No code changes yet.
- **2026-10-19** — Sprint item 1 done: legal/asset cleanup.
  - Added root `LICENSE` (Apache-2.0) and `NOTICE` (bundled-font attributions).
  - Added `assets/fonts/licenses/` OFL texts for Courier Prime, PT Sans, IBM Plex Mono.
  - Removed proprietary `cour*.ttf`; added `CourierPrime-Italic.ttf` (OFL-from Google Fonts).
  - Editor + PDF now load Courier Prime (`app.rs` `install_fonts`, `pdf_bytes`;
    `scriptcraft-core/examples/export_pdf.rs`, `test_a95.rs`).
  - `ROW_FACTOR` updated to measured Courier Prime value `2300/2048` (runtime
    derivation still pending); comments/docs/README updated (no more Courier New).
  - Regenerated 4 UI snapshots (`01`, `02`, `08`, `09`); verified script pages
    visually (grid intact, no drift).
  - Verification: `cargo fmt --check` OK; `cargo test -p scriptcraft-core` 39+16
    pass; `cargo test -p scriptcraft-app` 8 interaction + 10 snapshot tests pass.
  - Next: sprint item 2 (metadata key fix + A95 round-trip test).
- **2026-10-19** — Sprint item 2 done: metadata keys + no-data-loss tests.
  - `kitsp.rs`: canonical key constants (`scene_numbers_prefix`,
    `scene_start_number`); `get_data` NULL-safe (`COALESCE`); `get_data_alias`
    reads canonical first, legacy singular fallback; `save_metadata` upserts
    canonical keys and syncs legacy rows only when they already exist;
    `create_file` seeds canonical keys. Canonical names confirmed against
    KITScenaristCore `develop` (`Domain/ScenarioData.cpp`).
  - `app.rs`: new `sync_meta_buffers` + `write_metadata` (clears dirty flag on
    success); `do_save` persists dirty metadata; `after_save_as` carries dirty
    metadata into the new file; `flush_meta` keeps dirty edits from read-only/
    unsaved projects so Save As preserves them; Fountain import marks metadata
    dirty and resets logline/synopsis buffers; `after_save_as` takes `&Path`.
  - `test_a95.rs`: new tests `metadata_reads_kit_scene_number_keys`,
    `metadata_reads_and_migrates_legacy_scene_number_keys`,
    `save_metadata_does_not_invent_legacy_keys`,
    `a95_save_file_preserves_all_other_tables` (dump-based before/after compare
    of 10 tables + `scenario_data` + `scenario` id=2 across `save_file`);
    `a95_metadata_roundtrip` extended with scene-numbering round-trip.
  - Verification: `cargo fmt --check` OK; `cargo test -p scriptcraft-core`
    39 unit + 20 `test_a95` pass; `cargo test -p scriptcraft-app` 8 interaction
    + 10 snapshot pass; clippy clean vs. pre-existing warnings only.
  - Next: sprint item 3 (atomic save + recovery file).
- **2026-10-19** — Sprint item 3 done: atomic saves + crash recovery.
  - `kitsp.rs`: `save_file` now copies `.bak`, builds the new file at
    `<name>.tmp.<pid>`, fsyncs, renames over the target, best-effort dir
    fsync, and removes the temp on failure. New `write_recovery_file`
    (full loadable copy + `RECOVERY_ORIGIN_KEY` recording the original path)
    and `recovery_origin`.
  - `app.rs`: dirty projects snapshot to the recovery dir immediately then at
    most every 10 s (production only); copies carry title-page fields; every
    successful save clears them; launch banner with Restore/Discard
    (`recovery_bar`, `restore_recovery`, `discard_recovery`); restored
    sessions keep the original path when it exists and are marked dirty.
  - Tests: `save_file_keeps_bak_and_leaves_no_temp_files`,
    `recovery_file_roundtrip` (core); `recovery_bar_restores_and_discards`
    (interaction, 9 total now). No snapshot changes (banner only appears with
    recovery copies; tests inject them via `pin_recovery_for_test`).
  - Verification: `cargo fmt --check` OK; `cargo test -p scriptcraft-core`
    39 unit + 22 `test_a95` pass; `cargo test -p scriptcraft-app` 9 interaction
    + 10 snapshot pass; clippy clean vs. pre-existing warnings only.
  - Next: sprint item 4 (CI + `cargo deny`/`audit`).
- **2026-10-19** — Sprint item 4 done: CI + `cargo deny` (+ printpdf/quick-xml
  upgrade it forced, and two real bugs it uncovered).
  - Upgraded `printpdf 0.7 → 0.12` and `quick-xml 0.37 → 0.41`
    (`scriptcraft-core/Cargo.toml`); fixed the two deprecated
    `decode_and_unescape_value` call sites (`kitsp.rs` now uses
    `decoded_and_normalized_value(XmlVersion::Implicit1_0, ..)`). This removed
    the lopdf RUSTSEC-2026-0187 and quick-xml RUSTSEC-2026-0194 advisories.
  - Rewrote `pdf.rs` for printpdf 0.12's Op-based API (`PdfDocument::new`,
    `add_font`, `PdfPage::new`, `Op::StartTextSection`/`SetFont`/
    `SetTextMatrix::Translate`/`ShowText`, `DrawLine`); `PdfError::Backend`
    replaced by `PdfError::Font`. Bug found visually (fixture → PNG): the new
    `TextMatrix` is in **points** while all our geometry is mm — added
    `MM_TO_PT` conversion; units error had squashed the whole page into one
    line. Lesson: always rasterize an export and look at it.
  - **Courier Prime has no Cyrillic** (verified in the TTF cmap) — the Bulgarian
    fixture's headings were never actually rendering in PDFs. Added a fallback
    chain to the exporter: `ExportFonts { regular, bold, fallback_regular,
    fallback_bold }`; text is split per character coverage (`split_runs`) and
    drawn with IBM Plex Mono (same 0.6 em advance, OFL) when Courier Prime
    lacks the glyph. App/example/tests pass both font pairs.
  - `a95_exports_valid_pdf` rewritten: parses the output with `lopdf` (new
    dev-dependency), asserts the page count equals the paginator's, and asserts
    both Latin (`A95`, `MORNING`) and Cyrillic (`ПАРКИНГ`) text extract via the
    embedded fonts' ToUnicode maps (proves the fallback face is embedded with a
    working cmap). Size floor lowered to 20 KB (streams are now compressed).
  - `deny.toml`: `unmaintained = "workspace"` (transitive bincode/paste/
    proc-macro-error/ttf-parser/zbus_names/zvariant have no fixed releases and
    don't parse untrusted input for us; noted in-file); MPL-2.0 allowed
    (azul-simplecss via printpdf); removed the zvariant ignore. All four checks
    (`advisories/advisories/bans/licenses/sources`) pass.
  - `.github/workflows/ci.yml`: fmt + `clippy -D warnings` (Linux), full
    workspace tests incl. snapshots (macOS), core tests + app compile
    (Linux/Windows), `cargo-deny-action`. Wildcard path-dep on
    `scriptcraft-core` pinned with `version = "0.1.0"` so `bans` passes.
  - Verification: `cargo fmt --check` OK; `cargo clippy --workspace
    --all-targets -- -D warnings` OK; `cargo test --workspace` = core 39+22,
    app 9 interaction + 10 snapshots, all pass; `cargo deny check` = advisories
    ok, bans ok, licenses ok, sources ok; exported PDF of `fixtures/A95.kitsp`
    visually verified (Latin + Cyrillic, correct A4 geometry).
  - Next: sprint item 5 (`scriptcraft-layout` crate with editor/PDF golden
    parity test) — this is the big one: one pagination engine for editor and
    export.
- **2026-10-19** — Editor 2.0 kickoff: sprint reorder + M1 `scriptcraft-layout`.
  - Plan locked with the user: Editor 2.0 (formerly items 5+7) runs before FDX;
    zoom is page zoom only (Fit width/Fit page/100% — never reflows); no
    interim patches to the old chunk editor. Milestones M1–M5 in "Current
    sprint"; architecture reference: WordCraft (MIT/Apache-2.0) canvas +
    engine-`Selection` patterns; KIT `.kitss` behavior kept.
  - New workspace member `scriptcraft-layout` (Apache-2.0, depends only on
    `scriptcraft-core`):
    - `wrap.rs` — 1:1 port of `editing::wrap_lines`/`push_word`/`hyphen_pieces`
      returning source char spans that tile each paragraph exactly.
    - `paginate.rs` — 1:1 port of `editing::paginate_m` (anchors, orphan
      guards, marker budgeting) with local `Chunk`/`Page` types.
    - `place.rs` — chunks → `PlacedLine`s in document units: `row` ×
      `ROW_MM` (Courier Prime 12 pt × 2300/2048), x in mm from the text-column
      edge; `(CONT'D)` cues, `(MORE)` rows, margin scene numbers (4 mm gap),
      KIT page numbers, `SourceSpan` on every body line.
    - `RUSSIAN_GEOMETRY` mirrors the editor's `FRAC_*` columns in mm.
  - Tests (11 new): `tests/parity.rs` — chunk-for-chunk equality with
    `editing::paginate_m` on A95 and 200 synthetic documents under both
    templates, plus wrap-text parity on every fixture paragraph at every
    template width; `tests/place.rs` — rows/text/x recomputed from the old
    engine match exactly, spans tile paragraphs and reproduce display text,
    scene numbers align with their headings, split dialogue carries correct
    cue/(MORE) rows; unit tests for hyphen breaks, blank paragraphs and range
    tiling. App code untouched (M3 wires the widget).
  - Verification: `cargo test -p scriptcraft-layout` 11 pass;
    `cargo test --workspace` = core 39+22, app 9 interaction + 10 snapshots,
    layout 4 unit + 3 parity + 4 place — all pass; `cargo fmt --check` OK;
    `cargo clippy --workspace --all-targets -- -D warnings` OK;
    `cargo deny check` advisories/bans/licenses/sources OK.
   - Next: M2 — `EditCommand` + inverse ops in `scriptcraft-core` with
     command-based undo/redo.

- **2026-10-19** — Editor 2.0 M2: `EditCommand` + exact inverses +
  `History` (`scriptcraft-core/src/edit.rs`, `pub mod edit`).
  - `Pos { bi, para, ch }` (document order), `EditError { InvalidBlock,
    InvalidPosition, MultiBlockRange }` (thiserror). Commands: `InsertText`,
    `DeleteRange` (both ends in one block; reversed endpoints accepted),
    `SetType` (leaves `kit_uuid` alone), `SetText`, `InsertBlock`,
    `DeleteBlock`, `SplitBlock { at, new_type, fuse }` (fuse is only
    meaningful mid-paragraph; at char 0 it degrades to a boundary split),
    `MergeBlocks { bi, fuse }`.
  - `apply` returns the exact undo command sequence: split/merge return two
    steps (whole-text `SetText` restore + `InsertBlock`/`DeleteBlock`
    carrying the removed block itself), so undo/redo are byte-for-byte
    identical including `id`/`kit_uuid`; every other command is one step.
  - KIT invariant enforced on every command: trailing empty paragraphs are
    trimmed (`InsertBlock` normalizes its block; split normalization can trim
    the boundary), which is exactly why split's inverse is a whole-text
    restore — a normalized empty boundary cannot be re-merged unambiguously.
  - Merge semantics follow Word rules: merging into an empty upper block
    removes it; an empty lower block leaves the upper text untouched.
  - `History`: undo/redo stacks of inverse sequences (`DEFAULT_HISTORY_LIMIT =
    1000`, oldest dropped), redo cleared on new edits; `apply` for one
    command, `apply_seq` for a compound user step.
  - Tests (10 unit): strict per-command round trips, merge identity, invalid
    positions, history undo/redo-all (strict) + limit eviction, and a
    4000-step LCG fuzz over all command kinds asserting strict document
    equality after apply → inverse. The fuzz caught and fixed: `DeleteRange`
    trailing-collapse inverse (fall back to `SetText`), lossy split/merge
    inverses (replaced by exact sequences), unnormalized `InsertBlock`.
  - Verification: `cargo test -p scriptcraft-core edit::` 10/10;
    `cargo test --workspace` = core 49+22, app 9 interaction + 10 snapshots,
    layout 4+3+4 — all pass; `cargo fmt --check` OK;
    `cargo clippy --workspace --all-targets -- -D warnings` OK (only the
    known `block` future-incompat note); `cargo deny check` all four OK.
  - Next: M3 — read-only `ScriptView` widget rendering `DocLayout` (visible
    pages only) with the zoom-invariant test (identical breaks at
    95/100/150/200%).

- **2026-10-19** — Editor 2.0 M3: read-only `ScriptView` preview wired in
  (`scriptcraft-app/src/script_view.rs`).
  - Painting: `place::layout` → pages drawn as pinned A4 sheets (KIT
    margins 37.5/25/25 mm, `ROW_MM` grid, KIT page numbers from page 2,
    hanging scene numbers, `(MORE)`/`(CONT'D)` rows); colors/fonts match the
    chunk editor (bold cues, italic lyrics, muted notes/scene numbers).
    Only pages intersecting the clip rect are painted; the scroll area spans
    the full document height, with its own scroll state (`id_salt`).
  - Zoom is paint-only by construction: `layout_for_zoom` ignores its zoom
    argument, so breaks are provably zoom-invariant; pinch and View ▸ Zoom
    still work (menu + native macOS menu item "Editor 2.0 preview", checked
    state synced).
  - Tests: 3 unit (identical `DocLayout` at 95/100/150/200% incl. a
    400-block multi-page doc; linear page scaling; A4-at-72dpi at zoom 1);
    1 interaction — on the A95 fixture the layout engine's page count equals
    the legacy paginator's, and preview frames at 95/100/200% paint without
    touching the document; 1 new snapshot `11_editor2_preview` (visually
    verified against `01_script_top` — same grid, differing only where the
    old editor clips single-line elements instead of wrapping like the
    paginator/PDF).
  - Verification: `cargo test --workspace` = app 3 unit + 10 interaction +
    11 snapshots, core 49+22, layout 4+3+4 — all pass; `cargo fmt --check`
    OK; `cargo clippy --workspace --all-targets -- -D warnings` OK;
    `cargo deny check` all four OK.
  - Next: M4 — selection/caret/clipboard/IME on `ScriptView` behind the same
    flag (old editor stays the default): `Selection{anchor,focus}`,
    cross-block copy/cut/paste, split/merge through `core::edit` commands.

- **2026-10-19** — Editor 2.0 M4: `ScriptView` is interactive (still behind
  View ▸ Editor 2.0 preview; the chunk editor stays the default).
  - Architecture: the widget never mutates the project. Events are processed
    in order against a per-frame working copy (`Editor`), so positions stay
    valid within a frame; the frame's commands are returned as
    `ScriptViewOutput` and the app applies them as one history step via
    `core::edit::History` — undo/redo restore byte-for-byte (unit/history
    tests already lock this). `EditState` (selection, history, structured
    clipboard, scroll flag) lives in the app; opening a file resets it.
  - Input: click/drag hit-tests body lines from placed geometry (nearest
    line + cell, no dead zones); arrows/Home/End/word-style Up-Down column
    keeping, Shift-selection, ⌘A select-all; typing via `Event::Text` and
    `ImeEvent::Commit`; cut/copy/paste through `Event::Copy/Cut/Paste`
    (`internal_clip` keeps element types, so a multi-block paste recreates
    blocks via `InsertBlock`); Enter splits with `enter_next` continuation;
    Backspace/Delete join blocks at boundaries; Tab/⇧Tab cycle element
    types. Tab and arrows are claimed with an egui focus-lock filter.
  - Caret: drawn when focused, caret rect fed to `output.ime` (IME popup
    positioning); scroll-to-caret on edits; `Cmd+Z` still intercepted at
    app level and routed to the preview history; toolbar/menu Undo-Redo
    enablement follows the active editor.
  - Keyboard-only issue found while wiring: entering text via `Event::Text`
    also arrives for Enter — filtered (`
`-only text is ignored; the key
    event owns splits).
  - Tests: 10 new unit tests in `script_view.rs` (navigation across
    paragraphs/blocks, wrap-aware Up/Down, cross-block delete joins,
    backspace merge, KIT Enter continuation, clipboard fragments, hit-test,
    Tab cycling, shift-extend, structured paste); 3 new interaction tests
    (click→type→⌘Z; Enter-splits-then-Backspace-merges on A95;
    cut→paste round trip); new snapshot `12_editor2_selection` (selection
    highlight + caret, visually verified). `core::edit` gained
    `clamp_pos` (+test) so a stale caret after undo cannot leave the
    document.
  - Verification: `cargo test --workspace` = app 13 unit + 13 interaction +
    12 snapshots, core 50+22, layout 4+3+4 — all pass; `cargo fmt --check`
    OK; `cargo clippy --workspace --all-targets -- -D warnings` OK;
    `cargo deny check` all four OK.
  - Known gaps (M5 polish): no IME preedit underline; no blinking caret;
    the working copy clones the block list once per input frame (fine for
    A95; revisit in the M5 perf pass); find/autocomplete/navigator/
    typewriter/stats still talk to the old editor only.

- **2026-10-19** — Editor 2.0 M5: switchover complete — `ScriptView` is the
  only editor, the old chunk editor and old paginator are deleted, and the
  perf pass is done (incremental relayout).

  Switchover & deletions:
  - `scriptcraft-core/src/editing.rs`: the old wrap/paginator
    (`paginate`, `chunk_text`, `wrap_lines`, `PageChunk`, `A4_LINES_PER_PAGE`,
    anchors/contd helpers) deleted; kept text utilities (`enter_next`,
    autocomplete candidates, `template` metrics, `cue_name_for`, …).
    `wrap_rows_chars` is now count-only (mirrors layout break rules) for
    coach/statistics. `parity.rs` `golden_dump` no longer A/B-compares the two
    engines (constants were captured while both existed; the tests pin the
    current engine); the A/B test in `tests/place.rs` was retired and its
    page-number assert moved into the scene-number test.
  - `scriptcraft-app/src/app.rs`: old editor removed (incl. its
    `delete_selected`/`sync_card_title` leftovers); Menu/View items for the
    old editor gone; popup rows for autocomplete are fed from the widget's
    debug hook.
  - `script_view.rs` bug found by the rewritten interaction tests: egui
    surrenders focus when a pointer press lands "elsewhere" (`context.rs`
    `pointer_pressed_elsewhere`), so clicking an autocomplete row defocused the
    widget before the click resolved. Fix: remember last frame's popup rects
    and re-request focus when this frame's press landed on them.
  - Tests rewritten to the new editor: `interaction.rs` (13) — typing/undo,
    Enter/Backspace split-merge, cut/paste, autocomplete pick/escape, zoom
    page-geometry ratio, zoom-invariant page count on A95, export dialog,
    recovery bar, theme, menu; snapshots: `11_editor2_preview` deleted,
    `12_editor2_selection` → `12_script_selection`, all 11 regenerated and
    spot-checked visually.

  Perf pass (sprint M5 perf; Phase 4 first half):
  - `wrap::row_count` — count-only twin of `wrap_paragraph` (differential
    test); paginate on the 500-page script 19.7 ms → 7 ms (release).
  - `paginate::Pager` — the page breaker is now a stepwise state machine
    (`new/step/run_until/finish`), same output (golden parity locked), so
    relayout can replay pagination from any page.
  - `place::relayout_from` — rebuilds only the pages an edit can have
    affected: resumes at the first page of the lowest touched block (plus a
    2-block pull-back because orphan guards peek ahead), walks back when the
    resume page opens mid-block (continuation), then rebuilds page-by-page
    and, for pure in-place text edits, converges by splicing the untouched
    old tail once a rebuilt page equals the old one and the changed block is
    consumed (never for changed Character blocks — later `(CONT'D)` labels
    scan back to them). Differentially fuzzed against full `layout` in
    `tests/relayout.rs`: 240 random documents × random edits (text, split,
    merge, insert/delete block, retype) + 8 targeted A95 edits — all equal.
  - App: `apply_editor2_output` derives the hint from the command batch
    (same-block text edits only), `refresh_derived` consumes it; structural
    edits, undo/redo and opens fall back to a full relayout.
  - `tests/scale.rs` — 500-page script (3,500 blocks → 500 pages, 24,750
    lines): release ~7 ms paginate, ~50 ms full relayout; incremental typing
    **~0.7 ms per keystroke (≈80× faster)**; widget working-copy clone
    ~0.17 ms/input frame — first keystroke budget comfortably inside the
    16 ms non-negotiable.
  - Verification: `cargo test --workspace` all green (app 13 unit + 13
    interaction + 11 snapshots; core 40 lib + 20 a95; layout 12 unit + 3
    parity (+1 ignored) + 1 pagination + 3 place + 1 pdf_export + 3 scale +
    2 relayout); `cargo fmt --check` OK; `cargo clippy --workspace
    --all-targets -- -D warnings` OK; `cargo deny check` all four OK.
  - Known gaps carried: IME preedit underline and blinking caret still open;
    template divergence (editor RUSSIAN vs PDF FINAL_DRAFT_A4) tracked in
    known issues.

- **2026-10-19** — User report investigated ("gap between the text and the
  page end changes with zoom", 79%/88% screenshots) → measured, answered, and
  closed with the roadmap-promised fit modes.
  - Measurement (throwaway pixel probe over rendered harness frames, deleted
    after use): page-1 top gap ≈ 25.4–25.9 mm (≈5.4 rows) and bottom gap ≈
    19.8–20.3 mm (≈4.2 rows) at zoom 0.50 / 0.79 / 0.88 — constant in mm and
    rows, i.e. the painted geometry is exactly zoom-linear (every
    `script_view` term is `× zoom`). The apparent shift was the window
    clipping the taller page at higher zoom, not a layout change. The toolbar
    percentages are the font-grid-snapped rendering zoom (`render_zoom`:
    integer 12-pt pixel steps), so 79% = 19/24 and 88% = 21/24.
  - New **View ▸ Fit Page / Fit Width** (native View menu + in-app File menu
    for parity). `script_view::fit_page_zoom` / `fit_width_zoom` compute the
    zoom from the live script viewport; `app::FitMode` re-fits every frame
    (window resize included) and `snap_down_zoom` floors to the font grid so
    a fit can never overflow. Zoom in/out, 100%, and pinch all exit fit mode;
    the toolbar shows the live fitted percentage while a fit mode is active.
  - Test: `interaction.rs::fit_zoom_keeps_the_page_in_view` — whole page plus
    view pads fits the viewport, is tight (12% larger overflows), same for
    fit width, and `set_zoom` unlocks. Fixed the known-issue wording: the
    editor template `RUSSIAN` is 54 rows (58 is `FINAL_DRAFT_A4`).
  - Verification: `cargo test --workspace` all green (app 13 unit + 14
    interaction + 11 snapshots; core 40 lib + 20 a95; layout 12 unit + 3
    parity (+1 ignored) + 1 pagination + 3 place + 1 pdf_export + 3 scale +
    2 relayout); `cargo fmt --check` OK; `cargo clippy --workspace
    --all-targets -- -D warnings` OK; `cargo deny check` all four OK.

- **2026-10-19** — Sprint item 6 done: **FDX import/export** with a sample
  fixture (user: "build anything left"). Unblocks item 7 (FDX was its
  prerequisite) and the Phase 1 exit ("open KIT or FDX, save KIT, deliver
  PDF/FDX/Fountain").
  - `scriptcraft-core/src/fdx.rs` — hand-rolled XML (quick-xml), same style as
    the other converters. Export: FDX 1 shape (`FinalDraft` → `Content` →
    `Paragraph`/`Text`), element-type map (SceneCharacters ↔ `Cast List`,
    NonPrintable/New Act/End of Act → `General`), scene numbers as the
    `Number` attribute (frozen labels when numbering is locked, else
    prefix/start — same rule as the editor/PDF), positional title page
    (title, genre, `Written by`, author, left-aligned contacts) and a default
    `<Revisions>` scaffold. Import: paragraphs, inline `ScriptNote`s become
    non-printing blocks, title-page heuristics (skip `Written by`-style
    lines), and FD's `Number` attributes are frozen into
    `locked_scene_numbers` so a re-export numbers scenes identically.
  - Fixture: `fixtures/example.fdx` (title page, 2 scenes, all element types,
    a script note, entities). Tests: 6 unit tests in `fdx.rs` + A95
    export→import round-trip in `tests/test_a95.rs` (block-exact, numbers
    1..21 preserved, second export byte-identical) + app interaction tests
    (`fdx_import_builds_a_project` via new `import_fdx_str` hook,
    `export_dialog_offers_fdx_not_docx`).
  - App: File ▸ Import Final Draft (in-app + native menu), Export dialog
    supports FDX (DOCX radio hidden per the Phase 0 promise — the enum variant
    and "not implemented" hint are gone), shared `adopt_imported` used by both
    importers (fresh KIT uuids on scene headings).
  - README corrected (FDX in features/layout/status; `scriptcraft-layout`
    listed) and the stale repo-map line fixed: `pdf.rs` belongs to
    `scriptcraft-layout`, not core.
  - Verification: `cargo test --workspace` all green (app 13 unit + 16
    interaction + 11 snapshots; core 46 lib + 21 a95; layout unchanged);
    snapshot `08_export_dialog` regenerated + visually checked (PDF/FDX/
    Fountain radios, no DOCX); `cargo fmt --check` OK; clippy `-D warnings`
    OK; `cargo deny check` all four OK.

- **2026-10-19** — Zoom feel + macOS chrome (user request: camera-style zoom
  and a smaller, app-colored titlebar).
  - Zoom is now continuous: removed the font-grid snapping (`render_zoom` /
    `snap_down_zoom` in `app.rs`), so 79% stays 79% instead of 19/24 ≈
    79.17%, and fit modes use the exact viewport fit (clamped 25–400%).
  - Camera-style anchoring: `script_view` remembers the last painted zoom,
    scroll offset and viewport; on any zoom change it adjusts the scroll
    offset so the page-space point under the anchor stays put — the pointer
    for pinch/⌘+wheel, the viewport center for toolbar/menu/fit resizes
    (`anchored_scroll_offset`, unit-tested).
  - Panning: the script area is a two-axis `ScrollArea` now (horizontal
    wheel/trackpad scrolling when the page overflows) plus middle-button drag
    panning (`pan_active` keeps a pan alive if the pointer leaves the view).
  - macOS window: the production window uses winit's fullsize content view
    with a transparent, title-less native titlebar
    (`with_fullsize_content_view`, `with_titlebar_shown(false)`,
    `with_title_shown(false)`), so the dark toolbar continues behind the
    native traffic lights (macOS owns their size/color). The toolbar gets a
    62 px traffic-light inset, a compact row (≈40 px bar instead of native
    28 + 38), and a top strip that drags the window (double-click =
    zoom/maximize). Tests keep the plain layout via `custom_titlebar`
    (default false), so snapshots are unchanged.
  - Tests: `script_view::tests::zoom_anchor_keeps_the_document_point_pinned`;
    `interaction.rs::zoom_is_continuous_not_font_grid_snapped`; tightened
    `fit_zoom_keeps_the_page_in_view` to the exact continuous fit.
  - Verification: `cargo test --workspace` all green (app 14 unit + 17
    interaction + 11 snapshots; core 46 lib + 21 a95; layout unchanged);
    `cargo fmt --check` OK; clippy `-D warnings` OK; `cargo deny check` all
    four OK. The native-titlebar visual could not be captured from the test
    environment (`screencapture` permission denied); its layout math is
    covered by the `custom_titlebar` path, and the production window was
    launched cleanly.

- **2026-10-19** — Follow-up bug from the user: "when I zoom out text still
  stretches to fill the space sideways." Root cause measured offscreen:
  epaint rounds the pen position to the pixel grid after every glyph and its
  advances are font/hinting dependent (Courier Prime 'M' ≈ 6.94 pt vs the
  7.2 pt cell at 12 pt; Plex Mono Cyrillic ≈ 7.38 pt per glyph), so whole
  lines breathe ±3–5% relative to the exactly scaled page as the zoom
  changes — at 79% Cyrillic ran ~5% wider than the layout column.
  - Fix: `script_view::line_galley` lays each line out as one section per
    character and pins every glyph to `i * cell` with `leading_space`
    (mirroring epaint's cursor rounding), so the painted block is always the
    same `n * cell` width as the wrap/caret/selection grid. The page can no
    longer look stretched relative to its text at any zoom.
  - Test: `script_view::tests::painted_lines_snap_every_glyph_to_the_cell_grid`
    (Latin/Cyrillic/mixed lines at 100/88/79/55/30/25%: every glyph within
    0.05 pt of its cell; line width within one glyph advance of the grid).
  - Snapshots 01/02/09/12 regenerated (text now fills the intended Courier
    column) and visually checked; selection highlight/caret line up with the
    glyphs.
  - Verification: `cargo test --workspace` all green (app 15 unit + 17
    interaction + 11 snapshots; core 46 lib + 21 a95; layout unchanged);
    `cargo fmt --check` OK; clippy `-D warnings` OK; `cargo deny check` OK.

- **2026-10-19** — Local install updated to **0.5.0**: release build of
  `scriptcraft-app` replaced `/Applications/ScriptCraft.app` (arm64, ad-hoc
  re-signed, `CFBundleShortVersionString`/`CFBundleVersion` = 0.5.0),
  launched cleanly. Version 0.5.0 covers what landed since the 0.4.0 zip:
  FDX import/export, fit page/width, continuous anchored zoom, custom
  macOS titlebar, and the glyph-grid fix.

- **2026-10-19** — The first CI run on GitHub exposed three cross-platform
  issues; all fixed:
  - **Windows save** failed every save/recovery test: `save_file` used
    `std::fs::rename` to swap the temp file over the project, which POSIX
    replaces but Windows refuses ("Access is denied"). New `replace_file`
    helper uses `MoveFileExW(MOVEFILE_REPLACE_EXISTING |
    MOVEFILE_WRITE_THROUGH)` on Windows (POSIX keeps `fs::rename`); the
    temp file is now fsynced through a write handle (`FlushFileBuffers`
    rejects read-only handles).
  - **Snapshot 07** embedded the machine's `$HOME` backups path. The
    harness now pins `/Users/test/Documents/ScriptCraft/backups` through
    the new `pin_backups_dir` hook; only 07 was regenerated.
  - **Linux** CI lacked the GTK3 headers `muda` needs. The `fmt + clippy`
    and Linux jobs now install `libgtk-3-dev libxkbcommon-dev
    libxcb-{render,shape,xfixes}0-dev`; the workflow triggers on `main`.
  - Verification: `cargo test --workspace` green locally; the Windows
    helper type-checks for `x86_64-pc-windows-msvc` (a full cross-build
    would need a Windows C toolchain for bundled SQLite).

---

## 7. Update protocol (for assistants)

When a user prompt arrives:

1. Read this file top to bottom; treat "Locked decisions" and "Non-negotiables"
   as binding.
2. Work on the highest-priority unchecked item unless the user says otherwise.
3. Update in the same commit/session:
   - tick `[x]` / set `[-]` markers as work lands;
   - append a dated entry under "Session log" summarizing what changed
     (files touched, tests run, results);
   - rewrite "Current sprint" checkboxes and "Next actions" if priorities moved;
   - add any new verified bug to "Known issues" with `file:line` references.
4. Never delete history. Never mark something done that was not verified by a
   command; note the exact verification command in the log.
5. Keep this file the only progress tracker so a new chat can resume from it alone.
