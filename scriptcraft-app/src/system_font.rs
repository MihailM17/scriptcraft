//! Courier New is a Monotype font that ships with macOS and Windows. It is
//! **not** bundled with ScriptCraft (redistribution isn't allowed), so the
//! face is read from the operating system when the writer picks it and falls
//! back to Courier Prime when it isn't installed.
//!
//! Both faces are 0.6 em monospace, so the page grid, line breaks and
//! pagination are identical either way; only the glyph shapes change.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Courier New's three faces, as read from disk. Bold and italic are optional
/// because a stripped-down system image may ship only the regular weights.
pub struct SystemCourier {
    pub regular: Vec<u8>,
    pub bold: Option<Vec<u8>>,
    pub italic: Option<Vec<u8>>,
}

/// The installed Courier New, loaded once per launch. `None` means the system
/// does not have it (or the files are unreadable), so callers fall back to
/// Courier Prime.
pub fn courier_new() -> Option<&'static SystemCourier> {
    static FONT: OnceLock<Option<SystemCourier>> = OnceLock::new();
    FONT.get_or_init(|| {
        let regular = courier_new_paths().into_iter().find(|p| p.is_file())?;
        let bold = courier_new_bold_paths().into_iter().find(|p| p.is_file());
        let italic = courier_new_italic_paths().into_iter().find(|p| p.is_file());
        load_from(&regular, bold.as_deref(), italic.as_deref())
    })
    .as_ref()
}

/// True when the chosen face can be drawn with real Courier New glyphs.
pub fn courier_new_installed() -> bool {
    courier_new().is_some()
}

/// Regular-face candidates, most likely first.
pub fn courier_new_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    #[cfg(target_os = "macos")]
    {
        paths.push(PathBuf::from(
            "/System/Library/Fonts/Supplemental/Courier New.ttf",
        ));
        paths.push(PathBuf::from("/Library/Fonts/Courier New.ttf"));
    }
    #[cfg(target_os = "windows")]
    {
        let windir = std::env::var_os("WINDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        paths.push(windir.join("Fonts").join("cour.ttf"));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        for dir in linux_font_dirs() {
            paths.push(dir.join("msttcorefonts").join("Courier_New.ttf"));
            paths.push(dir.join("Courier_New.ttf"));
            paths.push(dir.join("ttf-mscorefonts").join("Courier_New.ttf"));
        }
    }
    paths
}

/// Bold-face candidates.
pub fn courier_new_bold_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    #[cfg(target_os = "macos")]
    {
        paths.push(PathBuf::from(
            "/System/Library/Fonts/Supplemental/Courier New Bold.ttf",
        ));
        paths.push(PathBuf::from("/Library/Fonts/Courier New Bold.ttf"));
    }
    #[cfg(target_os = "windows")]
    {
        let windir = std::env::var_os("WINDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        paths.push(windir.join("Fonts").join("courbd.ttf"));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        for dir in linux_font_dirs() {
            paths.push(dir.join("msttcorefonts").join("Courier_New_Bold.ttf"));
            paths.push(dir.join("Courier_New_Bold.ttf"));
            paths.push(dir.join("ttf-mscorefonts").join("Courier_New_Bold.ttf"));
        }
    }
    paths
}

/// Italic-face candidates. Courier New Italic is used for parentheticals and
/// emphasised action, not only for the editor's carets.
pub fn courier_new_italic_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    #[cfg(target_os = "macos")]
    {
        paths.push(PathBuf::from(
            "/System/Library/Fonts/Supplemental/Courier New Italic.ttf",
        ));
        paths.push(PathBuf::from("/Library/Fonts/Courier New Italic.ttf"));
    }
    #[cfg(target_os = "windows")]
    {
        let windir = std::env::var_os("WINDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        paths.push(windir.join("Fonts").join("couri.ttf"));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        for dir in linux_font_dirs() {
            paths.push(dir.join("msttcorefonts").join("Courier_New_Italic.ttf"));
            paths.push(dir.join("Courier_New_Italic.ttf"));
            paths.push(dir.join("ttf-mscorefonts").join("Courier_New_Italic.ttf"));
        }
    }
    paths
}

/// Linux distributions scatter the Microsoft core fonts differently; try the
/// common roots. The per-user dir is where `fc-cache`-installed copies land.
#[cfg(all(unix, not(target_os = "macos")))]
fn linux_font_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/usr/share/fonts/truetype"),
        PathBuf::from("/usr/share/fonts"),
        PathBuf::from("/usr/local/share/fonts"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/fonts"));
    }
    dirs
}

/// Read the faces at explicit paths. Split out from [`courier_new`] so the
/// fallback behaviour is testable without depending on the host's fonts.
pub fn load_from(
    regular: &Path,
    bold: Option<&Path>,
    italic: Option<&Path>,
) -> Option<SystemCourier> {
    let regular = read_font(regular)?;
    Some(SystemCourier {
        regular,
        bold: bold.and_then(read_font),
        italic: italic.and_then(read_font),
    })
}

/// Read a font file and reject empty or obviously-not-a-font files, so a
/// broken system file degrades to Courier Prime instead of breaking export.
fn read_font(path: &Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < 1024 {
        return None;
    }
    let tag = &bytes[..4];
    let is_font = matches!(tag, [0x00, 0x01, 0x00, 0x00] | b"OTTO" | b"true" | b"ttcf");
    is_font.then_some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_lists_are_not_empty() {
        assert!(!courier_new_paths().is_empty());
        assert!(!courier_new_bold_paths().is_empty());
        assert!(!courier_new_italic_paths().is_empty());
    }

    #[test]
    fn candidates_all_end_in_ttf_or_ttc() {
        for path in courier_new_paths()
            .into_iter()
            .chain(courier_new_bold_paths())
            .chain(courier_new_italic_paths())
        {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            assert!(matches!(ext, "ttf" | "ttc"), "{}", path.display());
        }
    }

    #[test]
    fn a_missing_font_returns_none() {
        let missing = Path::new("/definitely/not/here/Courier New.ttf");
        assert!(load_from(missing, None, None).is_none());
    }

    #[test]
    fn junk_files_are_rejected() {
        let dir = std::env::temp_dir().join(format!("scriptcraft-font-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("junk.ttf");
        std::fs::write(&path, vec![0xAA; 4096]).unwrap();
        assert!(load_from(&path, None, None).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn short_files_are_rejected() {
        let dir = std::env::temp_dir().join(format!("scriptcraft-short-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("short.ttf");
        std::fs::write(&path, [0x00, 0x01, 0x00, 0x00]).unwrap();
        assert!(load_from(&path, None, None).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// macOS ships Courier New in the Supplemental folder; if this fails the
    /// face selector still works and falls back, so assert the file shape
    /// rather than requiring every CI image to have it.
    #[test]
    fn macos_candidate_points_at_the_supplemental_font() {
        #[cfg(target_os = "macos")]
        assert!(courier_new_paths()
            .iter()
            .any(|p| p.ends_with("Supplemental/Courier New.ttf")));
    }
}
