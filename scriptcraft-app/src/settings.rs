//! Settings module: the persisted options and the five Settings pages
//! (General, Appearance, Writing, Spelling, Saving). Behaviour is unchanged
//! from the original `app.rs` code; the caller applies the chosen theme and
//! saves the file when a control reports a change.

use egui::{Context, Ui};

use crate::app::{kit_section, kit_square_button, start_button, start_button_primary, KitIcon};
use crate::theme::{kit, Theme, ThemeGroup};

/// Selected page in Settings. Pages group by the kind of decision: about the
/// app, about how the page looks, about how writing behaves, about words, and
/// about the files on disk.
#[derive(Default, PartialEq, Eq, Clone, Copy, Debug)]
pub enum SettingsPage {
    #[default]
    General,
    Appearance,
    Writing,
    Spelling,
    Saving,
}

impl SettingsPage {
    pub const ALL: [Self; 5] = [
        Self::General,
        Self::Appearance,
        Self::Writing,
        Self::Spelling,
        Self::Saving,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Appearance => "Appearance",
            Self::Writing => "Writing",
            Self::Spelling => "Spelling",
            Self::Saving => "Saving",
        }
    }
}

/// One searchable row. The title is the label a writer sees; `keywords` are
/// the words they might type instead of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SettingEntry {
    pub page: SettingsPage,
    pub title: &'static str,
    keywords: &'static str,
}

/// Every row the search box can find. A new row belongs here too, otherwise
/// the one search box silently stops being complete.
const SETTINGS_INDEX: &[SettingEntry] = &[
    SettingEntry {
        page: SettingsPage::General,
        title: "Check for updates",
        keywords: "update upgrade github release latest version",
    },
    SettingEntry {
        page: SettingsPage::General,
        title: "About ScriptCraft",
        keywords: "version build number about release",
    },
    SettingEntry {
        page: SettingsPage::General,
        title: "Reset settings",
        keywords: "defaults restore start over factory",
    },
    SettingEntry {
        page: SettingsPage::Appearance,
        title: "Theme",
        keywords: "colour color palette dark light appearance",
    },
    SettingEntry {
        page: SettingsPage::Appearance,
        title: "Monochrome paper",
        keywords: "white page black ink mono print paper",
    },
    SettingEntry {
        page: SettingsPage::Appearance,
        title: "Script font",
        keywords: "page face typeface courier new prime plex mono typography",
    },
    SettingEntry {
        page: SettingsPage::Appearance,
        title: "Page size",
        keywords: "paper size a4 us letter format template dimensions",
    },
    SettingEntry {
        page: SettingsPage::Writing,
        title: "Show scene numbers",
        keywords: "margin numbering page",
    },
    SettingEntry {
        page: SettingsPage::Writing,
        title: "Highlight current line",
        keywords: "caret tint cursor row",
    },
    SettingEntry {
        page: SettingsPage::Writing,
        title: "Bold character names (speaking parts)",
        keywords: "bold character cue speaker dialogue speaking parts names",
    },
    SettingEntry {
        page: SettingsPage::Writing,
        title: "Number scenes in the list",
        keywords: "navigator scene list numbering",
    },
    SettingEntry {
        page: SettingsPage::Writing,
        title: "Show scene description",
        keywords: "navigator preview action line",
    },
    SettingEntry {
        page: SettingsPage::Writing,
        title: "Capitalize first word",
        keywords: "correction sentence case capital",
    },
    SettingEntry {
        page: SettingsPage::Writing,
        title: "Correct DOuble CApitals",
        keywords: "correction capitalisation capitalization",
    },
    SettingEntry {
        page: SettingsPage::Writing,
        title: "Replace three dots with ellipsis",
        keywords: "correction typography dots",
    },
    SettingEntry {
        page: SettingsPage::Writing,
        title: "Smart quotes",
        keywords: "correction typography apostrophe curly",
    },
    SettingEntry {
        page: SettingsPage::Spelling,
        title: "Check spelling as you type",
        keywords: "spellcheck dictionary underlines",
    },
    SettingEntry {
        page: SettingsPage::Spelling,
        title: "Dictionary",
        keywords: "language english bulgarian russian spell",
    },
    SettingEntry {
        page: SettingsPage::Spelling,
        title: "Ignore ALL-CAPS words",
        keywords: "spelling uppercase caps headings cues",
    },
    SettingEntry {
        page: SettingsPage::Saving,
        title: "Automatically Save Every",
        keywords: "autosave minutes interval save",
    },
    SettingEntry {
        page: SettingsPage::Saving,
        title: "Save Backups",
        keywords: "backup copies folder timestamped",
    },
];

