//! 首次启动: Readus.md over softly moving flat book spines, followed by two pages —
//! where the library lives and the quick-note shortcut, then notes to bring
//! along. ⌘⌥⇧O and Settings › About replay it without touching the library. Nothing is written until 开始使用;
//! closing the window before that quits without a library.

use crate::settings::tr;
use crate::theme::{self, Theme};
use crate::ui;
use crate::design::{Button, ButtonSize, Card, Checkbox, Field, FieldSize, Separator};
use gpui::*;
use gpui::prelude::FluentBuilder;
use std::path::{Path, PathBuf};
use crate::import_sources::{discover, Found};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use web_time::Instant;

static PENDING: AtomicBool = AtomicBool::new(false);

/// First launch's window is open and no library is chosen yet.
pub fn pending() -> bool { PENDING.load(Ordering::Relaxed) }

/// A new library gets the pages; `QUIET_MD_ONBOARDING` shows them anyway (previews, screenshots).
pub fn wanted(existed: bool, scripted: bool) -> bool {
    std::env::var_os("QUIET_MD_ONBOARDING").is_some() || (!existed && !scripted)
}

// ---- the reading backdrop

const HOLD: f32 = 1.6;
const MARK_IN: (f32, f32) = (0.1, 0.6);
const FOUND_SHOWN: usize = 4;
const FLOW_FPS: f32 = 24.;

/// Project artwork: flat book spines, with no video textures or perspective.
/// Blue #2A72D1 is sampled from assets/branding/app-icon.png at (512, 120).
pub(crate) fn paint_backdrop(b: Bounds<Pixels>, t: f32, window: &mut Window) {
    let (x, y, w, h) = (f32::from(b.left()), f32::from(b.top()), f32::from(b.size.width), f32::from(b.size.height));
    let blue = Hsla::from(rgb(0x2a72d1));
    let dark = theme::dark();
    window.paint_quad(fill(b, crate::design::color::background()));
    window.paint_quad(fill(b, blue.opacity(if dark { 0.16 } else { 0.07 })));
    let travel = if crate::settings::motion_high() { 8. } else { 2. };
    for (side, cy, scale) in [(0., 0.28, 0.9), (1., 0.76, 1.1)] {
        let width = 176. * scale;
        let at = if w < 1000. { if side == 0. { -92. } else { w - 60. } } else if side == 0. { w * 0.055 } else { w * 0.945 - width };
        for i in 0..5 {
            let phase = t / 5. + i as f32 * 0.7 + side * 2.;
            let bx = x + at + if i % 2 == 0 { 0. } else { 18. * scale } + phase.sin() * travel;
            let by = y + h * cy + (i as f32 - 2.) * 35. * scale;
            let cover = Bounds::new(point(px(bx), px(by)), size(px(width), px(29. * scale)));
            window.paint_quad(fill(cover, blue.opacity(if dark { 0.22 } else { 0.12 })).corner_radii(px(5. * scale)));
            let page = Bounds::new(point(px(bx + 8. * scale), px(by + 5. * scale)), size(px(width - 13. * scale), px(19. * scale)));
            window.paint_quad(fill(page, crate::design::color::background().opacity(if dark { 0.28 } else { 0.72 })).corner_radii(px(2. * scale)));
            for line in [11., 17.] {
                window.paint_quad(fill(Bounds::new(point(px(bx + 15. * scale), px(by + line * scale)), size(px(width - 30. * scale), px(scale))), blue.opacity(0.1)));
            }
        }
    }
}

fn wordmark(alpha: f32) -> Div {
    div().absolute().inset_0().flex().flex_col().items_center().justify_center().gap(px(20.)).opacity(alpha)
        .child(img(crate::settings::about_icon()).size(px(88.)))
        .child(div().text_size(px(64.)).line_height(px(76.)).font_weight(FontWeight::SEMIBOLD).text_color(crate::design::color::foreground()).child("Readus.md"))
}

// ---- syncing

/// Whether macOS keeps `path` (or the nearest folder above it that exists)
/// in iCloud; this also covers Documents with 桌面与文稿 syncing on.
#[cfg(target_os = "macos")]
pub(crate) fn ubiquitous(path: &Path) -> bool {
    use fdl_core::vfs::PathExt as _;
    use objc::runtime::{BOOL, NO, Object};
    use objc::{class, msg_send, sel, sel_impl};
    #[link(name = "Foundation", kind = "framework")]
    unsafe extern "C" { static NSURLIsUbiquitousItemKey: *mut Object; }
    let mut p = path;
    while !p.fs_exists() { match p.parent() { Some(q) => p = q, None => return false } }
    let s = p.to_string_lossy();
    unsafe {
        let ns: *mut Object = msg_send![class!(NSString), alloc];
        let ns: *mut Object = msg_send![ns, initWithBytes:s.as_ptr() length:s.len() encoding:4usize];
        let url: *mut Object = msg_send![class!(NSURL), fileURLWithPath:ns];
        let _: () = msg_send![ns, release];
        let mut value: *mut Object = std::ptr::null_mut();
        let ok: BOOL = msg_send![url, getResourceValue:&mut value forKey:NSURLIsUbiquitousItemKey error:std::ptr::null_mut::<*mut Object>()];
        if ok == NO || value.is_null() { return false }
        let on: BOOL = msg_send![value, boolValue];
        on != NO
    }
}

