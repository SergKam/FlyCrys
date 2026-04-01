use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk4 as gtk;
use notify::Watcher;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::config::constants::{AGENT_PANEL_MIN_WIDTH, TREE_MAX_EXPAND_PASSES};
use crate::config::types::{NotificationLevel, PanelMode, Theme};
use crate::file_entry::FileEntry;
use crate::run_panel::RunPanel;
use crate::services::platform;
use crate::session::{self, WorkspaceConfig};
use crate::ui::agent_panel;
use crate::ui::agent_panel::state::{BackgroundTaskResultCb, TaskCompletedCb};
use crate::watcher::FileWatcher;
use crate::{git_panel, highlight, textview, tree};

/// All the widgets for a single workspace tab
pub struct Workspace {
    pub root: gtk::Box,
    pub config: Rc<RefCell<WorkspaceConfig>>,
    pub tab_spinner: gtk::Spinner,
    pub chat_history: Rc<RefCell<Vec<session::ChatMessage>>>,
    pub run_panel: RunPanel,
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

        let current_file = Rc::new(RefCell::new(String::new()));

        // Left pane: tree panel (toolbar + search + file tree)
        let tp = tree::create_tree_panel(&working_dir);
        tree::wire_search(&tp, &working_dir);

        // Right pane top: text view with toolbar
        let tv = textview::create_text_view();

        // Late-binding slot for "Add Selected to Chat" — filled after agent panel creation
        let agent_input_slot: Rc<RefCell<Option<gtk::TextView>>> = Rc::new(RefCell::new(None));
        let on_add_to_chat: Rc<dyn Fn(String)> = {
            let slot = Rc::clone(&agent_input_slot);
            Rc::new(move |text: String| {
                if let Some(ref ai) = *slot.borrow() {
                    let buffer = ai.buffer();
                    let mut end = buffer.end_iter();
                    let current = buffer.text(&buffer.start_iter(), &end, false);
                    let prefix = if current.is_empty()
                        || current.ends_with(' ')
                        || current.ends_with('\n')
                    {
                        ""
                    } else {
                        " "
                    };
                    buffer.insert(&mut end, &format!("{prefix}{text}"));
                }
            })
        };

        // Right pane bottom: run panel (multi-terminal tabs)
        let run_panel = RunPanel::new(&config.borrow(), Rc::clone(&theme), on_add_to_chat);

        // Toggle handlers (view mode, diff mode)
        wire_panel_mode_handlers(&tv, &current_file, &theme, &config, &working_dir);

        // Toolbar button handlers (open, edit, browser, terminal, copy)
        wire_toolbar_buttons(&tv, &current_file, &run_panel, &config);

        // Right pane: editor + terminal split
        let right_paned = gtk::Paned::new(gtk::Orientation::Vertical);
        right_paned.set_start_child(Some(&tv.container));
        right_paned.set_end_child(Some(run_panel.container()));
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

        // Build on_open_file — single entry point for all file opens
        let on_open_file: Rc<dyn Fn(&str)> = build_on_open_file(
            &tv,
            &tp.selection,
            &current_file,
            &theme,
            &config,
            &working_dir,
        );

        // Git changes panel
        let git_panel_rc: Option<Rc<RefCell<git_panel::GitPanel>>> =
            git_panel::GitPanel::new(&working_dir, Rc::clone(&on_open_file)).map(|gp| {
                tp.container.append(&gp.container);
                let rc = Rc::new(RefCell::new(gp));
                git_panel::start_refresh_timer(&rc);
                rc
            });

        // Git status coloring in the file tree (runs initial + periodic refresh)
        {
            let lv = tp.list_view.clone();
            let gs = tp.git_status.clone();
            let wd = working_dir.clone();
            // Initial population
            tree::refresh_git_tree_status(&lv, &gs, &wd);
            // Periodic refresh on the same cadence as the git panel
            glib::timeout_add_local(
                std::time::Duration::from_secs(crate::config::constants::GIT_REFRESH_INTERVAL_SECS),
                move || {
                    tree::refresh_git_tree_status(&lv, &gs, &wd);
                    glib::ControlFlow::Continue
                },
            );
        }

        // Main horizontal split (tree+git | editor+terminal)
        let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
        paned.set_position(config.borrow().tree_pane_width);
        paned.set_start_child(Some(&tp.container));
        paned.set_end_child(Some(&right_paned));