/// Settings whose title or keywords contain every word in `query`.
/// Case-insensitive; an empty query matches nothing (the page list is shown
/// instead of results).
pub fn search_matches(query: &str) -> Vec<SettingEntry> {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|term| term.to_lowercase())
        .collect();
    if terms.is_empty() {
        return Vec::new();
    }
    SETTINGS_INDEX
        .iter()
        .copied()
        .filter(|entry| {
            let haystack =
                format!("{} {} {}", entry.title, entry.keywords, entry.page.label()).to_lowercase();
            terms.iter().all(|term| haystack.contains(term.as_str()))
        })
        .collect()
}

/// Persistent UI state for the Settings module: the chosen page, the search
/// box, and whether the reset confirmation is showing.
#[derive(Default)]
pub struct SettingsUi {
    pub page: SettingsPage,
    pub search: String,
    /// "Reset Settings…" only arms this; a second, explicit click resets.
    reset_armed: bool,
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
    /// Liberation Mono: metric-compatible with Courier New, with Latin,
    /// Greek and Cyrillic in one design. Bundled under the SIL OFL.
    LiberationMono,
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
    pub const ALL: [Self; 4] = [
        Self::PlexMono,
        Self::CourierPrime,
        Self::CourierNew,
        Self::LiberationMono,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::CourierPrime => "courier",
            Self::CourierNew => "courier-new",
            Self::PlexMono => "plex",
            Self::LiberationMono => "liberation",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::CourierPrime => "Courier Prime (classic, Latin only)",
            Self::CourierNew => "Courier New (system font)",
            Self::PlexMono => "IBM Plex Mono (Latin + Cyrillic)",
            Self::LiberationMono => "Liberation Mono (Latin + Cyrillic)",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim() {
            "courier" | "courier_prime" => Some(Self::CourierPrime),
            "courier-new" | "courier_new" => Some(Self::CourierNew),
            "plex" | "plex_mono" => Some(Self::PlexMono),
            "liberation" | "liberation-mono" | "liberation_mono" => Some(Self::LiberationMono),
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
    /// Paper size and template metrics for the editor and PDF export.
    /// Defaults to A4; US Letter is opt-in.
    pub page_size: scriptcraft_core::editing::PageSize,
    /// Keep the page white with black ink whatever the theme does. The rest
    /// of the UI keeps its palette, and revision marks stay coloured.
    pub paper_mono: bool,
    /// KIT's Scenario Editor toggles. Defaults preserve the editor's
    /// previous behaviour: scene numbers and a preview in the navigator on,
    /// automatic typography off until the writer asks for it.
    pub show_scene_numbers: bool,
    pub highlight_current_line: bool,
    /// Automatically bold speaking character cues (`ElementType::Character`)
    /// without affecting character names mentioned in Action.
    pub bold_character_cues: bool,
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
            page_size: scriptcraft_core::editing::PageSize::A4,
            paper_mono: false,
            show_scene_numbers: true,
            highlight_current_line: false,
            bold_character_cues: false,
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
            page_size: self.page_size,
            show_scene_numbers: self.show_scene_numbers,
            highlight_current_line: self.highlight_current_line,
            bold_character_cues: self.bold_character_cues,
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
                    "page_size" => {
                        s.page_size =
                            scriptcraft_core::editing::PageSize::from_key(v).unwrap_or(s.page_size);
                    }
                    "show_scene_numbers" => s.show_scene_numbers = v != "0",
                    "highlight_current_line" => s.highlight_current_line = v == "1",
                    "bold_character_cues" => s.bold_character_cues = v == "1",
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
             script_face={}\npage_size={}\npaper_mono={}\nshow_scene_numbers={}\nhighlight_current_line={}\nbold_character_cues={}\ncapitalize_first_word={}\n\
             correct_double_capitals={}\nreplace_three_dots={}\nsmart_quotes={}\nnavigator_preview={}\n\
             navigator_show_numbers={}\nspellcheck={}\nspell_language={}\nspell_ignore_caps={}\n",
            self.autosave as u8,
            self.autosave_minutes.clamp(1, 60),
            self.backups as u8,
            self.backups_dir,
            self.theme.key(),
            self.script_face.key(),
            self.page_size.key(),
            self.paper_mono as u8,
            self.show_scene_numbers as u8,
            self.highlight_current_line as u8,
            self.bold_character_cues as u8,
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
    state: &mut SettingsUi,
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
                ui.add_space(10.0);
                settings_search_field(ui, &mut state.search);
                ui.add_space(10.0);
                settings_pages_or_results(ui, state);
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
                ui.label(egui::RichText::new(state.page.label()).size(16.0).strong());
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
                        let result = match state.page {
                            SettingsPage::General => {
                                settings_general(ui, settings, update, &mut state.reset_armed)
                            }
                            SettingsPage::Appearance => settings_appearance(ui, settings),
                            SettingsPage::Writing => settings_writing(ui, settings),
                            SettingsPage::Spelling => settings_spelling(ui, settings),
                            SettingsPage::Saving => settings_saving(ui, settings),
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

/// The sidebar shows either the page list or, while a query is typed, the
/// matching rows with the page that holds each one.
fn settings_pages_or_results(ui: &mut Ui, state: &mut SettingsUi) {
    if state.search.trim().is_empty() {
        for item in SettingsPage::ALL {
            let selected = state.page == item;
            if settings_nav_row(ui, item.label(), selected).clicked() {
                state.page = item;
            }
            ui.add_space(2.0);
        }
        return;
    }

    let matches = search_matches(&state.search);
    if matches.is_empty() {
        ui.label(
            egui::RichText::new("No matching settings.")
                .size(12.0)
                .color(kit().disabled_text),
        );
        return;
    }
    for entry in matches.iter().take(MAX_SEARCH_RESULTS) {
        if settings_result_row(ui, entry).clicked() {
            state.page = entry.page;
            state.search.clear();
            state.reset_armed = false;
        }
        ui.add_space(2.0);
    }
    if matches.len() > MAX_SEARCH_RESULTS {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Keep typing to narrow the list.")
                .size(11.0)
                .color(kit().disabled_text),
        );
    }
}

/// Most results the narrow sidebar shows at once before asking for a more
/// specific query.
const MAX_SEARCH_RESULTS: usize = 7;

fn settings_search_field(ui: &mut Ui, query: &mut String) {
    let visuals = field_visuals(ui);
    ui.scope(|ui| {
        ui.style_mut().visuals = visuals;
        ui.add(
            egui::TextEdit::singleline(query)
                .hint_text("Search settings")
                .margin(egui::Margin::symmetric(8, 5))
                .font(egui::FontId::proportional(12.5))
                .desired_width(f32::INFINITY),
        );
    });
}

/// One search hit: the row title over the page that holds it.
fn settings_result_row(ui: &mut Ui, entry: &SettingEntry) -> egui::Response {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 40.0), egui::Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 6.0, kit().hover_light);
    }
    ui.painter().text(
        egui::pos2(rect.left() + 10.0, rect.center().y - 7.0),
        egui::Align2::LEFT_CENTER,
        entry.title,
        egui::FontId::proportional(12.5),
        kit().text,
    );
    ui.painter().text(
        egui::pos2(rect.left() + 10.0, rect.center().y + 8.0),
        egui::Align2::LEFT_CENTER,
        entry.page.label(),
        egui::FontId::proportional(10.5),
        kit().disabled_text,
    );
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, entry.title));
    resp
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

