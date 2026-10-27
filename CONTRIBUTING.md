# Contributing to ScriptCraft

ScriptCraft is a free, open-source, local-first screenwriting studio. The
current development focus is the revision workflow; read
[`docs/v0.8-plan.md`](docs/v0.8-plan.md) before starting a large change.

## Before you start

- For a bug, open an issue with exact steps, the OS, the version, and a minimal
  project file if the bug involves loading or saving.
- For a feature, open an issue first. v0.8 has explicit non-goals; a good patch
  that is out of scope still has to wait.
- For a small fix, a pull request is fine without a prior issue.

## Development setup

Requirements:

- Rust stable (the workspace uses edition 2021).
- On Linux: GTK3 development libraries for the desktop app.
- On macOS: Xcode command-line tools.
- On Windows: the MSVC Rust toolchain.

Run the app:

```sh
cargo run -p scriptcraft-app
```

Run the full checks:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
```

Update UI snapshots only when the visual change is intentional:

```sh
UPDATE_SNAPSHOTS=1 cargo test -p scriptcraft-app --test ui_snapshots
```

## Architecture boundaries

- `scriptcraft-core/` — document model, `.kitsp`/FDX/Fountain conversion,
  revisions, page locks, editing commands. No UI dependencies.
- `scriptcraft-layout/` — wrapping, pagination and placement. The editor and
  the PDF renderer must use this crate; do not add a second paginator.
- `scriptcraft-app/` — egui/eframe UI, state, IO and platform integration.

Rules that keep the core testable:

- Do not put egui types in `scriptcraft-core` or `scriptcraft-layout`.
- Do not compute layout in the app. If the editor needs a new placement rule,
  add it to `scriptcraft-layout` and test it there.
- Do not change `.kitsp` tables. ScriptCraft-only data goes in versioned
  `scenario_data` rows that KIT ignores.
- Keep editor and PDF output in sync. A layout change needs a parity test.

## Tests

- Unit tests live next to the code or in `tests/` in the owning crate.
- Layout changes need parity tests against the editor and the PDF renderer.
- UI changes need an interaction test or a snapshot, not only a manual check.
- Performance-sensitive changes need a check against the 500-page fixtures in
  `scriptcraft-layout/tests/scale.rs`.
- Do not weaken a test to make a change pass. Fix the change or explain the
  behaviour change in the pull request.

## Fixtures

Only add files we have the right to redistribute.

- Every real-world `.kitsp`, `.fdx`, `.fountain` or PDF needs a provenance note:
  where it came from, who gave permission, and the date.
- No copyrighted screenplays or production drafts without written permission.
- Prefer small, synthetic fixtures for unit tests. Keep large real scripts in
  the compatibility corpus only.
- Never commit personal contact details, addresses or unpublished production
  data from a real project.

## Pull requests

- Keep the change focused on one milestone or bug.
- Include tests for new behaviour and a note about what you could not test.
- Update `README.md`, `ROADMAP.md` or `docs/` when behaviour or scope changes.
- Do not mix a refactor with a feature in the same commit. Refactors must not
  change snapshots.
- Run `cargo fmt --check`, clippy and the workspace tests before opening the
  pull request.
- Describe the user-visible result, not only the implementation.

## Commit messages

Use the imperative mood and a short scope:

```text
revision: compare Blue against current draft
layout: place dual dialogue in two columns
kitsp: preserve unknown scenario_data rows
```

## Releases

Releases are cut from `main` with a version tag. The release owner runs the
gate in `docs/v0.8-plan.md`, builds the packages, publishes SHA-256 files and
writes the release notes. macOS builds must be signed and notarised before they
are described as signed; Windows and Linux builds must pass the launch smoke
test before they are described as tested.

## Questions

Open a discussion or an issue. For security-sensitive reports, do not paste
private project files into a public issue; describe the problem first and wait
for a private channel.
