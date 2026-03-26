use gtk::gio;
use gtk::glib;
use gtk4 as gtk;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use vte4::prelude::*;

use crate::config::constants::{AGENT_PANEL_MIN_WIDTH, TREE_MAX_EXPAND_PASSES};
use crate::config::types::{NotificationLevel, Theme};
use crate::file_entry::FileEntry;
use crate::services::platform;
use crate::session::{self, WorkspaceConfig};
use crate::ui::agent_panel;
use crate::watcher::FileWatcher;
use crate::{git_panel, highlight, terminal, textview, tree};

/// All the widgets for a single workspace tab
pub struct Workspace {
    pub root: gtk::Box,
    pub config: Rc<RefCell<WorkspaceConfig>>,
    pub tab_spinner: gtk::Spinner,
    pub chat_history: Rc<RefCell<Vec<session::ChatMessage>>>,
    pub vte_terminal: vte4::Terminal,
    /// True while a shell child process is running inside the terminal.
    pub terminal_has_child: Rc<Cell<bool>>,
    /// Set by `contents-changed` signal, cleared after save.
    pub terminal_dirty: Rc<Cell<bool>>,
    /// Called on theme change to re-highlight the current file.
    pub on_theme_rehighlight: Rc<dyn Fn(bool)>,
    // Prevent drop — stopping the watcher closes the channel and the GTK timer exits.
    _file_watcher: Option<FileWatcher>,
}

