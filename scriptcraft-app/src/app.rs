use eframe::egui;
use scriptcraft_core::{
    edit,
    editing::{auto_upper, format_duration, RUSSIAN},
    format::{validate, Issue},
    kitsp::{self, Card, Metadata},
    model::{ElementType, Project, Scene, ScriptBlock},
    Stats,
};

// Palette, global style and the painted widgets live in `theme.rs`.
use crate::theme::*;
pub use crate::theme::{apply_theme, kit_visuals, set_kit_theme};
/// Statistics report bar palette (sampled from KIT's summary report).
const REP_GREEN_LIGHT: egui::Color32 = egui::Color32::from_rgb(0xB1, 0xE1, 0x86);
const REP_GREEN: egui::Color32 = egui::Color32::from_rgb(0x2B, 0xA1, 0x21);
const REP_PINK: egui::Color32 = egui::Color32::from_rgb(0xFC, 0x99, 0x98);
const REP_RED: egui::Color32 = egui::Color32::from_rgb(0xD8, 0x4D, 0x4B);
const REP_SAND: egui::Color32 = egui::Color32::from_rgb(0xFD, 0xC0, 0x69);
const REP_ORANGE: egui::Color32 = egui::Color32::from_rgb(0xFE, 0x82, 0x00);
const REP_BLUE: egui::Color32 = egui::Color32::from_rgb(0x69, 0xC6, 0xFF);
pub(crate) const MONO: egui::FontFamily = egui::FontFamily::Monospace;

/// `(block, paragraph, char)` triple used by the Editor 2.0 test hooks.
type PosTuple = (usize, usize, usize);

pub(crate) fn bold_mono() -> egui::FontFamily {
    egui::FontFamily::Name("BoldMono".into())
}

pub(crate) fn italic_mono() -> egui::FontFamily {
    egui::FontFamily::Name("ItalicMono".into())
}

/// Decode a stored KIT research picture (PNG or JPEG) into an egui image.
fn decode_image(bytes: &[u8]) -> Option<egui::ColorImage> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}

fn image_hash(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

pub fn run() -> eframe::Result<()> {
    let app_icon =
        eframe::icon_data::from_png_bytes(include_bytes!("../assets/ScriptCraft-1024.png"))
            .unwrap_or_default();
    let viewport_builder = egui::ViewportBuilder::default()
        .with_inner_size([1360.0, 880.0])
        .with_icon(app_icon);
    // macOS: draw the toolbar behind a transparent native titlebar so the
    // app's dark surface continues to the window edge while the traffic
    // lights stay native (macOS owns their size and color).
    #[cfg(target_os = "macos")]
    let viewport_builder = viewport_builder
        .with_fullsize_content_view(true)
        .with_titlebar_shown(false)
        .with_title_shown(false)
        .with_titlebar_buttons_shown(true);
    let options = eframe::NativeOptions {
        viewport: viewport_builder,
        ..Default::default()
    };
    eframe::run_native(
        "ScriptCraft",
        options,
        Box::new(|cc| {
            apply_theme(&cc.egui_ctx);
            install_fonts(&cc.egui_ctx);
            let mut app = ScriptCraftApp::default();
            app.load_persistent();
            #[cfg(target_os = "macos")]
            {
                app.custom_titlebar = true;
                if let Ok(menu) = crate::menu::NativeMenu::install(
                    app.settings.two_panel,
                    app.show_left_panel,
                    app.show_format_panel,
                    app.typewriter,
                ) {
                    app.menu = Some(menu);
                }
            }
            Ok(Box::new(app))
        }),
    )
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    // UI face: PT Sans (Roboto-spirit humanist, Latin + Cyrillic).
    fonts.font_data.insert(
        "ui_regular".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/PT_Sans-Web-Regular.ttf"))
            .into(),
    );
    fonts.font_data.insert(
        "ui_bold".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/PT_Sans-Web-Bold.ttf")).into(),
    );
    // Script face: Courier Prime (OFL, screenplay metrics), IBM Plex Mono
    // behind it as a safety net (same 0.6em advance, alignment preserved).
    fonts.font_data.insert(
        "courier_prime".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/CourierPrime-Regular.ttf"))
            .into(),
    );
    fonts.font_data.insert(
        "courier_prime_bold".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/CourierPrime-Bold.ttf")).into(),
    );
    fonts.font_data.insert(
        "courier_prime_italic".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/CourierPrime-Italic.ttf"))
            .into(),
    );
    fonts.font_data.insert(
        "plex_mono".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/IBMPlexMono-Regular.ttf"))
            .into(),
    );
    fonts.font_data.insert(
        "plex_mono_bold".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/IBMPlexMono-Bold.ttf")).into(),
    );
    if let Some(prop) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        prop.insert(0, "ui_regular".to_owned());
    }
    if let Some(mono) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        *mono = vec!["courier_prime".to_owned(), "plex_mono".to_owned()]
            .into_iter()
            .chain(mono.iter().cloned())
            .collect();
    }
    fonts.families.insert(
        bold_mono(),
        vec![
            "courier_prime_bold".to_owned(),
            "plex_mono_bold".to_owned(),
            "plex_mono".to_owned(),
        ],
    );
    fonts.families.insert(
        italic_mono(),
        vec![
            "courier_prime_italic".to_owned(),
            "courier_prime".to_owned(),
            "plex_mono".to_owned(),
        ],
    );
    ctx.set_fonts(fonts);
}

/// KIT FastFormat order for the numbered element list.
const FORMAT_ORDER: [(u8, ElementType, &str); 10] = [
    (0, ElementType::SceneHeading, "Scene Heading"),
    (1, ElementType::SceneCharacters, "Scene Characters"),
    (2, ElementType::Action, "Action"),
    (3, ElementType::Character, "Character"),
    (4, ElementType::Dialogue, "Dialogue"),
    (5, ElementType::Parenthetical, "Parenthetical"),
    (6, ElementType::Shot, "Shot"),
    (7, ElementType::Transition, "Transition"),
    (8, ElementType::NonPrintable, "Non-Printable Text"),
    (9, ElementType::Lyrics, "Lyrics"),
];

fn element_label(t: ElementType) -> &'static str {
    FORMAT_ORDER
        .iter()
        .find(|(_, e, _)| *e == t)
        .map(|(_, _, n)| *n)
        .unwrap_or("Action")
}

/// One flat left-aligned KIT format button. Active rows invert to
/// light off-white with dark ink, exactly like the original.
fn fmt_button(ui: &mut egui::Ui, label: &str, active: bool) -> bool {
    let c = kit();
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 30.0), egui::Sense::click());
    let (bg, fg, muted) = if active {
        (
            c.accent,
            egui::Color32::WHITE,
            egui::Color32::from_white_alpha(170),
        )
    } else if resp.hovered() {
        (c.button_hover, c.text, c.disabled_text)
    } else {
        (c.elt_bg, c.text, c.disabled_text)
    };
    ui.painter().rect_filled(rect, 7.0, bg);
    if !active {
        ui.painter().rect_stroke(
            rect,
            7.0,
            egui::Stroke::new(1.0_f32, c.elt_stroke),
            egui::StrokeKind::Inside,
        );
    }
    // "3 Dialogue": the FastFormat number becomes a muted gutter.
    let (num, name) = match label.split_once(' ') {
        Some((n, rest)) if n.len() == 1 && n.chars().all(|ch| ch.is_ascii_digit()) => {
            (Some(n), rest)
        }
        _ => (None, label),
    };
    let mut x = rect.left() + 12.0;
    if let Some(n) = num {
        ui.painter().text(
            egui::pos2(x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            n,
            egui::FontId::proportional(11.5),
            muted,
        );
        x += 18.0;
    }
    ui.painter().text(
        egui::pos2(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        name,
        egui::FontId::proportional(12.5),
        fg,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

/// KIT QMenu-style row: full-width, accent fill on hover, painted check gutter.
fn menu_row(
    ui: &mut egui::Ui,
    label: &str,
    shortcut: Option<&str>,
    enabled: bool,
    checked: Option<bool>,
) -> bool {
    let c = kit();
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), egui::Sense::click());
    let hover = enabled && resp.hovered();
    if hover {
        ui.painter()
            .rect_filled(rect.shrink2(egui::vec2(4.0, 1.0)), 6.0, c.button_hover);
    }
    let fg = if enabled { c.text } else { c.disabled_text };
    let mut x = rect.left() + 14.0;
    if let Some(is_checked) = checked {
        if is_checked {
            let ctr = egui::pos2(x + 4.0, rect.center().y);
            let st = egui::Stroke::new(1.8_f32, c.accent_text);
            ui.painter().line_segment(
                [
                    egui::pos2(ctr.x - 4.0, ctr.y),
                    egui::pos2(ctr.x - 1.0, ctr.y + 3.0),
                ],
                st,
            );
            ui.painter().line_segment(
                [
                    egui::pos2(ctr.x - 1.0, ctr.y + 3.0),
                    egui::pos2(ctr.x + 5.0, ctr.y - 4.0),
                ],
                st,
            );
        }
        x += 20.0;
    }
    ui.painter().text(
        egui::pos2(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0),
        fg,
    );
    if let Some(sc) = shortcut {
        ui.painter().text(
            egui::pos2(rect.right() - 14.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            sc,
            egui::FontId::proportional(12.0),
            c.disabled_text,
        );
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    enabled && resp.clicked()
}

fn menu_item(ui: &mut egui::Ui, label: &str, shortcut: Option<&str>, enabled: bool) -> bool {
    menu_row(ui, label, shortcut, enabled, None)
}

fn menu_check(
    ui: &mut egui::Ui,
    label: &str,
    checked: bool,
    shortcut: Option<&str>,
    enabled: bool,
) -> bool {
    menu_row(ui, label, shortcut, enabled, Some(checked))
}

#[derive(Default, PartialEq, Eq, Clone, Copy)]
pub enum SideTab {
    #[default]
    Start,
    Script,
    Research,
    Cards,
    Statistics,
    Settings,
}

/// Settings module pages (left list): name says it all.
#[derive(Default, PartialEq, Eq, Clone, Copy)]
pub enum SettingsPane {
    #[default]
    Application,
    Research,
    Cards,
    ScriptEditor,
    Navigator,
    DurationCounters,
    TemplatesLibrary,
}

/// Start module project-source tabs.
#[derive(Default, PartialEq, Eq, Clone, Copy, Hash)]
pub enum StartTab {
    #[default]
    Local,
    Online,
}

/// Export dialog modules, mirroring KIT's Research / Outline / Script tabs.
#[derive(Default, PartialEq, Eq, Clone, Copy)]
enum ExportTab {
    Research,
    Outline,
    #[default]
    Script,
}

/// Export dialog file formats, mirroring KIT's format radio group.
#[derive(Default, PartialEq, Eq, Clone, Copy)]
enum ExportFormat {
    #[default]
    Pdf,
    Fdx,
    Fountain,
}

impl ExportFormat {
    fn label(self) -> &'static str {
        match self {
            Self::Pdf => "PDF",
            Self::Fdx => "FDX",
            Self::Fountain => "Fountain",
        }
    }
}

/// Persisted application settings (`~/Library/Application Support/ScriptCraft/settings.conf`).
#[derive(Clone)]
struct AppSettings {
    autosave: bool,
    autosave_minutes: u32,
    backups: bool,
    backups_dir: String,
    spell_check: bool,
    compact_mode: bool,
    two_panel: bool,
    hidpi: bool,
    dark_mode: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            autosave: true,
            autosave_minutes: 5,
            backups: true,
            backups_dir: default_backups_dir(),
            spell_check: false,
            compact_mode: false,
            two_panel: false,
            hidpi: false,
            dark_mode: true,
        }
    }
}

/// Research module selection (KIT's research tree).
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum ResearchPane {
    Script,
    TitlePage,
    Logline,
    Synopsis,
    Versions,
    Characters,
    Locations,
    Documents,
    Entry(i64),
}

/// Statistics module selection (KIT's report tree).
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum StatsPane {
    Summary,
    SceneReport,
    LocationReport,
    CastReport,
    SpeakingParts,
    Structure,
    Activity,
}

/// Editable buffer for one research entry (synced on selection).
#[derive(Debug, Clone)]
struct ResearchEdit {
    id: i64,
    kind: kitsp::ResearchKind,
    name: String,
    url: String,
    description: String,
}

/// How the script zoom is chosen: free pinch/steps, or locked to the
/// viewport (Fit Page / Fit Width) and recomputed as the window resizes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum FitMode {
    #[default]
    Off,
    Page,
    Width,
}

pub struct ScriptCraftApp {
    project: Option<Project>,
    title: String,
    path: String,
    status: String,
    dirty: bool,
    issues: Vec<Issue>,
    stats: Stats,
    /// Cached document layout (paginate + place) for the open project,
    /// recomputed on every edit, never per frame.
    doc: Option<scriptcraft_layout::place::DocLayout>,
    /// Incremental-relayout hint set by the edit that dirtied the project:
    /// `(lowest touched block, last touched block if the edit was in-place
    /// text only)`. Consumed by `refresh_derived`.
    doc_hint: Option<(usize, Option<usize>)>,
    cards: Vec<Card>,
    meta: Metadata,
    research: Vec<kitsp::ResearchEntry>,
    title_logline: String,
    title_synopsis: String,
    research_sel: Option<i64>,
    research_edit: Option<ResearchEdit>,
    /// Decoded picture of the open research entry (id, bytes hash, texture).
    research_tex: Option<(i64, u64, egui::TextureHandle)>,
    research_pane: ResearchPane,
    stats_pane: StatsPane,
    /// "Generated …" stamp of the statistics report.
    stats_gen: String,
    /// Include non-printable notes in statistics totals.
    stats_notes: bool,
    /// Draft versions stored inside the .kitsp (name, datetime).
    versions: Vec<(String, String)>,
    /// Title-page fields edited in the Research view, pending save.
    meta_dirty: bool,
    selected_scene: usize,
    /// The block the Editor 2.0 caret is in (mirrors `editor2.selection`).
    selected_block: Option<usize>,
    side_tab: SideTab,
    /// Start module: which project source tab is active.
    start_tab: StartTab,
    /// Recent projects (path, "dd.mm.yyyy HH:MM"); persisted in recent.txt.
    recent: Vec<(String, String)>,
    /// Settings module: selected page and Common/Modules tab.
    settings_pane: SettingsPane,
    settings_modules: bool,
    settings: AppSettings,
    /// Disk persistence armed (real app only; tests never touch the disk).
    persist: bool,
    last_autosave: std::time::Instant,
    /// Crash-recovery copies found in the recovery dir at launch
    /// (file path, display label = original file name or "untitled").
    recovery: Vec<(std::path::PathBuf, String)>,
    /// Last recovery write; `None` means the next dirty frame writes one.
    last_recovery: Option<std::time::Instant>,
    show_format_panel: bool,
    show_left_panel: bool,
    show_about: bool,
    show_help: bool,
    zoom: f32,
    /// Viewport-locked zoom mode (View ▸ Fit Page / Fit Width).
    fit: FitMode,
    /// Zoom actually painted last frame (toolbar % while a fit mode is on).
    last_render_zoom: f32,
    /// Script viewport the fit modes computed from, in points (tests).
    last_viewport: egui::Vec2,
    typewriter: bool,
    /// Caret/selection and command-based undo history of the script editor.
    editor2: crate::script_view::EditState,
    show_find: bool,
    find_query: String,
    find_replace: String,
    find_idx: usize,
    /// KIT-style Export dialog: module tab, format, and PDF options.
    show_export: bool,
    export_tab: ExportTab,
    export_format: ExportFormat,
    /// Destination chosen via the "Choose file to export" row.
    export_path: String,
    /// "Additional (Numbering, Title Page etc.)" reveals the options panel.
    export_additional: bool,
    export_options: scriptcraft_layout::pdf::ExportOptions,
    export_check_breaks: bool,
    export_watermark_on: bool,
    export_watermark: String,
    drag_scene: Option<usize>,
    drop_before: Option<(usize, bool)>,
    last_window_title: String,
    /// Native macOS menu bar (production only; tests keep it None).
    menu: Option<crate::menu::NativeMenu>,
    /// The toolbar is painted behind a transparent native titlebar (macOS
    /// production window only; tests keep the native titlebar layout).
    custom_titlebar: bool,
}

impl Default for ScriptCraftApp {
    fn default() -> Self {
        Self {
            project: None,
            title: String::new(),
            path: String::new(),
            status: "Open a .kitsp via the File menu, or create a new script.".to_string(),
            dirty: false,
            issues: Vec::new(),
            stats: Stats::default(),
            doc: None,
            doc_hint: None,
            cards: Vec::new(),
            meta: Metadata::default(),
            research: Vec::new(),
            title_logline: String::new(),
            title_synopsis: String::new(),
            research_sel: None,
            research_edit: None,
            research_tex: None,
            research_pane: ResearchPane::Script,
            stats_pane: StatsPane::Summary,
            stats_gen: String::new(),
            stats_notes: false,
            versions: Vec::new(),
            meta_dirty: false,
            selected_scene: 0,
            selected_block: None,
            side_tab: SideTab::default(),
            start_tab: StartTab::default(),
            recent: Vec::new(),
            settings_pane: SettingsPane::default(),
            settings_modules: false,
            settings: AppSettings::default(),
            persist: false,
            last_autosave: std::time::Instant::now(),
            recovery: Vec::new(),
            last_recovery: None,
            show_format_panel: true,
            show_left_panel: true,
            show_about: false,
            show_help: false,
            zoom: 1.0,
            fit: FitMode::Off,
            last_render_zoom: 1.0,
            last_viewport: egui::Vec2::ZERO,
            typewriter: false,
            editor2: Default::default(),
            show_find: false,
            find_query: String::new(),
            find_replace: String::new(),
            find_idx: 0,
            show_export: false,
            export_tab: ExportTab::default(),
            export_format: ExportFormat::default(),
            export_path: String::new(),
            export_additional: false,
            export_options: scriptcraft_layout::pdf::ExportOptions::default(),
            export_check_breaks: true,
            export_watermark_on: false,
            export_watermark: String::new(),
            drag_scene: None,
            drop_before: None,
            last_window_title: String::new(),
            menu: None,
            custom_titlebar: false,
        }
    }
}

impl ScriptCraftApp {
    fn is_fixture_path(path: &str) -> bool {
        path.ends_with("fixtures/A95.kitsp")
    }

    /// Load the bundled A95 sample (tests, snapshots, demo).
    pub fn open_fixture(&mut self) -> bool {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/A95.kitsp");
        if std::path::Path::new(path).exists() {
            self.open_path(path);
            true
        } else {
            false
        }
    }

    pub fn goto_tab(&mut self, tab: SideTab) {
        self.side_tab = tab;
    }

    /// Arm disk persistence and load settings + recent projects.
    /// Called by the real app only — tests stay disk-free.
    pub fn load_persistent(&mut self) {
        self.settings = load_settings();
        self.recent = load_recent();
        self.persist = true;
        self.recovery = Self::scan_recovery_files();
    }

    /// Remember an opened project (most recent first, deduplicated).
    fn push_recent(&mut self, path: &str) {
        self.recent.retain(|(p, _)| p != path);
        self.recent
            .insert(0, (path.to_string(), recent_stamp(path)));
        self.recent.truncate(20);
        if self.persist {
            save_recent(&self.recent);
        }
    }

    /// Re-stat every recent entry so the list shows fresh timestamps.
    fn refresh_recent(&mut self) {
        self.recent = self
            .recent
            .iter()
            .map(|(p, _)| (p.clone(), recent_stamp(p)))
            .collect();
        if self.persist {
            save_recent(&self.recent);
        }
    }

    /// Test hook: pin the statistics "Generated" stamp (deterministic snapshots).
    pub fn pin_stats_generated(&mut self, stamp: &str) {
        self.stats_gen = stamp.to_string();
    }

    /// Test hook: set the persisted theme choice.
    pub fn set_dark_mode(&mut self, dark: bool) {
        self.settings.dark_mode = dark;
    }

    /// Test hook: pin the Start-page recent projects list.
    pub fn pin_recent(&mut self, list: Vec<(String, String)>) {
        self.recent = list;
    }

    /// Test hook: fixed backups folder, so UI snapshots don't depend on `$HOME`.
    pub fn pin_backups_dir(&mut self, dir: &str) {
        self.settings.backups_dir = dir.to_string();
    }

    /// Test hook: pretend a previous session left these recovery copies.
    #[doc(hidden)]
    pub fn pin_recovery_for_test(&mut self, files: Vec<(std::path::PathBuf, String)>) {
        self.recovery = files;
    }

    /// Test hook: pending unsaved changes?
    #[doc(hidden)]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Test hook: current project path ("" = never saved).
    #[doc(hidden)]
    pub fn project_path(&self) -> &str {
        &self.path
    }

    /// Test hook: select a research pane.
    pub fn set_research_pane(&mut self, pane: ResearchPane) {
        self.research_pane = pane;
    }

    /// Test hook: select a statistics pane.
    pub fn set_stats_pane(&mut self, pane: StatsPane) {
        self.stats_pane = pane;
    }

    pub fn set_zoom(&mut self, zoom: f32) {
        self.fit = FitMode::Off;
        self.zoom = zoom.clamp(0.25, 4.0);
    }

    /// Lock the zoom so a whole page fits the script viewport (Fit Page).
    pub fn fit_page(&mut self) {
        self.fit = FitMode::Page;
    }

    /// Lock the zoom so the page width spans the script viewport.
    pub fn fit_width(&mut self) {
        self.fit = FitMode::Width;
    }

    /// Last painted zoom and the viewport it was computed from (tests).
    #[doc(hidden)]
    pub fn last_viewport_zoom(&self) -> (f32, egui::Vec2) {
        (self.last_render_zoom, self.last_viewport)
    }

    /// Scroll the editor to a block (snapshot tests).
    pub fn scroll_to_block(&mut self, bi: usize) {
        let n = self.project.as_ref().map(|p| p.blocks.len()).unwrap_or(0);
        if n > 0 {
            self.goto_block(bi.min(n - 1));
        }
    }

    /// Focus a block with the caret at `offset` (interaction tests).
    #[doc(hidden)]
    pub fn focus_block_at(&mut self, bi: usize, offset: usize) {
        let n = self.project.as_ref().map(|p| p.blocks.len()).unwrap_or(0);
        if n == 0 {
            return;
        }
        let bi = bi.min(n - 1);
        let ch = offset.min(self.block_text(bi).map(|t| t.chars().count()).unwrap_or(0));
        self.scroll_to_block(bi);
        self.editor2_set_caret((bi, 0, ch));
        self.editor2.scroll_caret = true;
    }

    /// First Character block whose text is a known cue (interaction tests).
    #[doc(hidden)]
    pub fn find_first_cue_block(&self) -> Option<usize> {
        let p = self.project.as_ref()?;
        let cues: Vec<String> = p.characters().iter().map(|c| c.to_uppercase()).collect();
        p.blocks.iter().position(|b| {
            b.element_type == ElementType::Character && cues.contains(&b.text.trim().to_uppercase())
        })
    }

    /// First SceneHeading block (interaction tests).
    #[doc(hidden)]
    pub fn find_first_heading_block(&self) -> Option<usize> {
        self.project
            .as_ref()?
            .blocks
            .iter()
            .position(|b| b.element_type == ElementType::SceneHeading)
    }

    /// Text of a block (interaction tests).
    #[doc(hidden)]
    pub fn block_text(&self, bi: usize) -> Option<String> {
        self.project
            .as_ref()?
            .blocks
            .get(bi)
            .map(|b| b.text.clone())
    }

