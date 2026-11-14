//! Spellchecking: Hunspell dictionaries, a screenwriting-aware tokenizer and a
//! per-paragraph cache.
//!
//! Dictionaries ship as data next to the binary (see
//! [`dictionary_search_paths`]) rather than inside it: they are large, they
//! carry their own licences, and a user can drop another one in place. The
//! engine lives in a thread-local because egui paints on the UI thread and a
//! parsed dictionary is expensive to rebuild.
//!
//! Tokenizing is caller-side by design: this module knows that a screenplay is
//! mostly ALL-CAPS slugs, cues and sound effects, so those are skipped by
//! default instead of being flagged on every line.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Languages whose dictionaries ship with the app.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpellLang {
    #[default]
    EnUs,
    BgBg,
    RuRu,
}

impl SpellLang {
    pub const ALL: [SpellLang; 3] = [SpellLang::EnUs, SpellLang::BgBg, SpellLang::RuRu];

    /// Menu label.
    pub fn label(self) -> &'static str {
        match self {
            SpellLang::EnUs => "English (US)",
            SpellLang::BgBg => "Bulgarian",
            SpellLang::RuRu => "Russian",
        }
    }

    /// Stable key: the Hunspell locale, also the dictionary's folder name.
    pub fn key(self) -> &'static str {
        match self {
            SpellLang::EnUs => "en_US",
            SpellLang::BgBg => "bg_BG",
            SpellLang::RuRu => "ru_RU",
        }
    }

    pub fn from_key(key: &str) -> Option<SpellLang> {
        SpellLang::ALL.into_iter().find(|l| l.key() == key)
    }
}

/// A misspelled run of characters, in paragraph char offsets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Misspelling {
    pub start: usize,
    pub end: usize,
}

impl Misspelling {
    /// Whether a click at `ch` lands inside this word.
    pub fn contains(&self, ch: usize) -> bool {
        self.start <= ch && ch < self.end
    }
}
/// One word as it appears in a paragraph.
#[derive(Clone, Copy, Debug)]
pub struct Token<'a> {
    /// Start char offset (inclusive).
    pub start: usize,
    /// End char offset (exclusive).
    pub end: usize,
    pub word: &'a str,
}

fn is_word_char(c: char) -> bool {
    c.is_alphabetic() || c.is_numeric() || c == '\'' || c == '\u{2019}' || c == '-'
}

/// Split a paragraph into words, keeping internal apostrophes and hyphens so
/// `don't` and `light-weight` reach the dictionary as one token.
///
/// Digits stay attached (`3rd`, `COVID-19`) so those tokens can be skipped
/// whole rather than reduced to `rd`.
pub fn tokenize(text: &str) -> Vec<Token<'_>> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !is_word_char(chars[i].1) {
            i += 1;
            continue;
        }
        let start_i = i;
        while i < chars.len() && is_word_char(chars[i].1) {
            i += 1;
        }
        // Trim leading/trailing punctuation-ish word chars ("--", "'twas").
        let mut s = start_i;
        let mut e = i;
        let keep = |c: char| c.is_alphabetic() || c.is_numeric();
        while s < e && !keep(chars[s].1) {
            s += 1;
        }
        while e > s && !keep(chars[e - 1].1) {
            e -= 1;
        }
        if e > s {
            let start_byte = chars[s].0;
            let end_byte = chars.get(e).map_or(text.len(), |(b, _)| *b);
            out.push(Token {
                start: s,
                end: e,
                word: &text[start_byte..end_byte],
            });
        }
    }
    out
}

/// True when every cased letter is uppercase (`INT`, `MARA`, `BANG`).
/// Screenplay slugs, cues and sound effects are written this way.
fn is_all_caps(word: &str) -> bool {
    let mut cased = 0;
    for c in word.chars().filter(|c| c.is_alphabetic()) {
        cased += 1;
        if !c.is_uppercase() {
            return false;
        }
    }
    cased > 1
}

fn has_digit(word: &str) -> bool {
    word.chars().any(|c| c.is_numeric())
}

// ---- dictionary location ---------------------------------------------------

