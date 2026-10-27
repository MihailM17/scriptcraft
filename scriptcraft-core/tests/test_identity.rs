use scriptcraft_core::{kitsp, ElementType, Project, SceneMeta, ScriptBlock};
use std::path::{Path, PathBuf};
use uuid::Uuid;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/A95.kitsp");

struct TempKitsp {
    path: PathBuf,
}

impl TempKitsp {
    fn fixture(name: &str) -> Self {
        let path = temp_path(name);
        std::fs::copy(FIXTURE, &path).expect("copy A95 fixture");
        Self { path }
    }

    fn created(name: &str, title: &str) -> Self {
        let path = temp_path(name);
        kitsp::create_file(&path, title).expect("create project");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempKitsp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_file(self.path.with_extension("kitsp.bak"));
    }
}

fn temp_path(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "scriptcraft_identity_{}_{}.kitsp",
        std::process::id(),
        name
    ));
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("kitsp.bak"));
    path
}

fn load(t: &TempKitsp) -> kitsp::LoadReport {
    kitsp::load_file_report(t.path()).expect("load project")
}

/// Write the script XML exactly as KIT would after an external edit: the
/// ScriptCraft sidecar rows are deliberately left at their old values.
fn write_scenario_only(t: &TempKitsp, p: &Project) {
    let conn = rusqlite::Connection::open(t.path()).expect("open project");
    conn.execute(
        "UPDATE scenario SET text=?1 WHERE id=1",
        rusqlite::params![kitsp::project_to_scenario_xml(p)],
    )
    .expect("write scenario text");
}

fn get_data(t: &TempKitsp, key: &str) -> Option<String> {
    let conn = rusqlite::Connection::open(t.path()).expect("open project");
    conn.query_row(
        "SELECT data_value FROM scenario_data WHERE data_name=?1",
        rusqlite::params![key],
        |row| row.get(0),
    )
    .ok()
}

fn set_data(t: &TempKitsp, key: &str, value: &str) {
    let conn = rusqlite::Connection::open(t.path()).expect("open project");
    conn.execute(
        "INSERT INTO scenario_data (data_name, data_value) VALUES (?1, ?2)
         ON CONFLICT(data_name) DO UPDATE SET data_value=excluded.data_value",
        rusqlite::params![key, value],
    )
    .expect("write sidecar row");
}

fn block_ids(p: &Project) -> Vec<Uuid> {
    p.blocks.iter().map(|b| b.id).collect()
}

fn heading_id_by_uuid(p: &Project, uuid: &str) -> Option<Uuid> {
    p.blocks
        .iter()
        .find(|b| b.kit_uuid.as_deref() == Some(uuid))
        .map(|b| b.id)
}

#[test]
fn save_reopen_preserves_block_and_scene_ids() {
    let t = TempKitsp::fixture("reopen");

    let first = load(&t).project;
    assert!(
        get_data(&t, kitsp::IDENTITY_KEY).is_none(),
        "fixture has no sidecar before the first ScriptCraft save"
    );
    kitsp::save_file(t.path(), &first).expect("save");
    let saved = load(&t).project;
    assert_eq!(block_ids(&saved), block_ids(&first));

    kitsp::save_file(t.path(), &saved).expect("save again");
    let reopened = load(&t).project;
    assert_eq!(block_ids(&reopened), block_ids(&first));
    assert_eq!(
        reopened.scenes().iter().map(|s| s.id).collect::<Vec<_>>(),
        first.scenes().iter().map(|s| s.id).collect::<Vec<_>>()
    );
    assert!(load(&t).warnings.is_empty());
}

#[test]
fn missing_sidecar_opens_cleanly_and_assigns_unique_ids() {
    let t = TempKitsp::fixture("missing_sidecar");

    let report = load(&t);
    assert!(report.warnings.is_empty(), "no sidecar is not an error");
    let p = report.project;
    assert!(kitsp::load_file(t.path()).is_ok());

    let mut ids = std::collections::HashSet::new();
    for block in &p.blocks {
        assert!(ids.insert(block.id), "fresh block IDs must be unique");
    }
    assert!(p.scene_meta.is_empty());

    kitsp::save_file(t.path(), &p).expect("save");
    let q = load(&t).project;
    assert_eq!(block_ids(&q), block_ids(&p));
    assert!(load(&t).warnings.is_empty());
}