    /// Page count of the cached document layout (interaction tests).
    #[doc(hidden)]
    pub fn page_count(&self) -> usize {
        self.doc.as_ref().map(|d| d.pages.len()).unwrap_or(0)
    }

    /// Editor 2.0 selection as `(anchor, focus)` `(bi, para, ch)` triples
    /// (interaction tests).
    #[doc(hidden)]
    pub fn editor2_selection(&self) -> Option<(PosTuple, PosTuple)> {
        let s = self.editor2.selection?;
        Some((
            (s.anchor.bi, s.anchor.para, s.anchor.ch),
            (s.focus.bi, s.focus.para, s.focus.ch),
        ))
    }

    /// Place the Editor 2.0 caret (interaction tests).
    #[doc(hidden)]
    pub fn editor2_set_caret(&mut self, at: (usize, usize, usize)) {
        let at = edit::Pos {
            bi: at.0,
            para: at.1,
            ch: at.2,
        };
        self.editor2.selection = Some(crate::script_view::Selection {
            anchor: at,
            focus: at,
        });
        self.editor2.autocomplete = None;
    }

    /// Place a whole Editor 2.0 selection (interaction tests).
    #[doc(hidden)]
    pub fn editor2_set_selection(
        &mut self,
        anchor: (usize, usize, usize),
        focus: (usize, usize, usize),
    ) {
        self.editor2.selection = Some(crate::script_view::Selection {
            anchor: edit::Pos {
                bi: anchor.0,
                para: anchor.1,
                ch: anchor.2,
            },
            focus: edit::Pos {
                bi: focus.0,
                para: focus.1,
                ch: focus.2,
            },
        });
    }

    /// Page rectangles from the last Editor 2.0 frame (interaction tests).
    #[doc(hidden)]
    pub fn editor2_page_rects(&self) -> Vec<egui::Rect> {
        self.editor2.debug_pages.clone()
    }

    /// Element type of a block (interaction tests).
    #[doc(hidden)]
    pub fn block_element_type(&self, bi: usize) -> Option<ElementType> {
        Some(self.project.as_ref()?.blocks.get(bi)?.element_type)
    }

    /// Number of blocks in the open project (interaction tests).
    #[doc(hidden)]
    pub fn block_count(&self) -> usize {
        self.project.as_ref().map(|p| p.blocks.len()).unwrap_or(0)
    }

    /// Page count the document layout produces for the open project
    /// (interaction tests).
    #[doc(hidden)]
    pub fn editor2_page_count(&self) -> usize {
        self.page_count()
    }

    /// Caret screen rect from the last Editor 2.0 frame (interaction tests).
    #[doc(hidden)]
    pub fn editor2_caret_rect(&self) -> Option<egui::Rect> {
        self.editor2.caret_rect
    }

    /// Autocomplete suggestion rows from the last painted dropdown;
    /// `(text, screen rect)` (interaction tests).
    #[doc(hidden)]
    pub fn editor2_autocomplete_rows(&self) -> Vec<(String, egui::Rect)> {
        self.editor2.debug_autocomplete.clone()
    }

    fn refresh_derived(&mut self) {
        if let Some(p) = &self.project {
            self.issues = validate(p);
            self.stats = p.stats();
            self.title = p.title.clone();
        }
        // "Lock new scenes numbers": scenes added while numbering is locked
        // keep the number they receive on insert.
        if self.meta.locked_scene_numbers.starts_with('*') {
            let n = self.project.as_ref().map(|p| p.scenes().len()).unwrap_or(0);
            if let Some(mut labels) = self.locked_scene_labels() {
                while labels.len() < n {
                    let i = labels.len();
                    labels.push(format!(
                        "{}{}",
                        self.meta.scene_number_prefix.trim(),
                        self.scene_number_start() + i as u32
                    ));
                }
                self.meta.locked_scene_numbers = format!("*{}", labels.join(","));
            }
        }
        // Rebuild the cached document layout (single source of truth for
        // editor pagination, PDF and the navigator). When the edit that
        // dirtied the project left an incremental hint, resume from the
        // affected page; otherwise (structural ops, undo/redo, open) fall
        // back to a full relayout.
        let hint = self.doc_hint.take();
        let old = self.doc.take();
        self.doc = match &self.project {
            Some(p) => {
                let labels: Vec<String> =
                    (0..p.scenes().len()).map(|i| self.scene_label(i)).collect();
                let incremental = match (hint, old.as_ref()) {
                    (Some((first, upto)), Some(prev)) => scriptcraft_layout::place::relayout_from(
                        &p.blocks,
                        &RUSSIAN,
                        &scriptcraft_layout::place::RUSSIAN_GEOMETRY,
                        &labels,
                        prev,
                        first,
                        upto,
                    ),
                    _ => None,
                };
                incremental.or_else(|| {
                    Some(crate::script_view::document_layout(
                        &p.blocks, &RUSSIAN, &labels,
                    ))
                })
            }
            None => None,
        };
    }

    /// Incremental-relayout hint for a command batch: the lowest touched
    /// block, plus the touched block when every command is an in-place text
    /// edit of that same block (the only case where pagination can be shown
    /// to re-converge and the tail can be reused).
    fn relayout_hint(cmds: &[edit::EditCommand]) -> Option<(usize, Option<usize>)> {
        fn lowest(c: &edit::EditCommand) -> usize {
            match c {
                edit::EditCommand::InsertText { at, .. } => at.bi,
                edit::EditCommand::DeleteRange { start, .. } => start.bi,
                edit::EditCommand::SetText { bi, .. } | edit::EditCommand::SetType { bi, .. } => {
                    *bi
                }
                edit::EditCommand::SplitBlock { at, .. } => at.bi,
                edit::EditCommand::MergeBlocks { bi, .. } => *bi,
                // The block inserted at `at` lands after block `at - 1`, and
                // the orphan guard of `at - 1` starts reading it.
                edit::EditCommand::InsertBlock { at, .. } => at.saturating_sub(1),
                edit::EditCommand::DeleteBlock { bi } => *bi,
                edit::EditCommand::ReplaceAll { .. } => 0,
            }
        }
        fn text_bi(c: &edit::EditCommand) -> Option<usize> {
            match c {
                edit::EditCommand::InsertText { at, .. } => Some(at.bi),
                edit::EditCommand::DeleteRange { start, .. } => Some(start.bi),
                edit::EditCommand::SetText { bi, .. } => Some(*bi),
                _ => None,
            }
        }
        let first = cmds.iter().map(lowest).min()?;
        let same = cmds
            .iter()
            .map(text_bi)
            .collect::<Option<Vec<usize>>>()
            .and_then(|bs| {
                let f = bs[0];
                bs.iter().all(|b| *b == f).then_some(f)
            });
        Some((first, same))
    }

    /// "18.10.2026 16:03:47" — the statistics report stamp.
    fn now_stamp() -> String {
        chrono::Local::now().format("%d.%m.%Y %H:%M:%S").to_string()
    }

    /// Frozen scene-number texts, if numbering is locked.
    fn locked_scene_labels(&self) -> Option<Vec<String>> {
        let raw = self.meta.locked_scene_numbers.trim();
        if raw.is_empty() {
            return None;
        }
        let raw = raw.strip_prefix('*').unwrap_or(raw);
        let v: Vec<String> = raw
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if v.is_empty() {
            None
        } else {
            Some(v)
        }
    }

    fn scene_number_start(&self) -> u32 {
        self.meta.scene_number_start.trim().parse().unwrap_or(1)
    }

    /// "1." / "SC3." — the number shown ahead of a scene heading.
    fn scene_label(&self, i: usize) -> String {
        if let Some(labels) = self.locked_scene_labels() {
            if let Some(l) = labels.get(i) {
                return format!("{l}.");
            }
        }
        format!(
            "{}{}.",
            self.meta.scene_number_prefix.trim(),
            self.scene_number_start() + i as u32
        )
    }

    /// Persist research-view title-page edits once no field has focus.
    fn flush_meta(&mut self) {
        if !self.meta_dirty {
            return;
        }
        if self.project.is_none() {
            self.meta_dirty = false;
            return;
        }
        if Self::is_fixture_path(&self.path) || self.path.is_empty() {
            // Keep the flag: a later Save As must carry these edits.
            return;
        }
        self.do_save_meta();
    }

    fn after_history_jump(&mut self, what: &str) {
        self.sync_card_titles();
        self.dirty = true;
        self.refresh_derived();
        self.status = what.to_string();
    }

    /// Apply Editor 2.0's frame output through its command history.
    fn apply_editor2_output(&mut self, out: crate::script_view::ScriptViewOutput) {
        if out.commands.is_empty() && !out.undo && !out.redo {
            return;
        }
        let Some(p) = self.project.as_mut() else {
            return;
        };
        // Blocks the commands touch (headings born this frame need ids).
        let touched: Vec<usize> = out
            .commands
            .iter()
            .filter_map(|c| match c {
                edit::EditCommand::InsertText { at, .. } => Some(at.bi),
                edit::EditCommand::DeleteRange { start, .. } => Some(start.bi),
                edit::EditCommand::SetType { bi, .. } => Some(*bi),
                edit::EditCommand::SetText { bi, .. } => Some(*bi),
                edit::EditCommand::InsertBlock { at, .. } => Some(*at),
                _ => None,
            })
            .collect();
        let hint = if out.undo || out.redo {
            None
        } else {
            Self::relayout_hint(&out.commands)
        };
        let mut changed = false;
        if out.undo {
            changed |= self.editor2.history.undo(&mut p.blocks).unwrap_or(false);
        }
        if out.redo {
            changed |= self.editor2.history.redo(&mut p.blocks).unwrap_or(false);
        }
        if !out.commands.is_empty()
            && self
                .editor2
                .history
                .apply_seq(&mut p.blocks, out.commands)
                .is_ok()
        {
            changed = true;
        }
        if !changed {
            self.doc_hint = None;
            return;
        }
        if let Some(sel) = self.editor2.selection.as_mut() {
            match (
                edit::clamp_pos(&p.blocks, sel.anchor),
                edit::clamp_pos(&p.blocks, sel.focus),
            ) {
                (Some(anchor), Some(focus)) => {
                    sel.anchor = anchor;
                    sel.focus = focus;
                }
                _ => self.editor2.selection = None,
            }
        }
        // A block that BECAME a scene heading (typing, tab-cycling) needs an
        // identity so the corkboard can track it.
        for bi in touched {
            let fresh = match p.blocks.get_mut(bi) {
                Some(b) if b.element_type == ElementType::SceneHeading && b.kit_uuid.is_none() => {
                    b.kit_uuid = Some(format!("{{{}}}", uuid::Uuid::new_v4()));
                    Some((b.kit_uuid.clone().unwrap(), b.text.clone()))
                }
                _ => None,
            };
            if let Some((id, title)) = fresh {
                if !self.cards.iter().any(|c| c.id == id) {
                    self.cards.push(Card {
                        id,
                        title,
                        description: String::new(),
                    });
                }
            }
        }
        self.selected_block = self.editor2.selection.map(|s| s.focus.bi);
        self.doc_hint = hint;
        let what = if out.undo {
            "Undo."
        } else if out.redo {
            "Redo."
        } else {
            "Edited."
        };
        self.after_history_jump(what);
    }

    fn do_undo(&mut self) {
        self.apply_editor2_output(crate::script_view::ScriptViewOutput {
            undo: true,
            ..Default::default()
        });
    }

    fn do_redo(&mut self) {
        self.apply_editor2_output(crate::script_view::ScriptViewOutput {
            redo: true,
            ..Default::default()
        });
    }

    /// Re-sync all in-memory card titles from headings.
    fn sync_card_titles(&mut self) {
        let Some(p) = &self.project else { return };
        let titles: Vec<(Option<String>, String)> = p
            .blocks
            .iter()
            .filter(|b| b.element_type == ElementType::SceneHeading)
            .map(|b| (b.kit_uuid.clone(), b.text.clone()))
            .collect();
        for card in self.cards.iter_mut() {
            if let Some((_, t)) = titles.iter().find(|(k, _)| *k == Some(card.id.clone())) {
                card.title = t.clone();
            }
        }
    }

    pub fn open_path(&mut self, path: &str) {
        match kitsp::load_file(path) {
            Ok(p) => {
                self.path = path.to_string();
                self.project = Some(p);
                self.selected_scene = 0;
                self.selected_block = None;
                self.editor2 = Default::default();
                self.dirty = false;
                self.cards = kitsp::load_cards(path).unwrap_or_default();
                self.meta = kitsp::load_metadata(path).unwrap_or_default();
                self.research = kitsp::load_research(path).unwrap_or_default();
                self.versions = kitsp::load_versions(path).unwrap_or_default();
                self.research_sel = None;
                self.research_edit = None;
                self.research_pane = ResearchPane::Script;
                self.stats_pane = StatsPane::Summary;
                self.stats_gen = Self::now_stamp();
                self.meta_dirty = false;
                self.title_logline = kitsp::html_to_text(&self.meta.logline_html.clone());
                self.title_synopsis = kitsp::html_to_text(&self.meta.synopsis_html.clone());
                self.refresh_derived();
                let s = &self.stats;
                let pages = self.page_count();
                self.status = if self.issues.is_empty() {
                    format!(
                        "{} scenes · {} pages · {} blocks · {} characters · {} words (~{:.1} pages) — format clean",
                        s.scenes, pages, s.blocks, s.characters, s.words, s.est_pages
                    )
                } else {
                    format!(
                        "{} scenes · {} pages · {} blocks — {} format issues",
                        s.scenes,
                        pages,
                        s.blocks,
                        self.issues.len()
                    )
                };
                self.side_tab = SideTab::Script;
                self.push_recent(path);
            }
            Err(e) => self.status = format!("Open failed: {e}"),
        }
    }

    fn do_new(&mut self) {
        if let Some(dst) = rfd::FileDialog::new()
            .add_filter("KITScenarist", &["kitsp"])
            .set_file_name("Untitled.kitsp")
            .save_file()
        {
            let path = dst.to_string_lossy().into_owned();
            match kitsp::create_file(&dst, "Untitled") {
                Ok(_) => {
                    self.open_path(&path);
                    self.status = format!("New project: {path}");
                }
                Err(e) => self.status = format!("New failed: {e}"),
            }
        }
    }

    fn do_save(&mut self) {
        if Self::is_fixture_path(&self.path) || self.path.is_empty() {
            self.do_save_as();
            return;
        }
        let script = match &self.project {
            Some(p) => kitsp::save_file(&self.path, p),
            None => return,
        };
        match script {
            Ok(()) => {
                // Edited title-page fields ride along with the save; untouched
                // metadata is left exactly as KIT wrote it.
                let meta = if self.meta_dirty {
                    self.write_metadata()
                } else {
                    Ok(())
                };
                self.dirty = false;
                self.last_autosave = std::time::Instant::now();
                self.write_backup();
                self.clear_recovery_files();
                self.status = match meta {
                    Ok(()) => format!("Saved to {} (.bak backup kept)", self.path),
                    Err(e) => format!("Script saved, but title page failed: {e}"),
                };
            }
            Err(e) => self.status = format!("Save failed: {e}"),
        }
    }

    /// Timestamped copy of the project in the configured backups folder.
    fn write_backup(&self) {
        if !self.settings.backups
            || self.path.is_empty()
            || Self::is_fixture_path(&self.path)
            || !self.persist
        {
            return;
        }
        let dir = std::path::PathBuf::from(&self.settings.backups_dir);
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let stem = std::path::Path::new(&self.path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "script".to_string());
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let _ = std::fs::copy(&self.path, dir.join(format!("{stem}_{stamp}.kitsp")));
    }

    // ---- Crash recovery -------------------------------------------------

    fn recovery_dir() -> std::path::PathBuf {
        config_dir().join("recovery")
    }

