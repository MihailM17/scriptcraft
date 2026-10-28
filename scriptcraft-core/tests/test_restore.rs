//! Restore-as-copy: the current draft is never overwritten.

use scriptcraft_core::kitsp::{self, KitspError, Metadata};
use scriptcraft_core::page_lock::PageLock;
use scriptcraft_core::revision::{RevisionBlock, RevisionColor, RevisionHistory, RevisionSet};
use scriptcraft_core::{ElementType, Project, ScriptBlock};
use std::path::PathBuf;

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "scriptcraft_restore_{}_{}",
            std::process::id(),
            name
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    fn file(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn script() -> Project {
    let mut first = ScriptBlock::new(ElementType::SceneHeading, "INT. ROOM - DAY".into());
    first.kit_uuid = Some("{11111111-1111-4111-8111-111111111111}".into());
    let mut second = ScriptBlock::new(ElementType::SceneHeading, "EXT. STREET - NIGHT".into());
    second.kit_uuid = Some("{22222222-2222-4222-8222-222222222222}".into());
    Project {
        title: "Restore Test".into(),
        blocks: vec![
            first,
            ScriptBlock::new(ElementType::Action, "She waits.".into()),
            ScriptBlock::new(ElementType::Character, "ANNA".into()),
            ScriptBlock::new(ElementType::Dialogue, "Hello.".into()),
            second,
            ScriptBlock::new(ElementType::Action, "Rain.".into()),
        ],
        scene_meta: Vec::new(),
    }
}

fn texts(blocks: &[ScriptBlock]) -> Vec<String> {
    blocks.iter().map(|b| b.text.clone()).collect()
}

fn create_with_script(dir: &TempDir, name: &str, project: &Project) -> PathBuf {
    let path = dir.file(name);
    kitsp::create_file(&path, &project.title).expect("create project");
    kitsp::save_file(&path, project).expect("save script");
    path
}

fn title_page() -> Metadata {
    Metadata {
        title: "Restore Test".into(),
        author: "A. Writer".into(),
        genre: "Drama".into(),
        logline_html: "<p>A test logline.</p>".into(),
        header: "BLUE REVISION".into(),
        footer: "2026".into(),
        scene_number_prefix: "".into(),
        scene_number_start: "1".into(),
        locked_scene_numbers: "1,2".into(),
        ..Default::default()
    }
}

#[test]
fn restore_writes_a_new_file_and_leaves_the_source_byte_identical() {
    let dir = TempDir::new("copy");
    let source = script();
    let baseline = source.blocks.clone();
    let path = create_with_script(&dir, "Source.kitsp", &source);

    let mut history = RevisionHistory::default();
    history.start(
        &baseline,
        "Blue Revision".into(),
        "01.10.2026 10:00".into(),
        2,
    );

    // Edit the live draft after the baseline was captured.
    let mut current = source.clone();
    current.blocks[0].text = "INT. ROOM - NIGHT".into();
    current.blocks[1].text = "She waits, then leaves.".into();
    kitsp::save_file_with_revisions(&path, &current, &history).expect("save draft");

    // Page locks and title-page metadata belong to the current project and
    // must ride along into the restored copy.
    let mut meta = title_page();
    let lock = PageLock::capture(&baseline, &[(0, 0), (4, 0)]);
    assert_eq!(lock.anchors.len(), 2);
    meta.locked_pages = lock.to_json().expect("page lock json");
    kitsp::save_metadata(&path, &meta).expect("save metadata");

    let before = std::fs::read(&path).expect("read source");
    let out = dir.file("Source-Blue-2026-10-01.kitsp");
    let report = kitsp::restore_revision_as_copy(&path, history.latest().unwrap(), &out)
        .expect("restore copy");

    assert_eq!(
        std::fs::read(&path).expect("read source again"),
        before,
        "source project must stay byte-identical"
    );
    assert!(out.exists(), "restored copy must exist");
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("title-page") && w.contains("metadata")),
        "baseline metadata warning must be reported: {:?}",
        report.warnings
    );
    assert_eq!(report.dropped_revisions, 0);

    let restored = kitsp::load_file_report(&out).expect("restored file opens");
    assert!(restored.warnings.is_empty(), "{:?}", restored.warnings);
    assert_eq!(texts(&restored.project.blocks), texts(&baseline));
    assert_eq!(
        restored
            .project
            .blocks
            .iter()
            .map(|b| b.id)
            .collect::<Vec<_>>(),
        baseline.iter().map(|b| b.id).collect::<Vec<_>>(),
        "stable block identity survives restore"
    );

    let restored_meta = kitsp::load_metadata(&out).expect("restored metadata");
    assert_eq!(restored_meta.author, meta.author);
    assert_eq!(restored_meta.genre, meta.genre);
    assert_eq!(restored_meta.logline_html, meta.logline_html);
    assert_eq!(restored_meta.header, meta.header);
    assert_eq!(restored_meta.footer, meta.footer);
    assert_eq!(
        restored_meta.locked_scene_numbers,
        meta.locked_scene_numbers
    );
    assert_eq!(restored_meta.locked_pages, meta.locked_pages);
    let restored_lock =
        PageLock::from_json(&restored_meta.locked_pages).expect("restored page lock json");
    assert_eq!(
        restored_lock.resolve(&restored.project.blocks).len(),
        restored_lock.anchors.len(),
        "page anchors resolve against the restored draft"
    );

    let restored_history = kitsp::load_revisions(&out).expect("restored history");
    assert_eq!(restored_history.sets.len(), 1);
    let diff = restored_history
        .latest()
        .expect("restored set")
        .diff_against_current(&restored.project.blocks);
    assert!(
        !diff.has_changes(),
        "restored baseline compared with itself must be empty"
    );
}

