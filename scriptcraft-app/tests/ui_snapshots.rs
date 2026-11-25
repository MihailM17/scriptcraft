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
fn snap_16_settings_appearance() {
    use egui_kittest::kittest::Queryable;
    let mut h = harness(SideTab::Settings, None);
    h.run();
    h.get_by_label("Appearance").click();
    h.run();
    h.snapshot("16_settings_appearance");
}

#[test]
fn snap_17_settings_writing() {
    use egui_kittest::kittest::Queryable;
    let mut h = harness(SideTab::Settings, None);
    h.run();
    h.get_by_label("Writing").click();
    h.run();
    h.snapshot("17_settings_writing");
}

#[test]
fn snap_32_settings_spelling() {
    use egui_kittest::kittest::Queryable;
    let mut h = harness(SideTab::Settings, None);
    h.run();
    h.get_by_label("Spelling").click();
    h.run();
    h.snapshot("32_settings_spelling");
}

#[test]
fn snap_33_settings_saving() {
    use egui_kittest::kittest::Queryable;
    let mut h = harness(SideTab::Settings, None);
    h.run();
    h.get_by_label("Saving").click();
    h.run();
    h.snapshot("33_settings_saving");
}

/// The sidebar while a query is typed: matching rows with their page names.
#[test]
fn snap_34_settings_search() {
    use egui::accesskit::Role;
    use egui_kittest::kittest::Queryable;

    let mut h = harness(SideTab::Settings, None);
    h.run();
    let field = h
        .get_all_by_role(Role::TextInput)
        .next()
        .expect("search box");
    field.focus();
    h.run();
    h.get_all_by_role(Role::TextInput)
        .next()
        .expect("search box")
        .type_text("save");
    h.run();
    h.snapshot("34_settings_search");
}

