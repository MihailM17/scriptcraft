# Migrating from KIT Scenarist

ScriptCraft reads and writes KIT Scenarist `.kitsp` projects (KIT 0.7.2 schema)
and is designed as a local-first, revision-focused continuation for writers who
already have KIT libraries.

## Open your existing projects

1. **Menu ▸ Open** (or the Start page) and choose any `.kitsp` file.
2. ScriptCraft loads the script, title page, research, cards and metadata it
   understands. Unsupported or future `scenario_data` rows are preserved on
   save, so opening in ScriptCraft does not strip unknown KIT data.
3. If the file was written by another tool, ScriptCraft re-anchors stable
   block/scene IDs by KIT UUID and content; ambiguous external edits are
   reported instead of guessed.

## What carries over

- **Script text and structure:** scene headings, action, character, dialogue,
  parenthetical, transition, lyrics/notes as supported by the script model.
- **Title page / metadata:** title, author/credit, source, contacts, draft
  date, header/footer and related `scenario_data` fields.
- **Research and cards:** KIT research entries and corkboard cards.
- **KIT versions:** real `script_versions` rows can be imported as revision
  baselines (Research ▸ Versions / revision tools).

## ScriptCraft additions (stored in `scenario_data`)

ScriptCraft keeps its own data in versioned sidecar keys that KIT ignores:

- `scriptcraft_ids` / `scriptcraft_scene_meta` — stable block and scene identity.
- `scriptcraft_revisions` — revision baselines, colours, page counts and marks.
- `scriptcraft_locked_pages`, `locked_scene_numbers` — production page/scene locks.
- `scriptcraft_dual_groups` — dual-dialogue pairing.
- `scriptcraft_recovery_origin` — crash-recovery origin path.

Revision summaries (counts/CSV/printable PDF) are computed from revision diffs;
they are not a separate persisted format.

## Revisions workflow

1. Start a revision from **Research ▸ Versions** or the Script/Versions menu.
2. Edits are marked with the current revision colour; compare drafts, restore a
   baseline as a copy, or open **Revision Summary** for scene-grouped counts,
   CSV export and a printable PDF.
3. Page locks and scene-number locks keep issued production pages stable
   (inserted pages become `42A`, `42B`, …).

## Interchange

- **Final Draft (.fdx):** import/export supported; revision metadata, dual
  dialogue and title-page fields round-trip where mapped. A conversion should be
  checked for loss on complex documents.
- **Fountain:** import/export supported; screenplay-aware paste helps when
  bringing text from other editors.
- **PDF:** export uses the same layout engine as the editor (pagination parity).

## Tips

- Keep backups of important `.kitsp` files before large migrations.
- Save regularly; ScriptCraft uses atomic saves and recovery copies.
- If you rely on a KIT feature not yet implemented in ScriptCraft, keep KIT
  available for that specific task and verify round-trips on a copy.