        // Wire search result activation and tree click handlers
        tree::wire_search_activate(&tp, &on_open_file);
        wire_tree_click(&tp.list_view, &on_open_file);
        let (ctx_path, ctx_is_dir) = wire_tree_context_menu(&tp.list_view);

        // Agent setup
        let agent_configs = session::list_agent_configs();
        let chat_history = Rc::new(RefCell::new(session::load_chat_history(
            &config.borrow().id,
        )));

        let on_tool_result: Option<Rc<dyn Fn()>> = git_panel_rc.as_ref().map(|gp| {
            let gp = Rc::clone(gp);
            Rc::new(move || gp.borrow().refresh()) as Rc<dyn Fn()>
        });

        let (agent_name_label, token_label, cost_label) = create_status_labels();

        let (agent_panel_1, agent_input_1, agent_on_theme_change) = {
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
            // Background task callbacks → run panel
            let on_bg_task: Option<Rc<dyn Fn(String, String)>> = {
                let rp = run_panel.clone();
                Some(Rc::new(move |command: String, tool_id: String| {
                    rp.add_background_task_tab(&command, &tool_id);
                }))
            };
            let on_bg_result: BackgroundTaskResultCb = {
                let rp = run_panel.clone();
                Some(Rc::new(
                    move |tool_id: String, output: String, is_error: bool| {
                        rp.update_task_result(&tool_id, &output, is_error);
                    },
                ))
            };
            let on_task_done: TaskCompletedCb = {
                let rp = run_panel.clone();
                Some(Rc::new(
                    move |tool_id: String, status: String, output_file: Option<String>| {
                        rp.complete_task(&tool_id, &status, output_file.as_deref());
                    },
                ))
            };

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
                on_bg_task,
                on_bg_result,
                on_task_done,
                token_label.clone(),
                cost_label.clone(),
                agent_name_label.clone(),
            )
        };

        // Fill the late-binding slot so "Add Selected to Chat" can reach the agent input
        *agent_input_slot.borrow_mut() = Some(agent_input_1.clone());

        // Wire chat button
        {
            let cf = Rc::clone(&current_file);
            let ai = agent_input_1.clone();
            tv.chat_btn.connect_clicked(move |_| {
                let file = cf.borrow().clone();
                if !file.is_empty() {
                    append_path_to_input(&ai, &file);
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

        // Status bar
        let status_bar =
            create_status_bar(&working_dir, &agent_name_label, &token_label, &cost_label);

        // Root container
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.set_vexpand(true);
        root.set_hexpand(true);
        root.append(&outer_paned);
        root.append(&status_bar);

        // Actions (context menu + gutter)
        register_workspace_actions(
            &root,
            &ctx_path,
            &ctx_is_dir,
            &agent_input_1,
            &run_panel,
            &config,
            &current_file,
            &tv,
        );

        // Drag & drop
        wire_drag_drop(&tp.list_view, &agent_input_1, &agent_container);

        // Autosave pane positions
        wire_pane_position_tracking(&paned, &outer_paned, &right_paned, &config);

        // Load initial file if restored from session
        let initial_file = config.borrow().open_file.clone();
        if let Some(ref path) = initial_file {
            on_open_file(path);
        }

        // File-system watcher
        let file_watcher = setup_file_watcher(
            &working_dir,
            tp.dir_stores,
            &current_file,
            &on_open_file,
            &tv,
            &tp.selection,
            &config,
        );

        // Theme change callback
        let on_theme_rehighlight: Rc<dyn Fn(bool)> = {
            let oof = Rc::clone(&on_open_file);
            let cf = Rc::clone(&current_file);
            let rp = run_panel.clone();
            Rc::new(move |dark: bool| {
                let file = cf.borrow().clone();
                if !file.is_empty() {
                    oof(&file);
                }
                let t = if dark { Theme::Dark } else { Theme::Light };
                rp.apply_colors(t);
                agent_on_theme_change(dark);
            })
        };

        Workspace {
            root,
            config,
            tab_spinner,
            chat_history,
            run_panel,
            on_theme_rehighlight,
            _file_watcher: file_watcher,
        }
    }
}

// ── Panel mode handlers (Source / Preview / Diff) ─────────────────────────────

fn wire_panel_mode_handlers(
    tv: &textview::TextViewPanel,
    current_file: &Rc<RefCell<String>>,
    theme: &Rc<Cell<Theme>>,
    config: &Rc<RefCell<WorkspaceConfig>>,
    working_dir: &Path,
) {
    // Source button
    {
        let text_view = tv.text_view.clone();
        let gutter = tv.gutter.clone();
        let path_label = tv.path_label.clone();
        let source_hbox = tv.source_hbox.clone();
        let preview_scroll = tv.preview_scroll.clone();
        let source_btn = tv.source_btn.clone();
        let preview_btn = tv.preview_btn.clone();
        let open_btn = tv.open_btn.clone();
        let edit_btn = tv.edit_btn.clone();
        let browser_btn = tv.browser_btn.clone();
        let terminal_btn = tv.terminal_btn.clone();
        let copy_btn = tv.copy_btn.clone();
        let chat_btn = tv.chat_btn.clone();
        let cf = Rc::clone(current_file);
        let th = Rc::clone(theme);
        let cfg = Rc::clone(config);
        tv.source_btn.connect_toggled(move |btn| {
            if !btn.is_active() {
                return;
            }
            cfg.borrow_mut().panel_mode = PanelMode::Source;
            source_hbox.set_visible(true);
            preview_scroll.set_visible(false);
            let file = cf.borrow().clone();
            if !file.is_empty() {
                let hl_theme = if th.get().is_dark() {
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
                    &source_btn,
                    &preview_btn,
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

    // Preview button
    {
        let source_hbox = tv.source_hbox.clone();
        let preview_scroll = tv.preview_scroll.clone();
        let cf = Rc::clone(current_file);
        let th = Rc::clone(theme);
        let cfg = Rc::clone(config);
        tv.preview_btn.connect_toggled(move |btn| {
            if !btn.is_active() {
                return;
            }
            cfg.borrow_mut().panel_mode = PanelMode::Preview;
            source_hbox.set_visible(false);
            preview_scroll.set_visible(true);
            let file = cf.borrow().clone();
            if !file.is_empty() {
                textview::load_preview(&preview_scroll, &file, th.get().is_dark());
            }
        });
    }

    // Diff button
    {
        let text_view = tv.text_view.clone();
        let gutter = tv.gutter.clone();
        let source_hbox = tv.source_hbox.clone();
        let preview_scroll = tv.preview_scroll.clone();
        let source_btn = tv.source_btn.clone();
        let cf = Rc::clone(current_file);
        let th = Rc::clone(theme);
        let cfg = Rc::clone(config);
        let wd = working_dir.to_path_buf();
        tv.diff_btn.connect_toggled(move |btn| {
            if !btn.is_active() {
                return;
            }
            cfg.borrow_mut().panel_mode = PanelMode::Diff;
            source_hbox.set_visible(true);
            preview_scroll.set_visible(false);
            let file = cf.borrow().clone();
            if !file.is_empty() {
                if let Some(diff) = git_panel::get_file_diff(&file, &wd) {
                    let line_count = diff.lines().count().max(1);
                    git_panel::load_diff_into_buffer(
                        &text_view.buffer(),
                        &diff,
                        th.get().is_dark(),
                    );
                    textview::update_gutter(&gutter, line_count);
                } else {
                    // No diff available — fall back to source
                    source_btn.set_active(true);
                }
            }
        });
    }

    // Set initial state from config (read then drop borrow before set_active,
    // because set_active fires the handler synchronously which borrows config).
    let initial_mode = config.borrow().panel_mode;
    match initial_mode {
        PanelMode::Source => {} // already active by default
        PanelMode::Preview => tv.preview_btn.set_active(true),
        PanelMode::Diff => tv.diff_btn.set_active(true),
    }
}

// ── Toolbar button wiring ────────────────────────────────────────────────────

fn wire_toolbar_buttons(
    tv: &textview::TextViewPanel,
    current_file: &Rc<RefCell<String>>,
    run_panel: &RunPanel,
    config: &Rc<RefCell<WorkspaceConfig>>,
) {
    // Open in default app
    {
        let cf = Rc::clone(current_file);
        tv.open_btn.connect_clicked(move |_| {
            let file = cf.borrow().clone();
            if !file.is_empty() {
                let _ = platform::open_with_default(&file);
            }
        });
    }
    // Edit in text editor
    {
        let cf = Rc::clone(current_file);
        tv.edit_btn.connect_clicked(move |_| {
            let file = cf.borrow().clone();
            if !file.is_empty() {
                let _ = platform::open_in_editor(&file);
            }
        });
    }
    // Open in browser
    {
        let cf = Rc::clone(current_file);
        tv.browser_btn.connect_clicked(move |_| {
            let file = cf.borrow().clone();
            if !file.is_empty() {
                let _ = platform::open_file_in_browser(&file);
            }
        });
    }
    // Open terminal in file's parent dir
    {
        let cf = Rc::clone(current_file);
        let rp = run_panel.clone();
        let cfg = Rc::clone(config);
        tv.terminal_btn.connect_clicked(move |_| {
            let file = cf.borrow().clone();
            if !file.is_empty() {
                let parent_dir = Path::new(&file)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| "/".to_string());
                rp.show_and_cd(&parent_dir);
                cfg.borrow_mut().terminal_visible = true;
            }
        });
    }
    // Copy file path to clipboard
    {
        let cf = Rc::clone(current_file);
        let btn = tv.copy_btn.clone();
        btn.connect_clicked(move |b: &gtk::Button| {
            let file = cf.borrow().clone();
            if !file.is_empty() {
                b.clipboard().set_text(&file);
            }
        });
    }
}

// ── on_open_file builder ─────────────────────────────────────────────────────

fn build_on_open_file(
    tv: &textview::TextViewPanel,
    selection: &gtk::SingleSelection,
    current_file: &Rc<RefCell<String>>,
    theme: &Rc<Cell<Theme>>,
    config: &Rc<RefCell<WorkspaceConfig>>,
    working_dir: &Path,
) -> Rc<dyn Fn(&str)> {
    let text_view = tv.text_view.clone();
    let gutter = tv.gutter.clone();
    let path_label = tv.path_label.clone();
    let source_btn = tv.source_btn.clone();
    let preview_btn = tv.preview_btn.clone();
    let diff_btn = tv.diff_btn.clone();
    let preview_scroll = tv.preview_scroll.clone();
    let open_btn = tv.open_btn.clone();
    let edit_btn = tv.edit_btn.clone();
    let browser_btn = tv.browser_btn.clone();
    let terminal_btn = tv.terminal_btn.clone();
    let copy_btn = tv.copy_btn.clone();
    let chat_btn = tv.chat_btn.clone();
    let sel = selection.clone();
    let cf = Rc::clone(current_file);
    let th = Rc::clone(theme);
    let cfg = Rc::clone(config);
    let wd = working_dir.to_path_buf();

    Rc::new(move |file_path: &str| {
        let hl_theme = if th.get().is_dark() {
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
            &source_btn,
            &preview_btn,
            &[
                &open_btn,
                &edit_btn,
                &browser_btn,
                &terminal_btn,
                &copy_btn,
                &chat_btn,
            ],
        );
        *cf.borrow_mut() = file_path.to_string();
        cfg.borrow_mut().open_file = Some(file_path.to_string());
        select_file_in_tree(&sel, file_path);

        // If preview is active, render preview
        if preview_btn.is_active() {
            textview::load_preview(&preview_scroll, file_path, th.get().is_dark());
        }

        // Update diff button sensitivity; force source if diff is active but no changes
        let is_modified = git_panel::is_file_modified(file_path, &wd);
        diff_btn.set_sensitive(is_modified);
        if diff_btn.is_active() {
            if is_modified {
                if let Some(diff) = git_panel::get_file_diff(file_path, &wd) {
                    let line_count = diff.lines().count().max(1);
                    git_panel::load_diff_into_buffer(
                        &text_view.buffer(),
                        &diff,
                        th.get().is_dark(),
                    );
                    textview::update_gutter(&gutter, line_count);
                }
            } else {
                source_btn.set_active(true);
            }
        }
    })
}

// ── Tree click handling ──────────────────────────────────────────────────────

fn wire_tree_click(list_view: &gtk::ListView, on_open_file: &Rc<dyn Fn(&str)>) {
    let left_click = gtk::GestureClick::new();
    left_click.set_button(1);
    let oof = Rc::clone(on_open_file);
    left_click.connect_pressed(glib::clone!(
        #[weak]
        list_view,
        #[strong]
        oof,
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
                            oof(&entry.path());
                        }
                    }
                    return;
                }
                current = w.parent();
            }
        }
    ));
    list_view.add_controller(left_click);
}

fn wire_tree_context_menu(list_view: &gtk::ListView) -> (Rc<RefCell<String>>, Rc<RefCell<bool>>) {
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
    popover.set_parent(list_view);
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

    (ctx_path, ctx_is_dir)
}

// ── Status bar ───────────────────────────────────────────────────────────────

fn create_status_labels() -> (gtk::Label, gtk::Label, gtk::Label) {
    let agent_name = gtk::Label::new(Some("Agent"));
    agent_name.add_css_class("statusbar-item");

    let tokens = gtk::Label::new(Some("\u{2013}"));
    tokens.set_tooltip_text(Some("Context window usage"));
    tokens.add_css_class("statusbar-item");

    let cost = gtk::Label::new(Some("$0.00"));
    cost.set_tooltip_text(Some("Session cost"));
    cost.add_css_class("statusbar-item");

    (agent_name, tokens, cost)
}

fn create_status_bar(
    working_dir: &Path,
    agent_name_label: &gtk::Label,
    token_label: &gtk::Label,
    cost_label: &gtk::Label,
) -> gtk::Box {
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    bar.add_css_class("statusbar");

    bar.append(agent_name_label);
    bar.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    bar.append(token_label);
    bar.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    bar.append(cost_label);

    bar.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    let branch_label = gtk::Label::new(None);
    branch_label.add_css_class("statusbar-item");
    if let Some(branch) = crate::services::git::current_branch(working_dir) {
        branch_label.set_text(&format!("git: {branch}"));
    }
    bar.append(&branch_label);

    // Watch .git/ directory for branch changes (instant update via inotify).
    // We watch the directory, not .git/HEAD directly, because git replaces HEAD
    // atomically (write HEAD.lock, rename → HEAD) which invalidates a file-level
    // inotify watch after the first event.
    {
        let git_dir = working_dir.join(".git");
        if git_dir.is_dir() {
            let bl = branch_label.clone();
            let wd = working_dir.to_path_buf();
            let (tx, rx) = std::sync::mpsc::channel();
            let watcher = notify::recommended_watcher(move |res: Result<notify::Event, _>| {
                if let Ok(ev) = res {
                    // Filter: only care about HEAD (or HEAD.lock rename → HEAD)
                    let dominated_head = ev.paths.iter().any(|p| {
                        p.file_name()
                            .map(|n| n == "HEAD" || n == "HEAD.lock")
                            .unwrap_or(false)
                    });
                    if dominated_head {
                        let _ = tx.send(());
                    }
                }
            });
            if let Ok(mut w) = watcher {
                let _ = w.watch(&git_dir, notify::RecursiveMode::NonRecursive);
                // Keep watcher alive by moving into the polling closure.
                glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
                    let _keep = &w; // prevent drop
                    if rx.try_recv().is_ok() {
                        // Drain any extra events
                        while rx.try_recv().is_ok() {}
                        if let Some(branch) = crate::services::git::current_branch(&wd) {
                            bl.set_text(&format!("git: {branch}"));
                        } else {
                            bl.set_text("");
                        }
                    }
                    glib::ControlFlow::Continue
                });
            }
        }
    }

    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    bar.append(&spacer);

    let path_label = gtk::Label::new(Some(&working_dir.to_string_lossy()));
    path_label.add_css_class("statusbar-item");
    path_label.set_ellipsize(gtk::pango::EllipsizeMode::Start);
    bar.append(&path_label);

    bar
}

