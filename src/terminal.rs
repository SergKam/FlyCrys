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

/// Save terminal scrollback content to a file (plain text).
pub fn save_scrollback(terminal: &vte4::Terminal, path: &Path) {
    let file = match gtk::gio::File::for_path(path).replace(
        None,
        false,
        gtk::gio::FileCreateFlags::REPLACE_DESTINATION,
        None::<&gtk::gio::Cancellable>,
    ) {
        Ok(stream) => stream,
        Err(e) => {
            eprintln!("flycrys: failed to create terminal save file: {e}");
            return;
        }
    };
    if let Err(e) = terminal.write_contents_sync(
        &file,
        vte4::WriteFlags::Default,
        None::<&gtk::gio::Cancellable>,
    ) {
        eprintln!("flycrys: failed to save terminal content: {e}");
    }
}

/// Restore previously saved terminal scrollback content.
/// Feeds the text into the terminal display before the shell is spawned,
/// giving visual continuity of previous output.
pub fn restore_scrollback(terminal: &vte4::Terminal, path: &Path) {
    if let Ok(content) = std::fs::read(path)
        && !content.is_empty()
    {
        // Strip trailing blank lines to keep things clean
        let text = String::from_utf8_lossy(&content);
        let trimmed = text.trim_end();
        if !trimmed.is_empty() {
            // Feed as terminal output (rendered, not sent to shell)
            terminal.feed(trimmed.as_bytes());
            // Add a newline separator before the fresh shell prompt
            terminal.feed(b"\r\n");
        }
    }
}