/// Text fields get the same recessed look on every page.
fn field_visuals(ui: &Ui) -> egui::Visuals {
    let mut v = ui.visuals().clone();
    v.extreme_bg_color = kit().field_stroke;
    v.widgets.inactive.weak_bg_fill = kit().field_stroke;
    v.widgets.hovered.weak_bg_fill = kit().hover_light;
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, kit().sep);
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, kit().sep);
    v
}

/// General page: update checking, About, and the guarded reset action.
/// Returns `(changed, reset, check_updates)`.
fn settings_general(
    ui: &mut Ui,
    settings: &mut AppSettings,
    update: &crate::update::Check,
    reset_armed: &mut bool,
) -> (bool, bool, bool) {
    let mut changed = false;
    let mut reset = false;
    let mut check_updates = false;

    kit_section(ui, "Updates");
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
        let note = update.note();
        if !note.is_empty() {
            ui.add_space(10.0);
            let color = match update {
                crate::update::Check::Failed(_) => kit().spell_error,
                crate::update::Check::Available(_) => kit().accent_text,
                _ => kit().disabled_text,
            };
            ui.label(egui::RichText::new(note).color(color))
                .on_hover_text(update.detail());
        }
    });
    ui.add_space(6.0);
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

    ui.add_space(20.0);
    kit_section(ui, "About");
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.label(egui::RichText::new(format!("ScriptCraft {}", crate::update::CURRENT)).strong());
    });
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.add_space(16.0);
        ui.label(
            egui::RichText::new("Free and open-source, built with egui and eframe.")
                .size(11.0)
                .color(kit().disabled_text),
        );
    });

    ui.add_space(20.0);
    kit_section(ui, "Defaults");
    ui.horizontal_wrapped(|ui| {
        ui.add_space(16.0);
        ui.label(
            egui::RichText::new(
                "Restores every option on these pages to its default. Projects, recent \
                 files and backups are not touched.",
            )
            .size(11.0)
            .color(kit().disabled_text),
        );
    });
    ui.add_space(10.0);
    if *reset_armed {
        egui::Frame::new()
            .fill(kit().surface)
            .stroke(egui::Stroke::new(1.0_f32, kit().spell_error))
            .corner_radius(9)
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new("Reset every setting to its default?").strong());
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new("This cannot be undone.")
                        .size(11.0)
                        .color(kit().spell_error),
                );
                ui.add_space(10.0);
                ui.columns(2, |cols| {
                    if start_button(&mut cols[0], None, "Cancel") {
                        *reset_armed = false;
                    }
                    if start_button_primary(&mut cols[1], None, "Reset") {
                        *settings = AppSettings::default();
                        *reset_armed = false;
                        reset = true;
                        changed = true;
                    }
                });
            });
    } else if start_button(ui, None, "Reset Settings…") {
        *reset_armed = true;
    }
    ui.add_space(24.0);
    (changed, reset, check_updates)
}

