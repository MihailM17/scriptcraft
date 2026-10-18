//! End-to-end interaction tests (kittest): simulated clicks and keystrokes.

use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use scriptcraft_app::app::{self, ScriptCraftApp, SideTab};

fn harness(tab: SideTab) -> Harness<'static, ScriptCraftApp> {
    Harness::builder()
        .with_size(egui::Vec2::new(1360.0, 880.0))
        .build_eframe(move |cc| {
            app::apply_theme(&cc.egui_ctx);
            app::install_fonts(&cc.egui_ctx);
            let mut a = ScriptCraftApp::default();
            assert!(a.open_fixture(), "A95 fixture not found");
            a.goto_tab(tab);
            a
        })
}

/// Focus the first character cue at offset 0 and type its first two chars.
fn type_cue_prefix(h: &mut Harness<'static, ScriptCraftApp>) -> (usize, String, String) {
    let bi = h
        .state()
        .find_first_cue_block()
        .expect("A95 should contain at least one character cue");
    let full = h.state().block_text(bi).unwrap();
    assert!(full.chars().count() >= 3, "cue too short to build a prefix");
    let prefix: String = full.chars().take(2).collect();

    h.state_mut().focus_block_at(bi, 0);
    h.run_steps(2);

    h.input_mut().events.push(egui::Event::Text(prefix.clone()));
    h.run();

    (bi, full, prefix)
}

/// Regression: clicking a suggestion in the autocomplete dropdown must
/// complete the token. This used to fail because the popup was only rendered
/// while the TextEdit had focus, so the click's release landed on a popup that
/// had already vanished when the press moved focus to the popup button.
#[test]
fn autocomplete_click_fills_suggestion() {
    let mut h = harness(SideTab::Script);
    h.run();

    let (bi, full, _prefix) = type_cue_prefix(&mut h);

    let node = h.get_by_label(&full);
    node.click();
    h.run();

    let after = h.state().block_text(bi).unwrap_or_default();
    assert_eq!(
        after,
        format!("{full}{full}"),
        "clicking the suggestion should replace the typed prefix with the full cue"
    );
}

/// The fixed row grid must keep every chunk on its page at any zoom. Text
/// used to drift downward (per-chunk `ui.horizontal` offset + TextEdit margin
/// + row-height rounding) and leak into the gaps between pages.
#[test]
fn rows_stay_on_page_at_all_zooms() {
    let mut h = harness(SideTab::Script);
    for z in [0.5_f32, 1.0, 1.5, 2.0, 3.0] {
        h.state_mut().set_zoom(z);
        h.run();
        h.run();
        let bounds = h.state().debug_row_bounds.clone();
        assert!(!bounds.is_empty(), "editor should render rows at zoom {z}");
        for (pi, row, page) in bounds {
            assert!(
                row.top() >= page.top() - 1.0
                    && row.bottom() <= page.bottom() - 1.0
                    && row.left() >= page.left() - 1.0
                    && row.right() <= page.right() - 1.0,
                "chunk on page {pi} escaped the paper at zoom {z}: row={row:?} page={page:?}"
            );
        }
    }
}

/// Escape closes the dropdown without changing the block.
#[test]
fn autocomplete_escape_keeps_text() {
    let mut h = harness(SideTab::Script);
    h.run();

    let (bi, full, prefix) = type_cue_prefix(&mut h);
    assert!(
        h.query_by_label(&full).is_some(),
        "dropdown should be open after typing the prefix"
    );

    h.key_press(egui::Key::Escape);
    h.run();
    assert!(
        h.query_by_label(&full).is_none(),
        "dropdown should close on Escape"
    );
    assert_eq!(
        h.state().block_text(bi).unwrap_or_default(),
        format!("{prefix}{full}"),
        "Escape must not modify the block"
    );
}

/// Focusing a block (e.g. clicking a scene in the navigator puts the caret
/// at the end of its heading) must not open the dropdown. It may only open
/// as a result of actual typing.
#[test]
fn focusing_block_does_not_open_autocomplete() {
    let mut h = harness(SideTab::Script);
    h.run();

    let bi = h
        .state()
        .find_first_cue_block()
        .expect("A95 should contain at least one character cue");
    let full = h.state().block_text(bi).unwrap();

    h.state_mut().focus_block_at(bi, full.chars().count());
    h.run_steps(2);

    assert!(
        h.query_by_label(&full).is_none(),
        "dropdown must stay closed when a block is merely focused"
    );
    assert_eq!(h.state().block_text(bi).as_deref(), Some(full.as_str()));
}

/// Picking a scene-heading suggestion must replace the typed line, not
/// splice a full line into it. Typing "EXT. П" and picking
/// "EXT. ПАРКИНГ…" used to leave duplicated cues/letters in the heading.
#[test]
fn scene_heading_pick_replaces_the_line() {
    let mut h = harness(SideTab::Script);
    h.run();

    let bi = h
        .state()
        .find_first_heading_block()
        .expect("A95 should contain at least one scene heading");
    h.state_mut().focus_block_at(bi, 0);
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run();
    h.input_mut()
        .events
        .push(egui::Event::Text("EXT. П".to_string()));
    h.run();

    // A location from another scene remains in the known-location list
    // even after this heading was cleared.
    let label = "EXT. ПРЕД ВХОДА НА БЛОКА";
    h.get_by_label(label).click();
    h.run();

    assert_eq!(
        h.state().block_text(bi).as_deref(),
        Some(label),
        "picking a heading suggestion must replace the whole typed line"
    );
}

/// The Menu entry opens the KIT-style Export dialog; Cancel closes it.
/// (Widgets inside a floating Window don't receive simulated pointer
/// clicks in kittest, so the dialog buttons use the accesskit action.)
#[test]
fn export_dialog_opens_and_cancels() {
    let mut h = harness(SideTab::Script);
    h.run();

    h.get_by_label("Menu").click();
    h.run();
    h.get_by_label("Export…").click();
    h.run();
    assert!(
        h.query_by_label("Export…").is_none(),
        "menu popup should close after choosing Export…"
    );
    assert!(
        h.query_by_label("Cancel").is_some(),
        "Export dialog should be open after Menu ▸ Export…"
    );

    h.get_by_label("Cancel").click_accesskit();
    h.run();
    h.run();
    assert!(
        h.query_by_label("Cancel").is_none(),
        "Cancel should close the Export dialog"
    );
}

/// Settings ▸ Use Dark Theme switches the whole palette to the light theme.
#[test]
fn theme_toggle_switches_palette() {
    let mut h = harness(SideTab::Settings);
    h.run();
    assert!(h.ctx.style().visuals.dark_mode, "app starts in dark theme");

    h.get_by_label("Use Dark Theme").click();
    h.run();
    h.run();

    let v = &h.ctx.style().visuals;
    assert!(
        !v.dark_mode,
        "unchecking Use Dark Theme should switch egui Visuals to light"
    );
    assert_eq!(
        v.panel_fill,
        egui::Color32::from_rgb(0xF8, 0xF9, 0xFB),
        "panel fill must come from the KIT light palette"
    );
}

/// The KIT-style popup rows are painted, but carry accesskit labels: opening
/// Menu → Help must show the Help window and close the popup.
#[test]
fn menu_popup_opens_help_and_closes() {
    let mut h = harness(SideTab::Script);
    h.run();
    h.get_by_label("Menu").click();
    h.run();
    h.get_by_label("Help").click();
    h.run();
    assert!(
        h.query_by_label("Keep writing!").is_some(),
        "Help window should open from the menu row"
    );
}
