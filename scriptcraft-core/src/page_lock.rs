//! Production page locks: every original page start is stored as a
//! content fingerprint plus paragraph offset. Layout resolves those anchors
//! to force the same page starts after edits; inserted pages are labelled
//! 42A, 42B, … so schedules that reference page numbers stay valid.

use serde::{Deserialize, Serialize};

use crate::model::{DocumentView, ElementType, ScriptBlock};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageAnchor {
    /// Original page number ("1", "2", …).
    pub page: u32,
    pub element_type: ElementType,
    /// Full block text for a page start at a block boundary; for a page
    /// start inside a splittable block, the paragraph text at that point.
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kit_uuid: Option<String>,
    /// Paragraph offset when the page starts inside a splittable block.
    #[serde(default)]
    pub para: usize,
    /// Text just before the page start (previous paragraph/block last line);
    /// disambiguates repeated cues and actions without making the lock
    /// depend on unchanged neighbours.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// Text just after the page start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageLock {
    #[serde(default)]
    pub pages: u32,
    #[serde(default)]
    pub anchors: Vec<PageAnchor>,
}

/// A page anchor found in the current script.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedAnchor {
    pub bi: usize,
    pub para: usize,
    pub page: u32,
}

impl PageLock {
    pub fn is_empty(&self) -> bool {
        self.anchors.is_empty()
    }

    /// Capture anchors from page starts `(block index, paragraph offset)`.
    pub fn capture(blocks: &[ScriptBlock], starts: &[(usize, usize)]) -> Self {
        let mut anchors = Vec::with_capacity(starts.len());
        for (i, &(bi, para)) in starts.iter().enumerate() {
            let Some(b) = blocks.get(bi) else { continue };
            let text = if para == 0 {
                b.text.clone()
            } else {
                b.text.split('\n').nth(para).unwrap_or(&b.text).to_string()
            };
            anchors.push(PageAnchor {
                page: i as u32 + 1,
                element_type: b.element_type,
                text,
                kit_uuid: b.kit_uuid.clone(),
                para,
                before: context_before(blocks, bi, para),
                after: context_after(blocks, bi, para),
            });
        }
        Self {
            pages: anchors.len() as u32,
            anchors,
        }
    }

    /// Resolve anchors against the current blocks, preserving document order.
    /// Missing anchors (deleted scenes/pages) are skipped. Paragraph offsets
    /// are clamped when the block got shorter.
    pub fn resolve(&self, blocks: &[ScriptBlock]) -> Vec<ResolvedAnchor> {
        let mut out = Vec::with_capacity(self.anchors.len());
        let mut cursor: Option<(usize, usize)> = None;
        for anchor in &self.anchors {
            // Page one starts at the document start by definition; keep it
            // even if the opening block was rewritten.
            if cursor.is_none() && anchor.page == 1 && anchor.para == 0 && !blocks.is_empty() {
                out.push(ResolvedAnchor {
                    bi: 0,
                    para: 0,
                    page: 1,
                });
                cursor = Some((0, 0));
                continue;
            }
            let start_bi = cursor.map(|(bi, _)| bi).unwrap_or(0);
            let mut best: Option<(u8, usize, usize)> = None;
            for bi in start_bi..blocks.len() {
                let min_para = match cursor {
                    Some((last_bi, last_para)) if bi == last_bi => last_para + 1,
                    _ => 0,
                };
                for para in candidates(anchor, &blocks[bi], min_para) {
                    let score = context_score(anchor, blocks, bi, para);
                    if best.is_none_or(|(s, _, _)| score > s) {
                        best = Some((score, bi, para));
                    }
                }
            }
            let Some((_, bi, para)) = best else { continue };
            out.push(ResolvedAnchor {
                bi,
                para,
                page: anchor.page,
            });
            cursor = Some((bi, para));
        }
        out
    }

