use gtk::gdk;
use gtk::glib;
use gtk4 as gtk;
use std::path::Path;
use std::process::Command;
use vte4::prelude::*;

use crate::config::constants::{DEFAULT_SHELL, TERMINAL_FONT, TERMINAL_SCROLLBACK_LINES};
use crate::config::types::Theme;

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

/// Apply terminal colors matching the system terminal profile or our theme.
///
/// Tries to read GNOME Terminal's default profile via gsettings (works on
/// GNOME, Cinnamon, and most Ubuntu-family desktops). Falls back to built-in
/// palettes matching the active Theme.
pub fn apply_colors(terminal: &vte4::Terminal, theme: Theme) {
    if let Some(profile) = load_gnome_terminal_profile() {
        let fg = parse_color(&profile.foreground).unwrap_or_else(|| fallback_fg(theme));
        let bg = parse_color(&profile.background).unwrap_or_else(|| fallback_bg(theme));
        let palette: Vec<gdk::RGBA> = profile
            .palette
            .iter()
            .filter_map(|s| parse_color(s))
            .collect();
        if palette.len() >= 16 {
            let refs: Vec<&gdk::RGBA> = palette.iter().collect();
            terminal.set_colors(Some(&fg), Some(&bg), &refs);
        } else {
            apply_builtin_colors(terminal, theme);
        }
    } else {
        apply_builtin_colors(terminal, theme);
    }
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

/// Send a `cd` command to the running shell inside the terminal.
/// Uses feed_child to write to the PTY stdin, simulating user input.
pub fn send_cd(terminal: &vte4::Terminal, directory: &str) {
    // Shell-escape the path by wrapping in single quotes
    // (replace any ' inside the path with '\'' to break out and re-enter)
    let escaped = directory.replace('\'', "'\\''");
    let cmd = format!("cd '{}'\n", escaped);
    terminal.feed_child(cmd.as_bytes());
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

// ---------------------------------------------------------------------------
// GNOME Terminal profile reading
// ---------------------------------------------------------------------------

struct GnomeTerminalProfile {
    foreground: String,
    background: String,
    palette: Vec<String>,
}

/// Try to read the default GNOME Terminal profile colors via gsettings.
fn load_gnome_terminal_profile() -> Option<GnomeTerminalProfile> {
    // Get the default profile UUID
    let uuid = gsettings_get("org.gnome.Terminal.ProfilesList", "default")?;
    let schema =
        format!("org.gnome.Terminal.Legacy.Profile:/org/gnome/terminal/legacy/profiles:/:{uuid}/");

    // Check if the profile uses system theme colors — if so, read the explicit
    // colors anyway (they're the defaults GNOME Terminal would use)
    let fg = gsettings_get(&schema, "foreground-color")?;
    let bg = gsettings_get(&schema, "background-color")?;
    let palette_raw = gsettings_get(&schema, "palette")?;

    // Parse the palette from gsettings array format: ['#xxx', '#yyy', ...]
    let palette: Vec<String> = palette_raw
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|s| s.trim().trim_matches('\'').trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect();

    Some(GnomeTerminalProfile {
        foreground: fg,
        background: bg,
        palette,
    })
}

/// Run `gsettings get <schema> <key>` and return the trimmed, unquoted value.
fn gsettings_get(schema: &str, key: &str) -> Option<String> {
    let output = Command::new("gsettings")
        .args(["get", schema, key])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    // gsettings wraps strings in single quotes: 'value'
    Some(raw.trim_matches('\'').to_string())
}

/// Parse a color string like "#D3D7CF" or "rgb(211,215,207)" into gdk::RGBA.
fn parse_color(s: &str) -> Option<gdk::RGBA> {
    gdk::RGBA::parse(s).ok()
}

// ---------------------------------------------------------------------------
// Built-in fallback palettes (Tango-inspired, matching GNOME defaults)
// ---------------------------------------------------------------------------

fn fallback_fg(theme: Theme) -> gdk::RGBA {
    match theme {
        Theme::Dark => gdk::RGBA::parse("#D3D7CF").unwrap(),
        Theme::Light => gdk::RGBA::parse("#2E3436").unwrap(),
    }
}

fn fallback_bg(theme: Theme) -> gdk::RGBA {
    match theme {
        Theme::Dark => gdk::RGBA::parse("#2E3436").unwrap(),
        Theme::Light => gdk::RGBA::parse("#FAFAFA").unwrap(),
    }
}

/// Standard Tango palette — the same 16 colors GNOME Terminal uses by default.
const TANGO_PALETTE: [&str; 16] = [
    "#2E3436", "#CC0000", "#4E9A06", "#C4A000", "#3465A4", "#75507B", "#06989A", "#D3D7CF",
    "#555753", "#EF2929", "#8AE234", "#FCE94F", "#729FCF", "#AD7FA8", "#34E2E2", "#EEEEEC",
];

fn apply_builtin_colors(terminal: &vte4::Terminal, theme: Theme) {
    let fg = fallback_fg(theme);
    let bg = fallback_bg(theme);
    let palette: Vec<gdk::RGBA> = TANGO_PALETTE
        .iter()
        .filter_map(|s| gdk::RGBA::parse(*s).ok())
        .collect();
    let refs: Vec<&gdk::RGBA> = palette.iter().collect();
    terminal.set_colors(Some(&fg), Some(&bg), &refs);
}
