# Format spikes

This file records the exact on-disk shapes we need for v0.8 features. Do not
guess a format from documentation alone: create a minimal file in the real
application, inspect it, and record what the app actually writes.

## Rules

- Use only files we have the right to inspect and redistribute.
- Record the application and version used to create the sample.
- Keep the smallest possible fixture: one scene, one dialogue pair, one omitted
  scene, one revision.
- Add the fixture to `fixtures/real/` only with a provenance note.
- If a format cannot represent a feature, write that down. A documented loss is
  better than a silent one.

## Spike 1 — Omitted scenes

Questions:

- How does Final Draft write an omitted scene in `.fdx`? Which element or
  attribute carries the state?
- How does KIT Scenarist represent an omitted scene in `.kitsp`? Is it a scene
  heading rewrite, a `scenario_data` row, or nothing at all?
- Does an omitted scene keep its number, its page starts and its revision
  colour on export?
- What happens to the body text: retained but hidden, or removed?

Sample: `fixtures/real/omitted-scene.fdx` + `fixtures/real/omitted-scene.kitsp`.

Findings:

- FDX: _to fill_
- KIT: _to fill_
- Decision for ScriptCraft: _to fill_

## Spike 2 — Dual dialogue

Questions:

- How does Final Draft group two consecutive dialogues into a dual pair in
  `.fdx`? Which wrapper, attribute or paragraph property?
- How does KIT store the pairing in `.kitsp`?
- What happens to `(CONT'D)`, parentheticals and page breaks inside a pair?
- Does the pairing survive a `.fdx` round-trip through Final Draft?

Sample: `fixtures/real/dual-dialogue.fdx` + `fixtures/real/dual-dialogue.kitsp`.

Findings:

- FDX: _to fill_
- KIT: _to fill_
- Decision for ScriptCraft: _to fill_

## Spike 3 — FDX revision metadata

Questions:

- Which revision fields does Final Draft write per paragraph and per document?
- Can revision IDs be matched to ScriptCraft revision sets without prior text?
- What does Final Draft do with revision colours on import/export?

Sample: `fixtures/real/revisions-blue-pink.fdx`.

Findings:

- FDX: _to fill_
- What can be preserved: _to fill_
- What must be reported as lost: _to fill_

## Spike 4 — Title page

Questions:

- Which title-page fields survive `.kitsp` and `.fdx` round-trips?
- How are Revised fields represented in each format?
- What does KIT do with unknown title-page HTML?

Sample: `fixtures/real/title-page.fdx` + `fixtures/real/title-page.kitsp`.

Findings:

- FDX: _to fill_
- KIT: _to fill_
- Decision: _to fill_
