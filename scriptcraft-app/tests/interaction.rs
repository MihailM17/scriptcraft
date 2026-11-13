//! End-to-end interaction tests (kittest): simulated clicks and keystrokes.

use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::Harness;
use scriptcraft_app::app::{self, ResearchPane, ScriptCraftApp, SideTab};

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

/// Show Research ▸ Versions, where KIT-style revisions now live.
fn open_research_versions(h: &mut Harness<'static, ScriptCraftApp>) {
    h.state_mut().goto_tab(SideTab::Research);
    h.state_mut().set_research_pane(ResearchPane::Versions);
    h.run();
}

/// Click into the first page so the script widget owns keyboard focus.
fn click_editor2_page(h: &mut Harness<'static, ScriptCraftApp>) {
    let pages = h.state().editor2_page_rects();
    assert!(!pages.is_empty(), "the editor must paint at least one page");
    let page = pages[0];
    let pos = egui::pos2(page.left() + 120.0, page.top() + 80.0);
    h.input_mut().events.push(egui::Event::PointerMoved(pos));
    h.input_mut().events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.run();
    h.input_mut().events.push(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run();
}

/// A harness with the editor focused (caret on the first page).
fn focused_harness() -> Harness<'static, ScriptCraftApp> {
    let mut h = harness(SideTab::Script);
    h.run();
    click_editor2_page(&mut h);
    h
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
/// while the editor had focus, so the click's release landed on a popup that
/// had already vanished when the press moved focus elsewhere.
#[test]
fn autocomplete_click_fills_suggestion() {
    let mut h = focused_harness();

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

/// The painted paper must keep A4 geometry at every zoom: height/width ratio
/// constant, height scaling linearly with the zoom factor.
#[test]
fn paper_geometry_scales_with_zoom() {
    let mut h = harness(SideTab::Script);
    h.run();
    let mut unit_height = None;
    for z in [0.5_f32, 1.0, 1.5, 2.0, 3.0] {
        h.state_mut().set_zoom(z);
        h.run();
        h.run();
        let pages = h.state().editor2_page_rects();
        assert!(!pages.is_empty(), "editor should paint paper at zoom {z}");
        let first = pages
            .iter()
            .min_by(|a, b| a.top().partial_cmp(&b.top()).unwrap())
            .unwrap();
        let ratio = first.height() / first.width();
        assert!(
            (ratio - 841.890 / 595.276).abs() < 0.02,
            "paper must stay A4-shaped at zoom {z}: {first:?}"
        );
        match unit_height {
            None => unit_height = Some(first.height() / z),
            Some(unit) => assert!(
                (first.height() / z - unit).abs() < 1.0,
                "paper height must scale linearly at zoom {z}: {first:?}"
            ),
        }
    }
}

/// Escape closes the dropdown without changing the block.
#[test]
fn autocomplete_escape_keeps_text() {
    let mut h = focused_harness();

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
    let mut h = focused_harness();

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
/// "EXT. ПРЕД ВХОДА НА БЛОКА" used to leave duplicated cues/letters in the
/// heading.
#[test]
fn scene_heading_pick_replaces_the_line() {
    let mut h = focused_harness();

    let bi = h
        .state()
        .find_first_heading_block()
        .expect("A95 should contain at least one scene heading");
    let len = h.state().block_text(bi).unwrap().chars().count();
    h.state_mut()
        .editor2_set_selection((bi, 0, 0), (bi, 0, len));
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

/// The crash-recovery banner: a leftover unsaved copy can be restored (the
/// session comes back dirty and keeps its original path) or discarded.
#[test]
fn recovery_bar_restores_and_discards() {
    let dir = std::env::temp_dir().join("scriptcraft_test_recovery_ui");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/A95.kitsp");

    let mut h = harness(SideTab::Script);
    h.run();

    // A never-saved copy: Restore must bring it back without a path.
    let rec = dir.join("rec.kitsp");
    std::fs::copy(&src, &rec).unwrap();
    h.state_mut()
        .pin_recovery_for_test(vec![(rec.clone(), "untitled script".to_string())]);
    h.run();
    assert!(
        h.query_by_label("Restore").is_some(),
        "recovery banner should be visible"
    );
    h.get_by_label("Restore").click();
    h.run();
    assert!(!rec.exists(), "Restore must consume the recovery file");
    assert!(h.state().is_dirty(), "restored session must be dirty");
    assert!(
        h.state().project_path().is_empty(),
        "no origin on disk → Save As is required"
    );
    assert!(
        h.query_by_label("Restore").is_none(),
        "banner must clear after restoring"
    );

    // A second copy can be thrown away.
    let rec2 = dir.join("rec2.kitsp");
    std::fs::copy(&src, &rec2).unwrap();
    h.state_mut()
        .pin_recovery_for_test(vec![(rec2.clone(), "untitled script".to_string())]);
    h.run();
    h.get_by_label("Discard").click();
    h.run();
    assert!(!rec2.exists(), "Discard must delete the recovery file");
    assert!(
        h.query_by_label("Discard").is_none(),
        "banner must clear after discarding"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Settings pages apply their choices to the editor and navigator.
#[test]
fn settings_pages_apply_navigator_and_typography_options() {
    use scriptcraft_core::ElementType;

    let mut h = harness(SideTab::Settings);
    h.run();
    h.get_by_label("Scenario Editor").click();
    h.run();
    assert!(
        h.query_by_label("Smart quotes").is_some(),
        "the Scenario Editor page must expose its typing options"
    );
    h.get_by_label("Smart quotes").click();
    h.run();

    h.get_by_label("Navigator").click();
    h.run();
    h.get_by_label("Show scene numbers").click();
    h.get_by_label("Show scene description").click();
    h.run();

    let heading_bi = h.state().scene_heading_block_for_test(0).expect("heading");
    let heading_text = h.state().block_text(heading_bi).expect("heading text");
    h.state_mut().goto_tab(SideTab::Script);
    h.run();
    click_editor2_page(&mut h);
    let row = h.get_by_label_contains(&heading_text);
    let label = row
        .accesskit_node()
        .label()
        .expect("scene rows expose their text to assistive tech");
    assert!(
        !label.contains(" — "),
        "scene description must be hidden: {label}"
    );
    assert_eq!(
        label.trim(),
        heading_text.trim(),
        "scene numbers must be hidden"
    );

    let action_bi = (0..h.state().block_count())
        .find(|&bi| h.state().block_element_type(bi) == Some(ElementType::Action))
        .expect("A95 has action blocks");
    h.state_mut().set_block_text_for_test(action_bi, "");
    h.state_mut().focus_block_at(action_bi, 0);
    h.run_steps(2);
    h.input_mut()
        .events
        .push(egui::Event::Text("\"Hi\"".into()));
    h.run();
    assert_eq!(
        h.state().block_text(action_bi).as_deref(),
        Some("“Hi”"),
        "Settings ▸ Smart quotes must change typed text"
    );
}

/// Settings ▸ Theme switches the whole palette between Midnight and Light.
#[test]
fn theme_dropdown_switches_palette() {
    let mut h = harness(SideTab::Settings);
    h.run();
    assert!(
        h.ctx.style().visuals.dark_mode,
        "app starts in the Midnight theme"
    );

    let theme_combo = h
        .get_all_by_role(egui::accesskit::Role::ComboBox)
        .find(|n| !n.accesskit_node().is_disabled())
        .expect("theme chooser should be an enabled combo box");
    theme_combo.click();
    h.run();
    h.get_by_label("Light").click();
    h.run_steps(2);

    let v = &h.ctx.style().visuals;
    assert!(
        !v.dark_mode,
        "choosing Light should switch egui Visuals to light"
    );
    assert_eq!(
        v.panel_fill,
        egui::Color32::from_rgb(0xF8, 0xF9, 0xFB),
        "panel fill must come from the KIT light palette"
    );

    h.get_all_by_role(egui::accesskit::Role::ComboBox)
        .find(|n| !n.accesskit_node().is_disabled())
        .expect("theme chooser should still be enabled")
        .click();
    h.run();
    h.get_by_label("Midnight").click();
    h.run_steps(2);
    assert!(
        h.ctx.style().visuals.dark_mode,
        "choosing Midnight should restore the dark palette"
    );
}

/// Cmd/Ctrl+1…0 retypes every block covered by the editor selection.
#[test]
fn format_shortcuts_apply_to_selection() {
    use scriptcraft_core::model::ElementType;

    let mut h = focused_harness();
    let first = h
        .state()
        .find_first_cue_block()
        .expect("A95 should contain at least one character cue");
    let second = first + 1;
    assert!(
        second < h.state().block_count(),
        "cue must have a next block"
    );

    h.state_mut()
        .editor2_set_selection((first, 0, 0), (second, 0, 0));
    h.run();

    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num1);
    h.run();

    for bi in [first, second] {
        assert_eq!(
            h.state().block_element_type(bi),
            Some(ElementType::SceneHeading),
            "Cmd+1 should turn block {bi} into a scene heading"
        );
        let text = h.state().block_text(bi).unwrap_or_default();
        assert_eq!(
            text,
            text.to_uppercase(),
            "scene headings are uppercased on conversion"
        );
    }
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

/// The fixture must paginate to the same page count at every zoom (zoom only
/// scales painted paper; it never reflows) and painting must not touch text.
#[test]
fn a95_page_count_is_zoom_invariant() {
    let mut h = harness(SideTab::Script);
    h.run();

    let pages = h.state().page_count();
    assert!(pages > 1, "the fixture must span several pages");
    assert_eq!(
        h.state().editor2_page_count(),
        pages,
        "the painted editor must read the same pagination cache"
    );

    let snapshot: Vec<Option<String>> = (0..20).map(|i| h.state().block_text(i)).collect();

    for zoom in [1.0_f32, 0.95, 2.0] {
        h.state_mut().set_zoom(zoom);
        h.run_steps(2);
        assert_eq!(
            h.state().page_count(),
            pages,
            "zoom {zoom} must not change the pagination"
        );
    }

    let after: Vec<Option<String>> = (0..20).map(|i| h.state().block_text(i)).collect();
    assert_eq!(snapshot, after, "painting must not touch the text");
}

/// Clicking a page, typing, and ⌘Z must edit the clicked block and undo it
/// through the command history.
#[test]
fn editor2_click_typing_and_undo() {
    let mut h = focused_harness();

    let (_, focus) = h
        .state()
        .editor2_selection()
        .expect("clicking a page must place the caret");
    let bi = focus.0;
    let before = h.state().block_text(bi).expect("clicked block exists");

    h.input_mut().events.push(egui::Event::Text("Я".into()));
    h.run();
    let after = h.state().block_text(bi).unwrap();
    assert_ne!(before, after, "typing must edit the clicked block");
    assert!(after.contains('Я'), "typed character must be inserted");

    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run();
    assert_eq!(
        h.state().block_text(bi).unwrap(),
        before,
        "⌘Z must restore the text through the command history"
    );
}

/// Cmd/Ctrl+D pairs the current speech unit with the next one, removes the
/// pair when pressed again, and both sides undo/redo as one step.
#[test]
fn dual_dialogue_shortcut_pairs_unpairs_and_undoes() {
    let mut h = focused_harness();
    let (left, right) = h
        .state_mut()
        .focus_first_dual_candidate_for_test()
        .expect("A95 should contain two adjacent speech units");
    h.run();

    let (target_left, target_right, paired) = h
        .state()
        .dual_target_for_test()
        .expect("the focused cue is pair-able");
    assert_eq!((target_left, target_right), (left, right));
    assert!(!paired, "the fixture starts unpaired");

    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::D);
    h.run();
    assert_eq!(
        h.state().first_dual_pair_for_test(),
        Some((left, right)),
        "Cmd+D pairs the selected unit with the next one"
    );
    assert!(
        h.state().status_for_test().contains("side by side"),
        "status must explain the pairing: {}",
        h.state().status_for_test()
    );

    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::D);
    h.run();
    assert_eq!(
        h.state().first_dual_pair_for_test(),
        None,
        "Cmd+D on a pair removes it"
    );
    assert!(h.state().status_for_test().contains("removed"));

    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::D);
    h.run();
    assert!(h.state().first_dual_pair_for_test().is_some());
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run();
    assert_eq!(
        h.state().first_dual_pair_for_test(),
        None,
        "one undo removes both markers"
    );
    h.key_press_modifiers(
        egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
        egui::Key::Z,
    );
    h.run();
    assert_eq!(
        h.state().first_dual_pair_for_test(),
        Some((left, right)),
        "redo restores both markers"
    );
}

/// A pairing survives Save and reopening in KIT's sidecar.
#[test]
fn dual_dialogue_persists_across_save_and_reopen() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/A95.kitsp");
    let dir = std::env::temp_dir().join(format!("scriptcraft-dual-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Dual.kitsp");
    std::fs::copy(fixture, &path).unwrap();
    let path_str = path.to_string_lossy().into_owned();

    let mut h = harness(SideTab::Script);
    h.run();
    h.state_mut().open_path(&path_str);
    h.run();
    let (left, right) = h
        .state_mut()
        .focus_first_dual_candidate_for_test()
        .expect("cue pair");
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::D);
    h.run();
    let paired = h.state().first_dual_pair_for_test().expect("paired");
    assert_eq!(paired, (left, right));
    assert!(
        h.state()
            .dual_save_note_for_test()
            .contains("KIT will show"),
        "saving to KIT must explain that pairing is ScriptCraft-only"
    );
    assert!(h
        .state()
        .dual_export_note_for_test("Fountain")
        .contains("not represented"));
    assert!(h.state().dual_export_note_for_test("FDX").is_empty());

    h.get_by_label("Menu").click();
    h.run();
    h.get_by_label("Save").click();
    h.run();
    assert!(!h.state().is_dirty());

    let mut reopened = harness(SideTab::Script);
    reopened.run();
    reopened.state_mut().open_path(&path_str);
    reopened.run();
    assert_eq!(
        reopened.state().first_dual_pair_for_test(),
        Some(paired),
        "the sidecar re-pairs the same two cues"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn editor2_enter_splits_and_backspace_merges() {
    use scriptcraft_core::editing::enter_next;
    use scriptcraft_core::model::ElementType;

    let mut h = focused_harness();

    let bi = h.state().find_first_heading_block().expect("heading");
    let text = h.state().block_text(bi).unwrap();
    let len = text.chars().count();
    h.state_mut().editor2_set_caret((bi, 0, len));
    let n = h.state().block_count();

    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(h.state().block_count(), n + 1, "Enter must split a block");
    assert_eq!(h.state().block_text(bi + 1).unwrap(), "");
    assert_eq!(
        h.state().block_element_type(bi + 1).unwrap(),
        enter_next(ElementType::SceneHeading),
        "split continues with the KIT element type"
    );

    h.state_mut().editor2_set_caret((bi + 1, 0, 0));
    h.key_press(egui::Key::Backspace);
    h.run();
    assert_eq!(h.state().block_count(), n, "Backspace must merge back");
    assert_eq!(h.state().block_text(bi).unwrap(), text);
}

/// Cut removes a whole block's text and paste restores it (plain path).
#[test]
fn editor2_cut_and_paste_roundtrip() {
    let mut h = focused_harness();

    let bi = h.state().find_first_heading_block().expect("heading");
    let text = h.state().block_text(bi).unwrap();
    let len = text.chars().count();
    h.state_mut()
        .editor2_set_selection((bi, 0, 0), (bi, 0, len));
    h.run();

    h.input_mut().events.push(egui::Event::Cut);
    h.run();
    assert_eq!(
        h.state().block_text(bi).unwrap(),
        "",
        "cut must remove the selected text"
    );

    h.input_mut().events.push(egui::Event::Paste(text.clone()));
    h.run();
    assert_eq!(
        h.state().block_text(bi).unwrap(),
        text,
        "paste must restore the text"
    );
}

/// Pasting plain screenplay text detects element types instead of dropping
/// everything into the host block as one action paragraph.
#[test]
fn editor2_paste_screenplay_formats_blocks() {
    use scriptcraft_core::model::ElementType;

    let mut h = focused_harness();
    let last = h.state().block_count() - 1;
    let text = h.state().block_text(last).unwrap();
    let end = (
        last,
        text.split('\n').count() - 1,
        text.rsplit('\n').next().unwrap().chars().count(),
    );
    h.state_mut().editor2_set_selection((0, 0, 0), end);
    h.run();

    let screenplay = "\
INT. COFFEE SHOP - DAY

Sunlight cuts through the blinds.

MAYA
(to herself)
Just one more page.

She opens her laptop.

CUT TO:

EXT. STREET - NIGHT

Rain falls.";
    h.input_mut()
        .events
        .push(egui::Event::Paste(screenplay.into()));
    h.run();

    let got: Vec<(ElementType, String)> = (0..h.state().block_count())
        .map(|bi| {
            (
                h.state().block_element_type(bi).unwrap(),
                h.state().block_text(bi).unwrap(),
            )
        })
        .collect();
    use ElementType::*;
    assert_eq!(
        got,
        vec![
            (SceneHeading, "INT. COFFEE SHOP - DAY".into()),
            (Action, "Sunlight cuts through the blinds.".into()),
            (Character, "MAYA".into()),
            (Parenthetical, "(to herself)".into()),
            (Dialogue, "Just one more page.".into()),
            (Action, "She opens her laptop.".into()),
            (Transition, "CUT TO:".into()),
            (SceneHeading, "EXT. STREET - NIGHT".into()),
            (Action, "Rain falls.".into()),
        ]
    );
}

/// Ordinary multi-line text keeps the old paste behaviour: one block, not a
/// guessed screenplay.
#[test]
fn editor2_paste_plain_prose_stays_one_block() {
    use scriptcraft_core::model::ElementType;

    let mut h = focused_harness();
    let bi = (0..h.state().block_count())
        .find(|&bi| h.state().block_element_type(bi) == Some(ElementType::Action))
        .expect("action block");
    let text = h.state().block_text(bi).unwrap();
    let len = text.chars().count();
    h.state_mut()
        .editor2_set_selection((bi, 0, 0), (bi, 0, len));
    h.run();
    let count = h.state().block_count();

    let prose = "Meet me at 10.\nBring the notes.";
    h.input_mut().events.push(egui::Event::Paste(prose.into()));
    h.run();

    assert_eq!(h.state().block_count(), count, "prose must not add blocks");
    assert_eq!(h.state().block_text(bi).unwrap(), prose);
}

/// Fit Page / Fit Width lock the zoom to the viewport: the whole page (or
/// its width) stays visible at any window size, and manual zoom unlocks.
#[test]
fn fit_zoom_keeps_the_page_in_view() {
    use scriptcraft_app::script_view::{fit_page_zoom, fit_width_zoom, page_size_pt};

    let mut h = harness(SideTab::Script);
    h.run();

    h.state_mut().fit_page();
    h.run();
    let (z, vp) = h.state().last_viewport_zoom();
    let raw = fit_page_zoom(vp);
    assert!(
        (z - raw).abs() < 1e-4,
        "fit page must use the exact continuous zoom {raw} (got {z})"
    );
    let page = page_size_pt(z);
    assert!(
        page.y + 46.0 * z <= vp.y,
        "the whole page must fit (page {} + pads vs viewport {})",
        page.y,
        vp.y
    );
    let bigger = page_size_pt(z * 1.12);
    assert!(
        bigger.y + 46.0 * z * 1.12 > vp.y,
        "fit page must be tight (12% larger would overflow)"
    );

    h.state_mut().fit_width();
    h.run();
    let (z, vp) = h.state().last_viewport_zoom();
    let raw = fit_width_zoom(vp);
    assert!(
        (z - raw).abs() < 1e-4,
        "fit width must use the exact continuous zoom {raw} (got {z})"
    );
    assert!(
        page_size_pt(z).x <= vp.x,
        "the page width must fit the viewport"
    );
    assert!(
        page_size_pt(z * 1.12).x > vp.x,
        "fit width must be tight (12% larger would overflow)"
    );

    h.state_mut().set_zoom(1.0);
    h.run();
    let (z, _) = h.state().last_viewport_zoom();
    assert!(
        (z - 1.0).abs() < 1e-4,
        "manual zoom must leave fit mode (got {z})"
    );
}

/// Zoom is continuous: it no longer snaps to the 12 pt font's integer pixel
/// grid (79% used to become 19/24 ≈ 79.17%, 88% became 21/24 = 87.5%).
#[test]
fn zoom_is_continuous_not_font_grid_snapped() {
    let mut h = harness(SideTab::Script);
    h.run();
    h.state_mut().set_zoom(0.79);
    h.run();
    let (z, _) = h.state().last_viewport_zoom();
    assert!((z - 0.79).abs() < 1e-4, "zoom must stay 0.79 (got {z})");
    h.state_mut().set_zoom(1.37);
    h.run();
    let (z, _) = h.state().last_viewport_zoom();
    assert!((z - 1.37).abs() < 1e-4, "zoom must stay 1.37 (got {z})");
}

/// FDX import replaces the session: fixture paragraphs map to element
/// types, notes survive as non-printing blocks, and scene headings get
/// fresh KIT uuids for a later Save As.
#[test]
fn fdx_import_builds_a_project() {
    use scriptcraft_core::model::ElementType;

    let mut h = harness(SideTab::Script);
    h.run();
    h.state_mut()
        .import_fdx_str(include_str!("../../fixtures/example.fdx"))
        .expect("the sample FDX must import");
    assert!(h.state_mut().import_fdx_str("not xml").is_err());
    assert!(h.state_mut().import_fdx_str("<x/>").is_err());
    h.run();

    let a = h.state();
    assert_eq!(a.block_count(), 14);
    assert_eq!(a.block_text(0).unwrap(), "INT. COFFEE SHOP - MORNING");
    assert_eq!(a.block_element_type(0).unwrap(), ElementType::SceneHeading);
    assert_eq!(a.block_element_type(2).unwrap(), ElementType::NonPrintable);
    assert_eq!(
        a.block_text(2).unwrap(),
        "Check the shop name with production."
    );
    assert_eq!(
        a.block_element_type(6).unwrap(),
        ElementType::SceneCharacters,
        "FD Cast List maps to KIT scene characters"
    );
    assert_eq!(
        a.block_text(10).unwrap(),
        "Mira steps into the rain & opens her umbrella."
    );
    assert_eq!(a.block_element_type(13).unwrap(), ElementType::NonPrintable);
}

/// The Export dialog now offers PDF / FDX / Fountain — DOCX is hidden
/// until implemented.
#[test]
fn export_dialog_offers_fdx_not_docx() {
    let mut h = harness(SideTab::Script);
    h.run();

    h.get_by_label("Menu").click();
    h.run();
    h.get_by_label("Export…").click();
    h.run();
    assert!(h.query_by_label("FDX").is_some(), "FDX radio must be shown");
    assert!(
        h.query_by_label("DOCX").is_none(),
        "unimplemented DOCX must be hidden"
    );

    h.get_by_label("Cancel").click_accesskit();
    h.run();
    h.run();
}

/// Start page: the × on a recent-project card removes the entry from the
/// list (the project file itself is untouched).
#[test]
fn start_view_removes_recent_projects() {
    let mut h = harness(SideTab::Start);
    h.state_mut().pin_recent(vec![(
        "/tmp/Delete_Me.kitsp".to_string(),
        "18.10.2026 10:00".to_string(),
    )]);
    h.run();
    assert!(
        h.query_by_label("Delete_Me").is_some(),
        "the pinned project should be listed"
    );

    h.get_by_label("Remove from list").click();
    h.run();

    assert!(
        h.query_by_label("Delete_Me").is_none(),
        "the card should disappear after removal"
    );
    assert!(
        h.query_by_label("No recent projects yet").is_some(),
        "empty state should be shown"
    );
}

/// Start ▸ Open Projects Folder reveals the folder of the latest project.
#[test]
fn projects_folder_follows_latest_project() {
    let mut h = harness(SideTab::Start);
    let dir = std::env::temp_dir().join(format!("scriptcraft-projects-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Sample.kitsp");
    std::fs::write(&path, b"").unwrap();
    h.state_mut()
        .pin_recent(vec![(path.to_string_lossy().into_owned(), String::new())]);

    assert_eq!(
        h.state().projects_folder_for_test(),
        dir.to_string_lossy().as_ref()
    );

    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir(&dir).unwrap();
}

/// Typing into a scene heading forces caps, and once the " - " separator is
/// typed the dropdown offers the standard times of day.
#[test]
fn scene_heading_caps_and_time_autocomplete() {
    let mut h = focused_harness();
    let bi = h
        .state()
        .find_first_heading_block()
        .expect("A95 should contain a scene heading");
    h.state_mut().focus_block_at(bi, 0);
    h.run_steps(2);
    h.input_mut()
        .events
        .push(egui::Event::Text("int. test - ".into()));
    h.run();

    let text = h.state().block_text(bi).unwrap_or_default();
    assert!(
        text.starts_with("INT. TEST - "),
        "typed heading must be uppercase: {text}"
    );
    let rows = h.state().editor2_autocomplete_rows();
    let labels: Vec<&str> = rows.iter().map(|(s, _)| s.as_str()).collect();
    assert!(
        labels.contains(&"INT. TEST - DAY"),
        "times should be suggested: {labels:?}"
    );
    assert!(
        labels.contains(&"INT. TEST - A FEW MOMENTS LATER"),
        "all standard times should be suggested: {labels:?}"
    );
}

/// Switching projects from Start must ask before discarding unsaved edits;
/// Cancel keeps the current project, Don't Save proceeds.
#[test]
fn switching_from_start_prompts_to_save() {
    let mut h = focused_harness();
    let bi = h
        .state()
        .find_first_heading_block()
        .expect("A95 should contain a scene heading");
    h.state_mut().focus_block_at(bi, 0);
    h.run_steps(2);
    h.input_mut().events.push(egui::Event::Text("Q".into()));
    h.run();
    assert!(h.state().is_dirty(), "typing must dirty the project");

    h.state_mut().goto_tab(SideTab::Start);
    h.run();

    h.get_by_label("A95").click();
    h.run();
    assert!(
        h.state().save_prompt_open(),
        "switching projects must prompt when dirty"
    );
    assert!(h.state().is_dirty(), "nothing saved yet");

    h.get_by_label("Cancel").click();
    h.run();
    assert!(!h.state().save_prompt_open());
    assert!(h.state().is_dirty(), "Cancel must keep the unsaved project");

    h.get_by_label("A95").click();
    h.run();
    h.get_by_label("Don't Save").click();
    h.run();
    assert!(!h.state().save_prompt_open());
    assert!(!h.state().is_dirty(), "the project was reloaded from disk");
}

/// Save in the prompt writes the project, then the requested switch happens.
#[test]
fn switching_from_start_can_save_first() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/A95.kitsp");
    let dir = std::env::temp_dir().join(format!("scriptcraft-save-prompt-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Prompt_Test.kitsp");
    std::fs::copy(fixture, &path).unwrap();
    let path_str = path.to_string_lossy().into_owned();

    let mut h = harness(SideTab::Script);
    h.run();
    h.state_mut().open_path(&path_str);
    h.run();
    click_editor2_page(&mut h);

    let bi = h
        .state()
        .find_first_heading_block()
        .expect("A95 should contain a scene heading");
    h.state_mut().focus_block_at(bi, 0);
    h.run_steps(2);
    h.input_mut().events.push(egui::Event::Text("Q".into()));
    h.run();
    assert!(h.state().is_dirty());

    h.state_mut().goto_tab(SideTab::Start);
    h.run();
    h.get_by_label("Prompt_Test").click();
    h.run();
    assert!(h.state().save_prompt_open());

    h.get_by_label("Save").click();
    h.run();
    assert!(
        !h.state().save_prompt_open(),
        "prompt resolves after saving"
    );
    assert!(!h.state().is_dirty(), "the project must have been saved");

    let saved = scriptcraft_core::kitsp::load_file(&path_str).expect("saved project loads");
    assert_eq!(
        saved.blocks[bi].text.chars().next().map(|c| c.to_string()),
        Some("Q".to_string()),
        "typed text must be on disk"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Quit (native menu / window close) asks before discarding unsaved work.
#[test]
fn quit_prompts_when_dirty() {
    let mut h = focused_harness();
    let bi = h
        .state()
        .find_first_heading_block()
        .expect("A95 should contain a scene heading");
    h.state_mut().focus_block_at(bi, 0);
    h.run_steps(2);
    h.input_mut().events.push(egui::Event::Text("Q".into()));
    h.run();
    assert!(h.state().is_dirty());

    let mut info = h
        .input()
        .viewports
        .get(&egui::ViewportId::ROOT)
        .cloned()
        .unwrap_or_default();
    info.events.push(egui::ViewportEvent::Close);
    h.input_mut().viewports.insert(egui::ViewportId::ROOT, info);
    h.run();
    assert!(h.state().save_prompt_open(), "quit must prompt when dirty");

    let before = h.state().block_text(bi);
    h.input_mut().events.push(egui::Event::Text("Z".into()));
    h.run();
    assert_eq!(
        h.state().block_text(bi),
        before,
        "the modal must swallow typing meant for the editor behind it"
    );

    h.get_by_label("Cancel").click();
    h.run();
    assert!(!h.state().save_prompt_open());
    assert!(h.state().is_dirty());
}

/// Dock ▸ Quit / AppleScript `quit` arrive through the native quit hook; they
/// must open the same save prompt as the window close button.
#[test]
fn native_quit_request_prompts_when_dirty() {
    let mut h = focused_harness();
    let bi = h
        .state()
        .find_first_heading_block()
        .expect("A95 should contain a scene heading");
    h.state_mut().focus_block_at(bi, 0);
    h.run_steps(2);
    h.input_mut().events.push(egui::Event::Text("Q".into()));
    h.run();
    assert!(h.state().is_dirty());

    scriptcraft_app::quit_guard::request_for_test();
    h.run();

    assert!(
        h.state().save_prompt_open(),
        "a Dock/AppleScript quit must show the save prompt"
    );
    assert!(h.state().is_dirty(), "nothing saved yet");

    h.get_by_label("Cancel").click();
    h.run();
    assert!(!h.state().save_prompt_open());
    assert!(h.state().is_dirty(), "Cancel must keep the unsaved project");
}

/// Revision marks follow edits; starting a new colour re-baselines so only
/// later edits take the new colour.
#[test]
fn revision_marks_track_edits_and_override_colors() {
    use scriptcraft_core::revision::RevisionColor;

    let mut h = focused_harness();
    let bi = h
        .state()
        .find_first_heading_block()
        .expect("A95 should contain a scene heading");

    h.state_mut().start_revision();
    h.run();
    assert_eq!(h.state().latest_revision_color(), Some(RevisionColor::Blue));
    assert!(h.state().latest_revision_pages().unwrap_or(0) > 0);

    h.state_mut().focus_block_at(bi, 0);
    h.run_steps(2);
    h.input_mut().events.push(egui::Event::Text("REV ".into()));
    h.run();
    assert_eq!(
        h.state().revision_marks_for_test().get(bi),
        Some(&Some(RevisionColor::Blue))
    );

    h.state_mut().start_revision();
    h.run();
    assert_eq!(h.state().latest_revision_color(), Some(RevisionColor::Pink));

    let bj = bi + 1;
    h.state_mut().focus_block_at(bj, 0);
    h.run_steps(2);
    h.input_mut().events.push(egui::Event::Text("X".into()));
    h.run();
    assert_eq!(
        h.state().revision_marks_for_test().get(bi),
        Some(&Some(RevisionColor::Blue))
    );
    assert_eq!(
        h.state().revision_marks_for_test().get(bj),
        Some(&Some(RevisionColor::Pink))
    );

    h.state_mut().set_current_revision_only(true);
    h.run();
    assert!(h.state().current_revision_only());
    assert_eq!(h.state().revision_marks_for_test().get(bi), Some(&None));
    assert_eq!(
        h.state().revision_marks_for_test().get(bj),
        Some(&Some(RevisionColor::Pink))
    );

    h.state_mut().set_current_revision_only(false);
    h.run();
    assert_eq!(
        h.state().revision_marks_for_test().get(bi),
        Some(&Some(RevisionColor::Blue))
    );
}

/// The Menu popup offers the next revision colour and starts it.
#[test]
fn revision_controls_in_research_versions_start_next_color() {
    use scriptcraft_core::revision::RevisionColor;

    let mut h = harness(SideTab::Script);
    h.run();
    open_research_versions(&mut h);
    assert!(
        h.query_by_label("Start Blue Revision").is_some(),
        "first revision must be offered as Blue"
    );
    h.get_by_label("Start Blue Revision").click();
    h.run();
    assert_eq!(h.state().latest_revision_color(), Some(RevisionColor::Blue));

    h.state_mut().goto_tab(SideTab::Research);
    h.state_mut().set_research_pane(ResearchPane::Versions);
    h.run();
    assert!(
        h.query_by_label("Start Pink Revision").is_some(),
        "the next revision must advance to Pink"
    );
}

/// The revision-history window lists saved sets and controls the filter.
#[test]
fn revision_history_window_lists_sets_and_toggles_mode() {
    let mut h = harness(SideTab::Script);
    h.run();
    h.state_mut().start_revision();
    h.run();

    open_research_versions(&mut h);
    h.get_by_label("Revision History…").click();
    h.run();
    assert!(h.query_by_label("Revision History").is_some());
    assert!(h.query_by_label("Blue Revision").is_some());
    assert!(h.query_by_label("current").is_some());
    assert!(h.query_by_label("Restore as copy…").is_some());

    let toggles: Vec<_> = h.get_all_by_label("Show current revision only").collect();
    assert!(!toggles.is_empty(), "the filter toggle must be available");
    toggles[0].click();
    h.run();
    assert!(h.state().current_revision_only());
}

/// Page locks capture the current starts, survive Save/reopen, and unlock.
#[test]
fn page_locks_capture_persist_and_unlock() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/A95.kitsp");
    let dir = std::env::temp_dir().join(format!("scriptcraft-pagelock-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("PageLock.kitsp");
    std::fs::copy(fixture, &path).unwrap();
    let path_str = path.to_string_lossy().into_owned();

    let mut h = harness(SideTab::Script);
    h.run();
    h.state_mut().open_path(&path_str);
    h.run();
    h.state_mut().lock_pages_for_test();
    h.run();
    let lock = h.state().page_lock_for_test().expect("pages locked");
    assert!(lock.pages >= 2, "A95 spans at least two pages");
    let page_count = lock.pages;
    let labels = h.state().page_labels_for_test();
    assert_eq!(labels.first(), Some(&None));
    assert_eq!(labels.get(1), Some(&Some("2".to_string())));

    h.get_by_label("Menu").click();
    h.run();
    h.get_by_label("Save").click();
    h.run();
    assert!(!h.state().is_dirty());

    let mut reopened = harness(SideTab::Script);
    reopened.run();
    reopened.state_mut().open_path(&path_str);
    reopened.run();
    let lock = reopened
        .state()
        .page_lock_for_test()
        .expect("lock persisted");
    assert_eq!(lock.pages, page_count);
    assert!(!lock.anchors.is_empty());
    assert_eq!(
        reopened.state().page_labels_for_test().get(1),
        Some(&Some("2".to_string()))
    );

    reopened.state_mut().unlock_pages_for_test();
    assert!(reopened.state().page_lock_for_test().is_none());
    assert_eq!(
        reopened.state().page_labels_for_test().get(1),
        Some(&Some("2".to_string()))
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Revision baselines and colours survive Save and reopening the .kitsp.
#[test]
fn revisions_persist_across_save_and_reopen() {
    use scriptcraft_core::revision::RevisionColor;

    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/A95.kitsp");
    let dir = std::env::temp_dir().join(format!("scriptcraft-revisions-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Revisions.kitsp");
    std::fs::copy(fixture, &path).unwrap();
    let path_str = path.to_string_lossy().into_owned();

    let mut h = harness(SideTab::Script);
    h.run();
    h.state_mut().open_path(&path_str);
    h.run();
    click_editor2_page(&mut h);

    let bi = h.state().find_first_heading_block().expect("heading");
    h.state_mut().start_revision();
    h.state_mut().focus_block_at(bi, 0);
    h.run_steps(2);
    h.input_mut().events.push(egui::Event::Text("REV ".into()));
    h.run();
    assert!(h.state().is_dirty());

    h.get_by_label("Menu").click();
    h.run();
    h.get_by_label("Save").click();
    h.run();
    assert!(!h.state().is_dirty(), "menu Save must write the project");

    let mut reopened = harness(SideTab::Script);
    reopened.run();
    reopened.state_mut().open_path(&path_str);
    reopened.run();
    assert_eq!(
        reopened.state().latest_revision_color(),
        Some(RevisionColor::Blue)
    );
    assert_eq!(
        reopened.state().revision_marks_for_test().get(bi),
        Some(&Some(RevisionColor::Blue))
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Saving a version writes a KIT `script_versions` row with the current
/// scenario XML, refreshes the Research ▸ Versions list, and refuses a
/// duplicate name without disturbing the first version.
#[test]
fn save_version_writes_kit_row_and_refreshes_list() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/A95.kitsp");
    let dir = std::env::temp_dir().join(format!("scriptcraft-versions-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Versions.kitsp");
    std::fs::copy(fixture, &path).unwrap();
    let path_str = path.to_string_lossy().into_owned();

    let mut h = harness(SideTab::Script);
    h.run();
    h.state_mut().open_path(&path_str);
    h.run();
    assert!(
        h.state()
            .versions_for_test()
            .iter()
            .all(|(name, _)| name != "Draft One"),
        "the fixture has no Draft One version"
    );
    let seed_rows = h.state().versions_for_test().len();

    assert!(
        h.state_mut()
            .save_version_for_test("Draft One", "first pass"),
        "saving a version must succeed"
    );
    assert!(!h.state().is_dirty(), "saving a version is a save");
    assert_eq!(h.state().versions_for_test().len(), seed_rows + 1);
    assert_eq!(h.state().versions_for_test().last().unwrap().0, "Draft One");

    let rows = scriptcraft_core::kitsp::load_version_scripts(&path).unwrap();
    assert_eq!(rows.len(), seed_rows + 1, "one KIT-visible row was added");
    let row = rows.iter().find(|r| r.name == "Draft One").unwrap();
    assert_eq!(row.description, "first pass");
    assert_eq!(row.username, "tester");
    assert_eq!(row.datetime, "2026-11-01 10:00:00:123");
    let saved = scriptcraft_core::kitsp::parse_scenario(&row.script_text).unwrap();
    assert_eq!(
        saved.blocks.len(),
        h.state().block_count(),
        "the version XML must hold the whole current script"
    );

    // A duplicate name is refused, and the first row is untouched.
    assert!(!h.state_mut().save_version_for_test("Draft One", "again"));
    assert!(
        h.state().status_for_test().contains("already exists"),
        "the status must explain the duplicate: {}",
        h.state().status_for_test()
    );
    assert_eq!(h.state().versions_for_test().len(), seed_rows + 1);
    assert_eq!(
        scriptcraft_core::kitsp::load_version_scripts(&path)
            .unwrap()
            .len(),
        seed_rows + 1
    );

    // Reopening picks the version list back up.
    let mut reopened = harness(SideTab::Script);
    reopened.run();
    reopened.state_mut().open_path(&path_str);
    reopened.run();
    assert_eq!(reopened.state().versions_for_test().len(), seed_rows + 1);
    assert!(reopened
        .state()
        .versions_for_test()
        .iter()
        .any(|(name, _)| name == "Draft One"));

    let _ = std::fs::remove_dir_all(&dir);
}

/// The compare workspace is reachable from a button in the Research module
/// header, not only from the Versions pane.
#[test]
fn compare_opens_from_research_header() {
    let mut h = harness(SideTab::Script);
    h.run();
    h.state_mut().start_revision();
    h.run();
    h.state_mut().goto_tab(SideTab::Research);
    h.run();

    assert!(
        h.query_by_label("Compare Drafts…").is_some(),
        "Research must expose a Compare Drafts button"
    );
    h.get_by_label("Compare Drafts…").click();
    h.run();

    assert!(
        h.state().compare_open_for_test(),
        "the Research button must open the compare workspace"
    );
}

/// The compare workspace opens from the menu, lists changes, filters them,
/// navigates with keyboard, jumps to the editor, and closes without mutating
/// the project.
#[test]
fn compare_workspace_open_filter_navigate_jump_close() {
    use scriptcraft_app::app::{ChangeFilter, CompareView};

    let mut h = harness(SideTab::Script);
    h.run();

    // Start a revision so there's something to compare against.
    h.state_mut().start_revision();
    h.run();

    // Make an edit to the current draft so the diff has changes.
    let bi = h
        .state()
        .find_first_heading_block()
        .expect("A95 should contain a scene heading");
    let original = h.state().block_text(bi).unwrap();
    let edited = format!("{original} REV");
    h.state_mut().set_block_text_for_test(bi, &edited);
    let second = bi + 1;
    assert!(
        second < h.state().block_count(),
        "heading must have a following block"
    );
    let original_second = h.state().block_text(second).unwrap();
    h.state_mut()
        .set_block_text_for_test(second, &format!("{original_second} REV"));
    h.run();
    // Verify the edit was applied.
    let after_edit = h.state().block_text(bi).unwrap();
    assert_ne!(after_edit, original, "edit must change the block text");
    assert!(after_edit.ends_with("REV"), "edit must append REV");
    assert!(h.state().is_dirty());

    // Open compare workspace from the Research header.
    open_research_versions(&mut h);
    h.get_by_label("Compare Drafts…").click();
    h.run();
    // Extra frame to ensure compare workspace renders and computes diff.
    h.run();

    // Verify the compare workspace is open.
    assert!(h.state().compare_open_for_test());
    assert!(h.query_by_label("Compare Drafts").is_some());
    assert!(h.query_by_label("Close").is_some());
    // The redesigned header states the read-only guarantee and the keyboard
    // controls instead of relying on hover text.
    assert!(
        h.query_by_label("Read-only").is_some(),
        "compare must show the read-only badge"
    );
    assert!(
        h.query_by_label("J/K navigate · V switch view · Enter open · Esc close")
            .is_some(),
        "compare must surface its keyboard shortcuts"
    );
    assert!(
        h.query_by_label("Changes").is_some(),
        "compare must title the change list"
    );

    // Verify the change list shows changes.
    let counts = h
        .state()
        .compare_counts_for_test()
        .expect("diff must have counts");
    // The edit to a scene heading should appear as a Changed block.
    assert!(
        counts.2 > 0,
        "at least one changed block expected, got {:?}",
        counts
    ); // changed > 0

    // Verify default filter is "All" and view is "Unified".
    assert_eq!(h.state().compare_filter_for_test(), Some(ChangeFilter::All));
    assert_eq!(
        h.state().compare_view_for_test(),
        Some(CompareView::Unified)
    );

    // Test filter cycling: click Added, Removed, Changed, All. Labels carry
    // counts, so build them from the live diff.
    let (added, removed, changed) = counts;
    for (name, count) in [
        ("Added", added),
        ("Removed", removed),
        ("Changed", changed),
        ("All", added + removed + changed),
    ] {
        let label = format!("{name} {count}");
        h.get_by_label(&label).click();
        h.run();
    }
    assert_eq!(h.state().compare_filter_for_test(), Some(ChangeFilter::All));

    // Test keyboard navigation: J/ArrowDown for next, K/ArrowUp for previous.
    let before_step = h.state().compare_selected_for_test();
    h.key_press(egui::Key::J);
    h.run();
    let after_j = h.state().compare_selected_for_test();
    assert_ne!(before_step, after_j, "J must advance selection");

    h.key_press(egui::Key::K);
    h.run();
    let after_k = h.state().compare_selected_for_test();
    assert_eq!(after_k, before_step, "K must go back to previous");

    // Test arrow keys also work.
    h.key_press(egui::Key::ArrowDown);
    h.run();
    let after_arrow = h.state().compare_selected_for_test();
    assert_ne!(after_arrow, after_k, "ArrowDown must advance selection");

    h.key_press(egui::Key::ArrowUp);
    h.run();
    let after_arrow_up = h.state().compare_selected_for_test();
    assert_eq!(after_arrow_up, after_k, "ArrowUp must go back");

    // Test V toggles view.
    assert_eq!(
        h.state().compare_view_for_test(),
        Some(CompareView::Unified)
    );
    h.key_press(egui::Key::V);
    h.run();
    assert_eq!(
        h.state().compare_view_for_test(),
        Some(CompareView::SideBySide)
    );
    h.key_press(egui::Key::V);
    h.run();
    assert_eq!(
        h.state().compare_view_for_test(),
        Some(CompareView::Unified)
    );

    // Test Enter jumps to editor and closes compare.
    // First, ensure there's a valid jump target.
    let jump_target = h
        .state()
        .compare_jump_target_for_test()
        .expect("selection must have a jump target");
    h.key_press(egui::Key::Enter);
    h.run();

    // Compare should be closed, editor should be active at the jumped block.
    assert!(
        !h.state().compare_open_for_test(),
        "compare must close on jump"
    );
    assert_eq!(h.state().side_tab_for_test(), SideTab::Script);
    assert_eq!(h.state().selected_block_for_test(), Some(jump_target));

    // Verify the project was NOT mutated by the compare workspace.
    let after = h.state().block_text(bi).unwrap();
    assert_eq!(after, edited, "project text unchanged by compare");
    assert_eq!(
        h.state().block_text(second).unwrap(),
        format!("{original_second} REV"),
        "second edited block unchanged by compare"
    );
}

/// Compare workspace does not mutate the project or revision history.
#[test]
fn compare_workspace_is_read_only() {
    use scriptcraft_app::app::ChangeFilter;

    let mut h = harness(SideTab::Script);
    h.run();
    h.state_mut().start_revision();
    h.run();

    let revision_count_before = h.state().revision_count_for_test();
    let revision_color_before = h.state().latest_revision_color_for_test();
    let dirty_before = h.state().is_dirty();

    // Open compare.
    open_research_versions(&mut h);
    h.get_by_label("Compare Drafts…").click();
    h.run();

    // Navigate around, toggle view, filter.
    h.key_press(egui::Key::J);
    h.run();
    h.key_press(egui::Key::V);
    h.run();
    // Test filter via test helper (avoids broad button query).
    h.state_mut()
        .set_compare_filter_for_test(ChangeFilter::Added);
    h.run();
    h.state_mut().set_compare_filter_for_test(ChangeFilter::All);
    h.run();

    // Close via Escape.
    h.key_press(egui::Key::Escape);
    h.run();

    // Verify nothing was mutated.
    assert!(!h.state().compare_open_for_test());
    assert_eq!(h.state().revision_count_for_test(), revision_count_before);
    assert_eq!(
        h.state().latest_revision_color_for_test(),
        revision_color_before
    );
    assert_eq!(
        h.state().is_dirty(),
        dirty_before,
        "compare must not dirty the project"
    );
}

/// Opening a new project or starting a revision closes the compare workspace.
#[test]
fn compare_workspace_closes_on_project_change() {
    let mut h = harness(SideTab::Script);
    h.run();
    h.state_mut().start_revision();
    h.run();

    // Open compare.
    open_research_versions(&mut h);
    h.get_by_label("Compare Drafts…").click();
    h.run();
    assert!(h.state().compare_open_for_test());

    // Start a new revision → compare should close.
    h.state_mut().start_revision();
    h.run();
    assert!(
        !h.state().compare_open_for_test(),
        "new revision must close compare"
    );

    // Re-open compare.
    open_research_versions(&mut h);
    h.get_by_label("Compare Drafts…").click();
    h.run();
    assert!(h.state().compare_open_for_test());

    // Open another project → compare should close.
    h.state_mut().open_fixture();
    h.run();
    assert!(
        !h.state().compare_open_for_test(),
        "open_path must close compare"
    );
}

/// An identical draft pair shows a dedicated empty state, and “Show full
/// script” switches the workspace to the complete script.
#[test]
fn compare_identical_drafts_show_empty_state_and_full_script() {
    let mut h = harness(SideTab::Script);
    h.run();
    h.state_mut().start_revision();
    h.run();
    h.state_mut().goto_tab(SideTab::Research);
    h.run();
    h.get_by_label("Compare Drafts…").click();
    h.run();

    assert!(h.state().compare_open_for_test());
    assert!(
        h.query_by_label("No differences").is_some(),
        "identical drafts must show the empty state"
    );
    assert!(h.query_by_label("Show unchanged").is_some());

    h.get_by_label("Show full script").click();
    h.run();

    assert!(
        h.query_by_label("No differences").is_none(),
        "showing the full script must leave the empty state"
    );
}

/// Changes-only mode collapses long unchanged runs; clicking a gap reveals
/// its blocks.
#[test]
fn compare_gap_row_reveals_unchanged_blocks() {
    let mut h = harness(SideTab::Script);
    h.run();
    h.state_mut().start_revision();
    h.run();

    let bi = h
        .state()
        .find_first_heading_block()
        .expect("A95 should contain a scene heading");
    let heading = h.state().block_text(bi).unwrap();
    let action = h.state().block_text(bi + 1).unwrap();
    h.state_mut()
        .set_block_text_for_test(bi, &format!("{heading} — REVISED"));
    h.state_mut()
        .set_block_text_for_test(bi + 1, &format!("{action} (revised)"));
    h.state_mut().open_compare();
    h.run();

    let before = h
        .query_all_by_label_contains("unchanged blocks · click to show")
        .count();
    assert!(
        before > 0,
        "a long unchanged run must collapse into at least one gap row"
    );
    h.query_all_by_label_contains("unchanged blocks · click to show")
        .next()
        .expect("gap row")
        .click();
    h.run();
    let after = h
        .query_all_by_label_contains("unchanged blocks · click to show")
        .count();
    assert!(
        after < before,
        "clicking a gap must reveal its blocks ({before} -> {after})"
    );
}

/// Revision summary counts come from the same diff as compare, group by
/// scene, and show the live page baseline.
#[test]
fn revision_summary_matches_compare_and_groups_by_scene() {
    use scriptcraft_core::model::ElementType;

    let mut h = harness(SideTab::Script);
    h.run();
    h.state_mut().start_revision();
    h.run();
    let base_pages = h
        .state()
        .latest_revision_pages()
        .expect("the revision must record pages");

    // Change the heading of two different scenes.
    let headings: Vec<usize> = (0..h.state().block_count())
        .filter(|bi| h.state().block_element_type(*bi) == Some(ElementType::SceneHeading))
        .collect();
    assert!(headings.len() >= 2, "A95 has 21 scenes");
    let mut edited_headings = Vec::new();
    for &bi in headings.iter().take(2) {
        let original = h.state().block_text(bi).unwrap();
        let edited = format!("{original} REV");
        edited_headings.push(edited.clone());
        h.state_mut().set_block_text_for_test(bi, &edited);
    }
    h.run();

    open_research_versions(&mut h);
    assert!(
        h.query_by_label("Revision Summary…").is_some(),
        "Versions must offer the summary"
    );
    h.get_by_label("Revision Summary…").click();
    h.run();
    h.run();

    assert!(h.state().summary_open_for_test());
    assert!(h.query_by_label("Revision Summary").is_some());
    let summary = h
        .state()
        .summary_for_test()
        .expect("summary must be computed");
    assert_eq!(summary.base_pages, base_pages);
    assert_eq!(summary.current_pages, h.state().page_count());
    assert_eq!(
        summary.total(),
        summary.added + summary.removed + summary.changed
    );
    assert_eq!(
        summary.scenes.iter().map(|s| s.changed).sum::<usize>(),
        summary.changed,
        "scene rows must account for every changed block"
    );
    assert!(
        summary.changed >= 2,
        "two heading edits must be counted: {summary:?}"
    );
    for heading in &edited_headings {
        assert!(
            summary.scenes.iter().any(|s| &s.heading == heading),
            "summary must group by scene heading {heading:?}: {summary:?}"
        );
        assert!(
            h.query_by_label(heading).is_some(),
            "the table must show {heading:?}"
        );
    }
    assert!(h.query_by_label("TOTAL").is_some());

    // The compare workspace must report exactly the same counts.
    h.state_mut().open_compare();
    h.run();
    let compare = h
        .state()
        .compare_counts_for_test()
        .expect("compare must have counts");
    assert_eq!(
        compare,
        (summary.added, summary.removed, summary.changed),
        "summary and compare must agree by construction"
    );
}

/// An untouched revision is a valid zero row in the sheet and the CSV.
#[test]
fn revision_summary_empty_revision_is_a_zero_row() {
    let mut h = harness(SideTab::Script);
    h.run();
    h.state_mut().start_revision();
    h.run();

    open_research_versions(&mut h);
    h.get_by_label("Revision Summary…").click();
    h.run();
    h.run();

    let summary = h
        .state()
        .summary_for_test()
        .expect("summary must exist even with no changes");
    assert_eq!(summary.total(), 0);
    assert!(summary.scenes.is_empty());
    assert!(h.query_by_label("No changes in this revision.").is_some());
    assert!(h.query_by_label("TOTAL").is_some());

    let csv = h
        .state_mut()
        .summary_csv_for_test()
        .expect("CSV must render");
    assert!(csv.starts_with('\u{feff}'));
    assert_eq!(csv.lines().count(), 2, "header plus TOTAL only");
    assert!(
        csv.contains(&format!(
            "TOTAL,,,{},{},0,,,0,0,0",
            summary.base_pages, summary.current_pages
        )),
        "empty revision must still produce a valid total row: {csv}"
    );

    let dir =
        std::env::temp_dir().join(format!("scriptcraft-summary-empty-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Empty Summary.csv");
    assert!(h.state_mut().write_summary_csv_for_test(&path));
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[..3], b"\xEF\xBB\xBF", "Excel needs the UTF-8 BOM");
    let written = String::from_utf8(bytes).expect("summary CSV must be valid UTF-8");
    assert_eq!(written, csv);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The CSV keeps Cyrillic headings intact and is deterministic.
#[test]
fn revision_summary_csv_is_cyrillic_safe() {
    let mut h = harness(SideTab::Script);
    h.run();
    h.state_mut().start_revision();
    h.run();

    let bi = h
        .state()
        .find_first_heading_block()
        .expect("A95 should contain a scene heading");
    let original = h.state().block_text(bi).unwrap();
    let edited = format!("{original} — ПРОМЯНА");
    h.state_mut().set_block_text_for_test(bi, &edited);
    h.run();

    assert!(h.state_mut().open_revision_summary());
    let csv = h
        .state_mut()
        .summary_csv_for_test()
        .expect("CSV must render");
    assert!(csv.starts_with('\u{feff}'));
    assert!(csv.contains(
        "Revision,Colour,Date,Base pages,Current pages,Pages delta,Scene,Heading,\
         Added blocks,Removed blocks,Changed blocks"
    ));
    assert!(csv.contains("— ПРОМЯНА"), "Cyrillic heading lost: {csv}");
    assert!(csv.contains("TOTAL"));
    assert_eq!(
        csv,
        h.state_mut().summary_csv_for_test().unwrap(),
        "the same summary must always produce the same CSV"
    );
}

/// Print Summary builds a real PDF from the same counts.
#[test]
fn revision_summary_pdf_is_printable() {
    let mut h = harness(SideTab::Script);
    h.run();
    h.state_mut().start_revision();
    h.run();

    let bi = h
        .state()
        .find_first_heading_block()
        .expect("A95 should contain a scene heading");
    let original = h.state().block_text(bi).unwrap();
    h.state_mut()
        .set_block_text_for_test(bi, &format!("{original} REV"));
    h.run();

    assert!(h.state_mut().open_revision_summary());
    let bytes = h
        .state_mut()
        .summary_pdf_for_test()
        .expect("summary PDF must render");
    assert!(bytes.starts_with(b"%PDF-"), "not a PDF: {:?}", &bytes[..8]);
    assert!(bytes.len() > 2_000, "summary PDF looks empty");
}

/// The baseline selector can summarize an older revision, not just the
/// latest one.
#[test]
fn revision_summary_compares_an_older_baseline() {
    use scriptcraft_core::revision::RevisionColor;

    let mut h = harness(SideTab::Script);
    h.run();
    h.state_mut().start_revision();
    h.run();
    let bi = h
        .state()
        .find_first_heading_block()
        .expect("A95 should contain a scene heading");
    let original = h.state().block_text(bi).unwrap();
    h.state_mut()
        .set_block_text_for_test(bi, &format!("{original} BLUE"));
    h.run();

    h.state_mut().start_revision();
    h.run();
    let bj = bi + 1;
    let second = h.state().block_text(bj).unwrap();
    h.state_mut()
        .set_block_text_for_test(bj, &format!("{second} PINK"));
    h.run();

    assert!(h.state_mut().open_revision_summary());
    let latest = h.state().summary_for_test().expect("latest summary");
    assert_eq!(latest.color, RevisionColor::Pink);
    assert_eq!(latest.changed, 1, "Pink only contains the second edit");

    assert!(h.state_mut().set_summary_base_for_test(0));
    let older = h.state().summary_for_test().expect("older summary");
    assert_eq!(older.color, RevisionColor::Blue);
    assert!(
        older.changed > latest.changed,
        "Blue must include both edits: {older:?} vs {latest:?}"
    );
}

/// Restoring a revision writes a new project without touching the current
/// draft, then the restored copy opens with the same baseline.
#[test]
fn restore_revision_as_copy_leaves_original_and_opens_copy() {
    use scriptcraft_core::revision::RevisionColor;

    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/A95.kitsp");
    let dir = std::env::temp_dir().join(format!("scriptcraft-restore-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Restore.kitsp");
    std::fs::copy(fixture, &path).unwrap();
    let path_str = path.to_string_lossy().into_owned();

    let mut h = harness(SideTab::Script);
    h.run();
    h.state_mut().open_path(&path_str);
    h.run();

    let bi = h
        .state()
        .find_first_heading_block()
        .expect("A95 should contain a scene heading");
    let original = h.state().block_text(bi).unwrap();
    let edited = format!("{original} — BLUE");
    h.state_mut()
        .start_revision_at_for_test("Blue Revision", "28.10.2026 11:58");
    h.state_mut().set_block_text_for_test(bi, &edited);
    h.run();
    assert!(h.state().is_dirty());

    h.get_by_label("Menu").click();
    h.run();
    h.get_by_label("Save").click();
    h.run();
    assert!(!h.state().is_dirty(), "save must write the Blue draft");

    let source_before = std::fs::read(&path).expect("read source");
    let restored = dir.join("Restore-blue-2026-10-28.kitsp");
    let report = h
        .state_mut()
        .restore_revision_copy_for_test(0, &restored)
        .expect("restore blue as copy");
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("title-page") && w.contains("metadata")),
        "the metadata warning must be available to the UI: {:?}",
        report.warnings
    );
    assert_eq!(
        std::fs::read(&path).expect("read source again"),
        source_before,
        "the source project must stay byte-identical"
    );
    assert_eq!(
        h.state().block_text(bi).unwrap(),
        edited,
        "the current draft must not be mutated by the restore"
    );
    assert_eq!(h.state().project_path(), path_str);
    assert!(
        !h.state().is_dirty(),
        "restore must not dirty the current draft"
    );
    assert!(
        h.state()
            .restore_file_name_for_test(0)
            .unwrap()
            .starts_with("Restore-blue-"),
        "suggested filename carries the project and colour"
    );

    // The restored file opens with the baseline script and history.
    let restored_str = restored.to_string_lossy().into_owned();
    h.state_mut().open_path(&restored_str);
    h.run();
    assert_eq!(h.state().block_text(bi).unwrap(), original);
    assert_eq!(h.state().latest_revision_color(), Some(RevisionColor::Blue));
    assert_eq!(h.state().revision_count_for_test(), 1);

    // Comparing the restored baseline with itself is empty, and the compare
    // header offers the restore action.
    h.state_mut().open_compare();
    h.run();
    assert_eq!(
        h.state().compare_counts_for_test(),
        Some((0, 0, 0)),
        "restored baseline must match the restored draft"
    );
    assert!(h.query_by_label("Restore copy…").is_some());

    let _ = std::fs::remove_dir_all(&dir);
}

/// The scene context menu hides the body from pages and statistics while the
/// heading, scene number and identity stay; Restore scene brings it back.
/// FDX/Fountain exports name the unsupported omission instead of silently
/// dropping the scene.
#[test]
fn omitted_scene_menu_hides_body_stats_and_reports_export_loss() {
    let mut h = harness(SideTab::Script);
    h.run();

    let pages_before = h.state().page_count();
    let words_before = h.state().stats_for_test().words;
    let blocks_before = h.state().stats_for_test().blocks;
    let scenes_before = h.state().stats_for_test().scenes;
    let heading = h.state().scene_heading_block_for_test(0).expect("heading");
    let heading_text = h.state().block_text(heading).expect("heading text");

    h.get_by_label_contains(&heading_text).click_secondary();
    h.run();
    assert!(
        h.query_by_label("Omit scene").is_some(),
        "the scene context menu must offer Omit scene"
    );
    h.get_by_label("Omit scene").click();
    h.run();

    assert!(h.state().scene_omitted_for_test(0));
    assert_eq!(h.state().omitted_scene_count_for_test(), 1);
    assert_eq!(h.state().scene_count_for_test(), scenes_before);
    let hidden = h.state().hidden_block_indices_for_test();
    assert!(!hidden.is_empty(), "an omitted scene hides its body");
    assert!(!hidden.contains(&heading), "the heading stays visible");
    assert_eq!(h.state().hidden_block_count_for_test(), hidden.len());

    let stats = h.state().stats_for_test();
    assert_eq!(stats.scenes, scenes_before, "scene count stays intact");
    assert_eq!(stats.blocks, blocks_before - hidden.len());
    assert!(stats.words < words_before, "omitted words leave statistics");
    assert!(
        h.state().page_count() < pages_before,
        "body pages disappear"
    );

    let lines = h.state().doc_lines_for_test();
    assert!(
        lines
            .iter()
            .any(|(_, text, role)| text == "OMITTED" && role == "Omitted"),
        "the page prints a synthetic OMITTED row"
    );
    let body = hidden
        .iter()
        .find_map(|&bi| {
            let text = h.state().block_text(bi)?;
            (!text.trim().is_empty()).then_some(text)
        })
        .expect("omitted scene has body text");
    assert!(
        !lines
            .iter()
            .any(|(_, text, role)| role == "Body" && text == &body),
        "hidden body text must not print"
    );

    let fdx = h.state().omission_export_note_for_test("FDX");
    assert!(fdx.contains("Omission is unsupported in FDX"));
    assert!(fdx.contains("1 omitted scene"));
    let fountain = h.state().omission_export_note_for_test("Fountain");
    assert!(fountain.contains("Omission is unsupported in Fountain"));

    h.get_by_label_contains(&heading_text).click_secondary();
    h.run();
    assert!(
        h.query_by_label("Restore scene").is_some(),
        "the scene context menu must offer Restore scene while omitted"
    );
    h.get_by_label("Restore scene").click();
    h.run();

    assert!(!h.state().scene_omitted_for_test(0));
    assert_eq!(h.state().omitted_scene_count_for_test(), 0);
    assert_eq!(h.state().stats_for_test().words, words_before);
    assert_eq!(h.state().page_count(), pages_before);
    assert!(
        !h.state()
            .doc_lines_for_test()
            .iter()
            .any(|(_, text, role)| text == "OMITTED" && role == "Omitted"),
        "restoring removes the synthetic marker"
    );
}

/// The caret skips an omitted body and edits inside it are refused until the
/// scene is restored.
#[test]
fn editor_navigation_skips_omitted_body_and_refuses_hidden_edits() {
    let mut h = focused_harness();
    let heading = h.state().scene_heading_block_for_test(0).expect("heading");
    h.state_mut().set_scene_omitted_for_test(0, true);
    h.run();

    let hidden = h.state().hidden_block_indices_for_test();
    assert!(!hidden.is_empty());

    h.state_mut().focus_block_at(heading, 0);
    h.run();
    h.key_press(egui::Key::ArrowDown);
    h.run();
    let (_, focus) = h.state().editor2_selection().expect("caret");
    assert!(
        !hidden.contains(&focus.0),
        "arrow navigation must skip hidden body blocks; landed on {focus:?}"
    );

    let hidden_bi = hidden[0];
    let original = h.state().block_text(hidden_bi).expect("hidden body text");
    h.state_mut()
        .set_block_text_for_test(hidden_bi, "THIS MUST NOT LAND");
    h.run();
    assert_eq!(
        h.state().block_text(hidden_bi).as_deref(),
        Some(original.as_str()),
        "editing a hidden body block must be refused"
    );
    assert!(h.state().status_for_test().contains("omitted"));

    h.state_mut().set_scene_omitted_for_test(0, false);
    h.run();
    h.state_mut()
        .set_block_text_for_test(hidden_bi, "EDITED AFTER RESTORE");
    h.run();
    assert_eq!(
        h.state().block_text(hidden_bi).as_deref(),
        Some("EDITED AFTER RESTORE"),
        "restoring the scene makes its body editable again"
    );
}

/// Locked pages keep every scene heading on its production page across
/// omission, and both omission and the lock survive Save/reopen.
#[test]
fn omitted_scene_persists_with_page_locks_and_keeps_later_numbers() {
    use scriptcraft_core::kitsp;
    use scriptcraft_core::model::{ElementType, Project, ScriptBlock};

    fn project() -> Project {
        let mut blocks = Vec::new();
        for scene in 1..=4 {
            blocks.push(ScriptBlock::new(
                ElementType::SceneHeading,
                format!("INT. OMIT TEST {scene} - DAY"),
            ));
            for line in 0..40 {
                blocks.push(ScriptBlock::new(
                    ElementType::Action,
                    format!("Omit test scene {scene} action line {line}."),
                ));
            }
        }
        Project {
            title: "Omission Test".into(),
            blocks,
            scene_meta: Vec::new(),
        }
    }

    fn heading_pages(h: &Harness<'static, ScriptCraftApp>) -> Vec<(String, Option<String>)> {
        let lines = h.state().doc_lines_for_test();
        (1..=4)
            .map(|scene| {
                let heading = format!("INT. OMIT TEST {scene} - DAY");
                let page = lines
                    .iter()
                    .find(|(_, text, role)| role == "Body" && text == &heading)
                    .map(|(page, _, _)| page.clone())
                    .unwrap_or_else(|| panic!("heading {scene} must print"));
                (heading, page)
            })
            .collect()
    }

    let dir =
        std::env::temp_dir().join(format!("scriptcraft-omission-lock-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Omission.kitsp");
    let path_str = path.to_string_lossy().into_owned();
    kitsp::create_file(&path, "Omission Test").expect("create");
    kitsp::save_file(&path, &project()).expect("save project");

    let mut h = harness(SideTab::Script);
    h.run();
    h.state_mut().open_path(&path_str);
    h.run();

    h.state_mut().lock_pages_for_test();
    h.run();
    let lock = h.state().page_lock_for_test().expect("pages locked");
    let pages_before = h.state().page_count();
    let headings_before = heading_pages(&h);
    assert!(
        pages_before >= 4,
        "the synthetic script must span several pages"
    );

    h.state_mut().set_scene_omitted_for_test(1, true);
    h.run();
    assert!(h.state().scene_omitted_for_test(1));
    assert!(
        h.state().page_count() < pages_before,
        "body pages disappear"
    );
    assert_eq!(
        h.state().page_lock_for_test(),
        Some(lock.clone()),
        "omission must not rewrite the lock"
    );
    assert_eq!(
        heading_pages(&h),
        headings_before,
        "later headings keep their locked page numbers"
    );

    h.get_by_label("Menu").click();
    h.run();
    h.get_by_label("Save").click();
    h.run();
    assert!(!h.state().is_dirty());

    let mut reopened = harness(SideTab::Script);
    reopened.run();
    reopened.state_mut().open_path(&path_str);
    reopened.run();
    assert!(
        reopened.state().scene_omitted_for_test(1),
        "omission survives Save/reopen"
    );
    assert_eq!(reopened.state().omitted_scene_count_for_test(), 1);
    assert_eq!(
        reopened.state().page_lock_for_test(),
        Some(lock),
        "the production lock survives Save/reopen"
    );
    assert_eq!(
        heading_pages(&reopened),
        headings_before,
        "reopened pages still use the locked numbers"
    );
    assert!(reopened.state().page_count() < pages_before);

    // Restoring and saving brings the body and the original page count back.
    reopened.state_mut().set_scene_omitted_for_test(1, false);
    reopened.run();
    reopened.get_by_label("Menu").click();
    reopened.run();
    reopened.get_by_label("Save").click();
    reopened.run();

    let mut restored = harness(SideTab::Script);
    restored.run();
    restored.state_mut().open_path(&path_str);
    restored.run();
    assert!(!restored.state().scene_omitted_for_test(1));
    assert_eq!(restored.state().omitted_scene_count_for_test(), 0);
    assert!(restored.state().hidden_block_indices_for_test().is_empty());
    assert_eq!(restored.state().page_count(), pages_before);

    let _ = std::fs::remove_dir_all(&dir);
}

/// Every title-page field writes through the normal metadata path and comes
/// back after reopening; the preview paints the PDF exporter's own lines.
#[test]
fn title_page_edits_save_and_reopen() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/A95.kitsp");
    let dir = std::env::temp_dir().join(format!("scriptcraft-titlepage-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("TitlePage.kitsp");
    std::fs::copy(fixture, &path).unwrap();
    let path_str = path.to_string_lossy().into_owned();

    let mut h = harness(SideTab::Research);
    h.run();
    h.state_mut().open_path(&path_str);
    h.state_mut().goto_tab(SideTab::Research);
    h.state_mut().set_research_pane(ResearchPane::TitlePage);
    h.run();
    assert!(h.query_by_label("Title Page").is_some());
    assert!(h.query_by_label("Script Page Header / Footer").is_some());

    h.state_mut()
        .set_title_field_for_test("title", "THE LAST RUN");
    h.state_mut().set_title_field_for_test("genre", "Thriller");
    h.state_mut()
        .set_title_field_for_test("author", "Mihail Mihaylov");
    h.state_mut()
        .set_title_field_for_test("source", "Based on a true story");
    h.state_mut()
        .set_title_field_for_test("contact", "writer@example.com\n+359 2 123 4567");
    h.state_mut()
        .set_title_field_for_test("draft_date", "09.11.2026");
    h.state_mut()
        .set_title_field_for_test("header", "{revision}");
    h.state_mut().set_title_field_for_test("footer", "{page}");
    assert!(
        h.state().meta_dirty_for_test(),
        "field edits mark the title page dirty before the flush"
    );
    h.run();

    let preview = h.state().title_page_preview_for_test();
    for text in [
        "THE LAST RUN",
        "Thriller",
        "Mihail Mihaylov",
        "Based on a true story",
        "writer@example.com",
        "+359 2 123 4567",
        "09.11.2026",
    ] {
        assert!(
            preview.iter().any(|l| l.text == text),
            "preview is missing {text:?}: {preview:?}"
        );
    }

    assert!(h.state_mut().save_meta_for_test(), "title page must save");
    h.run();
    assert!(!h.state().is_dirty());

    let mut reopened = harness(SideTab::Research);
    reopened.run();
    reopened.state_mut().open_path(&path_str);
    reopened.run();
    let m = reopened.state().title_meta_for_test();
    assert_eq!(m.title, "THE LAST RUN");
    assert_eq!(m.genre, "Thriller");
    assert_eq!(m.author, "Mihail Mihaylov");
    assert_eq!(m.additional_info, "Based on a true story");
    assert_eq!(m.contacts, "writer@example.com\n+359 2 123 4567");
    assert_eq!(m.year, "09.11.2026");
    assert_eq!(m.header, "{revision}");
    assert_eq!(m.footer, "{page}");

    let _ = std::fs::remove_dir_all(&dir);
}

/// Untouched KIT logline/synopsis rich text must survive a save byte-for-byte;
/// editing one buffer rewrites only that buffer (and as plain text).
#[test]
fn untouched_kit_rich_text_is_not_rewritten() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/A95.kitsp");
    let dir = std::env::temp_dir().join(format!("scriptcraft-titlehtml-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("TitleHtml.kitsp");
    std::fs::copy(fixture, &path).unwrap();
    let path_str = path.to_string_lossy().into_owned();

    let before = scriptcraft_core::kitsp::load_metadata(&path_str).unwrap();
    assert!(
        before.logline_html.contains("<p style="),
        "the fixture must carry KIT's rich HTML"
    );
    assert!(before.synopsis_html.contains("<p style="));

    let mut h = harness(SideTab::Research);
    h.run();
    h.state_mut().open_path(&path_str);
    h.state_mut().goto_tab(SideTab::Research);
    h.state_mut().set_research_pane(ResearchPane::TitlePage);
    h.run();
    // Change a different field only: the rich text must not be touched.
    h.state_mut()
        .set_title_field_for_test("title", "A95 DIRECTOR'S CUT");
    assert!(h.state_mut().save_meta_for_test());
    let after = scriptcraft_core::kitsp::load_metadata(&path_str).unwrap();
    assert_eq!(after.title, "A95 DIRECTOR'S CUT");
    assert_eq!(after.logline_html, before.logline_html);
    assert_eq!(after.synopsis_html, before.synopsis_html);

    // Editing the logline buffer rewrites that field, and only that field.
    h.state_mut()
        .set_title_buffer_for_test(false, "A new logline.");
    h.run();
    assert!(h.state_mut().save_meta_for_test());
    let edited = scriptcraft_core::kitsp::load_metadata(&path_str).unwrap();
    assert_eq!(
        edited.logline_html,
        scriptcraft_core::kitsp::text_to_html("A new logline.")
    );
    assert_eq!(edited.synopsis_html, before.synopsis_html);

    let _ = std::fs::remove_dir_all(&dir);
}

/// Cyrillic, empty and over-long fields stay on the title page, and the
/// metadata survives a save/reopen round trip.
#[test]
fn title_page_handles_cyrillic_empty_and_long_fields() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/A95.kitsp");
    let dir = std::env::temp_dir().join(format!("scriptcraft-titleuni-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("TitleUnicode.kitsp");
    std::fs::copy(fixture, &path).unwrap();
    let path_str = path.to_string_lossy().into_owned();

    let mut h = harness(SideTab::Research);
    h.run();
    h.state_mut().open_path(&path_str);
    h.state_mut().goto_tab(SideTab::Research);
    h.state_mut().set_research_pane(ResearchPane::TitlePage);
    h.state_mut()
        .set_title_field_for_test("title", "ЗАГЛАВИЕ НА ФИЛМ");
    h.state_mut().set_title_field_for_test("genre", "");
    h.state_mut().set_title_field_for_test("author", "");
    h.state_mut().set_title_field_for_test("source", "");
    h.state_mut()
        .set_title_field_for_test("contact", "ул. Примерна 1\nСофия");
    h.run();
    let preview = h.state().title_page_preview_for_test();
    assert!(preview.iter().any(|l| l.text == "ЗАГЛАВИЕ НА ФИЛМ"));
    assert!(preview.iter().any(|l| l.text == "ул. Примерна 1"));
    assert!(preview.iter().any(|l| l.text == "София"));
    assert!(
        preview.iter().all(|l| l.text != "Thriller/Horror"),
        "cleared fields must leave the page"
    );

    assert!(h.state_mut().save_meta_for_test());
    let mut reopened = harness(SideTab::Research);
    reopened.run();
    reopened.state_mut().open_path(&path_str);
    reopened.run();
    let m = reopened.state().title_meta_for_test();
    assert_eq!(m.title, "ЗАГЛАВИЕ НА ФИЛМ");
    assert_eq!(m.author, "");
    assert_eq!(m.contacts, "ул. Примерна 1\nСофия");

    // A title longer than the 58-character column wraps instead of running
    // off the page, still underlined as the title.
    reopened.state_mut().set_title_field_for_test(
        "title",
        "THE EXTRAORDINARILY LONG TITLE OF A FILM THAT KEEPS GOING AND GOING",
    );
    reopened.run();
    let preview = reopened.state().title_page_preview_for_test();
    let underlined = preview.iter().filter(|l| l.underline).count();
    assert_eq!(underlined, 2, "long title must wrap in the preview too");

    let _ = std::fs::remove_dir_all(&dir);
}

/// The form itself accepts typed text (not just the test setter).
#[test]
fn title_page_form_accepts_typed_text() {
    use egui::accesskit::Role;

    let mut h = harness(SideTab::Research);
    h.run();
    h.state_mut().set_research_pane(ResearchPane::TitlePage);
    h.run();

    let title = h
        .get_all_by_role(Role::TextInput)
        .find(|n| n.value().as_deref() == Some("A95"))
        .expect("the title field should show the KIT title");
    title.focus();
    h.run();
    h.get_all_by_role(Role::TextInput)
        .find(|n| n.value().as_deref() == Some("A95"))
        .expect("the title field should still be there")
        .type_text("!");
    h.run();

    let title = &h.state().title_meta_for_test().title;
    assert!(
        title.contains('!') && title.contains("A95"),
        "typing in the form must edit the title: {title:?}"
    );
}
