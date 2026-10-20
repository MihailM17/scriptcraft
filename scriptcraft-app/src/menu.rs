//! Native macOS menu bar, matching KIT's File/Edit menus.
//!
//! Built with `muda`; menu events are polled from the global channel in
//! `ScriptCraftApp::update`. The in-app "Menu" popup mirrors the same items.

use muda::accelerator::Accelerator;
use muda::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

fn acc(s: &str) -> Option<Accelerator> {
    s.parse().ok()
}

pub const ID_NEW: &str = "kit.new";
pub const ID_OPEN: &str = "kit.open";
pub const ID_SAVE: &str = "kit.save";
pub const ID_SAVE_AS: &str = "kit.save_as";
pub const ID_IMPORT: &str = "kit.import";
pub const ID_IMPORT_FDX: &str = "kit.import_fdx";
pub const ID_EXPORT: &str = "kit.export";
pub const ID_PREVIEW: &str = "kit.preview";
pub const ID_TWO_PANEL: &str = "kit.two_panel";
pub const ID_UNDO: &str = "kit.undo";
pub const ID_REDO: &str = "kit.redo";
pub const ID_FIND: &str = "kit.find";
pub const ID_NAVIGATOR: &str = "kit.navigator";
pub const ID_FORMAT_PANEL: &str = "kit.format_panel";
pub const ID_TYPEWRITER: &str = "kit.typewriter";
pub const ID_ZOOM_IN: &str = "kit.zoom_in";
pub const ID_ZOOM_OUT: &str = "kit.zoom_out";
pub const ID_ZOOM_RESET: &str = "kit.zoom_reset";
pub const ID_ZOOM_FIT_PAGE: &str = "kit.zoom_fit_page";
pub const ID_ZOOM_FIT_WIDTH: &str = "kit.zoom_fit_width";
pub const ID_ABOUT: &str = "kit.about";
pub const ID_HELP: &str = "kit.help";
pub const ID_QUIT: &str = "kit.quit";

pub struct NativeMenu {
    _menu: Menu,
    pub two_panel: CheckMenuItem,
    pub navigator: CheckMenuItem,
    pub format_panel: CheckMenuItem,
    pub typewriter: CheckMenuItem,
}