/// Appearance page: palette, paper treatment and the page typeface.
fn settings_appearance(ui: &mut Ui, settings: &mut AppSettings) -> (bool, bool, bool) {
    let mut changed = false;
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

    ui.add_space(20.0);
    kit_section(ui, "Page type");
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
                "Courier New, IBM Plex Mono and Liberation Mono draw Latin and Cyrillic \
                 in one design. Courier Prime is Latin only and borrows Plex for Cyrillic. \
                 All four share 0.6 em advances, so pagination is identical whichever \
                 face you pick.",
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
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.label("Page size:");
        ui.add_space(6.0);
        changed |= egui::ComboBox::from_id_salt("page_size")
            .selected_text(settings.page_size.label())
            .width(220.0)
            .show_ui(ui, |ui| {
                let mut picked = false;
                for size in scriptcraft_core::editing::PageSize::ALL {
                    picked |= ui
                        .selectable_value(&mut settings.page_size, size, size.label())
                        .changed();
                }
                picked
            })
            .inner
            .unwrap_or(false);
    });
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.add_space(16.0);
        ui.label(
            egui::RichText::new(
                "A4 (58 lines/page) is the default. Switch to US Letter (55 lines/page, \
                 Hollywood standard) for North American submissions; you can also override \
                 the template per export.",
            )
            .size(11.0)
            .color(kit().disabled_text),
        );
    });
    (changed, false, false)
}

/// Writing page: what the page shows, what the scene list shows, and the
/// typing corrections applied as you write.
fn settings_writing(ui: &mut Ui, settings: &mut AppSettings) -> (bool, bool, bool) {
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
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        changed |= ui
            .checkbox(
                &mut settings.bold_character_cues,
                "Bold character names (speaking parts)",
            )
            .on_hover_text(
                "Automatically bold character cues above dialogue in the editor and PDF, without bolding mentions in Action.",
            )
            .changed();
    });

    ui.add_space(20.0);
    kit_section(ui, "Scene list");
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        changed |= ui
            .checkbox(
                &mut settings.navigator_show_numbers,
                "Number scenes in the list",
            )
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

