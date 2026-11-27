//! The reader observes the same virtualized document as the editor.

use super::render::Item;
use super::view::{Editor, EditorEvent};
use fdl_core::reading::{Anchor, Index};
use gpui::*;

#[derive(Default)]
pub struct Reader {
    generation: Option<u64>,
    index: Index,
    seen: Option<(usize, u32, u32)>,
    pub restore: Option<Anchor>,
    fraction: Option<(crate::model::BlockId, f32)>,
    pub track: bool,
}

impl Editor {
    fn reading_index(&mut self) {
        if self.reader.generation != Some(self.core.generation) {
            self.reader.index = Index::new(&self.core.doc);
            self.reader.generation = Some(self.core.generation);
        }
    }

    pub fn reading_anchor(&mut self, selection: bool) -> Option<Anchor> {
        self.reading_index();
        if selection && !self.core.sel.is_collapsed() {
            return self.reader.index.anchor(self.core.sel.head.block);
        }
        let top = self.list.logical_scroll_top().item_ix;
        let item = self.items.get(top).or_else(|| self.items.last());
        let path = match item {
            Some(Item::Block { path, .. }) => path,
            Some(Item::Below) => return self.reader.index.anchor(self.core.doc.blocks.last()?.id),
            _ => return self.reader.index.first(),
        };
        let mut blocks = self.core.doc.blocks.as_slice();
        let mut id = None;
        for &i in path { let b = blocks.get(i)?; id = Some(b.id); blocks = &b.children; }
        let id = id?;
        let mut anchor = self.reader.index.anchor(id)?;
        if let Some(b) = self.block_bounds.borrow().get(&id) {
            let height = f32::from(b.size.height).max(1.);
            anchor.fraction = (f32::from(self.list.viewport_bounds().top() - b.top()) / height).clamp(0., 1.);
        }
        Some(anchor)
    }

    pub fn jump_reading(&mut self, anchor: &Anchor, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.reading_index();
        let Some(id) = self.reader.index.resolve(anchor) else { return false };
        if self.core.is_hidden(id) {
            self.core.checkpoint(super::core::EditKind::Other);
            self.core.unfold_to(id);
            self.keep_read_only_fold();
            self.changed_preserving_viewport(window, cx);
        }
        self.scroll_to_block(id, cx);
        if anchor.fraction > 0. { self.reader.fraction = Some((id, anchor.fraction.clamp(0., 1.))); }
        self.reader.seen = None;
        true
    }

    pub(super) fn restore_reading(&mut self) {
        let Some(anchor) = self.reader.restore.take() else { return };
        self.reading_index();
        if let Some(id) = self.reader.index.resolve(&anchor) && let Some(ix) = self.item_of(id) {
            self.list.scroll_to(ListOffset { item_ix: ix, offset_in_item: px(0.) });
            if anchor.fraction > 0. { self.reader.fraction = Some((id, anchor.fraction.clamp(0., 1.))); }
        }
    }

    /// Called after painting; emit from a task so the parent refresh is not dropped.
    pub(super) fn observe_reading(&mut self, cx: &mut Context<Self>) {
        if let Some((id, fraction)) = self.reader.fraction {
            let bounds = self.block_bounds.borrow().get(&id).copied();
            if let Some(b) = bounds && let Some(ix) = self.item_of(id) {
                self.reader.fraction = None;
                self.reader.seen = None;
                self.list.scroll_to(ListOffset { item_ix: ix, offset_in_item: b.size.height * fraction });
                cx.spawn(async |this, cx| { let _ = this.update(cx, |_, cx| cx.notify()); }).detach();
                return;
            }
        }
        if self.preview || (!self.read_only && !self.reader.track) || self.items.is_empty() { return }
        let top = self.list.logical_scroll_top();
        let (offset, content, view) = self.list.estimated_scroll_metrics();
        if view <= px(0.) { return }
        let range = f32::from(content - view).max(0.);
        let progress = if range <= 1. { 1. } else { (f32::from(offset) / range).clamp(0., 1.) };
        let seen = (top.item_ix, (f32::from(top.offset_in_item) / 4.).max(0.) as u32, (progress * 1000.) as u32);
        if self.reader.seen == Some(seen) { return }
        self.reader.seen = Some(seen);
        let Some(anchor) = self.reading_anchor(false) else { return };
        cx.spawn(async move |this, cx| { let _ = this.update(cx, |_, cx| cx.emit(EditorEvent::Reading(anchor, progress))); }).detach();
    }
}
