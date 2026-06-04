//! In-view find bar for the source / diff / preview content panel.
//!
//! One search input drives two backends, chosen by the currently active view:
//!   * Source & Diff → `gtk::TextView` / `TextBuffer` (iter-based search + tag highlight)
//!   * Preview        → `webkit6::WebView`'s native `FindController`
//!
//! Matching is substring + case-insensitive. Findings are highlighted and the
//! current match is scrolled into view. `Ctrl+F` reveals the bar, `Esc` closes it,
//! `Enter` / `Shift+Enter` (and the ↓/↑ buttons) cycle next / previous.

use gtk::gdk;
use gtk::glib;
use gtk4 as gtk;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
// Pulls in both webkit6's WebView/FindController traits and (transitively) gtk4's
// prelude, which provides the TextBuffer/Widget/Editable/Button extension traits.
use webkit6::prelude::*;

use crate::textview::TextViewPanel;

/// Background-highlight tag for every match.
const TAG_MATCH: &str = "search-match";
/// Background-highlight tag for the currently-focused match (higher priority).
const TAG_CURRENT: &str = "search-current";
/// Upper bound passed to WebKit's find controller.
const MAX_MATCHES: u32 = 10_000;
/// Delay before re-running a preview search after switching into it, to let the
/// WebView finish (re)loading its content asynchronously.
const PREVIEW_RELOAD_MS: u64 = 200;

/// Shared search state. `current` is a 1-based display index (0 = no match).
#[derive(Default)]
struct SearchState {
    /// (start_offset, end_offset) of every match in the text buffer.
    matches: Vec<(i32, i32)>,
    current: usize,
    total: usize,
    /// The WebView we've already attached find-result signals to.
    connected: Option<webkit6::WebView>,
}

/// Widgets making up the find bar. Behavior is attached separately in [`wire`].
pub struct FindBar {
    pub container: gtk::Box,
    pub entry: gtk::SearchEntry,
    pub prev_btn: gtk::Button,
    pub next_btn: gtk::Button,
    pub close_btn: gtk::Button,
    pub counter: gtk::Label,
}

/// Build the (inert, hidden) find bar widgets.
pub fn create_find_bar() -> FindBar {
    let container = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    container.add_css_class("find-bar");
    container.set_margin_start(4);
    container.set_margin_end(4);
    container.set_margin_top(2);
    container.set_margin_bottom(2);
    container.set_visible(false);

    let entry = gtk::SearchEntry::new();
    entry.set_placeholder_text(Some("Find"));
    entry.set_hexpand(true);

    let counter = gtk::Label::new(None);
    counter.add_css_class("dim-label");
    counter.set_width_chars(9);
    counter.set_xalign(1.0);

    let prev_btn = gtk::Button::from_icon_name("go-up-symbolic");
    prev_btn.set_tooltip_text(Some("Previous match (Shift+Enter)"));
    prev_btn.set_has_frame(false);

    let next_btn = gtk::Button::from_icon_name("go-down-symbolic");
    next_btn.set_tooltip_text(Some("Next match (Enter)"));
    next_btn.set_has_frame(false);

    let close_btn = gtk::Button::from_icon_name("window-close-symbolic");
    close_btn.set_tooltip_text(Some("Close (Esc)"));
    close_btn.set_has_frame(false);

    container.append(&entry);
    container.append(&counter);
    container.append(&prev_btn);
    container.append(&next_btn);
    container.append(&close_btn);

    FindBar {
        container,
        entry,
        prev_btn,
        next_btn,
        close_btn,
        counter,
    }
}