/// Spelling page: the checker, its dictionary and the ALL-CAPS exemption.
fn settings_spelling(ui: &mut Ui, settings: &mut AppSettings) -> (bool, bool, bool) {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        changed |= ui
            .checkbox(&mut settings.spellcheck, "Check spelling as you type")
            .on_hover_text(
                "Underlines words the dictionary doesn't know. Right-click one for suggestions.",
            )
            .changed();
    });
    ui.add_space(12.0);
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
    (changed, false, false)
}

/// Saving page: autosave timing and the backup folder.
fn settings_saving(ui: &mut Ui, settings: &mut AppSettings) -> (bool, bool, bool) {
    let mut changed = false;
    let visuals = field_visuals(ui);

    kit_section(ui, "Autosave");
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        let r = ui.checkbox(&mut settings.autosave, "Automatically Save Every");
        changed |= r.changed();
        ui.add_space(6.0);
        changed |= ui
            .scope(|ui| {
                ui.style_mut().visuals = visuals.clone();
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

    ui.add_space(20.0);
    kit_section(ui, "Backups");
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        let r = ui.checkbox(&mut settings.backups, "Save Backups");
        changed |= r.changed();
        r.on_hover_text("Timestamped copies are written here on every save.");
        ui.add_space(6.0);
        let field_w = (ui.available_width() - 44.0).max(80.0);
        changed |= ui
            .scope(|ui| {
                ui.style_mut().visuals = visuals.clone();
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

    /// Nav labels are the only way into a page, so they must be distinct.
    #[test]
    fn every_page_is_listed_once_with_its_own_label() {
        let labels: Vec<&str> = SettingsPage::ALL.iter().map(|p| p.label()).collect();
        let mut unique = labels.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(
            unique.len(),
            labels.len(),
            "duplicate page label: {labels:?}"
        );
        assert_eq!(labels.first(), Some(&"General"), "General opens first");
    }

    /// Search is only honest if every page can be reached through it.
    #[test]
    fn search_covers_every_page() {
        for page in SettingsPage::ALL {
            assert!(
                SETTINGS_INDEX.iter().any(|entry| entry.page == page),
                "{} has no searchable row",
                page.label()
            );
        }
    }

    /// The words a writer is likely to type reach the page that holds the row.
    #[test]
    fn search_finds_rows_across_pages() {
        let hit = |query: &str, title: &str, page: SettingsPage| {
            search_matches(query)
                .iter()
                .any(|entry| entry.title == title && entry.page == page)
        };
        assert!(hit("courier", "Script font", SettingsPage::Appearance));
        assert!(hit("typeface", "Script font", SettingsPage::Appearance));
        assert!(hit("palette", "Theme", SettingsPage::Appearance));
        assert!(hit(
            "autosave",
            "Automatically Save Every",
            SettingsPage::Saving
        ));
        assert!(hit("backup", "Save Backups", SettingsPage::Saving));
        assert!(hit(
            "scene numbers",
            "Show scene numbers",
            SettingsPage::Writing
        ));
        assert!(hit(
            "spell",
            "Check spelling as you type",
            SettingsPage::Spelling
        ));
        assert!(hit("update", "Check for updates", SettingsPage::General));
        assert!(hit("reset", "Reset settings", SettingsPage::General));
        assert!(search_matches("zzzz").is_empty());
        assert!(search_matches("   ").is_empty());
    }

    #[test]
    fn search_ignores_case() {
        assert_eq!(search_matches("COURIER"), search_matches("courier"));
        assert_eq!(
            search_matches("Script   Font"),
            search_matches("script font")
        );
    }

    #[test]
    fn settings_ui_opens_on_general_with_no_search_and_no_armed_reset() {
        let ui = SettingsUi::default();
        assert_eq!(ui.page, SettingsPage::General);
        assert!(ui.search.is_empty());
        assert!(!ui.reset_armed, "reset must never be armed on open");
    }
}
