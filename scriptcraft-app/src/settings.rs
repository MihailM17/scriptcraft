//! Settings module: the persisted options and the KIT-style preferences
//! pages. Behaviour is unchanged from the original `app.rs` code; the caller
//! applies the chosen theme and saves the file when a control reports a change.

use egui::{Context, Ui};

use crate::app::{kit_section, kit_square_button, start_button, KitIcon};
use crate::theme::{kit, Theme};

/// Selected page in Settings, mirroring KIT's Preferences list.
#[derive(Default, PartialEq, Eq, Clone, Copy, Debug)]
pub enum SettingsPage {
    #[default]
    Application,
    ScenarioEditor,
    Navigator,
}

impl SettingsPage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Application => "Application",
            Self::ScenarioEditor => "Scenario Editor",
            Self::Navigator => "Navigator",
        }
    }
}

/// Persisted application settings (`settings.conf` in the per-user config dir).
#[derive(Clone)]
pub struct AppSettings {
    pub autosave: bool,
    pub autosave_minutes: u32,
    pub backups: bool,
    pub backups_dir: String,
    pub theme: Theme,
    /// KIT's Scenario Editor toggles. Defaults preserve the editor's
    /// previous behaviour: scene numbers and a preview in the navigator on,
    /// automatic typography off until the writer asks for it.
    pub show_scene_numbers: bool,
    pub highlight_current_line: bool,
    pub capitalize_first_word: bool,
    pub correct_double_capitals: bool,
    pub replace_three_dots: bool,
    pub smart_quotes: bool,
    pub navigator_preview: bool,
    pub navigator_show_numbers: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            autosave: true,
            autosave_minutes: 5,
            backups: true,
            backups_dir: crate::app::default_backups_dir(),
            theme: Theme::Midnight,
            show_scene_numbers: true,
            highlight_current_line: false,
            capitalize_first_word: false,
            correct_double_capitals: false,
            replace_three_dots: false,
            smart_quotes: false,
            navigator_preview: true,
            navigator_show_numbers: true,
        }
    }
}

impl AppSettings {
    pub fn editor_options(&self) -> crate::script_view::EditorOptions {
        crate::script_view::EditorOptions {
            show_scene_numbers: self.show_scene_numbers,
            highlight_current_line: self.highlight_current_line,
            capitalize_first_word: self.capitalize_first_word,
            correct_double_capitals: self.correct_double_capitals,
            replace_three_dots: self.replace_three_dots,
            smart_quotes: self.smart_quotes,
        }
    }

    /// Read `settings.conf`; missing or unknown keys keep their defaults.
    pub fn load() -> Self {
        let mut s = Self::default();
        let mut theme_seen = false;
        if let Ok(text) = std::fs::read_to_string(crate::app::config_dir().join("settings.conf")) {
            for line in text.lines() {
                let Some((k, v)) = line.split_once('=') else {
                    continue;
                };
                let v = v.trim();
                match k.trim() {
                    "autosave" => s.autosave = v == "1",
                    "autosave_minutes" => s.autosave_minutes = v.parse().unwrap_or(5).clamp(1, 60),
                    "backups" => s.backups = v == "1",
                    "backups_dir" => {
                        if !v.is_empty() {
                            s.backups_dir = v.to_string();
                        }
                    }
                    "show_scene_numbers" => s.show_scene_numbers = v != "0",
                    "highlight_current_line" => s.highlight_current_line = v == "1",
                    "capitalize_first_word" => s.capitalize_first_word = v == "1",
                    "correct_double_capitals" => s.correct_double_capitals = v == "1",
                    "replace_three_dots" => s.replace_three_dots = v == "1",
                    "smart_quotes" => s.smart_quotes = v == "1",
                    "navigator_preview" => s.navigator_preview = v != "0",
                    "navigator_show_numbers" => s.navigator_show_numbers = v != "0",
                    "theme" => {
                        s.theme = match v {
                            "light" => Theme::Light,
                            "midnight" => Theme::Midnight,
                            _ => s.theme,
                        };
                        theme_seen = true;
                    }
                    // Pre-theme-picker setting: 1 = Midnight, 0 = Light.
                    "dark_mode" if !theme_seen => {
                        s.theme = if v == "1" {
                            Theme::Midnight
                        } else {
                            Theme::Light
                        };
                    }
                    _ => {}
                }
            }
        }
        s
    }

