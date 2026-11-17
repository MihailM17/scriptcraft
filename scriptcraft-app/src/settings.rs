//! Settings module: the persisted options and the KIT-style preferences
//! pages. Behaviour is unchanged from the original `app.rs` code; the caller
//! applies the chosen theme and saves the file when a control reports a change.

use egui::{Context, Ui};

use crate::app::{kit_section, kit_square_button, start_button, KitIcon};
use crate::theme::{kit, Theme, ThemeGroup};

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

/// Script face used for the page (editor and PDF). Bounded to faces with the
/// same 0.6 em advance, so choosing one never moves a page break.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScriptFace {
    /// Courier Prime: the classic screenplay look. Latin only — Cyrillic is
    /// drawn from IBM Plex Mono.
    CourierPrime,
    /// Courier New: the Windows/Mac system face and the default when the
    /// computer has it. Not bundled (its licence does not allow
    /// redistribution); read from the OS, with Courier Prime as the fallback
    /// when it isn't installed.
    CourierNew,
    /// IBM Plex Mono: monospaced, with a matched Latin and Cyrillic design.
    /// The fallback default: it is the only bundled face whose Latin and
    /// Cyrillic share proportions, so mixed-script pages stay level.
    PlexMono,
}

impl Default for ScriptFace {
    /// Courier New is the face most screenwriters and producers expect to
    /// see on a script, so it is used whenever the operating system provides
    /// it. Machines without it (and stripped-down Linux images) get IBM Plex
    /// Mono, the only bundled face that draws Cyrillic and Latin alike.
    fn default() -> Self {
        if crate::system_font::courier_new_installed() {
            Self::CourierNew
        } else {
            Self::PlexMono
        }
    }
}

impl ScriptFace {
    pub const ALL: [Self; 3] = [Self::PlexMono, Self::CourierPrime, Self::CourierNew];

    pub fn key(self) -> &'static str {
        match self {
            Self::CourierPrime => "courier",
            Self::CourierNew => "courier-new",
            Self::PlexMono => "plex",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::CourierPrime => "Courier Prime (classic, Latin only)",
            Self::CourierNew => "Courier New (system font)",
            Self::PlexMono => "IBM Plex Mono (Latin + Cyrillic)",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim() {
            "courier" | "courier_prime" => Some(Self::CourierPrime),
            "courier-new" | "courier_new" => Some(Self::CourierNew),
            "plex" | "plex_mono" => Some(Self::PlexMono),
            _ => None,
        }
    }

    /// True when the chosen face is Courier New but the OS doesn't have it,
    /// so the page is drawn and exported with Courier Prime instead.
    pub fn is_fallback(self) -> bool {
        self == Self::CourierNew && !crate::system_font::courier_new_installed()
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
    /// Face used for the page (editor and exported PDF).
    pub script_face: ScriptFace,
    /// Keep the page white with black ink whatever the theme does. The rest
    /// of the UI keeps its palette, and revision marks stay coloured.
    pub paper_mono: bool,
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
    /// Spellchecking. Off by default: a Bulgarian or Russian script would
    /// light up red on the first run before the writer picks a language.
    pub spellcheck: bool,
    pub spell_language: crate::spell::SpellLang,
    /// Screenplay slugs, cues and sound effects are ALL CAPS and are not
    /// prose. On by default.
    pub spell_ignore_caps: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            autosave: true,
            autosave_minutes: 5,
            backups: true,
            backups_dir: crate::app::default_backups_dir(),
            theme: Theme::Midnight,
            script_face: ScriptFace::default(),
            paper_mono: false,
            show_scene_numbers: true,
            highlight_current_line: false,
            capitalize_first_word: false,
            correct_double_capitals: false,
            replace_three_dots: false,
            smart_quotes: false,
            navigator_preview: true,
            navigator_show_numbers: true,
            spellcheck: false,
            spell_language: crate::spell::SpellLang::default(),
            spell_ignore_caps: true,
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
                    "paper_mono" => s.paper_mono = v == "1",
                    "script_face" => {
                        s.script_face = ScriptFace::from_key(v).unwrap_or(s.script_face);
                    }
                    "show_scene_numbers" => s.show_scene_numbers = v != "0",
                    "highlight_current_line" => s.highlight_current_line = v == "1",
                    "capitalize_first_word" => s.capitalize_first_word = v == "1",
                    "correct_double_capitals" => s.correct_double_capitals = v == "1",
                    "replace_three_dots" => s.replace_three_dots = v == "1",
                    "smart_quotes" => s.smart_quotes = v == "1",
                    "navigator_preview" => s.navigator_preview = v != "0",
                    "navigator_show_numbers" => s.navigator_show_numbers = v != "0",
                    "spellcheck" => s.spellcheck = v == "1",
                    "spell_language" => {
                        s.spell_language =
                            crate::spell::SpellLang::from_key(v).unwrap_or(s.spell_language);
                    }
                    "spell_ignore_caps" => s.spell_ignore_caps = v != "0",
                    "theme" => {
                        s.theme = Theme::from_key(v).unwrap_or(s.theme);
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
             script_face={}\npaper_mono={}\nshow_scene_numbers={}\nhighlight_current_line={}\ncapitalize_first_word={}\n\
             correct_double_capitals={}\nreplace_three_dots={}\nsmart_quotes={}\nnavigator_preview={}\n\
             navigator_show_numbers={}\nspellcheck={}\nspell_language={}\nspell_ignore_caps={}\n",
            self.autosave as u8,
            self.autosave_minutes.clamp(1, 60),
            self.backups as u8,
            self.backups_dir,
            self.theme.key(),
            self.script_face.key(),
            self.paper_mono as u8,
            self.show_scene_numbers as u8,
            self.highlight_current_line as u8,
            self.capitalize_first_word as u8,
            self.correct_double_capitals as u8,
            self.replace_three_dots as u8,
            self.smart_quotes as u8,
            self.navigator_preview as u8,
            self.navigator_show_numbers as u8,
            self.spellcheck as u8,
            self.spell_language.key(),
            self.spell_ignore_caps as u8,
        );
        let _ = std::fs::write(crate::app::config_dir().join("settings.conf"), text);
    }
}