// ── Action registration ──────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn register_workspace_actions(
    root: &gtk::Box,
    ctx_path: &Rc<RefCell<String>>,
    ctx_is_dir: &Rc<RefCell<bool>>,
    agent_input: &gtk::TextView,
    run_panel: &RunPanel,
    config: &Rc<RefCell<WorkspaceConfig>>,
    current_file: &Rc<RefCell<String>>,
    tv: &textview::TextViewPanel,
) {
    let action_group = gio::SimpleActionGroup::new();

    // Tree context menu actions
    register_tree_actions(
        &action_group,
        root,
        ctx_path,
        ctx_is_dir,
        agent_input,
        run_panel,
        config,
    );

    // Gutter context menu actions
    let gutter_ctx_line = wire_gutter_context_menu(&tv.gutter);
    register_gutter_actions(
        &action_group,
        root,
        current_file,
        &gutter_ctx_line,
        agent_input,
    );

    root.insert_action_group("ws", Some(&action_group));
}

fn register_tree_actions(
    group: &gio::SimpleActionGroup,
    root: &gtk::Box,
    ctx_path: &Rc<RefCell<String>>,
    ctx_is_dir: &Rc<RefCell<bool>>,
    agent_input: &gtk::TextView,
    run_panel: &RunPanel,
    config: &Rc<RefCell<WorkspaceConfig>>,
) {
    // Copy Path
    let action = gio::SimpleAction::new("copy-path", None);
    {
        let cp = Rc::clone(ctx_path);
        let r = root.clone();
        action.connect_activate(move |_, _| {
            let p = cp.borrow().clone();
            if !p.is_empty() {
                r.clipboard().set_text(&p);
            }
        });
    }
    group.add_action(&action);

    // Add to Chat
    let action = gio::SimpleAction::new("add-to-chat", None);
    {
        let cp = Rc::clone(ctx_path);
        let ai = agent_input.clone();
        action.connect_activate(move |_, _| {
            let p = cp.borrow().clone();
            if !p.is_empty() {
                append_path_to_input(&ai, &p);
            }
        });
    }
    group.add_action(&action);

    // Open Terminal Here
    let action = gio::SimpleAction::new("open-terminal-here", None);
    {
        let cp = Rc::clone(ctx_path);
        let cd = Rc::clone(ctx_is_dir);
        let rp = run_panel.clone();
        let cfg = Rc::clone(config);
        action.connect_activate(move |_, _| {
            let path = cp.borrow().clone();
            let is_dir = *cd.borrow();
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
            rp.show_and_cd(&dir);
            cfg.borrow_mut().terminal_visible = true;
        });
    }
    group.add_action(&action);

    // Open in Default App
    let action = gio::SimpleAction::new("open-default", None);
    {
        let cp = Rc::clone(ctx_path);
        action.connect_activate(move |_, _| {
            let p = cp.borrow().clone();
            if !p.is_empty() {
                let _ = platform::open_with_default(&p);
            }
        });
    }
    group.add_action(&action);

    // Edit in Text Editor
    let action = gio::SimpleAction::new("edit-in-editor", None);
    {
        let cp = Rc::clone(ctx_path);
        action.connect_activate(move |_, _| {
            let p = cp.borrow().clone();
            if !p.is_empty() {
                let _ = platform::open_in_editor(&p);
            }
        });
    }
    group.add_action(&action);

    // Open in Browser
    let action = gio::SimpleAction::new("open-in-browser", None);
    {
        let cp = Rc::clone(ctx_path);
        action.connect_activate(move |_, _| {
            let p = cp.borrow().clone();
            if !p.is_empty() {
                let _ = platform::open_file_in_browser(&p);
            }
        });
    }
    group.add_action(&action);
}

