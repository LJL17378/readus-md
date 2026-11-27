//! A reading view of the existing libraries: no second copy of their files.

use crate::app::{View, Workspace};
use crate::design::{Button, ButtonSize, Card, Progress, Segment, Segmented, Separator, color};
use crate::input::{TextField, TextFieldEvent};
use crate::settings::{display_title, tr};
use crate::store::Entry;
use crate::ui::{TipExt, TitleBarExt, ToastKind};
use fdl_core::reading::{Anchor, Reading, Status};
use gpui::{prelude::FluentBuilder, *};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Page { #[default] Shelf, Bookmarks, Knowledge }
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Filter { #[default] All, Later, Reading, Finished }

pub struct Reader {
    pub page: Page,
    pub filter: Filter,
    pub drawer: bool,
    pub reference_context: Option<String>,
    pub query: Entity<TextField>,
    pub pending: HashMap<String, (Anchor, f32)>,
    scroll: ScrollHandle,
}

impl Reader {
    pub fn new(cx: &mut Context<Workspace>) -> Self {
        let query = cx.new(|cx| TextField::new("搜索标题、正文或书签".to_string(), cx));
        cx.subscribe(&query, |_, _, ev: &TextFieldEvent, cx| { if matches!(ev, TextFieldEvent::Changed) { cx.notify(); } }).detach();
        Self { page: Page::Shelf, filter: Filter::All, drawer: false, reference_context: None, query, pending: HashMap::new(), scroll: ScrollHandle::new() }
    }
}

fn status(s: Status) -> &'static str {
    match s { Status::Later => tr("待读", "To read"), Status::Reading => tr("在读", "Reading"), Status::Finished => tr("已读", "Finished") }
}
fn percent(n: f32) -> String { format!("{}%", (n.clamp(0., 1.) * 100.).round() as u32) }
fn eligible(e: &Entry) -> bool { e.meta.folder.is_none() && e.meta.kind.is_doc() && !e.meta.attachment() }
fn tracked(name: impl Into<SharedString>, control: impl IntoElement) -> Div { div().relative().flex_none().child(control).children(crate::devtools::anchor_probe(name)) }
fn small(s: impl Into<SharedString>) -> Div { div().text_size(px(crate::design::geo().text_xs)).text_color(color::muted_foreground()).child(s.into()) }
fn title(s: impl Into<SharedString>) -> Div { div().text_size(px(16.)).line_height(px(24.)).font_weight(FontWeight::SEMIBOLD).text_color(color::foreground()).child(s.into()) }
fn excerpt(s: &str, n: usize) -> String { s.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(n).collect() }
fn key(prefix: &str, id: &str) -> SharedString { format!("{prefix}-{id}").into() }

impl Workspace {
    fn knowledge_links(&self) -> HashMap<String, HashSet<String>> {
        let docs: Vec<_> = self.entries.iter().filter(|e| eligible(e)).collect();
        let ids: HashSet<_> = docs.iter().map(|e| e.id.as_str()).collect();
        let paths: HashMap<_, _> = docs.iter().filter_map(|e| Some((fdl_core::paths::lexical(&self.store.file_path(&e.id)?), e.id.clone()))).collect();
        let mut links: HashMap<String, HashSet<String>> = HashMap::new();
        for e in &docs {
            for l in &e.links { if ids.contains(l.target.as_str()) && e.id != l.target { links.entry(e.id.clone()).or_default().insert(l.target.clone()); } }
        }
        for (from, path) in self.store.reference_paths() {
            if ids.contains(from.as_str()) && let Some(to) = paths.get(&path) && &from != to { links.entry(from).or_default().insert(to.clone()); }
        }
        links
    }

    fn show_references(&mut self, id: &str, cx: &mut Context<Self>) {
        self.show_library(Page::Knowledge, cx);
        self.reader.reference_context = Some(id.to_string());
        self.reader.query.update(cx, |q, cx| q.set_text("", cx));
        cx.notify();
    }

    #[cfg(feature = "devtools")]
    pub(crate) fn reader_state(&self, cx: &App) -> serde_json::Value {
        let active = self.entries.iter().find(|e| Some(&e.id) == self.current.as_ref());
        let r = active.map(|e| e.meta.reader.clone()).unwrap_or_default();
        let graph = self.knowledge_links();
        let id = self.current.as_deref().unwrap_or("");
        let related = self.reader.reference_context.as_ref().map(|id| self.entries.iter().filter(|e| graph.get(id).is_some_and(|v| v.contains(&e.id)) || graph.get(&e.id).is_some_and(|v| v.contains(id))).count()).unwrap_or(0);
        serde_json::json!({
            "commit": option_env!("READUS_COMMIT").unwrap_or("unknown"),
            "references": graph.get(id).map_or(0, HashSet::len),
            "backlinks": graph.values().filter(|v| v.contains(id)).count(),
            "related_count": related,
            "reading": self.reading(), "shelved": r.shelved, "status": r.status,
            "progress": r.progress, "furthest": r.furthest, "bookmarks": r.bookmarks.len(),
            "shelf_count": self.entries.iter().filter(|e| eligible(e) && e.meta.reader.shelved).count(),
            "position": r.position, "drawer": self.reader.drawer,
            "read_only": self.editor.as_ref().is_some_and(|e| e.read(cx).read_only),
        })
    }

    pub fn show_library(&mut self, page: Page, cx: &mut Context<Self>) {
        self.reader.page = page;
        self.reader.reference_context = None;
        self.reader.scroll.set_offset(Point::default());
        self.show_view(View::Library, cx);
    }

    pub fn reading_progress(&mut self, anchor: Anchor, progress: f32, cx: &mut Context<Self>) {
        if self.preview.is_some() || self.view != View::Doc { return }
        let Some(id) = self.current.clone() else { return };
        if let Some(e) = self.entries.edit().iter_mut().find(|e| e.id == id && eligible(e)) {
            e.meta.reader.visit(anchor.clone(), progress, crate::time::now());
            self.reader.pending.insert(id, (anchor, progress));
            self.schedule_view_state(cx);
            cx.notify();
        }
    }

    pub(crate) fn flush_reading(&mut self) {
        let pending = std::mem::take(&mut self.reader.pending);
        for (id, (anchor, progress)) in pending {
            let at = crate::time::now();
            if let Err(e) = self.store.update_meta(&id, |m| m.reader.visit(anchor.clone(), progress, at)) {
                log::error!("reading: could not save progress: {e:#}");
                self.reader.pending.insert(id, (anchor, progress));
            }
        }
    }

    fn change_reading(&mut self, id: &str, f: impl FnOnce(&mut Reading), cx: &mut Context<Self>) {
        self.flush_reading();
        let mut saved = None;
        let r = self.store.update_meta(id, |m| { f(&mut m.reader); saved = Some(m.reader.clone()); });
        match r {
            Ok(()) => {
                if let Some(reader) = saved && let Some(e) = self.entries.edit().iter_mut().find(|e| e.id == id) { e.meta.reader = reader; }
                if self.current.as_deref() == Some(id) && let Some(ed) = self.editor.clone() {
                    let track = self.entries.iter().find(|e| e.id == id).is_some_and(|e| e.meta.reader.shelved);
                    ed.update(cx, |e, _| e.reader.track = track);
                }
            }
            Err(e) => self.show_toast_kind(ToastKind::Error, &crate::localized_format!("无法保存阅读数据：{}", "Could not save reading data: {}", e), cx),
        }
        cx.notify();
    }

    pub fn toggle_shelf(&mut self, id: &str, cx: &mut Context<Self>) {
        self.change_reading(id, |r| r.shelved = !r.shelved, cx);
    }

    pub fn add_bookmark(&mut self, cx: &mut Context<Self>) {
        if self.preview.is_some() { return }
        let (Some(id), Some(ed)) = (self.current.clone(), self.editor.clone()) else { return };
        if !self.entries.iter().any(|e| e.id == id && eligible(e)) || ed.read(cx).board_open() || ed.read(cx).table_open() { return }
        let Some(anchor) = ed.update(cx, |e, _| e.reading_anchor(true)) else { return };
        self.change_reading(&id, |r| { r.add_bookmark(anchor, crate::time::now()); }, cx);
        self.reader.drawer = true;
        self.shortcuts = false;
        cx.notify();
    }

    fn remove_bookmark(&mut self, id: &str, bookmark: &str, cx: &mut Context<Self>) {
        self.change_reading(id, |r| r.bookmarks.retain(|b| b.id != bookmark), cx);
    }

    fn set_reading_status(&mut self, id: &str, to: Status, cx: &mut Context<Self>) {
        let restart = to == Status::Reading;
        self.change_reading(id, |r| {
            r.status = to;
            if restart { r.position = None; r.progress = 0.; r.furthest = 0.; }
        }, cx);
        if restart {
            self.store.remember_scroll(id, None);
            self.schedule_view_state(cx);
            if self.current.as_deref() == Some(id) && let Some(ed) = self.editor.clone() {
                ed.update(cx, |e, cx| { if let Some(b) = e.core.doc.blocks.first() { e.scroll_to_block(b.id, cx); } });
            }
        }
    }

    fn read_from_shelf(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.try_open(id, window, cx) { self.set_mode(true, window, cx); }
    }

    fn jump_bookmark(&mut self, id: &str, anchor: &Anchor, window: &mut Window, cx: &mut Context<Self>) {
        if !self.try_open(id, window, cx) { return }
        self.set_mode(true, window, cx);
        let ok = self.editor.clone().is_some_and(|ed| ed.update(cx, |e, cx| e.jump_reading(anchor, window, cx)));
        if !ok { self.show_toast_kind(ToastKind::Info, tr("原段落已修改或删除，书签仍为你保留", "This passage changed or was removed. Your bookmark is kept."), cx); }
    }

    pub(crate) fn render_reading_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.preview.is_some() { return None }
        let e = self.entries.iter().find(|e| Some(&e.id) == self.current.as_ref() && eligible(e))?;
        if self.editor.as_ref().is_some_and(|ed| ed.read(cx).board_open() || ed.read(cx).table_open()) { return None }
        let r = &e.meta.reader;
        let id = e.id.clone();
        let reading = self.reading();
        Some(div().flex_none().h(px(44.)).px(px(16.)).flex().items_center().gap(px(8.))
            .child(tracked("reader-mode", Button::ghost("reader-mode").sized(ButtonSize::Sm).icon(if reading { "pen-line" } else { "eye-open" })
                .label(if reading { tr("编辑", "Edit") } else { tr("阅读", "Read") })
                .on_click(cx.listener(move |w, _, window, cx| w.set_mode(!reading, window, cx)))))
            .child(div().flex_1().min_w(px(24.)).max_w(px(160.)).child(Progress::new(r.progress).h(px(3.))))
            .child(small(percent(r.progress)).w(px(34.)))
            .child(div().flex_1())
            .child(tracked("reader-bookmark-add", Button::ghost("reader-bookmark-add").sized(ButtonSize::Sm).icon("reader-bookmark").tip(tr("为选中段落或当前位置添加书签", "Bookmark the selection or current passage"), Some(crate::shortcuts::hint("workspace::AddReadingBookmark", "⌘ + D")))
                .on_click(cx.listener(|w, _, _, cx| w.add_bookmark(cx)))))
            .child(tracked("reader-bookmarks", Button::ghost("reader-bookmarks").sized(ButtonSize::Sm).label(format!("{} {}", tr("书签", "Bookmarks"), r.bookmarks.len())).selected(self.reader.drawer)
                .on_click(cx.listener(|w, _, _, cx| { w.reader.drawer = !w.reader.drawer; w.shortcuts = false; cx.notify(); }))))
            .child(tracked("reader-shelf", Button::ghost("reader-shelf").sized(ButtonSize::Sm).icon("reader-book").selected(r.shelved)
                .tip(if r.shelved { tr("从书架移除（保留文档与阅读数据）", "Remove from shelf (keep document and reading data)") } else { tr("加入书架", "Add to shelf") }, None)
                .on_click(cx.listener(move |w, _, _, cx| w.toggle_shelf(&id, cx)))))
            .into_any_element())
    }

    fn bookmark_row(&self, e: &Entry, b: &fdl_core::reading::Bookmark, document: bool, cx: &mut Context<Self>) -> AnyElement {
        let (id, anchor, remove_id, bookmark) = (e.id.clone(), b.anchor.clone(), e.id.clone(), b.id.clone());
        div().id(key("bookmark", &b.id)).w_full().flex().items_start().gap(px(8.)).py(px(12.))
            .child(Button::ghost(key("jump", &b.id)).sized(ButtonSize::Sm).icon("reader-bookmark")
                .on_click(cx.listener(move |w, _, window, cx| w.jump_bookmark(&id, &anchor, window, cx))))
            .child(div().id(key("passage", &b.id)).flex_1().min_w_0().cursor_pointer().flex().flex_col().gap(px(4.))
                .when(document, |d| d.child(small(display_title(&e.title))))
                .relative().children(crate::devtools::anchor_probe(format!("bookmark:{}", display_title(&e.title))))
                .child(div().text_size(px(crate::design::geo().text)).line_height(px(22.)).text_color(color::foreground())
                    .child(if b.anchor.quote.is_empty() { tr("段落书签", "Passage bookmark").to_string() } else { b.anchor.quote.clone() }))
                .on_click({ let (id, a) = (e.id.clone(), b.anchor.clone()); cx.listener(move |w, _, window, cx| w.jump_bookmark(&id, &a, window, cx)) }))
            .child(Button::ghost(key("remove-bookmark", &b.id)).sized(ButtonSize::Xs).icon("x").tip(tr("移除书签", "Remove bookmark"), None)
                .on_click(cx.listener(move |w, _, _, cx| w.remove_bookmark(&remove_id, &bookmark, cx))))
            .into_any_element()
    }

    pub(crate) fn render_bookmark_drawer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.reader.drawer || self.preview.is_some() { return None }
        let e = self.entries.iter().find(|e| Some(&e.id) == self.current.as_ref() && eligible(e))?;
        let scroll = crate::design::scroll::handle("reader-bookmark-drawer");
        Some(div().w(px(280.)).flex_none().h_full().border_l_1().border_color(color::border()).flex().flex_col()
            .child(div().h(px(52.)).px(px(16.)).flex_none().flex().items_center().justify_between()
                .child(title(tr("书签", "Bookmarks")))
                .child(tracked("reader-drawer-close", Button::ghost("reader-drawer-close").sized(ButtonSize::Sm).icon("x").on_click(cx.listener(|w, _, _, cx| { w.reader.drawer = false; cx.notify(); })))))
            .child(div().id("reader-bookmark-list").relative().flex_1().min_h_0().overflow_y_scroll().track_scroll(&scroll).px(px(16.)).pb(px(16.))
                .when(e.meta.reader.bookmarks.is_empty(), |d| d.child(small(tr("在想回来的段落留下书签。选择文字后，也可以点击下方书签按钮。", "Leave a bookmark where you want to return. You can also select text, then bookmark it.")).py(px(16.))))
                .children(e.meta.reader.bookmarks.iter().map(|b| self.bookmark_row(e, b, false, cx)))
                .child(crate::design::scroll::bar("reader-bookmark-list", &scroll, 0.)))
            .into_any_element())
    }

    fn shelf_card(&self, e: &Entry, cx: &mut Context<Self>) -> AnyElement {
        let r = &e.meta.reader;
        let (id, remove, finish) = (e.id.clone(), e.id.clone(), e.id.clone());
        let completion = if r.status == Status::Finished { 1. } else { r.furthest };
        let next = if r.status == Status::Finished { Status::Reading } else { Status::Finished };
        Card::new(key("shelf-card", &e.id)).w(px(256.)).p(px(20.)).gap(px(16.))
            .child(div().flex().items_center().justify_between()
                .child(crate::design::icon("reader-book", 24., color::muted_foreground()))
                .child(Button::ghost(key("shelf-remove", &e.id)).sized(ButtonSize::Xs).icon("x")
                    .tip(tr("从书架移除，保留文档", "Remove from shelf, keep document"), None)
                    .on_click(cx.listener(move |w, _, _, cx| w.toggle_shelf(&remove, cx)))))
            .child(div().id(key("shelf-cover", &e.id)).h(px(104.)).overflow_hidden().flex().flex_col().gap(px(8.)).cursor_pointer()
                .relative().children(crate::devtools::anchor_probe(format!("shelf:{}", display_title(&e.title))))
                .child(title(display_title(&e.title)))
                .child(small(excerpt(&e.body, 80)))
                .on_click({ let id = e.id.clone(); cx.listener(move |w, _, window, cx| w.read_from_shelf(&id, window, cx)) }))
            .child(div().flex().justify_between().child(small(status(r.status))).child(small(percent(completion))))
            .child(Progress::new(completion).h(px(3.)))
            .child(div().flex().items_center().justify_between()
                .child(Button::secondary(key("shelf-open", &e.id)).sized(ButtonSize::Sm)
                    .label(if r.position.is_some() { tr("继续阅读", "Continue") } else { tr("开始阅读", "Start reading") })
                    .on_click(cx.listener(move |w, _, window, cx| w.read_from_shelf(&id, window, cx))))
                .child(Button::ghost(key("shelf-finish", &e.id)).sized(ButtonSize::Sm)
                    .label(if next == Status::Finished { tr("标记已读", "Mark finished") } else { tr("重新阅读", "Read again") })
                    .on_click(cx.listener(move |w, _, _, cx| w.set_reading_status(&finish, next, cx)))))
            .into_any_element()
    }

    pub(crate) fn render_library(&self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let page = self.reader.page;
        let (name, description) = match page {
            Page::Shelf => (tr("书架", "Bookshelf"), tr("留一本待读，接着上次读。", "Something to read. Somewhere to return.")),
            Page::Bookmarks => (tr("书签", "Bookmarks"), tr("值得再读的段落，都在这里。", "Passages worth returning to, all in one place.")),
            Page::Knowledge => (tr("知识库", "Knowledge library"), tr("工作目录里的文档，通过引用相互连接。", "Documents in your working folders, connected by references.")),
        };
        let query = self.reader.query.read(cx).text.trim().to_lowercase();
        let matches = |e: &&Entry| query.is_empty() || e.title.to_lowercase().contains(&query) || e.body.to_lowercase().contains(&query)
            || e.meta.reader.bookmarks.iter().any(|b| b.anchor.quote.to_lowercase().contains(&query));
        let docs: Vec<_> = self.entries.iter().filter(|e| eligible(e)).filter(matches).collect();
        let mut body = div().flex().flex_col().gap(px(24.));
        if page == Page::Shelf {
            let filter = self.reader.filter;
            let mut tabs = Segmented::new();
            for (i, (f, label)) in [(Filter::All, tr("全部", "All")), (Filter::Later, tr("待读", "To read")), (Filter::Reading, tr("在读", "Reading")), (Filter::Finished, tr("已读", "Finished"))].into_iter().enumerate() {
                tabs = tabs.child(Segment::new(("shelf-filter", i), label).selected(filter == f)
                    .on_click(cx.listener(move |w, _, _, cx| { w.reader.filter = f; cx.notify(); })));
            }
            body = body.child(div().flex().child(tabs));
            let mut books: Vec<_> = docs.iter().copied().filter(|e| e.meta.reader.shelved && match filter {
                Filter::All => true, Filter::Later => e.meta.reader.status == Status::Later,
                Filter::Reading => e.meta.reader.status == Status::Reading, Filter::Finished => e.meta.reader.status == Status::Finished,
            }).collect();
            books.sort_by_key(|e| (std::cmp::Reverse(e.meta.reader.last_read), e.title.clone()));
            if books.is_empty() {
                body = body.child(self.reader_empty(tr("书架为下一次阅读留着位置", "Make room for your next read"), tr("打开文档，点击底部的书本按钮加入书架；也可以从知识库选择。", "Open a document and use the book button below, or choose one from your library."), cx));
            } else {
                if filter == Filter::All {
                    let recent: Vec<_> = books.iter().copied().filter(|e| e.meta.reader.status == Status::Reading && e.meta.reader.position.is_some()).take(3).collect();
                    if !recent.is_empty() {
                        body = body.child(title(tr("继续阅读", "Continue reading"))).child(div().id("shelf-continue").flex().flex_wrap().gap(px(20.)).children(recent.iter().map(|e| self.shelf_card(e, cx))));
                        let shown: HashSet<_> = recent.iter().map(|e| e.id.clone()).collect();
                        books.retain(|e| !shown.contains(&e.id));
                        if !books.is_empty() { body = body.child(Separator::new()).child(title(tr("我的书架", "My shelf"))); }
                    }
                }
                body = body.child(div().id("shelf-all").flex().flex_wrap().gap(px(20.)).children(books.iter().map(|e| self.shelf_card(e, cx))));
            }
        } else if page == Page::Bookmarks {
            let mut bookmarks: Vec<_> = docs.iter().flat_map(|e| e.meta.reader.bookmarks.iter().map(move |b| (*e, b))).filter(|(_, b)| query.is_empty() || b.anchor.quote.to_lowercase().contains(&query) || docs.iter().any(|e| e.title.to_lowercase().contains(&query) && e.meta.reader.bookmarks.iter().any(|x| x.id == b.id))).collect();
            bookmarks.sort_by_key(|(_, b)| std::cmp::Reverse(b.created));
            if bookmarks.is_empty() { body = body.child(self.reader_empty(tr("把想重读的段落留下来", "Keep a passage for later"), tr("在文档底部点击书签按钮，书签会自动归集到这里。", "Use the bookmark button below a document. Your bookmarks collect here."), cx)); }
            for (e, b) in bookmarks { body = body.child(self.bookmark_row(e, b, true, cx)).child(Separator::new()); }
        } else {
            let mut folders = div().flex().flex_wrap().gap(px(16.));
            for (k, name, _, _) in self.store.shown_tops() {
                let root = crate::shared_store::top_row(&k);
                let n = self.entries.iter().filter(|e| e.top == k && eligible(e)).count();
                folders = folders.child(Card::new(key("knowledge-folder", &k)).w(px(256.)).p(px(20.)).gap(px(12.))
                    .child(crate::design::icon("folder", 22., color::muted_foreground())).child(title(name))
                    .child(small(crate::localized_format!("{} 篇文档", "{} documents", n)))
                    .child(Button::ghost(key("knowledge-open", &k)).sized(ButtonSize::Sm).label(tr("浏览目录", "Browse folder"))
                        .on_click(cx.listener(move |w, _, window, cx| w.open(&root, window, cx)))));
            }
            let graph = self.knowledge_links();
            let context = self.reader.reference_context.as_deref();
            if let Some(id) = context {
                let name = self.entries.iter().find(|e| e.id == id).map(|e| display_title(&e.title)).unwrap_or_default();
                body = body.child(div().flex().items_center().justify_between().gap(px(16.)).child(title(name))
                    .child(Button::ghost("knowledge-back").sized(ButtonSize::Sm).icon("arrow-left").label(tr("全部文档", "All documents"))
                        .on_click(cx.listener(|w, _, _, cx| { w.reader.reference_context = None; cx.notify(); }))));
            } else { body = body.child(folders).child(title(tr("文档与引用", "Documents & references"))); }
            let mut incoming: HashMap<&str, usize> = HashMap::new();
            for targets in graph.values() { for id in targets { *incoming.entry(id).or_default() += 1; } }
            let mut sorted: Vec<_> = docs.into_iter().filter(|e| context.is_none_or(|id| graph.get(id).is_some_and(|v| v.contains(&e.id)) || graph.get(&e.id).is_some_and(|v| v.contains(id)))).collect();
            sorted.sort_by_key(|e| (std::cmp::Reverse(e.meta.opened), e.title.clone()));
            if sorted.is_empty() { body = body.child(self.reader_empty(tr("从一个工作目录开始", "Start with a working folder"), tr("添加现有文件夹即可浏览文档。用 @ 引用其他文档，反向引用会自动显示在文末。", "Add an existing folder to browse its documents. Mention a document with @; backlinks appear below it."), cx)); }
            for e in sorted {
                let (id, add, refs) = (e.id.clone(), e.id.clone(), e.id.clone());
                body = body.child(div().flex().items_center().gap(px(16.))
                    .child(div().id(key("knowledge-doc", &e.id)).flex_1().min_w_0().flex().flex_col().gap(px(4.)).cursor_pointer()
                        .child(title(display_title(&e.title)))
                        .child(small(crate::localized_format!("引用 {} 篇 · 被引用 {} 次", "{} referenced documents · {} backlinks", graph.get(&e.id).map_or(0, HashSet::len), incoming.get(e.id.as_str()).copied().unwrap_or(0))))
                        .child(small(excerpt(&e.body, 110)))
                        .on_click(cx.listener(move |w, _, window, cx| w.open(&id, window, cx))))
                    .child(tracked(format!("references:{}", display_title(&e.title)), Button::ghost(key("knowledge-refs", &e.id)).sized(ButtonSize::Sm)
                        .label(tr("查看引用", "References")).disabled(graph.get(&e.id).is_none_or(HashSet::is_empty) && !incoming.contains_key(e.id.as_str()))
                        .on_click(cx.listener(move |w, _, _, cx| w.show_references(&refs, cx)))))
                    .child(Button::ghost(key("knowledge-shelve", &e.id)).sized(ButtonSize::Sm).icon("reader-book").selected(e.meta.reader.shelved)
                        .label(if e.meta.reader.shelved { tr("已在书架", "On shelf") } else { tr("加入书架", "Add to shelf") })
                        .on_click(cx.listener(move |w, _, _, cx| w.toggle_shelf(&add, cx)))))
                    .child(Separator::new());
            }
        }
        let scroll = &self.reader.scroll;
        div().flex_1().min_w_0().h_full().flex().flex_col()
            .child(div().h(px(crate::app::HEADER_H)).flex_none().px(px(24.)).flex().items_center().gap(px(8.)).title_bar()
                .when(!self.sidebar_open, |d| d.pl(px(crate::theme::TRAFFIC_LIGHTS_W))
                    .child(Button::ghost("library-sidebar").no_drag().icon("sidebar").on_click(cx.listener(|w, _, _, cx| { w.sidebar_open = true; cx.notify(); }))))
                .child(div().flex_1()).child(Button::ghost("library-add-folder").no_drag().icon("folder-plus").label(tr("添加工作目录", "Add working folder"))
                    .on_click(cx.listener(|w, _, window, cx| w.open_folder_dialog(window, cx)))))
            .child(div().id("reading-library").relative().flex_1().min_h_0().overflow_y_scroll().track_scroll(scroll).px(px(32.)).pb(px(32.))
                .child(div().w_full().max_w(px(1120.)).mx_auto().flex().flex_col().gap(px(24.))
                    .child(div().pt(px(16.)).flex().flex_col().gap(px(8.))
                        .child(div().text_size(px(24.)).line_height(px(32.)).font_weight(FontWeight::SEMIBOLD).text_color(color::foreground()).child(name))
                        .child(small(description)))
                    .child(div().max_w(px(360.)).child(crate::design::Field::new("library-search-field").child(self.reader.query.clone())))
                    .child(body))
                .child(crate::design::scroll::bar("reading-library", scroll, 0.)))
            .into_any_element()
    }

    fn reader_empty(&self, heading: &'static str, description: &'static str, cx: &mut Context<Self>) -> Div {
        div().py(px(48.)).flex().flex_col().items_center().gap(px(16.))
            .child(crate::design::icon("reader-book", 32., color::muted_foreground()))
            .child(title(heading)).child(small(description).max_w(px(400.)).text_center())
            .child(Button::outline("reader-empty-library").label(if self.reader.page == Page::Knowledge { tr("添加工作目录", "Add working folder") } else { tr("浏览知识库", "Browse library") })
                .on_click(cx.listener(|w, _, window, cx| if w.reader.page == Page::Knowledge { w.open_folder_dialog(window, cx) } else { w.show_library(Page::Knowledge, cx) })))
    }
}
