//! Reversible edit commands over the script block list.
//!
//! Applying a command returns the exact inverse as a small command sequence
//! (most commands need just one step; split/merge return two: a whole-text
//! restore plus a block insert/delete carrying the removed block itself, so
//! undo and redo restore it byte-for-byte including its `id`). An undo stack
//! is therefore a `Vec<Vec<EditCommand>>`: apply the inverse sequence to undo,
//! apply the inverse of *that* to redo. [`History`] wraps that pattern with a
//! depth cap. Round-trip exactness (apply → apply inverse → strictly identical
//! document) is locked by unit tests including a fuzz pass.
//!
//! Positions are `(block, paragraph, char)` triples — paragraphs are the
//! `\n`-separated pieces of a block's text, chars are Unicode scalar values.
//! [`EditCommand::DeleteRange`] requires both ends in the same block;
//! cross-block edits are composed by callers (merge/delete then delete-range).
//!
//! Merging into an empty upper block removes it (Word semantics); the inverse
//! split re-creates the empty block, so nothing is lost on undo. `SetType`
//! never touches `kit_uuid` — assigning it for new scene headings stays a
//! caller concern until the app migrates to commands (M4/M5).

use crate::model::{ElementType, ScriptBlock};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EditError {
    #[error("block {0} does not exist")]
    InvalidBlock(usize),
    #[error("position {bi}:{para}:{ch} is outside the script")]
    InvalidPosition { bi: usize, para: usize, ch: usize },
    #[error("range endpoints must be in the same block")]
    MultiBlockRange,
}

/// A caret position: char `ch` of paragraph `para` of block `bi`.
/// Ordering is document order (block, then paragraph, then char).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pos {
    pub bi: usize,
    pub para: usize,
    pub ch: usize,
}

/// Clamp `p` to the nearest valid position in `blocks` (`None` when the
/// script is empty). Editors call this after undo/redo replaced content so a
/// stale caret or selection end can never point outside the document.
/// Absolute char index of `pos` inside its block's text (newlines count
/// as one char).
pub fn char_index(text: &str, pos: Pos) -> usize {
    let mut abs = 0usize;
    for (i, para) in text.split('\n').enumerate() {
        if i == pos.para {
            return abs + pos.ch.min(para.chars().count());
        }
        abs += para.chars().count() + 1;
    }
    abs
}

/// The position at absolute char index `abs` inside block `bi`'s text.
pub fn pos_at_char(text: &str, bi: usize, abs: usize) -> Pos {
    let mut left = abs;
    for (i, para) in text.split('\n').enumerate() {
        let len = para.chars().count();
        if left <= len {
            return Pos {
                bi,
                para: i,
                ch: left,
            };
        }
        left -= len + 1;
    }
    let para = text.split('\n').count().saturating_sub(1);
    Pos {
        bi,
        para,
        ch: text
            .split('\n')
            .nth(para)
            .map(|p| p.chars().count())
            .unwrap_or(0),
    }
}

pub fn clamp_pos(blocks: &[ScriptBlock], p: Pos) -> Option<Pos> {
    if blocks.is_empty() {
        return None;
    }
    let bi = p.bi.min(blocks.len() - 1);
    let n_paras = blocks[bi].text.split('\n').count();
    let para = p.para.min(n_paras - 1);
    let len = blocks[bi]
        .text
        .split('\n')
        .nth(para)
        .map(char_len)
        .unwrap_or(0);
    Some(Pos {
        bi,
        para,
        ch: p.ch.min(len),
    })
}

/// One reversible edit. `text` fields may contain `\n` (paragraph breaks);
/// trailing empty paragraphs are trimmed exactly like `editing::replace_chunk`
/// (KIT never stores them).
#[derive(Clone, Debug, PartialEq)]
pub enum EditCommand {
    /// Insert `text` at `at`.
    InsertText { at: Pos, text: String },
    /// Delete the chars between `start` and `end` (either order, same block).
    DeleteRange { start: Pos, end: Pos },
    /// Change a block's element type (leaves `kit_uuid` alone).
    SetType { bi: usize, t: ElementType },
    /// Replace a block's whole text (autocomplete picks, case folding).
    SetText { bi: usize, text: String },
    /// Insert a block (fresh or previously removed) before index `at`.
    InsertBlock { at: usize, block: ScriptBlock },
    /// Remove block `bi`.
    DeleteBlock { bi: usize },
    /// Split block `at.bi` at `at`: from `at` on moves into a new block of
    /// `new_type` inserted right after. With `fuse` (only effective when
    /// splitting mid-paragraph) the boundary chars stay in one paragraph when
    /// merged back; otherwise the split runs at a paragraph boundary.
    SplitBlock {
        at: Pos,
        new_type: ElementType,
        fuse: bool,
    },
    /// Merge block `bi` into `bi - 1`. With `fuse`, the upper block's last
    /// paragraph and the lower block's first fuse into one (the reverse of a
    /// mid-paragraph [`EditCommand::SplitBlock`]).
    MergeBlocks { bi: usize, fuse: bool },
    /// Swap in a whole new block list. Used for structural edits (scene
    /// reordering, bulk operations) where composing per-block commands would
    /// be noisy; the inverse carries the previous list, so undo restores it
    /// byte-for-byte including ids.
    ReplaceAll { blocks: Vec<ScriptBlock> },
}

