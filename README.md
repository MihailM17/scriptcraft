# ScriptCraft

A 100% Rust screenwriting studio built with `egui` and `eframe`, inspired by
the workflow and typography of KIT Scenarist. Clean-room implementation, no
KIT code.

![ScriptCraft editor](scriptcraft-app/tests/snapshots/01_script_top.png)

## Features

- Fixed A4 page editor with Courier Prime, page-accurate pagination, scene
  numbers, `(MORE)` / `(CONT'D)` markers and zoom that scales pages without
  reflowing text.
- Autocomplete for scene headings, characters, locations and transitions.
- Undo/redo, find/replace, typewriter scrolling, multi-document support and
  autosave.
- Script, Research, Cards, Statistics and Settings modules.
- Dark and light themes with the ScriptCraft icon-blue accent.
- Native macOS menu bar plus the KIT-style in-app menu (Windows and
  Linux use the in-app menu).
- Fountain and Final Draft `.fdx` import/export, plus template-matched PDF
  export (title page, scene numbers, letters).
- Reads and writes real KIT `.kitsp` projects (SQLite + XML).

## Layout

- `scriptcraft-core/` — UI-free model, `.kitsp` reader/writer, Fountain and
  FDX converters, format validation and coaching checks.
- `scriptcraft-layout/` — the shared wrap/pagination/placement engine that
  drives both the editor pages and the PDF renderer.
- `scriptcraft-app/` — `eframe`/`egui` desktop app, theme, native menu and
  modules.
- `fixtures/A95.kitsp` — 21-scene Bulgarian test project used by the tests;
  `fixtures/example.fdx` — small Final Draft sample for the FDX tests.
- `docs/` — reverse-engineered KIT file/design notes.

## Build and run

```sh
cargo run -p scriptcraft-app
```

To build the optimized binary:

```sh
cargo build --release -p scriptcraft-app
```

Prebuilt packages are attached to each GitHub release:

- macOS (Apple silicon): `ScriptCraft-<version>-macos-arm64.dmg`, ad-hoc signed
  — right-click ▸ Open on first launch.
- Windows 10/11 (x64): `ScriptCraft-<version>-windows-x64.zip` — unzip and run
  `ScriptCraft.exe`.
- Linux (x86_64): `ScriptCraft-<version>-linux-x86_64.tar.gz` — needs GTK3
  (`sudo apt install libgtk-3-0` on Debian/Ubuntu); extract and run
  `./scriptcraft`. The archive also contains a `.desktop` file and icon.

## Tests

```sh
cargo test --workspace
cargo fmt --check
```

Snapshot tests:

```sh
UPDATE_SNAPSHOTS=1 cargo test -p scriptcraft-app --test ui_snapshots
```

## Status

The editor, project format, export pipeline and all five workspace modules are
implemented. The macOS, Windows and Linux desktop builds share the same UI and
project format; Windows/Linux packages are unsigned. Planned next: DOCX export
and deeper production/revision tooling.
