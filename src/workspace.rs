use gtk4 as gtk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::file_entry::FileEntry;
use crate::session::{self, WorkspaceConfig};
use crate::watcher::FileWatcher;
use crate::{agent_panel, highlight, terminal, textview, tree};

/// All the widgets for a single workspace tab
pub struct Workspace {
    pub root: gtk::Box,
    pub config: Rc<RefCell<WorkspaceConfig>>,
    pub tab_spinner: gtk::Spinner,
    pub chat_history: Rc<RefCell<Vec<session::ChatMessage>>>,
    /// Called on theme change to re-highlight the current file.
    pub on_theme_rehighlight: Rc<dyn Fn(bool)>,
    // Prevent drop — stopping the watcher closes the channel and the GTK timer exits.
    _file_watcher: Option<FileWatcher>,
}

impl Workspace {
    pub fn new(
        config: WorkspaceConfig,
        is_dark: Rc<Cell<bool>>,
    ) -> Self {
        let working_dir = PathBuf::from(&config.working_directory);
        let config = Rc::new(RefCell::new(config));

        // Current file in text viewer
        let current_file = Rc::new(RefCell::new(String::new()));

        // Left pane: file tree
        let (tree_scroll, list_view, selection, dir_stores) = tree::create_file_tree(&working_dir);

        // Right pane top: text view
        let (text_container, text_view, gutter, path_label) = textview::create_text_view();

        // Right pane bottom: terminal (initially hidden)
        let (terminal_container, vte_terminal) = terminal::create_terminal_panel();
        terminal_container.set_visible(config.borrow().terminal_visible);

        let right_paned = gtk::Paned::new(gtk::Orientation::Vertical);
        right_paned.set_start_child(Some(&text_container));
        right_paned.set_end_child(Some(&terminal_container));
        right_paned.set_resize_start_child(true);
        right_paned.set_resize_end_child(true);
        right_paned.set_shrink_start_child(false);
        right_paned.set_shrink_end_child(false);
        {
            let pos = config.borrow().editor_terminal_split;
            if pos > 0 {
                right_paned.set_position(pos);
            }
        }

        // Main horizontal split (tree | editor+terminal)
        let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
        paned.set_position(config.borrow().tree_pane_width);
        paned.set_start_child(Some(&tree_scroll));
        paned.set_end_child(Some(&right_paned));

        // Single-click: open files / toggle directories
        // Note: we don't use set_single_click_activate(true) because it moves
        // selection on hover. Instead we handle clicks manually via GestureClick.
        let left_click = gtk::GestureClick::new();
        left_click.set_button(1);
        left_click.connect_pressed(glib::clone!(
            #[weak] list_view,
            #[weak] text_view,
            #[weak] gutter,
            #[weak] path_label,
            #[strong] current_file,
            #[strong] is_dark,
            #[strong] config,
            move |_gesture, n_press, x, y| {
                if n_press != 1 {
                    return;
                }
                let Some(picked) = list_view.pick(x, y, gtk::PickFlags::DEFAULT) else {
                    return;
                };
                let mut current = Some(picked.clone());
                while let Some(w) = current {
                    if let Some(expander) = w.downcast_ref::<gtk::TreeExpander>() {
                        if let Some(row) = expander.list_row() {
                            if let Some(entry) = row.item().and_downcast::<FileEntry>() {
                                if entry.is_dir() {
                                    let on_content = expander.child().is_some_and(|child| {
                                        let child_w: gtk::Widget = child.upcast();
                                        let mut check = Some(picked.clone());
                                        while let Some(c) = check {
                                            if c == child_w {
                                                return true;
                                            }
                                            check = c.parent();
                                        }
                                        false
                                    });
                                    if on_content {
                                        row.set_expanded(!row.is_expanded());
                                    }
                                } else {
                                    let path = entry.path();
                                    let theme = if is_dark.get() { highlight::DARK_THEME } else { highlight::LIGHT_THEME };
                                    textview::load_file(&text_view, &gutter, &path_label, &path, theme);
                                    *current_file.borrow_mut() = path.clone();
                                    config.borrow_mut().open_file = Some(path);
                                }
                            }
                        }
                        return;
                    }
                    current = w.parent();
                }
            }
        ));
        list_view.add_controller(left_click);

        // --- Right-click context menu ---
        let ctx_path: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
        let ctx_is_dir: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));

        let menu = gio::Menu::new();
        menu.append(Some("Copy Path"), Some("ws.copy-path"));
        menu.append(Some("Add to Chat"), Some("ws.add-to-chat"));
        menu.append(Some("Open Terminal Here"), Some("ws.open-terminal-here"));

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

        // Agent panel callbacks
        let on_open_file: Rc<dyn Fn(&str)> = {
            let text_view = text_view.clone();
            let gutter = gutter.clone();
            let path_label = path_label.clone();
            let selection = selection.clone();
            let current_file = Rc::clone(&current_file);
            let is_dark = Rc::clone(&is_dark);
            let config = Rc::clone(&config);
            Rc::new(move |file_path: &str| {
                let theme = if is_dark.get() { highlight::DARK_THEME } else { highlight::LIGHT_THEME };
                textview::load_file(&text_view, &gutter, &path_label, file_path, theme);
                *current_file.borrow_mut() = file_path.to_string();
                config.borrow_mut().open_file = Some(file_path.to_string());
                select_file_in_tree(&selection, file_path);
            })
        };

        // Load agent profiles
        let agent_configs = session::list_agent_configs();

        // Tab spinner: shown in notebook tab when agent is working
        let tab_spinner = gtk::Spinner::new();
        tab_spinner.set_size_request(12, 12);

        // Chat history
        let chat_history = Rc::new(RefCell::new(
            session::load_chat_history(&config.borrow().id),
        ));

        // Agent panel
        let (agent_panel_1, agent_input_1) = {
            let profile = config.borrow().agent_1_profile.clone();
            let session_id = config.borrow().agent_1_session_id.clone();
            let cfg = Rc::clone(&config);
            let on_profile = Rc::new(move |name: &str| {
                cfg.borrow_mut().agent_1_profile = name.to_string();
            });
            let cfg = Rc::clone(&config);
            let on_session = Rc::new(move |id: Option<String>| {
                cfg.borrow_mut().agent_1_session_id = id;
            });
            agent_panel::create_agent_panel(
                Rc::clone(&on_open_file),
                Rc::clone(&is_dark),
                tab_spinner.clone(),
                &working_dir,
                "Agent",
                agent_configs,
                &profile,
                session_id,
                on_profile,
                on_session,
                Rc::clone(&chat_history),
            )
        };

        let agent_container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        agent_container.set_width_request(420);
        agent_container.append(&agent_panel_1);

        // Outer split: agents | content
        let outer_paned = gtk::Paned::new(gtk::Orientation::Horizontal);
        outer_paned.set_start_child(Some(&agent_container));
        outer_paned.set_end_child(Some(&paned));
        outer_paned.set_position(config.borrow().agent_pane_width);
        outer_paned.set_resize_start_child(false);
        outer_paned.set_resize_end_child(true);
        outer_paned.set_shrink_start_child(false);
        outer_paned.set_shrink_end_child(false);

        // Root container
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.set_vexpand(true);
        root.set_hexpand(true);
        root.append(&outer_paned);

        // --- Actions (scoped to this workspace via ActionGroup) ---
        let action_group = gio::SimpleActionGroup::new();

        // Copy Path
        let action_copy = gio::SimpleAction::new("copy-path", None);
        {
            let ctx_path = Rc::clone(&ctx_path);
            let root = root.clone();
            action_copy.connect_activate(move |_, _| {
                let path = ctx_path.borrow().clone();
                if !path.is_empty() {
                    root.clipboard().set_text(&path);
                }
            });
        }
        action_group.add_action(&action_copy);

        // Open Terminal Here
        let action_terminal = gio::SimpleAction::new("open-terminal-here", None);
        {
            let ctx_path = Rc::clone(&ctx_path);
            let ctx_is_dir = Rc::clone(&ctx_is_dir);
            let terminal_container = terminal_container.clone();
            let vte_terminal = vte_terminal.clone();
            let config = Rc::clone(&config);
            action_terminal.connect_activate(move |_, _| {
                let path = ctx_path.borrow().clone();
                let is_dir = *ctx_is_dir.borrow();
                if path.is_empty() {
                    return;
                }
                let dir = if is_dir {
                    path
                } else {
                    Path::new(&path)
                        .parent()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|| "/".to_string())
                };
                terminal_container.set_visible(true);
                config.borrow_mut().terminal_visible = true;
                terminal::spawn_shell(&vte_terminal, &dir);
            });
        }
        action_group.add_action(&action_terminal);

        // Add to Chat (sends to agent 1 input)
        let action_add_chat = gio::SimpleAction::new("add-to-chat", None);
        {
            let agent_input = agent_input_1.clone();
            let ctx_path = Rc::clone(&ctx_path);
            action_add_chat.connect_activate(move |_, _| {
                let path = ctx_path.borrow().clone();
                if !path.is_empty() {
                    append_path_to_input(&agent_input, &path);
                }
            });
        }
        action_group.add_action(&action_add_chat);

        // --- Gutter right-click: Copy Line Link / Add Line Link to Chat ---
        let gutter_ctx_line: Rc<Cell<u32>> = Rc::new(Cell::new(0));

        let gutter_menu = gio::Menu::new();
        gutter_menu.append(Some("Copy Line Link"), Some("ws.copy-line-link"));
        gutter_menu.append(Some("Add Line Link to Chat"), Some("ws.add-line-link-to-chat"));

        let gutter_popover = gtk::PopoverMenu::from_model(Some(&gutter_menu));
        gutter_popover.set_parent(&gutter);
        gutter_popover.set_has_arrow(false);

        let gutter_click = gtk::GestureClick::new();
        gutter_click.set_button(3);
        gutter_click.connect_pressed(glib::clone!(
            #[weak] gutter,
            #[weak] gutter_popover,
            #[strong] gutter_ctx_line,
            move |gesture, _n_press, x, y| {
                // Claim the sequence so the default TextView context menu is suppressed
                gesture.set_state(gtk::EventSequenceState::Claimed);
                let (bx, by) = gutter.window_to_buffer_coords(
                    gtk::TextWindowType::Widget,
                    x as i32,
                    y as i32,
                );
                if let Some(iter) = gutter.iter_at_location(bx, by) {
                    gutter_ctx_line.set(iter.line() as u32 + 1);
                    gutter_popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(
                        x as i32, y as i32, 1, 1,
                    )));
                    gutter_popover.popup();
                }
            }
        ));
        gutter.add_controller(gutter_click);

        // Copy Line Link
        let action_copy_line = gio::SimpleAction::new("copy-line-link", None);
        {
            let current_file = Rc::clone(&current_file);
            let gutter_ctx_line = Rc::clone(&gutter_ctx_line);
            let root = root.clone();
            action_copy_line.connect_activate(move |_, _| {
                let file = current_file.borrow().clone();
                let line = gutter_ctx_line.get();
                if !file.is_empty() && line > 0 {
                    root.clipboard().set_text(&format!("{}:{}", file, line));
                }
            });
        }
        action_group.add_action(&action_copy_line);

        // Add Line Link to Chat
        let action_add_line = gio::SimpleAction::new("add-line-link-to-chat", None);
        {
            let current_file = Rc::clone(&current_file);
            let gutter_ctx_line = Rc::clone(&gutter_ctx_line);
            let agent_input = agent_input_1.clone();
            action_add_line.connect_activate(move |_, _| {
                let file = current_file.borrow().clone();
                let line = gutter_ctx_line.get();
                if !file.is_empty() && line > 0 {
                    append_path_to_input(&agent_input, &format!("{}:{}", file, line));
                }
            });
        }
        action_group.add_action(&action_add_line);

        root.insert_action_group("ws", Some(&action_group));

        // --- Drag from file tree → drop on agent input ---
        let drag_source = gtk::DragSource::new();
        drag_source.set_actions(gtk::gdk::DragAction::COPY);
        drag_source.connect_prepare(glib::clone!(
            #[weak] list_view,
            #[upgrade_or] None,
            move |_source, x, y| {
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

        // Drop target on agent input
        let drop_target = gtk::DropTarget::new(glib::Type::STRING, gtk::gdk::DragAction::COPY);
        drop_target.connect_drop(glib::clone!(
            #[weak] agent_input_1,
            #[upgrade_or] false,
            move |_target, value, _x, _y| {
                if let Ok(path) = value.get::<String>() {
                    append_path_to_input(&agent_input_1, &path);
                    return true;
                }
                false
            }
        ));
        agent_input_1.add_controller(drop_target);

        // Drop on agent panel area
        let panel_drop = gtk::DropTarget::new(glib::Type::STRING, gtk::gdk::DragAction::COPY);
        panel_drop.connect_drop(glib::clone!(
            #[weak] agent_input_1,
            #[upgrade_or] false,
            move |_target, value, _x, _y| {
                if let Ok(path) = value.get::<String>() {
                    append_path_to_input(&agent_input_1, &path);
                    return true;
                }
                false
            }
        ));
        agent_container.add_controller(panel_drop);

        // Autosave pane positions on change
        {
            let config = Rc::clone(&config);
            paned.connect_position_notify(glib::clone!(
                #[strong] config,
                move |p| {
                    config.borrow_mut().tree_pane_width = p.position();
                }
            ));
        }
        {
            let config = Rc::clone(&config);
            outer_paned.connect_position_notify(glib::clone!(
                #[strong] config,
                move |p| {
                    config.borrow_mut().agent_pane_width = p.position();
                }
            ));
        }
        {
            let config = Rc::clone(&config);
            right_paned.connect_position_notify(glib::clone!(
                #[strong] config,
                move |p| {
                    config.borrow_mut().editor_terminal_split = p.position();
                }
            ));
        }
        // Load initial file if restored from session
        {
            let open_file = config.borrow().open_file.clone();
            if let Some(ref path) = open_file {
                let theme = if is_dark.get() { highlight::DARK_THEME } else { highlight::LIGHT_THEME };
                textview::load_file(&text_view, &gutter, &path_label, path, theme);
                *current_file.borrow_mut() = path.clone();
                select_file_in_tree(&selection, path);
            }
        }

        // File-system watcher: auto-refresh tree and current file on changes
        let file_watcher = {
            let on_file_changed: Rc<dyn Fn()> = {
                let text_view = text_view.clone();
                let gutter = gutter.clone();
                let path_label = path_label.clone();
                let current_file = Rc::clone(&current_file);
                let is_dark = Rc::clone(&is_dark);
                Rc::new(move || {
                    let file = current_file.borrow().clone();
                    if !file.is_empty() {
                        let theme = if is_dark.get() {
                            highlight::DARK_THEME
                        } else {
                            highlight::LIGHT_THEME
                        };
                        textview::load_file(&text_view, &gutter, &path_label, &file, theme);
                    }
                })
            };

            let on_file_removed: Rc<dyn Fn()> = {
                let text_view = text_view.clone();
                let gutter = gutter.clone();
                let path_label = path_label.clone();
                let current_file = Rc::clone(&current_file);
                let selection = selection.clone();
                let config = Rc::clone(&config);
                Rc::new(move || {
                    textview::clear_view(&text_view, &gutter, &path_label);
                    *current_file.borrow_mut() = String::new();
                    config.borrow_mut().open_file = None;
                    selection.set_selected(gtk::INVALID_LIST_POSITION);
                })
            };

            FileWatcher::new(
                &working_dir,
                dir_stores,
                Rc::clone(&current_file),
                on_file_changed,
                on_file_removed,
            )
        };

        // Re-highlight callback for theme changes
        let on_theme_rehighlight: Rc<dyn Fn(bool)> = {
            let text_view = text_view.clone();
            let gutter = gutter.clone();
            let path_label = path_label.clone();
            let current_file = Rc::clone(&current_file);
            Rc::new(move |dark: bool| {
                let file = current_file.borrow().clone();
                if !file.is_empty() {
                    let theme = if dark {
                        highlight::DARK_THEME
                    } else {
                        highlight::LIGHT_THEME
                    };
                    textview::load_file(&text_view, &gutter, &path_label, &file, theme);
                }
            })
        };

        Workspace { root, config, tab_spinner, chat_history, on_theme_rehighlight, _file_watcher: file_watcher }
    }
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
                break;
            }
        }
        if !expanded_any {
            break;
        }
    }
}