#[test]
fn scene_reorder_keeps_ids_after_external_edit() {
    let t = TempKitsp::fixture("reorder");
    let p = load(&t).project;
    kitsp::save_file(t.path(), &p).expect("save");

    // Simulate KIT moving the second scene before the first. The identity row
    // still describes the old order.
    let scenes = p.scenes();
    assert!(scenes.len() > 2);
    let mut reordered = Project {
        title: p.title.clone(),
        blocks: Vec::new(),
        scene_meta: p.scene_meta.clone(),
    };
    for scene in [&scenes[1], &scenes[0]]
        .into_iter()
        .chain(scenes[2..].iter())
    {
        for &index in &scene.blocks {
            reordered.blocks.push(p.blocks[index].clone());
        }
    }
    write_scenario_only(&t, &reordered);

    let report = load(&t);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let q = report.project;
    assert_eq!(q.blocks.len(), p.blocks.len());
    for uuid in p.blocks.iter().filter_map(|b| b.kit_uuid.as_deref()) {
        assert_eq!(
            heading_id_by_uuid(&q, uuid),
            heading_id_by_uuid(&p, uuid),
            "scene heading {uuid} kept its stable ID"
        );
    }
    let before: std::collections::HashSet<Uuid> = block_ids(&p).into_iter().collect();
    let after: std::collections::HashSet<Uuid> = block_ids(&q).into_iter().collect();
    assert_eq!(before, after, "reordering must not create new IDs");
    assert_eq!(q.scenes()[0].heading, scenes[1].heading);
}

#[test]
fn inserted_and_deleted_blocks_do_not_disturb_existing_ids() {
    let t = TempKitsp::fixture("insert_delete");
    let p = load(&t).project;
    kitsp::save_file(t.path(), &p).expect("save");

    let mut inserted = p.clone();
    inserted.blocks.insert(
        1,
        ScriptBlock::new(ElementType::Action, "A newly inserted action.".into()),
    );
    write_scenario_only(&t, &inserted);

    let report = load(&t);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let q = report.project;
    assert_eq!(q.blocks.len(), p.blocks.len() + 1);
    let old: std::collections::HashSet<Uuid> = block_ids(&p).into_iter().collect();
    let new: std::collections::HashSet<Uuid> = block_ids(&q).into_iter().collect();
    assert!(old.is_subset(&new), "every existing block kept its ID");
    assert_eq!(new.len() - old.len(), 1, "only the new block is fresh");
    assert_eq!(q.blocks[1].text, "A newly inserted action.");

    // Now delete an existing block externally. Everything that remains must
    // keep its identity; the deleted block's ID disappears.
    let mut deleted = p.clone();
    deleted.blocks.remove(2);
    write_scenario_only(&t, &deleted);
    let report = load(&t);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let r = report.project;
    assert_eq!(r.blocks.len(), p.blocks.len() - 1);
    let remaining: std::collections::HashSet<Uuid> = block_ids(&r).into_iter().collect();
    let expected: std::collections::HashSet<Uuid> = p
        .blocks
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != 2)
        .map(|(_, b)| b.id)
        .collect();
    assert_eq!(remaining, expected);
}

#[test]
fn external_text_edit_in_place_keeps_identity() {
    let t = TempKitsp::fixture("text_edit");
    let p = load(&t).project;
    kitsp::save_file(t.path(), &p).expect("save");

    let mut edited = p.clone();
    let index = edited
        .blocks
        .iter()
        .position(|b| b.element_type == ElementType::Action)
        .expect("fixture has action");
    edited.blocks[index].text = "Externally rewritten action line.".into();
    write_scenario_only(&t, &edited);

    let report = load(&t);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let q = report.project;
    assert_eq!(block_ids(&q), block_ids(&p));
    assert_eq!(q.blocks[index].text, "Externally rewritten action line.");
}