impl EditCommand {
    /// Apply this command and return the command sequence that exactly
    /// undoes it (in the order the undo steps must run).
    pub fn apply(self, blocks: &mut Vec<ScriptBlock>) -> Result<Vec<EditCommand>, EditError> {
        match self {
            EditCommand::InsertText { at, text } => Ok(vec![apply_insert_text(blocks, at, &text)?]),
            EditCommand::DeleteRange { start, end } => {
                Ok(vec![apply_delete_range(blocks, start, end)?])
            }
            EditCommand::SetType { bi, t } => Ok(vec![apply_set_type(blocks, bi, t)?]),
            EditCommand::SetText { bi, text } => Ok(vec![apply_set_text(blocks, bi, text)?]),
            EditCommand::InsertBlock { at, block } => {
                Ok(vec![apply_insert_block(blocks, at, block)?])
            }
            EditCommand::DeleteBlock { bi } => Ok(vec![apply_delete_block(blocks, bi)?]),
            EditCommand::SplitBlock { at, new_type, fuse } => {
                apply_split_block(blocks, at, new_type, fuse)
            }
            EditCommand::MergeBlocks { bi, fuse } => apply_merge_blocks(blocks, bi, fuse),
            EditCommand::ReplaceAll { blocks: new } => {
                let old = std::mem::replace(blocks, new);
                Ok(vec![EditCommand::ReplaceAll { blocks: old }])
            }
        }
    }
}

/// Command-based undo/redo stack (inverse sequences of applied commands).
#[derive(Debug)]
pub struct History {
    undo: Vec<Vec<EditCommand>>,
    redo: Vec<Vec<EditCommand>>,
    limit: usize,
}

