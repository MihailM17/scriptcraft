# ScriptCraft design language (after KIT Scenarist 0.7.2)

Source: `github.com/dimkanovikov/KITScenarist` (+ `KITScenaristCore`
submodule). Studied for look-and-feel tokens only; all code is clean-room.

## Theme: Qt Fusion, dark

From `UIConfigurator` dark-palette defaults:

| Role | Hex |
|---|---|
| Window / panels | `#26282a` |
| Draft desk (recessed well) | `#1b1c1e` |
| Page face | `#404040` (KIT dark Base) |
| Module rail | `#1e2022`, active fill `#26282a` + 3px `#2b78da` bar |
| Window text | `#ebebeb` |
| Buttons | `#414244`, hover `#4c4e51` |
| Input base | `#333537` (KIT `#404040`, deepened one step for contrast) |
| Alt background | `#2e3033` |
| Accent / selection | `#2b78da`, selected text `#ffffff` |
| Disabled text | `#a1a1a1` |
| Window border | `#1c2023`, separators `#3a3d40` |

Corners: 2px windows/menus, 3px widgets (Fusion is near-square).
Mapped onto `egui::Visuals` in `kit_visuals()`.

## Fonts (bundled, `scriptcraft-app/assets/fonts/`)

KIT loads Roboto + Courier New + Courier Prime at startup
(`Application.cpp`). We mirror the look with OFL faces only — Courier New is
proprietary and is not redistributable:

| Use | Face | Notes |
|---|---|---|
| UI (Roboto-spirit) | PT Sans Regular/Bold | humanist, Latin + Cyrillic (Roboto itself ships variable-only now) |
| Script body | Courier Prime Regular | OFL; 0.6em advance, Latin + Cyrillic |
| Script bold | Courier Prime Bold | headings, cues, transitions |
| Script italic | Courier Prime Italic | lyrics labels |
| Script safety net | IBM Plex Mono Regular/Bold | same 0.6em advance, alignment preserved |

License texts: `scriptcraft-app/assets/fonts/licenses/`.

## Screenplay metrics (KIT `russian.kitss` template)

Page A4, margins left 37.5 / top 25 / right 25 / bottom 12.5 mm
→ text width 147.5mm. Courier Prime 12, uppercase headings/characters/transitions.

| Block | Left margin | Right | Top space |
|---|---|---|---|
| scene_heading | 0 | 0 | 1 line |
| action | 0 | 0 | 1 line |
| character | 67.5mm (45.8%) | 0 | 1 line |
| parenthetical | 55mm (37.3%) | 45mm | 0 |
| dialog | 37.5mm (25.4%) | 37.5mm | 0 |
| transition | right-aligned | 0 | 1 line |

Implemented as width fractions (`INDENT_*` in `main.rs`) so zoom stays exact.

## Row height: standard 1/6 inch

The line grid is a fixed screenplay pitch, not a font metric: `ROW_MM =
25.4 / 6 = 4.2333 mm`, the standard 12 pt leading used by KIT Scenarist and
Final Draft. The bundled font's hhea height (Courier Prime: 4.7542 mm) is a
font-design value and made the same script paginate about 7% longer than KIT.
Page geometry uses `ROW_MM` directly, so rendered pages hold exactly what the
paginator promises. (Before v0.8.8 a build script derived the pitch from the
bundled TTF; that build script is removed.)

## Fixed A4 pages (`editing::paginate`, 58 lines/page)

A4 is 297 mm tall; KIT's top and bottom margins are 25 mm each, leaving 247 mm.
floor(247 / 4.2333) = 58 rows. Pagination counts rows, and pixel geometry is
built from the same pitch (including backwards from 58 chars/line), so render
wrap points equal pagination wrap points at every zoom: zoom scales pages,
never reflows them (Word-style). Headings and character cues are never
stranded last on a page. Unit-tested, plus an A95 assertion (~20–35 pages,
anchors safe).

## Chronometry (calibrated on A95 vs KIT's own scene times)

Least-squares fit of KIT's per-scene durations: `0.22s` per action-ish
word + `0.27s` per dialogue-ish word. Predicts 17:55 total vs KIT's
17:54, mean scene error 5.7s. Shown per scene, in the toolbar, in Stats.

## Chrome (from `ApplicationManager::createMenu` + reference screenshots)

- Single full-width toolbar: Menu popup (File / Edit / View / Help
  sections), `Scenes: N`, +/− scene, undo/redo, element combo in FastFormat
  order, `Duration: cur | total` + status on the right. No zoom slider
  (zoom lives in the View section).
- Far-left 68px module rail (`#1E2022`): Script, Research, Cards,
  Statistics, Tools, Settings — active cell filled with a 3px `#2b78da`
  left bar. Each module owns the full center stage (KIT workspaces);
  navigator + format bar appear only in the Script view.
- Scene navigator: edge-to-edge 50px two-line rows, full-row `#2b78da`
  selection, duration pinned right, 1px dividers.
- Right fast-format list 0–9: flat left-aligned boxes, active row inverted
  light (`#DCDDDD` + dark ink), ↑ Prev / ↓ Next navigation, autocomplete
  suggestions; no insert/delete (Enter/Backspace own that).
- Draft canvas: recessed desk (`#1B1C1E`), fixed A4 pages (`#404040`) with
  template margins, right-aligned page-number headers, hanging scene
  numbers painted in the left margin (headings flush with action text).
- Floating autocomplete popup anchored to the focused editor (KIT-style
  dropdown), prefix = line text before the caret; click applies.
- OS window title follows KIT: `Title — ScriptCraft •` while dirty.
- Corkboard `scheme` is synced on every save (titles updated, new scenes
  appended with layout attrs preserved) so KIT never loses cards.
