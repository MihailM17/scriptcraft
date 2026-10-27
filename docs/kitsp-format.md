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
  (`element_type`, `text`, optional scene-heading `kit_uuid`). Missing or
  unreadable JSON is treated as no revisions; it never blocks opening the
  script. Saved KIT `script_versions` rows can be imported as baselines;
  imports are idempotent and capped at 24 revisions.
- `locked_scene_numbers` — comma-separated frozen scene numbers; a leading
  `*` also locks numbers for newly inserted scenes.
- `scriptcraft_locked_pages` — JSON page lock: original page count plus one
  anchor per page start (`element_type`, text/paragraph, optional `kit_uuid`,
  and neighbouring-line context). Layout forces the starts and labels inserted
  pages `42A`, `42B`, …. KIT ignores this row.
- `scriptcraft_recovery_origin` — original project path inside a crash
  recovery copy.