impl Workspace {
    pub fn new(
        config: WorkspaceConfig,
        theme: Rc<Cell<Theme>>,
        notification_level: Rc<Cell<NotificationLevel>>,
        tab_spinner: gtk::Spinner,
    ) -> Self {
        let working_dir = PathBuf::from(&config.working_directory);
        let config = Rc::new(RefCell::new(config));

        // Current file in text viewer
        let current_file = Rc::new(RefCell::new(String::new()));

        // Left pane: file tree
        let (tree_scroll, list_view, selection, dir_stores) = tree::create_file_tree(&working_dir);

        // Right pane top: text view with toolbar
        let tv = textview::create_text_view();

        // Right pane bottom: terminal (initially hidden, deferred spawn if was visible)
        let (terminal_container, vte_terminal) = terminal::create_terminal_panel();
        terminal::apply_colors(&vte_terminal, theme.get());
        let terminal_dirty = Rc::new(Cell::new(false));
        let terminal_has_child = Rc::new(Cell::new(false));
        let terminal_was_visible = config.borrow().terminal_visible;
        terminal_container.set_visible(terminal_was_visible);
        if terminal_was_visible {
            // Defer restore+spawn to after the UI is painted — don't block startup
            let vte = vte_terminal.clone();
            let ws_id = config.borrow().id.clone();
            let wd = working_dir.to_string_lossy().to_string();
            let has_child = Rc::clone(&terminal_has_child);
            glib::idle_add_local_once(move || {
                let term_path = session::terminal_content_path(&ws_id);
                terminal::restore_scrollback(&vte, &term_path);
                terminal::spawn_shell(&vte, &wd);
                has_child.set(true);
            });
        }
        // Clear the flag when the shell process exits
        {
            let has_child = Rc::clone(&terminal_has_child);
            vte_terminal.connect_child_exited(move |_, _status| {
                has_child.set(false);
            });
        }
        // Track terminal content changes via dirty flag
        {
            let dirty = Rc::clone(&terminal_dirty);
            vte_terminal.connect_contents_changed(move |_| {
                dirty.set(true);
            });
        }

        // --- Connect toggle handlers FIRST (before setting initial state) ---

        // View toggle: switch between source and preview
        {
            let source_hbox = tv.source_hbox.clone();
            let preview_scroll = tv.preview_scroll.clone();
            let current_file = Rc::clone(&current_file);
            let theme = Rc::clone(&theme);
            let config = Rc::clone(&config);
            tv.view_toggle.connect_toggled(move |toggle| {
                let is_preview = toggle.is_active();
                config.borrow_mut().view_mode = if is_preview {
                    crate::config::types::ViewMode::Preview
                } else {
                    crate::config::types::ViewMode::Source
                };
                if is_preview {
                    source_hbox.set_visible(false);
                    preview_scroll.set_visible(true);
                    let file = current_file.borrow().clone();
                    if !file.is_empty() {
                        textview::load_preview(&preview_scroll, &file, theme.get().is_dark());
                    }
                } else {
                    source_hbox.set_visible(true);
                    preview_scroll.set_visible(false);
                }
            });
        }

        // Diff toggle: switch between normal source and git diff
        {
            let text_view = tv.text_view.clone();
            let gutter = tv.gutter.clone();
            let path_label = tv.path_label.clone();
            let view_toggle = tv.view_toggle.clone();
            let open_btn = tv.open_btn.clone();
            let edit_btn = tv.edit_btn.clone();
            let browser_btn = tv.browser_btn.clone();
            let terminal_btn = tv.terminal_btn.clone();
            let copy_btn = tv.copy_btn.clone();
            let chat_btn = tv.chat_btn.clone();
            let current_file = Rc::clone(&current_file);
            let theme = Rc::clone(&theme);
            let config = Rc::clone(&config);
            let working_dir = working_dir.clone();
            tv.diff_toggle.connect_toggled(move |toggle| {
                let is_visible = toggle.is_active();
                config.borrow_mut().diff_mode = if is_visible {
                    crate::config::types::DiffMode::Visible
                } else {
                    crate::config::types::DiffMode::Hidden
                };
                let file = current_file.borrow().clone();
                if file.is_empty() {
                    return;
                }
                if is_visible {
                    if let Some(diff) = git_panel::get_file_diff(&file, &working_dir) {
                        let line_count = diff.lines().count().max(1);
                        git_panel::load_diff_into_buffer(
                            &text_view.buffer(),
                            &diff,
                            theme.get().is_dark(),
                        );
                        textview::update_gutter(&gutter, line_count);
                    }
                } else {
                    // Reload normal file
                    let hl_theme = if theme.get().is_dark() {
                        highlight::DARK_THEME
                    } else {
                        highlight::LIGHT_THEME
                    };
                    textview::load_file(
                        &text_view,
                        &gutter,
                        &path_label,
                        &file,
                        hl_theme,
                        &view_toggle,
                        &[
                            &open_btn,
                            &edit_btn,
                            &browser_btn,
                            &terminal_btn,
                            &copy_btn,
                            &chat_btn,
                        ],
                    );
                }
            });
        }

        // Set initial toggle states from config (triggers handlers, but current_file is empty)
        {
            let diff_mode = config.borrow().diff_mode;
            tv.diff_toggle.set_active(diff_mode.is_visible());
            let view_mode = config.borrow().view_mode;
            if view_mode.is_preview() {
                tv.view_toggle.set_active(true);
            }
        }

        // --- Toolbar button wiring ---

        // Open button: xdg-open the file
        {
            let current_file = Rc::clone(&current_file);
            tv.open_btn.connect_clicked(move |_| {
                let file = current_file.borrow().clone();
                if !file.is_empty() {
                    let _ = platform::open_with_default(&file);
                }
            });
        }

        // Edit button: open in text editor
        {
            let current_file = Rc::clone(&current_file);
            tv.edit_btn.connect_clicked(move |_| {
                let file = current_file.borrow().clone();
                if !file.is_empty() {
                    let _ = platform::open_in_editor(&file);
                }
            });
        }

        // Browser button: open in web browser
        {
            let current_file = Rc::clone(&current_file);
            tv.browser_btn.connect_clicked(move |_| {
                let file = current_file.borrow().clone();
                if !file.is_empty() {
                    let _ = platform::open_file_in_browser(&file);
                }
            });
        }

        // Terminal button: open terminal in file's parent dir
        {
            let current_file = Rc::clone(&current_file);
            let terminal_container = terminal_container.clone();
            let vte_terminal = vte_terminal.clone();
            let config = Rc::clone(&config);
            let has_child = Rc::clone(&terminal_has_child);
            tv.terminal_btn.connect_clicked(move |_| {
                let file = current_file.borrow().clone();
                if !file.is_empty() {
                    let parent_dir = Path::new(&file)
                        .parent()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|| "/".to_string());
                    terminal_container.set_visible(true);
                    config.borrow_mut().terminal_visible = true;
                    if has_child.get() {
                        terminal::send_cd(&vte_terminal, &parent_dir);
                    } else {
                        terminal::spawn_shell(&vte_terminal, &parent_dir);
                        has_child.set(true);
                    }
                }
            });
        }

