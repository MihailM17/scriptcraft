//! Off-screen snapshots of every view with the A95 fixture loaded.
//!
//! First run (or after intentional visual changes):
//!     UPDATE_SNAPSHOTS=1 cargo test -p scriptcraft-app --test ui_snapshots
//! PNGs land in `scriptcraft-app/tests/snapshots/`.

use egui_kittest::Harness;
use scriptcraft_app::app::{self, ScriptCraftApp, SideTab};

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