    /// Write every key in the stable order used since v0.7.
    pub fn save(&self) {
        let _ = std::fs::create_dir_all(crate::app::config_dir());
        let text = format!(
            "autosave={}\nautosave_minutes={}\nbackups={}\nbackups_dir={}\ntheme={}\n\
             show_scene_numbers={}\nhighlight_current_line={}\ncapitalize_first_word={}\n\
             correct_double_capitals={}\nreplace_three_dots={}\nsmart_quotes={}\nnavigator_preview={}\n\
             navigator_show_numbers={}\n",
            self.autosave as u8,
            self.autosave_minutes.clamp(1, 60),
            self.backups as u8,
            self.backups_dir,
            self.theme.key(),
            self.show_scene_numbers as u8,
            self.highlight_current_line as u8,
            self.capitalize_first_word as u8,
            self.correct_double_capitals as u8,
            self.replace_three_dots as u8,
            self.smart_quotes as u8,
            self.navigator_preview as u8,
            self.navigator_show_numbers as u8,
        );
        let _ = std::fs::write(crate::app::config_dir().join("settings.conf"), text);
    }
}

/// What the settings panel did this frame.
#[derive(Default, Clone, Copy)]
pub struct SettingsOutcome {
    pub changed: bool,
    pub reset: bool,
}

/// Draw the Settings module. `changed` means an option was edited; `reset`
/// means the defaults were restored (also included in `changed`).
pub fn show(ctx: &Context, settings: &mut AppSettings, page: &mut SettingsPage) -> SettingsOutcome {
    let mut outcome = SettingsOutcome::default();
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.add_space(20.0);
        ui.horizontal_top(|ui| {
            ui.add_space(24.0);
            ui.vertical(|ui| {
                ui.set_width(186.0);
                ui.label(egui::RichText::new("Settings").size(18.0).strong());
                ui.add_space(14.0);
                for item in [
                    SettingsPage::Application,
                    SettingsPage::ScenarioEditor,
                    SettingsPage::Navigator,
                ] {
                    let selected = *page == item;
                    if settings_nav_row(ui, item.label(), selected).clicked() {
                        *page = item;
                    }
                    ui.add_space(2.0);
                }
            });
            ui.add_space(10.0);
            let (line, _) = ui.allocate_exact_size(
                egui::vec2(1.0, ui.available_height().max(320.0)),
                egui::Sense::hover(),
            );
            ui.painter().rect_filled(line, 0.0, kit().sep);
            ui.add_space(24.0);
            ui.vertical(|ui| {
                ui.set_max_width(640.0);
                ui.label(egui::RichText::new(page.label()).size(16.0).strong());
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "Everything here is applied immediately and saved for the next launch.",
                    )
                    .size(12.0)
                    .color(kit().disabled_text),
                );
                ui.add_space(14.0);
                egui::ScrollArea::vertical()
                    .id_salt("settings_page")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let result = match page {
                            SettingsPage::Application => settings_application(ui, settings),
                            SettingsPage::ScenarioEditor => settings_editor(ui, settings),
                            SettingsPage::Navigator => settings_navigator(ui, settings),
                        };
                        outcome.changed |= result.0;
                        outcome.reset |= result.1;
                    });
            });
        });
    });
    outcome
}

/// Left-hand page row in Settings, mirroring KIT's preferences list.
fn settings_nav_row(ui: &mut Ui, label: &str, selected: bool) -> egui::Response {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 32.0), egui::Sense::click());
    if selected {
        ui.painter().rect_filled(rect, 6.0, kit().accent_soft);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 6.0, kit().hover_light);
    }
    ui.painter().text(
        egui::pos2(rect.left() + 12.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0),
        if selected {
            kit().accent_text
        } else {
            kit().text
        },
    );
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let text = label.to_owned();
    resp.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, text.clone())
    });
    resp
}

