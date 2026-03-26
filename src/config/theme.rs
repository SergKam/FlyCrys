use super::types::Theme;

/// Return the CSS for the given theme.
pub fn css_for_theme(theme: Theme) -> &'static str {
    match theme {
        Theme::Light => light_css(),
        Theme::Dark => dark_css(),
    }
}

fn light_css() -> &'static str {
    r#"
    .user-message { background: alpha(@accent_bg_color, 0.15); border-radius: 8px; }
    .system-info { color: alpha(@window_fg_color, 0.5); font-size: small; }
    .error-text { color: @error_color; }
    .monospace { font-family: monospace; font-size: 0.9em; }
    .code-view text { background-color: #ffffff; color: #333333; }
    .line-gutter text { background-color: #f0f0f0; color: #999999; }
    .image-thumb { border-radius: 4px; }
    .attach-thumb { border-radius: 4px; border: 1px solid alpha(@window_fg_color, 0.2); }
    button.file-link { padding: 0 2px; min-height: 0; min-width: 0; }
    listview.file-tree > row:selected {
        border-left: 3px solid #3584e4;
        font-weight: bold;
    }
    paned > separator { background-color: #c0c0c0; min-width: 2px; min-height: 2px; }
    notebook header tabs tab { min-height: 0; padding: 4px 8px; }
    .toolbar-info { font-size: small; color: alpha(@window_fg_color, 0.55); margin: 0 4px; }
    .git-modified { color: #e5a50a; font-weight: bold; }
    .git-added { color: #2ec27e; font-weight: bold; }
    .git-deleted { color: #e01b24; font-weight: bold; }
    .git-untracked { color: alpha(@window_fg_color, 0.45); }
    "#
}

fn dark_css() -> &'static str {
    r#"
    .user-message { background: alpha(@accent_bg_color, 0.15); border-radius: 8px; }
    .system-info { color: alpha(@window_fg_color, 0.5); font-size: small; }
    .error-text { color: @error_color; }
    .monospace { font-family: monospace; font-size: 0.9em; }
    .code-view text { background-color: #2d2d2d; color: #d3d0c8; }
    .line-gutter text { background-color: #252525; color: #666666; }
    .image-thumb { border-radius: 4px; }
    .attach-thumb { border-radius: 4px; border: 1px solid alpha(@window_fg_color, 0.2); }
    button.file-link { padding: 0 2px; min-height: 0; min-width: 0; }
    listview.file-tree > row:selected {
        border-left: 3px solid #3584e4;
        font-weight: bold;
    }
    paned > separator { background-color: #555555; min-width: 2px; min-height: 2px; }
    notebook header tabs tab { min-height: 0; padding: 4px 8px; }
    .toolbar-info { font-size: small; color: alpha(@window_fg_color, 0.55); margin: 0 4px; }
    .git-modified { color: #e5a50a; font-weight: bold; }
    .git-added { color: #2ec27e; font-weight: bold; }
    .git-deleted { color: #e01b24; font-weight: bold; }
    .git-untracked { color: alpha(@window_fg_color, 0.45); }
    "#
}
