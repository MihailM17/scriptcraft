//! Off-screen snapshots of every view with the A95 fixture loaded.
//!
//! First run (or after intentional visual changes):
//!     UPDATE_SNAPSHOTS=1 cargo test -p scriptcraft-app --test ui_snapshots
//! PNGs land in `scriptcraft-app/tests/snapshots/`.

use egui_kittest::Harness;
use scriptcraft_app::app::{self, ResearchPane, ScriptCraftApp, SideTab};

fn harness(tab: SideTab, scroll_block: Option<usize>) -> Harness<'static, ScriptCraftApp> {
    Harness::builder()
        .with_size(egui::Vec2::new(1360.0, 880.0))
        .build_eframe(move |cc| {
            app::apply_theme(&cc.egui_ctx);
            app::install_fonts(&cc.egui_ctx);
            let mut a = ScriptCraftApp::default();
            assert!(a.open_fixture(), "A95 fixture not found");
            a.pin_stats_generated("18.10.2026 16:03:47");
            a.pin_backups_dir("/Users/test/Documents/ScriptCraft/backups");
            a.goto_tab(tab);
            if let Some(bi) = scroll_block {
                a.scroll_to_block(bi);
            }
            a
        })
}

#[test]
fn snap_01_script_top() {
    let mut h = harness(SideTab::Script, None);
    h.run();
    h.snapshot("01_script_top");
}

#[test]
fn snap_02_script_mid() {
    let mut h = harness(SideTab::Script, Some(150));
    h.run();
    h.snapshot("02_script_mid");
}

#[test]
fn snap_03_research() {
    let mut h = harness(SideTab::Research, None);
    h.run();
    h.snapshot("03_research");
}

#[test]
fn snap_04_cards() {
    let mut h = harness(SideTab::Cards, None);
    h.run();
    h.snapshot("04_cards");
}

#[test]
fn snap_05_statistics() {
    let mut h = harness(SideTab::Statistics, None);
    h.run();
    h.snapshot("05_statistics");
}

#[test]
fn snap_06_start() {
    let mut h = Harness::builder()
        .with_size(egui::Vec2::new(1360.0, 880.0))
        .build_eframe(|cc| {
            app::apply_theme(&cc.egui_ctx);
            app::install_fonts(&cc.egui_ctx);
            let mut a = ScriptCraftApp::default();
            a.pin_recent(vec![
                (
                    "/Users/mihailmihaylov/Documents/Writing_Scripts/KITScenarist/A95.kitsp"
                        .to_string(),
                    "18.10.2026 15:37".to_string(),
                ),
                (
                    "/Users/mihailmihaylov/Documents/Writing_Scripts/KITScenarist/Untitled.kitsp"
                        .to_string(),
                    "12.10.2026 09:12".to_string(),
                ),
            ]);
            a.goto_tab(SideTab::Start);
            a
        });
    h.run();
    h.snapshot("06_start");
}

#[test]
fn snap_07_settings() {
    let mut h = harness(SideTab::Settings, None);
    h.run();
    h.snapshot("07_settings");
}

#[test]
fn snap_16_settings_editor() {
    use egui_kittest::kittest::Queryable;
    let mut h = harness(SideTab::Settings, None);
    h.run();
    h.get_by_label("Scenario Editor").click();
    h.run();
    h.snapshot("16_settings_editor");
}

#[test]
fn snap_17_settings_navigator() {
    use egui_kittest::kittest::Queryable;
    let mut h = harness(SideTab::Settings, None);
    h.run();
    h.get_by_label("Navigator").click();
    h.run();
    h.snapshot("17_settings_navigator");
}

#[test]
fn snap_19_title_page() {
    let mut h = harness(SideTab::Research, None);
    h.state_mut().set_research_pane(ResearchPane::TitlePage);
    h.run();
    h.snapshot("19_title_page");
}

/// Revision summary after edits in two scenes, grouped by scene with totals.
#[test]
fn snap_20_revision_summary() {
    use scriptcraft_core::model::ElementType;

    let mut h = harness(SideTab::Research, None);
    h.state_mut().set_research_pane(ResearchPane::Versions);
    h.state_mut()
        .start_revision_at_for_test("Blue Revision", "10.11.2026 10:00");
    let first = h
        .state()
        .find_first_heading_block()
        .expect("A95 should contain a scene heading");
    let second = (first + 1..h.state().block_count())
        .find(|&bi| h.state().block_element_type(bi) == Some(ElementType::SceneHeading))
        .expect("A95 has more than one scene");
    for bi in [first, second] {
        let text = h.state().block_text(bi).unwrap();
        h.state_mut()
            .set_block_text_for_test(bi, &format!("{text} — REVISED"));
    }
    h.run();
    assert!(h.state_mut().open_revision_summary());
    h.run();
    h.snapshot("20_revision_summary");
}