#[test]
fn replaced_block_of_another_type_gets_a_new_id_and_warning() {
    let t = TempKitsp::fixture("replaced");
    let p = load(&t).project;
    kitsp::save_file(t.path(), &p).expect("save");

    let index = p
        .blocks
        .iter()
        .position(|b| b.element_type == ElementType::Action)
        .expect("fixture has action");
    let old_id = p.blocks[index].id;
    let mut replaced = p.clone();
    let replacement = ScriptBlock::new(ElementType::Dialogue, "A replacement line.".into());
    replaced.blocks[index] = replacement;
    write_scenario_only(&t, &replaced);

    let report = load(&t);
    assert!(
        report.warnings.iter().any(|w| w.contains("re-anchored")),
        "ambiguous replacement must warn: {:?}",
        report.warnings
    );
    let q = report.project;
    assert_ne!(
        q.blocks[index].id, old_id,
        "an old ID is never reused for a different block type"
    );
    let old: std::collections::HashSet<Uuid> = block_ids(&p).into_iter().collect();
    let after: std::collections::HashSet<Uuid> = block_ids(&q).into_iter().collect();
    assert!(!after.contains(&old_id));
    let preserved = after.intersection(&old).count();
    assert_eq!(preserved, p.blocks.len() - 1);
}

#[test]
fn duplicate_headings_keep_distinct_ids_across_reorder() {
    let t = TempKitsp::created("duplicates", "Duplicates");
    let mut p = kitsp::load_file(t.path()).expect("load created project");
    p.blocks = vec![
        ScriptBlock {
            id: Uuid::new_v4(),
            kit_uuid: Some("{duplicate-one}".into()),
            element_type: ElementType::SceneHeading,
            text: "INT. ROOM - DAY".into(),
        },
        ScriptBlock::new(ElementType::Action, "First scene.".into()),
        ScriptBlock {
            id: Uuid::new_v4(),
            kit_uuid: Some("{duplicate-two}".into()),
            element_type: ElementType::SceneHeading,
            text: "INT. ROOM - DAY".into(),
        },
        ScriptBlock::new(ElementType::Action, "Second scene.".into()),
    ];
    kitsp::save_file(t.path(), &p).expect("save duplicate headings");

    let saved = kitsp::load_file(t.path()).expect("reload duplicates");
    let one = heading_id_by_uuid(&saved, "{duplicate-one}").unwrap();
    let two = heading_id_by_uuid(&saved, "{duplicate-two}").unwrap();
    assert_ne!(one, two, "identical heading text still has distinct scenes");
    assert_ne!(saved.scenes()[0].id, saved.scenes()[1].id);

    // Swap the two identical scenes outside ScriptCraft. KIT UUIDs are the
    // only reliable way to keep them apart.
    let mut swapped = saved.clone();
    swapped.blocks = vec![
        saved.blocks[2].clone(),
        saved.blocks[3].clone(),
        saved.blocks[0].clone(),
        saved.blocks[1].clone(),
    ];
    write_scenario_only(&t, &swapped);

    let report = load(&t);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let q = report.project;
    assert_eq!(heading_id_by_uuid(&q, "{duplicate-one}"), Some(one));
    assert_eq!(heading_id_by_uuid(&q, "{duplicate-two}"), Some(two));
}