/// Default depth — plenty for a writing session, bounded for memory.
pub const DEFAULT_HISTORY_LIMIT: usize = 1000;

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl History {
    pub fn new() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit: DEFAULT_HISTORY_LIMIT,
        }
    }

    pub fn with_limit(limit: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit: limit.max(1),
        }
    }

    /// Apply `cmd`, recording its inverse sequence (and dropping the redo
    /// stack).
    pub fn apply(
        &mut self,
        blocks: &mut Vec<ScriptBlock>,
        cmd: EditCommand,
    ) -> Result<(), EditError> {
        let inverse = cmd.apply(blocks)?;
        self.push_undo(inverse);
        Ok(())
    }

    /// Apply `cmds` as one user step, recording the combined inverse.
    pub fn apply_seq(
        &mut self,
        blocks: &mut Vec<ScriptBlock>,
        cmds: Vec<EditCommand>,
    ) -> Result<(), EditError> {
        let inverse = invert_seq(&cmds, blocks)?;
        self.push_undo(inverse);
        Ok(())
    }

    /// Push an undo step for a change that was applied outside the command
    /// system (e.g. a structural edit recorded as
    /// [`EditCommand::ReplaceAll`]). Clears the redo stack like any edit.
    pub fn record(&mut self, inverse: Vec<EditCommand>) {
        self.push_undo(inverse);
    }

    fn push_undo(&mut self, inverse: Vec<EditCommand>) {
        self.undo.push(inverse);
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Undo the last step; `Ok(false)` when there is nothing to undo.
    pub fn undo(&mut self, blocks: &mut Vec<ScriptBlock>) -> Result<bool, EditError> {
        let Some(step) = self.undo.pop() else {
            return Ok(false);
        };
        let redo = invert_seq(&step, blocks)?;
        self.redo.push(redo);
        Ok(true)
    }

    /// Redo the last undone step; `Ok(false)` when there is nothing.
    pub fn redo(&mut self, blocks: &mut Vec<ScriptBlock>) -> Result<bool, EditError> {
        let Some(step) = self.redo.pop() else {
            return Ok(false);
        };
        let undo = invert_seq(&step, blocks)?;
        self.undo.push(undo);
        Ok(true)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

/// Apply `seq`, returning the command sequence that undoes the whole of it:
/// the per-command inverses, in reverse command order.
fn invert_seq(
    seq: &[EditCommand],
    blocks: &mut Vec<ScriptBlock>,
) -> Result<Vec<EditCommand>, EditError> {
    let mut groups: Vec<Vec<EditCommand>> = Vec::with_capacity(seq.len());
    for cmd in seq {
        groups.push(cmd.clone().apply(blocks)?);
    }
    let mut out = Vec::new();
    for group in groups.into_iter().rev() {
        out.extend(group);
    }
    Ok(out)
}

fn paras(b: &ScriptBlock) -> Vec<String> {
    b.text.split('\n').map(str::to_string).collect()
}

/// Paragraphs with the trailing-empty invariant applied (see [`set_paras`]).
fn normalized_paras(b: &ScriptBlock) -> Vec<String> {
    let mut v = paras(b);
    while v.len() > 1 && v.last().is_some_and(|s| s.is_empty()) {
        v.pop();
    }
    if v.is_empty() {
        v.push(String::new());
    }
    v
}

fn text_paras(text: &str) -> Vec<String> {
    text.split('\n').map(str::to_string).collect()
}

/// Store paragraphs back, trimming trailing empty ones (KIT invariant).
fn set_paras(b: &mut ScriptBlock, mut v: Vec<String>) {
    while v.len() > 1 && v.last().is_some_and(|s| s.is_empty()) {
        v.pop();
    }
    b.text = v.join("\n");
}

fn char_len(s: &str) -> usize {
    s.chars().count()
}

fn split_at_char(s: &str, ch: usize) -> (String, String) {
    (s.chars().take(ch).collect(), s.chars().skip(ch).collect())
}

fn check_pos(blocks: &[ScriptBlock], p: Pos) -> Result<(), EditError> {
    let b = blocks.get(p.bi).ok_or(EditError::InvalidBlock(p.bi))?;
    let para = b
        .text
        .split('\n')
        .nth(p.para)
        .ok_or(EditError::InvalidPosition {
            bi: p.bi,
            para: p.para,
            ch: p.ch,
        })?;
    if p.ch > char_len(para) {
        return Err(EditError::InvalidPosition {
            bi: p.bi,
            para: p.para,
            ch: p.ch,
        });
    }
    Ok(())
}

/// The position `text.chars().count()` after `at` if `text` were inserted.
fn advance(at: Pos, text: &str) -> Pos {
    let newlines = text.chars().filter(|&c| c == '\n').count();
    if newlines == 0 {
        Pos {
            bi: at.bi,
            para: at.para,
            ch: at.ch + char_len(text),
        }
    } else {
        let last_len = text.rsplit('\n').next().map(char_len).unwrap_or(0);
        Pos {
            bi: at.bi,
            para: at.para + newlines,
            ch: last_len,
        }
    }
}

fn apply_insert_text(
    blocks: &mut [ScriptBlock],
    at: Pos,
    text: &str,
) -> Result<EditCommand, EditError> {
    check_pos(blocks, at)?;
    let end_raw = advance(at, text);
    let b = &mut blocks[at.bi];
    let mut ps = paras(b);
    let (head, tail) = split_at_char(&ps[at.para], at.ch);
    let joined = format!("{head}{text}{tail}");
    ps.splice(at.para..=at.para, text_paras(&joined));
    set_paras(b, ps);
    // Trailing paragraph breaks can be trimmed away; clamp the inverse range
    // to what actually survived so undo deletes exactly the inserted chars.
    let final_n = b.text.split('\n').count();
    let end = if end_raw.para >= final_n {
        let last = b.text.rsplit('\n').next().unwrap_or("");
        Pos {
            bi: at.bi,
            para: final_n - 1,
            ch: char_len(last),
        }
    } else {
        end_raw
    };
    Ok(EditCommand::DeleteRange { start: at, end })
}

fn apply_delete_range(
    blocks: &mut [ScriptBlock],
    start: Pos,
    end: Pos,
) -> Result<EditCommand, EditError> {
    let (start, end) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    check_pos(blocks, start)?;
    check_pos(blocks, end)?;
    if start.bi != end.bi {
        return Err(EditError::MultiBlockRange);
    }
    let b = &mut blocks[start.bi];
    let old_text = b.text.clone();
    let ps = paras(b);
    let region = ps[start.para..=end.para].join("\n");
    let region_len = char_len(&region);
    let tail_len = char_len(&ps[end.para]) - end.ch;
    let removed: String = region
        .chars()
        .skip(start.ch)
        .take(region_len - start.ch - tail_len)
        .collect();
    let (head, _) = split_at_char(&ps[start.para], start.ch);
    let (_, tail) = split_at_char(&ps[end.para], end.ch);
    let mut out: Vec<String> = ps[..start.para].to_vec();
    out.push(format!("{head}{tail}"));
    out.extend_from_slice(&ps[end.para + 1..]);
    set_paras(b, out);
    // Deleting everything up to the end of the block trims trailing empty
    // paragraphs, so the insertion point may no longer exist; a whole-text
    // restore is the exact inverse in that case.
    let final_n = b.text.split('\n').count();
    if start.para >= final_n {
        return Ok(EditCommand::SetText {
            bi: start.bi,
            text: old_text,
        });
    }
    Ok(EditCommand::InsertText {
        at: start,
        text: removed,
    })
}

fn apply_set_type(
    blocks: &mut [ScriptBlock],
    bi: usize,
    t: ElementType,
) -> Result<EditCommand, EditError> {
    let b = blocks.get_mut(bi).ok_or(EditError::InvalidBlock(bi))?;
    let old = b.element_type;
    b.element_type = t;
    Ok(EditCommand::SetType { bi, t: old })
}

fn apply_set_text(
    blocks: &mut [ScriptBlock],
    bi: usize,
    text: String,
) -> Result<EditCommand, EditError> {
    let b = blocks.get_mut(bi).ok_or(EditError::InvalidBlock(bi))?;
    let old = b.text.clone();
    set_paras(b, text_paras(&text));
    Ok(EditCommand::SetText { bi, text: old })
}

fn apply_insert_block(
    blocks: &mut Vec<ScriptBlock>,
    at: usize,
    mut block: ScriptBlock,
) -> Result<EditCommand, EditError> {
    if at > blocks.len() {
        return Err(EditError::InvalidBlock(at));
    }
    // Keep the trailing-empty invariant for every block in the list, or
    // later position math on this block would be off.
    let ps = normalized_paras(&block);
    set_paras(&mut block, ps);
    blocks.insert(at, block);
    Ok(EditCommand::DeleteBlock { bi: at })
}

fn apply_delete_block(blocks: &mut Vec<ScriptBlock>, bi: usize) -> Result<EditCommand, EditError> {
    if bi >= blocks.len() {
        return Err(EditError::InvalidBlock(bi));
    }
    let removed = blocks.remove(bi);
    Ok(EditCommand::InsertBlock {
        at: bi,
        block: removed,
    })
}

fn apply_split_block(
    blocks: &mut Vec<ScriptBlock>,
    at: Pos,
    new_type: ElementType,
    fuse: bool,
) -> Result<Vec<EditCommand>, EditError> {
    check_pos(blocks, at)?;
    // A "fuse" split at char 0 is a paragraph-boundary split in disguise.
    let fuse = fuse && at.ch > 0;
    let b = &mut blocks[at.bi];
    let old_text = b.text.clone();
    let ps = paras(b);
    let (upper, lower) = if fuse {
        let (head, tail) = split_at_char(&ps[at.para], at.ch);
        let mut up: Vec<String> = ps[..at.para].to_vec();
        up.push(head);
        let mut lo = vec![tail];
        lo.extend_from_slice(&ps[at.para + 1..]);
        (up, lo)
    } else {
        (ps[..at.para].to_vec(), ps[at.para..].to_vec())
    };
    set_paras(b, upper);
    let mut fresh = ScriptBlock::new(new_type, String::new());
    set_paras(&mut fresh, lower);
    if new_type == ElementType::SceneHeading && fresh.kit_uuid.is_none() {
        fresh.kit_uuid = Some(format!("{{{}}}", uuid::Uuid::new_v4()));
    }
    blocks.insert(at.bi + 1, fresh);
    // Split normalization can trim paragraph breaks (the same text can split
    // in more than one way), so the exact inverse restores the whole upper
    // text rather than re-merging.
    Ok(vec![
        EditCommand::DeleteBlock { bi: at.bi + 1 },
        EditCommand::SetText {
            bi: at.bi,
            text: old_text,
        },
    ])
}

fn apply_merge_blocks(
    blocks: &mut Vec<ScriptBlock>,
    bi: usize,
    fuse: bool,
) -> Result<Vec<EditCommand>, EditError> {
    if bi == 0 || bi >= blocks.len() {
        return Err(EditError::InvalidBlock(bi));
    }
    let upper_old_text = blocks[bi - 1].text.clone();
    let lower_block = blocks[bi].clone();
    let up = normalized_paras(&blocks[bi - 1]);
    let lo = normalized_paras(&blocks[bi]);
    let prev_empty = up.len() == 1 && up[0].is_empty();
    let cur_empty = lo.len() == 1 && lo[0].is_empty();
    let merged: Vec<String> = if fuse {
        let n = up.len();
        let mut m: Vec<String> = up[..n - 1].to_vec();
        m.push(format!("{}{}", up[n - 1], lo[0]));
        m.extend_from_slice(&lo[1..]);
        m
    } else if prev_empty {
        lo
    } else if cur_empty {
        up
    } else {
        let mut m = up;
        m.extend_from_slice(&lo);
        m
    };
    set_paras(&mut blocks[bi - 1], merged);
    blocks.remove(bi);
    // Restore the upper text and re-insert the removed lower block (with its
    // original id) for a byte-for-byte undo.
    Ok(vec![
        EditCommand::SetText {
            bi: bi - 1,
            text: upper_old_text,
        },
        EditCommand::InsertBlock {
            at: bi,
            block: lower_block,
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ElementType;

    fn doc() -> Vec<ScriptBlock> {
        vec![
            ScriptBlock::new(ElementType::SceneHeading, "EXT. ПАРК - ДЕН".into()),
            ScriptBlock::new(ElementType::Action, "Първи ред.\nВтори ред.".into()),
            ScriptBlock::new(ElementType::Character, "ИВАН".into()),
            ScriptBlock::new(ElementType::Dialogue, "Здравей!".into()),
            ScriptBlock::new(ElementType::Action, String::new()),
        ]
    }

    #[test]
    fn char_offsets_and_positions_round_trip() {
        let text = "ab\ncd ef\n\ngh";
        for abs in 0..text.chars().count() + 1 {
            let pos = pos_at_char(text, 3, abs);
            assert_eq!(char_index(text, pos), abs, "abs {abs} -> {pos:?}");
        }
        assert_eq!(
            pos_at_char(text, 3, 0),
            Pos {
                bi: 3,
                para: 0,
                ch: 0
            }
        );
        assert_eq!(
            pos_at_char(text, 3, 3),
            Pos {
                bi: 3,
                para: 1,
                ch: 0
            }
        );
    }

    #[test]
    fn replace_all_round_trips_strictly() {
        let mut b = doc();
        let before = b.clone();
        let replacement = vec![
            ScriptBlock::new(ElementType::Action, "Ново начало.".into()),
            ScriptBlock::new(ElementType::Dialogue, "Реплика.".into()),
        ];
        let mut inverse = EditCommand::ReplaceAll {
            blocks: replacement.clone(),
        }
        .apply(&mut b)
        .unwrap();
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].text, "Ново начало.");
        apply_all(&mut b, inverse.drain(..));
        assert!(same_strict(&b, &before));
    }

    #[test]
    fn recorded_steps_undo_and_redo() {
        let mut b = doc();
        let before = b.clone();
        let mut hist = History::new();
        // A structural edit applied outside the command system.
        b.remove(1);
        b[0].text = "Променено.".into();
        hist.record(vec![EditCommand::ReplaceAll {
            blocks: before.clone(),
        }]);
        assert!(hist.can_undo());
        assert!(!hist.can_redo());
        assert!(hist.undo(&mut b).unwrap());
        assert!(same_strict(&b, &before), "undo restores the exact document");
        assert!(hist.can_redo());
        assert!(hist.redo(&mut b).unwrap());
        assert_eq!(b.len(), 4);
        assert_eq!(b[0].text, "Променено.");
        assert!(hist.undo(&mut b).unwrap());
        assert!(same_strict(&b, &before));
    }

    #[test]
    fn clamp_pos_pulls_stale_positions_into_range() {
        assert_eq!(
            clamp_pos(
                &[],
                Pos {
                    bi: 3,
                    para: 1,
                    ch: 4
                }
            ),
            None,
            "no blocks, no valid position"
        );
        let b = doc();
        let clamped = clamp_pos(
            &b,
            Pos {
                bi: 99,
                para: 9,
                ch: 99,
            },
        )
        .unwrap();
        assert_eq!(clamped.bi, b.len() - 1);
        assert_eq!(clamped.para, 0);
        assert_eq!(clamped.ch, 0, "last block is empty");
        let mid = Pos {
            bi: 1,
            para: 1,
            ch: 3,
        };
        assert_eq!(clamp_pos(&b, mid), Some(mid), "valid positions stay put");
        let too_deep = clamp_pos(
            &b,
            Pos {
                bi: 1,
                para: 5,
                ch: 3,
            },
        )
        .unwrap();
        assert_eq!((too_deep.para, too_deep.ch), (1, 3));
    }

    /// Strict equality including ids — every command's inverse sequence must
    /// restore the exact document, not just its semantics.
    fn same_strict(a: &[ScriptBlock], b: &[ScriptBlock]) -> bool {
        a.len() == b.len()
            && a.iter().zip(b).all(|(x, y)| {
                x.id == y.id
                    && x.kit_uuid == y.kit_uuid
                    && x.element_type == y.element_type
                    && x.text == y.text
            })
    }

    fn apply_all(blocks: &mut Vec<ScriptBlock>, cmds: impl IntoIterator<Item = EditCommand>) {
        for c in cmds {
            c.apply(blocks).expect("apply");
        }
    }

    fn round_trip(blocks: &mut Vec<ScriptBlock>, cmd: EditCommand) {
        let before = blocks.clone();
        let inverse = cmd.apply(blocks).expect("apply");
        apply_all(blocks, inverse);
        assert!(
            same_strict(blocks, &before),
            "round trip failed: {blocks:?} vs {before:?}"
        );
    }

    #[test]
    fn insert_text_round_trips() {
        let mut b = doc();
        round_trip(
            &mut b,
            EditCommand::InsertText {
                at: Pos {
                    bi: 1,
                    para: 0,
                    ch: 7,
                },
                text: " нов".into(),
            },
        );
        round_trip(
            &mut b,
            EditCommand::InsertText {
                at: Pos {
                    bi: 1,
                    para: 0,
                    ch: 7,
                },
                text: "+\nнов ред".into(),
            },
        );
        // A trailing paragraph break leaves no trace (KIT trims it) and undo
        // still deletes exactly what appeared.
        round_trip(
            &mut b,
            EditCommand::InsertText {
                at: Pos {
                    bi: 3,
                    para: 0,
                    ch: 8,
                },
                text: "\n".into(),
            },
        );
        round_trip(
            &mut b,
            EditCommand::InsertText {
                at: Pos {
                    bi: 4,
                    para: 0,
                    ch: 0,
                },
                text: "тук".into(),
            },
        );
    }

    #[test]
    fn delete_range_round_trips() {
        let mut b = doc();
        round_trip(
            &mut b,
            EditCommand::DeleteRange {
                start: Pos {
                    bi: 1,
                    para: 0,
                    ch: 2,
                },
                end: Pos {
                    bi: 1,
                    para: 1,
                    ch: 5,
                },
            },
        );
        round_trip(
            &mut b,
            EditCommand::DeleteRange {
                start: Pos {
                    bi: 1,
                    para: 0,
                    ch: 0,
                },
                end: Pos {
                    bi: 1,
                    para: 0,
                    ch: 4,
                },
            },
        );
        // Reversed endpoints are accepted.
        round_trip(
            &mut b,
            EditCommand::DeleteRange {
                start: Pos {
                    bi: 3,
                    para: 0,
                    ch: 8,
                },
                end: Pos {
                    bi: 3,
                    para: 0,
                    ch: 3,
                },
            },
        );
        // Zero-width range is a no-op.
        round_trip(
            &mut b,
            EditCommand::DeleteRange {
                start: Pos {
                    bi: 0,
                    para: 0,
                    ch: 1,
                },
                end: Pos {
                    bi: 0,
                    para: 0,
                    ch: 1,
                },
            },
        );
    }

    #[test]
    fn split_and_merge_round_trip() {
        let mut b = doc();
        for fuse in [false, true] {
            round_trip(
                &mut b,
                EditCommand::SplitBlock {
                    at: Pos {
                        bi: 1,
                        para: 0,
                        ch: 5,
                    },
                    new_type: ElementType::Action,
                    fuse,
                },
            );
            round_trip(
                &mut b,
                EditCommand::SplitBlock {
                    at: Pos {
                        bi: 1,
                        para: 1,
                        ch: 0,
                    },
                    new_type: ElementType::Dialogue,
                    fuse,
                },
            );
            // Splitting at the very end of a block: fuse at the last char.
            round_trip(
                &mut b,
                EditCommand::SplitBlock {
                    at: Pos {
                        bi: 1,
                        para: 1,
                        ch: 10,
                    },
                    new_type: ElementType::Action,
                    fuse: true,
                },
            );
        }
        // Boundary split of an empty block, and merge of empty sides.
        round_trip(
            &mut b,
            EditCommand::SplitBlock {
                at: Pos {
                    bi: 4,
                    para: 0,
                    ch: 0,
                },
                new_type: ElementType::Action,
                fuse: false,
            },
        );
        round_trip(&mut b, EditCommand::MergeBlocks { bi: 4, fuse: false });
        round_trip(&mut b, EditCommand::MergeBlocks { bi: 4, fuse: true });
    }

    #[test]
    fn merge_keeps_upper_block_identity() {
        let mut b = doc();
        let before = b.clone();
        let upper_id = b[2].id;
        let inverse = EditCommand::MergeBlocks { bi: 3, fuse: false }
            .apply(&mut b)
            .unwrap();
        assert_eq!(b.len(), 4);
        assert_eq!(b[2].id, upper_id, "merged block keeps the upper id");
        assert_eq!(b[2].element_type, ElementType::Character);
        assert_eq!(b[2].text, "ИВАН\nЗдравей!");
        apply_all(&mut b, inverse);
        assert!(same_strict(&b, &before));
    }

    #[test]
    fn set_type_and_text_round_trip_strictly() {
        let mut b = doc();
        let before = b.clone();
        for t in [
            ElementType::Parenthetical,
            ElementType::Transition,
            ElementType::SceneHeading,
        ] {
            round_trip(&mut b, EditCommand::SetType { bi: 1, t });
        }
        assert!(b[1].kit_uuid.is_none(), "SetType must not assign kit_uuid");
        let inverse = EditCommand::SetText {
            bi: 1,
            text: "Ново\nсъдържание\n\n".into(),
        }
        .apply(&mut b)
        .unwrap();
        assert_eq!(b[1].text, "Ново\nсъдържание", "trailing empties trimmed");
        apply_all(&mut b, inverse);
        assert!(same_strict(&b, &before));
    }

    #[test]
    fn block_insert_delete_round_trip_strictly() {
        let mut b = doc();
        let before = b.clone();
        let inverse = EditCommand::InsertBlock {
            at: 2,
            block: ScriptBlock::new(ElementType::Shot, "КАДЪР".into()),
        }
        .apply(&mut b)
        .unwrap();
        assert_eq!(b.len(), 6);
        apply_all(&mut b, inverse);
        assert!(same_strict(&b, &before));
    }

    #[test]
    fn invalid_positions_are_rejected() {
        let mut b = doc();
        assert_eq!(
            check_pos(
                &b,
                Pos {
                    bi: 9,
                    para: 0,
                    ch: 0
                }
            ),
            Err(EditError::InvalidBlock(9))
        );
        assert!(matches!(
            check_pos(
                &b,
                Pos {
                    bi: 1,
                    para: 9,
                    ch: 0
                }
            ),
            Err(EditError::InvalidPosition { .. })
        ));
        assert!(matches!(
            check_pos(
                &b,
                Pos {
                    bi: 1,
                    para: 0,
                    ch: 99
                }
            ),
            Err(EditError::InvalidPosition { .. })
        ));
        assert_eq!(
            EditCommand::DeleteRange {
                start: Pos {
                    bi: 0,
                    para: 0,
                    ch: 0
                },
                end: Pos {
                    bi: 1,
                    para: 0,
                    ch: 0
                }
            }
            .apply(&mut b),
            Err(EditError::MultiBlockRange)
        );
    }

    #[test]
    fn history_undo_redo_round_trips() {
        let mut b = doc();
        let start = b.clone();
        let mut h = History::new();
        let cmds = vec![
            EditCommand::InsertText {
                at: Pos {
                    bi: 3,
                    para: 0,
                    ch: 8,
                },
                text: " как си".into(),
            },
            EditCommand::SetType {
                bi: 2,
                t: ElementType::Parenthetical,
            },
            EditCommand::SplitBlock {
                at: Pos {
                    bi: 1,
                    para: 1,
                    ch: 0,
                },
                new_type: ElementType::Action,
                fuse: false,
            },
            EditCommand::InsertBlock {
                at: 0,
                block: ScriptBlock::new(ElementType::SceneHeading, "INT. СТАЯ - НОЩ".into()),
            },
            EditCommand::DeleteRange {
                start: Pos {
                    bi: 4,
                    para: 0,
                    ch: 1,
                },
                end: Pos {
                    bi: 4,
                    para: 0,
                    ch: 3,
                },
            },
            EditCommand::MergeBlocks { bi: 3, fuse: false },
            EditCommand::DeleteBlock { bi: 0 },
            EditCommand::SetText {
                bi: 0,
                text: "ОТВАРЯНЕ".into(),
            },
        ];
        for c in cmds {
            h.apply(&mut b, c).unwrap();
        }
        let after = b.clone();
        assert!(h.can_undo() && !h.can_redo());
        while h.undo(&mut b).unwrap() {}
        assert!(same_strict(&b, &start), "undo-all must restore the start");
        assert!(!h.can_undo() && h.can_redo());
        while h.redo(&mut b).unwrap() {}
        assert!(same_strict(&b, &after), "redo-all must restore the end");
        assert!(!h.redo(&mut b).unwrap(), "redo past the end is a no-op");
    }

    #[test]
    fn history_limit_drops_oldest() {
        let mut b = doc();
        let mut h = History::with_limit(2);
        for _ in 0..3 {
            h.apply(
                &mut b,
                EditCommand::SetText {
                    bi: 0,
                    text: "x".into(),
                },
            )
            .unwrap();
        }
        assert!(h.can_undo());
        h.undo(&mut b).unwrap();
        h.undo(&mut b).unwrap();
        assert!(!h.undo(&mut b).unwrap(), "oldest inverse was dropped");
        assert_eq!(b[0].text, "x", "the first edit is permanent once dropped");
    }

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0 >> 33
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }

        fn text(&mut self) -> String {
            const ALPHABET: [char; 8] = ['а', 'б', 'в', ' ', '\n', '-', 'x', 'Я'];
            let mut s = String::new();
            for _ in 0..self.below(7) {
                s.push(ALPHABET[self.below(ALPHABET.len())]);
            }
            s
        }

        fn pos(&mut self, blocks: &[ScriptBlock]) -> Pos {
            let bi = self.below(blocks.len());
            let paras = blocks[bi].text.split('\n').count();
            let para = self.below(paras);
            let len = blocks[bi]
                .text
                .split('\n')
                .nth(para)
                .map(char_len)
                .unwrap_or(0);
            let ch = self.below(len + 1);
            Pos { bi, para, ch }
        }

        fn etype(&mut self) -> ElementType {
            const TYPES: [ElementType; 10] = [
                ElementType::SceneHeading,
                ElementType::SceneCharacters,
                ElementType::Action,
                ElementType::Character,
                ElementType::Parenthetical,
                ElementType::Dialogue,
                ElementType::Shot,
                ElementType::Transition,
                ElementType::NonPrintable,
                ElementType::Lyrics,
            ];
            TYPES[self.below(TYPES.len())]
        }
    }

    #[test]
    fn fuzz_apply_inverse_round_trips() {
        let mut rng = Rng(0xC0FFEE);
        let mut blocks = doc();
        for step in 0..4000 {
            let cmd = match rng.below(8) {
                0 => EditCommand::InsertText {
                    at: rng.pos(&blocks),
                    text: rng.text(),
                },
                1 => {
                    let bi = rng.below(blocks.len());
                    let p1 = {
                        let d = &blocks;
                        let paras = d[bi].text.split('\n').count();
                        let para = rng.below(paras);
                        let len = d[bi].text.split('\n').nth(para).map(char_len).unwrap_or(0);
                        Pos {
                            bi,
                            para,
                            ch: rng.below(len + 1),
                        }
                    };
                    let p2 = {
                        let d = &blocks;
                        let paras = d[bi].text.split('\n').count();
                        let para = rng.below(paras);
                        let len = d[bi].text.split('\n').nth(para).map(char_len).unwrap_or(0);
                        Pos {
                            bi,
                            para,
                            ch: rng.below(len + 1),
                        }
                    };
                    EditCommand::DeleteRange { start: p1, end: p2 }
                }
                2 => EditCommand::SetType {
                    bi: rng.below(blocks.len()),
                    t: rng.etype(),
                },
                3 => EditCommand::SetText {
                    bi: rng.below(blocks.len()),
                    text: rng.text(),
                },
                4 => EditCommand::InsertBlock {
                    at: rng.below(blocks.len() + 1),
                    block: ScriptBlock::new(rng.etype(), rng.text()),
                },
                5 if blocks.len() > 1 => EditCommand::DeleteBlock {
                    bi: rng.below(blocks.len()),
                },
                6 => EditCommand::SplitBlock {
                    at: rng.pos(&blocks),
                    new_type: rng.etype(),
                    fuse: rng.below(2) == 0,
                },
                7 if blocks.len() > 1 => EditCommand::MergeBlocks {
                    bi: 1 + rng.below(blocks.len() - 1),
                    fuse: rng.below(2) == 0,
                },
                _ => continue,
            };
            let baseline = blocks.clone();
            let inverse = cmd
                .clone()
                .apply(&mut blocks)
                .unwrap_or_else(|e| panic!("step {step} apply failed: {e}\ncmd: {cmd:?}"));
            for undo_step in &inverse {
                undo_step.clone().apply(&mut blocks).unwrap_or_else(|e| {
                    panic!(
                        "step {step} inverse failed: {e}\ncmd: {cmd:?}\nundo step: {undo_step:?}"
                    )
                });
            }
            assert!(
                same_strict(&blocks, &baseline),
                "step {step} broke round trip\nbefore: {baseline:#?}\nafter:  {blocks:#?}\ncmd: {cmd:?}"
            );
            // Leave the command applied so the document keeps evolving.
            cmd.apply(&mut blocks).expect("re-apply");
        }
        // The fuzz must actually have grown/shaped the document.
        assert!(blocks.len() > 3);
    }
}
