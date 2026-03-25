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
    .tool-call {
        background-color: #ffffff;
        border: 1px solid #d0d0d0;
        border-radius: 6px;
        padding: 6px;
    }
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

    /* ── Markdown block widgets ── */
    .md-paragraph { margin-top: 2px; margin-bottom: 6px; }
    .md-heading-1 { margin-top: 16px; margin-bottom: 8px; }
    .md-heading-2 { margin-top: 12px; margin-bottom: 6px; }
    .md-heading-3 { margin-top: 8px; margin-bottom: 4px; }
    .md-code-block {
        background-color: #f6f8fa;
        border-radius: 6px;
        padding: 10px 12px;
        margin-top: 4px;
        margin-bottom: 4px;
    }
    .md-code-label { color: #333333; }
    .md-blockquote {
        border-left: 3px solid #d0d7de;
        padding: 2px 0 2px 12px;
        margin-top: 4px;
        margin-bottom: 4px;
    }
    .md-blockquote label { color: #656d76; }
    .md-table-header-cell {
        background-color: #f6f8fa;
        padding: 6px 12px;
        border-bottom: 1px solid #d0d7de;
    }
    .md-table-cell {
        padding: 6px 12px;
        border-bottom: 1px solid #eaeef2;
    }
    .md-list-bullet { min-width: 18px; }
    "#
}

fn dark_css() -> &'static str {
    r#"
    .user-message { background: alpha(@accent_bg_color, 0.15); border-radius: 8px; }
    .tool-call {
        background-color: #383838;
        border: 1px solid #555555;
        border-radius: 6px;
        padding: 6px;
    }
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

    /* ── Markdown block widgets ── */
    .md-paragraph { margin-top: 2px; margin-bottom: 6px; }
    .md-heading-1 { margin-top: 16px; margin-bottom: 8px; }
    .md-heading-2 { margin-top: 12px; margin-bottom: 6px; }
    .md-heading-3 { margin-top: 8px; margin-bottom: 4px; }
    .md-code-block {
        background-color: #2a2a2a;
        border-radius: 6px;
        padding: 10px 12px;
        margin-top: 4px;
        margin-bottom: 4px;
    }
    .md-code-label { color: #e0e0e0; }
    .md-blockquote {
        border-left: 3px solid #555555;
        padding: 2px 0 2px 12px;
        margin-top: 4px;
        margin-bottom: 4px;
    }
    .md-blockquote label { color: #9e9e9e; }
    .md-table-header-cell {
        background-color: #333333;
        padding: 6px 12px;
        border-bottom: 1px solid #555555;
    }
    .md-table-cell {
        padding: 6px 12px;
        border-bottom: 1px solid #444444;
    }
    .md-list-bullet { min-width: 18px; }
    "#
}