/// Where to look for the `dictionaries/` folder, in order.
///
/// The app ships the folder next to the executable; macOS keeps it in
/// `Contents/Resources`. `CARGO_MANIFEST_DIR` is the `cargo run`/test
/// fallback and simply doesn't exist in an installed build.
pub fn dictionary_search_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(dir) = std::env::var_os("SCRIPTCRAFT_DICTIONARIES") {
        out.push(PathBuf::from(dir));
    }
    if let Some(exe) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        out.push(exe.join("dictionaries"));
        if let Some(contents) = exe.parent() {
            out.push(contents.join("Resources/dictionaries"));
        }
    }
    out.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/dictionaries"));
    out
}

/// The first search path that exists.
pub fn dictionary_dir() -> Option<PathBuf> {
    dictionary_search_paths().into_iter().find(|p| p.is_dir())
}

fn check(lang: SpellLang, dir: &Path) -> Result<spellbook::Dictionary, String> {
    let base = dir.join(lang.key());
    let aff = std::fs::read_to_string(base.join(format!("{}.aff", lang.key())))
        .map_err(|e| format!("{} dictionary: {e}", lang.label()))?;
    let dic = std::fs::read_to_string(base.join(format!("{}.dic", lang.key())))
        .map_err(|e| format!("{} dictionary: {e}", lang.label()))?;
    spellbook::Dictionary::new(&aff, &dic).map_err(|e| format!("{} dictionary: {e}", lang.label()))
}

// ---- engine ----------------------------------------------------------------

/// One language's dictionary plus the words the user taught it.
struct Checker {
    dict: spellbook::Dictionary,
    custom: HashSet<String>,
    ignored: HashSet<String>,
}

impl Checker {
    fn accepts(&self, word: &str) -> bool {
        let lower = word.to_lowercase();
        self.custom.contains(&lower) || self.ignored.contains(&lower) || self.dict.check(word)
    }
}

struct Engine {
    enabled: bool,
    lang: SpellLang,
    ignore_caps: bool,
    checker: Option<Checker>,
    error: Option<String>,
    /// Cache keyed by `(block, paragraph)`, holding the text hash it was built
    /// from. Only visible paragraphs are ever checked, so this stays small.
    cache: HashMap<(usize, usize), (u64, Vec<Misspelling>)>,
    /// Overridable for tests; production uses the per-user config folder.
    custom_dir: Option<PathBuf>,
}

impl Default for Engine {
    fn default() -> Self {
        Self {
            enabled: false,
            lang: SpellLang::default(),
            ignore_caps: true,
            checker: None,
            error: None,
            cache: HashMap::new(),
            custom_dir: None,
        }
    }
}

thread_local! {
    static ENGINE: RefCell<Engine> = RefCell::new(Engine::default());
}

fn custom_file(dir: &Path, lang: SpellLang) -> PathBuf {
    dir.join(format!("custom-dictionary-{}.txt", lang.key()))
}

fn load_custom(dir: &Path, lang: SpellLang) -> HashSet<String> {
    std::fs::read_to_string(custom_file(dir, lang))
        .map(|text| {
            text.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(|l| l.to_lowercase())
                .collect()
        })
        .unwrap_or_default()
}

fn resolve_custom_dir(engine: &Engine) -> PathBuf {
    engine
        .custom_dir
        .clone()
        .unwrap_or_else(crate::app::config_dir)
}