    /// Stable, filesystem-safe recovery file name for a project path.
    fn recovery_key(project_path: &str) -> String {
        if project_path.is_empty() || Self::is_fixture_path(project_path) {
            return "untitled.kitsp".to_string();
        }
        let stem: String = std::path::Path::new(project_path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .take(40)
            .collect();
        let stem = if stem.is_empty() {
            "script".to_string()
        } else {
            stem
        };
        format!("{stem}-{:016x}.kitsp", fnv1a(project_path))
    }

    /// Recovery copies left behind by a previous (possibly crashed) session.
    fn scan_recovery_files() -> Vec<(std::path::PathBuf, String)> {
        let Ok(entries) = std::fs::read_dir(Self::recovery_dir()) else {
            return Vec::new();
        };
        let mut files: Vec<std::path::PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "kitsp"))
            .collect();
        files.sort();
        files
            .into_iter()
            .map(|p| {
                let label = match kitsp::recovery_origin(&p) {
                    Some(origin) if !origin.is_empty() => std::path::Path::new(&origin)
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or(origin),
                    _ => "untitled script".to_string(),
                };
                (p, label)
            })
            .collect()
    }

    /// Snapshot the dirty project into the recovery dir. Silent on failure:
    /// recovery is a safety net, never a blocker.
    fn write_recovery(&self) {
        let Some(p) = &self.project else {
            return;
        };
        let dir = Self::recovery_dir();
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let origin = if !self.path.is_empty() && !Self::is_fixture_path(&self.path) {
            Some(self.path.as_str())
        } else {
            None
        };
        let dst = dir.join(Self::recovery_key(&self.path));
        if kitsp::write_recovery_file(&dst, origin, p).is_ok() {
            // Carry the title page too, including unsaved field edits.
            let mut m = self.meta.clone();
            m.logline_html = kitsp::text_to_html(&self.title_logline);
            m.synopsis_html = kitsp::text_to_html(&self.title_synopsis);
            let _ = kitsp::save_metadata(&dst, &m);
        }
    }

    /// The current project is safely on disk: drop its recovery copy.
    fn clear_recovery_files(&mut self) {
        self.last_recovery = None;
        let _ = std::fs::remove_file(Self::recovery_dir().join(Self::recovery_key(&self.path)));
        let _ = std::fs::remove_file(Self::recovery_dir().join("untitled.kitsp"));
        self.recovery.retain(|(p, _)| p.exists());
    }

    /// Restore a crashed session: open the recovery copy, point it back at
    /// the original project (if it still exists), and keep it dirty so the
    /// user is prompted to save.
    fn restore_recovery(&mut self, idx: usize) {
        let Some((path, _)) = self.recovery.get(idx).cloned() else {
            return;
        };
        let origin = kitsp::recovery_origin(&path);
        let rec_str = path.to_string_lossy().into_owned();
        self.open_path(&rec_str);
        if self.project.is_none() {
            return;
        }
        self.path = match origin {
            Some(o) if !o.is_empty() && std::path::Path::new(&o).exists() => o,
            _ => String::new(),
        };
        self.dirty = true;
        self.last_recovery = None;
        let _ = std::fs::remove_file(&path);
        self.recovery.retain(|(p, _)| p != &path);
        // Don't leave the (now deleted) recovery copy in the recent list.
        self.recent.retain(|(p, _)| p != &rec_str);
        if !self.path.is_empty() {
            let keep = self.path.clone();
            self.push_recent(&keep);
        } else if self.persist {
            save_recent(&self.recent);
        }
        self.status = "Recovered unsaved changes — Save to keep them.".to_string();
    }

    fn discard_recovery(&mut self, idx: usize) {
        let Some((path, _)) = self.recovery.get(idx).cloned() else {
            return;
        };
        let _ = std::fs::remove_file(&path);
        self.recovery.retain(|(p, _)| p != &path);
    }

    /// Pick an existing .kitsp and open it (menu + Start page).
    fn open_dialog(&mut self) {
        if let Some(src) = rfd::FileDialog::new()
            .add_filter("KITScenarist", &["kitsp"])
            .pick_file()
        {
            self.open_path(&src.to_string_lossy());
        }
    }

    fn do_save_as(&mut self) {
        let Some(p) = self.project.clone() else {
            return;
        };
        if let Some(dst) = rfd::FileDialog::new()
            .add_filter("KITScenarist", &["kitsp"])
            .set_file_name("script.kitsp")
            .save_file()
        {
            if !self.path.is_empty() {
                if let Err(e) = std::fs::copy(&self.path, &dst) {
                    self.status = format!("Save As failed (copy): {e}");
                    return;
                }
                match kitsp::save_file(&dst, &p) {
                    Ok(()) => self.after_save_as(&dst),
                    Err(e) => self.status = format!("Save As failed: {e}"),
                }
            } else if kitsp::create_file(&dst, &p.title).is_ok() {
                match kitsp::save_file(&dst, &p) {
                    Ok(()) => self.after_save_as(&dst),
                    Err(e) => self.status = format!("Save As failed: {e}"),
                }
            } else {
                self.status = "Save As failed (create).".to_string();
            }
        }
    }

    fn after_save_as(&mut self, dst: &std::path::Path) {
        let old = std::mem::replace(&mut self.path, dst.to_string_lossy().into_owned());
        // The old location's recovery copy (if any) is obsolete now.
        if !old.is_empty() && !Self::is_fixture_path(&old) {
            let _ = std::fs::remove_file(Self::recovery_dir().join(Self::recovery_key(&old)));
        }
        // Persist whatever was edited in the title page before the copy is
        // re-read, so Save As cannot drop in-memory metadata edits.
        if self.meta_dirty {
            let _ = self.write_metadata();
        }
        self.cards = kitsp::load_cards(&self.path).unwrap_or_default();
        self.meta = kitsp::load_metadata(&self.path).unwrap_or_default();
        self.research = kitsp::load_research(&self.path).unwrap_or_default();
        self.dirty = false;
        self.last_autosave = std::time::Instant::now();
        self.write_backup();
        self.clear_recovery_files();
        let p = self.path.clone();
        self.push_recent(&p);
        self.status = format!("Saved to {}", self.path);
    }

    /// Import a .fountain file into a new in-memory project (Save As to keep it).
    fn do_import_fountain(&mut self) {
        let Some(src) = rfd::FileDialog::new()
            .add_filter("Fountain", &["fountain", "txt"])
            .pick_file()
        else {
            return;
        };
        let text = match std::fs::read_to_string(&src) {
            Ok(t) => t,
            Err(e) => {
                self.status = format!("Import failed: {e}");
                return;
            }
        };
        let (p, fmeta) = scriptcraft_core::fountain::parse_fountain(&text);
        let meta = kitsp::Metadata {
            title: fmeta.title.clone(),
            genre: fmeta.credit.clone(),
            author: fmeta.author.clone(),
            contacts: fmeta.contact.clone(),
            year: fmeta.draft_date.clone(),
            ..Default::default()
        };
        self.adopt_imported(p, meta, &src.to_string_lossy());
    }

    /// Import a Final Draft `.fdx` file into a new in-memory project.
    fn do_import_fdx(&mut self) {
        let Some(src) = rfd::FileDialog::new()
            .add_filter("Final Draft", &["fdx"])
            .pick_file()
        else {
            return;
        };
        let text = match std::fs::read_to_string(&src) {
            Ok(t) => t,
            Err(e) => {
                self.status = format!("Import failed: {e}");
                return;
            }
        };
        match scriptcraft_core::fdx::from_fdx(&text) {
            Ok((p, meta)) => self.adopt_imported(p, meta, &src.to_string_lossy()),
            Err(e) => self.status = format!("Import failed: {e}"),
        }
    }

    /// Parse FDX text directly (tests; also the future FDX clipboard path).
    #[doc(hidden)]
    pub fn import_fdx_str(&mut self, text: &str) -> Result<(), String> {
        let (p, meta) = scriptcraft_core::fdx::from_fdx(text).map_err(|e| e.to_string())?;
        self.adopt_imported(p, meta, "Final Draft document");
        Ok(())
    }

    /// Replace the session with an imported project, assigning KIT scene
    /// uuids so a later Save As writes a complete `.kitsp`.
    fn adopt_imported(&mut self, mut p: Project, meta: kitsp::Metadata, from: &str) {
        for b in p
            .blocks
            .iter_mut()
            .filter(|b| b.element_type == ElementType::SceneHeading && b.kit_uuid.is_none())
        {
            b.kit_uuid = Some(format!("{{{}}}", uuid::Uuid::new_v4()));
        }
        self.project = Some(p);
        self.meta = meta;
        self.title_logline = kitsp::html_to_text(&self.meta.logline_html);
        self.title_synopsis = kitsp::html_to_text(&self.meta.synopsis_html);
        self.meta_dirty = true;
        self.path = String::new();
        self.selected_scene = 0;
        self.selected_block = None;
        self.editor2 = Default::default();
        self.cards = Vec::new();
        self.research = Vec::new();
        self.dirty = true;
        self.refresh_derived();
        self.status = format!("Imported {} — Save As to keep it as .kitsp.", from);
    }

    /// Render the script to PDF (Courier Prime embedded) with the current
    /// Export-dialog options. `None` (with a status message) on failure.
    fn pdf_bytes(&mut self) -> Option<Vec<u8>> {
        let Some(p) = &self.project else { return None };
        let mut opts = self.export_options.clone();
        opts.watermark = if self.export_watermark_on {
            let text = self.export_watermark.trim();
            Some(if text.is_empty() {
                "DRAFT".to_string()
            } else {
                text.to_string()
            })
        } else {
            None
        };
        let fonts = scriptcraft_layout::pdf::ExportFonts {
            regular: include_bytes!("../assets/fonts/CourierPrime-Regular.ttf"),
            bold: include_bytes!("../assets/fonts/CourierPrime-Bold.ttf"),
            fallback_regular: include_bytes!("../assets/fonts/IBMPlexMono-Regular.ttf"),
            fallback_bold: include_bytes!("../assets/fonts/IBMPlexMono-Bold.ttf"),
        };
        match scriptcraft_layout::pdf::to_pdf_bytes(p, &self.meta, &opts, &fonts) {
            Ok(b) => Some(b),
            Err(e) => {
                self.status = format!("PDF failed: {e}");
                None
            }
        }
    }

    /// Write the PDF to `dst` (the path chosen in the Export dialog).
    fn save_pdf(&mut self, dst: &std::path::Path) {
        let Some(bytes) = self.pdf_bytes() else {
            return;
        };
        match std::fs::write(dst, &bytes) {
            Ok(()) => self.status = format!("Exported to {}", dst.to_string_lossy()),
            Err(e) => self.status = format!("Export failed: {e}"),
        }
    }

    /// Write the Fountain text to `dst` (the path chosen in the Export dialog).
    fn save_fountain(&mut self, dst: &std::path::Path) {
        let Some(p) = &self.project else { return };
        let text = scriptcraft_core::fountain::to_fountain(p, &self.meta);
        match std::fs::write(dst, text) {
            Ok(()) => self.status = format!("Exported to {}", dst.to_string_lossy()),
            Err(e) => self.status = format!("Export failed: {e}"),
        }
    }

    /// Write Final Draft XML to `dst` (the path chosen in the Export dialog).
    fn save_fdx(&mut self, dst: &std::path::Path) {
        let Some(p) = &self.project else { return };
        let xml = scriptcraft_core::fdx::to_fdx(p, &self.meta);
        match std::fs::write(dst, xml) {
            Ok(()) => self.status = format!("Exported to {}", dst.to_string_lossy()),
            Err(e) => self.status = format!("Export failed: {e}"),
        }
    }

    /// Render to a temp file and open it in the system viewer.
    fn preview_pdf(&mut self) {
        let Some(bytes) = self.pdf_bytes() else {
            return;
        };
        {
            let mut path = std::env::temp_dir();
            path.push(format!(
                "{}_preview.pdf",
                if self.title.is_empty() {
                    "script".to_string()
                } else {
                    self.title.clone()
                }
            ));
            match std::fs::write(&path, &bytes) {
                Ok(()) => {
                    self.status = format!("Preview: {}", path.to_string_lossy());
                    #[cfg(target_os = "macos")]
                    let _ = std::process::Command::new("open").arg(&path).spawn();
                    #[cfg(target_os = "windows")]
                    let _ = std::process::Command::new("cmd")
                        .args(["/C", "start", "", &path.to_string_lossy().to_string()])
                        .spawn();
                    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
                    let _ = std::process::Command::new("xdg-open").arg(&path).spawn();
                }
                Err(e) => self.status = format!("Preview failed: {e}"),
            }
        }
    }

    /// Push the plain-text logline/synopsis buffers into the rich-HTML
    /// fields. Must run before any metadata write.
    fn sync_meta_buffers(&mut self) {
        let logline = kitsp::text_to_html(&self.title_logline);
        let synopsis = kitsp::text_to_html(&self.title_synopsis);
        self.meta.logline_html = logline;
        self.meta.synopsis_html = synopsis;
    }

    /// Write title-page metadata to the current project path (no status
    /// side effects). Used by Save/autosave so metadata is never left behind.
    fn write_metadata(&mut self) -> Result<(), kitsp::KitspError> {
        self.sync_meta_buffers();
        kitsp::save_metadata(&self.path, &self.meta)?;
        self.meta_dirty = false;
        Ok(())
    }

    fn do_save_meta(&mut self) {
        if Self::is_fixture_path(&self.path) || self.path.is_empty() {
            self.status = "Save the script first (Save As) — fixture is read-only.".to_string();
            return;
        }
        match self.write_metadata() {
            Ok(()) => {
                if let Some(p) = self.project.as_mut() {
                    p.title = self.meta.title.clone();
                }
                self.refresh_derived();
                self.status = "Title page saved.".to_string();
            }
            Err(e) => self.status = format!("Title save failed: {e}"),
        }
    }

    /// Copy the selected research entry into the edit buffer.
    fn sync_research_buffer(&mut self) {
        self.research_edit = self
            .research
            .iter()
            .find(|e| Some(e.id) == self.research_sel)
            .map(|e| ResearchEdit {
                id: e.id,
                kind: e.kind,
                name: e.name.clone(),
                url: e.url.clone(),
                description: e.description.clone(),
            });
    }

    fn save_research_edit(&mut self) {
        let Some(edit) = self.research_edit.clone() else {
            return;
        };
        if Self::is_fixture_path(&self.path) || self.path.is_empty() {
            self.status = "Save the script first (Save As) — fixture is read-only.".to_string();
            return;
        }
        match kitsp::update_research(
            &self.path,
            edit.id,
            &edit.name,
            &edit.description,
            &edit.url,
        ) {
            Ok(()) => {
                self.research = kitsp::load_research(&self.path).unwrap_or_default();
                self.research_sel = Some(edit.id);
                self.sync_research_buffer();
                self.status = "Research entry saved.".to_string();
            }
            Err(e) => self.status = format!("Research save failed: {e}"),
        }
    }

    fn delete_selected_research(&mut self) {
        let Some(id) = self.research_sel else { return };
        if Self::is_fixture_path(&self.path) || self.path.is_empty() {
            self.status = "Save the script first (Save As) — fixture is read-only.".to_string();
            return;
        }
        match kitsp::delete_research(&self.path, id) {
            Ok(()) => {
                self.research = kitsp::load_research(&self.path).unwrap_or_default();
                self.research_sel = None;
                self.research_edit = None;
                self.status = "Research entry deleted.".to_string();
            }
            Err(e) => self.status = format!("Research delete failed: {e}"),
        }
    }

    fn add_research_entry(&mut self, kind: kitsp::ResearchKind, name: &str) {
        if Self::is_fixture_path(&self.path) || self.path.is_empty() {
            self.status = "Save the script first (Save As) — fixture is read-only.".to_string();
            return;
        }
        match kitsp::insert_research(&self.path, kind, name) {
            Ok(id) => {
                self.research = kitsp::load_research(&self.path).unwrap_or_default();
                self.research_sel = Some(id);
                self.research_pane = ResearchPane::Entry(id);
                self.sync_research_buffer();
                self.status = "Research entry added — edit it on the right.".to_string();
            }
            Err(e) => self.status = format!("Research add failed: {e}"),
        }
    }

    /// Import character cues from the script as research entries.
    fn import_characters_from_script(&mut self) {
        let Some(p) = &self.project else { return };
        if Self::is_fixture_path(&self.path) || self.path.is_empty() {
            self.status = "Save the script first (Save As) — fixture is read-only.".to_string();
            return;
        }
        let mut names: Vec<String> = Vec::new();
        for b in &p.blocks {
            if b.element_type != ElementType::Character {
                continue;
            }
            // Strip parenthetical continuations like "ИВАН (ПРОД.)".
            let mut name = b.text.trim().to_string();
            if let Some(i) = name.find(" (") {
                name.truncate(i);
            }
            let name = name.trim().to_string();
            if name.is_empty() || names.iter().any(|n: &String| n.eq_ignore_ascii_case(&name)) {
                continue;
            }
            names.push(name);
        }
        let existing: Vec<String> = self
            .research
            .iter()
            .filter(|e| e.kind == kitsp::ResearchKind::Character)
            .map(|e| e.name.to_lowercase())
            .collect();
        let mut added = 0;
        for name in names {
            if existing.iter().any(|k| k == &name.to_lowercase()) {
                continue;
            }
            if kitsp::insert_research(&self.path, kitsp::ResearchKind::Character, &name).is_ok() {
                added += 1;
            }
        }
        self.research = kitsp::load_research(&self.path).unwrap_or_default();
        self.status = if added == 0 {
            "All script characters already have research cards.".to_string()
        } else {
            format!("Added {added} character(s) from the script.")
        };
    }

    /// Import locations parsed from scene headings as research entries.
    fn import_locations_from_script(&mut self) {
        let Some(p) = &self.project else { return };
        if Self::is_fixture_path(&self.path) || self.path.is_empty() {
            self.status = "Save the script first (Save As) — fixture is read-only.".to_string();
            return;
        }
        let locs = kitsp::extract_locations(&p.blocks);
        let existing: Vec<String> = self
            .research
            .iter()
            .filter(|e| e.kind == kitsp::ResearchKind::Location)
            .map(|e| e.name.to_lowercase())
            .collect();
        let mut added = 0;
        for loc in locs {
            if existing.iter().any(|k| k == &loc.to_lowercase()) {
                continue;
            }
            if kitsp::insert_research(&self.path, kitsp::ResearchKind::Location, &loc).is_ok() {
                added += 1;
            }
        }
        self.research = kitsp::load_research(&self.path).unwrap_or_default();
        self.status = if added == 0 {
            "All script locations already have research cards.".to_string()
        } else {
            format!("Added {added} location(s) from the script.")
        };
    }

    /// Jump selection + view to an exact block.
    fn goto_block(&mut self, bi: usize) {
        let n = self.project.as_ref().map(|p| p.blocks.len()).unwrap_or(0);
        if n == 0 {
            return;
        }
        let bi = bi.min(n - 1);
        self.selected_block = Some(bi);
        if let Some(p) = &self.project {
            for (si, s) in p.scenes().iter().enumerate() {
                if s.blocks.contains(&bi) {
                    self.selected_scene = si;
                    break;
                }
            }
        }
        let at = edit::Pos { bi, para: 0, ch: 0 };
        self.editor2.selection = Some(crate::script_view::Selection {
            anchor: at,
            focus: at,
        });
        self.editor2.autocomplete = None;
        self.editor2.scroll_caret = true;
    }

    /// Set the selected block's element type (Format bar / toolbar combo).
    fn set_selected_type(&mut self, t: ElementType) {
        let Some(bi) = self.selected_block else {
            return;
        };
        let Some(b) = self.project.as_ref().and_then(|p| p.blocks.get(bi)) else {
            return;
        };
        if b.element_type == t {
            return;
        }
        let mut cmds = vec![edit::EditCommand::SetType { bi, t }];
        if auto_upper(t) && b.text != b.text.to_uppercase() {
            cmds.push(edit::EditCommand::SetText {
                bi,
                text: b.text.to_uppercase(),
            });
        }
        self.apply_editor2_output(crate::script_view::ScriptViewOutput {
            commands: cmds,
            ..Default::default()
        });
    }

    /// Insert an empty block of `t` below the selection and select it.
    fn insert_below(&mut self, t: ElementType) {
        let n = self.project.as_ref().map(|p| p.blocks.len()).unwrap_or(0);
        let at = self.selected_block.map(|bi| bi + 1).unwrap_or(n).min(n);
        let b = ScriptBlock::new(t, String::new());
        self.apply_editor2_output(crate::script_view::ScriptViewOutput {
            commands: vec![edit::EditCommand::InsertBlock { at, block: b }],
            ..Default::default()
        });
        let pos = edit::Pos {
            bi: at,
            para: 0,
            ch: 0,
        };
        self.editor2.selection = Some(crate::script_view::Selection {
            anchor: pos,
            focus: pos,
        });
        self.editor2.scroll_caret = true;
        self.selected_block = Some(at);
    }

    /// Insert a new scene heading after the current scene.
    fn insert_scene_below(&mut self) {
        let Some(p) = self.project.as_ref() else {
            return;
        };
        let scenes = p.scenes();
        let at = scenes
            .get(self.selected_scene)
            .and_then(|s| s.blocks.last().copied().map(|b| b + 1))
            .unwrap_or(p.blocks.len());
        let before = p.blocks.clone();
        let mut b = ScriptBlock::new(
            ElementType::SceneHeading,
            "INT. NEW SCENE - DAY".to_string(),
        );
        b.kit_uuid = Some(format!("{{{}}}", uuid::Uuid::new_v4()));
        self.cards.push(Card {
            id: b.kit_uuid.clone().unwrap_or_default(),
            title: b.text.clone(),
            description: String::new(),
        });
        let Some(p) = self.project.as_mut() else {
            return;
        };
        p.blocks.insert(at, b);
        self.record_structural(before);
        self.goto_block(at);
    }

    /// Delete the whole current scene (undoable).
    fn delete_scene(&mut self) {
        let Some(p) = self.project.as_ref() else {
            return;
        };
        let scenes = p.scenes();
        if scenes.len() <= 1 {
            self.status = "Cannot delete the only scene.".to_string();
            return;
        }
        let Some(scene) = scenes.get(self.selected_scene) else {
            return;
        };
        let kill: std::collections::HashSet<usize> = scene.blocks.iter().copied().collect();
        let dead_uuids: Vec<String> = scene
            .blocks
            .iter()
            .filter_map(|&bi| p.blocks.get(bi))
            .filter(|b| b.element_type == ElementType::SceneHeading)
            .filter_map(|b| b.kit_uuid.clone())
            .collect();
        let before = p.blocks.clone();
        if let Some(p) = self.project.as_mut() {
            let mut kept = Vec::with_capacity(p.blocks.len());
            for (i, b) in p.blocks.drain(..).enumerate() {
                if !kill.contains(&i) {
                    kept.push(b);
                }
            }
            p.blocks = kept;
        }
        self.cards.retain(|c| !dead_uuids.contains(&c.id));
        self.record_structural(before);
        self.selected_scene = self.selected_scene.saturating_sub(1);
        self.editor2.selection = None;
        self.editor2.autocomplete = None;
        self.selected_block = None;
        self.after_history_jump("Scene deleted.");
    }

    /// Record a whole-document change (structural ops) as one undo step.
    fn record_structural(&mut self, before: Vec<ScriptBlock>) {
        self.editor2
            .history
            .record(vec![edit::EditCommand::ReplaceAll { blocks: before }]);
        self.dirty = true;
        self.refresh_derived();
    }

    /// Move selection to the previous/next block (Format Prev/Next, arrows).
    fn step_selection(&mut self, dir: i32) {
        let n = self.project.as_ref().map(|p| p.blocks.len()).unwrap_or(0);
        if n == 0 {
            return;
        }
        let cur = self
            .editor2
            .selection
            .map(|s| s.focus.bi)
            .or(self.selected_block)
            .unwrap_or(if dir >= 0 { 0 } else { n - 1 });
        let next = (cur as i32 + dir).clamp(0, n as i32 - 1) as usize;
        self.goto_block(next);
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// One flat KIT-style toolbar icon button (painted — no font glyphs, so
/// no tofu boxes on systems/fonts without the arrow symbols).
fn toolbar_icon_button(
    ui: &mut egui::Ui,
    enabled: bool,
    tip: &str,
    draw: impl FnOnce(&egui::Painter, egui::Pos2, f32, egui::Color32),
) -> egui::Response {
    let c = kit();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(30.0, 28.0), egui::Sense::click());
    if enabled && resp.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, 7.0, c.field_stroke);
    } else if enabled && resp.hovered() {
        ui.painter().rect_filled(rect, 7.0, c.button_hover);
    }
    let color = if enabled {
        c.text
    } else {
        c.disabled_text.gamma_multiply(0.5)
    };
    draw(ui.painter(), rect.center(), 15.0, color);
    let resp = if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    };
    resp.on_hover_text(tip)
}

/// Curved undo/redo arrow, drawn as a top arc plus a small arrowhead.
fn draw_undo_arrow(painter: &egui::Painter, center: egui::Pos2, size: f32, color: egui::Color32) {
    draw_curved_arrow(painter, center, size, color, false);
}

fn draw_redo_arrow(painter: &egui::Painter, center: egui::Pos2, size: f32, color: egui::Color32) {
    draw_curved_arrow(painter, center, size, color, true);
}

fn draw_curved_arrow(
    painter: &egui::Painter,
    center: egui::Pos2,
    size: f32,
    color: egui::Color32,
    mirrored: bool,
) {
    let x = if mirrored { -1.0_f32 } else { 1.0 };
    let r = size * 0.30;
    let c = egui::pos2(center.x, center.y + size * 0.08);
    let pts: Vec<egui::Pos2> = (0..=12)
        .map(|i| {
            let a = std::f32::consts::PI * (1.0 + i as f32 / 12.0);
            egui::pos2(c.x + x * r * a.cos(), c.y + r * a.sin())
        })
        .collect();
    painter.add(egui::Shape::line(pts, egui::Stroke::new(1.5_f32, color)));
    let ex = c.x - x * r;
    let ey = c.y;
    let tip = egui::pos2(ex - x * size * 0.02, ey + size * 0.17);
    let a1 = egui::pos2(ex + x * size * 0.03, ey - size * 0.01);
    let a2 = egui::pos2(ex - x * size * 0.12, ey - size * 0.05);
    painter.add(egui::Shape::convex_polygon(
        vec![tip, a1, a2],
        color,
        egui::Stroke::NONE,
    ));
}

fn draw_plus(painter: &egui::Painter, center: egui::Pos2, size: f32, color: egui::Color32) {
    let s = size * 0.30;
    painter.line_segment(
        [center - egui::vec2(s, 0.0), center + egui::vec2(s, 0.0)],
        egui::Stroke::new(1.6_f32, color),
    );
    painter.line_segment(
        [center - egui::vec2(0.0, s), center + egui::vec2(0.0, s)],
        egui::Stroke::new(1.6_f32, color),
    );
}

fn draw_minus(painter: &egui::Painter, center: egui::Pos2, size: f32, color: egui::Color32) {
    let s = size * 0.30;
    painter.line_segment(
        [center - egui::vec2(s, 0.0), center + egui::vec2(s, 0.0)],
        egui::Stroke::new(1.6_f32, color),
    );
}

/// Module rail icon, drawn with strokes (no icon font needed).
fn draw_rail_icon(painter: &egui::Painter, tab: SideTab, center: egui::Pos2, color: egui::Color32) {
    let stroke = egui::Stroke::new(1.3_f32, color);
    match tab {
        SideTab::Script => {
            // Document with text lines.
            let r = egui::Rect::from_center_size(center, egui::vec2(11.0, 14.0));
            painter.rect_stroke(r, 1.0, stroke, egui::StrokeKind::Inside);
            for i in 0..3 {
                let y = r.top() + 4.0 + i as f32 * 3.2;
                painter.line_segment(
                    [
                        egui::pos2(r.left() + 2.5, y),
                        egui::pos2(r.right() - 2.5, y),
                    ],
                    egui::Stroke::new(1.1_f32, color),
                );
            }
        }
        SideTab::Start => {
            // 3×3 module grid (KIT's Start page glyph).
            for dx in [-6.5_f32, 0.0, 6.5] {
                for dy in [-6.5_f32, 0.0, 6.5] {
                    let r = egui::Rect::from_center_size(
                        center + egui::vec2(dx, dy),
                        egui::vec2(4.6, 4.6),
                    );
                    painter.rect_filled(r, 0.8, color);
                }
            }
        }
        SideTab::Research => {
            // Magnifier.
            painter.circle_stroke(
                center - egui::vec2(1.5, 1.5),
                4.5,
                egui::Stroke::new(1.4_f32, color),
            );
            painter.line_segment(
                [center + egui::vec2(1.8, 1.8), center + egui::vec2(5.0, 5.0)],
                egui::Stroke::new(1.6_f32, color),
            );
        }
        SideTab::Cards => {
            // Corkboard grid.
            for (dx, dy) in [(-4.5, -4.5), (0.5, -4.5), (-4.5, 0.5), (0.5, 0.5)] {
                let r =
                    egui::Rect::from_min_size(center + egui::vec2(dx, dy), egui::vec2(4.0, 4.0));
                painter.rect_stroke(
                    r,
                    0.5,
                    egui::Stroke::new(1.2_f32, color),
                    egui::StrokeKind::Inside,
                );
            }
        }
        SideTab::Statistics => {
            // Bar chart.
            for (i, h) in [4.0_f32, 7.0, 10.0].iter().enumerate() {
                let x = center.x - 4.5 + i as f32 * 4.5;
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(x - 1.4, center.y + 5.0 - h),
                        egui::pos2(x + 1.4, center.y + 5.0),
                    ),
                    0.5,
                    color,
                );
            }
        }
        SideTab::Settings => {
            // Gear: ring + radial teeth.
            painter.circle_stroke(center, 4.2, egui::Stroke::new(1.4_f32, color));
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::TAU / 8.0;
                let (s, c) = a.sin_cos();
                painter.line_segment(
                    [
                        center + egui::vec2(4.2 * c, 4.2 * s),
                        center + egui::vec2(6.4 * c, 6.4 * s),
                    ],
                    egui::Stroke::new(1.4_f32, color),
                );
            }
        }
    }
}

/// Slim KIT-style module header strip (title + optional hint, 1px rule).
fn module_header(ui: &mut egui::Ui, title: &str, hint: Option<&str>) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).size(20.0));
        if let Some(h) = hint {
            ui.add_space(12.0);
            ui.label(egui::RichText::new(h).small().weak());
        }
    });
    ui.add_space(6.0);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, kit().sep);
    ui.add_space(8.0);
}

/// Flat line icons for the KIT-style trees and module strips.
#[derive(Clone, Copy, PartialEq, Eq)]
enum KitIcon {
    TriangleDown,
    TriangleRight,
    Script,
    Book,
    Text,
    Paragraph,
    Layers,
    Person,
    House,
    File,
    Folder,
    Chart,
    Plot,
    Gear,
    Refresh,
    Floppy,
    Printer,
    LockClosed,
    LockOpen,
    Plus,
    Help,
}

