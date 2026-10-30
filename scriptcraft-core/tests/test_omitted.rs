//! Omitted scenes: the body disappears from pages and statistics, while the
//! scene heading, number and identity survive and the sidecar round-trips.

use scriptcraft_core::{kitsp, ElementType, Project, ScriptBlock};
use std::path::PathBuf;

struct TempKitsp {
    path: PathBuf,
}

impl TempKitsp {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "scriptcraft_omitted_{}_{}.kitsp",
            std::process::id(),
            name
        ));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("kitsp.bak"));
        Self { path }
    }
}

impl Drop for TempKitsp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_file(self.path.with_extension("kitsp.bak"));
    }
}

fn script() -> Project {
    Project {
        title: "Omitted".into(),
        blocks: vec![
            ScriptBlock::new(ElementType::SceneHeading, "INT. ROOM - DAY".into()),
            ScriptBlock::new(ElementType::Action, "She waits quietly.".into()),
            ScriptBlock::new(ElementType::Character, "ANNA".into()),
            ScriptBlock::new(ElementType::Dialogue, "Hello there.".into()),
            ScriptBlock::new(ElementType::SceneHeading, "EXT. STREET - NIGHT".into()),
            ScriptBlock::new(ElementType::Action, "Rain falls.".into()),
        ],
        scene_meta: Vec::new(),
    }
}

fn omit_first_scene(p: &mut Project) {
    let id = p.scenes()[0].id;
    p.set_scene_omitted(id, true);
}

#[test]
fn document_view_collapses_body_and_maps_indices() {
    let mut p = script();
    omit_first_scene(&mut p);
    let view = p.document_view();

    assert_eq!(
        view.blocks
            .iter()
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>(),
        vec![
            "INT. ROOM - DAY",
            "OMITTED",
            "EXT. STREET - NIGHT",
            "Rain falls."
        ]
    );
    assert_eq!(view.sources, vec![Some(0), None, Some(4), Some(5)]);
    assert_eq!(view.omitted, vec![false, true, false, false]);
    assert_eq!(
        view.revision_sources,
        vec![Some(0), Some(0), Some(4), Some(5)]
    );
    assert_eq!(view.original_to_view[0], Some(0));
    // Every hidden body block points at its scene's OMITTED line.
    assert_eq!(view.original_to_view[1], Some(1));
    assert_eq!(view.original_to_view[2], Some(1));
    assert_eq!(view.original_to_view[3], Some(1));
    assert_eq!(view.original_to_view[4], Some(2));
    assert_eq!(view.original_to_view[5], Some(3));
}

#[test]
fn omission_excludes_body_from_stats_duration_and_cast() {
    let full = script();
    let mut omitted = full.clone();
    omit_first_scene(&mut omitted);

    assert_eq!(omitted.omitted_scene_count(), 1);
    assert!(omitted.has_omitted_scenes());
    assert_eq!(omitted.scenes().len(), full.scenes().len());
    assert_eq!(omitted.stats().scenes, full.stats().scenes);

    let hidden_words = "She waits quietly.".split_whitespace().count()
        + "ANNA".split_whitespace().count()
        + "Hello there.".split_whitespace().count();
    assert_eq!(omitted.word_count(), full.word_count() - hidden_words);
    assert_eq!(
        omitted.stats().words,
        full.stats().words - hidden_words,
        "statistics exclude omitted body words"
    );
    assert_eq!(omitted.stats().actions, full.stats().actions - 1);
    assert_eq!(omitted.stats().dialogues, full.stats().dialogues - 1);
    assert_eq!(omitted.stats().blocks, full.stats().blocks - 3);
    assert!(
        omitted.duration_secs() < full.duration_secs(),
        "omitted body must not consume estimated screen time"
    );
    assert!(
        omitted.scene_duration_secs(omitted.scenes().first())
            < full.scene_duration_secs(full.scenes().first())
    );
    assert_eq!(
        omitted.scene_duration_secs(omitted.scenes().get(1)),
        full.scene_duration_secs(full.scenes().get(1)),
        "other scenes keep their duration"
    );
    assert!(
        !omitted.characters().contains(&"ANNA".to_string()),
        "an omitted scene's cast must not leak into reports"
    );
    assert!(full.characters().contains(&"ANNA".to_string()));
}

#[test]
fn no_omissions_stay_an_identity_view() {
    let p = script();
    let view = p.document_view();
    assert!(view.is_identity());
    assert_eq!(view.blocks.len(), p.blocks.len());
}

#[test]
fn omission_survives_save_and_reopen() {
    let t = TempKitsp::new("reopen");
    kitsp::create_file(&t.path, "Omitted").expect("create");
    let mut p = script();
    // Assign KIT uuids so the project is a normal saved screenplay.
    for (i, b) in p
        .blocks
        .iter_mut()
        .filter(|b| b.element_type == ElementType::SceneHeading)
        .enumerate()
    {
        b.kit_uuid = Some(format!("{{00000000-0000-4000-8000-00000000000{i}}}"));
    }
    omit_first_scene(&mut p);
    kitsp::save_file(&t.path, &p).expect("save");

    let loaded = kitsp::load_file(&t.path).expect("reopen");
    let scenes = loaded.scenes();
    assert!(loaded.scene_is_omitted(&scenes[0]));
    assert!(!loaded.scene_is_omitted(&scenes[1]));
    assert_eq!(loaded.omitted_scene_count(), 1);
    assert_eq!(loaded.blocks.len(), p.blocks.len());

    // Restoring the scene brings its body back without changing identity.
    let mut restored = loaded;
    let id = restored.scenes()[0].id;
    restored.set_scene_omitted(id, false);
    restored.retain_scene_meta();
    assert!(!restored.has_omitted_scenes());
    assert!(restored.document_view().is_identity());
    assert_eq!(restored.word_count(), p.word_count() + 3 + 1 + 2);
}