// ── Gutter context menu ──────────────────────────────────────────────────────

fn wire_gutter_context_menu(gutter: &gtk::TextView) -> Rc<Cell<u32>> {
    let ctx_line: Rc<Cell<u32>> = Rc::new(Cell::new(0));

    let menu = gio::Menu::new();
    menu.append(Some("Copy Line Link"), Some("ws.copy-line-link"));
    menu.append(
        Some("Add Line Link to Chat"),
        Some("ws.add-line-link-to-chat"),
    );

    let popover = gtk::PopoverMenu::from_model(Some(&menu));
    popover.set_parent(gutter);
    popover.set_has_arrow(false);

    let click = gtk::GestureClick::new();
    click.set_button(3);
    {
        let g = gutter.clone();
        let cl = Rc::clone(&ctx_line);
        click.connect_pressed(glib::clone!(
            #[weak]
            g,
            #[weak]
            popover,
            #[strong]
            cl,
            move |gesture, _n_press, x, y| {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                let (bx, by) =
                    g.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
                if let Some(iter) = g.iter_at_location(bx, by) {
                    cl.set(iter.line() as u32 + 1);
                    popover
                        .set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
                    popover.popup();
                }
            }
        ));
    }
    gutter.add_controller(click);

    ctx_line
}

fn register_gutter_actions(
    group: &gio::SimpleActionGroup,
    root: &gtk::Box,
    current_file: &Rc<RefCell<String>>,
    gutter_ctx_line: &Rc<Cell<u32>>,
    agent_input: &gtk::TextView,
) {
    // Copy Line Link
    let action = gio::SimpleAction::new("copy-line-link", None);
    {
        let cf = Rc::clone(current_file);
        let cl = Rc::clone(gutter_ctx_line);
        let r = root.clone();
        action.connect_activate(move |_, _| {
            let file = cf.borrow().clone();
            let line = cl.get();
            if !file.is_empty() && line > 0 {
                r.clipboard().set_text(&format!("{file}:{line}"));
            }
        });
    }
    group.add_action(&action);

    // Add Line Link to Chat
    let action = gio::SimpleAction::new("add-line-link-to-chat", None);
    {
        let cf = Rc::clone(current_file);
        let cl = Rc::clone(gutter_ctx_line);
        let ai = agent_input.clone();
        action.connect_activate(move |_, _| {
            let file = cf.borrow().clone();
            let line = cl.get();
            if !file.is_empty() && line > 0 {
                append_path_to_input(&ai, &format!("{file}:{line}"));
            }
        });
    }
    group.add_action(&action);
}