fn draw_kit_icon(p: &egui::Painter, icon: KitIcon, c: egui::Pos2, color: egui::Color32) {
    let stroke = egui::Stroke::new(1.3_f32, color);
    let pt = |x: f32, y: f32| egui::pos2(c.x + x, c.y + y);
    let poly = |points: Vec<egui::Pos2>| {
        p.add(egui::Shape::convex_polygon(
            points,
            color,
            egui::Stroke::NONE,
        ));
    };
    let line = |pts: Vec<egui::Pos2>| p.add(egui::Shape::line(pts, stroke));
    match icon {
        KitIcon::TriangleDown => poly(vec![pt(-4.5, -2.5), pt(4.5, -2.5), pt(0.0, 3.0)]),
        KitIcon::TriangleRight => poly(vec![pt(-2.5, -4.5), pt(-2.5, 4.5), pt(3.0, 0.0)]),
        KitIcon::Script => {
            p.rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(11.0, 14.0)),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            for i in 0..3 {
                let y = -3.5 + i as f32 * 3.4;
                line(vec![pt(-2.5, y), pt(2.5, y)]);
            }
        }
        KitIcon::Book => {
            p.rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(12.0, 13.0)),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            line(vec![pt(0.0, -6.5), pt(0.0, 6.5)]);
        }
        KitIcon::Text => {
            for i in 0..3 {
                let y = -4.0 + i as f32 * 4.0;
                let x1 = if i == 2 { 1.5 } else { 6.0 };
                line(vec![pt(-6.0, y), pt(x1, y)]);
            }
        }
        KitIcon::Paragraph => {
            for i in 0..4 {
                let y = -5.4 + i as f32 * 3.6;
                let x1 = if i == 3 { 0.0 } else { 6.0 };
                line(vec![pt(-6.0, y), pt(x1, y)]);
            }
        }
        KitIcon::Layers => {
            for i in 0..3 {
                let o = i as f32 * 2.6;
                p.rect_stroke(
                    egui::Rect::from_min_size(
                        egui::pos2(c.x - 5.0 + o, c.y - 4.5 + o),
                        egui::vec2(9.0, 6.0),
                    ),
                    1.0,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
        }
        KitIcon::Person => {
            p.circle_stroke(pt(0.0, -3.4), 2.6, stroke);
            line(vec![
                pt(-4.6, 5.0),
                pt(-3.0, 0.8),
                pt(3.0, 0.8),
                pt(4.6, 5.0),
            ]);
        }
        KitIcon::House => {
            line(vec![pt(-5.0, 0.6), pt(0.0, -4.6), pt(5.0, 0.6)]);
            p.rect_stroke(
                egui::Rect::from_min_max(pt(-3.4, 0.6), pt(3.4, 5.0)),
                0.0,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        KitIcon::File => {
            p.rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(10.5, 13.0)),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            line(vec![pt(1.5, -6.5), pt(1.5, -3.0), pt(5.2, -3.0)]);
            line(vec![pt(-2.8, 0.4), pt(2.8, 0.4)]);
            line(vec![pt(-2.8, 3.0), pt(2.8, 3.0)]);
        }
        KitIcon::Folder => {
            line(vec![
                pt(-6.0, 5.0),
                pt(-6.0, -3.6),
                pt(-2.0, -3.6),
                pt(0.0, -1.4),
                pt(6.0, -1.4),
                pt(6.0, 5.0),
                pt(-6.0, 5.0),
            ]);
        }
        KitIcon::Chart => {
            for (x, h) in [(-4.6f32, 6.0f32), (0.0, 10.0), (4.6, 4.0)] {
                p.rect_filled(
                    egui::Rect::from_min_max(pt(x - 1.6, 5.0 - h), pt(x + 1.6, 5.0)),
                    0.0,
                    color,
                );
            }
        }
        KitIcon::Plot => {
            line(vec![
                pt(-6.0, 3.8),
                pt(-2.0, 0.2),
                pt(2.0, 2.2),
                pt(6.0, -4.2),
            ]);
        }
        KitIcon::Gear => {
            p.circle_stroke(c, 3.6, stroke);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let (s, co) = a.sin_cos();
                line(vec![pt(s * 4.4, co * 4.4), pt(s * 6.6, co * 6.6)]);
            }
        }
        KitIcon::Refresh => {
            let mut pts = Vec::new();
            for i in 0..=12 {
                let a = 0.9 + i as f32 * (std::f32::consts::TAU * 0.78 / 12.0);
                let (s, co) = a.sin_cos();
                pts.push(pt(s * 5.2, co * 5.2));
            }
            let last = *pts.last().unwrap();
            line(pts);
            poly(vec![
                last,
                egui::pos2(last.x + 3.4, last.y - 0.6),
                egui::pos2(last.x + 1.0, last.y + 3.0),
            ]);
        }
        KitIcon::Floppy => {
            p.rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(13.0, 13.0)),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            p.rect_stroke(
                egui::Rect::from_min_size(pt(-3.0, -6.5), egui::vec2(6.0, 4.4)),
                0.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            p.rect_stroke(
                egui::Rect::from_min_size(pt(-4.0, 1.0), egui::vec2(8.0, 5.2)),
                0.0,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        KitIcon::Printer => {
            p.rect_stroke(
                egui::Rect::from_min_size(pt(-3.4, -6.6), egui::vec2(6.8, 5.0)),
                0.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            p.rect_stroke(
                egui::Rect::from_min_size(pt(-6.2, -1.6), egui::vec2(12.4, 5.6)),
                0.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            line(vec![pt(-4.6, 1.6), pt(4.6, 1.6)]);
            p.rect_stroke(
                egui::Rect::from_min_size(pt(-3.4, 4.0), egui::vec2(6.8, 2.8)),
                0.0,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        KitIcon::LockClosed => {
            p.circle_stroke(pt(0.0, -2.6), 2.8, stroke);
            p.rect_filled(
                egui::Rect::from_min_size(pt(-4.6, -0.8), egui::vec2(9.2, 6.0)),
                1.0,
                color,
            );
        }
        KitIcon::LockOpen => {
            p.circle_stroke(pt(1.8, -3.2), 2.6, stroke);
            p.rect_filled(
                egui::Rect::from_min_size(pt(-4.6, -0.4), egui::vec2(9.2, 6.0)),
                1.0,
                color,
            );
        }
        KitIcon::Plus => {
            line(vec![pt(-5.0, 0.0), pt(5.0, 0.0)]);
            line(vec![pt(0.0, -5.0), pt(0.0, 5.0)]);
        }
        KitIcon::Help => {
            p.circle_stroke(c, 6.2, stroke);
            p.text(
                pt(0.0, 0.4),
                egui::Align2::CENTER_CENTER,
                "?",
                egui::FontId::proportional(8.5),
                color,
            );
        }
    }
}

/// One row in a KIT-style module tree; nest rows inside an indent block for
/// expanded folders. Returns (row response, arrow clicked, plus clicked).
struct KitTreeRow {
    sel: bool,
    arrow: Option<bool>,
    plus: bool,
}

fn kit_tree_row(
    ui: &mut egui::Ui,
    row: KitTreeRow,
    icon: KitIcon,
    label: &str,
    id_salt: impl std::hash::Hash,
) -> (egui::Response, bool, bool) {
    let c = kit();
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 34.0), egui::Sense::click());
    let inner = rect.shrink2(egui::vec2(6.0, 2.0));
    if row.sel {
        ui.painter().rect_filled(inner, 7.0, c.accent_soft);
    } else if resp.hovered() {
        ui.painter().rect_filled(inner, 7.0, c.hover_light);
    }
    let ink = if row.sel { c.accent_text } else { c.text };
    draw_kit_icon(
        ui.painter(),
        icon,
        egui::pos2(rect.left() + 28.0, rect.center().y),
        ink,
    );
    ui.painter().text(
        egui::pos2(rect.left() + 46.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.5),
        if row.sel { c.heading_text } else { c.text },
    );
    let mut arrow_hit = false;
    if let Some(open) = row.arrow {
        let a = egui::Rect::from_center_size(
            egui::pos2(rect.left() + 14.0, rect.center().y),
            egui::Vec2::splat(20.0),
        );
        draw_kit_icon(
            ui.painter(),
            if open {
                KitIcon::TriangleDown
            } else {
                KitIcon::TriangleRight
            },
            a.center(),
            c.disabled_text,
        );
        arrow_hit = ui
            .interact(a, ui.id().with(("tw", &id_salt)), egui::Sense::click())
            .clicked();
    }
    let mut plus_hit = false;
    if row.plus {
        let b = egui::Rect::from_center_size(
            egui::pos2(rect.right() - 20.0, rect.center().y),
            egui::Vec2::splat(22.0),
        );
        let r = ui.interact(b, ui.id().with(("add", &id_salt)), egui::Sense::click());
        if r.hovered() {
            ui.painter().rect_filled(b, 6.0, c.button_hover);
        }
        draw_kit_icon(ui.painter(), KitIcon::Plus, b.center(), c.text);
        plus_hit = r.clicked();
    }
    (resp, arrow_hit, plus_hit)
}

enum StripItem<'a> {
    Icon(KitIcon, &'a str),
    Sep,
}

/// KIT module strip ("Research" / "Statistics" + right-aligned icon buttons).
fn kit_module_strip(
    ui: &mut egui::Ui,
    title: &str,
    items: &[StripItem<'_>],
) -> Vec<egui::Response> {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 38.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, kit().strip);
    ui.painter().text(
        egui::pos2(rect.left() + 16.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(14.0),
        kit().text,
    );
    let mut x = rect.right() - 6.0;
    let mut out: Vec<(usize, egui::Response)> = Vec::new();
    for (idx, item) in items.iter().enumerate().rev() {
        match item {
            StripItem::Sep => {
                x -= 7.0;
                ui.painter().line_segment(
                    [
                        egui::pos2(x, rect.top() + 9.0),
                        egui::pos2(x, rect.bottom() - 9.0),
                    ],
                    egui::Stroke::new(1.0_f32, kit().stroke),
                );
                x -= 7.0;
            }
            StripItem::Icon(icon, tip) => {
                let slot = egui::Rect::from_min_size(
                    egui::pos2(x - 32.0, rect.top() + 2.0),
                    egui::vec2(32.0, rect.height() - 4.0),
                );
                let r = ui.interact(
                    slot,
                    ui.id().with(("strip", title, idx)),
                    egui::Sense::click(),
                );
                if r.hovered() {
                    ui.painter().rect_filled(slot, 7.0, kit().button_hover);
                }
                let icon_color = if kit_is_light() {
                    kit().text
                } else {
                    egui::Color32::WHITE
                };
                draw_kit_icon(ui.painter(), *icon, slot.center(), icon_color);
                out.push((idx, r.on_hover_text(*tip)));
                x -= 36.0;
            }
        }
    }
    ui.painter().line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        egui::Stroke::new(1.0_f32, kit().stroke),
    );
    out.sort_by_key(|(i, _)| *i);
    out.into_iter().map(|(_, r)| r).collect()
}

/// Input-box visuals for the lighter Research/Statistics module panes.
fn kit_field_visuals(ui: &egui::Ui) -> egui::Visuals {
    let mut v = ui.visuals().clone();
    v.extreme_bg_color = kit().module_bg;
    v.selection.stroke = egui::Stroke::new(1.0_f32, kit().accent);
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, kit().field_stroke);
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, kit().field_stroke);
    v.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, kit().field_stroke);
    v
}

/// KIT labeled single-line field (label above, filled box below).
fn kit_field(ui: &mut egui::Ui, label: &str, text: &mut String) -> bool {
    ui.label(egui::RichText::new(label).size(13.0));
    ui.add_space(4.0);
    let v = kit_field_visuals(ui);
    let changed = ui
        .scope(|ui| {
            ui.style_mut().visuals = v;
            ui.add(
                egui::TextEdit::singleline(text)
                    .margin(egui::Margin::symmetric(10, 5))
                    .font(egui::FontId::proportional(13.0))
                    .desired_width(f32::INFINITY),
            )
            .changed()
        })
        .inner;
    ui.add_space(10.0);
    changed
}

fn kit_field_multi(ui: &mut egui::Ui, label: &str, text: &mut String, rows: usize) -> bool {
    ui.label(egui::RichText::new(label).size(13.0));
    ui.add_space(4.0);
    let v = kit_field_visuals(ui);
    let changed = ui
        .scope(|ui| {
            ui.style_mut().visuals = v;
            ui.add(
                egui::TextEdit::multiline(text)
                    .margin(egui::Margin::symmetric(10, 6))
                    .font(egui::FontId::proportional(13.0))
                    .desired_rows(rows)
                    .desired_width(f32::INFINITY),
            )
            .changed()
        })
        .inner;
    ui.add_space(10.0);
    changed
}

/// Filled monospace editor (logline / synopsis panes).
fn kit_mono_editor(ui: &mut egui::Ui, text: &mut String, height: f32) -> bool {
    let v = kit_field_visuals(ui);
    let size = egui::vec2(ui.available_width(), height);
    ui.scope(|ui| {
        ui.style_mut().visuals = v;
        ui.add_sized(
            size,
            egui::TextEdit::multiline(text)
                .margin(egui::Margin::symmetric(10, 8))
                .font(egui::FontId::new(13.0, MONO)),
        )
        .changed()
    })
    .inner
}

fn kit_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(
        text.to_string(),
        egui::FontId::proportional(13.0),
        kit().text,
    );
    let w = (galley.size().x + 28.0).max(96.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 30.0), egui::Sense::click());
    let fill = button_fill(&resp);
    ui.painter().rect_filled(rect, 7.0, fill);
    ui.painter().rect_stroke(
        rect,
        7.0,
        egui::Stroke::new(1.0_f32, kit().stroke),
        egui::StrokeKind::Inside,
    );
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, kit().text);
    resp
}

fn kit_icon_button(ui: &mut egui::Ui, icon: KitIcon, text: &str) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(
        text.to_string(),
        egui::FontId::proportional(13.0),
        kit().text,
    );
    let w = (galley.size().x + 50.0).max(112.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 30.0), egui::Sense::click());
    let fill = button_fill(&resp);
    let p = ui.painter();
    p.rect_filled(rect, 7.0, fill);
    p.rect_stroke(
        rect,
        7.0,
        egui::Stroke::new(1.0_f32, kit().stroke),
        egui::StrokeKind::Inside,
    );
    draw_kit_icon(
        p,
        icon,
        egui::pos2(rect.left() + 16.0, rect.center().y),
        kit().text,
    );
    p.galley(
        egui::pos2(rect.left() + 28.0, rect.center().y - galley.size().y / 2.0),
        galley,
        kit().text,
    );
    resp
}

/// Full-width Start-page button: optional left icon + label, 34pt tall.
fn start_button(ui: &mut egui::Ui, icon: Option<KitIcon>, text: &str) -> bool {
    start_button_styled(ui, icon, text, false)
}

/// Primary (filled accent) variant for the one main action on a page.
fn start_button_primary(ui: &mut egui::Ui, icon: Option<KitIcon>, text: &str) -> bool {
    start_button_styled(ui, icon, text, true)
}

fn start_button_styled(
    ui: &mut egui::Ui,
    icon: Option<KitIcon>,
    text: &str,
    primary: bool,
) -> bool {
    let c = kit();
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 38.0), egui::Sense::click());
    let (fill, ink) = if primary {
        let f = if resp.is_pointer_button_down_on() {
            c.accent.lerp_to_gamma(egui::Color32::BLACK, 0.15)
        } else if resp.hovered() {
            c.accent.lerp_to_gamma(egui::Color32::WHITE, 0.12)
        } else {
            c.accent
        };
        (f, egui::Color32::WHITE)
    } else {
        (button_fill(&resp), c.text)
    };
    ui.painter().rect_filled(rect, 9.0, fill);
    if !primary {
        ui.painter().rect_stroke(
            rect,
            9.0,
            egui::Stroke::new(1.0_f32, c.sep),
            egui::StrokeKind::Inside,
        );
    }
    let mut x = rect.left() + 14.0;
    if let Some(ic) = icon {
        draw_kit_icon(ui.painter(), ic, egui::pos2(x + 7.0, rect.center().y), ink);
        x += 26.0;
    }
    ui.painter().text(
        egui::pos2(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        text,
        egui::FontId::proportional(13.5),
        ink,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

/// Small square glyph button (folder pickers, strip icons).
fn kit_square_button(ui: &mut egui::Ui, icon: KitIcon) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(30.0, 28.0), egui::Sense::click());
    ui.painter().rect_filled(rect, 7.0, button_fill(&resp));
    draw_kit_icon(ui.painter(), icon, rect.center(), kit().text);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// One row of the Settings module's left page list.
fn settings_row(ui: &mut egui::Ui, icon: KitIcon, name: &str, selected: bool) -> egui::Response {
    let c = kit();
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 38.0), egui::Sense::click());
    let inner = rect.shrink2(egui::vec2(8.0, 2.0));
    if selected {
        ui.painter().rect_filled(inner, 8.0, c.accent_soft);
    } else if resp.hovered() {
        ui.painter().rect_filled(inner, 8.0, c.hover_light);
    }
    draw_kit_icon(
        ui.painter(),
        icon,
        egui::pos2(rect.left() + 26.0, rect.center().y),
        if selected { c.accent_text } else { c.text },
    );
    ui.painter().text(
        egui::pos2(rect.left() + 44.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        name,
        egui::FontId::proportional(13.5),
        if selected { c.heading_text } else { c.text },
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Common / Modules tab header of the Settings module.
fn settings_tab(ui: &mut egui::Ui, text: &str, selected: bool) -> egui::Response {
    let g = ui.painter().layout_no_wrap(
        text.to_string(),
        egui::FontId::proportional(14.0),
        kit().text,
    );
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(g.size().x + 18.0, 30.0), egui::Sense::click());
    let color = if selected {
        if kit_is_light() {
            kit().text
        } else {
            egui::Color32::WHITE
        }
    } else {
        kit().disabled_text
    };
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(14.0),
        color,
    );
    if selected {
        let bar = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 5.0, rect.bottom() - 2.0),
            egui::vec2(rect.width() - 10.0, 2.0),
        );
        ui.painter().rect_filled(bar, 1.0, kit().accent);
    }
    resp
}

// ---- Persisted settings & recent projects ---------------------------------

/// Tiny stable hash for recovery file names (FNV-1a, no deps).
fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn config_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(home).join("Library/Application Support/ScriptCraft")
}

fn default_backups_dir() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(home)
        .join("Documents/ScriptCraft/backups")
        .to_string_lossy()
        .into_owned()
}

fn load_settings() -> AppSettings {
    let mut s = AppSettings::default();
    if let Ok(text) = std::fs::read_to_string(config_dir().join("settings.conf")) {
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
                "spell_check" => s.spell_check = v == "1",
                "compact_mode" => s.compact_mode = v == "1",
                "two_panel" => s.two_panel = v == "1",
                "hidpi" => s.hidpi = v == "1",
                "dark_mode" => s.dark_mode = v == "1",
                _ => {}
            }
        }
    }
    s
}

fn save_settings(s: &AppSettings) {
    let _ = std::fs::create_dir_all(config_dir());
    let text = format!(
        "autosave={}\nautosave_minutes={}\nbackups={}\nbackups_dir={}\nspell_check={}\ncompact_mode={}\ntwo_panel={}\nhidpi={}\ndark_mode={}\n",
        s.autosave as u8,
        s.autosave_minutes.clamp(1, 60),
        s.backups as u8,
        s.backups_dir,
        s.spell_check as u8,
        s.compact_mode as u8,
        s.two_panel as u8,
        s.hidpi as u8,
        s.dark_mode as u8,
    );
    let _ = std::fs::write(config_dir().join("settings.conf"), text);
}

fn load_recent() -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Ok(text) = std::fs::read_to_string(config_dir().join("recent.txt")) {
        for line in text.lines() {
            let path = line.trim();
            if path.is_empty() {
                continue;
            }
            out.push((path.to_string(), recent_stamp(path)));
        }
    }
    out.truncate(20);
    out
}

fn save_recent(list: &[(String, String)]) {
    let _ = std::fs::create_dir_all(config_dir());
    let text: String = list.iter().map(|(p, _)| format!("{p}\n")).collect();
    let _ = std::fs::write(config_dir().join("recent.txt"), text);
}

/// "dd.mm.yyyy HH:MM" from the file's modification time.
fn recent_stamp(path: &str) -> String {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| {
            let dt: chrono::DateTime<chrono::Local> = t.into();
            dt.format("%d.%m.%Y %H:%M").to_string()
        })
        .unwrap_or_else(|_| "unknown".to_string())
}

/// One row of a statistics report table.
struct RepRow {
    label: String,
    cells: Vec<String>,
    color: egui::Color32,
    pct: f32,
}

fn pct_text(p: f32) -> String {
    if p > 0.0 && p < 1.0 {
        format!("{p:.1}%")
    } else {
        format!("{}%", p.round() as i64)
    }
}

/// Column headers with KIT's thin underline; the last column is the bar lane.
fn kit_report_cols(ui: &mut egui::Ui, cols: &[(&str, f32)]) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 22.0), egui::Sense::hover());
    let mut x = rect.left();
    for (name, w) in cols {
        let g = ui.painter().layout_no_wrap(
            name.to_string(),
            egui::FontId::proportional(12.5),
            kit().text,
        );
        let gw = g.size().x.min(*w - 6.0);
        ui.painter().galley(
            egui::pos2(x, rect.center().y - g.size().y / 2.0),
            g,
            kit().text,
        );
        ui.painter().line_segment(
            [
                egui::pos2(x, rect.bottom() - 2.0),
                egui::pos2(x + gw, rect.bottom() - 2.0),
            ],
            egui::Stroke::new(1.0_f32, egui::Color32::from_gray(0xDD)),
        );
        x += w;
    }
}

fn kit_report_row(
    ui: &mut egui::Ui,
    cols: &[(&str, f32)],
    texts: &[String],
    bar: Option<(egui::Color32, f32)>,
) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 21.0), egui::Sense::hover());
    let p = ui.painter();
    let font = egui::FontId::proportional(12.5);
    let mut x = rect.left();
    let last = cols.len().saturating_sub(1);
    for (i, (_, w)) in cols.iter().enumerate() {
        if i == last {
            if let Some((color, pct)) = bar {
                let capacity = (*w - 44.0).max(12.0);
                let bw = (capacity * pct / 100.0).max(2.5);
                let brect = egui::Rect::from_min_size(
                    egui::pos2(x, rect.center().y - 6.0),
                    egui::vec2(bw, 12.0),
                );
                p.rect_filled(brect, 0.0, color);
                p.text(
                    egui::pos2(brect.right() + 6.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    pct_text(pct),
                    font.clone(),
                    kit().text,
                );
            }
        } else if let Some(t) = texts.get(i) {
            if !t.is_empty() {
                p.text(
                    egui::pos2(x, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    t,
                    font.clone(),
                    kit().text,
                );
            }
        }
        x += w;
    }
}

/// Label column + fixed 80pt cell columns + a flexible bar lane at the end.
fn kit_report_table(
    ui: &mut egui::Ui,
    width: f32,
    label_name: &str,
    label_w: f32,
    cell_names: &[&str],
    rows: &[RepRow],
) {
    let used = label_w + 80.0 * cell_names.len() as f32;
    let bar_w = (width - used - 10.0).max(150.0);
    let mut cols: Vec<(&str, f32)> = Vec::with_capacity(cell_names.len() + 2);
    cols.push((label_name, label_w));
    for c in cell_names {
        cols.push((c, 80.0));
    }
    cols.push(("Percentages", bar_w));
    kit_report_cols(ui, &cols);
    for r in rows {
        let mut texts = vec![r.label.clone()];
        texts.extend(r.cells.iter().cloned());
        kit_report_row(ui, &cols, &texts, Some((r.color, r.pct)));
    }
}

/// Big number block + hairline rule under it.
fn kit_big_numbers(ui: &mut egui::Ui, items: &[(&str, &str)]) {
    let w = (ui.available_width() / items.len().max(1) as f32).max(110.0);
    ui.horizontal_top(|ui| {
        for (v, l) in items {
            ui.allocate_ui_with_layout(
                egui::vec2(w, 40.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.label(egui::RichText::new(*v).size(21.0).strong());
                    ui.label(egui::RichText::new(*l).size(11.5));
                },
            );
        }
    });
    ui.add_space(8.0);
    let (r, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(r, 0.0, egui::Color32::from_gray(0x99));
    ui.add_space(12.0);
}

fn kit_report_header(ui: &mut egui::Ui, project: &str, subtitle: &str, generated: &str) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(project).size(15.0).strong());
            ui.label(egui::RichText::new(subtitle).size(15.0).strong());
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
            ui.label(egui::RichText::new(generated).size(11.5));
        });
    });
    ui.add_space(14.0);
}