impl NativeMenu {
    /// Build the menu bar and attach it to the macOS application.
    pub fn install(
        two_panel: bool,
        navigator: bool,
        format_panel: bool,
        typewriter: bool,
    ) -> muda::Result<Self> {
        let new = MenuItem::with_id(ID_NEW, "New", true, None);
        let open = MenuItem::with_id(ID_OPEN, "Open...", true, acc("CmdOrCtrl+O"));
        let save = MenuItem::with_id(ID_SAVE, "Save", true, acc("CmdOrCtrl+S"));
        let save_as = MenuItem::with_id(ID_SAVE_AS, "Save as...", true, acc("Shift+CmdOrCtrl+S"));
        let import = MenuItem::with_id(ID_IMPORT, "Import Fountain...", true, None);
        let import_fdx = MenuItem::with_id(ID_IMPORT_FDX, "Import Final Draft...", true, None);
        let export = MenuItem::with_id(ID_EXPORT, "Export to...", true, None);
        let preview = MenuItem::with_id(ID_PREVIEW, "Print preview script", true, acc("F12"));
        let two_panel_item =
            CheckMenuItem::with_id(ID_TWO_PANEL, "Two panel mode", true, two_panel, acc("F2"));

        // The first submenu is the macOS application menu (app name).
        let app_about = MenuItem::with_id(ID_ABOUT, "About ScriptCraft", true, None);
        let app_sep_a = PredefinedMenuItem::separator();
        let services = PredefinedMenuItem::services(None);
        let app_sep_b = PredefinedMenuItem::separator();
        let hide = PredefinedMenuItem::hide(Some("Hide ScriptCraft"));
        let hide_others = PredefinedMenuItem::hide_others(Some("Hide Others"));
        let show_all = PredefinedMenuItem::show_all(Some("Show All"));
        let app_sep_c = PredefinedMenuItem::separator();
        let quit = MenuItem::with_id(ID_QUIT, "Quit ScriptCraft", true, acc("CmdOrCtrl+Q"));
        let app_menu = Submenu::with_items(
            "ScriptCraft",
            true,
            &[
                &app_about,
                &app_sep_a,
                &services,
                &app_sep_b,
                &hide,
                &hide_others,
                &show_all,
                &app_sep_c,
                &quit,
            ],
        )?;

        let sep_a = PredefinedMenuItem::separator();
        let sep_b = PredefinedMenuItem::separator();
        let file = Submenu::with_items(
            "File",
            true,
            &[
                &new,
                &open,
                &save,
                &save_as,
                &sep_a,
                &import,
                &import_fdx,
                &export,
                &preview,
                &sep_b,
                &two_panel_item,
            ],
        )?;

        let undo = MenuItem::with_id(ID_UNDO, "Undo", true, acc("CmdOrCtrl+Z"));
        let redo = MenuItem::with_id(ID_REDO, "Redo", true, acc("Shift+CmdOrCtrl+Z"));
        let find = MenuItem::with_id(ID_FIND, "Find...", true, acc("CmdOrCtrl+F"));
        let sep_c = PredefinedMenuItem::separator();
        let edit = Submenu::with_items("Edit", true, &[&undo, &redo, &sep_c, &find])?;

        let navigator_item =
            CheckMenuItem::with_id(ID_NAVIGATOR, "Navigator", true, navigator, None);
        let format_item =
            CheckMenuItem::with_id(ID_FORMAT_PANEL, "Format panel", true, format_panel, None);
        let typewriter_item = CheckMenuItem::with_id(
            ID_TYPEWRITER,
            "Typewriter scrolling",
            true,
            typewriter,
            None,
        );
        let zoom_in = MenuItem::with_id(ID_ZOOM_IN, "Zoom In", true, acc("CmdOrCtrl+="));
        let zoom_out = MenuItem::with_id(ID_ZOOM_OUT, "Zoom Out", true, acc("CmdOrCtrl+-"));
        let zoom_reset = MenuItem::with_id(ID_ZOOM_RESET, "Actual Size", true, acc("CmdOrCtrl+0"));
        let zoom_fit_page = MenuItem::with_id(ID_ZOOM_FIT_PAGE, "Fit Page", true, None);
        let zoom_fit_width = MenuItem::with_id(ID_ZOOM_FIT_WIDTH, "Fit Width", true, None);
        let sep_d = PredefinedMenuItem::separator();
        let view = Submenu::with_items(
            "View",
            true,
            &[
                &navigator_item,
                &format_item,
                &typewriter_item,
                &sep_d,
                &zoom_in,
                &zoom_out,
                &zoom_reset,
                &zoom_fit_page,
                &zoom_fit_width,
            ],
        )?;

        let about = MenuItem::with_id(ID_ABOUT, "About ScriptCraft", true, None);
        let help = MenuItem::with_id(ID_HELP, "ScriptCraft Help", true, acc("F1"));
        let help_menu = Submenu::with_items("Help", true, &[&about, &help])?;

        let menu = Menu::with_items(&[&app_menu, &file, &edit, &view, &help_menu])?;
        #[cfg(target_os = "macos")]
        menu.init_for_nsapp();

        Ok(Self {
            _menu: menu,
            two_panel: two_panel_item,
            navigator: navigator_item,
            format_panel: format_item,
            typewriter: typewriter_item,
        })
    }
}

/// Drain the global menu-event channel.
pub fn poll() -> Vec<String> {
    let mut out = Vec::new();
    while let Ok(event) = MenuEvent::receiver().try_recv() {
        out.push(event.id.0.clone());
    }
    out
}