/// What the settings panel did this frame.
#[derive(Default, Clone, Copy)]
pub struct SettingsOutcome {
    pub changed: bool,
    pub reset: bool,
    /// The writer pressed "Check for updates".
    pub check_updates: bool,
}

/// Draw the Settings module. `changed` means an option was edited; `reset`
/// means the defaults were restored (also included in `changed`).
pub fn show(
    ctx: &Context,
    settings: &mut AppSettings,
    page: &mut SettingsPage,
    update: &crate::update::Check,
) -> SettingsOutcome {
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
                            SettingsPage::Application => settings_application(ui, settings, update),
                            SettingsPage::ScenarioEditor => settings_editor(ui, settings),
                            SettingsPage::Navigator => settings_navigator(ui, settings),
                        };
                        outcome.changed |= result.0;
                        outcome.reset |= result.1;
                        outcome.check_updates |= result.2;
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
/// Returns `(changed, reset, check_updates)`.
fn settings_application(
    ui: &mut Ui,
    settings: &mut AppSettings,
    update: &crate::update::Check,
) -> (bool, bool, bool) {
    let mut changed = false;
    let mut reset = false;
    let mut check_updates = false;
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
        // A single dropdown, grouped by family. A solid scrollbar (rather
        // than egui's floating, fade-in bar) makes it obvious that the list
        // continues below the fold.
        let popup_style: egui::style::StyleModifier = (|style: &mut egui::Style| {
            style.spacing.scroll.floating = false;
            style.spacing.scroll.bar_width = 10.0;
        })
        .into();
        egui::ComboBox::from_id_salt("theme")
            .selected_text(settings.theme.label())
            .width(220.0)
            .height(380.0)
            .popup_style(popup_style)
            .show_ui(ui, |ui| {
                for group in ThemeGroup::ALL {
                    if group != ThemeGroup::ALL[0] {
                        ui.add_space(6.0);
                    }
                    ui.label(
                        egui::RichText::new(group.label())
                            .size(11.0)
                            .strong()
                            .color(kit().disabled_text),
                    );
                    for theme in group.themes() {
                        changed |= ui
                            .selectable_value(&mut settings.theme, *theme, theme.label())
                            .changed();
                    }
                }
            });
    });
    ui.add_space(8.0);
    // Right under the theme it modifies: the page can drop to white paper and
    // black ink while the palette above keeps the rest of the app themed.
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        changed |= ui
            .checkbox(
                &mut settings.paper_mono,
                "Monochrome paper (white page, black ink)",
            )
            .on_hover_text(
                "Keep the page white with black text whatever the theme does. Panels, \
                 menus and toolbars keep the theme; revision marks stay coloured.",
            )
            .changed();
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
    ui.add_space(20.0);
    kit_section(ui, "Updates");
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.label(egui::RichText::new(format!("ScriptCraft {}", crate::update::CURRENT)).strong());
        ui.add_space(8.0);
        let note = update.note();
        if !note.is_empty() {
            let color = match update {
                crate::update::Check::Failed(_) => kit().spell_error,
                crate::update::Check::Available(_) => kit().accent_text,
                _ => kit().disabled_text,
            };
            ui.label(egui::RichText::new(note).color(color))
                .on_hover_text(update.detail());
        }
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        let checking = update.is_checking();
        let label = if checking {
            "Checking…"
        } else {
            "Check for updates"
        };
        let button = egui::Button::new(egui::RichText::new(label).size(12.5))
            .min_size(egui::vec2(150.0, 26.0));
        if ui
            .add_enabled(!checking, button)
            .on_hover_text(update.detail())
            .clicked()
        {
            check_updates = true;
        }
    });
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.add_space(16.0);
        ui.label(
            egui::RichText::new(
                "Asks GitHub for the latest release. ScriptCraft also checks when it opens, \
                 and offers a newer version in a prompt.",
            )
            .size(11.0)
            .color(kit().disabled_text),
        );
    });
    ui.add_space(24.0);
    if start_button(ui, None, "Reset Settings") {
        *settings = AppSettings::default();
        reset = true;
        changed = true;
    }
    ui.add_space(24.0);
    (changed, reset, check_updates)
}