fn kit_section(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(15.0).strong());
    ui.add_space(6.0);
}

/// First printable action-ish line of a scene, for navigator rows.
fn scene_preview(p: &Project, s: &Scene) -> String {
    for &bi in &s.blocks {
        if let Some(b) = p.blocks.get(bi) {
            if matches!(
                b.element_type,
                ElementType::Action | ElementType::Shot | ElementType::SceneCharacters
            ) && !b.text.trim().is_empty()
            {
                let t: String = b.text.split_whitespace().collect::<Vec<_>>().join(" ");
                return if t.chars().count() > 64 {
                    format!("{}…", t.chars().take(64).collect::<String>())
                } else {
                    t
                };
            }
        }
    }
    String::new()
}

impl eframe::App for ScriptCraftApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Theme: keep the palette/Visuals in sync with the setting.
        set_kit_theme(!self.settings.dark_mode);
        if ctx.style().visuals.dark_mode != self.settings.dark_mode {
            apply_theme(ctx);
        }
        // Native macOS menu bar events (no-ops in tests).
        self.poll_native_menu();
        // Research-view title edits persist once no field has focus.
        if ctx.memory(|m| m.focused().is_none()) {
            self.flush_meta();
        }
        // Autosave: quiet periodic save of a real on-disk project.
        let autosave_due = self.settings.autosave
            && self.persist
            && self.project.is_some()
            && self.dirty
            && !self.path.is_empty()
            && !Self::is_fixture_path(&self.path)
            && self.last_autosave.elapsed().as_secs()
                >= self.settings.autosave_minutes.max(1) as u64 * 60;
        if autosave_due {
            self.do_save();
            if !self.dirty {
                self.status = format!("Autosaved {}.", self.path);
            }
        }
        // Crash recovery: snapshot dirty work immediately, then at most
        // every 10 seconds, so a crash or kill loses seconds at most.
        if self.persist
            && self.dirty
            && self.project.is_some()
            && self
                .last_recovery
                .is_none_or(|t| t.elapsed().as_secs() >= 10)
        {
            self.last_recovery = Some(std::time::Instant::now());
            self.write_recovery();
        }
        // Global shortcuts, consumed so focused editors don't double-handle.
        if self.project.is_some() {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
                self.do_save();
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
                self.show_find = !self.show_find;
            }
            if self.show_find && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.show_find = false;
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)) {
                self.do_undo();
            }
            let redo = ctx.input_mut(|i| {
                i.consume_key(
                    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
                    egui::Key::Z,
                ) || i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
            });
            if redo {
                self.do_redo();
            }
        }
        // KIT-style OS window title with dirty dot (the toolbar shows stats).
        let want_title = if self.title.is_empty() {
            "ScriptCraft".to_string()
        } else if self.dirty {
            format!("{} — ScriptCraft •", self.title)
        } else {
            format!("{} — ScriptCraft", self.title)
        };
        if want_title != self.last_window_title {
            self.last_window_title = want_title.clone();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(want_title));
        }
        self.toolbar(ctx);
        self.status_bar(ctx);
        if !self.recovery.is_empty() {
            self.recovery_bar(ctx);
        }
        if self.show_find {
            self.find_bar(ctx);
        }
        // Far-left rail always visible; navigator + format bar belong to
        // the Script view only — every other tab gets the full center.
        self.module_rail(ctx);
        let is_script = self.side_tab == SideTab::Script;
        if is_script && self.show_left_panel {
            self.left_panel(ctx);
        }
        if is_script && self.show_format_panel {
            self.format_panel(ctx);
        }
        if self.show_about {
            self.about_window(ctx);
        }
        if self.show_help {
            self.help_window(ctx);
        }
        if self.show_export {
            self.export_dialog(ctx);
        }
        match self.side_tab {
            SideTab::Start => self.start_view(ctx),
            SideTab::Script => self.editor_view(ctx),
            SideTab::Cards => self.cards_view(ctx),
            SideTab::Research => self.research_view(ctx),
            SideTab::Statistics => self.statistics_view(ctx),
            SideTab::Settings => self.settings_view(ctx),
        }
    }
}