/// A path as people know it: 文稿 › Quiet.md, iCloud 云盘 › Notes, Dropbox › …;
/// deep ones keep their first and last two names.
pub(crate) fn short_path(p: &Path) -> String {
    let home = dirs::home_dir().unwrap_or_default();
    let places: [(PathBuf, &str); 5] = [
        (home.join("Library/Mobile Documents/com~apple~CloudDocs"), tr("iCloud 云盘", "iCloud Drive")),
        (dirs::document_dir().unwrap_or_else(|| home.join("Documents")), tr("文稿", "Documents")),
        (dirs::desktop_dir().unwrap_or_else(|| home.join("Desktop")), tr("桌面", "Desktop")),
        (dirs::download_dir().unwrap_or_else(|| home.join("Downloads")), tr("下载", "Downloads")),
        (home.join("Library/Mobile Documents"), "iCloud"),
    ];
    let names = |r: &Path| r.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>();
    let (base, rest): (String, Vec<String>) = if let Some((b, r)) = places.iter().find_map(|(b, n)| p.strip_prefix(b).ok().map(|r| (n.to_string(), names(r)))) { (b, r) }
        else if let Ok(r) = p.strip_prefix(home.join("Library/CloudStorage")) {
            let mut r = names(r);
            let provider = if r.is_empty() { String::new() } else { r.remove(0) };
            (provider.split('-').next().unwrap_or(&provider).to_string(), r)
        }
        else if let Ok(r) = p.strip_prefix(&home) { ("~".into(), names(r)) }
        else { (String::new(), names(p).into_iter().filter(|c| c != "/").collect()) };
    let rest = if rest.len() > 3 { vec![rest[0].clone(), "…".into(), rest[rest.len() - 2].clone(), rest[rest.len() - 1].clone()] } else { rest };
    let mut parts = vec![];
    if !base.is_empty() { parts.push(base) }
    parts.extend(rest);
    parts.join(" › ")
}

/// A picked folder as a library, used as it is: its Markdown and DocxXML
/// files show up as documents (#282). Only the home folder and a disk's
/// top-level folder get a Quiet Notes folder inside; scanning all of either is never meant.
pub(crate) fn library_in(dir: PathBuf) -> PathBuf {
    let broad = dir.parent().is_none() || dirs::home_dir().is_some_and(|h| h == dir);
    if !broad || crate::store::Store::is_library(&dir) { return dir }
    // one builds before 2026-11-26 made there (`Quiet.md`) is the one
    let old = dir.join(fdl_core::store::OLD_DEFAULT_NAME);
    if crate::store::Store::is_library(&old) { old } else { dir.join(fdl_core::store::DEFAULT_NAME) }
}

// ---- the view

#[derive(Clone, Copy, PartialEq)]
enum Page { Setup, Import }

#[derive(Clone, Copy, PartialEq)]
enum Pick { New, Open, More }

pub struct Onboarding {
    focus: FocusHandle,
    opened: Instant,
    /// When the opening mark gives way to the pages (sooner on a key or click).
    cut_at: Instant,
    /// Over a running app's window (⌘⌥⇧O, Settings): a look only, nothing is written.
    replay: bool,
    page: Page,
    /// 新建文件夹: where a new, empty top-level folder would go.
    root: PathBuf,
    /// 添加文件夹: folders already on this computer, opened where they are
    /// (W12, the main choice); none means the new folder above.
    open: Vec<PathBuf>,
    /// The found and picked folders go in as copies (导入副本), not opened.
    copy: bool,
    /// 界面语言: the system's at first, applied as it is picked.
    lang: crate::settings::Lang,
    /// The language menu is open, with this one under the keys.
    lang_menu: Option<crate::settings::Lang>,
    lang_anchor: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    hotkey: String,
    recording: bool,
    hotkey_error: Option<&'static str>,
    picked: Vec<PathBuf>,
    found: Option<Vec<Found>>,
    /// 再显示 was pressed: every found folder is listed.
    show_all: bool,
    chosen: Vec<bool>,
    error: Option<String>,
    scripted: bool,
    incoming: Option<crate::local_files::Incoming>,
    /// 导入并开始 is copying notes in (on a background thread); since when.
    importing: Option<Instant>,
    _ticker: Task<()>,
}

pub fn open(scripted: bool, incoming: crate::local_files::Incoming, cx: &mut App) {
    PENDING.store(true, Ordering::Relaxed);
    crate::shortcuts::init(cx);
    let options = crate::main_window_options(scripted, None, cx);
    cx.open_window(options, |window, cx| {
        crate::devtools::start(window, cx);
        cx.new(|cx| Onboarding::new(scripted, incoming, false, window, cx))
    }).expect("open window");
}

/// A replay's last page is done; the window it covers takes over again.
pub struct Done;
impl EventEmitter<Done> for Onboarding {}

impl Onboarding {
    /// ⌘⌥⇧O and Settings › About › Onboarding: the pages again, laid over a
    /// main window. Only the shortcut is applied; the library stays as it is.
    pub(crate) fn replay(window: &mut Window, cx: &mut Context<Self>) -> Self { Self::new(false, Default::default(), true, window, cx) }