#[test]
fn snap_08_export_dialog() {
    use egui_kittest::kittest::Queryable;
    let mut h = harness(SideTab::Script, None);
    h.run();
    h.get_by_label("Menu").click();
    h.run();
    h.get_by_label("Export…").click();
    h.run();
    // Reveal the options panel (KIT's "Additional" checkbox). Widgets in a
    // floating Window don't get simulated pointer clicks, use accesskit.
    h.get_by_label("Additional (Numbering, Title Page etc.)")
        .click_accesskit();
    h.run();
    // The accesskit click leaves the synthetic pointer over the navigator;
    // clear it so the snapshot shows the dialog, not an incidental hover.
    h.input_mut().events.push(egui::Event::PointerGone);
    h.run();
    h.snapshot("08_export_dialog");
}

#[test]
fn snap_09_script_light() {
    let mut h = Harness::builder()
        .with_size(egui::Vec2::new(1360.0, 880.0))
        .build_eframe(|cc| {
            app::apply_theme(&cc.egui_ctx);
            app::install_fonts(&cc.egui_ctx);
            let mut a = ScriptCraftApp::default();
            assert!(a.open_fixture(), "A95 fixture not found");
            a.set_dark_mode(false);
            a.goto_tab(SideTab::Script);
            a
        });
    h.run();
    h.snapshot("09_script_light");
}

#[test]
fn snap_10_research_character_photo() {
    let photo: &[u8] = include_bytes!("data/test_photo.png");
    let mut h = Harness::builder()
        .with_size(egui::Vec2::new(1360.0, 880.0))
        .build_eframe(|cc| {
            app::apply_theme(&cc.egui_ctx);
            app::install_fonts(&cc.egui_ctx);
            let mut a = ScriptCraftApp::default();
            assert!(a.open_fixture(), "A95 fixture not found");
            a.goto_tab(SideTab::Research);
            assert!(a.research_photo_preview_for_test("ИВАН", photo));
            a
        });
    h.run();
    h.snapshot("10_research_character_photo");
}

/// Selection highlight and caret on the first page.
#[test]
fn snap_12_script_selection() {
    let mut h = harness(SideTab::Script, None);
    h.run();
    h.run_steps(2);

    // Click into the page to focus the editor (focus draws the caret).
    let page = h.state().editor2_page_rects()[0];
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

    let bi = h.state().find_first_heading_block().expect("heading");
    h.state_mut()
        .editor2_set_selection((bi, 0, 0), (bi + 1, 0, 6));
    h.run();
    h.snapshot("12_script_selection");
}

#[test]
fn snap_13_revision_marks() {
    let mut h = harness(SideTab::Script, None);
    h.run();
    let page = h.state().editor2_page_rects()[0];
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

    let bi = h.state().find_first_heading_block().expect("heading");
    h.state_mut().start_revision();
    h.state_mut().focus_block_at(bi, 0);
    h.run_steps(2);
    h.input_mut().events.push(egui::Event::Text("REV ".into()));
    h.run();

    h.state_mut().start_revision();
    h.state_mut().focus_block_at(bi + 1, 0);
    h.run_steps(2);
    h.input_mut().events.push(egui::Event::Text("REV ".into()));
    h.run();
    h.snapshot("13_revision_marks");
}

/// Compare workspace: latest revision against the current draft.
#[test]
fn snap_14_compare_drafts() {
    let mut h = harness(SideTab::Script, None);
    h.run();
    h.state_mut()
        .start_revision_at_for_test("Blue Revision", "28.10.2026 11:58");
    h.run();

    let bi = h.state().find_first_heading_block().expect("heading");
    let heading = h.state().block_text(bi).unwrap();
    let action = h.state().block_text(bi + 1).unwrap();
    h.state_mut()
        .set_block_text_for_test(bi, &format!("{heading} — REVISED"));
    h.state_mut()
        .set_block_text_for_test(bi + 1, &format!("{action} (revised action)"));
    h.state_mut().open_compare();
    h.run();
    h.run_steps(2);
    h.snapshot("14_compare_drafts");
}

/// Compare workspace side-by-side mode.
#[test]
fn snap_15_compare_side_by_side() {
    let mut h = harness(SideTab::Script, None);
    h.run();
    h.state_mut()
        .start_revision_at_for_test("Blue Revision", "28.10.2026 11:58");
    h.run();

    let bi = h.state().find_first_heading_block().expect("heading");
    let heading = h.state().block_text(bi).unwrap();
    let action = h.state().block_text(bi + 1).unwrap();
    h.state_mut()
        .set_block_text_for_test(bi, &format!("{heading} — REVISED"));
    h.state_mut()
        .set_block_text_for_test(bi + 1, &format!("{action} (revised action)"));
    h.state_mut().open_compare();
    h.run();
    h.run_steps(2);
    h.key_press(egui::Key::V);
    h.run();
    h.snapshot("15_compare_side_by_side");
}