/// Arming the reset shows the confirmation card instead of resetting.
#[test]
fn snap_35_settings_reset_confirm() {
    use egui_kittest::kittest::Queryable;

    let mut h = harness(SideTab::Settings, None);
    h.run();
    h.get_by_label("Reset Settings…").click();
    h.run();
    h.snapshot("35_settings_reset_confirm");
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

/// Monochrome paper: the page is white with black ink while the panels keep
/// the Midnight palette, and revision marks stay coloured.
#[test]
fn snap_26_script_mono_paper() {
    let mut h = Harness::builder()
        .with_size(egui::Vec2::new(1360.0, 880.0))
        .build_eframe(|cc| {
            app::apply_theme(&cc.egui_ctx);
            app::install_fonts(&cc.egui_ctx);
            let mut a = ScriptCraftApp::default();
            assert!(a.open_fixture(), "A95 fixture not found");
            a.pin_stats_generated("18.10.2026 16:03:47");
            a.pin_backups_dir("/Users/test/Documents/ScriptCraft/backups");
            a.set_paper_mono_for_test(true);
            a.goto_tab(SideTab::Script);
            a
        });
    h.run();
    assert!(h.state().paper_mono_for_test());
    h.snapshot("26_script_mono_paper");
}

/// Tab in an empty character cue opens the picker over the page.
#[test]
fn snap_27_character_picker() {
    use scriptcraft_core::edit::EditCommand;

    let mut h = harness(SideTab::Script, None);
    h.run();
    // Focus the editor the way a writer does: click the paper.
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

    let bi = h.state().find_first_cue_block().expect("A95 has cues");
    h.state_mut()
        .apply_editor_commands_for_test(vec![EditCommand::SetText {
            bi,
            text: String::new(),
        }]);
    h.state_mut().focus_block_at(bi, 0);
    h.run_steps(2);
    h.key_press(egui::Key::Tab);
    h.run();
    assert!(
        !h.state().editor2_autocomplete_rows().is_empty(),
        "the picker must be open for the snapshot"
    );
    h.snapshot("27_character_picker");
}

/// Research ▸ Characters lists the entities the script already contains.
#[test]
fn snap_28_research_characters() {
    let mut h = harness(SideTab::Research, None);
    h.run();
    h.state_mut().set_research_pane(ResearchPane::Characters);
    h.run_steps(2);
    h.snapshot("28_research_characters");
}

/// KIT Scenarist: near-white chrome, black ink, light paper.
#[test]
fn snap_29_script_kit_scenarist() {
    use scriptcraft_app::theme::Theme;

    let mut h = Harness::builder()
        .with_size(egui::Vec2::new(1360.0, 880.0))
        .build_eframe(|cc| {
            app::apply_theme(&cc.egui_ctx);
            app::install_fonts(&cc.egui_ctx);
            let mut a = ScriptCraftApp::default();
            assert!(a.open_fixture(), "A95 fixture not found");
            a.pin_stats_generated("18.10.2026 16:03:47");
            a.pin_backups_dir("/Users/test/Documents/ScriptCraft/backups");
            a.set_theme(Theme::KitScenarist);
            a.goto_tab(SideTab::Script);
            a
        });
    h.run();
    h.snapshot("29_script_kit_scenarist");
}

/// Settings ▸ Theme: one dropdown, two family headings, a solid scrollbar.
#[test]
fn snap_30_settings_theme_menu() {
    use egui_kittest::kittest::{NodeT, Queryable};

    let mut h = harness(SideTab::Settings, None);
    h.run();
    h.get_by_label("Appearance").click();
    h.run();
    h.get_all_by_role(egui::accesskit::Role::ComboBox)
        .find(|n| !n.accesskit_node().is_disabled())
        .expect("theme chooser should be an enabled combo box")
        .click();
    h.run();
    h.snapshot("30_settings_theme_menu");
}

/// KIT Scenarist Dark: #3d3d3d paper, #ebebeb ink, purple folder accent.
#[test]
fn snap_36_script_kit_scenarist_dark() {
    use scriptcraft_app::theme::Theme;

    let mut h = Harness::builder()
        .with_size(egui::Vec2::new(1360.0, 880.0))
        .build_eframe(|cc| {
            app::apply_theme(&cc.egui_ctx);
            app::install_fonts(&cc.egui_ctx);
            let mut a = ScriptCraftApp::default();
            assert!(a.open_fixture(), "A95 fixture not found");
            a.pin_stats_generated("18.10.2026 16:03:47");
            a.pin_backups_dir("/Users/test/Documents/ScriptCraft/backups");
            a.set_theme(Theme::KitScenaristDark);
            a.goto_tab(SideTab::Script);
            a
        });
    h.run();
    h.snapshot("36_script_kit_scenarist_dark");
}

/// The on-open update prompt, with the injected version (no network).
#[test]
fn snap_31_update_prompt() {
    use scriptcraft_app::update::{Check, Release};

    let mut h = harness(SideTab::Script, None);
    h.run();
    h.state_mut()
        .set_update_check_for_test(Check::Available(Release::without_assets(
            "0.8.9",
            "https://github.com/MihailM17/scriptcraft/releases/tag/v0.8.9",
        )));
    h.run_steps(2);
    h.snapshot("31_update_prompt");
}

/// Redesigned Menu button popup with uppercase sections and hairlines.
#[test]
fn snap_37_menu_popup() {
    use egui_kittest::kittest::Queryable;

    let mut h = harness(SideTab::Script, None);
    h.run();
    h.get_by_label("Menu").click();
    h.run();
    h.snapshot("37_menu_popup");
}

/// Redesigned About ScriptCraft window.
#[test]
fn snap_38_about_dialog() {
    use egui_kittest::kittest::Queryable;

    let mut h = harness(SideTab::Script, None);
    h.run();
    h.get_by_label("Menu").click();
    h.run();
    h.get_by_label("About ScriptCraft").click();
    h.run();
    h.input_mut().events.push(egui::Event::PointerGone);
    h.run();
    h.snapshot("38_about_dialog");
}

/// Inline Bold, Italic, and Underline painted on the screenplay page with
/// active B / I / U toolbar toggles.
#[test]
fn snap_39_inline_styles() {
    use scriptcraft_core::edit::EditCommand;
    use scriptcraft_core::model::{InlineStyle, StyleSpan};

    let mut h = harness(SideTab::Script, None);
    h.run();
    let bi = h.state().find_first_heading_block().expect("heading") + 1;
    let len = h.state().block_text(bi).unwrap().chars().count();
    let a = (len / 4).max(4).min(len);
    let b = (len / 2).max(a + 4).min(len);
    let c = ((len * 3) / 4).max(b + 4).min(len);
    h.state_mut()
        .apply_editor_commands_for_test(vec![EditCommand::SetStyles {
            bi,
            styles: vec![
                StyleSpan {
                    start: 0,
                    end: a,
                    style: InlineStyle::Bold,
                },
                StyleSpan {
                    start: a,
                    end: b,
                    style: InlineStyle::Italic,
                },
                StyleSpan {
                    start: b,
                    end: c,
                    style: InlineStyle::Underline,
                },
            ],
        }]);
    h.state_mut().editor2_set_selection((bi, 0, 0), (bi, 0, a));
    h.run();
    h.snapshot("39_inline_styles");
}

fn seed_demo_shots(app: &mut ScriptCraftApp) -> (uuid::Uuid, uuid::Uuid) {
    use scriptcraft_core::shots::{ShotColor, ShotMovement, ShotRange, ShotSize};

    let (scenes_data, block_ids): (Vec<(uuid::Uuid, Vec<usize>)>, Vec<uuid::Uuid>) = {
        let p = app.project_for_test().expect("A95 loaded");
        let scs = p.scenes().into_iter().map(|s| (s.id, s.blocks)).collect();
        let bids = p.blocks.iter().map(|b| b.id).collect();
        (scs, bids)
    };

    let shots = app.shots_mut_for_test().expect("shots list");
    let (s0_id, s0_blocks) = &scenes_data[0];
    let pick = |blocks: &[usize], idx: usize| block_ids[blocks[idx.min(blocks.len() - 1)]];

    let id_1a = shots.add_shot(
        Some(*s0_id),
        "1",
        ShotSize::Ws,
        ShotMovement::Dolly,
        "28mm".into(),
        "Wide establishing master across the parking lot".into(),
        ShotRange::blocks(pick(s0_blocks, 1), pick(s0_blocks, 5)),
    );
    if let Some(s) = shots.get_mut(id_1a) {
        s.color = ShotColor::Cobalt;
    }

    // Partial mid-paragraph phrase selection inside block 2 into block 4
    let id_1b = shots.add_shot(
        Some(*s0_id),
        "1",
        ShotSize::Mcu,
        ShotMovement::Static,
        "50mm".into(),
        "Medium close-up cutting in on carburetor action".into(),
        ShotRange::chars(pick(s0_blocks, 2), 45, pick(s0_blocks, 4), 6),
    );
    if let Some(s) = shots.get_mut(id_1b) {
        s.color = ShotColor::Emerald;
    }

    // Mid-dialogue cut inside block 5 into block 6
    let id_1c = shots.add_shot(
        Some(*s0_id),
        "1",
        ShotSize::Cu,
        ShotMovement::Handheld,
        "85mm".into(),
        "Tight close-up cutting mid-speech on spark warning".into(),
        ShotRange::chars(pick(s0_blocks, 5), 76, pick(s0_blocks, 6), 34),
    );
    if let Some(s) = shots.get_mut(id_1c) {
        s.color = ShotColor::Amber;
    }

    // Tight prop insert on just the last phrase of block 6
    let id_1d = shots.add_shot(
        Some(*s0_id),
        "1",
        ShotSize::Insert,
        ShotMovement::Static,
        "85mm".into(),
        "Insert detail on car door".into(),
        ShotRange::chars(pick(s0_blocks, 6), 22, pick(s0_blocks, 6), 54),
    );
    if let Some(s) = shots.get_mut(id_1d) {
        s.color = ShotColor::Crimson;
    }

    if let Some((s1_id, s1_blocks)) = scenes_data.get(1) {
        if s1_blocks.len() >= 3 {
            shots.add_shot(
                Some(*s1_id),
                "2",
                ShotSize::Ots,
                ShotMovement::Steadicam,
                "35mm".into(),
                "Over-the-shoulder tracking entry".into(),
                ShotRange::blocks(pick(s1_blocks, 1), pick(s1_blocks, 2)),
            );
        }
    }

    (id_1a, id_1b)
}

/// Phase 1 Shots workspace — Color Map mode with overlapping shots & inspector.
#[test]
fn snap_40_shots_color_map() {
    use scriptcraft_app::shots_view::ShotsViewMode;

    let mut h = harness(SideTab::Script, None);
    h.run();
    let (_id_1a, id_1b) = seed_demo_shots(h.state_mut());
    h.state_mut()
        .set_shots_view_for_test(ShotsViewMode::ColorMap, 0, Some(id_1b));
    h.run_steps(2);
    h.snapshot("40_shots_color_map");
}

/// Phase 1 Shots workspace — Hollywood Coverage Rails mode showing overlapping
/// vertical colored tracks down the right margin.
#[test]
fn snap_41_shots_coverage_rails() {
    use scriptcraft_app::shots_view::ShotsViewMode;

    let mut h = harness(SideTab::Script, None);
    h.run();
    let (id_1a, _id_1b) = seed_demo_shots(h.state_mut());
    h.state_mut()
        .set_shots_view_for_test(ShotsViewMode::CoverageRails, 0, Some(id_1a));
    h.run_steps(2);
    h.snapshot("41_shots_coverage_rails");
}

fn seed_demo_floor_plan(app: &mut ScriptCraftApp, ids: [uuid::Uuid; 4]) {
    use scriptcraft_core::shots::{PlanActor, PlanCamera, PlanProp, PropKind, ShotColor};

    let scene0_id = app.project_for_test().expect("A95").scenes()[0].id;
    let plan = app
        .shots_mut_for_test()
        .expect("shots")
        .ensure_floor_plan(scene0_id);

    // Apartment block wall + entrance door behind the parking lot
    plan.props = vec![
        PlanProp {
            id: uuid::Uuid::new_v4(),
            kind: PropKind::Wall,
            label: String::new(),
            x: -2.4,
            y: -3.2,
            w: 4.8,
            h: 0.24,
            angle_deg: 0.0,
        },
        PlanProp {
            id: uuid::Uuid::new_v4(),
            kind: PropKind::Door,
            label: "Entrance".into(),
            x: 0.6,
            y: -3.2,
            w: 1.1,
            h: 0.24,
            angle_deg: 0.0,
        },
        PlanProp {
            id: uuid::Uuid::new_v4(),
            kind: PropKind::Wall,
            label: String::new(),
            x: 3.1,
            y: -3.2,
            w: 3.8,
            h: 0.24,
            angle_deg: 0.0,
        },
        // Opel Kadett parked in the foreground
        PlanProp {
            id: uuid::Uuid::new_v4(),
            kind: PropKind::Car,
            label: "OPEL KADETT".into(),
            x: -0.3,
            y: -0.7,
            w: 4.2,
            h: 1.75,
            angle_deg: 0.0,
        },
    ];

    plan.actors = vec![
        PlanActor {
            id: uuid::Uuid::new_v4(),
            name: "БАЩАТА".into(),
            x: 2.3,
            y: -0.7,
            angle_deg: 180.0,
            color: ShotColor::Cobalt,
            move_to: None,
        },
        PlanActor {
            id: uuid::Uuid::new_v4(),
            name: "ИВАН".into(),
            x: -0.4,
            y: 0.55,
            angle_deg: -18.0,
            color: ShotColor::Emerald,
            move_to: Some((0.8, 1.05)),
        },
    ];

    plan.cameras = vec![
        PlanCamera {
            shot_id: ids[0], // 1A 28mm Dolly
            x: -2.9,
            y: 3.1,
            angle_deg: -46.0,
            fov_deg: None,
            cone_length: Some(3.1),
            track_to: Some((-1.1, 2.3)),
        },
        PlanCamera {
            shot_id: ids[1], // 1B 50mm MCU
            x: 1.3,
            y: 2.4,
            angle_deg: -72.0,
            fov_deg: None,
            cone_length: None,
            track_to: None,
        },
        PlanCamera {
            shot_id: ids[2], // 1C 85mm CU
            x: 3.8,
            y: 1.7,
            angle_deg: -145.0,
            fov_deg: None,
            cone_length: Some(2.8),
            track_to: None,
        },
        PlanCamera {
            shot_id: ids[3], // 1D 85mm Insert
            x: -2.0,
            y: 1.5,
            angle_deg: -28.0,
            fov_deg: None,
            cone_length: Some(2.1),
            track_to: None,
        },
    ];
}

/// Phase 2 Shots workspace — 2D Top-Down Blocking / Floor Plan canvas with
/// architectural props, actors, 180° Line of Action, and live focal-length FOV cones.
#[test]
fn snap_42_shots_floor_plan() {
    use scriptcraft_app::shots_view::ShotsViewMode;

    let mut h = harness(SideTab::Script, None);
    h.run();
    let (id_1a, _id_1b) = seed_demo_shots(h.state_mut());
    let ids: [uuid::Uuid; 4] = {
        let shots = &h.state().project_for_test().unwrap().shots.shots;
        [shots[0].id, shots[1].id, shots[2].id, shots[3].id]
    };
    seed_demo_floor_plan(h.state_mut(), ids);
    h.state_mut()
        .set_shots_view_for_test(ShotsViewMode::FloorPlan, 0, Some(id_1a));
    h.run_steps(2);
    h.snapshot("42_shots_floor_plan");
}

/// Phase 2 Shots workspace — Side-by-side Split mode showing character-highlighted
/// screenplay on the left synced live with the 2D Top-Down Floor Plan on the right.
#[test]
fn snap_43_shots_split_view() {
    use scriptcraft_app::shots_view::ShotsViewMode;

    let mut h = harness(SideTab::Script, None);
    h.run();
    let (_id_1a, id_1b) = seed_demo_shots(h.state_mut());
    let ids: [uuid::Uuid; 4] = {
        let shots = &h.state().project_for_test().unwrap().shots.shots;
        [shots[0].id, shots[1].id, shots[2].id, shots[3].id]
    };
    seed_demo_floor_plan(h.state_mut(), ids);
    h.state_mut()
        .set_shots_view_for_test(ShotsViewMode::Split, 0, Some(id_1b));
    h.run_steps(2);
    h.snapshot("43_shots_split_view");
}