    fn new(scripted: bool, incoming: crate::local_files::Incoming, replay: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let fps = FLOW_FPS;
        let ticker = cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_secs_f32(1. / fps)).await;
            if this.update(cx, |_, cx| cx.notify()).is_err() { break }
        });
        let root = crate::store::Store::active_dir();
        let lib = root.clone();
        cx.spawn(async move |this, cx| {
            let found = cx.background_executor().spawn(async move { discover(&[lib]) }).await;
            let _ = this.update(cx, |w, cx| { w.chosen = vec![true; found.len()]; w.found = Some(found); cx.notify(); });
        }).detach();
        let now = Instant::now();
        let cut_at = now + Duration::from_secs_f32(HOLD);
        Onboarding {
            focus,
            opened: now, cut_at, replay, page: Page::Setup, root, open: vec![], copy: false,
            lang: crate::settings::prefs().lang(), lang_menu: None, lang_anchor: Default::default(),
            hotkey: crate::shortcuts::global_key(), recording: false, hotkey_error: None,
            picked: vec![], found: None, show_all: false, chosen: vec![], error: None, scripted, incoming: Some(incoming), importing: None, _ticker: ticker,
        }
    }

    /// A key or click while the opening mark plays cuts to the pages now.
    fn cut(&mut self, cx: &mut Context<Self>) -> bool {
        let now = Instant::now();
        if now >= self.cut_at { return false }
        self.cut_at = now;
        cx.notify();
        true
    }

    fn go(&mut self, page: Page, cx: &mut Context<Self>) {
        self.stop_recording(cx);
        self.page = page;
        cx.notify();
    }

    /// 下一步 / 开始使用, from the button or ↩.
    fn advance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.page == Page::Setup { self.go(Page::Import, cx) } else { self.finish(window, cx) }
    }

    /// 跳过: the defaults, nothing imported.
    fn skip(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.picked.clear();
        self.chosen.iter_mut().for_each(|c| *c = false);
        self.finish(window, cx);
    }

    /// `Pick::New`: where the new folder goes; `Pick::Open`: folders to open
    /// where they are; `Pick::More`: one more folder for the second page.
    fn pick_folder(&mut self, what: Pick, window: &mut Window, cx: &mut Context<Self>) {
        let _ = window;
        let picked = crate::devtools::take_pick();
        let prompt = picked.is_none().then(|| cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: what == Pick::Open, prompt: Some(tr("选择", "Choose").into()), extensions: None }));
        cx.spawn(async move |this, cx| {
            let paths = match prompt { Some(p) => p.await.ok().and_then(Result::ok).flatten(), None => picked };
            let Some(dirs) = paths.filter(|p| !p.is_empty()) else { return };
            let _ = this.update(cx, |w, cx| {
                match what {
                    Pick::New => { w.root = library_in(dirs[0].clone()); w.open.clear(); }
                    Pick::Open => for d in dirs { if !w.open.contains(&d) { w.open.push(d); } },
                    Pick::More => for d in dirs { if !w.picked.contains(&d) { w.picked.push(d); } },
                }
                w.error = None;
                cx.notify();
            });
        }).detach();
    }

    fn start_recording(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.recording = true;
        self.hotkey_error = None;
        window.focus(&self.focus, cx);
        cx.clear_key_bindings();
        cx.notify();
    }

    fn stop_recording(&mut self, cx: &mut Context<Self>) {
        if !self.recording { return }
        self.recording = false;
        crate::shortcuts::rebuild(cx);
        cx.notify();
    }

    fn key_down(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(at) = self.lang_menu {
            use crate::settings::Lang;
            let i = Lang::ALL.iter().position(|l| *l == at).unwrap_or(0);
            match ev.keystroke.key.as_str() {
                "escape" => self.lang_menu = None,
                "up" => self.lang_menu = Some(Lang::ALL[i.saturating_sub(1)]),
                "down" => self.lang_menu = Some(Lang::ALL[(i + 1).min(Lang::ALL.len() - 1)]),
                "enter" | "space" => self.set_lang(at, cx),
                _ => {}
            }
            cx.stop_propagation();
            return cx.notify();
        }
        if !self.recording {
            if self.cut(cx) { return }
            if ev.keystroke.key == "enter" && ev.keystroke.modifiers == Modifiers::default() { self.advance(window, cx); }
            return
        }
        cx.stop_propagation();
        if ev.is_held { return }
        let k = &ev.keystroke;
        let plain = k.modifiers == Modifiers::default();
        if plain && k.key == "escape" { return self.stop_recording(cx) }
        if matches!(k.key.as_str(), "shift" | "alt" | "ctrl" | "cmd" | "fn" | "control" | "platform" | "function") { return }
        let m = k.modifiers;
        let key = k.unparse(crate::shortcuts::HOST_OS);
        if !(m.platform || m.control || m.alt) { self.hotkey_error = Some(tr("请包含 ⌘、⌃ 或 ⌥", "Include ⌘, ⌃ or ⌥")); }
        else if key == "cmd-alt-space" || key == "alt-cmd-space" { self.hotkey_error = Some(tr("⌘ ⌥ Space 用于打开上次的文档，请换一个", "⌘ ⌥ Space opens the last document; pick another")); }
        else { self.hotkey = key; self.hotkey_error = None; return self.stop_recording(cx) }
        cx.notify();
    }

    /// Show the pages in `lang` now. Saved with the library at the end; a
    /// replay saves it at once, since the library is already there.
    fn set_lang(&mut self, lang: crate::settings::Lang, cx: &mut Context<Self>) {
        self.lang_menu = None;
        self.lang = lang;
        if self.replay { save_lang(lang); } else { crate::settings::show_lang(lang); }
        cx.notify();
    }

    /// Each folder to import, and the name it takes in the library.
    /// Nothing left to bring in: 导入副本 has nothing to copy, so it is off again (its card is gone).
    fn sync_copy(&mut self) { if self.imports().is_empty() { self.copy = false; } }

    fn imports(&self) -> Vec<(PathBuf, Option<String>)> {
        let mut out: Vec<_> = self.picked.iter().map(|p| (p.clone(), None)).collect();
        if let Some(found) = &self.found {
            out.extend(found.iter().zip(&self.chosen).filter(|(_, on)| **on).map(|(f, _)| (f.path.clone(), Some(f.name.clone()))));
        }
        out
    }

    /// 开始使用: settle the library, then hand over to the main window.
    fn finish(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.importing.is_some() { return }
        self.stop_recording(cx);
        if self.replay {
            if cfg!(target_os = "macos") && self.hotkey != crate::shortcuts::global_key() { let _ = crate::shortcuts::set_global(&self.hotkey, cx); }
            self._ticker = Task::ready(());
            return cx.emit(Done);
        }
        // The folders picked open where they are, else a new one (W12).
        let fixed = std::env::var_os("QUIET_MD_HOME").is_some();
        // A test run's one folder (`QUIET_MD_HOME`) is the top-level folder; it gets the guides as a new folder would.
        let home = crate::store::Store::legacy_dir();
        let new_top = (self.open.is_empty() && !fixed).then(|| self.root.clone());
        let existed = crate::guides::seeded(new_top.as_ref().unwrap_or(&home)).unwrap_or(false);
        if fixed && !self.scripted { let _ = crate::guides::initialize(&home); }
        let mut ws = fdl_core::home::Home::load();
        let mut failed: Vec<String> = vec![];
        if let Some(root) = &new_top {
            let made = if self.scripted { Ok(()) } else { crate::guides::initialize(root) };
            if let Err(e) = made.map_err(|e| e.to_string()).and_then(|_| ws.add(root).map(|_| ()).or_else(|e| match e { fdl_core::home::AddError::Open(_) => Ok(()), e => Err(e.to_string()) })) {
                self.error = Some(format!("{}{e}", tr("无法在这里创建文件夹：", "Couldn't create the folder here: ")));
                return cx.notify();
            }
        }
        // 思源笔记 can only come in as copies, whichever way the rest go.
        let siyuan: Vec<(PathBuf, Option<String>)> = self.imports().into_iter().filter(|(p, _)| fdl_core::siyuan::recognizes(p)).collect();
        let others: Vec<PathBuf> = self.open.iter().cloned().chain(if self.copy && new_top.is_some() { vec![] } else { self.imports().into_iter().map(|(p, _)| p).filter(|p| !siyuan.iter().any(|(s, _)| s == p)).collect() }).collect();
        for d in others {
            if let Err(e) = ws.add(&d) && !matches!(e, fdl_core::home::AddError::Open(_) | fdl_core::home::AddError::Inside(_)) { failed.push(format!("{}：{e}", short_path(&d))); }
        }
        PENDING.store(false, Ordering::Relaxed);
        let target = new_top.clone();
        cx.set_global(crate::shared_store::SharedStore::with_workspace(ws));
        crate::settings::load();
        save_lang(self.lang);
        crate::settings::apply_appearance(&crate::settings::prefs().appearance);
        crate::settings::sync_dark(cx);
        crate::themes::load(&crate::settings::prefs().theme_id(), false, || cx.text_system().all_font_names());
        let open_error = (!failed.is_empty()).then(|| crate::localized_format!("有文件夹没能打开：{}", "Some folders could not be opened: {}", failed.join("；")));
        let imports = if self.copy { self.imports() } else { siyuan.clone() };
        let target = target.or_else(|| (!siyuan.is_empty()).then(|| self.open.first().cloned().or_else(|| fixed.then(|| home.clone()))).flatten());
        let Some(target) = target.filter(|_| !imports.is_empty()) else { return self.hand_over(existed, open_error, window, cx); };
        // Copy the notes in before the main window opens, off the main thread,
        // so the button's spinner keeps moving.
        self.importing = Some(Instant::now());
        cx.notify();
        let work = cx.background_executor().spawn(async move { import_all(&target, &imports) });
        cx.spawn_in(window, async move |this, cx| {
            let failed = work.await;
            let _ = this.update_in(cx, |w, window, cx| w.hand_over(existed, failed.or(open_error), window, cx));
        }).detach();
    }

    /// The notes take over this same window: no second window, nothing jumps.
    fn hand_over(&mut self, existed: bool, import_error: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self._ticker = Task::ready(());
        let ws = crate::start(existed, self.scripted, Some(window), self.incoming.take().unwrap_or_default(), cx);
        let hotkey_error = (cfg!(target_os = "macos") && self.hotkey != crate::shortcuts::global_key())
            .then(|| crate::shortcuts::set_global(&self.hotkey, cx).err()).flatten().map(|e| e.to_string());
        if let Some(e) = hotkey_error.or(import_error) && let Some(ws) = ws {
            ws.update(cx, |w, cx| w.show_toast_kind(ui::ToastKind::Error, &e, cx));
        }
    }

    // ---- pages

    /// Where notes live and the quick-note shortcut: the title on the left,
    /// the two settings as cards floating in on the right.
    fn setup_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let anchor = self.lang_anchor.clone();
        let open = self.lang_menu.is_some();
        let current = self.lang;
        let lang = card("onb-lang-card")
            .child(card_icon("globe"))
            .child(card_text(tr("界面语言", "Language").into(), tr("默认跟随系统，之后可以在设置里更改", "Follows the system; you can change it in Settings").into(), false))
            .child(Field::select("onb-lang").sized(FieldSize::Sm).open(open).child(ui::ui_text(current.name()))
                .child(canvas(move |b, _, _| anchor.set(Some(b)), |_, _, _, _| {}).absolute().inset_0())
                .relative()
                .on_click(cx.listener(move |w, _, _, cx| { w.lang_menu = if open { None } else { Some(current) }; cx.notify(); })));
        // Where notes live (W12): a folder already here, opened where it is, comes
        // first; a new folder is the other choice, ticked while none is picked.
        let opening = !self.open.is_empty();
        let open_desc: SharedString = if opening { self.open.iter().map(|p| short_path(p)).collect::<Vec<_>>().join("、").into() }
            else { tr("用电脑上已有的文件夹，比如 Obsidian 库或 git 仓库，文件就地编辑", "Use a folder you already have, such as an Obsidian vault or a git repository; files are edited where they are").into() };
        let open_card = card("onb-open-card")
            .child(card_icon("folder-input"))
            .child(card_text(tr("添加文件夹", "Add a folder").into(), open_desc, false))
            .child(soft_button("onb-open", if opening { tr("再选…", "Add…") } else { tr("添加…", "Add…") }).on_click(cx.listener(|w, _, window, cx| w.pick_folder(Pick::Open, window, cx))))
            .when(opening, |d| d.child(soft_button("onb-open-clear", tr("清除", "Clear")).on_click(cx.listener(|w, _, _, cx| { w.open.clear(); cx.notify(); }))));
        let lib = card("onb-lib-card")
            .when(opening, |d| d.opacity(0.55))
            .child(card_icon("folder"))
            .child(card_text(crate::localized_format!("新建文件夹 · {}", "New folder · {}", short_path(&self.root)).into(), (if cfg!(target_os = "macos") { tr("笔记以文件保存在这里，在访达里也能直接打开", "Notes are saved here as files you can open in Finder") }
                else if cfg!(target_os = "windows") { tr("笔记以文件保存在这里，在文件资源管理器里也能直接打开", "Notes are saved here as files you can open in File Explorer") }
                else { tr("笔记以文件保存在这里，在文件管理器里也能直接打开", "Notes are saved here as files you can open in your file manager") }).into(), false))
            .child(soft_button("onb-change", tr("更改…", "Change…")).on_click(cx.listener(|w, _, window, cx| w.pick_folder(Pick::New, window, cx))));
        let hotkey = cfg!(target_os = "macos").then(|| {
            let key = if self.recording { String::new() } else { self.hotkey.clone() };
            let (desc, error) = match (self.hotkey_error, self.recording) {
                (Some(e), _) => (e, true),
                (None, true) => (tr("按下新的组合键 · Esc 取消", "Press a new shortcut · Esc to cancel"), false),
                (None, false) => (tr("在任何应用里按下，打开一个小窗口记下想法", "Press it in any app to jot a thought in a small window"), false),
            };
            card("onb-hotkey-card")
                .child(card_icon("keyboard"))
                .child(card_text(tr("随手记", "Quick note").into(), desc.into(), error))
                .child(crate::settings::keys_field("onb-hotkey", &key, self.recording, tr("按下组合键…", "Press keys…")).min_w(px(96.))
                    .on_click(cx.listener(|w, _, window, cx| w.start_recording(window, cx))))
        });
        // Top to bottom: the app's icon, its name, one line, then the settings.
        // Centred while it fits (48 above, 88 below: where `pb(40)` alone put it); taller, it starts at the
        // top and scrolls, as the import page does.
        div().id("onb-setup-scroll").size_full().overflow_y_scroll().track_scroll(&crate::design::scroll::handle("onb-setup-scroll")).flex().flex_col().items_center().px(px(48.)).pt(px(48.)).pb(px(88.))
            .child(div().my_auto().flex_none().w_full().flex().flex_col().items_center()
            .child(div().flex().flex_col().items_center().gap(px(12.))
                .child(div().size(px(64.)).rounded(px(15.)).overflow_hidden().shadow(theme::shadow_card()).child(img(crate::settings::about_icon()).size_full()))
                .child(lifted(56., 64., true, false, "Readus.md"))
                .child(lifted(15., 23., false, true, tr("安静阅读，随时续读，让文档彼此连接。", "Read quietly. Return easily. Connect your knowledge."))))
            .child(div().w_full().max_w(px(560.)).mt(px(40.)).flex().flex_col().gap(px(12.))
                .child(lang)
                .child(open_card)
                .child(lib)
                .children(hotkey)
                .children(self.error.clone().map(|e| note_owned(e, true)))))
            .children(self.lang_menu(cx))
            // the shared scrollbar, clear of the window's corners at both ends (48 of room beside the column)
            .child({ crate::design::scroll::solid("onb-setup-scroll"); crate::design::scroll::bar_corners("onb-setup-scroll", &crate::design::scroll::handle("onb-setup-scroll"), crate::design::scroll::WINDOW_R, crate::design::scroll::WINDOW_R) })
            .into_any_element()
    }

    /// 界面语言's menu under its button, each language in its own name.
    fn lang_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        use crate::settings::Lang;
        let focused = self.lang_menu?;
        let b = self.lang_anchor.get()?;
        let rows = Lang::ALL.map(|l| {
            let g = SharedString::from(format!("onb-lang-{}", l.code()));
            crate::design::MenuItem::new(g).label(l.name()).selected(l == focused).checked(l == self.lang)
                .on_hover(cx.listener(move |w, hovered: &bool, _, cx| if *hovered && w.lang_menu != Some(l) { w.lang_menu = Some(l); cx.notify(); }))
                .on_click(cx.listener(move |w, _, _, cx| w.set_lang(l, cx)))
        });
        let anchor = self.lang_anchor.clone();
        let menu = crate::design::Popover::new("onb-lang-menu").menu().w(px(crate::design::menu_w::SM))
            .on_mouse_down_out(cx.listener(move |w, ev: &MouseDownEvent, _, cx| {
                if anchor.get().is_some_and(|b| b.contains(&ev.position)) { return }
                w.lang_menu = None;
                cx.notify();
            }))
            .children(rows);
        let menu = anchored().anchor(Anchor::TopRight).position(point(b.right(), b.bottom() + px(4.))).snap_to_window_with_margin(px(8.)).child(menu);
        // A replay is itself drawn deferred, over the notes, and deferred
        // drawing cannot nest: there the menu is simply the page's last child.
        Some(if self.replay { menu.into_any_element() } else { deferred(menu).with_priority(2).into_any_element() })
    }

    fn import_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut pick_rows = vec![
            row(tr("打开其他文件夹", "Open another folder"), Some(tr("Obsidian、Typora、思源笔记或任何放着 Markdown 的文件夹", "Obsidian, Typora, SiYuan or any folder of Markdown").into()),
                soft_button("onb-pick", tr("选择…", "Choose…")).on_click(cx.listener(|w, _, window, cx| w.pick_folder(Pick::More, window, cx)))).into_any_element(),
        ];
        for (i, p) in self.picked.iter().enumerate() {
            pick_rows.push(row(short_path(p), None, soft_button(("onb-unpick", i), tr("移除", "Remove"))
                .on_click(cx.listener(move |w, _, _, cx| { if i < w.picked.len() { w.picked.remove(i); } w.sync_copy(); cx.notify(); }))).into_any_element());
        }
        // Folders other apps keep notes in; nothing at all while looking or
        // when there are none. A long list shows its first few until 再显示.
        let found = self.found.as_ref().filter(|f| !f.is_empty()).map(|found| {
            let shown = if self.show_all || found.len() <= FOUND_SHOWN + 1 { found.len() } else { FOUND_SHOWN };
            let mut rows: Vec<AnyElement> = found.iter().take(shown).enumerate().map(|(i, f)| {
                let on = self.chosen.get(i).copied().unwrap_or(false);
                let (title, desc) = (f.title(), f.desc());
                div().id(("onb-found", i)).cursor_pointer().hover(|s| s.bg(Theme::fill_hover()))
                    .on_click(cx.listener(move |w, _, _, cx| { if let Some(c) = w.chosen.get_mut(i) { *c = !*c; } w.sync_copy(); cx.notify(); }))
                    .child(row(title, Some(desc.into()), Checkbox::new(on))).into_any_element()
            }).collect();
            if shown < found.len() {
                let more = crate::localized_format!("再显示 {} 处", "Show {} more", found.len() - shown);
                rows.push(div().id("onb-more").h(px(40.)).px(px(16.)).flex().items_center().cursor_pointer().hover(|s| s.bg(Theme::fill_hover()))
                    .text_size(px(13.)).text_color(Theme::text_caption()).child(more)
                    .on_click(cx.listener(|w, _, _, cx| { w.show_all = true; cx.notify(); })).into_any_element());
            }
            // 全选 when more than one can be ticked.
            let can: Vec<usize> = (0..found.len()).collect();
            let all = !can.is_empty() && can.iter().all(|&i| self.chosen.get(i).copied().unwrap_or(false));
            let head = div().w_full().px(px(4.)).flex().items_center().justify_between()
                .child(lifted(12., 16., false, true, crate::localized_format!("在这台电脑上找到 {} 处笔记", "Notes found on this computer ({})", found.len())))
                .when(can.len() > 1, |d| d.child(lesser(false, "onb-all", if all { tr("全不选", "Select none") } else { tr("全选", "Select all") },
                    cx.listener(move |w, _, _, cx| { for &i in &can { if let Some(c) = w.chosen.get_mut(i) { *c = !all; } } w.sync_copy(); cx.notify(); }))));
            (head, rows)
        });
        // Auto margins centre it while it fits; taller, it starts at the top and scrolls.
        div().id("onb-scroll").size_full().overflow_y_scroll().track_scroll(&crate::design::scroll::handle("onb-scroll")).flex().flex_col().items_center().px(px(48.)).pt(px(56.)).pb(px(88.))
            .child(div().my_auto().flex_none().w_full().max_w(px(560.)).flex().flex_col().gap(px(24.))
                .child(heading(tr("带上已有的笔记", "Bring your notes"), if self.copy { tr("导入的是副本，原来的文件不会被改动。也可以跳过，以后从菜单导入。", "Imports are copies; your files stay as they are. You can skip this and import later.") }
                    else { tr("选中的文件夹会直接打开，文件留在原处。也可以跳过，以后从菜单打开。", "The folders you pick open where they are; the files stay put. You can skip this and open them later.") }))
                .child(sheet("onb-pick-sheet", None, pick_rows))
                // 导入副本 needs a folder to copy into (the new one) and something to copy: shown once a
                // folder is picked or ticked, fading in.
                .when(self.open.is_empty() && !self.imports().is_empty(), |d| d.child(sheet("onb-copy-sheet", None, vec![div().id("onb-copy").cursor_pointer().hover(|s| s.bg(Theme::fill_hover()))
                    .on_click(cx.listener(|w, _, _, cx| { w.copy = !w.copy; cx.notify(); }))
                    .child(row(tr("改为导入副本", "Import copies instead"), Some(tr("复制到新文件夹里，和原来的文件夹分开", "Copy them into the new folder, apart from the originals").into()), Checkbox::new(self.copy))).into_any_element()])
                    .with_animation("onb-copy-in", Animation::new(std::time::Duration::from_millis(180)), |el, k| el.opacity(k))))
                .children(found.map(|(head, rows)| sheet("onb-found-sheet", Some(head.into_any_element()), rows)))
                .children(self.error.clone().map(|e| note_owned(e, true))))
            // the shared scrollbar, clear of the window's corners at both ends (48 of room beside the column)
            .child({ crate::design::scroll::solid("onb-scroll"); crate::design::scroll::bar_corners("onb-scroll", &crate::design::scroll::handle("onb-scroll"), crate::design::scroll::WINDOW_R, crate::design::scroll::WINDOW_R) })
            .into_any_element()
    }

    /// The frame every page shares: 跳过 top right, 上一步 bottom left, the
    /// step marks in the middle, the main button bottom right.
    fn chrome(&self, cx: &mut Context<Self>) -> Div {
        let n = self.imports().len();
        let label = match self.page { Page::Setup => tr("下一步", "Next"), Page::Import if n == 0 => tr("开始使用", "Get started"), Page::Import if self.copy && self.open.is_empty() => tr("导入并开始", "Import and start"), Page::Import => tr("打开并开始", "Open and start") };
        let mark = |on: bool| div().w(px(20.)).h(px(3.)).rounded(px(2.)).bg(if on { Theme::text() } else { Theme::fill_hover_strong() });
        div().absolute().inset_0()
            .when(self.importing.is_none(), |d| d.child(div().absolute().top(px(10.)).right(px(16.))
                .child(lesser(true, "onb-skip", tr("跳过", "Skip"), cx.listener(|w, _, window, cx| w.skip(window, cx))))))
            .child(div().absolute().left_0().right_0().bottom(px(40.)).flex().justify_center().gap(px(6.))
                .child(mark(self.page == Page::Setup)).child(mark(self.page == Page::Import)))
            .when(self.page == Page::Import && self.importing.is_none(), |d| d.child(div().absolute().left(px(24.)).bottom(px(30.))
                .child(lesser(true, "onb-back", tr("‹ 上一步", "‹ Back"), cx.listener(|w, _, _, cx| w.go(Page::Setup, cx))))))
            .child(div().absolute().right(px(24.)).bottom(px(28.)).child(match self.importing {
                // The label stays (transparent) so the button keeps its width.
                Some(t) => pill("onb-done", label).relative().cursor_default().text_color(transparent_white())
                    .child(div().absolute().inset_0().flex().items_center().justify_center().child(ui::spinner(t.elapsed().as_secs_f32(), Theme::bg_body()))),
                None => pill(if self.page == Page::Setup { "onb-next" } else { "onb-done" }, label).on_click(cx.listener(|w, _, window, cx| w.advance(window, cx))),
            }))
    }
}