#[test]
fn scene_metadata_roundtrips_and_is_dropped_when_scene_is_gone() {
    let t = TempKitsp::fixture("scene_meta");
    let mut p = load(&t).project;
    kitsp::save_file(t.path(), &p).expect("save");
    let scene_id = p.scenes()[0].id;

    p.set_scene_omitted(scene_id, true);
    kitsp::save_file(t.path(), &p).expect("save omitted");

    let report = load(&t);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let q = report.project;
    assert_eq!(q.scene_meta(scene_id).map(|m| m.omitted), Some(true));
    let json = get_data(&t, kitsp::SCENE_META_KEY).expect("scene metadata row");
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["schema"], 1);
    assert_eq!(value["scenes"][0]["id"], serde_json::json!(scene_id));

    // Keep only metadata whose heading still exists.
    let mut stale = q.clone();
    stale.scene_meta.push(SceneMeta {
        id: Uuid::new_v4(),
        omitted: true,
    });
    kitsp::save_file(t.path(), &stale).expect("save with stale metadata");
    let clean = load(&t).project;
    assert_eq!(clean.scene_meta.len(), 1);
    assert_eq!(clean.scene_meta[0].id, scene_id);

    // An external deletion of the scene must not leak its metadata onto a
    // different scene.
    let mut without = clean.clone();
    without.blocks.retain(|b| b.id != scene_id);
    write_scenario_only(&t, &without);
    let report = load(&t);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("scene metadata record(s)")),
        "{:?}",
        report.warnings
    );
    assert!(report.project.scene_meta.is_empty());
}

#[test]
fn unknown_scene_metadata_is_ignored_with_a_warning() {
    let t = TempKitsp::fixture("unknown_meta");
    let p = load(&t).project;
    kitsp::save_file(t.path(), &p).expect("save");
    let unknown = Uuid::new_v4();
    set_data(
        &t,
        kitsp::SCENE_META_KEY,
        &format!("{{\"schema\":1,\"scenes\":[{{\"id\":\"{unknown}\",\"omitted\":true}}]}}"),
    );

    let report = load(&t);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("no longer exist")),
        "{:?}",
        report.warnings
    );
    assert!(report.project.scene_meta.is_empty());
}

#[test]
fn invalid_identity_row_warns_then_heals_on_save() {
    let t = TempKitsp::fixture("invalid_identity");
    let p = load(&t).project;
    kitsp::save_file(t.path(), &p).expect("save");
    set_data(&t, kitsp::IDENTITY_KEY, "{ this is not json");

    let report = load(&t);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("could not be read")),
        "{:?}",
        report.warnings
    );
    let repaired_source = report.project;
    assert!(!repaired_source.scene_meta.iter().any(|m| m.omitted));

    kitsp::save_file(t.path(), &repaired_source).expect("heal save");
    let check = load(&t);
    assert!(check.warnings.is_empty(), "{:?}", check.warnings);
    assert_eq!(block_ids(&check.project), block_ids(&repaired_source));
}

#[test]
fn newer_identity_schema_is_warned_about_and_not_downgraded() {
    let t = TempKitsp::fixture("newer_schema");
    let p = load(&t).project;
    kitsp::save_file(t.path(), &p).expect("save");
    set_data(&t, kitsp::IDENTITY_KEY, "{\"schema\":99,\"blocks\":[]}");

    let report = load(&t);
    assert!(
        report.warnings.iter().any(|w| w.contains("newer schema")),
        "{:?}",
        report.warnings
    );
    kitsp::save_file(t.path(), &report.project).expect("save");
    let value: serde_json::Value =
        serde_json::from_str(&get_data(&t, kitsp::IDENTITY_KEY).unwrap()).unwrap();
    assert_eq!(value["schema"], 99, "future rows must not be overwritten");
}

#[test]
fn retain_scene_meta_drops_unknown_and_duplicate_ids() {
    let id = Uuid::new_v4();
    let stale = Uuid::new_v4();
    let mut p = Project {
        blocks: vec![ScriptBlock {
            id,
            kit_uuid: None,
            element_type: ElementType::SceneHeading,
            text: "INT. ROOM - DAY".into(),
        }],
        ..Default::default()
    };
    p.scene_meta = vec![
        SceneMeta { id, omitted: true },
        SceneMeta { id, omitted: false },
        SceneMeta {
            id: stale,
            omitted: true,
        },
    ];
    p.retain_scene_meta();
    assert_eq!(p.scene_meta.len(), 1);
    assert_eq!(p.scene_meta[0].id, id);
}