/// Scenario Editor page: KIT's view toggles and automatic corrections.
fn settings_editor(ui: &mut Ui, settings: &mut AppSettings) -> (bool, bool, bool) {
    let mut changed = false;
    kit_section(ui, "Page");
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.label("Script font:");
        ui.add_space(6.0);
        changed |= egui::ComboBox::from_id_salt("script_face")
            .selected_text(settings.script_face.label())
            .width(280.0)
            .show_ui(ui, |ui| {
                let mut picked = false;
                for face in ScriptFace::ALL {
                    picked |= ui
                        .selectable_value(&mut settings.script_face, face, face.label())
                        .changed();
                }
                picked
            })
            .inner
            .unwrap_or(false);
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.label(
            egui::RichText::new("INT. КУХНЯ — NIGHT      МАРА (V.O.)      Hello, свят.")
                .font(egui::FontId::monospace(14.0))
                .color(kit().text),
        );
    });
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.add_space(16.0);
        ui.label(
            egui::RichText::new(
                "Courier New and IBM Plex Mono both draw Latin and Cyrillic in one \
                 design. Courier Prime is Latin only and borrows Plex for Cyrillic. All \
                 three share 0.6 em advances, so pagination is identical whichever face \
                 you pick.",
            )
            .size(11.0)
            .color(kit().disabled_text),
        );
    });
    if settings.script_face.is_fallback() {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.add_space(16.0);
            ui.label(
                egui::RichText::new(
                    "Courier New is not installed on this computer, so pages use Courier \
                     Prime until it is.",
                )
                .size(11.0)
                .color(kit().spell_error),
            );
        });
    }
    ui.add_space(20.0);
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
    kit_section(ui, "Spelling");
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        changed |= ui
            .checkbox(&mut settings.spellcheck, "Check spelling as you type")
            .on_hover_text(
                "Underlines words the dictionary doesn't know. Right-click one for suggestions.",
            )
            .changed();
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.label("Dictionary:");
        ui.add_space(6.0);
        ui.add_enabled_ui(settings.spellcheck, |ui| {
            changed |= egui::ComboBox::from_id_salt("spell_language")
                .selected_text(settings.spell_language.label())
                .width(150.0)
                .show_ui(ui, |ui| {
                    let mut picked = false;
                    for lang in crate::spell::SpellLang::ALL {
                        picked |= ui
                            .selectable_value(&mut settings.spell_language, lang, lang.label())
                            .changed();
                    }
                    picked
                })
                .inner
                .unwrap_or(false);
        });
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.add_enabled_ui(settings.spellcheck, |ui| {
            changed |= ui
                .checkbox(&mut settings.spell_ignore_caps, "Ignore ALL-CAPS words")
                .on_hover_text("Scene headings, character cues and sound effects are not prose.")
                .changed();
        });
    });
    if settings.spellcheck {
        if let Some(err) = crate::spell::error() {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                ui.label(egui::RichText::new(err).size(11.0).color(kit().spell_error));
            });
        }
    }
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
    (changed, false, false)
}

/// Navigator page: what each scene row shows.
fn settings_navigator(ui: &mut Ui, settings: &mut AppSettings) -> (bool, bool, bool) {
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
    (changed, false, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default page face is the system's Courier New when it exists, and
    /// IBM Plex Mono when it does not. Either way the default must be
    /// drawable without a fallback warning.
    #[test]
    fn default_face_follows_the_installed_fonts() {
        let expected = if crate::system_font::courier_new_installed() {
            ScriptFace::CourierNew
        } else {
            ScriptFace::PlexMono
        };
        assert_eq!(ScriptFace::default(), expected);
        assert_eq!(AppSettings::default().script_face, expected);
        assert!(
            !ScriptFace::default().is_fallback(),
            "the default face must never need the fallback warning"
        );
    }

    #[test]
    fn every_face_round_trips_through_its_key() {
        for face in ScriptFace::ALL {
            assert_eq!(ScriptFace::from_key(face.key()), Some(face), "{face:?}");
        }
    }
}