/// Copy each folder into the library (one write for all of them); what went
/// wrong, for a toast in the main window.
fn import_all(root: &Path, dirs: &[(PathBuf, Option<String>)]) -> Option<String> {
    let mut store = match crate::store::Store::open_top_folder(root) { Ok(s) => s, Err(e) => return Some(e.to_string()) };
    let (mut failed, mut cloud, batch) = (0, 0, store.begin_batch().is_ok());
    let mut notes: Vec<String> = vec![];
    for (dir, name) in dirs {
        if fdl_core::siyuan::recognizes(dir) {
            match crate::siyuan::import(&mut store, dir, None) {
                Ok((_, r)) => if let (true, text) = crate::siyuan::summary(&r) { notes.push(text) },
                Err(e) => notes.push(e.to_string()),
            }
            continue;
        }
        match crate::transfer::scan_folder(dir, 0) {
            Some(t) if t.files > crate::transfer::IMPORT_FOLDER_LIMIT => failed += t.files,
            Some(mut t) => {
                cloud += t.cloud;
                if let Some(n) = name { t.name = n.clone(); }
                crate::transfer::import_tree(&mut store, &t, None, &mut failed);
            }
            None => {}
        }
    }
    let saved = if batch { store.end_batch() } else { Ok(()) };
    match saved {
        Err(e) => Some(crate::localized_format!("导入未能完成：{}", "Import could not finish: {}", crate::settings::diagnostic(&e.to_string()))),
        Ok(()) => { notes.extend(crate::transfer::import_problems(failed, cloud)); (!notes.is_empty()).then(|| notes.join(tr("；", "; "))) }
    }
}

