use gtk::glib;
use gtk4 as gtk;
use std::path::Path;
use vte4::prelude::*;

use crate::config::constants::{DEFAULT_SHELL, TERMINAL_FONT, TERMINAL_SCROLLBACK_LINES};

pub fn create_terminal_panel() -> (gtk::Box, vte4::Terminal) {
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    header.set_margin_start(8);
    header.set_margin_end(4);
    header.set_margin_top(2);
    header.set_margin_bottom(2);

    let label = gtk::Label::new(Some("Terminal"));
    label.set_hexpand(true);
    label.set_xalign(0.0);

    let close_btn = gtk::Button::from_icon_name("window-close-symbolic");
    close_btn.set_has_frame(false);
    close_btn.set_tooltip_text(Some("Close terminal"));

    header.append(&label);
    header.append(&close_btn);

    let terminal = vte4::Terminal::new();
    terminal.set_vexpand(true);
    terminal.set_hexpand(true);
    terminal.set_scrollback_lines(TERMINAL_SCROLLBACK_LINES);

    let font_desc = gtk::pango::FontDescription::from_string(TERMINAL_FONT);
    terminal.set_font(Some(&font_desc));

    vbox.append(&header);
    vbox.append(&terminal);

    // Close button hides the panel
    close_btn.connect_clicked(glib::clone!(
        #[weak]
        vbox,
        move |_| {
            vbox.set_visible(false);
        }
    ));

    (vbox, terminal)
}

pub fn spawn_shell(terminal: &vte4::Terminal, working_directory: &str) {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| DEFAULT_SHELL.to_string());

    terminal.spawn_async(
        vte4::PtyFlags::DEFAULT,
        Some(working_directory),
        &[shell.as_str()],
        &[] as &[&str],
        glib::SpawnFlags::DEFAULT,
        || {},
        -1,
        None::<&gtk::gio::Cancellable>,
        |_result| {},
    );
}

/// Save terminal scrollback content to a file (plain text, non-blocking).
///
/// Captures the scrollback into memory (fast, on main thread), then
/// writes to disk on a background thread so the terminal is never paused.
pub fn save_scrollback(terminal: &vte4::Terminal, path: &Path) {
    // Step 1: capture scrollback into memory (microseconds)
    let mem_stream = gtk::gio::MemoryOutputStream::new_resizable();
    if let Err(e) = terminal.write_contents_sync(
        &mem_stream,
        vte4::WriteFlags::Default,
        None::<&gtk::gio::Cancellable>,
    ) {
        eprintln!("flycrys: failed to capture terminal content: {e}");
        return;
    }
    if mem_stream.close(None::<&gtk::gio::Cancellable>).is_err() {
        return;
    }
    let bytes = mem_stream.steal_as_bytes();

    // Step 2: write to disk on a background thread
    let file_path = path.to_path_buf();
    std::thread::spawn(move || {
        if let Err(e) = std::fs::write(&file_path, &bytes) {
            eprintln!("flycrys: failed to write terminal save file: {e}");
        }
    });
}

/// Restore previously saved terminal scrollback content.
/// Feeds the text into the terminal display before the shell is spawned,
/// giving visual continuity of previous output.
///
/// Note: `write_contents_sync` saves plain text (no ANSI colors) with `\n`
/// line endings, but VTE `feed()` interprets raw terminal output where `\n`
/// only moves the cursor down (not back to column 0). We convert `\n` to
/// `\r\n` so lines render correctly, and dim the restored text so the user
/// knows it's historical (not a live session).
pub fn restore_scrollback(terminal: &vte4::Terminal, path: &Path) {
    if let Ok(content) = std::fs::read(path)
        && !content.is_empty()
    {
        let text = String::from_utf8_lossy(&content);
        let trimmed = text.trim_end();
        if !trimmed.is_empty() {
            // Dim attribute (SGR 2) so restored text is visually distinct
            terminal.feed(b"\x1b[2m");
            // Convert bare \n to \r\n for proper cursor positioning in feed()
            let fixed = trimmed.replace('\n', "\r\n");
            terminal.feed(fixed.as_bytes());
            // Reset attributes and add separator before fresh shell prompt
            terminal.feed(b"\x1b[0m\r\n");
        }
    }
}