    /// Resolve anchors against the full original blocks, then map them into
    /// the display view. Page starts that lived inside an omitted scene's
    /// body vanish with those pages; their last label is carried to the first
    /// visible block after the omission so later pages keep their production
    /// numbers instead of collapsing onto the previous page.
    pub fn resolve_view(&self, blocks: &[ScriptBlock], view: &DocumentView) -> Vec<ResolvedAnchor> {
        let mut visible: Vec<ResolvedAnchor> = Vec::new();
        // Marker view index -> last page label hidden behind that marker.
        let mut carries: Vec<(usize, u32)> = Vec::new();
        for anchor in self.resolve(blocks) {
            let Some(view_bi) = view.original_to_view.get(anchor.bi).copied().flatten() else {
                continue;
            };
            if view.omitted.get(view_bi).copied().unwrap_or(false) {
                if let Some(entry) = carries.iter_mut().find(|(marker, _)| *marker == view_bi) {
                    entry.1 = anchor.page;
                } else {
                    carries.push((view_bi, anchor.page));
                }
                continue;
            }
            let last = view
                .blocks
                .get(view_bi)
                .map(|b| b.text.split('\n').count().saturating_sub(1))
                .unwrap_or(0);
            visible.push(ResolvedAnchor {
                bi: view_bi,
                para: anchor.para.min(last),
                page: anchor.page,
            });
        }
        for (marker, page) in carries {
            let Some(target) = (marker + 1..view.blocks.len())
                .find(|&vi| !view.omitted.get(vi).copied().unwrap_or(false))
            else {
                continue;
            };
            // A visible anchor at the same block already pins the page; only
            // add the carried label when the omission swallowed that anchor.
            if visible.iter().any(|a| a.bi == target && a.page >= page) {
                continue;
            }
            visible.push(ResolvedAnchor {
                bi: target,
                para: 0,
                page,
            });
        }
        visible.sort_by_key(|a| (a.bi, a.para));
        visible.dedup_by(|next, prev| next.bi == prev.bi && next.para == prev.para);
        visible
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// Paragraph offsets where `anchor` starts in `block`, at or after
/// `min_para`. Boundaries match on full block text; mid-block page starts
/// match their paragraph text so earlier edits in the same block do not
/// break the lock.
fn candidates(anchor: &PageAnchor, block: &ScriptBlock, min_para: usize) -> Vec<usize> {
    if anchor.element_type != block.element_type {
        return Vec::new();
    }
    let uuid_match = match (anchor.kit_uuid.as_deref(), block.kit_uuid.as_deref()) {
        (Some(a), Some(b)) if !a.is_empty() && !b.is_empty() => a == b,
        _ => false,
    };
    let paras: Vec<&str> = block.text.split('\n').collect();
    if anchor.para == 0 {
        if min_para > 0 {
            Vec::new()
        } else if uuid_match || anchor.text == block.text {
            vec![0]
        } else {
            Vec::new()
        }
    } else if uuid_match {
        if min_para < paras.len() {
            vec![min_para]
        } else {
            Vec::new()
        }
    } else {
        paras
            .iter()
            .enumerate()
            .skip(min_para)
            .filter(|(_, p)| **p == anchor.text)
            .map(|(i, _)| i)
            .collect()
    }
}

fn context_before(blocks: &[ScriptBlock], bi: usize, para: usize) -> Option<String> {
    if para > 0 {
        return blocks[bi]
            .text
            .split('\n')
            .nth(para - 1)
            .map(str::to_string);
    }
    bi.checked_sub(1)
        .and_then(|i| blocks[i].text.split('\n').next_back())
        .map(str::to_string)
}

fn context_after(blocks: &[ScriptBlock], bi: usize, para: usize) -> Option<String> {
    let paras: Vec<&str> = blocks[bi].text.split('\n').collect();
    if let Some(line) = paras.get(para + 1) {
        return Some((*line).to_string());
    }
    blocks
        .get(bi + 1)
        .and_then(|b| b.text.split('\n').next())
        .map(str::to_string)
}

fn context_score(anchor: &PageAnchor, blocks: &[ScriptBlock], bi: usize, para: usize) -> u8 {
    let matches = |stored: &Option<String>, actual: Option<String>| {
        stored
            .as_deref()
            .zip(actual.as_deref())
            .is_some_and(|(a, b)| a == b)
    };
    u8::from(matches(&anchor.before, context_before(blocks, bi, para))) * 2
        + u8::from(matches(&anchor.after, context_after(blocks, bi, para)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(t: ElementType, text: &str) -> ScriptBlock {
        ScriptBlock::new(t, text.into())
    }

    #[test]
    fn capture_and_resolve_round_trip() {
        let blocks = vec![
            block(ElementType::SceneHeading, "INT. ROOM - DAY"),
            block(ElementType::Action, "One."),
            block(ElementType::Action, "Two."),
            block(ElementType::Action, "Three."),
        ];
        let lock = PageLock::capture(&blocks, &[(0, 0), (2, 0)]);
        assert_eq!(lock.pages, 2);
        assert_eq!(lock.anchors[1].page, 2);
        let json = lock.to_json().unwrap();
        let restored = PageLock::from_json(&json).unwrap();
        let resolved = restored.resolve(&blocks);
        assert_eq!(
            resolved,
            vec![
                ResolvedAnchor {
                    bi: 0,
                    para: 0,
                    page: 1
                },
                ResolvedAnchor {
                    bi: 2,
                    para: 0,
                    page: 2
                }
            ]
        );
    }

    #[test]
    fn resolve_skips_deleted_pages_and_keeps_order() {
        let blocks = vec![
            block(ElementType::SceneHeading, "INT. ONE - DAY"),
            block(ElementType::Action, "A."),
            block(ElementType::SceneHeading, "INT. TWO - DAY"),
        ];
        let mut lock = PageLock::capture(&blocks, &[(0, 0), (1, 0), (2, 0)]);
        lock.anchors[1].text = "GONE".into();
        lock.anchors[1].element_type = ElementType::Action;
        let resolved = lock.resolve(&blocks);
        assert_eq!(
            resolved.iter().map(|a| a.bi).collect::<Vec<_>>(),
            vec![0, 2]
        );
        assert_eq!(resolved[1].page, 3);
    }

    #[test]
    fn uuid_beats_renamed_heading() {
        let mut heading = block(ElementType::SceneHeading, "INT. ROOM - DAY");
        heading.kit_uuid = Some("{one}".into());
        let mut blocks = vec![heading];
        let mut lock = PageLock::capture(&blocks, &[(0, 0)]);
        blocks[0].text = "INT. OTHER ROOM - NIGHT".into();
        assert_eq!(lock.resolve(&blocks).len(), 1);
        lock.anchors[0].kit_uuid = None;
        // Page one still resolves to the document start even without a match.
        assert_eq!(lock.resolve(&blocks).len(), 1);
    }

    #[test]
    fn mid_block_pages_survive_earlier_paragraph_edits() {
        let mut blocks = vec![block(ElementType::Action, "one\ntwo\nthree")];
        let lock = PageLock::capture(&blocks, &[(0, 0), (0, 1), (0, 2)]);
        assert_eq!(lock.anchors[1].text, "two");
        blocks[0].text = "ONE EDITED\ntwo\nthree".into();
        let resolved = lock.resolve(&blocks);
        assert_eq!(
            resolved,
            vec![
                ResolvedAnchor {
                    bi: 0,
                    para: 0,
                    page: 1
                },
                ResolvedAnchor {
                    bi: 0,
                    para: 1,
                    page: 2
                },
                ResolvedAnchor {
                    bi: 0,
                    para: 2,
                    page: 3
                },
            ]
        );
    }

    #[test]
    fn paragraph_offset_is_clamped() {
        let blocks = vec![block(ElementType::Action, "Only one paragraph.")];
        let lock = PageLock {
            pages: 1,
            anchors: vec![PageAnchor {
                page: 1,
                element_type: ElementType::Action,
                text: blocks[0].text.clone(),
                kit_uuid: None,
                para: 9,
                before: None,
                after: None,
            }],
        };
        assert_eq!(lock.resolve(&blocks)[0].para, 0);
    }
}