fn heading(title: &'static str, sub: &'static str) -> Div {
    div().flex().flex_col().items_center().gap(px(8.))
        .child(lifted(34., 42., true, false, title))
        .child(lifted(14., 21., false, true, sub))
}

/// Plain text keeps contrast against the pale reading backdrop.
fn lifted(size: f32, line: f32, title: bool, soft: bool, text: impl Into<SharedString>) -> Div {
    div().text_size(px(size)).line_height(px(line)).whitespace_nowrap().when(title, |d| d.font_weight(FontWeight::MEDIUM))
        .text_color(if soft { Theme::text_caption() } else { Theme::text() }).child(text.into())
}

/// One row of a group: title and description on the left, a control on the right.
/// Keep `lang` in the library's preferences (a replay's, or the new one's).
fn save_lang(lang: crate::settings::Lang) {
    let mut p = crate::settings::prefs();
    if p.lang() == lang { return }
    p.set_lang(lang);
    if let Err(e) = crate::settings::save(&p) { log::warn!("onboarding: language not saved: {e:#}"); }
}

/// A line on a card: `design::Item`.
fn row(title: impl Into<SharedString>, desc: Option<SharedString>, control: impl IntoElement) -> Div {
    div().w_full().child(crate::design::Item::new(title).description(desc).child(control))
}