// ── Drag & drop ──────────────────────────────────────────────────────────────

fn wire_drag_drop(
    list_view: &gtk::ListView,
    agent_input: &gtk::TextView,
    agent_container: &gtk::Box,
) {
    // Drag from tree
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

    // Drop on agent input
    let drop_target = gtk::DropTarget::new(glib::Type::STRING, gtk::gdk::DragAction::COPY);
    drop_target.connect_drop(glib::clone!(
        #[weak]
        agent_input,
        #[upgrade_or]
        false,
        move |_target, value, _x, _y| {
            if let Ok(path) = value.get::<String>() {
                append_path_to_input(&agent_input, &path);
                return true;
            }
            false
        }
    ));
    agent_input.add_controller(drop_target);

    // Drop on agent panel area
    let panel_drop = gtk::DropTarget::new(glib::Type::STRING, gtk::gdk::DragAction::COPY);
    panel_drop.connect_drop(glib::clone!(
        #[weak]
        agent_input,
        #[upgrade_or]
        false,
        move |_target, value, _x, _y| {
            if let Ok(path) = value.get::<String>() {
                append_path_to_input(&agent_input, &path);
                return true;
            }
            false
        }
    ));
    agent_container.add_controller(panel_drop);
}

// ── Pane position persistence ────────────────────────────────────────────────

