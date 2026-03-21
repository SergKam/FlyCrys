use std::process::Command;

use crate::config::constants::{DEFAULT_SHELL, KNOWN_BROWSERS, KNOWN_EDITORS};

/// Open a file/folder with the desktop's default handler (xdg-open).
pub fn open_with_default(path: &str) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("xdg-open failed: {e}"))
}

/// Open a file in a text editor.
/// Checks: $VISUAL -> $EDITOR -> known editors -> xdg-open fallback.
pub fn open_in_editor(path: &str) -> Result<(), String> {
    for var in &["VISUAL", "EDITOR"] {
        if let Ok(editor) = std::env::var(var)
            && Command::new(&editor).arg(path).spawn().is_ok()
        {
            return Ok(());
        }
    }
    for editor in KNOWN_EDITORS {
        if Command::new(editor).arg(path).spawn().is_ok() {
            return Ok(());
        }
    }
    // Last resort: xdg-open
    open_with_default(path)
}

/// Open a URL in the user's preferred browser.
/// Checks: $BROWSER -> xdg-settings -> gtk-launch -> known browsers -> xdg-open.
pub fn open_in_browser(uri: &str) -> Result<(), String> {
    // Try $BROWSER first
    if let Ok(browser) = std::env::var("BROWSER")
        && Command::new(&browser).arg(uri).spawn().is_ok()
    {
        return Ok(());
    }
    // Try xdg-settings to find the default browser
    if let Ok(out) = Command::new("xdg-settings")
        .args(["get", "default-web-browser"])
        .output()
    {
        let desktop = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !desktop.is_empty()
            && Command::new("gtk-launch")
                .arg(&desktop)
                .arg(uri)
                .spawn()
                .is_ok()
        {
            return Ok(());
        }
    }
    // Fallback: sensible-browser (Debian/Ubuntu), x-www-browser, then common browsers
    for browser in KNOWN_BROWSERS {
        if Command::new(browser).arg(uri).spawn().is_ok() {
            return Ok(());
        }
    }
    // Last resort
    Command::new("xdg-open")
        .arg(uri)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not open browser: {e}"))
}

/// Get the user's default shell.
pub fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| DEFAULT_SHELL.to_string())
}

/// Open a path as a file:// URI in the user's preferred browser.
pub fn open_file_in_browser(path: &str) -> Result<(), String> {
    let uri = format!("file://{}", path);
    open_in_browser(&uri)
}
