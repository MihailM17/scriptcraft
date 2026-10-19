use crate::model::{ElementType, Project, ScriptBlock};
use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};
use rusqlite::{params, Connection};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum KitspError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("xml: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("missing scenario text (id=1)")]
    MissingScenario,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

const BLOCK_TAGS: &[&str] = &[
    "scene_heading",
    "scene_characters",
    "action",
    "character",
    "dialog",
    "parenthetical",
    "shot",
    "transition",
    "note",
    "lyrics",
];

/// KIT's canonical `scenario_data` keys for scene numbering
/// (KITScenaristCore `ScenarioData.cpp`).
pub const SCENE_NUMBERS_PREFIX_KEY: &str = "scene_numbers_prefix";
pub const SCENE_START_NUMBER_KEY: &str = "scene_start_number";
/// Legacy ScriptCraft 0.4.0 spelling; still read as a fallback and kept in
/// sync when present so older builds and new builds agree.
const LEGACY_SCENE_NUMBER_PREFIX_KEY: &str = "scene_number_prefix";
const LEGACY_SCENE_START_NUMBER_KEY: &str = "scene_number_start";
/// ScriptCraft extension: comma-separated frozen scene numbers, optional
/// leading `*` for "lock new scenes numbers". KIT has no such key and
/// ignores it.
pub const LOCKED_SCENE_NUMBERS_KEY: &str = "locked_scene_numbers";
/// ScriptCraft extension: inside a recovery copy, the project path the
/// session belongs to. KIT has no such key and ignores it.
pub const RECOVERY_ORIGIN_KEY: &str = "scriptcraft_recovery_origin";

/// Open a .kitsp file read-only and load scenario id=1 + title.
pub fn load_file(path: impl AsRef<Path>) -> Result<Project, KitspError> {
    let uri = format!("file:{}?mode=ro", path.as_ref().display());
    let conn = Connection::open_with_flags(
        uri,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .or_else(|_| Connection::open(path.as_ref()))?;
    load_conn(&conn)
}

fn load_conn(conn: &Connection) -> Result<Project, KitspError> {
    let title: String = conn
        .query_row(
            "SELECT data_value FROM scenario_data WHERE data_name='name'",
            params![],
            |r| r.get(0),
        )
        .unwrap_or_else(|_| "Untitled".to_string());
    let text: String = conn
        .query_row("SELECT text FROM scenario WHERE id=1", params![], |r| {
            r.get(0)
        })
        .map_err(|_| KitspError::MissingScenario)?;
    let mut project = parse_scenario(&text)?;
    project.title = title;
    Ok(project)
}

/// Serialize a project back to `<scenario>` XML (KIT shape, CDATA bodies).
pub fn project_to_scenario_xml(p: &Project) -> String {
    let mut s = String::from("<?xml version=\"1.0\"?>\n<scenario version=\"1.0\">\n");
    for b in &p.blocks {
        // CDATA cannot contain "]]>" — soften it (same rule as the Python fixer).
        let safe = b.text.replace("]]>", "]] >");
        match b.element_type {
            ElementType::SceneHeading => {
                let uuid = b
                    .kit_uuid
                    .as_deref()
                    .unwrap_or("{00000000-0000-0000-0000-000000000000}");
                s.push_str(&format!(
                    "<scene_heading uuid=\"{uuid}\">\n<v><![CDATA[{safe}]]></v>\n</scene_heading>\n"
                ));
            }
            other => {
                let tag = other.tag();
                s.push_str(&format!("<{tag}>\n<v><![CDATA[{safe}]]></v>\n</{tag}>\n"));
            }
        }
    }
    s.push_str("</scenario>\n");
    s
}

/// Rename `from` over `to`, replacing `to` if it exists.
///
/// `std::fs::rename` replaces the destination on POSIX, but fails with
/// "access denied" on Windows when the destination already exists. Windows
/// needs `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING` for the same atomic
/// swap (the project promises a save can never leave a half-written file).
#[cfg(windows)]
fn replace_file(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let wide = |p: &std::path::Path| -> Vec<u16> {
        p.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };
    let (from, to) = (wide(from), wide(to));
    let ok = unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    std::fs::rename(from, to)
}

/// Save a project into an EXISTING .kitsp file: updates scenario text and
/// syncs corkboard card titles (plus cards for new scenes). Makes a `.bak`
/// copy first, then writes a temp file next to the target and renames it
/// over the target, so a crash mid-save can never leave a half-written
/// project behind.
pub fn save_file(path: impl AsRef<Path>, p: &Project) -> Result<(), KitspError> {
    let path = path.as_ref();
    let bak = path.with_extension("kitsp.bak");
    std::fs::copy(path, &bak)?;

    let tmp = path.with_extension(format!("kitsp.tmp.{}", std::process::id()));
    let result = (|| -> Result<(), KitspError> {
        std::fs::copy(path, &tmp)?;
        let xml = project_to_scenario_xml(p);
        let conn = Connection::open(&tmp)?;
        conn.execute("UPDATE scenario SET text=?1 WHERE id=1", params![xml])?;
        // Corkboard sync must never break the save itself.
        if let Ok(scheme) =
            conn.query_row("SELECT scheme FROM scenario WHERE id=1", params![], |r| {
                r.get::<_, String>(0)
            })
        {
            let scenes: Vec<(Option<String>, String)> = p
                .scenes()
                .into_iter()
                .map(|s| (s.kit_uuid, s.heading))
                .collect();
            if let Ok(synced) = sync_scheme_cards(&scheme, &scenes) {
                let _ = conn.execute("UPDATE scenario SET scheme=?1 WHERE id=1", params![synced]);
            }
        }
        drop(conn);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&tmp)?
            .sync_all()?;
        replace_file(&tmp, path)?;
        // Best-effort: make the rename itself durable.
        if let Some(dir) = path.parent() {
            if let Ok(d) = std::fs::File::open(dir) {
                let _ = d.sync_all();
            }
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Write a standalone recovery copy of `p` at `dst` (replacing any previous
/// one), recording `origin` — the project path the session belongs to — so a
/// restored session can keep saving to the same file. `None` origin means the
/// project was never saved; the user will have to Save As.
pub fn write_recovery_file(
    dst: impl AsRef<Path>,
    origin: Option<&str>,
    p: &Project,
) -> Result<(), KitspError> {
    let dst = dst.as_ref();
    let title = if p.title.trim().is_empty() {
        "Recovered script"
    } else {
        p.title.as_str()
    };
    create_file(dst, title)?;
    if let Some(origin) = origin {
        let conn = Connection::open(dst)?;
        conn.execute(
            "INSERT INTO scenario_data (data_name, data_value) VALUES (?1, ?2)
             ON CONFLICT(data_name) DO UPDATE SET data_value=excluded.data_value",
            params![RECOVERY_ORIGIN_KEY, origin],
        )?;
    }
    save_file(dst, p)
}

/// The original project path recorded in a recovery file, if any.
pub fn recovery_origin(path: impl AsRef<Path>) -> Option<String> {
    let conn = Connection::open(path).ok()?;
    let origin = get_data(&conn, RECOVERY_ORIGIN_KEY);
    if origin.is_empty() {
        None
    } else {
        Some(origin)
    }
}

fn esc_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Sync corkboard `<card>` titles with scene headings and append cards for
/// scenes that have none. All other card attributes (position, colors,
/// folders) are preserved byte-for-byte, so KIT never loses layout.
pub fn sync_scheme_cards(
    scheme_xml: &str,
    scenes: &[(Option<String>, String)],
) -> Result<String, KitspError> {
    let mut reader = Reader::from_str(scheme_xml);
    reader.config_mut().trim_text(true);
    let mut root_attrs: Vec<(String, String)> = Vec::new();
    let mut root_name = String::from("cards");
    // (attrs in original order, id, title)
    type CardRow = (Vec<(String, String)>, String, String);
    let mut cards: Vec<CardRow> = Vec::new();

    let attr_list = |e: &quick_xml::events::BytesStart,
                     r: &Reader<&[u8]>|
     -> Vec<(String, String)> {
        let mut out = Vec::new();
        for a in e.attributes().flatten() {
            if let Ok(v) = a.decoded_and_normalized_value(XmlVersion::Implicit1_0, r.decoder()) {
                out.push((
                    String::from_utf8_lossy(a.key.as_ref()).into_owned(),
                    v.into_owned(),
                ));
            }
        }
        out
    };

    loop {
        match reader.read_event()? {
            Event::Start(e) | Event::Empty(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if name == "card" {
                    let attrs = attr_list(&e, &reader);
                    let id = attrs
                        .iter()
                        .find(|(k, _)| k == "id")
                        .map(|(_, v)| v.clone())
                        .unwrap_or_default();
                    let title = attrs
                        .iter()
                        .find(|(k, _)| k == "title")
                        .map(|(_, v)| v.clone())
                        .unwrap_or_default();
                    cards.push((attrs, id, title));
                } else if root_attrs.is_empty() {
                    root_name = name;
                    root_attrs = attr_list(&e, &reader);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if root_name != "cards" {
        return Err(KitspError::MissingScenario);
    }

    let mut next_number: i64 = cards
        .iter()
        .filter_map(|(attrs, _, _)| {
            attrs
                .iter()
                .find(|(k, _)| k == "number")
                .and_then(|(_, v)| v.parse::<i64>().ok())
        })
        .max()
        .unwrap_or(0)
        + 1;
    let mut max_y: i64 = cards
        .iter()
        .filter_map(|(attrs, _, _)| {
            attrs
                .iter()
                .find(|(k, _)| k == "y")
                .and_then(|(_, v)| v.parse::<i64>().ok())
        })
        .max()
        .unwrap_or(0);

    for (uuid, heading) in scenes {
        let Some(uuid) = uuid else { continue };
        if let Some((attrs, _, title)) = cards.iter_mut().find(|(_, id, _)| id == uuid) {
            *title = heading.clone();
            if let Some((_, v)) = attrs.iter_mut().find(|(k, _)| k == "title") {
                *v = heading.clone();
            } else {
                attrs.push(("title".to_string(), heading.clone()));
            }
            continue;
        }
        max_y += 220;
        let n = next_number;
        next_number += 1;
        cards.push((
            vec![
                ("id".to_string(), uuid.clone()),
                ("is_folder".to_string(), "false".to_string()),
                ("number".to_string(), n.to_string()),
                ("title".to_string(), heading.clone()),
                ("description".to_string(), String::new()),
                ("stamp".to_string(), String::new()),
                ("colors".to_string(), String::new()),
                ("is_embedded".to_string(), "false".to_string()),
                ("x".to_string(), "30".to_string()),
                ("y".to_string(), max_y.to_string()),
            ],
            uuid.clone(),
            heading.clone(),
        ));
    }

    // Reflect corkboard drag-reorder: when every card is scene-linked (no
    // folders), store them in scene order. Mixed boards keep file order so
    // KIT folder groupings are never scrambled.
    let scene_ids: Vec<&str> = scenes.iter().filter_map(|(id, _)| id.as_deref()).collect();
    if !scene_ids.is_empty()
        && cards
            .iter()
            .all(|(_, id, _)| scene_ids.contains(&id.as_str()))
    {
        let mut ordered = Vec::with_capacity(cards.len());
        for sid in &scene_ids {
            if let Some(pos) = cards.iter().position(|(_, id, _)| id == sid) {
                ordered.push(cards.remove(pos));
            }
        }
        ordered.append(&mut cards);
        cards = ordered;
        // Renumber sequentially so KIT keeps the visual order.
        for (i, (attrs, _, _)) in cards.iter_mut().enumerate() {
            let n = (i + 1).to_string();
            if let Some((_, v)) = attrs.iter_mut().find(|(k, _)| k == "number") {
                *v = n;
            } else {
                attrs.push(("number".to_string(), n));
            }
        }
    }

    let mut out = String::from("<?xml version=\"1.0\"?>\n<");
    out.push_str(&root_name);
    for (k, v) in &root_attrs {
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        out.push_str(&esc_attr(v));
        out.push('"');
    }
    out.push_str(">\n");
    for (attrs, _, _) in &cards {
        out.push_str("    <card");
        for (k, v) in attrs {
            out.push(' ');
            out.push_str(k);
            out.push_str("=\"");
            out.push_str(&esc_attr(v));
            out.push('"');
        }
        out.push_str("/>\n");
    }
    out.push_str("</cards>\n");
    Ok(out)
}

/// One research row (characters, locations, …) from the `research` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchEntry {
    pub id: i64,
    pub name: String,
    pub kind: ResearchKind,
    pub description: String,
    pub url: String,
    /// Original picture bytes (PNG/JPEG) as stored by KIT, if any.
    pub image: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchKind {
    Character,
    Location,
    Other,
}

impl ResearchKind {
    /// KIT research type codes (`101` = people, `201` = places).
    pub fn from_code(code: i64) -> Self {
        match code {
            101 => ResearchKind::Character,
            201 => ResearchKind::Location,
            _ => ResearchKind::Other,
        }
    }

    pub fn code(self) -> i64 {
        match self {
            ResearchKind::Character => 101,
            ResearchKind::Location => 201,
            ResearchKind::Other => 0,
        }
    }
}

/// Load research entries in sort order (read-only).
pub fn load_research(path: impl AsRef<Path>) -> Result<Vec<ResearchEntry>, KitspError> {
    let uri = format!("file:{}?mode=ro", path.as_ref().display());
    let conn = Connection::open_with_flags(
        uri,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .or_else(|_| Connection::open(path.as_ref()))?;
    let mut stmt = conn.prepare(
        "SELECT id, type, name, COALESCE(description,''), COALESCE(url,''), image FROM research ORDER BY sort_order, id",
    )?;
    let rows = stmt.query_map(params![], |r| {
        let t: i64 = r.get(1)?;
        Ok(ResearchEntry {
            id: r.get(0)?,
            name: r.get(2)?,
            kind: ResearchKind::from_code(t),
            description: r.get(3)?,
            url: r.get(4)?,
            image: r.get(5)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(KitspError::from)
}

/// Load draft versions stored inside the .kitsp (name, datetime), oldest first.
pub fn load_versions(path: impl AsRef<Path>) -> Result<Vec<(String, String)>, KitspError> {
    let uri = format!("file:{}?mode=ro", path.as_ref().display());
    let conn = Connection::open_with_flags(
        uri,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .or_else(|_| Connection::open(path.as_ref()))?;
    let mut stmt = match conn
        .prepare("SELECT COALESCE(name,''), COALESCE(datetime,'') FROM script_versions ORDER BY id")
    {
        Ok(s) => s,
        Err(_) => return Ok(Vec::new()),
    };
    let rows = stmt.query_map(params![], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(KitspError::from)
}

/// Update a research entry's editable fields.
pub fn update_research(
    path: impl AsRef<Path>,
    id: i64,
    name: &str,
    description: &str,
    url: &str,
) -> Result<(), KitspError> {
    let conn = Connection::open(path.as_ref())?;
    conn.execute(
        "UPDATE research SET name=?1, description=?2, url=?3 WHERE id=?4",
        params![name, description, url, id],
    )?;
    Ok(())
}

/// Set (or clear with `None`) a research entry's picture bytes.
pub fn update_research_image(
    path: impl AsRef<Path>,
    id: i64,
    image: Option<&[u8]>,
) -> Result<(), KitspError> {
    let conn = Connection::open(path.as_ref())?;
    conn.execute(
        "UPDATE research SET image=?1 WHERE id=?2",
        params![image, id],
    )?;
    Ok(())
}

/// Insert a research entry, returning its new id.
pub fn insert_research(
    path: impl AsRef<Path>,
    kind: ResearchKind,
    name: &str,
) -> Result<i64, KitspError> {
    let conn = Connection::open(path.as_ref())?;
    let order: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM research",
            params![],
            |r| r.get(0),
        )
        .unwrap_or(0);
    conn.execute(
        "INSERT INTO research (parent_id, type, name, description, url, sort_order) VALUES (NULL, ?1, ?2, '', '', ?3)",
        params![kind.code(), name, order],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Delete a research entry.
pub fn delete_research(path: impl AsRef<Path>, id: i64) -> Result<(), KitspError> {
    let conn = Connection::open(path.as_ref())?;
    conn.execute("DELETE FROM research WHERE id=?1", params![id])?;
    Ok(())
}

/// Create a fresh `.kitsp` file with the KIT table layout (minimal seed:
/// one scene, standard transitions/places/times) and return the project.
/// Existing file at `path` is replaced.
pub fn create_file(path: impl AsRef<Path>, title: &str) -> Result<Project, KitspError> {
    let path = path.as_ref();
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "CREATE TABLE _database_history ( id TEXT PRIMARY KEY, query TEXT NOT NULL, query_values TEXT NOT NULL, username TEXT NOT NULL, datetime TEXT NOT NULL );
         CREATE TABLE system_variables ( variable TEXT PRIMARY KEY ON CONFLICT REPLACE, value TEXT NOT NULL );
         CREATE TABLE places ( id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT UNIQUE NOT NULL );
         CREATE TABLE scenary_days ( id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT UNIQUE NOT NULL );
         CREATE TABLE times ( id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT UNIQUE NOT NULL );
         CREATE TABLE character_states ( id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT UNIQUE NOT NULL );
         CREATE TABLE transitions ( id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT UNIQUE NOT NULL );
         CREATE TABLE scenario ( id INTEGER PRIMARY KEY AUTOINCREMENT, scheme TEXT NOT NULL, text TEXT NOT NULL, is_draft INTEGER NOT NULL DEFAULT(0) );
         CREATE TABLE scenario_changes (id INTEGER PRIMARY KEY AUTOINCREMENT, uuid TEXT NOT NULL, datetime TEXT NOT NULL, username TEXT NOT NULL, undo_patch TEXT NOT NULL, redo_patch TEXT NOT NULL, is_draft INTEGER NOT NULL DEFAULT(0) );
         CREATE TABLE scenario_data (id INTEGER PRIMARY KEY AUTOINCREMENT, data_name TEXT NOT NULL UNIQUE, data_value TEXT DEFAULT(NULL) );
         CREATE TABLE research (id INTEGER PRIMARY KEY AUTOINCREMENT, parent_id INTEGER DEFAULT(NULL), type INTEGER NOT NULL DEFAULT(0), name TEXT NOT NULL, description TEXT DEFAULT(NULL), url TEXT DEFAULT(NULL), image BLOB DEFAULT(NULL), color TEXT DEFAULT(NULL), sort_order INTEGER NOT NULL DEFAULT(0) );
         CREATE TABLE script_versions ( id INTEGER PRIMARY KEY AUTOINCREMENT, fk_script_id INTEGER NOT NULL, username TEXT NOT NULL, datetime TEXT NOT NULL, color TEXT DEFAULT(NULL), name TEXT UNIQUE NOT NULL, description TEXT DEFAULT(NULL), script_text TEXT NOT NULL);",
    )?;
    for name in [
        "CUT TO:",
        "FADE IN:",
        "FADE OUT:",
        "FADE TO:",
        "DISSOLVE TO:",
        "BACK TO:",
        "MATCH CUT TO:",
        "JUMP CUT TO:",
        "FADE TO BLACK",
    ] {
        conn.execute("INSERT INTO transitions (name) VALUES (?1)", params![name])?;
    }
    for name in ["Voice Over", "Off Screen", "Off Camera", "SUBTITLE"] {
        conn.execute(
            "INSERT INTO character_states (name) VALUES (?1)",
            params![name],
        )?;
    }
    for name in [
        "DAY",
        "NIGHT",
        "MORNING",
        "AFTERNOON",
        "EVENING",
        "LATER",
        "MOMENTS LATER",
        "CONTINUOUS",
        "THE NEXT DAY",
    ] {
        conn.execute("INSERT INTO times (name) VALUES (?1)", params![name])?;
    }
    for name in ["INT", "EXT", "INT./EXT"] {
        conn.execute("INSERT INTO places (name) VALUES (?1)", params![name])?;
    }
    conn.execute(
        "INSERT INTO system_variables (variable, value) VALUES ('application-version', 'ScriptCraft')",
        params![],
    )?;

    let heading = "INT. NEW SCENE - DAY";
    let uuid = format!("{{{}}}", uuid::Uuid::new_v4());
    let scheme = format!(
        "<?xml version=\"1.0\"?>\n<cards x=\"0\" y=\"0\" width=\"5200\" height=\"3780\" scale=\"1\" scroll_x=\"30\" scroll_y=\"30\">\n    <card id=\"{uuid}\" is_folder=\"false\" number=\"1\" title=\"{heading}\" description=\"\" stamp=\"\" colors=\"\" is_embedded=\"false\" x=\"30\" y=\"30\"/>\n</cards>"
    );
    let mut first = crate::model::ScriptBlock::new(
        crate::model::ElementType::SceneHeading,
        heading.to_string(),
    );
    first.kit_uuid = Some(uuid);
    let seed = Project {
        title: title.to_string(),
        blocks: vec![first],
    };
    let text = project_to_scenario_xml(&seed);
    conn.execute(
        "INSERT INTO scenario (scheme, text, is_draft) VALUES (?1, ?2, 0)",
        params![scheme, text],
    )?;
    for (k, v) in [
        ("name", title),
        ("genre", ""),
        ("author", ""),
        ("director", ""),
        ("contacts", ""),
        ("year", ""),
        ("logline", ""),
        ("synopsis", ""),
        ("header", ""),
        ("footer", ""),
        ("additional_info", ""),
        (SCENE_NUMBERS_PREFIX_KEY, ""),
        (SCENE_START_NUMBER_KEY, "1"),
        (LOCKED_SCENE_NUMBERS_KEY, ""),
    ] {
        conn.execute(
            "INSERT INTO scenario_data (data_name, data_value) VALUES (?1, ?2)",
            params![k, v],
        )?;
    }
    drop(conn);
    load_file(path)
}

/// Title-page fields from `scenario_data`. `director` is a ScriptCraft
/// extension (plain TEXT key, ignored by KIT); everything else mirrors
/// KIT's own title page data. Scene numbering maps to KIT's
/// `scene_numbers_prefix` / `scene_start_number` keys; `locked_scene_numbers`
/// is a ScriptCraft extension KIT ignores.
#[derive(Debug, Clone, Default)]
pub struct Metadata {
    pub title: String,
    pub genre: String,
    pub author: String,
    pub director: String,
    pub contacts: String,
    pub year: String,
    pub additional_info: String,
    pub logline_html: String,
    pub synopsis_html: String,
    pub header: String,
    pub footer: String,
    /// Stored as KIT `scene_numbers_prefix`.
    pub scene_number_prefix: String,
    /// Stored as KIT `scene_start_number`.
    pub scene_number_start: String,
    /// Comma-separated frozen scene numbers; a leading `*` marks
    /// "lock new scenes numbers" (future scenes keep their number).
    pub locked_scene_numbers: String,
}

fn get_data(conn: &Connection, key: &str) -> String {
    conn.query_row(
        "SELECT COALESCE(data_value,'') FROM scenario_data WHERE data_name=?1",
        params![key],
        |r| r.get(0),
    )
    .unwrap_or_default()
}

/// First non-empty value among `keys`, for keys that were renamed over time.
fn get_data_alias(conn: &Connection, keys: &[&str]) -> String {
    for key in keys {
        let value = get_data(conn, key);
        if !value.is_empty() {
            return value;
        }
    }
    String::new()
}

/// Load title-page metadata (any .kitsp path, read-only).
pub fn load_metadata(path: impl AsRef<Path>) -> Result<Metadata, KitspError> {
    let uri = format!("file:{}?mode=ro", path.as_ref().display());
    let conn = Connection::open_with_flags(
        uri,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .or_else(|_| Connection::open(path.as_ref()))?;
    Ok(Metadata {
        title: get_data(&conn, "name"),
        genre: get_data(&conn, "genre"),
        author: get_data(&conn, "author"),
        director: get_data(&conn, "director"),
        contacts: get_data(&conn, "contacts"),
        year: get_data(&conn, "year"),
        additional_info: get_data(&conn, "additional_info"),
        logline_html: get_data(&conn, "logline"),
        synopsis_html: get_data(&conn, "synopsis"),
        header: get_data(&conn, "header"),
        footer: get_data(&conn, "footer"),
        scene_number_prefix: get_data_alias(
            &conn,
            &[SCENE_NUMBERS_PREFIX_KEY, LEGACY_SCENE_NUMBER_PREFIX_KEY],
        ),
        scene_number_start: get_data_alias(
            &conn,
            &[SCENE_START_NUMBER_KEY, LEGACY_SCENE_START_NUMBER_KEY],
        ),
        locked_scene_numbers: get_data(&conn, LOCKED_SCENE_NUMBERS_KEY),
    })
}

/// Save all title-page fields (logline/synopsis stored as rich HTML).
/// Scene numbering is written under KIT's canonical keys; legacy ScriptCraft
/// rows are updated too when they already exist, but never created.
pub fn save_metadata(path: impl AsRef<Path>, m: &Metadata) -> Result<(), KitspError> {
    let conn = Connection::open(path.as_ref())?;
    let upsert = |k: &str, v: &str| -> Result<(), KitspError> {
        conn.execute(
            "INSERT INTO scenario_data (data_name, data_value) VALUES (?1, ?2)
             ON CONFLICT(data_name) DO UPDATE SET data_value=excluded.data_value",
            params![k, v],
        )?;
        Ok(())
    };
    for (k, v) in [
        ("name", &m.title),
        ("genre", &m.genre),
        ("author", &m.author),
        ("director", &m.director),
        ("contacts", &m.contacts),
        ("year", &m.year),
        ("additional_info", &m.additional_info),
        ("logline", &m.logline_html),
        ("synopsis", &m.synopsis_html),
        ("header", &m.header),
        ("footer", &m.footer),
        (SCENE_NUMBERS_PREFIX_KEY, &m.scene_number_prefix),
        (SCENE_START_NUMBER_KEY, &m.scene_number_start),
        (LOCKED_SCENE_NUMBERS_KEY, &m.locked_scene_numbers),
    ] {
        upsert(k, v)?;
    }
    for (legacy, value) in [
        (LEGACY_SCENE_NUMBER_PREFIX_KEY, &m.scene_number_prefix),
        (LEGACY_SCENE_START_NUMBER_KEY, &m.scene_number_start),
    ] {
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM scenario_data WHERE data_name=?1)",
                params![legacy],
                |r| r.get(0),
            )
            .unwrap_or(false);
        if exists {
            upsert(legacy, value)?;
        }
    }
    Ok(())
}

/// Strip rich-text tags for read-only preview (`<p style=...>x</p>` → `x`).
pub fn strip_html(html: &str) -> String {
    html_to_text(html)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Convert KIT rich-text HTML to editable plain text: `</p>`/`<br>` become
/// newlines, remaining tags are stripped, basic entities unescaped.
pub fn html_to_text(html: &str) -> String {
    let mut s = html.replace("</p>", "\n");
    s = s.replace("<br>", "\n").replace("<br/>", "\n");
    let mut out = String::new();
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    let out = out
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&");
    // Collapse runs of blank lines to one, trim blank edges.
    let mut lines: Vec<&str> = Vec::new();
    let mut blanks = 0usize;
    for line in out.lines() {
        let t = line.trim();
        if t.is_empty() {
            blanks += 1;
            if blanks <= 1 {
                lines.push("");
            }
        } else {
            blanks = 0;
            lines.push(t);
        }
    }
    while lines.first() == Some(&"") {
        lines.remove(0);
    }
    while lines.last() == Some(&"") {
        lines.pop();
    }
    lines.join("\n")
}

/// KIT-style paragraph style used for stored title-page rich text.
pub const KIT_PARAGRAPH_OPEN: &str = "<p style=\" margin-top:0px; margin-bottom:14px; margin-left:0px; margin-right:0px; -qt-block-indent:0; text-indent:0px;\">";

/// Convert edited plain text back to KIT rich-text HTML (blank-line
/// separated paragraphs become `<p>` blocks, special chars escaped).
pub fn text_to_html(text: &str) -> String {
    let mut paras: Vec<String> = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    for line in text.lines().chain(std::iter::once("")) {
        if line.trim().is_empty() {
            if !cur.is_empty() {
                paras.push(cur.join(" "));
                cur.clear();
            }
        } else {
            cur.push(line.trim());
        }
    }
    paras
        .into_iter()
        .map(|p| {
            let e = p
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            format!("{KIT_PARAGRAPH_OPEN}{e}</p>")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Guess location names from scene headings (`INT. X - DAY` → `X`).
/// Headings without a parseable location are skipped. Order of first
/// appearance, deduplicated (case-insensitive).
pub fn extract_locations(blocks: &[crate::model::ScriptBlock]) -> Vec<String> {
    use crate::model::ElementType;
    let mut out: Vec<String> = Vec::new();
    for b in blocks {
        if b.element_type != ElementType::SceneHeading {
            continue;
        }
        let t = b.text.trim();
        // Strip INT./EXT. prefix.
        let after_dot = match t.find(". ") {
            Some(i) => &t[i + 2..],
            None => continue,
        };
        // Strip trailing " - TIME".
        let loc = match after_dot.rfind(" - ") {
            Some(i) => after_dot[..i].trim(),
            None => after_dot.trim(),
        };
        if loc.is_empty() {
            continue;
        }
        if !out.iter().any(|e: &String| e.eq_ignore_ascii_case(loc)) {
            out.push(loc.to_string());
        }
    }
    out
}

/// One corkboard card from `scenario.scheme`.
#[derive(Debug, Clone)]
pub struct Card {
    pub id: String,
    pub title: String,
    pub description: String,
}

/// Load corkboard cards in order (any .kitsp path, read-only).
pub fn load_cards(path: impl AsRef<Path>) -> Result<Vec<Card>, KitspError> {
    let uri = format!("file:{}?mode=ro", path.as_ref().display());
    let conn = Connection::open_with_flags(
        uri,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .or_else(|_| Connection::open(path.as_ref()))?;
    let scheme: String = conn
        .query_row("SELECT scheme FROM scenario WHERE id=1", params![], |r| {
            r.get(0)
        })
        .map_err(|_| KitspError::MissingScenario)?;
    parse_cards(&scheme)
}

fn parse_cards(xml: &str) -> Result<Vec<Card>, KitspError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut cards = Vec::new();

    loop {
        match reader.read_event()? {
            Event::Start(e) | Event::Empty(e) => {
                if e.name().as_ref() == b"card" {
                    // Inline (no closure): each borrow of `reader` ends
                    // before the next `read_event`.
                    let mut id = String::new();
                    let mut title = String::new();
                    let mut description = String::new();
                    for key in ["id", "title", "description"] {
                        if let Ok(Some(a)) = e.try_get_attribute(key) {
                            if let Ok(v) = a.decoded_and_normalized_value(
                                XmlVersion::Implicit1_0,
                                reader.decoder(),
                            ) {
                                match key {
                                    "id" => id = v.into_owned(),
                                    "title" => title = v.into_owned(),
                                    _ => description = v.into_owned(),
                                }
                            }
                        }
                    }
                    cards.push(Card {
                        id,
                        title,
                        description,
                    });
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(cards)
}
pub fn parse_scenario(xml: &str) -> Result<Project, KitspError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut blocks: Vec<ScriptBlock> = Vec::new();
    let mut current_tag: Option<String> = None;
    let mut current_uuid: Option<String> = None;
    let mut inside_v = false;
    let mut text_buf = String::new();

    loop {
        match reader.read_event()? {
            Event::Start(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if BLOCK_TAGS.contains(&name.as_str()) {
                    current_tag = Some(name.clone());
                    current_uuid = if name == "scene_heading" {
                        e.try_get_attribute("uuid")
                            .ok()
                            .flatten()
                            .and_then(|a| {
                                a.decoded_and_normalized_value(
                                    XmlVersion::Implicit1_0,
                                    reader.decoder(),
                                )
                                .ok()
                            })
                            .map(|v| v.into_owned())
                    } else {
                        None
                    };
                    text_buf.clear();
                } else if name == "v" && current_tag.is_some() {
                    inside_v = true;
                    text_buf.clear();
                }
            }
            Event::Empty(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if name == "v" && current_tag.is_some() {
                    // empty <v/> — keep empty text
                }
            }
            Event::Text(e) => {
                if inside_v && current_tag.is_some() {
                    let s = reader.decoder().decode(e.as_ref()).unwrap_or_default();
                    text_buf.push_str(&s);
                }
            }
            Event::CData(e) => {
                if inside_v && current_tag.is_some() {
                    // CData content is raw; decode as UTF-8 lossy
                    text_buf.push_str(&String::from_utf8_lossy(&e.into_inner()));
                }
            }
            Event::End(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if name == "v" {
                    inside_v = false;
                } else if Some(name.as_str()) == current_tag.as_deref() {
                    if let Some(tag) = current_tag.take() {
                        if let Some(el) = ElementType::from_tag(&tag) {
                            let mut b = ScriptBlock::new(el, text_buf.trim().to_string());
                            b.kit_uuid = current_uuid.take();
                            blocks.push(b);
                        }
                    }
                    text_buf.clear();
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    Ok(Project {
        title: String::new(),
        blocks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_scenario() {
        let xml = r#"<?xml version="1.0"?>
<scenario version="1.0">
<scene_heading uuid="{abc}"><v><![CDATA[EXT. PARK - DAY]]></v></scene_heading>
<action><v><![CDATA[A man walks.]]></v></action>
<character><v><![CDATA[IVAN]]></v></character>
<dialog><v><![CDATA[Hello.]]></v></dialog>
</scenario>"#;
        let p = parse_scenario(xml).unwrap();
        assert_eq!(p.blocks.len(), 4);
        assert_eq!(p.blocks[0].text, "EXT. PARK - DAY");
        assert_eq!(p.blocks[0].kit_uuid.as_deref(), Some("{abc}"));
        assert_eq!(p.scenes().len(), 1);
        assert_eq!(p.characters(), vec!["IVAN".to_string()]);
    }

    #[test]
    fn sync_scheme_updates_titles_and_appends() {
        let scheme = r#"<?xml version="1.0"?>
<cards x="0" y="0" width="5200" height="3780" scale="1" scroll_x="30" scroll_y="30">
    <card id="{a}" is_folder="false" number="1" title="OLD TITLE" description="d" stamp="" colors="" is_embedded="false" x="30" y="30"/>
    <card id="{folder}" is_folder="true" number="2" title="Act One" description="" stamp="" colors="" is_embedded="false" x="400" y="30"/>
</cards>"#;
        let scenes = vec![
            (Some("{a}".to_string()), "NEW TITLE".to_string()),
            (Some("{b}".to_string()), "INT. NEW - DAY".to_string()),
        ];
        let out = sync_scheme_cards(scheme, &scenes).unwrap();
        // Title updated in place, position attrs preserved.
        assert!(out.contains(r#"id="{a}""#));
        assert!(out.contains(r#"title="NEW TITLE""#));
        assert!(out.contains(r#"x="30" y="30""#));
        assert!(out.contains(r#"description="d""#));
        // Untouched folder card survives verbatim.
        assert!(out.contains(r#"id="{folder}""#));
        assert!(out.contains(r#"title="Act One""#));
        // Missing scene appended with next number.
        assert!(out.contains(r#"id="{b}""#));
        assert!(out.contains(r#"number="3""#));
        // Still parses as cards.
        let reparsed = parse_cards(&out).unwrap();
        assert_eq!(reparsed.len(), 3);
        assert_eq!(reparsed[0].title, "NEW TITLE");
        assert_eq!(reparsed[2].title, "INT. NEW - DAY");
    }

    #[test]
    fn sync_scheme_reorders_scene_cards() {
        let scheme = r#"<?xml version="1.0"?>
<cards x="0" y="0" width="100" height="100" scale="1" scroll_x="0" scroll_y="0">
    <card id="{a}" is_folder="false" number="1" title="AAA" description="" stamp="" colors="" is_embedded="false" x="30" y="30"/>
    <card id="{b}" is_folder="false" number="2" title="BBB" description="" stamp="" colors="" is_embedded="false" x="300" y="30"/>
</cards>"#;
        // Scenes reordered: b first.
        let scenes = vec![
            (Some("{b}".to_string()), "BBB".to_string()),
            (Some("{a}".to_string()), "AAA".to_string()),
        ];
        let out = sync_scheme_cards(scheme, &scenes).unwrap();
        let reparsed = parse_cards(&out).unwrap();
        assert_eq!(reparsed[0].id, "{b}");
        assert_eq!(reparsed[1].id, "{a}");
        // Numbers follow the new order; positions untouched.
        assert!(out.contains(r#"id="{b}" is_folder="false" number="1""#));
        assert!(out.contains(r#"id="{a}" is_folder="false" number="2""#));
        assert!(out.contains(r#"x="300" y="30""#));
    }

    #[test]
    fn roundtrip_preserves_blocks() {
        let xml = r#"<?xml version="1.0"?>
<scenario version="1.0">
<scene_heading uuid="{abc}"><v><![CDATA[EXT. PARK - DAY]]></v></scene_heading>
<action><v><![CDATA[A man walks.]]></v></action>
<character><v><![CDATA[IVAN]]></v></character>
<parenthetical><v><![CDATA[(softly)]]></v></parenthetical>
<dialog><v><![CDATA[Hello.]]></v></dialog>
<transition><v><![CDATA[FADE OUT:]]></v></transition>
</scenario>"#;
        let p = parse_scenario(xml).unwrap();
        let out = project_to_scenario_xml(&p);
        let p2 = parse_scenario(&out).unwrap();
        assert_eq!(p.blocks.len(), p2.blocks.len());
        for (a, b) in p.blocks.iter().zip(p2.blocks.iter()) {
            assert_eq!(a.element_type, b.element_type);
            assert_eq!(a.text, b.text);
            assert_eq!(a.kit_uuid, b.kit_uuid);
        }
    }
}