fn wire_pane_position_tracking(
    tree_paned: &gtk::Paned,
    agent_paned: &gtk::Paned,
    editor_paned: &gtk::Paned,
    config: &Rc<RefCell<WorkspaceConfig>>,
) {
    {
        let cfg = Rc::clone(config);
        tree_paned.connect_position_notify(move |p| {
            cfg.borrow_mut().tree_pane_width = p.position();
        });
    }
    {
        let cfg = Rc::clone(config);
        agent_paned.connect_position_notify(move |p| {
            cfg.borrow_mut().agent_pane_width = p.position();
        });
    }
    {
        let cfg = Rc::clone(config);
        editor_paned.connect_position_notify(move |p| {
            cfg.borrow_mut().editor_terminal_split = p.position();
        });
    }
}

// ── File watcher setup ───────────────────────────────────────────────────────

fn setup_file_watcher(
    working_dir: &Path,
    dir_stores: tree::DirStoreMap,
    current_file: &Rc<RefCell<String>>,
    on_open_file: &Rc<dyn Fn(&str)>,
    tv: &textview::TextViewPanel,
    selection: &gtk::SingleSelection,
    config: &Rc<RefCell<WorkspaceConfig>>,
) -> Option<FileWatcher> {
    let on_file_changed: Rc<dyn Fn()> = {
        let oof = Rc::clone(on_open_file);
        let cf = Rc::clone(current_file);
        Rc::new(move || {
            let file = cf.borrow().clone();
            if !file.is_empty() {
                oof(&file);
            }
        })
    };

    let on_file_removed: Rc<dyn Fn()> = {
        let text_view = tv.text_view.clone();
        let gutter = tv.gutter.clone();
        let path_label = tv.path_label.clone();
        let cf = Rc::clone(current_file);
        let sel = selection.clone();
        let cfg = Rc::clone(config);
        Rc::new(move || {
            textview::clear_view(&text_view, &gutter, &path_label);
            *cf.borrow_mut() = String::new();
            cfg.borrow_mut().open_file = None;
            sel.set_selected(gtk::INVALID_LIST_POSITION);
        })
    };

    FileWatcher::new(
        working_dir,
        dir_stores,
        Rc::clone(current_file),
        on_file_changed,
        on_file_removed,
    )
}

// ── Utilities ────────────────────────────────────────────────────────────────

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
