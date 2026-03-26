use gtk4 as gtk;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use webkit6::prelude::*;

use crate::services::platform;

// ---------------------------------------------------------------------------
// Theme CSS
// ---------------------------------------------------------------------------

fn light_theme_css() -> &'static str {
    r#"
    body { background: transparent; color: #24292f; }
    .user-msg { background: rgba(53,132,228,0.15); }
    .tool-call { background: #fff; border: 1px solid #d0d0d0; }
    code { background: #f6f8fa; }
    pre code { background: #f6f8fa; color: #24292f; }
    th { background: #f6f8fa; border-bottom: 2px solid #d0d7de; }
    td { border-bottom: 1px solid #eaeef2; }
    blockquote { border-left: 3px solid #d0d7de; color: #656d76; }
    .diff-del { background: #ffeef0; }
    .diff-add { background: #e6ffed; }
    a { color: #0969da; }
    .full-cmd { background: #f6f8fa; }
    .tool-call .spinner { border-color: #24292f; border-top-color: transparent; }
    "#
}

fn dark_theme_css() -> &'static str {
    r#"
    body { background: transparent; color: #e6edf3; }
    .user-msg { background: rgba(53,132,228,0.15); }
    .tool-call { background: #2d333b; border: 1px solid #444c56; }
    code { background: #2d333b; }
    pre code { background: #2d333b; color: #e6edf3; }
    th { background: #2d333b; border-bottom: 2px solid #444c56; }
    td { border-bottom: 1px solid #373e47; }
    blockquote { border-left: 3px solid #444c56; color: #8b949e; }
    .diff-del { background: rgba(248,81,73,0.15); }
    .diff-add { background: rgba(63,185,80,0.15); }
    a { color: #58a6ff; }
    .full-cmd { background: #2d333b; }
    .tool-call .spinner { border-color: #e6edf3; border-top-color: transparent; }
    "#
}

// ---------------------------------------------------------------------------
// Base CSS
// ---------------------------------------------------------------------------

const BASE_CSS: &str = r#"
* { margin: 0; padding: 0; box-sizing: border-box; }
body { font-family: system-ui, -apple-system, sans-serif; font-size: 14px; padding: 8px; }
#chat { display: flex; flex-direction: column; gap: 8px; min-height: 100vh; justify-content: flex-end; }
.msg { padding: 8px 12px; border-radius: 8px; max-width: 100%; word-wrap: break-word; overflow-wrap: break-word; }
.user-msg { align-self: flex-end; margin-left: 48px; }
.assistant-msg { align-self: flex-start; }
.system-msg { align-self: center; font-size: 0.85em; opacity: 0.6; }
/* Markdown styling */
.assistant-msg h1, .assistant-msg h2, .assistant-msg h3 { margin-top: 16px; margin-bottom: 8px; }
.assistant-msg h1 { font-size: 1.5em; }
.assistant-msg h2 { font-size: 1.3em; }
.assistant-msg h3 { font-size: 1.1em; }
.assistant-msg p { margin-bottom: 8px; line-height: 1.5; }
.assistant-msg ul, .assistant-msg ol { margin-left: 20px; margin-bottom: 8px; }
.assistant-msg li { margin-bottom: 4px; }
.assistant-msg table { border-collapse: collapse; margin: 8px 0; width: auto; }
.assistant-msg th, .assistant-msg td { padding: 6px 12px; text-align: left; }
.assistant-msg code { font-family: 'JetBrains Mono', 'Fira Code', monospace; font-size: 0.9em; padding: 2px 6px; border-radius: 3px; }
.assistant-msg pre { margin: 8px 0; border-radius: 6px; overflow-x: auto; }
.assistant-msg pre code { display: block; padding: 12px; }
.assistant-msg blockquote { padding-left: 12px; margin: 8px 0; }
.assistant-msg a { text-decoration: underline; cursor: pointer; }
.assistant-msg img { max-width: 100%; border-radius: 4px; }
/* Tool calls */
.tool-call { border-radius: 6px; padding: 6px; margin: 4px 8px; }
.tool-call summary { cursor: pointer; font-size: 0.9em; list-style: none; display: flex; align-items: center; gap: 4px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.tool-call summary::-webkit-details-marker { display: none; }
.tool-call .spinner { display: inline-block; width: 14px; height: 14px; border: 2px solid; border-top-color: transparent; border-radius: 50%; animation: spin 0.8s linear infinite; }
.tool-call .indicator { font-size: 0.8em; }
.tool-call .full-cmd { font-family: monospace; font-size: 0.85em; padding: 4px 8px; border-radius: 4px; margin: 4px 0; white-space: pre-wrap; word-break: break-all; }
.tool-call .output { font-family: monospace; font-size: 0.85em; white-space: pre-wrap; word-break: break-all; margin-top: 4px; max-height: 400px; overflow-y: auto; }
@keyframes spin { to { transform: rotate(360deg); } }
/* Thinking */
.thinking { font-size: 0.85em; opacity: 0.5; }
.thinking::after { content: '...'; animation: dots 1.5s steps(4,end) infinite; }
@keyframes dots { 0%,20%{content:'.'} 40%{content:'..'} 60%{content:'...'} 80%,100%{content:''} }
/* Load prev button */
#load-prev { text-align: center; padding: 8px; }
#load-prev a { font-size: 0.85em; cursor: pointer; }
/* Diff */
.diff-del { padding: 1px 0; }
.diff-add { padding: 1px 0; }
/* User images */
.user-images { display: flex; gap: 4px; flex-wrap: wrap; margin-top: 6px; }
.user-images img { max-height: 120px; max-width: 160px; border-radius: 4px; object-fit: cover; }
"#;

// ---------------------------------------------------------------------------
// Base JavaScript
// ---------------------------------------------------------------------------

const BASE_JS: &str = r#"
function appendUserMsg(id, textHtml, images) {
    var d = document.createElement('div');
    d.id = id;
    d.className = 'msg user-msg';
    d.innerHTML = textHtml;
    if (images && images.length > 0) {
        var ic = document.createElement('div');
        ic.className = 'user-images';
        for (var i = 0; i < images.length; i++) {
            var img = document.createElement('img');
            img.src = images[i];
            ic.appendChild(img);
        }
        d.appendChild(ic);
    }
    document.getElementById('chat').appendChild(d);
    scrollToBottom();
}

function beginStream(id) {
    var d = document.createElement('div');
    d.id = id;
    d.className = 'msg assistant-msg';
    document.getElementById('chat').appendChild(d);
    scrollToBottom();
}

function updateStream(id, html) {
    var el = document.getElementById(id);
    if (el) { el.innerHTML = html; scrollToBottom(); }
}

function finalizeStream(id, html) {
    var el = document.getElementById(id);
    if (el) { el.innerHTML = html; scrollToBottom(); }
}

function appendToolCall(id, name, hint, fullCmd, filePath) {
    var det = document.createElement('details');
    det.id = id;
    det.className = 'tool-call';
    var sum = document.createElement('summary');
    var spinner = document.createElement('span');
    spinner.className = 'spinner';
    spinner.id = id + '-spinner';
    sum.appendChild(spinner);
    var ind = document.createElement('span');
    ind.className = 'indicator';
    ind.id = id + '-indicator';
    ind.style.display = 'none';
    sum.appendChild(ind);
    var nameSpan = document.createElement('span');
    nameSpan.textContent = ' ' + name;
    sum.appendChild(nameSpan);
    if (hint) {
        var hintSpan = document.createElement('span');
        hintSpan.style.opacity = '0.6';
        hintSpan.textContent = ' ' + hint;
        sum.appendChild(hintSpan);
    }
    if (filePath) {
        var link = document.createElement('a');
        link.href = 'flycrys://open-file?path=' + encodeURIComponent(filePath);
        link.textContent = filePath;
        link.style.marginLeft = '4px';
        sum.appendChild(link);
    }
    det.appendChild(sum);
    if (fullCmd) {
        var cmdDiv = document.createElement('div');
        cmdDiv.className = 'full-cmd';
        cmdDiv.textContent = fullCmd;
        det.appendChild(cmdDiv);
    }
    var outDiv = document.createElement('div');
    outDiv.className = 'output';
    outDiv.id = id + '-output';
    det.appendChild(outDiv);
    document.getElementById('chat').appendChild(det);
    scrollToBottom();
}

function toolComplete(id, isError) {
    var spinner = document.getElementById(id + '-spinner');
    if (spinner) spinner.style.display = 'none';
    var ind = document.getElementById(id + '-indicator');
    if (ind) {
        ind.style.display = 'inline';
        ind.textContent = isError ? '\u25B6\u26A0' : '\u25B6';
    }
}

function toolOutput(id, html) {
    var el = document.getElementById(id + '-output');
    if (el) { el.innerHTML = html; }
}

function appendSystemMsg(id, text) {
    var d = document.createElement('div');
    d.id = id;
    d.className = 'msg system-msg';
    d.textContent = text;
    document.getElementById('chat').appendChild(d);
    scrollToBottom();
}

function showThinking(id) {
    var d = document.createElement('div');
    d.id = id;
    d.className = 'thinking';
    d.textContent = 'Thinking';
    document.getElementById('chat').appendChild(d);
    scrollToBottom();
}

function removeElement(id) {
    var el = document.getElementById(id);
    if (el) el.remove();
}

function scrollToBottom() {
    window.scrollTo(0, document.body.scrollHeight);
}

function setTheme(css) {
    var el = document.getElementById('theme-css');
    if (el) el.textContent = css;
}

function clearChat() {
    var c = document.getElementById('chat');
    while (c.firstChild) c.removeChild(c.firstChild);
}

function trimOldest(keep) {
    var c = document.getElementById('chat');
    while (c.children.length > keep) c.removeChild(c.firstChild);
}

function showLoadPrev() {
    var el = document.getElementById('load-prev');
    if (el) el.style.display = 'block';
}

function hideLoadPrev() {
    var el = document.getElementById('load-prev');
    if (el) el.style.display = 'none';
}
"#;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Escape a string for safe embedding inside a JavaScript single-quoted string literal.
fn js_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + s.len() / 8);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '<' => {
                // Prevent premature </script> closing inside inline JS strings.
                // We only need to escape the sequence "</script" (case-insensitive),
                // but escaping every '</' is simpler and harmless.
                out.push_str("\\x3C");
            }
            _ => out.push(ch),
        }
    }
    out
}

/// Build the full base HTML document with theme and static assets inlined.
fn build_base_html(is_dark: bool) -> String {
    let theme_css = if is_dark {
        dark_theme_css()
    } else {
        light_theme_css()
    };
    format!(
        r#"<!DOCTYPE html>
<html><head>
  <meta charset="utf-8">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src data:;">
  <style id="theme-css">{theme_css}</style>
  <style>{BASE_CSS}</style>
</head><body>
  <div id="load-prev" style="display:none"><a href="flycrys://load-prev">Load previous messages</a></div>
  <div id="chat"></div>
  <script>{BASE_JS}</script>
</body></html>"#
    )
}

// ---------------------------------------------------------------------------
// ChatWebView
// ---------------------------------------------------------------------------

pub struct ChatWebView {
    webview: webkit6::WebView,
    msg_counter: Cell<u32>,
    /// JS calls queued before the page finishes loading.
    pending_js: Rc<RefCell<Vec<String>>>,
    /// `true` once the base HTML document has finished loading.
    ready: Rc<Cell<bool>>,
}

impl ChatWebView {
    pub fn new(is_dark: bool, on_open_file: Rc<dyn Fn(&str)>) -> Self {
        let webview = webkit6::WebView::new();

        // --- Security & UI settings ---
        if let Some(settings) = webkit6::prelude::WebViewExt::settings(&webview) {
            settings.set_allow_file_access_from_file_urls(false);
            settings.set_enable_javascript(true);
            // Disable dev tools in release builds.
            #[cfg(debug_assertions)]
            settings.set_enable_developer_extras(true);
            #[cfg(not(debug_assertions))]
            settings.set_enable_developer_extras(false);
        }

        // Disable right-click context menu.
        webview.connect_context_menu(|_wv, _menu, _hit| {
            true // returning true suppresses the menu
        });

        // Transparent background so GTK theme shows through.
        webview.set_background_color(&gtk::gdk::RGBA::new(0.0, 0.0, 0.0, 0.0));

        // --- Navigation policy: intercept custom URIs, open http(s) externally ---
        let open_file_cb = on_open_file.clone();
        webview.connect_decide_policy(move |_wv, decision, decision_type| {
            if decision_type != webkit6::PolicyDecisionType::NavigationAction {
                decision.ignore();
                return true;
            }

            // Downcast to NavigationPolicyDecision.
            let nav_decision = match decision.downcast_ref::<webkit6::NavigationPolicyDecision>() {
                Some(d) => d,
                None => {
                    decision.ignore();
                    return true;
                }
            };

            let uri = match nav_decision
                .navigation_action()
                .and_then(|mut a| a.request())
                .and_then(|r| r.uri())
            {
                Some(u) => u.to_string(),
                None => {
                    decision.use_();
                    return true;
                }
            };

            if uri.starts_with("flycrys://open-file") {
                decision.ignore();
                // Extract path from flycrys://open-file?path=<encoded>
                if let Some(query) = uri.split('?').nth(1) {
                    for param in query.split('&') {
                        if let Some(value) = param.strip_prefix("path=") {
                            let decoded = percent_decode(value);
                            open_file_cb(&decoded);
                        }
                    }
                }
                return true;
            }

            if uri.starts_with("flycrys://load-prev") {
                decision.ignore();
                eprintln!("[chat_webview] load-prev requested (not yet wired)");
                return true;
            }

            if uri.starts_with("http://") || uri.starts_with("https://") {
                decision.ignore();
                let _ = platform::open_in_browser(&uri);
                return true;
            }

            // Allow the initial about:blank / load_html navigation.
            if uri == "about:blank" || uri.starts_with("about:") {
                decision.use_();
                return true;
            }

            // Block everything else.
            decision.ignore();
            true
        });

        // Queue for JS calls made before the page is ready.
        let pending_js: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let ready = Rc::new(Cell::new(false));

        // Flush queued JS once the base HTML has loaded.
        {
            let pending = Rc::clone(&pending_js);
            let rdy = Rc::clone(&ready);
            let wv = webview.clone();
            webview.connect_load_changed(move |_wv, event| {
                if event == webkit6::LoadEvent::Finished {
                    rdy.set(true);
                    let queue: Vec<String> = pending.borrow_mut().drain(..).collect();
                    for js in queue {
                        wv.evaluate_javascript(
                            &js,
                            None,
                            None,
                            None::<&gtk::gio::Cancellable>,
                            |_| {},
                        );
                    }
                }
            });
        }

        // Load the base HTML document.
        let html = build_base_html(is_dark);
        webview.load_html(&html, None);

        Self {
            webview,
            msg_counter: Cell::new(0),
            pending_js,
            ready,
        }
    }

    // --- Accessors ---

    /// Returns a reference to the underlying WebView widget.
    pub fn widget(&self) -> &webkit6::WebView {
        &self.webview
    }

    // --- Message API ---

    /// Append a right-aligned user message bubble, optionally with image thumbnails.
    pub fn append_user_message(&self, text: &str, image_data_uris: &[String]) {
        let id = self.next_id();
        let text_html = js_escape(text);
        let images_js = if image_data_uris.is_empty() {
            "[]".to_string()
        } else {
            let items: Vec<String> = image_data_uris
                .iter()
                .map(|u| format!("'{}'", js_escape(u)))
                .collect();
            format!("[{}]", items.join(","))
        };
        self.evaluate_js(&format!(
            "appendUserMsg('{id}', '{text_html}', {images_js});"
        ));
    }

    /// Create an empty assistant streaming div and return its ID.
    pub fn begin_stream(&self) -> String {
        let id = self.next_id();
        self.evaluate_js(&format!("beginStream('{id}');"));
        id
    }

    /// Update the innerHTML of a streaming assistant div.
    pub fn update_stream(&self, id: &str, html: &str) {
        let eid = js_escape(id);
        let ehtml = js_escape(html);
        self.evaluate_js(&format!("updateStream('{eid}', '{ehtml}');"));
    }

    /// Replace streaming content with final rendered HTML.
    pub fn finalize_stream(&self, id: &str, html: &str) {
        let eid = js_escape(id);
        let ehtml = js_escape(html);
        self.evaluate_js(&format!("finalizeStream('{eid}', '{ehtml}');"));
    }

    /// Append a collapsible tool-call element.
    pub fn append_tool_call(
        &self,
        id: &str,
        name: &str,
        hint: &str,
        full_command: &str,
        file_path: Option<&str>,
    ) {
        let eid = js_escape(id);
        let ename = js_escape(name);
        let ehint = js_escape(hint);
        let ecmd = js_escape(full_command);
        let epath = match file_path {
            Some(p) => format!("'{}'", js_escape(p)),
            None => "null".to_string(),
        };
        self.evaluate_js(&format!(
            "appendToolCall('{eid}', '{ename}', '{ehint}', '{ecmd}', {epath});"
        ));
    }

    /// Mark a tool call as complete — stop the spinner and show an indicator.
    pub fn tool_complete(&self, id: &str, is_error: bool) {
        let eid = js_escape(id);
        let err = if is_error { "true" } else { "false" };
        self.evaluate_js(&format!("toolComplete('{eid}', {err});"));
    }

    /// Set the output content inside a tool-call details element.
    pub fn tool_output(&self, id: &str, html: &str) {
        let eid = js_escape(id);
        let ehtml = js_escape(html);
        self.evaluate_js(&format!("toolOutput('{eid}', '{ehtml}');"));
    }

    /// Append a centered system message.
    pub fn append_system_message(&self, text: &str) {
        let id = self.next_id();
        let etext = js_escape(text);
        self.evaluate_js(&format!("appendSystemMsg('{id}', '{etext}');"));
    }

    /// Show an animated "Thinking..." indicator and return its element ID.
    pub fn show_thinking(&self) -> String {
        let id = self.next_id();
        self.evaluate_js(&format!("showThinking('{id}');"));
        id
    }

    /// Remove an element by ID (e.g., remove the thinking indicator).
    pub fn remove_element(&self, id: &str) {
        let eid = js_escape(id);
        self.evaluate_js(&format!("removeElement('{eid}');"));
    }

    /// Scroll the WebView to the bottom of the document.
    pub fn scroll_to_bottom(&self) {
        self.evaluate_js("scrollToBottom();");
    }

    /// Swap the theme CSS (light/dark).
    pub fn set_theme(&self, is_dark: bool) {
        let css = if is_dark {
            dark_theme_css()
        } else {
            light_theme_css()
        };
        let ecss = js_escape(css);
        self.evaluate_js(&format!("setTheme('{ecss}');"));
    }

    /// Remove all messages from the chat container.
    pub fn clear(&self) {
        self.evaluate_js("clearChat();");
    }

    /// Remove the oldest messages, keeping at most `keep` children in #chat.
    pub fn trim_oldest(&self, keep: usize) {
        self.evaluate_js(&format!("trimOldest({keep});"));
    }

    /// Show the "Load previous messages" link at the top.
    pub fn show_load_prev_button(&self) {
        self.evaluate_js("showLoadPrev();");
    }

    /// Hide the "Load previous messages" link.
    pub fn hide_load_prev_button(&self) {
        self.evaluate_js("hideLoadPrev();");
    }

    // --- Internal helpers ---

    /// Generate a unique element ID and bump the counter.
    fn next_id(&self) -> String {
        let n = self.msg_counter.get();
        self.msg_counter.set(n + 1);
        format!("m{n}")
    }

    /// Fire-and-forget JavaScript evaluation.
    ///
    /// If the base HTML document hasn't finished loading yet, the call is
    /// queued and will be flushed in order once `LoadEvent::Finished` fires.
    fn evaluate_js(&self, js: &str) {
        if self.ready.get() {
            self.webview.evaluate_javascript(
                js,
                None,
                None,
                None::<&gtk::gio::Cancellable>,
                |_result| {},
            );
        } else {
            self.pending_js.borrow_mut().push(js.to_string());
        }
    }
}

// ---------------------------------------------------------------------------
// Minimal percent-decoding (avoids pulling in a URL crate just for this)
// ---------------------------------------------------------------------------

fn percent_decode(input: &str) -> String {
    let mut out = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2]))
        {
            out.push(hi << 4 | lo);
            i += 3;
            continue;
        }
        // Also decode '+' as space (form encoding).
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