impl ScriptCraftApp {
    /// Single full-width toolbar, KIT-style: Menu, scene add/remove,
    /// undo/redo (painted icons — no glyph fallbacks), element menu.
    /// Status + duration live in the bottom status bar.
    fn toolbar(&mut self, ctx: &egui::Context) {
        // macOS production window: the toolbar is painted behind the
        // transparent native titlebar, so it doubles as window chrome (the
        // traffic lights float top-left and the strip above the row drags
        // the window). Fullscreen hides the titlebar, so drop the inset.
        let custom =
            self.custom_titlebar && !ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
        egui::TopBottomPanel::top("toolbar")
            .frame(
                egui::Frame::new()
                    .fill(kit().window)
                    .inner_margin(egui::Margin {
                        left: 12,
                        right: 12,
                        top: if custom { 2 } else { 7 },
                        bottom: if custom { 4 } else { 7 },
                    }),
            )
            .show(ctx, |ui| {
                if custom {
                    let (_, strip) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 6.0),
                        egui::Sense::click_and_drag(),
                    );
                    if strip.drag_started() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    if strip.double_clicked() {
                        let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
                        ui.ctx()
                            .send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
                    }
                }
                ui.horizontal(|ui| {
                    if custom {
                        // Clear the native traffic lights (macOS owns them).
                        ui.add_space(62.0);
                    }
                    let menu_resp = menu_trigger(ui);
                    egui::Popup::menu(&menu_resp).show(|ui| {
                        ui.set_min_width(230.0);
                        self.menu_contents(ui);
                    });
                    toolbar_sep(ui);
                    let has = self.project.is_some();
                    let n_scenes = self.project.as_ref().map(|p| p.scenes().len()).unwrap_or(0);
                    ui.label(
                        egui::RichText::new(format!("Scenes: {n_scenes}"))
                            .small()
                            .weak(),
                    );
                    if toolbar_icon_button(ui, has, "Add scene", draw_plus).clicked() {
                        self.insert_scene_below();
                    }
                    if toolbar_icon_button(ui, has && n_scenes > 1, "Delete scene", draw_minus)
                        .clicked()
                    {
                        self.delete_scene();
                    }
                    toolbar_sep(ui);
                    let can_undo = has && self.editor2.history.can_undo();
                    let can_redo = has && self.editor2.history.can_redo();
                    if toolbar_icon_button(ui, can_undo, "Undo (⌘Z)", draw_undo_arrow).clicked() {
                        self.do_undo();
                    }
                    if toolbar_icon_button(ui, can_redo, "Redo (⇧⌘Z)", draw_redo_arrow).clicked()
                    {
                        self.do_redo();
                    }
                    toolbar_sep(ui);
                    let current = self.selected_block.and_then(|bi| {
                        self.project
                            .as_ref()
                            .and_then(|p| p.blocks.get(bi))
                            .map(|b| b.element_type)
                    });
                    let mut pick: Option<ElementType> = None;
                    ui.add_enabled_ui(has, |ui| {
                        let label = current.map(element_label).unwrap_or("Element");
                        ui.menu_button(label, |ui| {
                            ui.set_min_width(190.0);
                            for (_, t, name) in FORMAT_ORDER {
                                if ui.selectable_label(current == Some(t), name).clicked() {
                                    pick = Some(t);
                                }
                            }
                        });
                    });
                    if let Some(t) = pick {
                        if self.selected_block.is_some() {
                            self.set_selected_type(t);
                        } else {
                            self.insert_below(t);
                        }
                    }
                    // Page zoom cluster, always visible (pinch works too).
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let mut z = self.zoom;
                        let mut free = false;
                        if toolbar_icon_button(ui, z < 4.0 - 1e-3, "Zoom in (pinch out)", draw_plus)
                            .clicked()
                        {
                            z = (z * 1.1).min(4.0);
                            free = true;
                        }
                        let pct_zoom = match self.fit {
                            FitMode::Off => self.zoom,
                            _ => self.last_render_zoom,
                        };
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(format!(
                                        "{}%",
                                        (pct_zoom * 100.0).round() as i32
                                    ))
                                    .size(12.0),
                                )
                                .frame(false)
                                .min_size(egui::vec2(46.0, 24.0)),
                            )
                            .on_hover_text("Reset zoom to 100%")
                            .clicked()
                        {
                            z = 1.0;
                            free = true;
                        }
                        if toolbar_icon_button(
                            ui,
                            z > 0.25 + 1e-3,
                            "Zoom out (pinch in)",
                            draw_minus,
                        )
                        .clicked()
                        {
                            z = (z / 1.1).max(0.25);
                            free = true;
                        }
                        if free {
                            self.fit = FitMode::Off;
                        }
                        self.zoom = z.clamp(0.25, 4.0);
                        ui.label(egui::RichText::new("Zoom").small().weak());
                    });
                });
            });
    }

    /// Bottom status bar (KIT): plain status text left, duration right.
    fn status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("statusbar")
            .exact_height(28.0)
            .frame(
                egui::Frame::new()
                    .fill(kit().window)
                    .inner_margin(egui::Margin::symmetric(14, 5)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.project.is_some() {
                        let (dot, dot_resp) =
                            ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                        status_dot(
                            ui.painter(),
                            dot.center(),
                            if self.dirty { REP_SAND } else { REP_GREEN },
                        );
                        dot_resp.on_hover_text(if self.dirty {
                            "Unsaved changes"
                        } else {
                            "All changes saved"
                        });
                    }
                    ui.label(egui::RichText::new(&self.status).small().weak());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(p) = &self.project {
                            let total = p.duration_secs();
                            let cur = self
                                .selected_block
                                .map(|bi| p.duration_upto(bi))
                                .unwrap_or(0.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "Duration: {} | {}",
                                    format_duration(cur).trim_matches(['(', ')']),
                                    format_duration(total).trim_matches(['(', ')'])
                                ))
                                .small()
                                .weak(),
                            );
                        }
                    });
                });
            });
    }

    /// Contents of the Menu popup: File / Edit / View / Help sections.
    fn menu_contents(&mut self, ui: &mut egui::Ui) {
        enum M {
            New,
            Open,
            Save,
            SaveAs,
            Import,
            ImportFdx,
            Export,
            Preview,
            TwoPanel,
            Undo,
            Redo,
            Find,
            Nav,
            Fmt,
            Typew,
            ZoomIn,
            ZoomOut,
            ZoomReset,
            ZoomFitPage,
            ZoomFitWidth,
            About,
            Help,
        }
        let mut action: Option<M> = None;
        ui.set_min_width(250.0);
        ui.spacing_mut().item_spacing.y = 2.0;
        {
            let has = self.project.is_some();
            let can_undo = has && self.editor2.history.can_undo();
            let can_redo = has && self.editor2.history.can_redo();

            if menu_item(ui, "New", None, true) {
                action = Some(M::New);
            }
            if menu_item(ui, "Open…", Some("⌘O"), true) {
                action = Some(M::Open);
            }
            if menu_item(ui, "Save", Some("⌘S"), true) {
                action = Some(M::Save);
            }
            if menu_item(ui, "Save as…", Some("Shift+⌘S"), true) {
                action = Some(M::SaveAs);
            }
            ui.separator();
            if menu_item(ui, "Import Fountain…", None, true) {
                action = Some(M::Import);
            }
            if menu_item(ui, "Import Final Draft (.fdx)…", None, true) {
                action = Some(M::ImportFdx);
            }
            if menu_item(ui, "Export…", None, has) {
                action = Some(M::Export);
            }
            if menu_item(ui, "Print preview script", Some("F12"), has) {
                action = Some(M::Preview);
            }
            ui.separator();
            if menu_check(
                ui,
                "Two panel mode",
                self.settings.two_panel,
                Some("F2"),
                true,
            ) {
                action = Some(M::TwoPanel);
            }
            ui.separator();
            if menu_item(ui, "Undo", Some("⌘Z"), can_undo) {
                action = Some(M::Undo);
            }
            if menu_item(ui, "Redo", Some("Shift+⌘Z"), can_redo) {
                action = Some(M::Redo);
            }
            if menu_item(ui, "Find…", Some("⌘F"), has) {
                action = Some(M::Find);
            }
            ui.separator();
            if menu_check(ui, "Navigator", self.show_left_panel, None, true) {
                action = Some(M::Nav);
            }
            if menu_check(ui, "Format panel", self.show_format_panel, None, true) {
                action = Some(M::Fmt);
            }
            if menu_check(ui, "Typewriter scrolling", self.typewriter, None, true) {
                action = Some(M::Typew);
            }
            if menu_item(ui, "Zoom In", None, true) {
                action = Some(M::ZoomIn);
            }
            if menu_item(ui, "Zoom Out", None, true) {
                action = Some(M::ZoomOut);
            }
            if menu_item(ui, "Zoom 100%", None, true) {
                action = Some(M::ZoomReset);
            }
            if menu_item(ui, "Fit Page", None, true) {
                action = Some(M::ZoomFitPage);
            }
            if menu_item(ui, "Fit Width", None, true) {
                action = Some(M::ZoomFitWidth);
            }
            ui.separator();
            if menu_item(ui, "About ScriptCraft", None, true) {
                action = Some(M::About);
            }
            if menu_item(ui, "Help", None, true) {
                action = Some(M::Help);
            }
        }
        match action {
            None => {}
            Some(M::New) => self.do_new(),
            Some(M::Open) => {
                if let Some(f) = rfd::FileDialog::new()
                    .add_filter("KITScenarist", &["kitsp"])
                    .pick_file()
                {
                    self.open_path(&f.to_string_lossy());
                }
            }
            Some(M::Save) => self.do_save(),
            Some(M::SaveAs) => self.do_save_as(),
            Some(M::Import) => self.do_import_fountain(),
            Some(M::ImportFdx) => self.do_import_fdx(),
            Some(M::Export) => self.show_export = true,
            Some(M::Preview) => self.preview_pdf(),
            Some(M::TwoPanel) => {
                self.settings.two_panel = !self.settings.two_panel;
                if self.persist {
                    save_settings(&self.settings);
                }
            }
            Some(M::Undo) => self.do_undo(),
            Some(M::Redo) => self.do_redo(),
            Some(M::Find) => {
                if self.project.is_some() {
                    self.show_find = true;
                }
            }
            Some(M::Nav) => self.show_left_panel = !self.show_left_panel,
            Some(M::Fmt) => self.show_format_panel = !self.show_format_panel,
            Some(M::Typew) => self.typewriter = !self.typewriter,
            Some(M::ZoomIn) => {
                self.fit = FitMode::Off;
                self.zoom = (self.zoom * 1.1).min(4.0);
            }
            Some(M::ZoomOut) => {
                self.fit = FitMode::Off;
                self.zoom = (self.zoom / 1.1).max(0.25);
            }
            Some(M::ZoomReset) => {
                self.fit = FitMode::Off;
                self.zoom = 1.0;
            }
            Some(M::ZoomFitPage) => self.fit = FitMode::Page,
            Some(M::ZoomFitWidth) => self.fit = FitMode::Width,
            Some(M::About) => self.show_about = true,
            Some(M::Help) => self.show_help = true,
        }
    }

    /// Handle native macOS menu bar events and mirror toggle state back.
    fn poll_native_menu(&mut self) {
        use crate::menu;
        if self.menu.is_none() {
            return;
        }
        for id in menu::poll() {
            match id.as_str() {
                menu::ID_NEW => self.do_new(),
                menu::ID_OPEN => {
                    if let Some(f) = rfd::FileDialog::new()
                        .add_filter("KITScenarist", &["kitsp"])
                        .pick_file()
                    {
                        self.open_path(&f.to_string_lossy());
                    }
                }
                menu::ID_SAVE => self.do_save(),
                menu::ID_SAVE_AS => self.do_save_as(),
                menu::ID_IMPORT => self.do_import_fountain(),
                menu::ID_IMPORT_FDX => self.do_import_fdx(),
                menu::ID_EXPORT => self.show_export = true,
                menu::ID_PREVIEW => self.preview_pdf(),
                menu::ID_TWO_PANEL => {
                    self.settings.two_panel = !self.settings.two_panel;
                    if self.persist {
                        save_settings(&self.settings);
                    }
                }
                menu::ID_UNDO => self.do_undo(),
                menu::ID_REDO => self.do_redo(),
                menu::ID_FIND => {
                    if self.project.is_some() {
                        self.show_find = true;
                    }
                }
                menu::ID_NAVIGATOR => self.show_left_panel = !self.show_left_panel,
                menu::ID_FORMAT_PANEL => self.show_format_panel = !self.show_format_panel,
                menu::ID_TYPEWRITER => self.typewriter = !self.typewriter,
                menu::ID_ZOOM_IN => {
                    self.fit = FitMode::Off;
                    self.zoom = (self.zoom * 1.1).min(4.0);
                }
                menu::ID_ZOOM_OUT => {
                    self.fit = FitMode::Off;
                    self.zoom = (self.zoom / 1.1).max(0.25);
                }
                menu::ID_ZOOM_RESET => {
                    self.fit = FitMode::Off;
                    self.zoom = 1.0;
                }
                menu::ID_ZOOM_FIT_PAGE => self.fit = FitMode::Page,
                menu::ID_ZOOM_FIT_WIDTH => self.fit = FitMode::Width,
                menu::ID_ABOUT => self.show_about = true,
                menu::ID_HELP => self.show_help = true,
                _ => {}
            }
        }
        if let Some(m) = &self.menu {
            m.two_panel.set_checked(self.settings.two_panel);
            m.navigator.set_checked(self.show_left_panel);
            m.format_panel.set_checked(self.show_format_panel);
            m.typewriter.set_checked(self.typewriter);
        }
    }

    fn about_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("About ScriptCraft")
            .open(&mut self.show_about)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.heading("ScriptCraft 0.4.0");
                ui.label("A 100% Rust screenwriting studio (egui + eframe).");
                ui.separator();
                ui.label("Design language after KIT Scenarist 0.7.2:");
                ui.label("• Calm dark/light themes with the icon-blue accent #4294f8");
                ui.label("• Courier Prime script face (OFL), PT Sans UI");
                ui.label("• Russian-template metrics, calibrated chronometry");
                ui.label("• Fixed A4 pages — zoom scales pages, never reflows");
                ui.separator();
                ui.label("Opens .kitsp, exports Fountain. Start, Script, Research,");
                ui.label("Cards, Statistics and Settings modules.");
            });
    }

    fn help_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("Help")
            .open(&mut self.show_help)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.heading("ScriptCraft");
                ui.label("Menu ▸ New / Open / Save / Export (Fountain, PDF).");
                ui.label("Ctrl+S save · Ctrl+Z / Ctrl+Y undo · Ctrl+F find.");
                ui.label("Research holds Title Page, Logline, Synopsis and notes.");
                ui.label("Statistics reports counts, timings and speaking parts.");
                ui.separator();
                ui.label("Keep writing!");
            });
    }

    /// KIT-style Export dialog (per ExportDialog.ui): a 550×500 window
    /// titled "Export Scenario" with module tabs across the top, a
    /// "Choose file to export" row, format radios, the "Additional
    /// (Numbering, Title Page etc.)" panel and Export / Print Preview /
    /// Cancel at the bottom right.
    fn export_dialog(&mut self, ctx: &egui::Context) {
        if self.project.is_none() {
            self.show_export = false;
            return;
        }
        let mut win_open = true;
        let mut close = false;
        let mut do_preview = false;
        let mut do_export = false;
        let mut browse = false;
        egui::Window::new("Export Scenario")
            .open(&mut win_open)
            .collapsible(false)
            .resizable(false)
            .fixed_size(egui::vec2(550.0, 500.0))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                // Tab bar across the top (KIT: Export research / outline / script).
                ui.spacing_mut().item_spacing.x = 4.0;
                let tab_w = (ui.available_width() - 8.0) / 3.0;
                ui.horizontal(|ui| {
                    for (tab, name) in [
                        (ExportTab::Research, "Export research"),
                        (ExportTab::Outline, "Export outline"),
                        (ExportTab::Script, "Export script"),
                    ] {
                        if ui
                            .add_sized(
                                [tab_w, 26.0],
                                egui::Button::selectable(self.export_tab == tab, name),
                            )
                            .clicked()
                        {
                            self.export_tab = tab;
                        }
                    }
                });
                ui.separator();
                let script_tab = self.export_tab == ExportTab::Script;

                // "Choose file to export:" + path field + browse button.
                ui.label("Choose file to export:");
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.horizontal(|ui| {
                    let w = (ui.available_width() - 36.0).max(80.0);
                    ui.add_sized(
                        [w, 22.0],
                        egui::TextEdit::singleline(&mut self.export_path)
                            .hint_text("Choose a destination file…"),
                    );
                    if ui
                        .add_sized([30.0, 22.0], egui::Button::new("..."))
                        .clicked()
                    {
                        browse = true;
                    }
                });
                let file_exists = !self.export_path.is_empty()
                    && std::path::Path::new(&self.export_path).exists();
                if file_exists {
                    ui.label(
                        egui::RichText::new("File exists. It will be replaced after exporting.")
                            .small()
                            .weak(),
                    );
                }

                match self.export_tab {
                    ExportTab::Script => {
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label("Format:");
                            for fmt in
                                [ExportFormat::Pdf, ExportFormat::Fdx, ExportFormat::Fountain]
                            {
                                ui.radio_value(&mut self.export_format, fmt, fmt.label());
                            }
                        });
                        ui.add_space(2.0);
                        ui.checkbox(
                            &mut self.export_additional,
                            "Additional (Numbering, Title Page etc.)",
                        );
                        if self.export_additional {
                            egui::Frame::group(ui.style()).show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label("Template:");
                                    egui::ComboBox::from_id_salt("export_template")
                                        .selected_text("Final Draft Screenplay (A4)")
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(
                                                &mut (),
                                                (),
                                                "Final Draft Screenplay (A4)",
                                            );
                                        });
                                });
                                ui.checkbox(
                                    &mut self.export_options.title_page,
                                    "Print Title Page",
                                );
                                ui.checkbox(
                                    &mut self.export_options.page_numbers,
                                    "Print pages numbers",
                                );
                                ui.checkbox(
                                    &mut self.export_options.scene_numbers,
                                    "Print scenes numbers",
                                );
                                ui.add_enabled(
                                    false,
                                    egui::Checkbox::new(&mut false, "Print dialogues numbers"),
                                )
                                .on_disabled_hover_text("Not supported yet");
                                ui.add_enabled(
                                    false,
                                    egui::Checkbox::new(&mut false, "Save Review Marks"),
                                )
                                .on_disabled_hover_text("No review marks in this app");
                                ui.checkbox(&mut self.export_check_breaks, "Check Page Breaks");
                                ui.horizontal(|ui| {
                                    ui.checkbox(&mut self.export_watermark_on, "Print watermark:");
                                    ui.add_enabled(
                                        self.export_watermark_on,
                                        egui::TextEdit::singleline(&mut self.export_watermark)
                                            .desired_width(150.0),
                                    );
                                });
                            });
                        }
                    }
                    ExportTab::Research => {
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new("Research export is not implemented yet.")
                                .small()
                                .weak(),
                        );
                        egui::ScrollArea::vertical()
                            .max_height(280.0)
                            .show(ui, |ui| {
                                for e in &self.research {
                                    let kind = match e.kind {
                                        kitsp::ResearchKind::Character => "character",
                                        kitsp::ResearchKind::Location => "place",
                                        kitsp::ResearchKind::Other => "note",
                                    };
                                    ui.label(format!("{}  ({kind})", e.name));
                                }
                            });
                    }
                    ExportTab::Outline => {
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new("Outline export is not implemented yet.")
                                .small()
                                .weak(),
                        );
                    }
                }

                // Bottom-right buttons (KIT order: Export, Print Preview, Cancel).
                let gap = (ui.available_height() - 32.0).max(4.0);
                ui.add_space(gap);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                    if ui.button("Print Preview").clicked() {
                        do_preview = true;
                    }
                    let supported = script_tab
                        && !self.export_path.is_empty()
                        && matches!(
                            self.export_format,
                            ExportFormat::Pdf | ExportFormat::Fdx | ExportFormat::Fountain
                        );
                    let label = if file_exists { "Rewrite" } else { "Export" };
                    if ui
                        .add_enabled(supported, egui::Button::new(label))
                        .clicked()
                    {
                        do_export = true;
                    }
                });
            });
        self.show_export = win_open && !close && self.show_export;
        if browse {
            let mut dlg = rfd::FileDialog::new();
            let (name, ext, filter) = match self.export_format {
                ExportFormat::Pdf => (
                    if self.title.is_empty() {
                        "script.pdf".to_string()
                    } else {
                        format!("{}.pdf", self.title)
                    },
                    "pdf",
                    "PDF",
                ),
                ExportFormat::Fountain => (
                    if self.title.is_empty() {
                        "script.fountain".to_string()
                    } else {
                        format!("{}.fountain", self.title)
                    },
                    "fountain",
                    "Fountain",
                ),
                ExportFormat::Fdx => ("script.fdx".to_string(), "fdx", "FDX"),
            };
            dlg = dlg.add_filter(filter, &[ext]).set_file_name(&name);
            if let Some(dst) = dlg.save_file() {
                self.export_path = dst.to_string_lossy().into_owned();
            }
        }
        if do_preview {
            self.preview_pdf();
        } else if do_export {
            let dst = std::path::PathBuf::from(&self.export_path);
            match self.export_format {
                ExportFormat::Pdf => self.save_pdf(&dst),
                ExportFormat::Fdx => self.save_fdx(&dst),
                ExportFormat::Fountain => self.save_fountain(&dst),
            }
            self.show_export = false;
        }
    }

    /// All case-insensitive matches as (block, char_start, char_len).
    fn find_matches(&self) -> Vec<(usize, usize, usize)> {
        let q = self.find_query.to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        if let Some(p) = &self.project {
            for (bi, b) in p.blocks.iter().enumerate() {
                let lower = b.text.to_lowercase();
                let mut from = 0usize;
                // char-indexed search (byte indices would split Cyrillic).
                let chars: Vec<char> = lower.chars().collect();
                let qchars: Vec<char> = q.chars().collect();
                while from + qchars.len() <= chars.len() {
                    if chars[from..from + qchars.len()] == qchars[..] {
                        out.push((bi, from, qchars.len()));
                        from += qchars.len().max(1);
                    } else {
                        from += 1;
                    }
                }
            }
        }
        out
    }

    /// Jump to match `idx` (wrapping): select, scroll, highlight.
    fn find_goto(&mut self, _ctx: &egui::Context, idx: usize) {
        let matches = self.find_matches();
        if matches.is_empty() {
            return;
        }
        let idx = idx % matches.len();
        self.find_idx = idx;
        let (bi, start, len) = matches[idx];
        self.goto_block(bi);
        // Select the match inside its block (char offsets → Pos).
        if let Some(p) = &self.project {
            if let Some(b) = p.blocks.get(bi) {
                let anchor = edit::pos_at_char(&b.text, bi, start);
                let focus = edit::pos_at_char(&b.text, bi, start + len);
                self.editor2.selection = Some(crate::script_view::Selection { anchor, focus });
                self.editor2.autocomplete = None;
                self.editor2.scroll_caret = true;
            }
        }
    }

    fn find_replace_current(&mut self, ctx: &egui::Context) {
        if self.find_replace.is_empty() {
            return;
        }
        let matches = self.find_matches();
        if matches.is_empty() {
            return;
        }
        let idx = self.find_idx.min(matches.len().saturating_sub(1));
        let (bi, start, len) = matches[idx];
        let repl = self.find_replace.clone();
        let Some(text) = self
            .project
            .as_ref()
            .and_then(|p| p.blocks.get(bi))
            .map(|b| {
                let mut chars: Vec<char> = b.text.chars().collect();
                let end = (start + len).min(chars.len());
                chars.splice(start..end, repl.chars());
                chars.into_iter().collect::<String>()
            })
        else {
            return;
        };
        self.apply_editor2_output(crate::script_view::ScriptViewOutput {
            commands: vec![edit::EditCommand::SetText { bi, text }],
            ..Default::default()
        });
        self.find_goto(ctx, idx); // land on the next match
    }

    fn find_replace_all(&mut self, ctx: &egui::Context) {
        if self.find_query.is_empty() || self.find_replace.is_empty() {
            return;
        }
        let matches = self.find_matches();
        if matches.is_empty() {
            self.status = "Find: no matches.".to_string();
            return;
        }
        let Some(before) = self.project.as_ref().map(|p| p.blocks.clone()) else {
            return;
        };
        // Apply back-to-front per block so indices don't shift.
        let mut by_block: std::collections::BTreeMap<usize, Vec<(usize, usize)>> =
            std::collections::BTreeMap::new();
        for (bi, s, l) in matches {
            by_block.entry(bi).or_default().push((s, l));
        }
        let n = by_block.values().map(|v| v.len()).sum::<usize>();
        if let Some(p) = self.project.as_mut() {
            let repl = self.find_replace.clone();
            for (bi, mut locs) in by_block {
                if let Some(b) = p.blocks.get_mut(bi) {
                    let mut chars: Vec<char> = b.text.chars().collect();
                    locs.sort_by_key(|a| std::cmp::Reverse(a.0));
                    for (s, l) in locs {
                        let e = (s + l).min(chars.len());
                        chars.splice(s..e, repl.chars());
                    }
                    b.text = chars.into_iter().collect();
                }
            }
        }
        self.record_structural(before);
        self.status = format!("Replaced {n} occurrence(s).");
        let _ = ctx;
    }

    /// Slim Find / Replace bar under the toolbar (KIT has search here).
    fn find_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("findbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Find:");
                let resp =
                    ui.add(egui::TextEdit::singleline(&mut self.find_query).desired_width(180.0));
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    let dir = !ui.input(|i| i.modifiers.shift);
                    let matches = self.find_matches();
                    if !matches.is_empty() {
                        let next = if dir {
                            self.find_idx + 1
                        } else {
                            self.find_idx.saturating_sub(1)
                        };
                        self.find_goto(ctx, next);
                    }
                }
                ui.label("Replace:");
                ui.add(egui::TextEdit::singleline(&mut self.find_replace).desired_width(140.0));
                let n = self.find_matches().len();
                if ui.small_button("Prev").clicked() {
                    self.find_goto(ctx, self.find_idx.saturating_sub(1));
                }
                if ui.small_button("Next").clicked() {
                    self.find_goto(ctx, self.find_idx + 1);
                }
                if ui.small_button("Replace").clicked() {
                    self.find_replace_current(ctx);
                }
                if ui.small_button("All").clicked() {
                    self.find_replace_all(ctx);
                }
                if n > 0 {
                    ui.label(
                        egui::RichText::new(format!("{}/{}", (self.find_idx + 1).min(n), n))
                            .small()
                            .weak(),
                    );
                } else if !self.find_query.is_empty() {
                    ui.label(egui::RichText::new("no matches").small().weak());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("✕").clicked() {
                        self.show_find = false;
                    }
                });
            });
        });
    }

    /// Launch banner offering to restore unsaved work from a previous
    /// session (or a crash). One row per recovery copy.
    fn recovery_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("recoverybar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("Unsaved changes from a previous session:").strong());
                let mut restore = None;
                let mut discard = None;
                for (i, (_, label)) in self.recovery.iter().enumerate() {
                    ui.label(label);
                    if ui.small_button("Restore").clicked() {
                        restore = Some(i);
                    }
                    if ui.small_button("Discard").clicked() {
                        discard = Some(i);
                    }
                    ui.separator();
                }
                if let Some(i) = restore {
                    self.restore_recovery(i);
                }
                if let Some(i) = discard {
                    self.discard_recovery(i);
                }
            });
        });
    }

    /// KIT's signature far-left module rail: near-black strip, painted
    /// stroke icons + labels, active cell with a 3px blue bar.
    fn module_rail(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("rail")
            .exact_width(RAIL_WIDTH)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(kit().rail)
                    .inner_margin(egui::Margin::same(0)),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 2.0);
                ui.add_space(8.0);
                for (tab, name) in [
                    (SideTab::Start, "Start"),
                    (SideTab::Script, "Script"),
                    (SideTab::Research, "Research"),
                    (SideTab::Cards, "Cards"),
                    (SideTab::Statistics, "Statistics"),
                    (SideTab::Settings, "Settings"),
                ] {
                    let active = self.side_tab == tab;
                    let resp = rail_item(ui, name, active, |p, center, color| {
                        draw_rail_icon(p, tab, center, color)
                    });
                    if resp.clicked() {
                        self.side_tab = tab;
                    }
                }
            });
    }

    fn left_panel(&mut self, ctx: &egui::Context) {
        // The navigator belongs to the Script view only; every other rail
        // tab gets the full center stage (like KIT's modules).
        egui::SidePanel::left("side")
            .default_width(420.0)
            .resizable(true)
            .frame(
                egui::Frame::new()
                    .fill(kit().window)
                    .inner_margin(egui::Margin::symmetric(0, 10)),
            )
            .show(ctx, |ui| {
                self.scenes_tab(ui);
            });
    }

    /// Edge-to-edge two-line navigator rows: full-row blue selection,
    /// duration pinned right, 1px dividers — like the original.
    fn scenes_tab(&mut self, ui: &mut egui::Ui) {
        let Some(p) = &self.project else {
            ui.label("No project open.");
            return;
        };
        // Owned rows: (scene idx, heading block, heading, preview, duration).
        let rows: Vec<(usize, usize, String, String, f64)> = p
            .scenes()
            .into_iter()
            .enumerate()
            .map(|(i, s)| {
                (
                    i,
                    s.blocks.first().copied().unwrap_or(0),
                    s.heading.clone(),
                    scene_preview(p, &s),
                    p.scene_duration_secs(Some(&s)),
                )
            })
            .collect();
        let n_rows = rows.len();
        ui.horizontal(|ui| {
            ui.add_space(18.0);
            overline(ui, &format!("Scenes · {n_rows}"));
        });
        ui.add_space(4.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                for (i, bi, heading, preview, secs) in rows {
                    let selected = self.selected_scene == i;
                    let dur = format_duration(secs);
                    let resp = scene_row(
                        ui,
                        &self.scene_label(i),
                        &heading,
                        &preview,
                        dur.trim_matches(['(', ')']),
                        selected,
                    );
                    if resp.clicked() {
                        self.selected_scene = i;
                        self.goto_block(bi);
                    }
                }
            });
    }

    /// Corkboard grid: scene cards you can drag to reorder scenes.
    /// Drop a card onto another; scenes (and their cards on save) follow.
    fn cards_tab(&mut self, ui: &mut egui::Ui) {
        let Some(p) = &self.project else {
            ui.label("No project open.");
            return;
        };
        // Owned rows: (scene idx, heading block, title, duration, preview).
        let rows: Vec<(usize, usize, String, f64, String)> = p
            .scenes()
            .into_iter()
            .enumerate()
            .map(|(i, s)| {
                let title = self
                    .cards
                    .iter()
                    .find(|c| Some(c.id.as_str()) == s.kit_uuid.as_deref())
                    .map(|c| c.title.clone())
                    .unwrap_or_else(|| s.heading.clone());
                (
                    i,
                    s.blocks.first().copied().unwrap_or(0),
                    title,
                    p.scene_duration_secs(Some(&s)),
                    scene_preview(p, &s),
                )
            })
            .collect();
        let dragging = self.drag_scene;
        let mut drop_at: Option<(usize, bool)> = None;
        let mut clicked: Option<usize> = None;
        let mut dbl_clicked: Option<(usize, usize)> = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
                for (i, (si, bi, title, secs, preview)) in rows.iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(
                        egui::vec2(168.0, 112.0),
                        egui::Sense::click_and_drag(),
                    );
                    let is_sel = *si == self.selected_scene;
                    let bg = if is_sel {
                        kit().accent_soft
                    } else if resp.hovered() || dragging == Some(*si) {
                        kit().hover_light
                    } else {
                        kit().surface
                    };
                    ui.painter().rect_filled(rect, 10.0, bg);
                    ui.painter().rect_stroke(
                        rect,
                        10.0,
                        egui::Stroke::new(1.0_f32, if is_sel { kit().accent } else { kit().sep }),
                        egui::StrokeKind::Inside,
                    );
                    let inner = rect.shrink(8.0);
                    let mut card_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(inner)
                            .layout(egui::Layout::top_down(egui::Align::LEFT)),
                    );
                    card_ui.set_width(inner.width());
                    card_ui.label(
                        egui::RichText::new(format!("{}  {}", self.scene_label(i), title))
                            .small()
                            .strong()
                            .color(kit().heading_text),
                    );
                    card_ui.label(
                        egui::RichText::new(format_duration(*secs))
                            .small()
                            .color(kit().disabled_text),
                    );
                    if !preview.is_empty() {
                        let prev_show: String = if preview.chars().count() > 32 {
                            format!("{}…", preview.chars().take(31).collect::<String>())
                        } else {
                            preview.clone()
                        };
                        card_ui.label(
                            egui::RichText::new(prev_show)
                                .small()
                                .color(kit().disabled_text),
                        );
                    }
                    let _ = bi;
                    // Drop indicator: accent bar on the hovered edge.
                    if dragging.is_some() && dragging != Some(*si) {
                        if let Some(pos) = ui.ctx().input(|inp| inp.pointer.hover_pos()) {
                            if rect.contains(pos) {
                                let before = pos.x < rect.center().x;
                                drop_at = Some((*si, before));
                                let x = if before { rect.left() } else { rect.right() };
                                ui.painter().rect_filled(
                                    egui::Rect::from_min_size(
                                        egui::pos2(x - 1.5, rect.top()),
                                        egui::vec2(3.0, rect.height()),
                                    ),
                                    0.0,
                                    kit().accent,
                                );
                            }
                        }
                    }
                    if resp.drag_started() {
                        self.drag_scene = Some(*si);
                    }
                    if resp.clicked() {
                        clicked = Some(*si);
                    }
                    if resp.double_clicked() {
                        dbl_clicked = Some((*si, *bi));
                    }
                }
            });
        });
        if let Some(si) = clicked {
            self.selected_scene = si;
        }
        if let Some((si, bi)) = dbl_clicked {
            self.selected_scene = si;
            self.goto_block(bi);
            self.side_tab = SideTab::Script;
        }
        // Finish drag: pointer released over a target card.
        let released = ui.input(|i| i.pointer.any_released());
        if released {
            if let (Some(from), Some((to, before))) = (self.drag_scene.take(), drop_at) {
                if from != to {
                    self.reorder_scene(from, (to, before));
                }
            } else {
                self.drag_scene = None;
            }
            self.drop_before = None;
        } else {
            self.drop_before = drop_at;
        }
    }

    /// Move scene `from` before/after scene `to` (undoable; card order
    /// follows on save via `sync_scheme_cards`).
    fn reorder_scene(&mut self, from: usize, to_before: (usize, bool)) {
        let (to, before) = to_before;
        let Some(p) = self.project.as_ref() else {
            return;
        };
        let n = p.scenes().len();
        if from >= n || to >= n || from == to {
            return;
        }
        // Remember the selection by heading uuid so it survives the move.
        let sel_uuid = self.selected_block.and_then(|bi| {
            p.blocks
                .get(bi)
                .filter(|b| b.element_type == ElementType::SceneHeading)
                .and_then(|b| b.kit_uuid.clone())
        });
        let before_blocks = p.blocks.clone();
        if let Some(p) = self.project.as_mut() {
            let scenes = p.scenes();
            let mut order: Vec<usize> = (0..scenes.len()).collect();
            order.remove(from);
            let mut at = if before { to } else { to + 1 };
            if to > from {
                at = at.saturating_sub(1);
            }
            order.insert(at.min(order.len()), from);
            let mut moved = Vec::with_capacity(p.blocks.len());
            for si in order {
                for &bi in &scenes[si].blocks {
                    moved.push(p.blocks[bi].clone());
                }
            }
            p.blocks = moved;
        }
        // Restore selection on the moved heading.
        if let Some(uuid) = sel_uuid {
            if let Some(p) = &self.project {
                for (si, s) in p.scenes().iter().enumerate() {
                    if let Some(&bi) = s.blocks.first() {
                        if p.blocks.get(bi).and_then(|b| b.kit_uuid.clone()) == Some(uuid.clone()) {
                            self.selected_scene = si;
                            self.selected_block = Some(bi);
                            break;
                        }
                    }
                }
            }
        }
        self.record_structural(before_blocks);
        if let (Some(sel), Some(p)) = (self.editor2.selection, &self.project) {
            let anchor = edit::clamp_pos(&p.blocks, sel.anchor);
            let focus = edit::clamp_pos(&p.blocks, sel.focus);
            self.editor2.selection = match (anchor, focus) {
                (Some(anchor), Some(focus)) => {
                    Some(crate::script_view::Selection { anchor, focus })
                }
                _ => None,
            };
        }
        self.editor2.autocomplete = None;
        self.status = "Scenes reordered — save to persist.".to_string();
    }

    fn research_view(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.project.is_none() {
                ui.centered_and_justified(|ui| {
                    ui.label("Open a .kitsp file to browse research.");
                });
                return;
            };
            ui.spacing_mut().item_spacing.y = 0.0;
            kit_module_strip(ui, "Research", &[]);
            ui.spacing_mut().item_spacing.y = 6.0;
            let pane_h = ui.available_height();
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(330.0, pane_h),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_min_width(330.0);
                        ui.set_max_width(330.0);
                        egui::ScrollArea::vertical()
                            .id_salt("research_tree")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                self.research_tree(ui);
                            });
                    },
                );
                ui.add_space(8.0);
                let (sep, _) =
                    ui.allocate_exact_size(egui::vec2(1.0, pane_h), egui::Sense::hover());
                ui.painter().rect_filled(sep, 0.0, kit().sep);
                ui.add_space(4.0);
                egui::Frame::new()
                    .fill(kit().module_bg)
                    .inner_margin(egui::Margin::symmetric(14, 12))
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            egui::ScrollArea::vertical()
                                .id_salt("research_pane")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    self.research_pane_view(ui);
                                });
                        });
                    });
            });
        });
    }

    fn scene_count(&self) -> usize {
        self.project.as_ref().map(|p| p.scenes().len()).unwrap_or(0)
    }

    fn research_tree(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        let mut sel = self.research_pane;

        // Script branch.
        let mut open = ui.data(|d| {
            d.get_temp::<bool>(egui::Id::new("rt_script"))
                .unwrap_or(true)
        });
        let (row, arrow, _) = kit_tree_row(
            ui,
            KitTreeRow {
                sel: false,
                arrow: Some(open),
                plus: false,
            },
            KitIcon::Script,
            "Script",
            "rt_script",
        );
        if row.clicked() || arrow {
            open = !open;
            ui.data_mut(|d| d.insert_temp(egui::Id::new("rt_script"), open));
        }
        if open {
            ui.indent("rt_script_i", |ui| {
                for (pane, icon, name) in [
                    (ResearchPane::TitlePage, KitIcon::Book, "Title Page"),
                    (ResearchPane::Logline, KitIcon::Text, "Logline"),
                    (ResearchPane::Synopsis, KitIcon::Paragraph, "Synopsis"),
                    (ResearchPane::Versions, KitIcon::Layers, "Versions"),
                ] {
                    let (r, _, _) = kit_tree_row(
                        ui,
                        KitTreeRow {
                            sel: sel == pane,
                            arrow: None,
                            plus: false,
                        },
                        icon,
                        name,
                        name,
                    );
                    if r.clicked() {
                        sel = pane;
                    }
                }
            });
        }

        self.research_folder(
            ui,
            "rt_chars",
            KitIcon::Person,
            "Characters",
            kitsp::ResearchKind::Character,
            &mut sel,
        );
        self.research_folder(
            ui,
            "rt_locs",
            KitIcon::House,
            "Locations",
            kitsp::ResearchKind::Location,
            &mut sel,
        );
        self.research_folder(
            ui,
            "rt_docs",
            KitIcon::Folder,
            "Documents",
            kitsp::ResearchKind::Other,
            &mut sel,
        );

        ui.add_space(8.0);
        self.research_pane = sel;
    }

    fn research_folder(
        &mut self,
        ui: &mut egui::Ui,
        salt: &'static str,
        icon: KitIcon,
        title: &str,
        kind: kitsp::ResearchKind,
        sel: &mut ResearchPane,
    ) {
        let mut open = ui.data(|d| d.get_temp::<bool>(egui::Id::new(salt)).unwrap_or(true));
        let (row, arrow, plus) = kit_tree_row(
            ui,
            KitTreeRow {
                sel: false,
                arrow: Some(open),
                plus: true,
            },
            icon,
            title,
            salt,
        );
        if row.clicked() || arrow {
            open = !open;
            ui.data_mut(|d| d.insert_temp(egui::Id::new(salt), open));
        }
        if plus {
            egui::Popup::menu(&row).show(|ui| {
                ui.set_min_width(190.0);
                match kind {
                    kitsp::ResearchKind::Character => {
                        if ui.button("Add character").clicked() {
                            self.add_research_entry(
                                kitsp::ResearchKind::Character,
                                "New character",
                            );
                            ui.close();
                        }
                        if ui.button("Import characters from script").clicked() {
                            self.import_characters_from_script();
                            ui.close();
                        }
                    }
                    kitsp::ResearchKind::Location => {
                        if ui.button("Add location").clicked() {
                            self.add_research_entry(kitsp::ResearchKind::Location, "New location");
                            ui.close();
                        }
                        if ui.button("Import locations from script").clicked() {
                            self.import_locations_from_script();
                            ui.close();
                        }
                    }
                    kitsp::ResearchKind::Other => {
                        if ui.button("Add note").clicked() {
                            self.add_research_entry(kitsp::ResearchKind::Other, "New entry");
                            ui.close();
                        }
                    }
                }
            });
        }
        if open {
            let ids: Vec<(i64, String)> = self
                .research
                .iter()
                .filter(|e| e.kind == kind)
                .map(|e| (e.id, e.name.clone()))
                .collect();
            ui.indent(salt, |ui| {
                for (id, name) in &ids {
                    let (c, _, _) = kit_tree_row(
                        ui,
                        KitTreeRow {
                            sel: *sel == ResearchPane::Entry(*id),
                            arrow: None,
                            plus: false,
                        },
                        icon,
                        name,
                        (salt, *id),
                    );
                    if c.clicked() {
                        *sel = ResearchPane::Entry(*id);
                    }
                }
            });
        }
    }

    fn research_pane_view(&mut self, ui: &mut egui::Ui) {
        match self.research_pane {
            ResearchPane::Script => self.research_script_pane(ui),
            ResearchPane::TitlePage => self.research_titlepage_pane(ui),
            ResearchPane::Logline => self.research_logline_pane(ui, false),
            ResearchPane::Synopsis => self.research_logline_pane(ui, true),
            ResearchPane::Versions => self.research_versions_pane(ui),
            ResearchPane::Entry(id) => self.research_entry_pane(ui, id),
            ResearchPane::Characters | ResearchPane::Locations | ResearchPane::Documents => {
                ui.label("Select an entry to edit it.");
            }
        }
    }

    fn research_script_pane(&mut self, ui: &mut egui::Ui) {
        let mut dirty = false;
        kit_section(ui, "Script");
        dirty |= kit_field(ui, "Script Name", &mut self.meta.title);
        dirty |= kit_field(ui, "Header", &mut self.meta.header);
        dirty |= kit_field(ui, "Footer", &mut self.meta.footer);
        dirty |= kit_field(
            ui,
            "Scene Number Prefix",
            &mut self.meta.scene_number_prefix,
        );
        dirty |= kit_field(
            ui,
            "Scene Numbering Starting at",
            &mut self.meta.scene_number_start,
        );
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if kit_icon_button(ui, KitIcon::LockClosed, "Lock scenes numbers").clicked() {
                let n = self.scene_count();
                if n > 0 {
                    let labels: Vec<String> = (0..n)
                        .map(|i| self.scene_label(i).trim_end_matches('.').to_string())
                        .collect();
                    self.meta.locked_scene_numbers = labels.join(",");
                    dirty = true;
                }
            }
            if kit_icon_button(ui, KitIcon::LockClosed, "Lock new scenes numbers").clicked() {
                let n = self.scene_count();
                let labels: Vec<String> = (0..n)
                    .map(|i| self.scene_label(i).trim_end_matches('.').to_string())
                    .collect();
                self.meta.locked_scene_numbers = format!("*{}", labels.join(","));
                dirty = true;
            }
            if kit_icon_button(ui, KitIcon::LockOpen, "Unlock scenes numbers").clicked() {
                self.meta.locked_scene_numbers.clear();
                dirty = true;
            }
        });
        if dirty {
            self.meta_dirty = true;
        }
    }

    fn research_titlepage_pane(&mut self, ui: &mut egui::Ui) {
        let mut dirty = false;
        kit_section(ui, "Title Page");
        dirty |= kit_field(ui, "Script Name", &mut self.meta.title);
        dirty |= kit_field(ui, "Genre", &mut self.meta.genre);
        dirty |= kit_field(ui, "Author", &mut self.meta.author);
        dirty |= kit_field(ui, "Additional", &mut self.meta.additional_info);
        dirty |= kit_field_multi(ui, "Contacts", &mut self.meta.contacts, 3);
        dirty |= kit_field(ui, "Year", &mut self.meta.year);
        if dirty {
            self.meta_dirty = true;
        }
    }

    fn research_logline_pane(&mut self, ui: &mut egui::Ui, synopsis: bool) {
        let title = if synopsis { "Synopsis" } else { "Logline" };
        let words = if synopsis {
            self.title_synopsis.split_whitespace().count()
        } else {
            self.title_logline.split_whitespace().count()
        };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(title).size(15.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(format!("words: {words}")).size(12.0));
            });
        });
        ui.add_space(8.0);
        let available = ui.available_height() - 8.0;
        let text = if synopsis {
            &mut self.title_synopsis
        } else {
            &mut self.title_logline
        };
        if kit_mono_editor(ui, text, available.max(120.0)) {
            self.meta_dirty = true;
        }
    }

    fn research_versions_pane(&mut self, ui: &mut egui::Ui) {
        kit_section(ui, "Versions");
        if self.versions.is_empty() {
            ui.label(egui::RichText::new("No versions in this project.").size(13.0));
            return;
        }
        for (name, when) in &self.versions {
            let (rect, _) = ui
                .allocate_exact_size(egui::vec2(ui.available_width(), 40.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 2.0, kit().strip);
            draw_kit_icon(
                ui.painter(),
                KitIcon::Layers,
                egui::pos2(rect.left() + 18.0, rect.center().y),
                kit().text,
            );
            ui.painter().text(
                egui::pos2(rect.left() + 36.0, rect.center().y - 7.0),
                egui::Align2::LEFT_CENTER,
                name,
                egui::FontId::proportional(13.0),
                kit().text,
            );
            ui.painter().text(
                egui::pos2(rect.left() + 36.0, rect.center().y + 8.0),
                egui::Align2::LEFT_CENTER,
                when,
                egui::FontId::proportional(11.5),
                egui::Color32::from_gray(0xA1),
            );
            ui.add_space(6.0);
        }
    }

    fn research_entry_pane(&mut self, ui: &mut egui::Ui, id: i64) {
        if self.research_sel != Some(id) {
            self.research_sel = Some(id);
            self.sync_research_buffer();
        }
        if self.research_edit.is_none() {
            self.sync_research_buffer();
        }
        // Refresh the picture texture when the stored bytes change.
        let photo = self
            .research
            .iter()
            .find(|e| e.id == id)
            .and_then(|e| e.image.clone());
        let want = photo.as_ref().map(|b| (id, image_hash(b)));
        if self.research_tex.as_ref().map(|(i, h, _)| (*i, *h)) != want {
            self.research_tex = None;
            if let Some(bytes) = &photo {
                if let Some(img) = decode_image(bytes) {
                    let tex = ui.ctx().load_texture(
                        format!("research-photo-{id}"),
                        img,
                        egui::TextureOptions::LINEAR,
                    );
                    self.research_tex = Some((id, image_hash(bytes), tex));
                }
            }
        }

        let writable = !Self::is_fixture_path(&self.path) && !self.path.is_empty();
        let mut save = false;
        let mut del = false;
        let mut pick_photo = false;
        let mut remove_photo = false;
        if let Some(edit) = self.research_edit.as_mut() {
            let kind_label = match edit.kind {
                kitsp::ResearchKind::Character => "Character",
                kitsp::ResearchKind::Location => "Location",
                kitsp::ResearchKind::Other => "Entry",
            };
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(edit.name.clone()).size(17.0));
                    ui.label(egui::RichText::new(kind_label).size(12.0));
                });
                ui.with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(220.0, 150.0), egui::Sense::hover());
                    let p = ui.painter();
                    p.rect_filled(rect, 2.0, kit().base);
                    p.rect_stroke(
                        rect,
                        2.0,
                        egui::Stroke::new(1.0_f32, kit().field_stroke),
                        egui::StrokeKind::Inside,
                    );
                    if let Some((_, _, tex)) = &self.research_tex {
                        let size = tex.size_vec2();
                        let scale = (rect.width() / size.x).min(rect.height() / size.y).min(2.0);
                        let draw = egui::Rect::from_center_size(rect.center(), size * scale);
                        p.image(
                            tex.id(),
                            draw,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                    } else {
                        p.text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "No picture",
                            egui::FontId::proportional(12.0),
                            kit().disabled_text,
                        );
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        pick_photo = kit_button(ui, "Picture…").clicked();
                        remove_photo = ui
                            .add_enabled(
                                self.research_tex.is_some() && writable,
                                egui::Button::new("Remove").min_size(egui::vec2(0.0, 22.0)),
                            )
                            .clicked();
                    });
                });
            });
            ui.add_space(8.0);
            kit_field(ui, "Name", &mut edit.name);
            kit_field(ui, "URL", &mut edit.url);
            kit_field_multi(ui, "Description", &mut edit.description, 8);
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                save = kit_button(ui, "Save").clicked();
                del = kit_button(ui, "Delete").clicked();
            });
        } else {
            ui.label("Select an entry to edit it.");
        }
        if save {
            self.save_research_edit();
        }
        if del {
            self.delete_selected_research();
        }
        if pick_photo {
            if let Some(f) = rfd::FileDialog::new()
                .add_filter("Images", &["png", "jpg", "jpeg"])
                .pick_file()
            {
                match std::fs::read(&f) {
                    Ok(bytes) if decode_image(&bytes).is_some() => {
                        self.set_research_photo(id, Some(&bytes));
                    }
                    Ok(_) => self.status = "Unsupported image file.".to_string(),
                    Err(e) => self.status = format!("Can't read picture: {e}"),
                }
            }
        }
        if remove_photo {
            self.set_research_photo(id, None);
        }
    }

    /// Store (or clear) an entry's picture; fixture copies are read-only.
    fn set_research_photo(&mut self, id: i64, bytes: Option<&[u8]>) {
        if Self::is_fixture_path(&self.path) || self.path.is_empty() {
            self.status = "Save the script first (Save As) — fixture is read-only.".to_string();
            return;
        }
        match kitsp::update_research_image(&self.path, id, bytes) {
            Ok(()) => {
                self.research = kitsp::load_research(&self.path).unwrap_or_default();
                self.research_tex = None;
                self.status = if bytes.is_some() {
                    "Research picture saved.".to_string()
                } else {
                    "Research picture removed.".to_string()
                };
            }
            Err(e) => self.status = format!("Picture save failed: {e}"),
        }
    }

    /// Test hook: open a research entry (by name) with an in-memory picture.
    pub fn research_photo_preview_for_test(&mut self, name: &str, bytes: &[u8]) -> bool {
        let Some(e) = self.research.iter_mut().find(|e| e.name == name) else {
            return false;
        };
        let id = e.id;
        e.image = Some(bytes.to_vec());
        self.research_sel = Some(id);
        self.research_pane = ResearchPane::Entry(id);
        self.research_edit = None;
        self.sync_research_buffer();
        self.research_tex = None;
        true
    }

    /// KIT numbered element list (0–9), active row inverted like the original.
    /// KIT fast-format list: flat left-aligned boxes, active row inverted
    /// light (#DCDDDD + dark ink). Autocomplete lives in the floating popup
    /// at the caret now, so this panel stays clean.
    fn format_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("format")
            .exact_width(172.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(kit().window)
                    .inner_margin(egui::Margin::symmetric(10, 12)),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                if self.project.is_none() {
                    ui.label("No project open.");
                    return;
                }
                let current = self.selected_block.and_then(|bi| {
                    self.project
                        .as_ref()
                        .and_then(|p| p.blocks.get(bi))
                        .map(|b| b.element_type)
                });
                if fmt_button(ui, "Prev", false) {
                    self.step_selection(-1);
                }
                ui.add_space(6.0);
                let mut pick: Option<ElementType> = None;
                for (n, t, name) in FORMAT_ORDER {
                    if fmt_button(ui, &format!("{n} {name}"), current == Some(t)) {
                        pick = Some(t);
                    }
                }
                if let Some(t) = pick {
                    if self.selected_block.is_some() {
                        self.set_selected_type(t);
                    } else {
                        self.insert_below(t);
                    }
                }
                ui.add_space(6.0);
                if fmt_button(ui, "Next", false) {
                    self.step_selection(1);
                }
            });
    }

    fn cards_view(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            module_header(
                ui,
                "Corkboard",
                Some("drag a card onto another to reorder scenes — double-click opens"),
            );
            self.cards_tab(ui);
        });
    }

    fn statistics_view(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let items = [
                StripItem::Icon(KitIcon::Gear, "Report options"),
                StripItem::Sep,
                StripItem::Icon(KitIcon::Refresh, "Refresh report"),
                StripItem::Icon(KitIcon::Floppy, "Save report…"),
                StripItem::Icon(KitIcon::Printer, "Copy report"),
            ];
            let r = kit_module_strip(ui, "Statistics", &items);
            egui::Popup::menu(&r[0]).show(|ui| {
                ui.checkbox(
                    &mut self.stats_notes,
                    "Include non-printable blocks in totals",
                );
            });
            if r[1].clicked() {
                self.stats_gen = Self::now_stamp();
            }
            if r[2].clicked() {
                self.save_report_dialog();
            }
            if r[3].clicked() {
                let text = self.report_text();
                if text.is_empty() {
                    self.status = "Nothing to copy — open a script first.".to_string();
                } else {
                    ctx.copy_text(text);
                    self.status = "Report copied to clipboard.".to_string();
                }
            }
            ui.spacing_mut().item_spacing.y = 6.0;
            if self.project.is_none() {
                ui.centered_and_justified(|ui| {
                    ui.label("Open a .kitsp file to see statistics.");
                });
                return;
            };
            let pane_h = ui.available_height();
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(280.0, pane_h),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_min_width(280.0);
                        ui.set_max_width(280.0);
                        egui::ScrollArea::vertical()
                            .id_salt("stats_tree")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                self.stats_tree(ui);
                            });
                    },
                );
                ui.add_space(8.0);
                let (sep, _) =
                    ui.allocate_exact_size(egui::vec2(1.0, pane_h), egui::Sense::hover());
                ui.painter().rect_filled(sep, 0.0, kit().sep);
                ui.add_space(4.0);
                egui::Frame::new()
                    .fill(kit().module_bg)
                    .inner_margin(egui::Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            egui::ScrollArea::vertical()
                                .id_salt("stats_body")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    self.stats_body(ui);
                                });
                        });
                    });
            });
        });
    }

    fn stats_tree(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        let mut sel = self.stats_pane;
        let mut open = ui.data(|d| {
            d.get_temp::<bool>(egui::Id::new("st_reports"))
                .unwrap_or(true)
        });
        let (row, arrow, _) = kit_tree_row(
            ui,
            KitTreeRow {
                sel: false,
                arrow: Some(open),
                plus: false,
            },
            KitIcon::Chart,
            "Reports",
            "st_reports",
        );
        if row.clicked() || arrow {
            open = !open;
            ui.data_mut(|d| d.insert_temp(egui::Id::new("st_reports"), open));
        }
        if open {
            ui.indent("st_reports_i", |ui| {
                for (pane, name) in [
                    (StatsPane::Summary, "Summary Statistics"),
                    (StatsPane::SceneReport, "Scene Report"),
                    (StatsPane::LocationReport, "Location Report"),
                    (StatsPane::CastReport, "Cast Report"),
                    (StatsPane::SpeakingParts, "Characters Speaking Parts"),
                ] {
                    let (r, _, _) = kit_tree_row(
                        ui,
                        KitTreeRow {
                            sel: sel == pane,
                            arrow: None,
                            plus: false,
                        },
                        KitIcon::File,
                        name,
                        name,
                    );
                    if r.clicked() {
                        sel = pane;
                    }
                }
            });
        }
        let mut open2 = ui.data(|d| {
            d.get_temp::<bool>(egui::Id::new("st_plots"))
                .unwrap_or(true)
        });
        let (row, arrow, _) = kit_tree_row(
            ui,
            KitTreeRow {
                sel: false,
                arrow: Some(open2),
                plus: false,
            },
            KitIcon::Plot,
            "Plots",
            "st_plots",
        );
        if row.clicked() || arrow {
            open2 = !open2;
            ui.data_mut(|d| d.insert_temp(egui::Id::new("st_plots"), open2));
        }
        if open2 {
            ui.indent("st_plots_i", |ui| {
                for (pane, name) in [
                    (StatsPane::Structure, "Story Structure Analysis"),
                    (StatsPane::Activity, "Characters Activity"),
                ] {
                    let (r, _, _) = kit_tree_row(
                        ui,
                        KitTreeRow {
                            sel: sel == pane,
                            arrow: None,
                            plus: false,
                        },
                        KitIcon::Folder,
                        name,
                        name,
                    );
                    if r.clicked() {
                        sel = pane;
                    }
                }
            });
        }
        ui.add_space(8.0);
        self.stats_pane = sel;
    }

    fn stats_pane_name(&self) -> &'static str {
        match self.stats_pane {
            StatsPane::Summary => "Summary Report",
            StatsPane::SceneReport => "Scene Report",
            StatsPane::LocationReport => "Location Report",
            StatsPane::CastReport => "Cast Report",
            StatsPane::SpeakingParts => "Characters Speaking Parts",
            StatsPane::Structure => "Story Structure Analysis",
            StatsPane::Activity => "Characters Activity",
        }
    }

    fn stats_body(&mut self, ui: &mut egui::Ui) {
        if self.stats_gen.is_empty() {
            self.stats_gen = Self::now_stamp();
        }
        let Some(p) = &self.project else {
            return;
        };
        let title = self.meta.title.clone();
        let gen = self.stats_gen.clone();
        let name = self.stats_pane_name();
        let width = ui.available_width();
        kit_report_header(ui, &title, name, &gen);
        match self.stats_pane {
            StatsPane::Summary => {
                let (text_rows, total_words, total_chars) = stat_text_rows(p, self.stats_notes);
                let dur = format_duration(p.duration_secs())
                    .trim_matches(['(', ')'])
                    .to_string();
                let pages_s = self.page_count().to_string();
                let words_s = total_words.to_string();
                let chars_s = total_chars.to_string();
                kit_big_numbers(
                    ui,
                    &[
                        (dur.as_str(), "Duration"),
                        (pages_s.as_str(), "Pages"),
                        (words_s.as_str(), "Words"),
                        (chars_s.as_str(), "Characters"),
                    ],
                );
                kit_section(ui, "Text");
                kit_report_table(
                    ui,
                    width,
                    "Element",
                    220.0,
                    &["Words", "Characters"],
                    &text_rows,
                );
                ui.add_space(14.0);
                let labels: Vec<String> = p
                    .scenes()
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        format!("{} {}", self.scene_label(i), truncate_text(&s.heading, 44))
                    })
                    .collect();
                kit_section(ui, "Scenes");
                kit_report_table(
                    ui,
                    width,
                    "Scene",
                    330.0,
                    &["Time", "Words"],
                    &stat_scene_rows(p, &labels),
                );
                ui.add_space(14.0);
                kit_section(ui, "Locations");
                kit_report_table(
                    ui,
                    width,
                    "Location",
                    260.0,
                    &["Time", "Scenes"],
                    &stat_location_rows(p),
                );
                ui.add_space(14.0);
                kit_section(ui, "Characters");
                kit_report_table(
                    ui,
                    width,
                    "Character",
                    220.0,
                    &["Time", "Words", "Characters"],
                    &stat_character_rows(p),
                );
            }
            StatsPane::SceneReport => {
                let labels: Vec<String> = p
                    .scenes()
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        format!("{} {}", self.scene_label(i), truncate_text(&s.heading, 44))
                    })
                    .collect();
                kit_report_table(
                    ui,
                    width,
                    "Scene",
                    330.0,
                    &["Time", "Words"],
                    &stat_scene_rows(p, &labels),
                );
            }
            StatsPane::LocationReport => {
                kit_report_table(
                    ui,
                    width,
                    "Location",
                    260.0,
                    &["Time", "Scenes"],
                    &stat_location_rows(p),
                );
            }
            StatsPane::CastReport => {
                kit_report_table(
                    ui,
                    width,
                    "Character",
                    220.0,
                    &["Time", "Words", "Characters"],
                    &stat_character_rows(p),
                );
            }
            StatsPane::SpeakingParts => {
                kit_report_table(
                    ui,
                    width,
                    "Speaking Parts",
                    320.0,
                    &["Characters"],
                    &self.stat_speaking_rows(p),
                );
            }
            StatsPane::Structure => {
                let scenes = p.scenes();
                if scenes.is_empty() {
                    ui.label("No scenes yet.");
                } else {
                    let total = p.duration_secs();
                    let average = total / scenes.len() as f64;
                    let longest = scenes
                        .iter()
                        .map(|s| (s.heading.clone(), p.scene_duration_secs(Some(s))))
                        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                        .unwrap();
                    let shortest = scenes
                        .iter()
                        .map(|s| (s.heading.clone(), p.scene_duration_secs(Some(s))))
                        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                        .unwrap();
                    kit_big_numbers(
                        ui,
                        &[
                            (
                                format_duration(total)
                                    .trim_matches(['(', ')'])
                                    .to_string()
                                    .as_str(),
                                "Total Duration",
                            ),
                            (scenes.len().to_string().as_str(), "Scenes"),
                            (
                                format_duration(average)
                                    .trim_matches(['(', ')'])
                                    .to_string()
                                    .as_str(),
                                "Average Scene",
                            ),
                        ],
                    );
                    kit_section(ui, "Highlights");
                    ui.label(format!("Longest scene: {}", truncate_text(&longest.0, 48)));
                    ui.label(format!(
                        "Longest scene duration: {}",
                        format_duration(longest.1).trim_matches(['(', ')'])
                    ));
                    ui.add_space(6.0);
                    ui.label(format!(
                        "Shortest scene: {}",
                        truncate_text(&shortest.0, 48)
                    ));
                    ui.label(format!(
                        "Shortest scene duration: {}",
                        format_duration(shortest.1).trim_matches(['(', ')'])
                    ));
                }
            }
            StatsPane::Activity => {
                let chars = stat_char_stats(p);
                if chars.is_empty() {
                    ui.label("No dialogue yet.");
                } else {
                    kit_section(ui, "Dialogue Activity");
                    for (cname, parts, words, _) in chars.iter() {
                        ui.label(format!("{cname}: {parts} dialogue blocks · {words} words"));
                    }
                }
            }
        }
    }

    fn stat_speaking_rows(&self, p: &Project) -> Vec<RepRow> {
        let chars = stat_char_stats(p);
        let mut names: Vec<String> = p.characters().iter().map(|c| cue_name(c)).collect();
        for e in &self.research {
            if e.kind == kitsp::ResearchKind::Character {
                let n = cue_name(&e.name);
                if !n.is_empty() && !names.contains(&n) {
                    names.push(n);
                }
            }
        }
        names.sort();
        names.dedup();
        let parts_of = |n: &str| -> usize {
            chars
                .iter()
                .find(|(name, _, _, _)| name == n)
                .map(|(_, parts, _, _)| *parts)
                .unwrap_or(0)
        };
        let mut more = 0usize;
        let mut about = 0usize;
        let mut non = 0usize;
        for n in &names {
            match parts_of(n) {
                0 => non += 1,
                1..=10 => about += 1,
                _ => more += 1,
            }
        }
        let total = names.len().max(1) as f32;
        let pct = |n: usize| n as f32 * 100.0 / total;
        vec![
            RepRow {
                label: "More than 10 Speaking Parts".to_string(),
                cells: vec![more.to_string()],
                color: REP_GREEN,
                pct: pct(more),
            },
            RepRow {
                label: "About 10 Speaking Parts".to_string(),
                cells: vec![about.to_string()],
                color: REP_SAND,
                pct: pct(about),
            },
            RepRow {
                label: "Non-Speaking".to_string(),
                cells: vec![non.to_string()],
                color: REP_RED,
                pct: pct(non),
            },
        ]
    }

    fn report_text(&self) -> String {
        let Some(p) = &self.project else {
            return String::new();
        };
        let mut out = String::new();
        out.push_str(&format!(
            "{}\r\n{}\r\nGenerated {}\r\n\r\n",
            self.meta.title,
            self.stats_pane_name(),
            self.stats_gen
        ));
        let labels: Vec<String> = p
            .scenes()
            .iter()
            .enumerate()
            .map(|(i, s)| format!("{} {}", self.scene_label(i), truncate_text(&s.heading, 44)))
            .collect();
        match self.stats_pane {
            StatsPane::Summary => {
                let (text_rows, total_words, total_chars) = stat_text_rows(p, self.stats_notes);
                out.push_str(&format!(
                    "Duration {}\r\nPages {}\r\nWords {}\r\nCharacters {}\r\n\r\n",
                    format_duration(p.duration_secs()).trim_matches(['(', ')']),
                    self.page_count(),
                    total_words,
                    total_chars
                ));
                push_report_section(&mut out, "Text", &text_rows);
                push_report_section(&mut out, "Scenes", &stat_scene_rows(p, &labels));
                push_report_section(&mut out, "Locations", &stat_location_rows(p));
                push_report_section(&mut out, "Characters", &stat_character_rows(p));
            }
            StatsPane::SceneReport => {
                push_report_section(&mut out, "Scenes", &stat_scene_rows(p, &labels));
            }
            StatsPane::LocationReport => {
                push_report_section(&mut out, "Locations", &stat_location_rows(p));
            }
            StatsPane::CastReport => {
                push_report_section(&mut out, "Characters", &stat_character_rows(p));
            }
            StatsPane::SpeakingParts => {
                push_report_section(
                    &mut out,
                    "Characters Speaking Parts",
                    &self.stat_speaking_rows(p),
                );
            }
            StatsPane::Structure | StatsPane::Activity => {
                out.push_str("Plot analysis is not available in this build.\r\n");
            }
        }
        out
    }

    fn save_report_dialog(&mut self) {
        let text = self.report_text();
        if text.is_empty() {
            self.status = "Nothing to save — open a script first.".to_string();
            return;
        }
        let name = format!(
            "{}-{}.txt",
            self.meta.title.replace(' ', "_"),
            self.stats_pane_name().replace(' ', "")
        );
        if let Some(f) = rfd::FileDialog::new().set_file_name(name).save_file() {
            match std::fs::write(&f, text) {
                Ok(()) => self.status = format!("Report saved: {}", f.display()),
                Err(e) => self.status = format!("Report save failed: {e}"),
            }
        }
    }

    /// KIT Start module: project actions on the left, local/online project
    /// list on the right — the launch screen of the original.
    fn start_view(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(kit().window)
                    .inner_margin(egui::Margin::same(0)),
            )
            .show(ctx, |ui| {
                let c = kit();
                let avail_h = ui.available_height();
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.horizontal_top(|ui| {
                    // Left: brand + primary actions + help card.
                    ui.allocate_ui_with_layout(
                        egui::vec2(316.0, avail_h),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_min_width(316.0);
                            ui.set_max_width(316.0);
                            ui.add_space(36.0);
                            ui.horizontal(|ui| {
                                ui.add_space(28.0);
                                ui.vertical(|ui| {
                                    ui.set_width(260.0);
                                    ui.label(
                                        egui::RichText::new("ScriptCraft")
                                            .size(28.0)
                                            .color(c.heading_text),
                                    );
                                    ui.add_space(2.0);
                                    ui.label(
                                        egui::RichText::new(
                                            "Your screenplay, from first idea to final page.",
                                        )
                                        .size(12.5)
                                        .color(c.disabled_text),
                                    );
                                    ui.add_space(28.0);
                                    if start_button_primary(
                                        ui,
                                        Some(KitIcon::Plus),
                                        "Create Project",
                                    ) {
                                        self.do_new();
                                    }
                                    ui.add_space(8.0);
                                    if start_button(ui, Some(KitIcon::Folder), "Open Project") {
                                        self.open_dialog();
                                    }
                                    ui.add_space(36.0);
                                    egui::Frame::new()
                                        .fill(c.surface)
                                        .stroke(egui::Stroke::new(1.0_f32, c.sep))
                                        .corner_radius(12.0)
                                        .inner_margin(egui::Margin::same(14))
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            ui.label(
                                                egui::RichText::new("New to KIT Scenarist?")
                                                    .size(13.5)
                                                    .color(c.heading_text),
                                            );
                                            ui.add_space(4.0);
                                            ui.label(
                                                egui::RichText::new(
                                                    "Learn how to write your own stories and \
                                                     explore KIT Scenarist through our help \
                                                     articles and tutorials.",
                                                )
                                                .size(12.5)
                                                .color(c.disabled_text),
                                            );
                                            ui.add_space(12.0);
                                            if start_button(ui, Some(KitIcon::Help), "Help") {
                                                self.show_help = true;
                                            }
                                        });
                                });
                            });
                        },
                    );
                    let (sep, _) =
                        ui.allocate_exact_size(egui::vec2(1.0, avail_h), egui::Sense::hover());
                    ui.painter().rect_filled(sep, 0.0, c.sep);
                    // Right: projects.
                    ui.vertical(|ui| {
                        egui::Frame::new()
                            .inner_margin(egui::Margin::symmetric(32, 32))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new("Projects")
                                        .size(20.0)
                                        .color(c.heading_text),
                                );
                                ui.add_space(12.0);
                                self.start_tabs(ui);
                                ui.add_space(12.0);
                                egui::ScrollArea::vertical()
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| match self.start_tab {
                                        StartTab::Local => self.start_recent_list(ui),
                                        StartTab::Online => {
                                            ui.add_space(8.0);
                                            ui.label(
                                                egui::RichText::new(
                                                    "Projects from kitscenarist.ru are not available in this build.",
                                                )
                                                .color(c.disabled_text),
                                            );
                                        }
                                    });
                            });
                    });
                });
            });
    }

    /// Light/dark tab strip of the Start module + refresh button.
    fn start_tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for (tab, label) in [
                (StartTab::Local, "Projects from Local Computer"),
                (StartTab::Online, "Projects from kitscenarist.ru"),
            ] {
                if settings_tab(ui, label, self.start_tab == tab).clicked() {
                    self.start_tab = tab;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if kit_square_button(ui, KitIcon::Refresh)
                    .on_hover_text("Refresh")
                    .clicked()
                {
                    self.refresh_recent();
                }
            });
        });
        let (line, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
        ui.painter().rect_filled(line, 0.0, kit().sep);
    }

    /// Recent projects: bold name, dim path + [date], click to open.
    fn start_recent_list(&mut self, ui: &mut egui::Ui) {
        if self.recent.is_empty() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("No recent projects yet")
                        .size(15.0)
                        .color(kit().text),
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Create a project or open an existing one to get started.")
                        .size(12.5)
                        .color(kit().disabled_text),
                );
            });
            return;
        }
        let mut clicked: Option<String> = None;
        for (path, when) in &self.recent {
            let name = std::path::Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.clone());
            if project_card(ui, &name, path, when).clicked() {
                clicked = Some(path.clone());
            }
        }
        if let Some(p) = clicked {
            self.open_path(&p);
        }
    }

    /// Settings module: page list on the left, Common/Modules options right —
    /// KIT's Preferences dialog rendered as a full module.
    fn settings_view(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let avail_h = ui.available_height();
            ui.spacing_mut().item_spacing.x = 0.0;
            let mut changed = false;
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(240.0, avail_h),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_min_width(240.0);
                        ui.set_max_width(240.0);
                        ui.add_space(6.0);
                        for (pane, icon, name) in [
                            (SettingsPane::Application, KitIcon::Gear, "Application"),
                            (SettingsPane::Research, KitIcon::Book, "Research"),
                            (SettingsPane::Cards, KitIcon::Layers, "Cards"),
                            (SettingsPane::ScriptEditor, KitIcon::Script, "Script Editor"),
                            (SettingsPane::Navigator, KitIcon::Paragraph, "Navigator"),
                            (
                                SettingsPane::DurationCounters,
                                KitIcon::Chart,
                                "Duration/Counters",
                            ),
                            (
                                SettingsPane::TemplatesLibrary,
                                KitIcon::File,
                                "Templates Library",
                            ),
                        ] {
                            if settings_row(ui, icon, name, self.settings_pane == pane).clicked() {
                                self.settings_pane = pane;
                            }
                        }
                        let rest = ui.available_height() - 42.0;
                        if rest > 0.0 {
                            ui.add_space(rest);
                        }
                        ui.horizontal(|ui| {
                            ui.add_space(14.0);
                            ui.vertical(|ui| {
                                ui.set_width(212.0);
                                if start_button(ui, None, "Reset Settings") {
                                    self.settings = AppSettings::default();
                                    if self.persist {
                                        save_settings(&self.settings);
                                    }
                                    self.status = "Settings reset to defaults.".to_string();
                                }
                            });
                        });
                        ui.add_space(8.0);
                    },
                );
                ui.add_space(6.0);
                let (sep, _) =
                    ui.allocate_exact_size(egui::vec2(1.0, avail_h), egui::Sense::hover());
                ui.painter().rect_filled(sep, 0.0, kit().sep);
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.add_space(16.0);
                        if settings_tab(ui, "Common", !self.settings_modules).clicked() {
                            self.settings_modules = false;
                        }
                        ui.add_space(6.0);
                        if settings_tab(ui, "Modules", self.settings_modules).clicked() {
                            self.settings_modules = true;
                        }
                    });
                    ui.add_space(12.0);
                    if self.settings_modules {
                        ui.horizontal(|ui| {
                            ui.add_space(16.0);
                            ui.label(
                                egui::RichText::new("No modules installed.")
                                    .color(kit().disabled_text),
                            );
                        });
                    } else if self.settings_pane == SettingsPane::Application {
                        changed |= self.settings_common(ui);
                    } else {
                        ui.horizontal(|ui| {
                            ui.add_space(16.0);
                            ui.label(
                                egui::RichText::new("No settings available yet.")
                                    .color(kit().disabled_text),
                            );
                        });
                    }
                });
            });
            if changed && self.persist {
                save_settings(&self.settings);
            }
        });
    }

    /// Application ▸ Common option rows; returns true when something changed.
    fn settings_common(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
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
            ui.label(egui::RichText::new("English").color(kit().disabled_text));
            ui.add_space(6.0);
            if kit_button(ui, "Change").clicked() {
                self.status = "Only English is bundled in this build.".to_string();
            }
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            changed |= ui
                .checkbox(&mut self.settings.dark_mode, "Use Dark Theme")
                .changed();
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            let r = ui.checkbox(&mut self.settings.autosave, "Automatically Save Every");
            changed |= r.changed();
            ui.add_space(6.0);
            changed |= ui
                .scope(|ui| {
                    ui.style_mut().visuals = v.clone();
                    ui.add_enabled(
                        self.settings.autosave,
                        egui::DragValue::new(&mut self.settings.autosave_minutes)
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
            let r = ui.checkbox(&mut self.settings.backups, "Save Backups");
            changed |= r.changed();
            r.on_hover_text("Timestamped copies are written here on every save.");
            ui.add_space(6.0);
            let field_w = (ui.available_width() - 44.0).max(80.0);
            changed |= ui
                .scope(|ui| {
                    ui.style_mut().visuals = v.clone();
                    ui.add_enabled(
                        self.settings.backups,
                        egui::TextEdit::singleline(&mut self.settings.backups_dir)
                            .margin(egui::Margin::symmetric(8, 4))
                            .font(egui::FontId::proportional(12.5))
                            .desired_width(field_w),
                    )
                })
                .inner
                .changed();
            if kit_square_button(ui, KitIcon::Folder).clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.settings.backups_dir = dir.to_string_lossy().into_owned();
                    changed = true;
                }
            }
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            let r = ui.checkbox(&mut self.settings.spell_check, "Spell Check");
            changed |= r.changed();
            r.on_hover_text("Dictionaries are not bundled in this build (setting is stored only).");
            ui.add_space(6.0);
            ui.add_enabled_ui(false, |ui| {
                egui::ComboBox::from_id_salt("spell_lang")
                    .selected_text("English (GB)")
                    .show_ui(ui, |_ui| {});
            });
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            let r = ui.checkbox(&mut self.settings.compact_mode, "Compact Mode");
            changed |= r.changed();
            r.on_hover_text("Stored only — layout is unchanged in this build.");
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            let r = ui.checkbox(&mut self.settings.two_panel, "Two Panel Mode (F2)");
            changed |= r.changed();
            r.on_hover_text("Stored only — layout is unchanged in this build.");
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            let r = ui.checkbox(&mut self.settings.hidpi, "Enable HiDPI scaling");
            changed |= r.changed();
            r.on_hover_text("Stored only — scaling is unchanged in this build.");
        });
        changed
    }

    /// Fixed-size A4 pages on a recessed desk: the shared layout engine
    /// paints the pages, the widget turns input into commands. Zoom scales
    /// painted geometry only — never a line or page break.
    fn editor_view(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(kit().desk)
                    .inner_margin(egui::Margin::same(0)),
            )
            .show(ctx, |ui| {
                if self.project.is_none() {
                    ui.centered_and_justified(|ui| {
                        ui.vertical_centered(|ui| {
                            ui.heading("ScriptCraft");
                            ui.label("A 100% Rust screenwriting studio (egui + eframe).");
                            ui.label(
                                "Open a .kitsp via the File menu, or create a new script with File \u{25b8} New\u{2026}",
                            );
                        });
                    });
                    return;
                }
                // Trackpad pinch (or \u{2318}/Ctrl+scroll) zooms the fixed pages,
                // keeping the point under the pointer pinned (camera zoom).
                let (pinch, pointer) = ctx.input(|i| (i.zoom_delta(), i.pointer.latest_pos()));
                let mut zoom_anchor = None;
                if (pinch - 1.0).abs() > 1e-3 {
                    self.fit = FitMode::Off;
                    self.zoom = (self.zoom * pinch).clamp(0.25, 4.0);
                    zoom_anchor = pointer;
                }
                let avail = ui.available_size();
                let zoom = match self.fit {
                    FitMode::Off => self.zoom,
                    FitMode::Page => crate::script_view::fit_page_zoom(avail),
                    FitMode::Width => crate::script_view::fit_width_zoom(avail),
                };
                self.last_viewport = avail;
                self.last_render_zoom = zoom;
                let out = match (&self.project, &self.doc) {
                    (Some(p), Some(doc)) => crate::script_view::script_view(
                        ui,
                        zoom,
                        zoom_anchor,
                        &p.blocks,
                        doc,
                        self.typewriter,
                        &mut self.editor2,
                    ),
                    _ => crate::script_view::ScriptViewOutput::default(),
                };
                self.apply_editor2_output(out);
                self.selected_block = self.editor2.selection.map(|s| s.focus.bi);
            });
    }
}

