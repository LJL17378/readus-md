//! Reading belongs to the library's metadata, never to the document body.
//! Quotes survive Markdown reparsing (its block ids are not persistent).

use crate::model::{BlockId, Document};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status { #[default] Later, Reading, Finished }

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Anchor {
    pub block: u64,
    pub ordinal: usize,
    pub quote: String,
    pub before: String,
    pub after: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bookmark {
    pub id: String,
    pub anchor: Anchor,
    pub created: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Reading {
    pub shelved: bool,
    pub status: Status,
    pub position: Option<Anchor>,
    pub progress: f32,
    pub furthest: f32,
    pub last_read: u64,
    pub bookmarks: Vec<Bookmark>,
}

impl Reading {
    pub fn is_default(&self) -> bool { self == &Self::default() }
    pub fn visit(&mut self, anchor: Anchor, progress: f32, now: u64) {
        if !progress.is_finite() { return }
        self.position = Some(anchor);
        self.progress = progress.clamp(0., 1.);
        self.furthest = self.furthest.max(self.progress).clamp(0., 1.);
        self.last_read = now;
        // Reaching the end is a position, not a claim that it was read.
        if self.status == Status::Later { self.status = Status::Reading; }
    }
    pub fn add_bookmark(&mut self, anchor: Anchor, now: u64) -> bool {
        if self.bookmarks.iter().any(|b| b.anchor == anchor) { return false }
        self.bookmarks.push(Bookmark { id: uuid::Uuid::new_v4().to_string(), anchor, created: now });
        true
    }
}

#[derive(Default)]
pub struct Index { passages: Vec<(BlockId, String)> }

fn quote(s: &str) -> String { s.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(160).collect() }

impl Index {
    pub fn new(doc: &Document) -> Self {
        let mut passages = vec![];
        doc.for_each_block(|b| passages.push((b.id, quote(&b.text.plain_text()))));
        Self { passages }
    }
    pub fn anchor(&self, id: BlockId) -> Option<Anchor> {
        let ordinal = self.passages.iter().position(|(b, _)| *b == id)?;
        Some(Anchor { block: id.0, ordinal, quote: self.passages[ordinal].1.clone(),
            before: ordinal.checked_sub(1).map(|i| self.passages[i].1.clone()).unwrap_or_default(),
            after: self.passages.get(ordinal + 1).map(|(_, q)| q.clone()).unwrap_or_default() })
    }
    pub fn first(&self) -> Option<Anchor> { self.anchor(self.passages.first()?.0) }
    pub fn resolve(&self, anchor: &Anchor) -> Option<BlockId> {
        // Never trust a recycled id from a freshly parsed Markdown document.
        let matches: Vec<usize> = self.passages.iter().enumerate().filter_map(|(i, (_, q))| (*q == anchor.quote && !q.is_empty()).then_some(i)).collect();
        let i = match matches.as_slice() {
            [i] => *i,
            _ => {
                let contextual: Vec<_> = matches.into_iter().filter(|&i| {
                    let before = i.checked_sub(1).map(|n| self.passages[n].1.as_str()).unwrap_or("");
                    let after = self.passages.get(i + 1).map(|(_, q)| q.as_str()).unwrap_or("");
                    (!anchor.before.is_empty() && before == anchor.before) || (!anchor.after.is_empty() && after == anchor.after)
                }).collect();
                match contextual.as_slice() { [i] => *i, _ => return None }
            }
        };
        Some(self.passages[i].0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Block;

    fn document(lines: &[&str]) -> Document { Document { blocks: lines.iter().map(|s| Block::text(*s)).collect(), ..Default::default() } }

    #[test]
    fn anchors_survive_reparse_and_insertions_and_reject_deleted_text() {
        let d = document(&["前言", "要继续阅读的段落", "结尾"]);
        let a = Index::new(&d).anchor(d.blocks[1].id).unwrap();
        let next = document(&["新插入的章节", "前言", "要继续阅读的段落", "结尾"]);
        assert_eq!(Index::new(&next).resolve(&a), Some(next.blocks[2].id));
        assert_eq!(Index::new(&document(&["前言", "结尾"])).resolve(&a), None);
    }

    #[test]
    fn repeated_text_requires_unambiguous_context() {
        let d = document(&["第一章", "重复", "第二章", "重复", "结束"]);
        let a = Index::new(&d).anchor(d.blocks[3].id).unwrap();
        let next = document(&["新段落", "第一章", "重复", "第二章", "重复", "结束"]);
        assert_eq!(Index::new(&next).resolve(&a), Some(next.blocks[4].id));
        let mut ambiguous = a; ambiguous.before.clear(); ambiguous.after.clear();
        assert_eq!(Index::new(&next).resolve(&ambiguous), None);
    }

    #[test]
    fn progress_preserves_furthest_and_completion_is_explicit() {
        let mut r = Reading::default();
        r.visit(Anchor::default(), 0.8, 1);
        r.visit(Anchor::default(), 0.1, 2);
        assert_eq!((r.progress, r.furthest, r.status), (0.1, 0.8, Status::Reading));
        r.visit(Anchor::default(), 1., 3);
        assert_eq!(r.status, Status::Reading);
        r.status = Status::Finished;
        r.visit(Anchor::default(), 0., 4);
        assert_eq!(r.status, Status::Finished);
        r.visit(Anchor::default(), f32::NAN, 5);
        assert_eq!(r.last_read, 4);
    }

    #[test]
    fn bookmarks_are_deduplicated_and_old_metadata_defaults() {
        let mut r: Reading = serde_json::from_str("{}").unwrap();
        assert!(r.is_default());
        assert!(r.add_bookmark(Anchor::default(), 1));
        assert!(!r.add_bookmark(Anchor::default(), 2));
        assert_eq!(serde_json::from_str::<Reading>(&serde_json::to_string(&r).unwrap()).unwrap(), r);
    }
}