/// Attach all behavior: keyboard shortcuts, incremental search, navigation and
/// view-switch refresh. Call once, after the panel-mode handlers are wired.
pub fn wire(tv: &TextViewPanel) {
    let fb = &tv.find_bar;
    let state = Rc::new(RefCell::new(SearchState::default()));

    let text_view = tv.text_view.clone();
    let preview_scroll = tv.preview_scroll.clone();
    let preview_btn = tv.preview_btn.clone();
    let entry = fb.entry.clone();
    let counter = fb.counter.clone();
    let container = fb.container.clone();

    // ── Incremental search as the query changes ──────────────────────────────
    {
        let text_view = text_view.clone();
        let preview_scroll = preview_scroll.clone();
        let preview_btn = preview_btn.clone();
        let counter = counter.clone();
        let state = Rc::clone(&state);
        fb.entry.connect_search_changed(move |e| {
            let query = e.text().to_string();
            if query.is_empty() {
                clear_all(&text_view, &preview_scroll, &state, &counter);
            } else {
                do_search(
                    &text_view,
                    &preview_scroll,
                    &preview_btn,
                    &query,
                    &state,
                    &counter,
                );
            }
        });
    }

    // ── Enter → next, ↓ button → next, ↑ button → previous ───────────────────
    let make_nav = |forward: bool| {
        let text_view = text_view.clone();
        let preview_scroll = preview_scroll.clone();
        let preview_btn = preview_btn.clone();
        let counter = counter.clone();
        let state = Rc::clone(&state);
        move || {
            do_nav(
                &text_view,
                &preview_scroll,
                &preview_btn,
                forward,
                &state,
                &counter,
            )
        }
    };
    {
        let next = make_nav(true);
        fb.entry.connect_activate(move |_| next());
    }
    {
        let next = make_nav(true);
        fb.next_btn.connect_clicked(move |_| next());
    }
    {
        let prev = make_nav(false);
        fb.prev_btn.connect_clicked(move |_| prev());
    }

    // ── Shift+Enter on the entry → previous ──────────────────────────────────
    {
        let prev = make_nav(false);
        let kc = gtk::EventControllerKey::new();
        kc.set_propagation_phase(gtk::PropagationPhase::Capture);
        kc.connect_key_pressed(move |_c, keyval, _code, mods| {
            let is_enter = keyval == gdk::Key::Return || keyval == gdk::Key::KP_Enter;
            if is_enter && mods.contains(gdk::ModifierType::SHIFT_MASK) {
                prev();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        fb.entry.add_controller(kc);
    }

    // ── reveal / close routines (shared) ─────────────────────────────────────
    let reveal: Rc<dyn Fn()> = {
        let container = container.clone();
        let entry = entry.clone();
        let text_view = text_view.clone();
        Rc::new(move || {
            container.set_visible(true);
            // Seed from a single-line selection in the source/diff buffer.
            let buffer = text_view.buffer();
            if let Some((s, e)) = buffer.selection_bounds() {
                let sel = buffer.text(&s, &e, false).to_string();
                if !sel.is_empty() && !sel.contains('\n') {
                    entry.set_text(&sel);
                }
            }
            entry.grab_focus();
            entry.select_region(0, -1);
        })
    };
    let close: Rc<dyn Fn()> = {
        let container = container.clone();
        let entry = entry.clone();
        let text_view = text_view.clone();
        let preview_scroll = preview_scroll.clone();
        let counter = counter.clone();
        let state = Rc::clone(&state);
        Rc::new(move || {
            clear_all(&text_view, &preview_scroll, &state, &counter);
            container.set_visible(false);
            entry.set_text("");
            text_view.grab_focus();
        })
    };

    {
        let close = Rc::clone(&close);
        fb.close_btn.connect_clicked(move |_| close());
    }
    {
        let close = Rc::clone(&close);
        fb.entry.connect_stop_search(move |_| close());
    }

    // ── Toolbar magnifier toggles the bar ────────────────────────────────────
    {
        let reveal = Rc::clone(&reveal);
        let close = Rc::clone(&close);
        let container = container.clone();
        tv.search_btn.connect_clicked(move |_| {
            if container.is_visible() {
                close();
            } else {
                reveal();
            }
        });
    }

    // ── Ctrl+F reveals (captured on the whole panel) ─────────────────────────
    {
        let reveal = Rc::clone(&reveal);
        let kc = gtk::EventControllerKey::new();
        kc.set_propagation_phase(gtk::PropagationPhase::Capture);
        kc.connect_key_pressed(move |_c, keyval, _code, mods| {
            let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
            if ctrl && (keyval == gdk::Key::f || keyval == gdk::Key::F) {
                reveal();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        tv.container.add_controller(kc);
    }

    // ── Buffer content reload (file/diff/source switch) resets the find ──────
    {
        let container = container.clone();
        let counter = counter.clone();
        let state = Rc::clone(&state);
        text_view.buffer().connect_changed(move |buf| {
            if !container.is_visible() {
                return;
            }
            clear_text_highlights(buf);
            let mut st = state.borrow_mut();
            st.matches.clear();
            st.total = 0;
            st.current = 0;
            counter.set_text("");
        });
    }

    // ── Re-run the search when a view becomes active while the bar is open ───
    for btn in [
        tv.source_btn.clone(),
        tv.diff_btn.clone(),
        tv.preview_btn.clone(),
    ] {
        let text_view = text_view.clone();
        let preview_scroll = preview_scroll.clone();
        let preview_btn = preview_btn.clone();
        let counter = counter.clone();
        let state = Rc::clone(&state);
        let entry = entry.clone();
        let container = container.clone();
        btn.connect_toggled(move |b| {
            if !b.is_active() || !container.is_visible() {
                return;
            }
            let query = entry.text().to_string();
            if query.is_empty() {
                return;
            }
            do_search(
                &text_view,
                &preview_scroll,
                &preview_btn,
                &query,
                &state,
                &counter,
            );
            // Preview content (re)loads asynchronously — re-run once it settles.
            if preview_btn.is_active() {
                let text_view = text_view.clone();
                let preview_scroll = preview_scroll.clone();
                let preview_btn = preview_btn.clone();
                let counter = counter.clone();
                let state = Rc::clone(&state);
                let entry = entry.clone();
                let container = container.clone();
                glib::timeout_add_local_once(Duration::from_millis(PREVIEW_RELOAD_MS), move || {
                    if container.is_visible() && preview_btn.is_active() {
                        let query = entry.text().to_string();
                        if !query.is_empty() {
                            do_search(
                                &text_view,
                                &preview_scroll,
                                &preview_btn,
                                &query,
                                &state,
                                &counter,
                            );
                        }
                    }
                });
            }
        });
    }
}

// ── Dispatch ─────────────────────────────────────────────────────────────────

fn do_search(
    text_view: &gtk::TextView,
    preview_scroll: &gtk::ScrolledWindow,
    preview_btn: &gtk::ToggleButton,
    query: &str,
    state: &Rc<RefCell<SearchState>>,
    counter: &gtk::Label,
) {
    if preview_btn.is_active() {
        search_preview(preview_scroll, query, state, counter);
    } else {
        search_text_view(text_view, query, state, counter);
    }
}

fn do_nav(
    text_view: &gtk::TextView,
    preview_scroll: &gtk::ScrolledWindow,
    preview_btn: &gtk::ToggleButton,
    forward: bool,
    state: &Rc<RefCell<SearchState>>,
    counter: &gtk::Label,
) {
    if preview_btn.is_active() {
        nav_preview(preview_scroll, forward, state, counter);
    } else {
        nav_text_view(text_view, forward, state, counter);
    }
}

fn clear_all(
    text_view: &gtk::TextView,
    preview_scroll: &gtk::ScrolledWindow,
    state: &Rc<RefCell<SearchState>>,
    counter: &gtk::Label,
) {
    clear_text_highlights(&text_view.buffer());
    if let Some(fc) = current_find_controller(preview_scroll) {
        fc.search_finish();
    }
    let mut st = state.borrow_mut();
    st.matches.clear();
    st.total = 0;
    st.current = 0;
    counter.set_text("");
}

// ── Text buffer backend (source + diff) ────────────────────────────────────────

fn ensure_tags(buffer: &gtk::TextBuffer) {
    let tt = buffer.tag_table();
    // Backgrounds only; foreground forced dark so matches stay legible over both
    // syntect (light/dark) foregrounds and the diff add/remove foreground colors.
    if tt.lookup(TAG_MATCH).is_none() {
        let t = gtk::TextTag::builder()
            .name(TAG_MATCH)
            .background("#ffe082")
            .foreground("#1a1a1a")
            .build();
        tt.add(&t);
    }
    // Created after TAG_MATCH → higher priority, so the current match wins.
    if tt.lookup(TAG_CURRENT).is_none() {
        let t = gtk::TextTag::builder()
            .name(TAG_CURRENT)
            .background("#ff9800")
            .foreground("#1a1a1a")
            .build();
        tt.add(&t);
    }
}

fn clear_text_highlights(buffer: &gtk::TextBuffer) {
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    let tt = buffer.tag_table();
    if tt.lookup(TAG_MATCH).is_some() {
        buffer.remove_tag_by_name(TAG_MATCH, &start, &end);
    }
    if tt.lookup(TAG_CURRENT).is_some() {
        buffer.remove_tag_by_name(TAG_CURRENT, &start, &end);
    }
}

fn search_text_view(
    text_view: &gtk::TextView,
    query: &str,
    state: &Rc<RefCell<SearchState>>,
    counter: &gtk::Label,
) {
    let buffer = text_view.buffer();
    clear_text_highlights(&buffer);
    ensure_tags(&buffer);

    let flags = gtk::TextSearchFlags::CASE_INSENSITIVE | gtk::TextSearchFlags::TEXT_ONLY;
    let mut matches = Vec::new();
    let mut iter = buffer.start_iter();
    while let Some((s, e)) = iter.forward_search(query, flags, None) {
        buffer.apply_tag_by_name(TAG_MATCH, &s, &e);
        matches.push((s.offset(), e.offset()));
        iter = e;
    }

    let total = matches.len();
    {
        let mut st = state.borrow_mut();
        st.matches = matches;
        st.total = total;
        st.current = if total > 0 { 1 } else { 0 };
    }
    if total > 0 {
        focus_current_text(text_view, state);
    }
    set_counter(counter, state);
}

fn nav_text_view(
    text_view: &gtk::TextView,
    forward: bool,
    state: &Rc<RefCell<SearchState>>,
    counter: &gtk::Label,
) {
    {
        let mut st = state.borrow_mut();
        let total = st.matches.len();
        if total == 0 {
            return;
        }
        st.current = if forward {
            st.current % total + 1
        } else {
            (st.current + total - 2) % total + 1
        };
    }
    focus_current_text(text_view, state);
    set_counter(counter, state);
}

fn focus_current_text(text_view: &gtk::TextView, state: &Rc<RefCell<SearchState>>) {
    let buffer = text_view.buffer();
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    buffer.remove_tag_by_name(TAG_CURRENT, &start, &end);

    let span = {
        let st = state.borrow();
        if st.current == 0 || st.current > st.matches.len() {
            return;
        }
        st.matches[st.current - 1]
    };
    let is = buffer.iter_at_offset(span.0);
    let ie = buffer.iter_at_offset(span.1);
    buffer.apply_tag_by_name(TAG_CURRENT, &is, &ie);

    let mark = buffer.create_mark(None, &is, false);
    text_view.scroll_to_mark(&mark, 0.1, false, 0.0, 0.0);
    buffer.delete_mark(&mark);
}

// ── WebKit backend (preview) ───────────────────────────────────────────────────

fn current_webview(preview_scroll: &gtk::ScrolledWindow) -> Option<webkit6::WebView> {
    preview_scroll
        .child()
        .and_then(|c| c.downcast::<webkit6::WebView>().ok())
}

fn current_find_controller(
    preview_scroll: &gtk::ScrolledWindow,
) -> Option<webkit6::FindController> {
    current_webview(preview_scroll).and_then(|wv| wv.find_controller())
}

fn search_preview(
    preview_scroll: &gtk::ScrolledWindow,
    query: &str,
    state: &Rc<RefCell<SearchState>>,
    counter: &gtk::Label,
) {
    let Some(wv) = current_webview(preview_scroll) else {
        // Image preview (or nothing) — not searchable.
        let mut st = state.borrow_mut();
        st.total = 0;
        st.current = 0;
        counter.set_text("No results");
        return;
    };
    let Some(fc) = wv.find_controller() else {
        return;
    };
    ensure_preview_signals(&wv, &fc, state, counter);

    // Tentative; the real count and re-confirmation arrive via `found-text`.
    state.borrow_mut().current = 1;
    let opts = (webkit6::FindOptions::CASE_INSENSITIVE | webkit6::FindOptions::WRAP_AROUND).bits();
    fc.search(query, opts, MAX_MATCHES);
}

fn nav_preview(
    preview_scroll: &gtk::ScrolledWindow,
    forward: bool,
    state: &Rc<RefCell<SearchState>>,
    counter: &gtk::Label,
) {
    let Some(fc) = current_find_controller(preview_scroll) else {
        return;
    };
    {
        let mut st = state.borrow_mut();
        if st.total == 0 {
            return;
        }
        st.current = if forward {
            st.current % st.total + 1
        } else {
            (st.current + st.total - 2) % st.total + 1
        };
    }
    if forward {
        fc.search_next();
    } else {
        fc.search_previous();
    }
    set_counter(counter, state);
}

/// Wire `found-text` / `failed-to-find-text` once per WebView instance so the
/// (asynchronously produced) match count updates the counter. Handlers hold a
/// weak ref to the state to avoid a reference cycle through the WebView.
fn ensure_preview_signals(
    wv: &webkit6::WebView,
    fc: &webkit6::FindController,
    state: &Rc<RefCell<SearchState>>,
    counter: &gtk::Label,
) {
    if state.borrow().connected.as_ref() == Some(wv) {
        return;
    }

    {
        let weak = Rc::downgrade(state);
        let counter = counter.clone();
        fc.connect_found_text(move |_fc, count| {
            if let Some(st_rc) = weak.upgrade() {
                {
                    let mut st = st_rc.borrow_mut();
                    st.total = count as usize;
                    if count == 0 {
                        st.current = 0;
                    } else if st.current == 0 {
                        st.current = 1;
                    }
                }
                set_counter(&counter, &st_rc);
            }
        });
    }
    {
        let weak = Rc::downgrade(state);
        let counter = counter.clone();
        fc.connect_failed_to_find_text(move |_fc| {
            if let Some(st_rc) = weak.upgrade() {
                {
                    let mut st = st_rc.borrow_mut();
                    st.total = 0;
                    st.current = 0;
                }
                set_counter(&counter, &st_rc);
            }
        });
    }

    state.borrow_mut().connected = Some(wv.clone());
}

// ── Shared counter rendering ───────────────────────────────────────────────────

fn set_counter(counter: &gtk::Label, state: &Rc<RefCell<SearchState>>) {
    let st = state.borrow();
    if st.total == 0 {
        counter.set_text("No results");
    } else {
        counter.set_text(&format!("{} of {}", st.current, st.total));
    }
}
