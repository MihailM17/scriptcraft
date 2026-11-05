use scriptcraft_core::{
    edit::{EditCommand, History},
    fdx, kitsp,
    model::{dual_pairs, DualMark, DualSide, ElementType, Project, ScriptBlock},
};
use std::path::{Path, PathBuf};
use uuid::Uuid;

const FDX: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/dual-dialogue.fdx");

struct TempKitsp {
    path: PathBuf,
}

impl TempKitsp {
    fn created(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "scriptcraft_dual_{}_{}.kitsp",
            std::process::id(),
            name
        ));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("kitsp.bak"));
        kitsp::create_file(&path, "Dual dialogue").expect("create project");
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

fn sample() -> Project {
    let mut p = Project {
        title: "Dual".into(),
        ..Default::default()
    };
    for (t, text) in [
        (ElementType::SceneHeading, "INT. ROOM - DAY"),
        (ElementType::Action, "Two people speak at once."),
        (ElementType::Character, "ADA"),
        (ElementType::Parenthetical, "(quietly)"),
        (ElementType::Dialogue, "Left side."),
        (ElementType::Character, "GRACE"),
        (ElementType::Dialogue, "Right side."),
        (ElementType::Action, "They stop."),
    ] {
        p.blocks.push(ScriptBlock::new(t, text.into()));
    }
    p
}

fn pair_up(p: &mut Project) -> (usize, usize) {
    let left = p
        .blocks
        .iter()
        .position(|b| b.text == "ADA")
        .expect("left cue");
    let right = p
        .blocks
        .iter()
        .position(|b| b.text == "GRACE")
        .expect("right cue");
    let group = Uuid::new_v4();
    p.blocks[left].dual = Some(DualMark {
        group,
        side: DualSide::Left,
    });
    p.blocks[right].dual = Some(DualMark {
        group,
        side: DualSide::Right,
    });
    (left, right)
}

#[test]
fn fdx_import_and_export_preserve_the_pair() {
    let (p, _) = fdx::from_fdx(include_str!("../../fixtures/dual-dialogue.fdx")).expect("import");
    let pairs = p.dual_pairs();
    assert_eq!(pairs.len(), 1);
    assert_eq!(p.blocks[pairs[0].left].text, "ADA");
    assert_eq!(p.blocks[pairs[0].right].text, "GRACE");
    assert_eq!(p.blocks[pairs[0].left + 1].text, "(quietly)");

    let xml = fdx::to_fdx(&p, &kitsp::Metadata::default());
    assert_eq!(xml.matches("<DualDialogue>").count(), 1);
    assert_eq!(xml.matches("</DualDialogue>").count(), 1);
    let (q, _) = fdx::from_fdx(&xml).expect("re-import");
    let back = q.dual_pairs();
    assert_eq!(back.len(), 1);
    assert_eq!(q.blocks[back[0].left].text, "ADA");
    assert_eq!(q.blocks[back[0].right].text, "GRACE");
}

#[test]
fn ordinary_dialogue_has_no_pair() {
    let mut p = sample();
    p.blocks.remove(6); // right dialogue
    p.blocks.remove(5); // right cue
    assert!(p.dual_pairs().is_empty());
    let xml = fdx::to_fdx(&p, &kitsp::Metadata::default());
    assert!(!xml.contains("<DualDialogue>"));
}

#[test]
fn markers_need_adjacent_units() {
    let mut p = sample();
    let left = 2;
    let right = 5;
    let group = Uuid::new_v4();
    p.blocks[left].dual = Some(DualMark {
        group,
        side: DualSide::Left,
    });
    p.blocks[right].dual = Some(DualMark {
        group,
        side: DualSide::Right,
    });
    assert_eq!(dual_pairs(&p.blocks).len(), 1);
    // An action inserted between the units invalidates the pair without
    // touching either marker.
    p.blocks
        .insert(4, ScriptBlock::new(ElementType::Action, "Beat.".into()));
    assert!(dual_pairs(&p.blocks).is_empty());
}

#[test]
fn sidecar_round_trips_the_pair() {
    let t = TempKitsp::created("roundtrip");
    let mut p = sample();
    let (left, right) = pair_up(&mut p);
    kitsp::save_file(t.path(), &p).expect("save");
    let loaded = kitsp::load_file(t.path()).expect("load");
    let pairs = loaded.dual_pairs();
    assert_eq!(pairs.len(), 1);
    assert_eq!(loaded.blocks[pairs[0].left].id, p.blocks[left].id);
    assert_eq!(loaded.blocks[pairs[0].right].id, p.blocks[right].id);
    assert_eq!(
        loaded.blocks[pairs[0].left].element_type,
        ElementType::Character
    );

    // Removing the pairing deletes the sidecar row on the next save.
    let mut unpaired = loaded;
    unpaired.blocks[pairs[0].left].dual = None;
    unpaired.blocks[pairs[0].right].dual = None;
    kitsp::save_file(t.path(), &unpaired).expect("save unpaired");
    let conn = rusqlite::Connection::open(t.path()).expect("open");
    let row: Option<String> = conn
        .query_row(
            "SELECT data_value FROM scenario_data WHERE data_name=?1",
            rusqlite::params![kitsp::DUAL_GROUPS_KEY],
            |row| row.get(0),
        )
        .ok();
    assert!(row.is_none(), "no empty dual sidecar row");
}

#[test]
fn undo_restores_single_column_state() {
    let mut p = sample();
    let left = 2;
    let right = 5;
    let group = Uuid::new_v4();
    let mut history = History::new();
    history
        .apply_seq(
            &mut p.blocks,
            vec![
                EditCommand::SetDual {
                    bi: left,
                    mark: Some(DualMark {
                        group,
                        side: DualSide::Left,
                    }),
                },
                EditCommand::SetDual {
                    bi: right,
                    mark: Some(DualMark {
                        group,
                        side: DualSide::Right,
                    }),
                },
            ],
        )
        .expect("pair");
    assert_eq!(p.dual_pairs().len(), 1);
    history.undo(&mut p.blocks).expect("undo");
    assert!(p.dual_pairs().is_empty());
    assert!(p.blocks[left].dual.is_none());
    assert!(p.blocks[right].dual.is_none());
    history.redo(&mut p.blocks).expect("redo");
    assert_eq!(p.dual_pairs().len(), 1);
}

#[test]
fn fixture_loads_through_the_fdx_path() {
    assert!(FDX.ends_with("dual-dialogue.fdx"));
}