/// Compare workspace in the light theme: row tints, marker badges and the
/// side-by-side divider must stay legible on white surfaces.
#[test]
fn snap_21_compare_light() {
    let mut h = Harness::builder()
        .with_size(egui::Vec2::new(1360.0, 880.0))
        .build_eframe(|cc| {
            app::apply_theme(&cc.egui_ctx);
            app::install_fonts(&cc.egui_ctx);
            let mut a = ScriptCraftApp::default();
            assert!(a.open_fixture(), "A95 fixture not found");
            a.set_dark_mode(false);
            a.goto_tab(SideTab::Script);
            a
        });
    h.run();
    h.state_mut()
        .start_revision_at_for_test("Blue Revision", "28.10.2026 11:58");
    h.run();

    let bi = h.state().find_first_heading_block().expect("heading");
    let heading = h.state().block_text(bi).unwrap();
    let action = h.state().block_text(bi + 1).unwrap();
    h.state_mut()
        .set_block_text_for_test(bi, &format!("{heading} — REVISED"));
    h.state_mut()
        .set_block_text_for_test(bi + 1, &format!("{action} (revised action)"));
    h.state_mut().open_compare();
    h.run();
    h.run_steps(2);
    h.snapshot("21_compare_light");
}

/// Identical drafts: the workspace replaces an empty diff with a clear
/// "No differences" state and a way into the full script.
#[test]
fn snap_22_compare_identical() {
    let mut h = harness(SideTab::Script, None);
    h.run();
    h.state_mut()
        .start_revision_at_for_test("Blue Revision", "28.10.2026 11:58");
    h.run();
    h.state_mut().open_compare();
    h.run();
    h.run_steps(2);
    h.snapshot("22_compare_identical");
}

/// A real dual-dialogue pair rendered side by side in the editor.
#[test]
fn snap_18_dual_dialogue() {
    let mut h = Harness::builder()
        .with_size(egui::Vec2::new(1360.0, 880.0))
        .build_eframe(|cc| {
            app::apply_theme(&cc.egui_ctx);
            app::install_fonts(&cc.egui_ctx);
            let mut a = ScriptCraftApp::default();
            assert!(a.open_fixture(), "A95 fixture not found");
            a.goto_tab(SideTab::Script);
            a.focus_first_dual_candidate_for_test()
                .expect("A95 contains two adjacent speech units");
            a
        });
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::D);
    h.run_steps(2);
    h.snapshot("18_dual_dialogue");
}

/// A second dark palette. Proves the theme table reaches the page, the
/// margins, the panels and the revision bars, not just the settings chrome.
#[test]
fn snap_23_script_tokyo_night() {
    use scriptcraft_app::theme::Theme;

    let mut h = Harness::builder()
        .with_size(egui::Vec2::new(1360.0, 880.0))
        .build_eframe(|cc| {
            app::apply_theme(&cc.egui_ctx);
            app::install_fonts(&cc.egui_ctx);
            let mut a = ScriptCraftApp::default();
            assert!(a.open_fixture(), "A95 fixture not found");
            a.set_theme(Theme::TokyoNight);
            a.goto_tab(SideTab::Script);
            a
        });
    h.run();
    h.snapshot("23_script_tokyo_night");
}

/// A second light palette: warm paper, where the pale revision colours are
/// hardest to keep visible.
#[test]
fn snap_24_script_sepia() {
    use scriptcraft_app::theme::Theme;

    let mut h = Harness::builder()
        .with_size(egui::Vec2::new(1360.0, 880.0))
        .build_eframe(|cc| {
            app::apply_theme(&cc.egui_ctx);
            app::install_fonts(&cc.egui_ctx);
            let mut a = ScriptCraftApp::default();
            assert!(a.open_fixture(), "A95 fixture not found");
            a.set_theme(Theme::Sepia);
            a.goto_tab(SideTab::Script);
            a
        });
    h.run();
    h.snapshot("24_script_sepia");
}

/// Bulgarian spellchecking: unknown words carry the palette's squiggle
/// colour, so the mark stays readable on every theme.
#[test]
fn snap_25_spelling_marks() {
    use scriptcraft_app::spell::SpellLang;

    let mut h = harness(SideTab::Script, None);
    h.run();
    let bi = h.state().find_first_heading_block().expect("heading") + 1;
    h.state_mut().set_spellcheck_for_test(true, SpellLang::BgBg);
    h.state_mut()
        .set_block_text_for_test(bi, "Иван откри грешкаф в сценария и кюфтеф на масата.");
    h.state_mut().scroll_to_block(bi);
    h.run_steps(2);
    h.snapshot("25_spelling_marks");
}