/// A card floating over the backdrop: icon, text, control in a line.
fn card(id: &'static str) -> Card {
    Card::new(id).w_full().px(px(16.)).py(px(14.)).flex_row().items_center().gap(px(14.))
}

/// Rows on one card, split by `Separator`s (the import page).
fn sheet(id: &'static str, head: Option<AnyElement>, rows: Vec<AnyElement>) -> Div {
    let n = rows.len();
    let mut card = Card::new(id).w_full().overflow_hidden();
    for (i, r) in rows.into_iter().enumerate() {
        card = card.child(div().w_full().child(r));
        if i + 1 < n { card = card.child(Separator::new()); }
    }
    div().w_full().flex().flex_col().gap(px(8.)).children(head).child(card)
}

fn card_icon(name: &str) -> Div {
    div().flex_none().size(px(36.)).rounded(px(10.)).bg(Theme::fill_hover()).flex().items_center().justify_center()
        .child(ui::icon(name, 18., Theme::text_caption()))
}

fn card_text(title: SharedString, desc: SharedString, error: bool) -> Div {
    div().flex_1().min_w_0().flex().flex_col().gap(px(2.))
        .child(div().text_size(px(13.)).line_height(px(19.)).font_weight(FontWeight::MEDIUM).text_color(Theme::text()).truncate().child(title))
        .child(div().text_size(px(12.)).line_height(px(17.)).text_color(if error { Theme::danger() } else { Theme::text_caption() }).child(desc))
}