impl Engine {
    fn load(&mut self) {
        self.cache.clear();
        self.checker = None;
        self.error = None;
        let Some(dir) = dictionary_dir() else {
            self.error = Some("Dictionaries folder not found.".to_string());
            return;
        };
        match check(self.lang, &dir) {
            Ok(dict) => {
                let custom = load_custom(&resolve_custom_dir(self), self.lang);
                self.checker = Some(Checker {
                    dict,
                    custom,
                    ignored: HashSet::new(),
                });
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn flags(&mut self, bi: usize, para: usize, text: &str) -> Vec<Misspelling> {
        if !self.enabled {
            return Vec::new();
        }
        if self.checker.is_none() && self.error.is_none() {
            self.load();
        }
        let Some(checker) = self.checker.as_ref() else {
            return Vec::new();
        };

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        let hash = hasher.finish();

        if let Some((cached_hash, ranges)) = self.cache.get(&(bi, para)) {
            if *cached_hash == hash {
                return ranges.clone();
            }
        }

        let mut ranges = Vec::new();
        for token in tokenize(text) {
            if has_digit(token.word) {
                continue;
            }
            if self.ignore_caps && is_all_caps(token.word) {
                continue;
            }
            if checker.accepts(token.word) {
                continue;
            }
            ranges.push(Misspelling {
                start: token.start,
                end: token.end,
            });
        }

        if self.cache.len() > 20_000 {
            self.cache.clear();
        }
        self.cache.insert((bi, para), (hash, ranges.clone()));
        ranges
    }
}

/// Tell the engine what the settings say. Cheap when nothing changed; the
/// dictionary is parsed at most once per language change.
pub fn configure(enabled: bool, lang: SpellLang, ignore_caps: bool) {
    ENGINE.with(|e| {
        let mut e = e.borrow_mut();
        let lang_changed = e.lang != lang;
        if e.enabled == enabled && !lang_changed && e.ignore_caps == ignore_caps {
            return;
        }
        e.enabled = enabled;
        e.ignore_caps = ignore_caps;
        if lang_changed {
            e.lang = lang;
            // Drop the old language's dictionary; `flags` loads the new one on
            // the next paint so a language switch doesn't parse two of them.
            e.checker = None;
            e.error = None;
            e.cache.clear();
        }
    });
}

/// Misspellings in one paragraph of one block, in paragraph char offsets.
pub fn flags(bi: usize, para: usize, text: &str) -> Vec<Misspelling> {
    ENGINE.with(|e| e.borrow_mut().flags(bi, para, text))
}

pub fn is_enabled() -> bool {
    ENGINE.with(|e| e.borrow().enabled)
}

/// Up to `max` suggestions for `word`, best first.
pub fn suggestions(word: &str, max: usize) -> Vec<String> {
    ENGINE.with(|e| {
        let e = e.borrow();
        let Some(checker) = e.checker.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        checker.dict.suggest(word, &mut out);
        out.truncate(max);
        out
    })
}

/// Add a word to the user's custom dictionary for the active language and
/// write it to disk. Returns the error text if the file can't be written.
pub fn add_word(word: &str) -> Result<(), String> {
    ENGINE.with(|e| {
        let mut e = e.borrow_mut();
        let lower = word.to_lowercase();
        let dir = resolve_custom_dir(&e);
        if let Err(err) = std::fs::create_dir_all(&dir) {
            return Err(format!("Could not save the dictionary: {err}"));
        }
        let path = custom_file(&dir, e.lang);
        let mut text = std::fs::read_to_string(&path).unwrap_or_default();
        // Re-adding a word must not grow the file.
        if text.lines().any(|l| l.trim().eq_ignore_ascii_case(word)) {
            if let Some(checker) = e.checker.as_mut() {
                checker.custom.insert(lower.clone());
                checker.ignored.remove(&lower);
            }
            e.cache.clear();
            return Ok(());
        }
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(word);
        text.push('\n');
        if let Err(err) = std::fs::write(&path, text) {
            return Err(format!("Could not save the dictionary: {err}"));
        }
        if let Some(checker) = e.checker.as_mut() {
            checker.custom.insert(lower.clone());
            checker.ignored.remove(&lower);
        }
        e.cache.clear();
        Ok(())
    })
}

/// Stop flagging a word for the rest of the session.
pub fn ignore_word(word: &str) {
    ENGINE.with(|e| {
        let mut e = e.borrow_mut();
        let lower = word.to_lowercase();
        if let Some(checker) = e.checker.as_mut() {
            checker.ignored.insert(lower);
        }
        e.cache.clear();
    });
}

/// Drop every cached result (after a dictionary change or an edit).
pub fn clear_cache() {
    ENGINE.with(|e| e.borrow_mut().cache.clear());
}

/// The error shown in Settings when a dictionary can't be loaded.
pub fn error() -> Option<String> {
    ENGINE.with(|e| e.borrow().error.clone())
}

/// Language of the loaded dictionary (`None` before the first load).
pub fn active_lang() -> Option<SpellLang> {
    ENGINE.with(|e| {
        let e = e.borrow();
        e.checker.as_ref().map(|_| e.lang)
    })
}

// ---- test hooks ------------------------------------------------------------

/// Point the custom dictionary at a temporary folder. Each test runs on its
/// own thread, so this never leaks into another test.
#[doc(hidden)]
pub fn set_custom_dir_for_test(dir: PathBuf) {
    ENGINE.with(|e| {
        let mut e = e.borrow_mut();
        e.custom_dir = Some(dir);
        e.cache.clear();
        let custom = load_custom(&resolve_custom_dir(&e), e.lang);
        if let Some(checker) = e.checker.as_mut() {
            checker.custom = custom;
        }
    });
}

/// Restore the disabled default (unit tests share a thread per test, but the
/// interaction harness reuses one).
#[doc(hidden)]
pub fn reset_for_test() {
    ENGINE.with(|e| *e.borrow_mut() = Engine::default());
}

/// The word around `ch`, if `ch` sits inside one.
pub fn word_at(text: &str, ch: usize) -> Option<Token<'_>> {
    tokenize(text).into_iter().find(|t| t.contains_char(ch))
}

impl Token<'_> {
    fn contains_char(&self, ch: usize) -> bool {
        self.start <= ch && ch < self.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<(usize, usize, String)> {
        tokenize(text)
            .into_iter()
            .map(|t| (t.start, t.end, t.word.to_string()))
            .collect()
    }

    #[test]
    fn tokenizer_keeps_apostrophes_and_hyphens() {
        let got = words("Don't light-weight it, it's 3rd.");
        assert_eq!(
            got,
            vec![
                (0, 5, "Don't".to_string()),
                (6, 18, "light-weight".to_string()),
                (19, 21, "it".to_string()),
                (23, 27, "it's".to_string()),
                (28, 31, "3rd".to_string()),
            ]
        );
    }

    #[test]
    fn tokenizer_handles_cyrillic_and_ellipsis() {
        let got = words("Иван отива… към „къщата“!");
        let words: Vec<&str> = got.iter().map(|(_, _, w)| w.as_str()).collect();
        assert_eq!(words, vec!["Иван", "отива", "към", "къщата"]);
    }

    #[test]
    fn tokenizer_offsets_are_char_not_byte_offsets() {
        // Cyrillic is two bytes per char: offsets must still count chars.
        let got = words("да не");
        assert_eq!(got[0].0, 0);
        assert_eq!(got[1].0, 3);
        assert_eq!(got[1].1, 5);
    }

    #[test]
    fn tokenizer_trims_stray_punctuation() {
        let got = words("-- a --");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].2, "a");
    }

