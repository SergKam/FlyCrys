mod agent_panel;
mod agent_process;
mod agent_widgets;
mod file_entry;
mod highlight;
mod markdown;
mod terminal;
mod textview;
mod tree;

use file_entry::FileEntry;
use gtk4 as gtk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const APP_ID: &str = "com.flycrys.app";

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
    .image-thumb { border-radius: 4px; }
    .attach-thumb { border-radius: 4px; border: 1px solid alpha(@window_fg_color, 0.2); }
    button.file-link { padding: 0 2px; min-height: 0; min-width: 0; }
    listview.file-tree > row:selected { background-color: #3584e4; color: #ffffff; }
    listview.file-tree > row:selected:hover { background-color: #3584e4; color: #ffffff; }
    listview.file-tree > row:hover:not(:selected) { background-color: #d4e4f7; }
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
    .image-thumb { border-radius: 4px; }
    .attach-thumb { border-radius: 4px; border: 1px solid alpha(@window_fg_color, 0.2); }
    button.file-link { padding: 0 2px; min-height: 0; min-width: 0; }
    listview.file-tree > row:selected { background-color: #3584e4; color: #ffffff; }
    listview.file-tree > row:selected:hover { background-color: #3584e4; color: #ffffff; }
    listview.file-tree > row:hover:not(:selected) { background-color: rgba(53, 132, 228, 0.15); }
    "#
}

fn main() -> glib::ExitCode {
    let app = gtk::Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_startup(|_app| {
        // Add bundled icons to the default icon theme
        let icon_theme = gtk::IconTheme::for_display(&gtk::gdk::Display::default().unwrap());
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()));
        // Check next to the binary first (installed), then project root (dev)
        let candidates = [
            exe_dir.as_ref().map(|d| d.join("icons")),
            exe_dir.as_ref().map(|d| d.join("../icons")),
            Some(std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/icons"))),
        ];
        for candidate in candidates.into_iter().flatten() {
            if candidate.is_dir() {
                icon_theme.add_search_path(&candidate);
                break;
            }
        }
    });

    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &gtk::Application) {
    // Theme state
    let is_dark = Rc::new(Cell::new(false));
    let current_file = Rc::new(RefCell::new(String::new()));

    // Left pane: file tree
    let (tree_scroll, list_view, selection) = tree::create_file_tree();

    // Right pane top: text view
    let (text_container, text_view, path_label) = textview::create_text_view();

    // Right pane bottom: terminal (initially hidden)
    let (terminal_container, vte_terminal) = terminal::create_terminal_panel();
    terminal_container.set_visible(false);

    let right_paned = gtk::Paned::new(gtk::Orientation::Vertical);
    right_paned.set_start_child(Some(&text_container));
    right_paned.set_end_child(Some(&terminal_container));
    right_paned.set_resize_start_child(true);
    right_paned.set_resize_end_child(true);
    right_paned.set_shrink_start_child(false);
    right_paned.set_shrink_end_child(false);

    // Main horizontal split
    let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
    paned.set_position(300);
    paned.set_start_child(Some(&tree_scroll));
    paned.set_end_child(Some(&right_paned));

    // Single-click: open files / toggle directories
    list_view.connect_activate(glib::clone!(
        #[weak] selection,
        #[weak] text_view,
        #[weak] path_label,
        #[strong] current_file,
        #[strong] is_dark,
        move |_view, position| {
            let Some(item) = selection.item(position) else {
                return;
            };
            let row = item.downcast_ref::<gtk::TreeListRow>().unwrap();
            let entry = row.item().and_downcast::<FileEntry>().unwrap();

            if entry.is_dir() {
                row.set_expanded(!row.is_expanded());
            } else {
                let path = entry.path();
                let theme = if is_dark.get() { highlight::DARK_THEME } else { highlight::LIGHT_THEME };
                textview::load_file(&text_view, &path_label, &path, theme);
                *current_file.borrow_mut() = path;
            }
        }
    ));

    // --- Right-click context menu ---
    let ctx_path: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
    let ctx_is_dir: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));

    let menu = gio::Menu::new();
    menu.append(Some("Copy Path"), Some("win.copy-path"));
    menu.append(Some("Add to Chat"), Some("win.add-to-chat"));
    menu.append(Some("Open Terminal Here"), Some("win.open-terminal-here"));

    let popover = gtk::PopoverMenu::from_model(Some(&menu));
    popover.set_parent(&list_view);
    popover.set_has_arrow(false);

    let right_click = gtk::GestureClick::new();
    right_click.set_button(3);
    right_click.connect_pressed(glib::clone!(
        #[weak] list_view,
        #[weak] popover,
        #[strong] ctx_path,
        #[strong] ctx_is_dir,
        move |_gesture, _n_press, x, y| {
            // Walk up from the picked widget to find a TreeExpander
            let Some(widget) = list_view.pick(x, y, gtk::PickFlags::DEFAULT) else {
                return;
            };
            let mut current = Some(widget);
            while let Some(w) = current {
                if let Some(expander) = w.downcast_ref::<gtk::TreeExpander>() {
                    if let Some(item) = expander.item() {
                        if let Some(entry) = item.downcast_ref::<FileEntry>() {
                            *ctx_path.borrow_mut() = entry.path();
                            *ctx_is_dir.borrow_mut() = entry.is_dir();

                            popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(
                                x as i32, y as i32, 1, 1,
                            )));
                            popover.popup();
                        }
                    }
                    return;
                }
                current = w.parent();
            }
        }
    ));
    list_view.add_controller(right_click);

    // CSS
    let css = gtk::CssProvider::new();
    css.load_from_string(light_css());
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().unwrap(),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // Theme change callback
    let on_theme_change: Rc<dyn Fn(bool)> = {
        let css = css.clone();
        let text_view = text_view.clone();
        let current_file = Rc::clone(&current_file);
        let is_dark = Rc::clone(&is_dark);
        Rc::new(move |dark: bool| {
            is_dark.set(dark);
            css.load_from_string(if dark { dark_css() } else { light_css() });

            if let Some(settings) = gtk::Settings::default() {
                settings.set_gtk_application_prefer_dark_theme(dark);
            }

            // Re-highlight current file
            let file_path = current_file.borrow().clone();
            if !file_path.is_empty() {
                let theme = if dark { highlight::DARK_THEME } else { highlight::LIGHT_THEME };
                let buffer = text_view.buffer();
                let content = buffer
                    .text(&buffer.start_iter(), &buffer.end_iter(), false)
                    .to_string();
                if highlight::is_highlightable(&file_path) && !content.is_empty() {
                    highlight::highlight_buffer_with_theme(
                        &buffer, &content, &file_path, theme,
                    );
                }
            }
        })
    };

    // Agent panel (right side) — with callback to open files in viewer + tree
    let on_open_file: Rc<dyn Fn(&str)> = {
        let text_view = text_view.clone();
        let path_label = path_label.clone();
        let selection = selection.clone();
        let current_file = Rc::clone(&current_file);
        let is_dark = Rc::clone(&is_dark);
        Rc::new(move |file_path: &str| {
            let theme = if is_dark.get() { highlight::DARK_THEME } else { highlight::LIGHT_THEME };
            textview::load_file(&text_view, &path_label, file_path, theme);
            *current_file.borrow_mut() = file_path.to_string();
            select_file_in_tree(&selection, file_path);
        })
    };
    let (agent_panel, agent_input) = agent_panel::create_agent_panel(on_open_file, on_theme_change);

    let outer_paned = gtk::Paned::new(gtk::Orientation::Horizontal);
    outer_paned.set_start_child(Some(&paned));
    outer_paned.set_end_child(Some(&agent_panel));
    outer_paned.set_position(980);
    outer_paned.set_resize_start_child(true);
    outer_paned.set_resize_end_child(false);
    outer_paned.set_shrink_start_child(false);
    outer_paned.set_shrink_end_child(false);

    // Window
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("FlyCrys")
        .icon_name(APP_ID)
        .default_width(1400)
        .default_height(800)
        .child(&outer_paned)
        .build();

    // Action: Copy Path
    let action_copy = gio::SimpleAction::new("copy-path", None);
    action_copy.connect_activate(glib::clone!(
        #[weak] window,
        #[strong] ctx_path,
        move |_, _| {
            let path = ctx_path.borrow().clone();
            if !path.is_empty() {
                window.clipboard().set_text(&path);
            }
        }
    ));
    window.add_action(&action_copy);

    // Action: Open Terminal Here
    let action_terminal = gio::SimpleAction::new("open-terminal-here", None);
    action_terminal.connect_activate(glib::clone!(
        #[weak] terminal_container,
        #[weak] vte_terminal,
        #[strong] ctx_path,
        #[strong] ctx_is_dir,
        move |_, _| {
            let path = ctx_path.borrow().clone();
            let is_dir = *ctx_is_dir.borrow();
            if path.is_empty() {
                return;
            }
            let dir = if is_dir {
                path
            } else {
                std::path::Path::new(&path)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| "/".to_string())
            };
            terminal_container.set_visible(true);
            terminal::spawn_shell(&vte_terminal, &dir);
        }
    ));
    window.add_action(&action_terminal);

    // Action: Add to Chat
    let action_add_chat = gio::SimpleAction::new("add-to-chat", None);
    action_add_chat.connect_activate(glib::clone!(
        #[weak] agent_input,
        #[strong] ctx_path,
        move |_, _| {
            let path = ctx_path.borrow().clone();
            if !path.is_empty() {
                append_path_to_input(&agent_input, &path);
            }
        }
    ));
    window.add_action(&action_add_chat);

    // --- Drag from file tree → drop on agent input ---

    // Drag source on the ListView
    let drag_source = gtk::DragSource::new();
    drag_source.set_actions(gtk::gdk::DragAction::COPY);
    drag_source.connect_prepare(glib::clone!(
        #[weak] list_view,
        #[upgrade_or] None,
        move |_source, x, y| {
            // Find the FileEntry at (x, y) via TreeExpander walk
            let widget = list_view.pick(x, y, gtk::PickFlags::DEFAULT)?;
            let mut current = Some(widget);
            while let Some(w) = current {
                if let Some(expander) = w.downcast_ref::<gtk::TreeExpander>() {
                    if let Some(item) = expander.item() {
                        if let Some(entry) = item.downcast_ref::<FileEntry>() {
                            let path = entry.path();
                            return Some(gtk::gdk::ContentProvider::for_value(
                                &path.to_value(),
                            ));
                        }
                    }
                    return None;
                }
                current = w.parent();
            }
            None
        }
    ));
    list_view.add_controller(drag_source);

    // Drop target on the agent input TextView
    let drop_target = gtk::DropTarget::new(glib::Type::STRING, gtk::gdk::DragAction::COPY);
    drop_target.connect_drop(glib::clone!(
        #[weak] agent_input,
        #[upgrade_or] false,
        move |_target, value, _x, _y| {
            if let Ok(path) = value.get::<String>() {
                append_path_to_input(&agent_input, &path);
                return true;
            }
            false
        }
    ));
    agent_input.add_controller(drop_target);

    // Also accept drops anywhere on the agent panel
    let panel_drop = gtk::DropTarget::new(glib::Type::STRING, gtk::gdk::DragAction::COPY);
    panel_drop.connect_drop(glib::clone!(
        #[weak] agent_input,
        #[upgrade_or] false,
        move |_target, value, _x, _y| {
            if let Ok(path) = value.get::<String>() {
                append_path_to_input(&agent_input, &path);
                return true;
            }
            false
        }
    ));
    agent_panel.add_controller(panel_drop);

    window.present();
}