#[test]
fn restoring_an_earlier_revision_drops_later_sets() {
    let dir = TempDir::new("history");
    let source = script();
    let path = create_with_script(&dir, "History.kitsp", &source);
    let blue_blocks = source.blocks.clone();

    let mut history = RevisionHistory::default();
    history.start(
        &blue_blocks,
        "Blue Revision".into(),
        "01.10.2026 10:00".into(),
        2,
    );

    let mut blue_draft = source.clone();
    blue_draft.blocks[1].text = "She waits for the train.".into();
    kitsp::save_file_with_revisions(&path, &blue_draft, &history).expect("save blue");

    history.start(
        &blue_draft.blocks,
        "Pink Revision".into(),
        "08.10.2026 09:30".into(),
        3,
    );
    let mut pink_draft = blue_draft.clone();
    pink_draft.blocks[5].text = "Rain hammers the street.".into();
    kitsp::save_file_with_revisions(&path, &pink_draft, &history).expect("save pink");

    let out = dir.file("History-Blue.kitsp");
    let report = kitsp::restore_revision_as_copy(&path, &history.sets[0], &out)
        .expect("restore blue as copy");
    assert_eq!(report.dropped_revisions, 1);

    let restored = kitsp::load_file(&out).expect("restored opens");
    assert_eq!(texts(&restored.blocks), texts(&blue_blocks));

    let restored_history = kitsp::load_revisions(&out).expect("restored history");
    assert_eq!(restored_history.sets.len(), 1, "Pink is not in the copy");
    let only = restored_history.latest().expect("blue set");
    assert_eq!(only.color, RevisionColor::Blue);
    assert_eq!(only.name, "Blue Revision");
}

#[test]
fn restore_refuses_to_overwrite_the_source_project() {
    let dir = TempDir::new("same");
    let source = script();
    let path = create_with_script(&dir, "Same.kitsp", &source);
    let mut history = RevisionHistory::default();
    history.start(
        &source.blocks,
        "Blue Revision".into(),
        "01.10.2026 10:00".into(),
        2,
    );
    kitsp::save_file_with_revisions(&path, &source, &history).expect("save");

    let err = kitsp::restore_revision_as_copy(&path, history.latest().unwrap(), &path)
        .expect_err("must refuse the source path");
    assert!(matches!(err, KitspError::RestoreOverwritesSource));
}

#[test]
fn old_baselines_without_ids_restore_and_compare_empty() {
    let dir = TempDir::new("old");
    let source = script();
    let path = create_with_script(&dir, "Old.kitsp", &source);

    let old_baseline = vec![
        RevisionBlock {
            element_type: ElementType::SceneHeading,
            text: "INT. ATTIC - NIGHT".into(),
            kit_uuid: None,
            block_id: None,
        },
        RevisionBlock {
            element_type: ElementType::Action,
            text: "Dust hangs in the air.".into(),
            kit_uuid: None,
            block_id: None,
        },
    ];
    let mut history = RevisionHistory::default();
    history.sets.push(RevisionSet {
        color: RevisionColor::Blue,
        name: "Imported Old Draft".into(),
        started: "20.09.2026 18:00".into(),
        pages: 1,
        baseline: old_baseline.clone(),
    });
    kitsp::save_file_with_revisions(&path, &source, &history).expect("save history");

    let out = dir.file("Old-restored.kitsp");
    let report = kitsp::restore_revision_as_copy(&path, &history.sets[0], &out)
        .expect("restore old baseline");
    assert!(
        report.warnings.iter().any(|w| w.contains("title-page")),
        "metadata warning is still shown"
    );

    let restored = kitsp::load_file(&out).expect("restored old draft opens");
    assert_eq!(
        texts(&restored.blocks),
        old_baseline
            .iter()
            .map(|b| b.text.clone())
            .collect::<Vec<_>>()
    );
    let mut ids = std::collections::HashSet::new();
    for block in &restored.blocks {
        assert!(ids.insert(block.id), "restored ids must be unique");
    }

    let restored_history = kitsp::load_revisions(&out).expect("history");
    let diff = restored_history
        .latest()
        .expect("set")
        .diff_against_current(&restored.blocks);
    assert!(
        !diff.has_changes(),
        "content fallback makes the self-comparison empty"
    );
}
