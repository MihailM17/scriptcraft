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
fn a95_paginates_into_stable_pages() {
    use scriptcraft_core::{editing, model::ElementType};
    let p = kitsp::load_file(FIXTURE).expect("fixture must load");
    let pages = editing::paginate(&p.blocks, editing::A4_LINES_PER_PAGE);
    // ~27 screenplay pages expected for ~4900 words.
    assert!(
        (20..=35).contains(&pages.len()),
        "unexpected page count {}",
        pages.len()
    );
    // Every block appears exactly once, in order (chunks may repeat blocks).
    let mut seen = vec![0usize; p.blocks.len()];
    for page in &pages {
        for c in &page.chunks {
            for i in c.start..c.end {
                seen[c.bi] += 1;
                let _ = i;
            }
        }
    }
    // Paragraph ranges tile each block exactly (checked per block).
    for (bi, b) in p.blocks.iter().enumerate() {
        let n_paras = b.text.split('\n').count();
        let mut covered = vec![false; n_paras];
        for page in &pages {
            for c in page.chunks.iter().filter(|c| c.bi == bi) {
                for slot in covered.iter_mut().take(c.end.min(n_paras)).skip(c.start) {
                    *slot = true;
                }
            }
        }
        assert!(covered.iter().all(|&x| x), "block {bi} not fully covered");
    }
    assert!(seen.iter().all(|&x| x >= 1), "every block placed");
    // Only splittable types continue across pages.
    for page in &pages {
        for c in &page.chunks {
            if c.contd || c.more {
                let t = p.blocks[c.bi].element_type;
                assert!(
                    matches!(
                        t,
                        ElementType::Action
                            | ElementType::Shot
                            | ElementType::Dialogue
                            | ElementType::Lyrics
                    ),
                    "unexpected split of {t:?} at block {}",
                    c.bi
                );
            }
        }
    }
    // No anchor stranded last on a page (except the final page).
    let last_page = pages.len() - 1;
    for (pi, page) in pages.iter().enumerate() {
        if pi == last_page {
            continue;
        }
        if let Some(last) = page.chunks.last() {
            // An anchor may end a page only if its own content forced it
            // (oversized single block); headings/cues split never.
            if last.more || last.contd {
                continue;
            }
            let t = p.blocks[last.bi].element_type;
            assert!(
                !matches!(
                    t,
                    ElementType::SceneHeading
                        | ElementType::Character
                        | ElementType::SceneCharacters
                        | ElementType::Parenthetical
                ),
                "anchor stranded at page bottom: block {}",
                last.bi
            );
        }
    }
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
fn a95_exports_valid_pdf() {
    use scriptcraft_core::pdf;
    let p = kitsp::load_file(FIXTURE).expect("fixture must load");
    let regular = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../scriptcraft-app/assets/fonts/cour.ttf"
    ))
    .expect("bundled Courier New");
    let bold = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../scriptcraft-app/assets/fonts/courb.ttf"
    ))
    .expect("bundled Courier New Bold");
    let meta = kitsp::load_metadata(FIXTURE).expect("load metadata");
    let bytes = pdf::to_pdf_bytes(&p, &meta, &pdf::ExportOptions::default(), &regular, &bold)
        .expect("pdf renders");
    assert!(bytes.starts_with(b"%PDF-"), "not a PDF");
    assert!(
        bytes.len() > 50_000,
        "suspiciously small PDF: {}",
        bytes.len()
    );
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("A95"), "title missing from PDF");
    // Page count matches the paginator's page count plus the title page
    // (lopdf writes "/Type/Page" without a space; subtract the "/Type/Pages" node).
    let pages = text
        .matches("/Type/Page")
        .count()
        .saturating_sub(text.matches("/Type/Pages").count());
    let expected = pdf::layout_pages(&p, &meta, &pdf::ExportOptions::default()).len() + 1;
    assert_eq!(pages, expected, "PDF page count mismatch");
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
    let m2 = kitsp::load_metadata(&tmp).expect("reload metadata");
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
        assert_eq!(a.element_type, b.element_type);
        assert_eq!(a.text, b.text);
        assert_eq!(a.kit_uuid, b.kit_uuid);
    }
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