/// Every lesser action on these pages: a secondary button.
fn soft_button(id: impl Into<ElementId>, label: &'static str) -> Button {
    Button::secondary(id).sized(ButtonSize::Sm).label(crate::settings::label(label))
}

/// Lesser actions use the same ghost and secondary buttons as the app.
fn lesser(quiet: bool, id: &'static str, label: &'static str, on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> AnyElement {
    if quiet { Button::ghost(id).label(crate::settings::label(label)) } else { soft_button(id, label) }.on_click(on_click).into_any_element()
}

/// The main button: a primary one with the ↩ that also presses it, after its label.
fn pill(id: &'static str, label: &'static str) -> Button {
    Button::primary(id).w_auto().pl(px(14.)).pr(px(12.))
        .child(ui::ui_text(crate::settings::label(label)))
        .child(div().text_size(px(12.)).opacity(0.55).child("↩"))
}

fn note_owned(text: String, error: bool) -> Div {
    div().px(px(4.)).text_size(px(12.)).line_height(px(18.)).text_color(if error { Theme::danger() } else { Theme::text_caption() }).child(text)
}

impl Render for Onboarding {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let t = now.duration_since(self.opened).as_secs_f32();
        let intro_up = now < self.cut_at;
        if intro_up || self.importing.is_some() { window.request_animation_frame(); }
        let backdrop = canvas(|b, _, _| b, move |_, b, window, _| paint_backdrop(b, t, window)).absolute().inset_0();
        let page = if self.page == Page::Setup { self.setup_page(cx) } else { self.import_page(cx) };
        let mark = if intro_up { ((t - MARK_IN.0) / (MARK_IN.1 - MARK_IN.0)).clamp(0., 1.) } else { 0. };
        div().id("onboarding").size_full().relative().overflow_hidden()
            .font_family(theme::ui_family()).text_color(Theme::text())
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|w, ev: &KeyDownEvent, window, cx| w.key_down(ev, window, cx)))
            .child(backdrop)
            .when(!intro_up, |d| d.child(div().absolute().inset_0().child(page)).child(self.chrome(cx)))
            // The opening mark: a click anywhere brings the pages now.
            .when(intro_up, |d| d.child(div().id("onb-cut").absolute().inset_0().occlude().on_mouse_down(MouseButton::Left, cx.listener(|w, _, _, cx| { w.cut(cx); }))))
            .when(mark > 0., |d| d.child(wordmark(mark)))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn a_picked_folder_is_the_library_itself() {
        let dir = fdl_core::vfs::temp_dir().join(format!("qmd-library-in-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("notes.md"), "# Notes").unwrap();
        assert_eq!(super::library_in(dir.clone()), dir);
        assert_eq!(super::library_in("/".into()), Path::new("/Quiet Notes"));
        let home = dirs::home_dir().unwrap();
        assert_eq!(super::library_in(home.clone()), home.join("Quiet Notes"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