        // Copy button: copy file path to clipboard
        {
            let current_file = Rc::clone(&current_file);
            let copy_btn = tv.copy_btn.clone();
            copy_btn.connect_clicked(move |btn| {
                let file = current_file.borrow().clone();
                if !file.is_empty() {
                    btn.clipboard().set_text(&file);
                }
            });
        }

        let right_paned = gtk::Paned::new(gtk::Orientation::Vertical);
        right_paned.set_start_child(Some(&tv.container));
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

        // Left pane: tree + git panel
        let left_vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        tree_scroll.set_vexpand(true);
        left_vbox.append(&tree_scroll);

        // Build on_open_file — single entry point for all file opens
        let on_open_file: Rc<dyn Fn(&str)> = {
            let text_view = tv.text_view.clone();
            let gutter = tv.gutter.clone();
            let path_label = tv.path_label.clone();
            let view_toggle = tv.view_toggle.clone();
            let diff_toggle = tv.diff_toggle.clone();
            let preview_scroll = tv.preview_scroll.clone();
            let open_btn = tv.open_btn.clone();
            let edit_btn = tv.edit_btn.clone();
            let browser_btn = tv.browser_btn.clone();
            let terminal_btn = tv.terminal_btn.clone();
            let copy_btn = tv.copy_btn.clone();
            let chat_btn = tv.chat_btn.clone();
            let selection = selection.clone();
            let current_file = Rc::clone(&current_file);
            let theme = Rc::clone(&theme);
            let config = Rc::clone(&config);
            let working_dir = working_dir.clone();
            Rc::new(move |file_path: &str| {
                let hl_theme = if theme.get().is_dark() {
                    highlight::DARK_THEME
                } else {
                    highlight::LIGHT_THEME
                };
                textview::load_file(
                    &text_view,
                    &gutter,
                    &path_label,
                    file_path,
                    hl_theme,
                    &view_toggle,
                    &[
                        &open_btn,
                        &edit_btn,
                        &browser_btn,
                        &terminal_btn,
                        &copy_btn,
                        &chat_btn,
                    ],
                );
                *current_file.borrow_mut() = file_path.to_string();
                config.borrow_mut().open_file = Some(file_path.to_string());
                select_file_in_tree(&selection, file_path);

                // If preview mode is active, refresh preview
                if view_toggle.is_active() {
                    textview::load_preview(&preview_scroll, file_path, theme.get().is_dark());
                }

                // Diff: check if file has git changes
                let is_modified = git_panel::is_file_modified(file_path, &working_dir);
                diff_toggle.set_sensitive(is_modified);
                if diff_toggle.is_active()
                    && is_modified
                    && let Some(diff) = git_panel::get_file_diff(file_path, &working_dir)
                {
                    let line_count = diff.lines().count().max(1);
                    git_panel::load_diff_into_buffer(
                        &text_view.buffer(),
                        &diff,
                        theme.get().is_dark(),
                    );
                    textview::update_gutter(&gutter, line_count);
                }
            })
        };