/// Cue text normalized for report matching ("NAME (V.O.)" -> "NAME").
fn cue_name(text: &str) -> String {
    let t = text.trim().to_uppercase();
    match t.find(" (") {
        Some(i) => t[..i].trim().to_string(),
        None => t,
    }
}

fn rep_color(i: usize) -> egui::Color32 {
    const CYCLE: [egui::Color32; 7] = [
        REP_GREEN_LIGHT,
        REP_GREEN,
        REP_PINK,
        REP_RED,
        REP_SAND,
        REP_ORANGE,
        REP_BLUE,
    ];
    CYCLE[i % CYCLE.len()]
}

fn truncate_text(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        format!("{}…", s.chars().take(max).collect::<String>())
    } else {
        s.to_string()
    }
}

/// Text section rows: one per element type (words, characters, word share).
fn stat_text_rows(p: &Project, include_notes: bool) -> (Vec<RepRow>, usize, usize) {
    let kinds: [(ElementType, &str); 9] = [
        (ElementType::SceneHeading, "Scene Heading"),
        (ElementType::SceneCharacters, "Scene Characters"),
        (ElementType::Action, "Action"),
        (ElementType::Character, "Character"),
        (ElementType::Dialogue, "Dialogue"),
        (ElementType::Parenthetical, "Parenthetical"),
        (ElementType::Shot, "Shot"),
        (ElementType::Transition, "Transition"),
        (ElementType::Lyrics, "Lyrics"),
    ];
    let counted = |b: &ScriptBlock| {
        b.element_type.is_printable()
            || (include_notes && b.element_type == ElementType::NonPrintable)
    };
    let total_words: usize = p
        .blocks
        .iter()
        .filter(|b| counted(b))
        .map(|b| b.text.split_whitespace().count())
        .sum();
    let total_chars: usize = p
        .blocks
        .iter()
        .filter(|b| counted(b))
        .map(|b| b.text.chars().count())
        .sum();
    let mut rows = Vec::new();
    for (idx, (t, name)) in kinds.iter().enumerate() {
        let mut size = 0usize;
        let mut words = 0usize;
        let mut chars = 0usize;
        for b in &p.blocks {
            if b.element_type == *t {
                size += 1;
                words += b.text.split_whitespace().count();
                chars += b.text.chars().count();
            }
        }
        if size == 0 {
            continue;
        }
        let pct = if total_words == 0 {
            0.0
        } else {
            words as f32 * 100.0 / total_words as f32
        };
        rows.push(RepRow {
            label: name.to_string(),
            cells: vec![words.to_string(), chars.to_string()],
            color: rep_color(idx),
            pct,
        });
    }
    if include_notes {
        let mut size = 0usize;
        let mut words = 0usize;
        let mut chars = 0usize;
        for b in &p.blocks {
            if b.element_type == ElementType::NonPrintable {
                size += 1;
                words += b.text.split_whitespace().count();
                chars += b.text.chars().count();
            }
        }
        if size > 0 {
            let pct = if total_words == 0 {
                0.0
            } else {
                words as f32 * 100.0 / total_words as f32
            };
            rows.push(RepRow {
                label: "Non-Printable".to_string(),
                cells: vec![words.to_string(), chars.to_string()],
                color: egui::Color32::from_gray(0x8A),
                pct,
            });
        }
    }
    (rows, total_words, total_chars)
}

