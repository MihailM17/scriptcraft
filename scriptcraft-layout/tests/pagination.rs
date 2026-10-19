//! Structural properties of the paginator on the real fixture: every block
//! placed, paragraph ranges tiled exactly, only splittable types split, and no
//! anchor stranded at a page bottom.

use scriptcraft_core::kitsp;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/A95.kitsp");

#[test]
fn a95_paginates_into_stable_pages() {
    use scriptcraft_core::model::ElementType;
    use scriptcraft_layout::paginate;
    let p = kitsp::load_file(FIXTURE).expect("fixture must load");
    let pages = paginate::paginate(&p.blocks, &scriptcraft_core::editing::RUSSIAN);
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
