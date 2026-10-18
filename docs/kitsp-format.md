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
  year, logline, synopsis, header/footer…`.
- `scenario_changes` — undo history. `script_versions` — named snapshots.
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