fn append_path_to_input(input: &gtk::TextView, path: &str) {
    let buffer = input.buffer();
    let mut end = buffer.end_iter();
    let current = buffer.text(&buffer.start_iter(), &end, false);
    let prefix = if current.is_empty() || current.ends_with(' ') || current.ends_with('\n') {
        ""
    } else {
        " "
    };
    buffer.insert(&mut end, &format!("{prefix}{path}"));
}

/// Navigate the file tree to select a file, expanding parent directories as needed
fn select_file_in_tree(selection: &gtk::SingleSelection, target_path: &str) {
    for _pass in 0..30 {
        let n = selection.n_items();
        let mut expanded_any = false;
        for i in 0..n {
            let Some(item) = selection.item(i) else {
                continue;
            };
            let Some(row) = item.downcast_ref::<gtk::TreeListRow>() else {
                continue;
            };
            let Some(entry) = row.item().and_downcast::<FileEntry>() else {
                continue;
            };

            if entry.path() == target_path {
                selection.set_selected(i);
                return;
            }

            let entry_path = entry.path();
            if entry.is_dir()
                && !row.is_expanded()
                && target_path.starts_with(&entry_path)
                && target_path.as_bytes().get(entry_path.len()) == Some(&b'/')
            {
                row.set_expanded(true);
                expanded_any = true;
                break; // Restart scan since indices changed
            }
        }
        if !expanded_any {
            break;
        }
    }
}
