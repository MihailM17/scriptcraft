use scriptcraft_core::revision::{RevisionColor, RevisionHistory};
use scriptcraft_core::{format, kitsp};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/A95.kitsp");

#[test]
fn a95_loads_with_expected_structure() {
    let p = kitsp::load_file(FIXTURE).expect("fixture must load");
    assert_eq!(p.title, "A95");
    assert_eq!(p.scenes().len(), 21, "must have 21 scenes");
    assert_eq!(p.blocks.len(), 352, "fixed script has 352 blocks");
    assert_eq!(p.characters().len(), 9, "9 unique character cues");
}

#[test]
fn a95_passes_format_validation() {
    let p = kitsp::load_file(FIXTURE).expect("fixture must load");
    let issues = format::validate(&p);
    assert!(
        issues.is_empty(),
        "expected zero format issues, got: {:?}",
        &issues[..issues.len().min(10)]
    );
}

#[test]
fn a95_stats_sane() {
    let p = kitsp::load_file(FIXTURE).expect("fixture must load");
    let s = p.stats();
    assert!(s.words > 4000 && s.words < 6000, "words={}", s.words);
    assert!((s.est_pages - s.words as f32 / 180.0).abs() < 0.01);
}

#[test]
fn a95_fountain_export_sane() {
    let p = kitsp::load_file(FIXTURE).expect("fixture must load");
    let meta = kitsp::load_metadata(FIXTURE).expect("metadata must load");
    assert!(!meta.author.is_empty());
    let f = scriptcraft_core::fountain::to_fountain(&p, &meta);
    assert!(f.starts_with("Title: A95\n"));
    assert!(f.contains(&format!("Author: {}\n", meta.author)));
    assert!(f.contains("EXT. ПАРКИНГ ПРЕД ПАНЕЛЕН БЛОК - MORNING\n"));
    assert!(f.contains("> FADE TO BLACK\n"));
    // Round-trips back through the importer.
    let (q, fm) = scriptcraft_core::fountain::parse_fountain(&f);
    assert_eq!(fm.title, "A95");
    assert_eq!(fm.author, meta.author);
    assert_eq!(q.blocks.len(), p.blocks.len());
}