    #[test]
    fn all_caps_skips_slugs_and_short_words() {
        assert!(is_all_caps("INT"));
        assert!(is_all_caps("MARA"));
        assert!(!is_all_caps("Ivan"));
        // A single capital is a word ("I"), not a slug.
        assert!(!is_all_caps("I"));
        // Uncased scripts are never "all caps" and must be checked.
        assert!(!is_all_caps("къщата"));
    }

    #[test]
    fn word_at_finds_the_clicked_word() {
        let text = "Иван отива";
        assert_eq!(word_at(text, 0).unwrap().word, "Иван");
        // The last character of a word is still inside it; the space after it
        // is not.
        assert_eq!(word_at(text, 3).unwrap().word, "Иван");
        assert_eq!(word_at(text, 5).unwrap().word, "отива");
        assert!(word_at(text, 4).is_none(), "the space belongs to no word");
    }

    #[test]
    fn shipped_dictionaries_load_and_check() {
        let dir = dictionary_dir().expect("dictionaries folder");
        for lang in SpellLang::ALL {
            let dict = check(lang, &dir).unwrap_or_else(|e| panic!("{}: {e}", lang.label()));
            match lang {
                SpellLang::EnUs => {
                    assert!(dict.check("hello"));
                    assert!(dict.check("screenplay"));
                }
                SpellLang::BgBg => {
                    assert!(dict.check("къщата"));
                    assert!(dict.check("Иван"));
                }
                SpellLang::RuRu => {
                    assert!(dict.check("сценарий"));
                }
            }
        }
    }

    #[test]
    fn custom_dictionary_round_trips_through_a_file() {
        let dir = std::env::temp_dir().join(format!("scriptcraft-spell-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        set_custom_dir_for_test(dir.clone());
        configure(true, SpellLang::EnUs, true);

        assert_eq!(flags(0, 0, "Zorblax").len(), 1, "unknown word is flagged");
        add_word("Zorblax").expect("write custom dictionary");
        assert!(flags(0, 1, "Zorblax").is_empty(), "added word is accepted");

        let saved = std::fs::read_to_string(custom_file(&dir, SpellLang::EnUs)).unwrap();
        assert!(saved.contains("Zorblax"));

        // A fresh engine reads the file back.
        reset_for_test();
        set_custom_dir_for_test(dir.clone());
        configure(true, SpellLang::EnUs, true);
        assert!(flags(0, 0, "Zorblax").is_empty());

        reset_for_test();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