        // Git changes panel
        let git_panel_rc: Option<Rc<RefCell<git_panel::GitPanel>>> =
            git_panel::GitPanel::new(&working_dir, Rc::clone(&on_open_file)).map(|gp| {
                left_vbox.append(&gp.container);
                let rc = Rc::new(RefCell::new(gp));
                git_panel::start_refresh_timer(&rc);
                rc
            });

        // Main horizontal split (tree+git | editor+terminal)
        let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
        paned.set_position(config.borrow().tree_pane_width);
        paned.set_start_child(Some(&left_vbox));
        paned.set_end_child(Some(&right_paned));

        // Single-click: open files / toggle directories
        let left_click = gtk::GestureClick::new();
        left_click.set_button(1);
        {
            let on_open_file = Rc::clone(&on_open_file);
            left_click.connect_pressed(glib::clone!(
                #[weak]
                list_view,
                #[strong]
                on_open_file,
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
                            if let Some(row) = expander.list_row()
                                && let Some(entry) = row.item().and_downcast::<FileEntry>()
                            {
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
                                    on_open_file(&entry.path());
                                }
                            }
                            return;
                        }
                        current = w.parent();
                    }
                }
            ));
        }
        list_view.add_controller(left_click);

        // --- Right-click context menu ---
        let ctx_path: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
        let ctx_is_dir: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));

        let menu = gio::Menu::new();
        menu.append(Some("Copy Path"), Some("ws.copy-path"));
        menu.append(Some("Add to Chat"), Some("ws.add-to-chat"));
        menu.append(Some("Open Terminal Here"), Some("ws.open-terminal-here"));
        menu.append(Some("Open in Default App"), Some("ws.open-default"));
        menu.append(Some("Edit in Text Editor"), Some("ws.edit-in-editor"));
        menu.append(Some("Open in Browser"), Some("ws.open-in-browser"));

        let popover = gtk::PopoverMenu::from_model(Some(&menu));
        popover.set_parent(&list_view);
        popover.set_has_arrow(false);

        let right_click = gtk::GestureClick::new();
        right_click.set_button(3);
        right_click.connect_pressed(glib::clone!(
            #[weak]
            list_view,
            #[weak]
            popover,
            #[strong]
            ctx_path,
            #[strong]
            ctx_is_dir,
            move |_gesture, _n_press, x, y| {
                let Some(widget) = list_view.pick(x, y, gtk::PickFlags::DEFAULT) else {
                    return;
                };
                let mut current = Some(widget);
                while let Some(w) = current {
                    if let Some(expander) = w.downcast_ref::<gtk::TreeExpander>() {
                        if let Some(item) = expander.item()
                            && let Some(entry) = item.downcast_ref::<FileEntry>()
                        {
                            *ctx_path.borrow_mut() = entry.path();
                            *ctx_is_dir.borrow_mut() = entry.is_dir();

                            popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(
                                x as i32, y as i32, 1, 1,
                            )));
                            popover.popup();
                        }
                        return;
                    }
                    current = w.parent();
                }
            }
        ));
        list_view.add_controller(right_click);

        // Load agent profiles
        let agent_configs = session::list_agent_configs();

        // Chat history
        let chat_history = Rc::new(RefCell::new(session::load_chat_history(
            &config.borrow().id,
        )));

        // on_tool_result callback for git panel refresh
        let on_tool_result: Option<Rc<dyn Fn()>> = git_panel_rc.as_ref().map(|gp| {
            let gp = Rc::clone(gp);
            Rc::new(move || {
                gp.borrow().refresh();
            }) as Rc<dyn Fn()>
        });

        // Status bar labels — created here, passed into the agent panel which updates them.
        let token_label = gtk::Label::new(Some("\u{2013}"));
        token_label.set_tooltip_text(Some("Context window usage"));
        token_label.add_css_class("statusbar-item");

        let cost_label = gtk::Label::new(Some("$0.00"));
        cost_label.set_tooltip_text(Some("Session cost"));
        cost_label.add_css_class("statusbar-item");

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
                Rc::clone(&theme),
                Rc::clone(&notification_level),
                tab_spinner.clone(),
                &working_dir,
                "Agent",
                agent_configs,
                &profile,
                session_id,
                on_profile,
                on_session,
                Rc::clone(&chat_history),
                on_tool_result,
                token_label.clone(),
                cost_label.clone(),
            )
        };

        // Wire chat button (needs agent_input_1)
        {
            let current_file = Rc::clone(&current_file);
            let agent_input = agent_input_1.clone();
            tv.chat_btn.connect_clicked(move |_| {
                let file = current_file.borrow().clone();
                if !file.is_empty() {
                    append_path_to_input(&agent_input, &file);
                }
            });
        }

        let agent_container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        agent_container.set_width_request(AGENT_PANEL_MIN_WIDTH);
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

        // ── Status bar ──
        let status_bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        status_bar.add_css_class("statusbar");

        // 1) Claude stats (left)
        status_bar.append(&token_label);
        status_bar.append(&gtk::Separator::new(gtk::Orientation::Vertical));
        status_bar.append(&cost_label);

        // 2) Git branch
        if let Some(branch) = crate::services::git::current_branch(&working_dir) {
            status_bar.append(&gtk::Separator::new(gtk::Orientation::Vertical));
            let branch_label = gtk::Label::new(Some(&format!("git: {branch}")));
            branch_label.add_css_class("statusbar-item");
            status_bar.append(&branch_label);
        }

        // 3) Full path (right-aligned)
        let status_spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        status_spacer.set_hexpand(true);
        status_bar.append(&status_spacer);

        let path_label = gtk::Label::new(Some(&working_dir.to_string_lossy()));
        path_label.add_css_class("statusbar-item");
        path_label.set_ellipsize(gtk::pango::EllipsizeMode::Start);
        status_bar.append(&path_label);

        // Root container
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.set_vexpand(true);
        root.set_hexpand(true);
        root.append(&outer_paned);
        root.append(&status_bar);

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
            let has_child = Rc::clone(&terminal_has_child);
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
                if has_child.get() {
                    terminal::send_cd(&vte_terminal, &dir);
                } else {
                    terminal::spawn_shell(&vte_terminal, &dir);
                    has_child.set(true);
                }
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

        // Open in Default App (right-click)
        let action_open_default = gio::SimpleAction::new("open-default", None);
        {
            let ctx_path = Rc::clone(&ctx_path);
            action_open_default.connect_activate(move |_, _| {
                let path = ctx_path.borrow().clone();
                if !path.is_empty() {
                    let _ = platform::open_with_default(&path);
                }
            });
        }
        action_group.add_action(&action_open_default);

        // Edit in Text Editor (right-click)
        let action_edit = gio::SimpleAction::new("edit-in-editor", None);
        {
            let ctx_path = Rc::clone(&ctx_path);
            action_edit.connect_activate(move |_, _| {
                let path = ctx_path.borrow().clone();
                if !path.is_empty() {
                    let _ = platform::open_in_editor(&path);
                }
            });
        }
        action_group.add_action(&action_edit);

        // Open in Browser (right-click)
        let action_browser = gio::SimpleAction::new("open-in-browser", None);
        {
            let ctx_path = Rc::clone(&ctx_path);
            action_browser.connect_activate(move |_, _| {
                let path = ctx_path.borrow().clone();
                if !path.is_empty() {
                    let _ = platform::open_file_in_browser(&path);
                }
            });
        }
        action_group.add_action(&action_browser);

        // --- Gutter right-click: Copy Line Link / Add Line Link to Chat ---
        let gutter_ctx_line: Rc<Cell<u32>> = Rc::new(Cell::new(0));

        let gutter_menu = gio::Menu::new();
        gutter_menu.append(Some("Copy Line Link"), Some("ws.copy-line-link"));
        gutter_menu.append(
            Some("Add Line Link to Chat"),
            Some("ws.add-line-link-to-chat"),
        );

        let gutter_popover = gtk::PopoverMenu::from_model(Some(&gutter_menu));
        gutter_popover.set_parent(&tv.gutter);
        gutter_popover.set_has_arrow(false);

        let gutter_click = gtk::GestureClick::new();
        gutter_click.set_button(3);
        {
            let gutter = tv.gutter.clone();
            gutter_click.connect_pressed(glib::clone!(
                #[weak]
                gutter,
                #[weak]
                gutter_popover,
                #[strong]
                gutter_ctx_line,
                move |gesture, _n_press, x, y| {
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
        }
        tv.gutter.add_controller(gutter_click);

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
            #[weak]
            list_view,
            #[upgrade_or]
            None,
            move |_source, x, y| {
                let widget = list_view.pick(x, y, gtk::PickFlags::DEFAULT)?;
                let mut current = Some(widget);
                while let Some(w) = current {
                    if let Some(expander) = w.downcast_ref::<gtk::TreeExpander>() {
                        if let Some(item) = expander.item()
                            && let Some(entry) = item.downcast_ref::<FileEntry>()
                        {
                            let path = entry.path();
                            return Some(gtk::gdk::ContentProvider::for_value(&path.to_value()));
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
            #[weak]
            agent_input_1,
            #[upgrade_or]
            false,
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
            #[weak]
            agent_input_1,
            #[upgrade_or]
            false,
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
                #[strong]
                config,
                move |p| {
                    config.borrow_mut().tree_pane_width = p.position();
                }
            ));
        }
        {
            let config = Rc::clone(&config);
            outer_paned.connect_position_notify(glib::clone!(
                #[strong]
                config,
                move |p| {
                    config.borrow_mut().agent_pane_width = p.position();
                }
            ));
        }
        {
            let config = Rc::clone(&config);
            right_paned.connect_position_notify(glib::clone!(
                #[strong]
                config,
                move |p| {
                    config.borrow_mut().editor_terminal_split = p.position();
                }
            ));
        }
        // Load initial file if restored from session
        {
            let open_file = config.borrow().open_file.clone();
            if let Some(ref path) = open_file {
                on_open_file(path);
            }
        }
        // File-system watcher: auto-refresh tree and current file on changes
        let file_watcher = {
            let on_file_changed: Rc<dyn Fn()> = {
                let on_open_file = Rc::clone(&on_open_file);
                let current_file = Rc::clone(&current_file);
                Rc::new(move || {
                    let file = current_file.borrow().clone();
                    if !file.is_empty() {
                        on_open_file(&file);
                    }
                })
            };

            let on_file_removed: Rc<dyn Fn()> = {
                let text_view = tv.text_view.clone();
                let gutter = tv.gutter.clone();
                let path_label = tv.path_label.clone();
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

        // Re-highlight and re-color callback for theme changes
        let on_theme_rehighlight: Rc<dyn Fn(bool)> = {
            let on_open_file = Rc::clone(&on_open_file);
            let current_file = Rc::clone(&current_file);
            let vte = vte_terminal.clone();
            Rc::new(move |dark: bool| {
                let file = current_file.borrow().clone();
                if !file.is_empty() {
                    on_open_file(&file);
                }
                let new_theme = if dark { Theme::Dark } else { Theme::Light };
                terminal::apply_colors(&vte, new_theme);
            })
        };

        Workspace {
            root,
            config,
            tab_spinner,
            chat_history,
            vte_terminal,
            terminal_has_child,
            terminal_dirty,
            on_theme_rehighlight,
            _file_watcher: file_watcher,
        }
    }
}

// open_in_text_editor and open_in_browser have moved to services::platform

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
    for _pass in 0..TREE_MAX_EXPAND_PASSES {
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