/// Application page: theme, autosave, backups and the reset action.
/// Returns `(changed, reset)`.
fn settings_application(ui: &mut Ui, settings: &mut AppSettings) -> (bool, bool) {
    let mut changed = false;
    let mut reset = false;
    let mut v = ui.visuals().clone();
    v.extreme_bg_color = kit().field_stroke;
    v.widgets.inactive.weak_bg_fill = kit().field_stroke;
    v.widgets.hovered.weak_bg_fill = kit().hover_light;
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, kit().sep);
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, kit().sep);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.label("Language:");
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("English (the only language in this build)")
                .color(kit().disabled_text),
        );
    });
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.label("Theme:");
        ui.add_space(6.0);
        egui::ComboBox::from_id_salt("theme")
            .selected_text(settings.theme.label())
            .width(140.0)
            .show_ui(ui, |ui| {
                for theme in [Theme::Light, Theme::Midnight] {
                    changed |= ui
                        .selectable_value(&mut settings.theme, theme, theme.label())
                        .changed();
                }
            });
    });
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        let r = ui.checkbox(&mut settings.autosave, "Automatically Save Every");
        changed |= r.changed();
        ui.add_space(6.0);
        changed |= ui
            .scope(|ui| {
                ui.style_mut().visuals = v.clone();
                ui.add_enabled(
                    settings.autosave,
                    egui::DragValue::new(&mut settings.autosave_minutes)
                        .range(1..=60)
                        .speed(0.05),
                )
            })
            .inner
            .changed();
        ui.add_space(6.0);
        ui.label("Minutes");
        ui.add_space(4.0);
        ui.label(egui::RichText::new("?").weak())
            .on_hover_text("Saves the project while you work. Turn off to save by hand only.");
    });
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        let r = ui.checkbox(&mut settings.backups, "Save Backups");
        changed |= r.changed();
        r.on_hover_text("Timestamped copies are written here on every save.");
        ui.add_space(6.0);
        let field_w = (ui.available_width() - 44.0).max(80.0);
        changed |= ui
            .scope(|ui| {
                ui.style_mut().visuals = v.clone();
                ui.add_enabled(
                    settings.backups,
                    egui::TextEdit::singleline(&mut settings.backups_dir)
                        .margin(egui::Margin::symmetric(8, 4))
                        .font(egui::FontId::proportional(12.5))
                        .desired_width(field_w),
                )
            })
            .inner
            .changed();
        if kit_square_button(ui, KitIcon::Folder).clicked() {
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                settings.backups_dir = dir.to_string_lossy().into_owned();
                changed = true;
            }
        }
    });
    ui.add_space(24.0);
    if start_button(ui, None, "Reset Settings") {
        *settings = AppSettings::default();
        reset = true;
        changed = true;
    }
    ui.add_space(24.0);
    (changed, reset)
}

/// Scenario Editor page: KIT's view toggles and automatic corrections.
fn settings_editor(ui: &mut Ui, settings: &mut AppSettings) -> (bool, bool) {
    let mut changed = false;
    kit_section(ui, "View");
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        changed |= ui
            .checkbox(&mut settings.show_scene_numbers, "Show scene numbers")
            .on_hover_text("Show the scene number in the page margin while editing.")
            .changed();
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        changed |= ui
            .checkbox(
                &mut settings.highlight_current_line,
                "Highlight current line",
            )
            .on_hover_text("Tint the line holding the caret, like KIT's editor.")
            .changed();
    });
    ui.add_space(20.0);
    kit_section(ui, "Automatic corrections");
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        changed |= ui
            .checkbox(&mut settings.capitalize_first_word, "Capitalize first word")
            .on_hover_text("Uppercase the first letter typed at the start of a sentence.")
            .changed();
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        changed |= ui
            .checkbox(
                &mut settings.correct_double_capitals,
                "Correct DOuble CApitals",
            )
            .on_hover_text("Fix a second capital in a word: DOuble becomes Double.")
            .changed();
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        changed |= ui
            .checkbox(
                &mut settings.replace_three_dots,
                "Replace three dots with …",
            )
            .on_hover_text("Typing ... inserts the single ellipsis character.")
            .changed();
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        changed |= ui
            .checkbox(&mut settings.smart_quotes, "Smart quotes")
            .on_hover_text("Straight quotes become typographic quotes and apostrophes.")
            .changed();
    });
    ui.add_space(20.0);
    ui.label(
        egui::RichText::new(
            "Corrections apply as you type. Existing text is never rewritten in bulk.",
        )
        .size(11.0)
        .color(kit().disabled_text),
    );
    (changed, false)
}

/// Navigator page: what each scene row shows.
fn settings_navigator(ui: &mut Ui, settings: &mut AppSettings) -> (bool, bool) {
    let mut changed = false;
    kit_section(ui, "Scene list");
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        changed |= ui
            .checkbox(&mut settings.navigator_show_numbers, "Show scene numbers")
            .on_hover_text("Show each scene's number in the navigator.")
            .changed();
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        changed |= ui
            .checkbox(&mut settings.navigator_preview, "Show scene description")
            .on_hover_text("The first action line under each heading.")
            .changed();
    });
    ui.add_space(20.0);
    ui.label(
        egui::RichText::new("Scene rows always show the heading and estimated duration.")
            .size(11.0)
            .color(kit().disabled_text),
    );
    (changed, false)
}
