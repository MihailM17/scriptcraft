//! End-to-end interaction tests (kittest): simulated clicks and keystrokes.

use egui_kittest::kittest::{NodeT, Queryable};
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

/// Enter at a block end splits with the KIT continuation type; Backspace at
/// the new block's start merges it straight back.
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
fn revision_menu_starts_next_color() {
    use scriptcraft_core::revision::RevisionColor;

    let mut h = harness(SideTab::Script);
    h.run();
    h.get_by_label("Menu").click();
    h.run();
    assert!(
        h.query_by_label("Start Blue Revision").is_some(),
        "first revision must be offered as Blue"
    );
    h.get_by_label("Start Blue Revision").click();
    h.run();
    assert_eq!(h.state().latest_revision_color(), Some(RevisionColor::Blue));

    h.get_by_label("Menu").click();
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

    h.get_by_label("Menu").click();
    h.run();
    h.get_by_label("Revision History…").click();
    h.run();
    assert!(h.query_by_label("Revision History").is_some());
    assert!(h.query_by_label("Blue Revision").is_some());
    assert!(h.query_by_label("current").is_some());

    h.get_by_label("Show current revision only").click();
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
