# `.kitsp` format (reverse-engineered from A95.kitsp, KITScenarist 0.7.2 rc15)

SQLite 3.x, ~1.1 MB. Relevant tables:

- `scenario(id, scheme, text)` — `id=1` is the script.
  `text` = `<?xml?><scenario>` with ordered children:
  `<scene_heading uuid="…"><v><![CDATA[…]]></v></scene_heading>`,
  `<action>`, `<character>`, `<dialog>`, `<parenthetical>`, `<transition>`
  (same `<v><![CDATA[…]]></v>` body).
- `scenario.scheme` = `<cards><card title="SCENE" description="…">` corkboard.
- `research(id, parent_id, type, name, …)` — `type 101` = character
  (`ИВАН, МАРИЯ, ГЕОРГИ, БАЩАТА, ПОЛИЦАЙ, ЧИЧО ЛЮБО`),
  `type 201` = location.
- `scenario_data(data_name, data_value)` — `name, genre, author, contacts,
  year, logline, synopsis, header/footer…`. ScriptCraft prints `header` and
  `footer` in PDF margins and expands `{revision}`, `{date}` and `{page}`.
- `scenario_changes` — undo history. `script_versions` — named snapshots;
  `script_text` is a complete `<scenario>` document, so ScriptCraft can import
  non-empty snapshots as revision baselines.
- `system_variables` (`application-version`…), `places, times,
  character_states (Voice Over/Off Screen/Off Camera/SUBTITLE),
  transitions (CUT TO:/FADE IN:/FADE OUT:/…/FADE TO BLACK)`.

## Normalization rules (validator)

- Headings: `EXT./INT./INT./EXT.` uppercase, time in
  `MORNING/DAY/NIGHT/MOMENTS LATER/CONTINUOUS` (no `morNING`, `НОЩ`,
  `the fight`, trailing `.`).
- Characters trimmed/uppercased; shot/SFX/montage labels
  (`БЛИЗЪК ПЛАН…`, `БУУУМ!`, `А)…`) are Action, never Character.
- `(ПРОД.)` = character extension, not parenthetical; consecutive
  parentheticals merged (`(О.К., през телефона, шепнешком)`).
- No literal `&quot;` (use `“`), no em-dash `—` (use `–`), no orphan
  `<dialog>` without character/parenthetical cue.

## ScriptCraft extension rows

ScriptCraft stores optional data in `scenario_data` using unique keys. KIT
ignores unknown rows, and ScriptCraft's normal save path preserves them.

- `scriptcraft_revisions` — JSON revision history: colour, display name,
  start timestamp, page count, and a block baseline for each revision
  (`element_type`, `text`, optional scene-heading `kit_uuid`, optional stable
  `block_id`). Older baselines without `block_id` still load and compare by
  content. Missing or unreadable JSON is treated as no revisions; it never
  blocks opening the script. Saved KIT `script_versions` rows can be imported
  as baselines; imports are idempotent and capped at 24 revisions.
  Restoring a baseline as a copy writes a new file and leaves the source
  untouched. The copy keeps the current title page, metadata, scene-number
  locks, page locks and other KIT tables, replaces the script and identity
  rows with the baseline, and truncates revision history after the restored
  set. Baselines do not store title-page data yet, so the restore reports
  that the current metadata was kept.
- `scriptcraft_ids` — schema-versioned JSON identity map, one record per
  screenplay block in `scenario` order (`id`, `element_type`, save-time `text`,
  optional scene-heading `kit_uuid`). This is what makes `ScriptBlock.id` and
  scene identity survive save/reopen and external KIT edits. See below.
- `scriptcraft_scene_meta` — schema-versioned JSON per-scene metadata keyed by
  the heading block's stable id. Currently stores `omitted` (v0.8 M4), the
  canonical ScriptCraft omission state. Entries for scenes that no longer exist
  are ignored with a warning.
- `locked_scene_numbers` — comma-separated frozen scene numbers; a leading
  `*` also locks numbers for newly inserted scenes.
- `scriptcraft_locked_pages` — JSON page lock: original page count plus one
  anchor per page start (`element_type`, text/paragraph, optional `kit_uuid`,
  and neighbouring-line context). Layout forces the starts and labels inserted
  pages `42A`, `42B`, …. KIT ignores this row.
- `scriptcraft_recovery_origin` — original project path inside a crash
  recovery copy.

### Omitted scenes (sidecar-only)

KIT Scenarist 0.7.2 has no native omitted-scene representation (source audit
recorded in `docs/format-spikes.md`). Omission is therefore ScriptCraft metadata
and never rewrites the KIT screenplay:

```json
{
  "schema": 1,
  "scenes": [{ "id": "<heading block id>", "omitted": true }]
}
```

While a scene is omitted:

- The full body stays in `scenario` unchanged. ScriptCraft keeps the heading,
  scene number and revision colour and replaces the body with one `OMITTED` row
  in the editor, print layout and PDF. Body text is excluded from statistics and
  page counts but is not deleted.
- Page-lock anchors inside the omitted body disappear with its pages. The last
  hidden page label is carried to the first visible block after the omission, so
  later headings keep their production page numbers.
- KIT shows the full body because it ignores the sidecar row. FDX export reports
  the omission as unsupported instead of inventing an attribute; Fountain export
  warns that omission cannot be represented. Adding an FDX mapping requires a
  real Final Draft fixture first.

## Stable identity and external edits

`scriptcraft_ids` and `scriptcraft_scene_meta` both carry a top-level
`"schema": 1`. A row looks like:

```json
{
  "schema": 1,
  "blocks": [
    {
      "id": "5f3c3c1e-7f5f-4a1d-9f17-0f8b2f7a6d10",
      "element_type": "scene_heading",
      "text": "INT. ROOM - DAY",
      "kit_uuid": "{b0e1...}"
    }
  ]
}
```

Rules:

- The normal save path rewrites both rows in the same atomic file replace as
  `scenario`. Existing KIT tables and unrelated `scenario_data` rows are copied
  through untouched.
- On load, IDs are matched by KIT scene UUID first, then by exact element type
  and text, then by equal-length same-element runs between those anchors. This
  preserves identity for inserted, deleted, reordered and text-edited blocks.
- A block ID is never assigned to two blocks. Blocks that cannot be matched get
  a fresh UUID; the app shows a warning when an external edit was ambiguous
  instead of silently reusing an ID for different content.
- Scene identity is the scene heading's `ScriptBlock.id`. Renaming a scene
  keeps its id; deleting the heading retires the id and drops its metadata with
  a warning.
- A project without these rows opens unchanged. It receives fresh IDs and they
  are persisted on the next save.
- A row with `"schema"` newer than the build is not interpreted and is **not
  overwritten** on save. ScriptCraft warns and leaves it for the newer build.
- The identity row stores a second copy of the script text, roughly doubling
  the sidecar size. This is deliberate: re-anchoring needs the pre-edit text,
  and KIT ignores the row.