#[test]
fn create_file_makes_loadable_project() {
    let tmp = std::env::temp_dir().join("scriptcraft_test_new.kitsp");
    let _ = std::fs::remove_file(&tmp);
    let p = kitsp::create_file(&tmp, "Fresh").expect("create");
    assert_eq!(p.title, "Fresh");
    assert_eq!(p.scenes().len(), 1);
    let cards = kitsp::load_cards(&tmp).expect("cards");
    assert_eq!(cards.len(), 1);
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn a95_fountain_export_import_is_stable() {
    use scriptcraft_core::fountain;
    let p = kitsp::load_file(FIXTURE).expect("fixture must load");
    let meta0 = kitsp::load_metadata(FIXTURE).expect("metadata must load");
    let f1 = fountain::to_fountain(&p, &meta0);
    let (q, meta) = fountain::parse_fountain(&f1);
    assert_eq!(meta.title, "A95");
    // Re-export must be byte-identical: nothing lost or reinterpreted.
    let mut meta1 = meta0.clone();
    meta1.title = q.title.clone();
    let f2 = fountain::to_fountain(&q, &meta1);
    assert_eq!(f1, f2, "fountain round-trip not stable");
    assert_eq!(q.scenes().len(), 21);
}

#[test]
fn a95_cards_match_scenes() {
    let cards = kitsp::load_cards(FIXTURE).expect("cards must load");
    assert_eq!(cards.len(), 21, "one card per scene, got {}", cards.len());
    let p = kitsp::load_file(FIXTURE).expect("fixture must load");
    let scenes = p.scenes();
    // Card order follows scene order; titles match normalized headings.
    for (card, scene) in cards.iter().zip(scenes.iter()) {
        assert_eq!(card.title, scene.heading, "card title mismatch");
    }
}

#[test]
fn a95_save_persists_new_scene_card() {
    use scriptcraft_core::model::{ElementType, ScriptBlock};
    let tmp = std::env::temp_dir().join("scriptcraft_test_newcard.kitsp");
    std::fs::copy(FIXTURE, &tmp).expect("copy fixture");
    let mut p = kitsp::load_file(&tmp).expect("load copy");
    let n_before = kitsp::load_cards(&tmp).expect("cards").len();
    // Simulate "insert scene below": heading at end + save.
    let mut b = ScriptBlock::new(ElementType::SceneHeading, "INT. ADDED - DAY".into());
    b.kit_uuid = Some("{added-scene-uuid}".to_string());
    p.blocks.push(b);
    kitsp::save_file(&tmp, &p).expect("save back");
    let cards = kitsp::load_cards(&tmp).expect("reload cards");
    assert_eq!(cards.len(), n_before + 1, "new scene card persisted");
    assert!(cards.iter().any(|c| c.title == "INT. ADDED - DAY"));
    let _ = std::fs::remove_file(&tmp);
    let _ = std::fs::remove_file(tmp.with_extension("kitsp.bak"));
}

#[test]
fn a95_research_crud_roundtrip() {
    use scriptcraft_core::kitsp::{
        delete_research, insert_research, load_research, update_research, ResearchKind,
    };
    let tmp = std::env::temp_dir().join("scriptcraft_test_research.kitsp");
    std::fs::copy(FIXTURE, &tmp).expect("copy fixture");
    let before = load_research(&tmp).expect("load research");
    // Insert a character + location, verify they come back.
    let cid = insert_research(&tmp, ResearchKind::Character, "NEWBIE").expect("insert char");
    insert_research(&tmp, ResearchKind::Location, "BIKE SHED").expect("insert loc");
    let after = load_research(&tmp).expect("reload research");
    assert_eq!(after.len(), before.len() + 2);
    // Update the character's name/description/url.
    update_research(
        &tmp,
        cid,
        "NEWBIE RENAMED",
        "A test person.",
        "http://example.com",
    )
    .expect("update");
    let edited = load_research(&tmp).expect("reload after update");
    let found = edited
        .iter()
        .find(|e| e.id == cid)
        .expect("edited entry present");
    assert_eq!(found.name, "NEWBIE RENAMED");
    assert!(found.description.contains("test person"));
    assert_eq!(found.url, "http://example.com");
    // Delete it again.
    delete_research(&tmp, cid).expect("delete");
    let gone = load_research(&tmp).expect("reload after delete");
    assert!(!gone.iter().any(|e| e.id == cid));
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn a95_research_lists_cast_and_places() {
    use scriptcraft_core::kitsp::{load_research, ResearchKind};
    let r = load_research(FIXTURE).expect("research must load");
    let chars: Vec<&str> = r
        .iter()
        .filter(|e| e.kind == ResearchKind::Character)
        .map(|e| e.name.as_str())
        .collect();
    assert!(chars.contains(&"ИВАН"));
    assert!(chars.contains(&"МАРИЯ"));
    assert!(chars.contains(&"ГЕОРГИ"));
    assert!(r.iter().any(|e| e.kind == ResearchKind::Location));
}

#[test]
fn extract_locations_parses_headings() {
    use scriptcraft_core::kitsp::extract_locations;
    let p = kitsp::load_file(FIXTURE).expect("load fixture");
    let locs = extract_locations(&p.blocks);
    assert!(!locs.is_empty());
    // No time-of-day suffixes leak into locations; no duplicates.
    for l in &locs {
        assert!(!l.contains(" - "));
    }
    let mut seen = std::collections::HashSet::new();
    for l in &locs {
        assert!(seen.insert(l.to_lowercase()), "duplicate location: {l}");
    }
}

#[test]
fn a95_metadata_roundtrip() {
    let tmp = std::env::temp_dir().join("scriptcraft_test_meta.kitsp");
    std::fs::copy(FIXTURE, &tmp).expect("copy fixture");
    let mut m = kitsp::load_metadata(&tmp).expect("load metadata");
    assert_eq!(m.title, "A95");
    assert!(!m.synopsis_html.is_empty());
    // Plain-text preview strips tags.
    assert!(!kitsp::strip_html(&m.synopsis_html).contains('<'));
    m.genre = "Thriller/Horror".to_string();
    m.year = "2026".to_string();
    m.director = "Test Director".to_string();
    m.logline_html = kitsp::text_to_html("Logline here.");
    m.synopsis_html = kitsp::text_to_html("Para one.\n\nPara two.");
    kitsp::save_metadata(&tmp, &m).expect("save metadata");
    let mut m2 = kitsp::load_metadata(&tmp).expect("reload metadata");
    assert_eq!(m2.genre, "Thriller/Horror");
    assert_eq!(m2.year, "2026");
    assert_eq!(m2.director, "Test Director");
    assert_eq!(m2.logline_html, m.logline_html);
    assert_eq!(m2.synopsis_html, m.synopsis_html);
    // HTML round-trips back to the edited plain text.
    assert_eq!(
        kitsp::html_to_text(&m2.synopsis_html),
        "Para one.\n\nPara two."
    );
    // Scene numbering round-trips through KIT's canonical keys.
    m2.scene_number_prefix = "SC".to_string();
    m2.scene_number_start = "5".to_string();
    kitsp::save_metadata(&tmp, &m2).expect("save numbering");
    let conn = rusqlite::Connection::open(&tmp).unwrap();
    assert_eq!(
        scenario_data_value(&conn, "scene_numbers_prefix").as_deref(),
        Some("SC")
    );
    assert_eq!(
        scenario_data_value(&conn, "scene_start_number").as_deref(),
        Some("5")
    );
    let m3 = kitsp::load_metadata(&tmp).expect("reload metadata");
    assert_eq!(m3.scene_number_prefix, "SC");
    assert_eq!(m3.scene_number_start, "5");
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn a95_save_reload_roundtrip() {
    // Copy the fixture to temp (never write into fixtures/) then save+reload.
    let tmp = std::env::temp_dir().join("scriptcraft_test_a95.kitsp");
    std::fs::copy(FIXTURE, &tmp).expect("copy fixture");
    let p = kitsp::load_file(&tmp).expect("load copy");
    kitsp::save_file(&tmp, &p).expect("save back");
    let q = kitsp::load_file(&tmp).expect("reload");
    assert_eq!(p.blocks.len(), q.blocks.len());
    for (a, b) in p.blocks.iter().zip(q.blocks.iter()) {
        assert_eq!(a.id, b.id, "block IDs survive save/reopen");
        assert_eq!(a.element_type, b.element_type);
        assert_eq!(a.text, b.text);
        assert_eq!(a.kit_uuid, b.kit_uuid);
    }
    assert_eq!(
        p.scenes().iter().map(|s| s.id).collect::<Vec<_>>(),
        q.scenes().iter().map(|s| s.id).collect::<Vec<_>>(),
        "scene IDs are derived from stable heading IDs"
    );
    assert!(format::validate(&q).is_empty());
    let _ = std::fs::remove_file(&tmp);
    let _ = std::fs::remove_file(tmp.with_extension("kitsp.bak"));
}

#[test]
fn research_image_roundtrip() {
    let tmp = std::env::temp_dir().join("scriptcraft_test_image.kitsp");
    std::fs::copy(FIXTURE, &tmp).expect("copy fixture");
    let entries = kitsp::load_research(&tmp).expect("load research");
    let id = entries[0].id;
    assert!(
        entries[0].image.is_none(),
        "fixture starts without pictures"
    );

    let png = b"\x89PNG\r\n\x1a\nfake-image-bytes";
    kitsp::update_research_image(&tmp, id, Some(png)).expect("set image");
    let entries = kitsp::load_research(&tmp).expect("reload");
    assert_eq!(entries[0].image.as_deref(), Some(png.as_slice()));

    kitsp::update_research_image(&tmp, id, None).expect("clear image");
    let entries = kitsp::load_research(&tmp).expect("reload");
    assert!(entries[0].image.is_none());
    let _ = std::fs::remove_file(&tmp);
}

/// Deterministic dump of every row of a query, for before/after comparison.
/// BLOBs are hex-encoded, so a single changed byte cannot hide.
fn dump_sql(conn: &rusqlite::Connection, sql: &str) -> Vec<String> {
    use rusqlite::types::Value;
    let mut stmt = conn.prepare(sql).expect("prepare dump");
    let cols = stmt.column_count();
    let mut rows = stmt.query([]).expect("query dump");
    let mut out = Vec::new();
    while let Some(row) = rows.next().expect("row") {
        let mut vals = Vec::new();
        for i in 0..cols {
            let v: Value = row.get(i).expect("col");
            vals.push(match v {
                Value::Null => "NULL".to_string(),
                Value::Integer(i) => format!("int:{i}"),
                Value::Real(r) => format!("real:{r:?}"),
                Value::Text(t) => format!("text:{t}"),
                Value::Blob(b) => format!(
                    "blob:{}",
                    b.iter().map(|x| format!("{x:02x}")).collect::<String>()
                ),
            });
        }
        out.push(vals.join("\u{1f}"));
    }
    out
}

fn dump_table(conn: &rusqlite::Connection, table: &str) -> Vec<String> {
    dump_sql(conn, &format!("SELECT * FROM {table}"))
}

fn is_scriptcraft_sidecar_row(row: &str) -> bool {
    let name = row.split('\u{1f}').nth(1).unwrap_or_default();
    let name = name.strip_prefix("text:").unwrap_or(name);
    name == kitsp::IDENTITY_KEY || name == kitsp::SCENE_META_KEY
}

fn scenario_data_value(conn: &rusqlite::Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT data_value FROM scenario_data WHERE data_name=?1",
        rusqlite::params![key],
        |r| r.get(0),
    )
    .ok()
    .flatten()
}

/// KIT stores scene numbering in `scenario_data` under these keys
/// (`KITScenaristCore/Domain/ScenarioData.cpp`): `scene_numbers_prefix`,
/// `scene_start_number`. Loading must read them, not the legacy spelling.
#[test]
fn metadata_reads_kit_scene_number_keys() {
    let tmp = std::env::temp_dir().join("scriptcraft_test_kit_keys.kitsp");
    std::fs::copy(FIXTURE, &tmp).expect("copy fixture");
    {
        let conn = rusqlite::Connection::open(&tmp).unwrap();
        conn.execute(
            "UPDATE scenario_data SET data_value='SC' WHERE data_name='scene_numbers_prefix'",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE scenario_data SET data_value='3' WHERE data_name='scene_start_number'",
            [],
        )
        .unwrap();
    }
    let m = kitsp::load_metadata(&tmp).expect("load metadata");
    assert_eq!(m.scene_number_prefix, "SC");
    assert_eq!(m.scene_number_start, "3");
    let _ = std::fs::remove_file(&tmp);
}

/// Older ScriptCraft builds wrote the singular spelling; those projects must
/// still load, and saving must not let the two spellings diverge.
#[test]
fn metadata_reads_and_migrates_legacy_scene_number_keys() {
    let tmp = std::env::temp_dir().join("scriptcraft_test_legacy_keys.kitsp");
    let _ = std::fs::remove_file(&tmp);
    kitsp::create_file(&tmp, "Legacy").expect("create");
    {
        let conn = rusqlite::Connection::open(&tmp).unwrap();
        // Emulate an old ScriptCraft project: canonical rows absent.
        conn.execute(
            "DELETE FROM scenario_data
             WHERE data_name IN ('scene_numbers_prefix','scene_start_number')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO scenario_data (data_name, data_value)
             VALUES ('scene_number_prefix','L'), ('scene_number_start','7')",
            [],
        )
        .unwrap();
    }

    let mut m = kitsp::load_metadata(&tmp).expect("load metadata");
    assert_eq!(m.scene_number_prefix, "L");
    assert_eq!(m.scene_number_start, "7");

    m.scene_number_prefix = "SC".to_string();
    m.scene_number_start = "12".to_string();
    kitsp::save_metadata(&tmp, &m).expect("save metadata");
    {
        let conn = rusqlite::Connection::open(&tmp).unwrap();
        assert_eq!(
            scenario_data_value(&conn, "scene_numbers_prefix").as_deref(),
            Some("SC"),
            "canonical KIT key must be written"
        );
        assert_eq!(
            scenario_data_value(&conn, "scene_start_number").as_deref(),
            Some("12")
        );
        assert_eq!(
            scenario_data_value(&conn, "scene_number_prefix").as_deref(),
            Some("SC"),
            "existing legacy row must stay in sync"
        );
        assert_eq!(
            scenario_data_value(&conn, "scene_number_start").as_deref(),
            Some("12")
        );
    }
    let m2 = kitsp::load_metadata(&tmp).expect("reload");
    assert_eq!(m2.scene_number_prefix, "SC");
    assert_eq!(m2.scene_number_start, "12");
    let _ = std::fs::remove_file(&tmp);
    let _ = std::fs::remove_file(tmp.with_extension("kitsp.bak"));
}

/// A project that never had the legacy keys must not gain them on save.
#[test]
fn save_metadata_does_not_invent_legacy_keys() {
    let tmp = std::env::temp_dir().join("scriptcraft_test_no_legacy.kitsp");
    let _ = std::fs::remove_file(&tmp);
    kitsp::create_file(&tmp, "Fresh").expect("create");
    let mut m = kitsp::load_metadata(&tmp).expect("load");
    m.scene_number_prefix = "SC".to_string();
    m.scene_number_start = "2".to_string();
    kitsp::save_metadata(&tmp, &m).expect("save");
    let conn = rusqlite::Connection::open(&tmp).unwrap();
    assert_eq!(
        scenario_data_value(&conn, "scene_numbers_prefix").as_deref(),
        Some("SC")
    );
    assert_eq!(
        scenario_data_value(&conn, "scene_number_prefix"),
        None,
        "legacy key should not be created"
    );
    let _ = std::fs::remove_file(&tmp);
    let _ = std::fs::remove_file(tmp.with_extension("kitsp.bak"));
}

/// `save_file` must swap the file atomically: `.bak` holds the previous
/// bytes, no temp file survives, and the result loads cleanly.
#[test]
fn save_file_keeps_bak_and_leaves_no_temp_files() {
    let tmp = std::env::temp_dir().join("scriptcraft_test_atomic.kitsp");
    std::fs::copy(FIXTURE, &tmp).expect("copy fixture");
    let original = std::fs::read(&tmp).expect("read original");

    let mut p = kitsp::load_file(&tmp).expect("load");
    p.blocks[0].text = "INT. TEST - DAY".to_string();
    kitsp::save_file(&tmp, &p).expect("save");

    let bak = std::fs::read(tmp.with_extension("kitsp.bak")).expect(".bak exists");
    assert_eq!(
        bak, original,
        ".bak must hold the previous file byte-for-byte"
    );

    let stem = tmp.file_name().unwrap().to_string_lossy().into_owned();
    let leftovers: Vec<String> = std::fs::read_dir(tmp.parent().unwrap())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(&format!("{stem}.tmp")))
        .collect();
    assert!(
        leftovers.is_empty(),
        "temp files left behind: {leftovers:?}"
    );

    let q = kitsp::load_file(&tmp).expect("reload");
    assert_eq!(q.blocks[0].text, "INT. TEST - DAY");
    let _ = std::fs::remove_file(&tmp);
    let _ = std::fs::remove_file(tmp.with_extension("kitsp.bak"));
}

/// A recovery copy is a full loadable project with the origin path recorded;
/// rewriting it replaces the previous copy (and can drop the origin).
#[test]
fn recovery_file_roundtrip() {
    let dir = std::env::temp_dir().join("scriptcraft_test_recovery");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let rec = dir.join("script-abc.kitsp");

    let p = kitsp::load_file(FIXTURE).expect("load fixture");
    let origin = "/tmp/original project.kitsp";
    kitsp::write_recovery_file(&rec, Some(origin), &p).expect("write recovery");

    let q = kitsp::load_file(&rec).expect("reload recovery");
    assert_eq!(q.blocks.len(), p.blocks.len());
    assert_eq!(q.blocks[0].text, p.blocks[0].text);
    assert_eq!(kitsp::recovery_origin(&rec).as_deref(), Some(origin));

    let mut p2 = p.clone();
    p2.blocks[0].text = "INT. CHANGED - DAY".to_string();
    kitsp::write_recovery_file(&rec, None, &p2).expect("rewrite recovery");
    let q2 = kitsp::load_file(&rec).expect("reload 2");
    assert_eq!(q2.blocks[0].text, "INT. CHANGED - DAY");
    assert_eq!(kitsp::recovery_origin(&rec), None);

    let _ = std::fs::remove_dir_all(&dir);
}

/// `save_file` may only touch `scenario` id=1 (text + corkboard sync).
/// Every other table and row in the KIT database must survive untouched.
#[test]
fn a95_save_file_preserves_all_other_tables() {
    let tmp = std::env::temp_dir().join("scriptcraft_test_preserve.kitsp");
    std::fs::copy(FIXTURE, &tmp).expect("copy fixture");

    const TABLES: [&str; 10] = [
        "_database_history",
        "system_variables",
        "places",
        "scenary_days",
        "times",
        "character_states",
        "transitions",
        "scenario_changes",
        "research",
        "script_versions",
    ];
    let (before, data_before, second_scenario) = {
        let conn = rusqlite::Connection::open(&tmp).unwrap();
        let before: Vec<(&str, Vec<String>)> =
            TABLES.iter().map(|t| (*t, dump_table(&conn, t))).collect();
        let data_before = dump_table(&conn, "scenario_data");
        let second_scenario = dump_sql(&conn, "SELECT * FROM scenario WHERE id=2");
        (before, data_before, second_scenario)
    };

    let mut p = kitsp::load_file(&tmp).expect("load copy");
    p.blocks[0].text = "EXT. ПАРКИНГ ПРЕД ПАНЕЛЕН БЛОК - NIGHT".to_string();
    kitsp::save_file(&tmp, &p).expect("save back");

    let conn = rusqlite::Connection::open(&tmp).unwrap();
    for (table, rows) in &before {
        assert_eq!(
            &dump_table(&conn, table),
            rows,
            "save_file modified unrelated table {table}"
        );
    }
    assert_eq!(
        dump_sql(&conn, "SELECT * FROM scenario WHERE id=2"),
        second_scenario,
        "save_file modified the second scenario row"
    );
    // ScriptCraft adds only its own versioned sidecar rows; every KIT row and
    // every other scenario_data row must stay byte-identical.
    let mut data_after = dump_table(&conn, "scenario_data");
    data_after.retain(|row| !is_scriptcraft_sidecar_row(row));
    assert_eq!(
        data_after, data_before,
        "save_file modified existing scenario_data rows"
    );
    let identity_json =
        scenario_data_value(&conn, kitsp::IDENTITY_KEY).expect("save must write the identity row");
    let identity: serde_json::Value =
        serde_json::from_str(&identity_json).expect("identity row is JSON");
    assert_eq!(identity["schema"], 1);
    assert_eq!(
        identity["blocks"].as_array().map(Vec::len),
        Some(p.blocks.len()),
        "one identity record per block"
    );

    let q = kitsp::load_file(&tmp).expect("reload");
    assert_eq!(q.blocks.len(), p.blocks.len());
    assert_eq!(q.blocks[0].text, p.blocks[0].text);
    assert_eq!(
        q.blocks.iter().map(|b| b.id).collect::<Vec<_>>(),
        p.blocks.iter().map(|b| b.id).collect::<Vec<_>>(),
        "block IDs survive save/reopen"
    );
    let cards = kitsp::load_cards(&tmp).expect("cards");
    assert_eq!(cards.len(), q.scenes().len(), "one card per scene");
    assert_eq!(cards[0].title, p.blocks[0].text, "card title synced");
    assert!(format::validate(&q).is_empty());
    let _ = std::fs::remove_file(&tmp);
    let _ = std::fs::remove_file(tmp.with_extension("kitsp.bak"));
}

/// KIT stores complete `<scenario>` snapshots in `script_versions`; those
/// are recoverable as revision baselines. Import must be idempotent.
#[test]
fn kit_versions_import_as_revision_baselines() {
    let tmp = std::env::temp_dir().join(format!(
        "scriptcraft_test_version_import_{}.kitsp",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&tmp);
    std::fs::copy(FIXTURE, &tmp).expect("copy fixture");
    let saved = r#"<?xml version="1.0"?>
<scenario version="1.0">
<scene_heading uuid="{old-first}"><v><![CDATA[EXT. PARK - DAY]]></v></scene_heading>
<action><v><![CDATA[A man walks.]]></v></action>
</scenario>"#;
    {
        let conn = rusqlite::Connection::open(&tmp).unwrap();
        conn.execute(
            "INSERT INTO script_versions
             (fk_script_id, username, datetime, color, name, description, script_text)
             VALUES (1, 'test', '2026-10-04 20:22:31:796', '', 'Imported Draft', '', ?1)",
            rusqlite::params![saved],
        )
        .unwrap();
    }
    let mut history = RevisionHistory::default();
    let imported =
        kitsp::import_versions_as_revisions(&tmp, &mut history).expect("import versions");
    assert_eq!(imported, 1);
    let set = history.latest().expect("one revision");
    assert_eq!(set.name, "Imported Draft");
    assert_eq!(set.started, "2026-10-04 20:22:31:796");
    assert_eq!(set.baseline.len(), 2);
    assert_eq!(set.baseline[0].kit_uuid.as_deref(), Some("{old-first}"));
    assert_eq!(
        kitsp::import_versions_as_revisions(&tmp, &mut history).unwrap(),
        0,
        "re-import must not duplicate a version"
    );

    let p = kitsp::load_file(&tmp).expect("current script");
    let marks = history.marks(&p.blocks);
    assert!(
        marks.changed > 0,
        "the saved draft differs from the fixture"
    );

    let _ = std::fs::remove_file(&tmp);
    let _ = std::fs::remove_file(tmp.with_extension("kitsp.bak"));
}

#[test]
fn a95_fdx_round_trip() {
    let p = kitsp::load_file(FIXTURE).expect("fixture must load");
    let meta = kitsp::load_metadata(FIXTURE).expect("metadata must load");
    let xml = scriptcraft_core::fdx::to_fdx(&p, &meta);
    let (q, qm) = scriptcraft_core::fdx::from_fdx(&xml).expect("exported fdx must parse");
    assert_eq!(qm.title, "A95");
    assert_eq!(q.title, "A95");
    assert_eq!(q.scenes().len(), 21);
    assert_eq!(q.blocks.len(), p.blocks.len());
    for (a, b) in p.blocks.iter().zip(q.blocks.iter()) {
        assert_eq!(a.element_type, b.element_type, "type for {:?}", a.text);
        assert_eq!(a.text, b.text);
    }
    // Final Draft numbers round-trip and are frozen on import.
    let numbers: Vec<&str> = qm.locked_scene_numbers.split(',').collect();
    assert_eq!(numbers.len(), 21);
    assert_eq!(numbers[0], "1");
    assert_eq!(numbers[20], "21");
    // Title-page fields the FDX title page carries survive.
    assert_eq!(qm.genre, meta.genre.trim());
    assert_eq!(qm.author, meta.author.trim());
    assert_eq!(qm.contacts, meta.contacts.trim());
    // Exporting the imported project reproduces the same XML byte-for-byte.
    assert_eq!(scriptcraft_core::fdx::to_fdx(&q, &qm), xml);
}

#[test]
fn revisions_round_trip_and_survive_normal_save() {
    let tmp = std::env::temp_dir().join(format!(
        "scriptcraft_test_revisions_{}.kitsp",
        std::process::id()
    ));
    let p = kitsp::create_file(&tmp, "Revision test").expect("create");
    let mut history = RevisionHistory::default();
    assert_eq!(
        history.start(
            &p.blocks,
            "Blue Revision".into(),
            "21.10.2026 10:00".into(),
            0,
        ),
        RevisionColor::Blue
    );
    kitsp::save_file_with_revisions(&tmp, &p, &history).expect("save revisions");

    let loaded = kitsp::load_revisions(&tmp).expect("load revisions");
    assert_eq!(loaded, history);
    assert_eq!(loaded.latest().unwrap().name, "Blue Revision");

    let reloaded = kitsp::load_file(&tmp).expect("reload project");
    kitsp::save_file(&tmp, &reloaded).expect("normal save");
    assert_eq!(
        kitsp::load_revisions(&tmp).expect("revisions survive save"),
        history
    );

    let rec = std::env::temp_dir().join(format!(
        "scriptcraft_test_revisions_recovery_{}.kitsp",
        std::process::id()
    ));
    kitsp::write_recovery_file_with_revisions(&rec, None, &p, &history)
        .expect("write recovery with revisions");
    assert_eq!(
        kitsp::load_revisions(&rec).expect("recovery revisions"),
        history
    );

    let _ = std::fs::remove_file(&tmp);
    let _ = std::fs::remove_file(tmp.with_extension("kitsp.bak"));
    let _ = std::fs::remove_file(&rec);
}