fn stat_scene_rows(p: &Project, labels: &[String]) -> Vec<RepRow> {
    let scenes = p.scenes();
    let total_blocks: usize = scenes
        .iter()
        .map(|s| {
            s.blocks
                .iter()
                .filter_map(|bi| p.blocks.get(*bi))
                .filter(|b| b.element_type.is_printable())
                .count()
        })
        .sum();
    let mut rows = Vec::new();
    for (i, s) in scenes.iter().enumerate() {
        let blocks = s
            .blocks
            .iter()
            .filter_map(|bi| p.blocks.get(*bi))
            .filter(|b| b.element_type.is_printable())
            .count();
        let words: usize = s
            .blocks
            .iter()
            .filter_map(|bi| p.blocks.get(*bi))
            .filter(|b| b.element_type.is_printable())
            .map(|b| b.text.split_whitespace().count())
            .sum();
        let secs = p.scene_duration_secs(Some(s));
        let pct = if total_blocks == 0 {
            0.0
        } else {
            blocks as f32 * 100.0 / total_blocks as f32
        };
        rows.push(RepRow {
            label: labels.get(i).cloned().unwrap_or_default(),
            cells: vec![
                format_duration(secs).trim_matches(['(', ')']).to_string(),
                words.to_string(),
            ],
            color: rep_color(i),
            pct,
        });
    }
    rows
}

/// "INT." / "EXT." / "INT./EXT." place token of a scene heading.
fn scene_place(heading: &str) -> String {
    let token = heading.split_whitespace().next().unwrap_or("").trim();
    let ok = !token.is_empty()
        && token.chars().any(|c| c.is_alphabetic())
        && token
            .chars()
            .all(|c| c.is_uppercase() || c == '.' || c == '/');
    if ok {
        token.replace('.', "")
    } else {
        "—".to_string()
    }
}

fn stat_location_rows(p: &Project) -> Vec<RepRow> {
    let scenes = p.scenes();
    let total = scenes.len().max(1) as f32;
    let mut order: Vec<String> = Vec::new();
    let mut stats: Vec<(usize, f64)> = Vec::new();
    for s in &scenes {
        let place = scene_place(&s.heading);
        let idx = match order.iter().position(|o| *o == place) {
            Some(i) => i,
            None => {
                order.push(place);
                stats.push((0, 0.0));
                stats.len() - 1
            }
        };
        stats[idx].0 += 1;
        stats[idx].1 += p.scene_duration_secs(Some(s));
    }
    let mut rows: Vec<RepRow> = order
        .iter()
        .zip(stats.iter())
        .enumerate()
        .map(|(i, (name, (count, secs)))| RepRow {
            label: name.clone(),
            cells: vec![
                format_duration(*secs).trim_matches(['(', ')']).to_string(),
                count.to_string(),
            ],
            color: rep_color(i),
            pct: *count as f32 * 100.0 / total,
        })
        .collect();
    rows.sort_by(|a, b| {
        b.pct
            .partial_cmp(&a.pct)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    rows
}

/// (name, dialogue blocks, words, characters) per speaking character.
fn stat_char_stats(p: &Project) -> Vec<(String, usize, usize, usize)> {
    let mut map: Vec<(String, usize, usize, usize)> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for b in &p.blocks {
        match b.element_type {
            ElementType::Character => {
                current = vec![cue_name(&b.text)];
            }
            ElementType::SceneCharacters => {
                current = b
                    .text
                    .split(',')
                    .map(cue_name)
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            ElementType::Dialogue => {
                for name in &current {
                    let idx = match map.iter().position(|(n, _, _, _)| n == name) {
                        Some(i) => i,
                        None => {
                            map.push((name.clone(), 0, 0, 0));
                            map.len() - 1
                        }
                    };
                    let e = &mut map[idx];
                    e.1 += 1;
                    e.2 += b.text.split_whitespace().count();
                    e.3 += b.text.chars().count();
                }
            }
            _ => {}
        }
    }
    map.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    map
}

fn stat_character_rows(p: &Project) -> Vec<RepRow> {
    let stats = stat_char_stats(p);
    let total_words: usize = stats.iter().map(|(_, _, w, _)| *w).sum();
    let total_chars: usize = stats.iter().map(|(_, _, _, c)| *c).sum();
    let total_secs = p.duration_secs();
    stats
        .iter()
        .enumerate()
        .map(|(i, (name, _parts, words, chars))| {
            let secs = if total_words == 0 {
                0.0
            } else {
                total_secs * (*words as f64 / total_words as f64)
            };
            let pct = if total_chars == 0 {
                0.0
            } else {
                *chars as f32 * 100.0 / total_chars as f32
            };
            RepRow {
                label: name.clone(),
                cells: vec![
                    format_duration(secs).trim_matches(['(', ')']).to_string(),
                    words.to_string(),
                    chars.to_string(),
                ],
                color: rep_color(i),
                pct,
            }
        })
        .collect()
}

fn push_report_section(out: &mut String, title: &str, rows: &[RepRow]) {
    out.push_str(title);
    out.push_str("\r\n");
    for r in rows {
        out.push_str("  ");
        out.push_str(&r.label);
        for c in &r.cells {
            out.push_str("  ");
            out.push_str(c);
        }
        out.push_str("  ");
        out.push_str(&pct_text(r.pct));
        out.push_str("\r\n");
    }
    out.push_str("\r\n");
}
