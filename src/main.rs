use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk4 as gtk;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use flycrys::config::constants::AUTOSAVE_INTERVAL_SECS;
use flycrys::config::theme::css_for_theme;
use flycrys::config::types::{NotificationLevel, Theme};
use flycrys::session::{self, AppConfig, WorkspaceConfig};
use flycrys::workspace::Workspace;

const APP_ID: &str = "com.flycrys.app";

fn main() -> glib::ExitCode {
    // No application ID → each process is independent (no D-Bus single-instance).
    // This lets multiple windows coexist and `cargo run` always starts the new build.
    let app = gtk::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();

    app.connect_startup(|_app| {
        let icon_theme = gtk::IconTheme::for_display(&gtk::gdk::Display::default().unwrap());
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()));
        let candidates = [
            exe_dir.as_ref().map(|d| d.join("icons")),
            exe_dir.as_ref().map(|d| d.join("../icons")),
            Some(std::path::PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/icons"
            ))),
        ];
        for candidate in candidates.into_iter().flatten() {
            if candidate.is_dir() {
                icon_theme.add_search_path(&candidate);
                break;
            }
        }
        // Also add the data/icons/hicolor path so the system icon theme
        // finds "flycrys" for the taskbar/window icon during development.
        let hicolor_candidates = [
            exe_dir.as_ref().map(|d| d.join("../data/icons")),
            Some(std::path::PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/data/icons"
            ))),
        ];
        for candidate in hicolor_candidates.into_iter().flatten() {
            if candidate.is_dir() {
                icon_theme.add_search_path(&candidate);
                break;
            }
        }
        // Set the default window icon so taskbar/dock shows it
        gtk::Window::set_default_icon_name("flycrys");
    });

    app.connect_activate(build_ui);
    app.run()
}

// ── Lazy tab slot ───────────────────────────────────────────────────────

/// A notebook tab that may or may not have its workspace materialised yet.
/// Only the active tab is built at startup; others are built on first switch.
struct TabSlot {
    /// The gtk::Box used as the notebook page widget (always present).
    wrapper: gtk::Box,
    /// Spinner widget shared between the tab label and the workspace.
    spinner: gtk::Spinner,
    /// Set when the tab hasn't been visited yet; consumed by `materialize()`.
    pending_config: Option<WorkspaceConfig>,
    /// Set once the workspace has been constructed.
    workspace: Option<Workspace>,
}

impl TabSlot {
    /// Create a tab that is already built (for the active tab, or new tabs).
    fn new_ready(
        config: WorkspaceConfig,
        theme: Rc<Cell<Theme>>,
        notification_level: Rc<Cell<NotificationLevel>>,
    ) -> Self {
        let spinner = gtk::Spinner::new();
        spinner.set_size_request(12, 12);
        let wrapper = gtk::Box::new(gtk::Orientation::Vertical, 0);
        wrapper.set_vexpand(true);
        wrapper.set_hexpand(true);
        let ws = Workspace::new(config, theme, notification_level, spinner.clone());
        wrapper.append(&ws.root);
        TabSlot {
            wrapper,
            spinner,
            pending_config: None,
            workspace: Some(ws),
        }
    }

    /// Create a lightweight placeholder tab (built on first switch).
    fn new_pending(config: WorkspaceConfig) -> Self {
        let spinner = gtk::Spinner::new();
        spinner.set_size_request(12, 12);
        let wrapper = gtk::Box::new(gtk::Orientation::Vertical, 0);
        wrapper.set_vexpand(true);
        wrapper.set_hexpand(true);
        TabSlot {
            wrapper,
            spinner,
            pending_config: Some(config),
            workspace: None,
        }
    }

    /// Build the workspace if it hasn't been built yet.
    fn materialize(
        &mut self,
        theme: Rc<Cell<Theme>>,
        notification_level: Rc<Cell<NotificationLevel>>,
    ) {
        if self.workspace.is_some() {
            return;
        }
        if let Some(config) = self.pending_config.take() {
            let ws = Workspace::new(config, theme, notification_level, self.spinner.clone());
            self.wrapper.append(&ws.root);
            self.workspace = Some(ws);
        }
    }

    fn workspace_id(&self) -> String {
        if let Some(ref ws) = self.workspace {
            ws.config.borrow().id.clone()
        } else if let Some(ref config) = self.pending_config {
            config.id.clone()
        } else {
            unreachable!("TabSlot has neither workspace nor pending_config")
        }
    }

    /// True once the workspace UI has been built (i.e. it has a live run panel).
    fn has_workspace(&self) -> bool {
        self.workspace.is_some()
    }

    fn working_directory(&self) -> String {
        if let Some(ref ws) = self.workspace {
            ws.config.borrow().working_directory.clone()
        } else if let Some(ref c) = self.pending_config {
            c.working_directory.clone()
        } else {
            String::new()
        }
    }

    /// The Claude session id last reported by the agent (persisted in config).
    fn session_id(&self) -> Option<String> {
        if let Some(ref ws) = self.workspace {
            ws.config.borrow().agent_1_session_id.clone()
        } else if let Some(ref c) = self.pending_config {
            c.agent_1_session_id.clone()
        } else {
            None
        }
    }

    /// Whether the next use of `session_id` should fork it (clone, pre-launch).
    fn fork_session(&self) -> bool {
        if let Some(ref ws) = self.workspace {
            ws.config.borrow().fork_session
        } else if let Some(ref c) = self.pending_config {
            c.fork_session
        } else {
            false
        }
    }

    /// The current displayed tab title (custom label or directory basename).
    fn tab_label_text(&self) -> String {
        if let Some(ref ws) = self.workspace {
            ws.config.borrow().tab_label()
        } else if let Some(ref c) = self.pending_config {
            c.tab_label()
        } else {
            String::new()
        }
    }

    /// A clone of this tab's config (live if materialized, else the pending one).
    fn config_snapshot(&self) -> WorkspaceConfig {
        if let Some(ref ws) = self.workspace {
            ws.config.borrow().clone()
        } else {
            self.pending_config
                .clone()
                .expect("TabSlot has neither workspace nor pending_config")
        }
    }

    /// The chat history to replay into a clone — live in-memory if materialized,
    /// otherwise read from disk.
    fn chat_history_snapshot(&self) -> Vec<session::ChatMessage> {
        if let Some(ref ws) = self.workspace {
            ws.chat_history.borrow().clone()
        } else {
            session::load_chat_history(&self.workspace_id())
        }
    }

    /// Set (or clear) the user's custom tab title and persist it.
    fn set_custom_label(&mut self, label: Option<String>) {
        if let Some(ref ws) = self.workspace {
            ws.config.borrow_mut().custom_tab_label = label;
            session::save_workspace_config(&ws.config.borrow());
        } else if let Some(ref mut c) = self.pending_config {
            c.custom_tab_label = label;
            session::save_workspace_config(c);
        }
    }

    /// Persist this tab's state to disk.
    fn save(&self) {
        if let Some(ref ws) = self.workspace {
            // Update run panel state into config before saving
            {
                let mut cfg = ws.config.borrow_mut();
                cfg.run_tabs = ws.run_panel.run_tab_configs();
                cfg.active_run_tab = ws.run_panel.active_run_tab();
                cfg.terminal_visible = ws.run_panel.is_visible();
            }
            session::save_workspace_config(&ws.config.borrow());
            session::save_chat_history(&ws.config.borrow().id, &ws.chat_history.borrow());
            // Save dirty terminal tab scrollbacks
            ws.run_panel.save_dirty_tabs();
        } else if let Some(ref config) = self.pending_config {
            // Never visited — config is unchanged on disk, save for consistency
            session::save_workspace_config(config);
        }
    }
}

// ── App state ───────────────────────────────────────────────────────────

struct AppState {
    config: AppConfig,
    slots: Vec<TabSlot>,
}

fn build_ui(app: &gtk::Application) {
    session::ensure_default_agents();
    session::ensure_default_bookmarks();
    let mut app_config = session::load_app_config();

    // Theme state
    let theme = Rc::new(Cell::new(app_config.theme));
    let notification_level = Rc::new(Cell::new(app_config.notification_level));

    // CSS
    let css = gtk::CssProvider::new();
    css.load_from_string(css_for_theme(app_config.theme));
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().unwrap(),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    if app_config.theme.is_dark()
        && let Some(settings) = gtk::Settings::default()
    {
        settings.set_gtk_application_prefer_dark_theme(true);
    }
    // Notebook for tabs
    let notebook = gtk::Notebook::new();
    notebook.set_scrollable(true);
    notebook.set_show_border(false);
    notebook.set_tab_pos(gtk::PositionType::Top);
    // No popup_enable(): we provide our own right-click menu on tab labels.

    let app_state = Rc::new(RefCell::new(AppState {
        config: app_config.clone(),
        slots: Vec::new(),
    }));

    // Theme change callback (shared across all workspaces)
    let on_theme_change: Rc<dyn Fn(bool)> = {
        let css = css.clone();
        let theme = Rc::clone(&theme);
        let app_state = Rc::clone(&app_state);
        Rc::new(move |dark: bool| {
            let new_theme = if dark { Theme::Dark } else { Theme::Light };
            theme.set(new_theme);
            // Update GTK dark-theme preference BEFORE loading our CSS so that
            // named colours like @window_fg_color resolve against the correct
            // theme variant.
            if let Some(settings) = gtk::Settings::default() {
                settings.set_gtk_application_prefer_dark_theme(dark);
            }
            css.load_from_string(css_for_theme(new_theme));
            app_state.borrow_mut().config.theme = new_theme;
            // Re-highlight only materialised workspaces
            let rehighlighters: Vec<_> = app_state
                .borrow()
                .slots
                .iter()
                .filter_map(|slot| {
                    slot.workspace
                        .as_ref()
                        .map(|ws| ws.on_theme_rehighlight.clone())
                })
                .collect();
            for rh in &rehighlighters {
                rh(dark);
            }
        })
    };

    // Burger menu (built now, placed on the right side below)
    let menu_btn = build_settings_menu(
        Rc::clone(&theme),
        Rc::clone(&notification_level),
        Rc::clone(&app_state),
        Rc::clone(&on_theme_change),
    );

    // ── Load workspaces (lazy: only active tab is built) ────────────────

    let workspace_configs: Vec<WorkspaceConfig> = app_config
        .workspace_ids
        .iter()
        .filter_map(|id| session::load_workspace_config(id))
        .collect();

    if workspace_configs.is_empty() {
        // No saved session — create one workspace for cwd
        let cwd = std::env::current_dir()
            .unwrap_or_else(|_| "/".into())
            .to_string_lossy()
            .to_string();
        let ws_config = WorkspaceConfig::new(&cwd);
        app_config.workspace_ids.push(ws_config.id.clone());
        app_config.active_tab = 0;

        let label_text = ws_config.tab_label();
        let slot = TabSlot::new_ready(ws_config, Rc::clone(&theme), Rc::clone(&notification_level));
        let label = create_tab_label(
            &label_text,
            &slot.spinner,
            &notebook,
            &slot.wrapper,
            &app_state,
            &theme,
            &notification_level,
        );
        notebook.append_page(&slot.wrapper, Some(&label));
        app_state.borrow_mut().slots.push(slot);
    } else {
        let active_idx = app_config.active_tab.min(workspace_configs.len() - 1);
        let labels = session::dedup_labels(&workspace_configs);
        for (i, ws_config) in workspace_configs.into_iter().enumerate() {
            let slot = if i == active_idx {
                TabSlot::new_ready(ws_config, Rc::clone(&theme), Rc::clone(&notification_level))
            } else {
                TabSlot::new_pending(ws_config)
            };
            let label = create_tab_label(
                &labels[i],
                &slot.spinner,
                &notebook,
                &slot.wrapper,
                &app_state,
                &theme,
                &notification_level,
            );
            notebook.append_page(&slot.wrapper, Some(&label));
            app_state.borrow_mut().slots.push(slot);
        }
        app_config.active_tab = active_idx;
    }

    // "+" and burger menu on the right side of the tab bar
    let add_btn = gtk::Button::from_icon_name("list-add-symbolic");
    add_btn.set_tooltip_text(Some("New workspace"));
    add_btn.set_has_frame(false);

    let right_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    right_box.append(&add_btn);
    right_box.append(&menu_btn);
    notebook.set_action_widget(&right_box, gtk::PackType::End);

    add_btn.connect_clicked(glib::clone!(
        #[weak]
        notebook,
        #[strong]
        app_state,
        #[strong]
        theme,
        #[strong]
        notification_level,
        move |btn| {
            let dialog = gtk::FileDialog::builder()
                .title("Open Folder for New Workspace")
                .modal(true)
                .build();

            let window = btn.root().and_downcast::<gtk::Window>();
            let notebook = notebook.clone();
            let app_state = Rc::clone(&app_state);
            let theme = Rc::clone(&theme);
            let notification_level = Rc::clone(&notification_level);

            dialog.select_folder(window.as_ref(), None::<&gio::Cancellable>, move |result| {
                if let Ok(folder) = result
                    && let Some(path) = folder.path()
                {
                    let dir = path.to_string_lossy().to_string();
                    let ws_config = WorkspaceConfig::new(&dir);
                    let label_text = ws_config.tab_label();

                    let slot = TabSlot::new_ready(
                        ws_config,
                        Rc::clone(&theme),
                        Rc::clone(&notification_level),
                    );
                    let label = create_tab_label(
                        &label_text,
                        &slot.spinner,
                        &notebook,
                        &slot.wrapper,
                        &app_state,
                        &theme,
                        &notification_level,
                    );
                    let page_num = notebook.append_page(&slot.wrapper, Some(&label));
                    notebook.set_current_page(Some(page_num));
                    notebook.set_tab_reorderable(&slot.wrapper, true);

                    let mut state = app_state.borrow_mut();
                    state.config.workspace_ids.push(slot.workspace_id());
                    state.slots.push(slot);
                }
            });
        }
    ));

    // Track active tab changes — materialise lazy tabs on first switch
    notebook.connect_switch_page(glib::clone!(
        #[strong]
        app_state,
        #[strong]
        theme,
        #[strong]
        notification_level,
        move |_nb, _page, page_num| {
            let mut state = app_state.borrow_mut();
            state.config.active_tab = page_num as usize;
            if let Some(slot) = state.slots.get_mut(page_num as usize) {
                slot.materialize(Rc::clone(&theme), Rc::clone(&notification_level));
            }
        }
    ));

    // Enable tab reordering
    for i in 0..notebook.n_pages() {
        if let Some(child) = notebook.nth_page(Some(i)) {
            notebook.set_tab_reorderable(&child, true);
        }
    }

    // Restore active tab
    if app_config.active_tab < notebook.n_pages() as usize {
        notebook.set_current_page(Some(app_config.active_tab as u32));
    }

    // Window
    let mut window_builder = gtk::ApplicationWindow::builder()
        .application(app)
        .title("FlyCrys")
        .icon_name("flycrys")
        .child(&notebook);

    if !app_config.window_maximized {
        window_builder = window_builder
            .default_width(app_config.window_width)
            .default_height(app_config.window_height);
    }

    let window = window_builder.build();

    // Register bundled icons so GTK can find "flycrys" icon by name
    {
        let display = gtk::gdk::Display::default().unwrap();
        let theme = gtk::IconTheme::for_display(&display);
        theme.add_search_path(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/icons/hicolor"),
        );
        if let Ok(exe) = std::env::current_exe()
            && let Some(base) = exe.parent()
        {
            theme.add_search_path(base.join("data/icons/hicolor"));
            theme.add_search_path(base.join("../data/icons/hicolor"));
        }
    }

    if app_config.window_maximized {
        window.maximize();
    }

    // Save window size on close
    window.connect_close_request(glib::clone!(
        #[strong]
        app_state,
        move |win| {
            let mut state = app_state.borrow_mut();
            state.config.window_maximized = win.is_maximized();
            if !win.is_maximized() {
                state.config.window_width = win.width();
                state.config.window_height = win.height();
            }

            // Sync workspace order
            let ids: Vec<String> = state.slots.iter().map(|s| s.workspace_id()).collect();
            state.config.workspace_ids = ids;

            // Save everything
            session::save_app_config(&state.config);
            for slot in &state.slots {
                slot.save();
            }

            glib::Propagation::Proceed
        }
    ));

    // Autosave timer
    {
        let app_state = Rc::clone(&app_state);
        glib::timeout_add_local(
            std::time::Duration::from_secs(AUTOSAVE_INTERVAL_SECS),
            move || {
                let state = app_state.borrow();
                session::save_app_config(&state.config);
                for slot in &state.slots {
                    slot.save();
                }
                glib::ControlFlow::Continue
            },
        );
    }

    window.present();
}

/// Build the settings menu button (burger menu) with theme toggle,
/// notifications toggle, and about dialog.
fn build_settings_menu(
    theme: Rc<Cell<Theme>>,
    notification_level: Rc<Cell<NotificationLevel>>,
    app_state: Rc<RefCell<AppState>>,
    on_theme_change: Rc<dyn Fn(bool)>,
) -> gtk::MenuButton {
    let menu_btn = gtk::MenuButton::new();
    menu_btn.set_icon_name("open-menu-symbolic");
    menu_btn.set_tooltip_text(Some("Menu"));
    menu_btn.set_has_frame(false);

    let popover_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    popover_box.set_margin_start(10);
    popover_box.set_margin_end(10);
    popover_box.set_margin_top(10);
    popover_box.set_margin_bottom(10);

    // Dark Theme toggle
    let theme_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let theme_label = gtk::Label::new(Some("Dark Theme"));
    theme_label.set_hexpand(true);
    theme_label.set_xalign(0.0);
    let theme_switch = gtk::Switch::new();
    theme_switch.set_active(theme.get().is_dark());
    theme_row.append(&theme_label);
    theme_row.append(&theme_switch);
    popover_box.append(&theme_row);

    // Notifications toggle
    let notif_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let notif_label = gtk::Label::new(Some("Notifications"));
    notif_label.set_hexpand(true);
    notif_label.set_xalign(0.0);
    let notif_switch = gtk::Switch::new();
    notif_switch.set_active(notification_level.get().is_enabled());
    notif_row.append(&notif_label);
    notif_row.append(&notif_switch);
    popover_box.append(&notif_row);

    popover_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    // About button
    let about_btn = gtk::Button::with_label("About");
    about_btn.set_has_frame(false);
    popover_box.append(&about_btn);

    let popover = gtk::Popover::new();
    popover.set_child(Some(&popover_box));
    menu_btn.set_popover(Some(&popover));

    // Theme switch handler
    {
        let on_theme_change = Rc::clone(&on_theme_change);
        theme_switch.connect_state_set(move |_, dark| {
            on_theme_change(dark);
            glib::Propagation::Proceed
        });
    }

    // Notifications switch handler
    {
        let notification_level = Rc::clone(&notification_level);
        let app_state = Rc::clone(&app_state);
        notif_switch.connect_state_set(move |_, enabled| {
            let level = if enabled {
                NotificationLevel::All
            } else {
                NotificationLevel::Disabled
            };
            notification_level.set(level);
            app_state.borrow_mut().config.notification_level = level;
            glib::Propagation::Proceed
        });
    }

    // About dialog handler
    {
        let popover = popover.clone();
        about_btn.connect_clicked(move |btn| {
            popover.popdown();

            let claude_version = std::process::Command::new("claude")
                .arg("--version")
                .output()
                .ok()
                .and_then(|out| String::from_utf8(out.stdout).ok())
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "not found".to_string());

            let about = gtk::Window::builder()
                .title("About FlyCrys")
                .modal(true)
                .resizable(false)
                .default_width(480)
                .build();

            if let Some(window) = btn.root().and_downcast::<gtk::Window>() {
                about.set_transient_for(Some(&window));
            }

            let vbox = gtk::Box::new(gtk::Orientation::Vertical, 12);
            vbox.set_margin_top(16);
            vbox.set_margin_bottom(16);
            vbox.set_margin_start(24);
            vbox.set_margin_end(24);
            vbox.set_halign(gtk::Align::Center);

            // Logo — large
            if let Some(texture) = load_about_logo() {
                let picture = gtk::Picture::for_paintable(&texture);
                picture.set_content_fit(gtk::ContentFit::Contain);
                picture.set_size_request(420, 320);
                vbox.append(&picture);
            }

            // Title
            let title = gtk::Label::new(Some("FlyCrys"));
            title.add_css_class("title-1");
            vbox.append(&title);

            // Version
            let version = gtk::Label::new(Some(&format!("v{}", env!("CARGO_PKG_VERSION"))));
            version.add_css_class("dim-label");
            vbox.append(&version);

            // Description
            let desc = gtk::Label::new(Some(&format!(
                "Fast like a fly, solid like a crystal\n\
                 GTK4 workspace with AI agent integration\n\n\
                 Claude CLI: {}",
                claude_version
            )));
            desc.set_justify(gtk::Justification::Center);
            desc.set_wrap(true);
            vbox.append(&desc);

            // GitHub link
            let link = gtk::LinkButton::with_label("https://github.com/SergKam/FlyCrys", "GitHub");
            vbox.append(&link);

            // License
            let license = gtk::Label::new(Some("MIT License"));
            license.add_css_class("dim-label");
            license.add_css_class("caption");
            vbox.append(&license);

            about.set_child(Some(&vbox));
            about.present();
        });
    }

    menu_btn
}

/// Create a tab label widget with spinner, text, and a close button
#[allow(clippy::too_many_arguments)]
fn create_tab_label(
    text: &str,
    tab_spinner: &gtk::Spinner,
    notebook: &gtk::Notebook,
    page_widget: &gtk::Box,
    app_state: &Rc<RefCell<AppState>>,
    theme: &Rc<Cell<Theme>>,
    notification_level: &Rc<Cell<NotificationLevel>>,
) -> gtk::Box {
    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 4);

    hbox.append(tab_spinner);

    let label = gtk::Label::new(Some(text));
    label.set_hexpand(true);
    label.set_xalign(0.0);

    let close_btn = gtk::Button::from_icon_name("window-close-symbolic");
    close_btn.set_has_frame(false);
    close_btn.set_tooltip_text(Some("Close workspace"));
    close_btn.add_css_class("flat");

    hbox.append(&label);
    hbox.append(&close_btn);

    // Right-click (secondary button) → workspace context menu.
    let menu_click = gtk::GestureClick::new();
    menu_click.set_button(gtk::gdk::BUTTON_SECONDARY);
    {
        let label = label.clone();
        let notebook = notebook.clone();
        let page_widget = page_widget.clone();
        let app_state = Rc::clone(app_state);
        let theme = Rc::clone(theme);
        let notification_level = Rc::clone(notification_level);
        let hbox_weak = hbox.downgrade();
        menu_click.connect_pressed(move |gesture, _n, x, y| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            if let Some(anchor) = hbox_weak.upgrade() {
                show_tab_menu(
                    &anchor,
                    &label,
                    &notebook,
                    &page_widget,
                    &app_state,
                    &theme,
                    &notification_level,
                    x,
                    y,
                );
            }
        });
    }
    hbox.add_controller(menu_click);

    close_btn.connect_clicked(glib::clone!(
        #[weak]
        notebook,
        #[weak]
        page_widget,
        #[strong]
        app_state,
        move |btn| {
            // Confirmation dialog
            let dialog = gtk::AlertDialog::builder()
                .message("Close this workspace?")
                .detail("The workspace tab will be closed and removed from the session.")
                .buttons(["Cancel", "Close"])
                .default_button(0)
                .cancel_button(0)
                .build();

            let window = btn.root().and_downcast::<gtk::Window>();
            let notebook = notebook.clone();
            let page_widget = page_widget.clone();
            let app_state = Rc::clone(&app_state);

            dialog.choose(window.as_ref(), None::<&gio::Cancellable>, move |result| {
                if result == Ok(1) {
                    // User confirmed close
                    if let Some(page_num) = notebook.page_num(&page_widget) {
                        // Find and remove slot from state
                        let mut state = app_state.borrow_mut();
                        if let Some(idx) = state.slots.iter().position(|s| s.wrapper == page_widget)
                        {
                            let slot = state.slots.remove(idx);
                            let id = slot.workspace_id();
                            state.config.workspace_ids.retain(|i| i != &id);
                            session::delete_workspace_config(&id);
                            session::delete_chat_history(&id);
                        }
                        drop(state);
                        notebook.remove_page(Some(page_num));
                    }
                }
            });
        }
    ));

    hbox
}

/// Free a transient popover after it closes (GTK4 popovers created on the fly
/// must be unparented or they leak).
fn unparent_on_close(popover: &gtk::Popover) {
    popover.connect_closed(|p| {
        let p = p.clone();
        glib::idle_add_local_once(move || p.unparent());
    });
}

/// Build and show the workspace context menu for a tab.
#[allow(clippy::too_many_arguments)]
fn show_tab_menu(
    anchor: &gtk::Box,
    label: &gtk::Label,
    notebook: &gtk::Notebook,
    page_widget: &gtk::Box,
    app_state: &Rc<RefCell<AppState>>,
    theme: &Rc<Cell<Theme>>,
    notification_level: &Rc<Cell<NotificationLevel>>,
    x: f64,
    y: f64,
) {
    // Read current slot facts under a short borrow.
    let (has_ws, session_id, working_dir) = {
        let state = app_state.borrow();
        match state.slots.iter().find(|s| s.wrapper == *page_widget) {
            Some(slot) => (
                slot.has_workspace(),
                slot.session_id(),
                slot.working_directory(),
            ),
            None => return,
        }
    };

    let popover = gtk::Popover::new();
    popover.set_parent(anchor);
    popover.set_position(gtk::PositionType::Bottom);
    popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
    unparent_on_close(&popover);

    let menu_box = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let make_item = |text: &str| {
        let b = gtk::Button::with_label(text);
        b.set_has_frame(false);
        b.set_hexpand(true);
        if let Some(lbl) = b.child().and_downcast::<gtk::Label>() {
            lbl.set_xalign(0.0);
        }
        b
    };

    // Rename ----------------------------------------------------------------
    let rename = make_item("Rename workspace");
    rename.connect_clicked(glib::clone!(
        #[weak]
        popover,
        #[weak]
        anchor,
        #[weak]
        label,
        #[weak]
        page_widget,
        #[strong]
        app_state,
        move |_| {
            popover.popdown();
            show_rename_popover(&anchor, &label, &page_widget, &app_state);
        }
    ));
    menu_box.append(&rename);

    // Clone -----------------------------------------------------------------
    let clone_item = make_item("Clone workspace");
    clone_item.connect_clicked(glib::clone!(
        #[weak]
        popover,
        #[weak]
        notebook,
        #[weak]
        page_widget,
        #[strong]
        app_state,
        #[strong]
        theme,
        #[strong]
        notification_level,
        move |_| {
            popover.popdown();
            clone_workspace(
                &notebook,
                &page_widget,
                &app_state,
                &theme,
                &notification_level,
            );
        }
    ));
    menu_box.append(&clone_item);

    // Open session in Claude CLI -------------------------------------------
    let cli_item = make_item("Open session in Claude CLI");
    if has_ws && session_id.is_some() {
        cli_item.connect_clicked(glib::clone!(
            #[weak]
            popover,
            #[weak]
            page_widget,
            #[strong]
            app_state,
            move |_| {
                popover.popdown();
                let state = app_state.borrow();
                if let Some(slot) = state.slots.iter().find(|s| s.wrapper == page_widget)
                    && let Some(ws) = slot.workspace.as_ref()
                    && let Some(sid) = slot.session_id()
                {
                    ws.run_panel.open_claude_session(&sid, slot.fork_session());
                }
            }
        ));
    } else {
        cli_item.set_sensitive(false);
        cli_item.set_tooltip_text(Some(if has_ws {
            "No Claude session yet \u{2014} start the agent first"
        } else {
            "Open this workspace first"
        }));
    }
    menu_box.append(&cli_item);

    // Open folder in file manager ------------------------------------------
    let folder_item = make_item("Open folder in file manager");
    folder_item.connect_clicked(glib::clone!(
        #[weak]
        popover,
        move |_| {
            popover.popdown();
            if let Err(e) = flycrys::services::platform::open_with_default(&working_dir) {
                eprintln!("flycrys: open folder failed: {e}");
            }
        }
    ));
    menu_box.append(&folder_item);

    popover.set_child(Some(&menu_box));
    popover.popup();
}

/// Show an inline entry popover to rename a workspace tab.
fn show_rename_popover(
    anchor: &gtk::Box,
    label: &gtk::Label,
    page_widget: &gtk::Box,
    app_state: &Rc<RefCell<AppState>>,
) {
    let popover = gtk::Popover::new();
    popover.set_parent(anchor);
    popover.set_position(gtk::PositionType::Bottom);
    unparent_on_close(&popover);

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let heading = gtk::Label::new(Some("Rename workspace"));
    heading.set_xalign(0.0);
    let entry = gtk::Entry::new();
    entry.set_text(&label.text());
    entry.set_hexpand(true);
    entry.set_placeholder_text(Some("Blank resets to folder name"));
    vbox.append(&heading);
    vbox.append(&entry);
    popover.set_child(Some(&vbox));

    entry.connect_activate(glib::clone!(
        #[weak]
        popover,
        #[weak]
        label,
        #[weak]
        page_widget,
        #[strong]
        app_state,
        move |entry| {
            let trimmed = entry.text().trim().to_string();
            let custom = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            };
            {
                let mut state = app_state.borrow_mut();
                if let Some(slot) = state.slots.iter_mut().find(|s| s.wrapper == page_widget) {
                    slot.set_custom_label(custom);
                    label.set_text(&slot.tab_label_text());
                }
            }
            popover.popdown();
        }
    ));

    popover.popup();
    entry.grab_focus();
}

/// Clone a workspace: a new tab on the same folder, replaying the source's chat
/// history so the new agent continues with the same context.
fn clone_workspace(
    notebook: &gtk::Notebook,
    page_widget: &gtk::Box,
    app_state: &Rc<RefCell<AppState>>,
    theme: &Rc<Cell<Theme>>,
    notification_level: &Rc<Cell<NotificationLevel>>,
) {
    // Snapshot source config + history under a short borrow, then release it.
    let (mut new_cfg, history) = {
        let state = app_state.borrow();
        let Some(slot) = state.slots.iter().find(|s| s.wrapper == *page_widget) else {
            return;
        };
        (slot.config_snapshot(), slot.chat_history_snapshot())
    };

    // Re-key the clone: fresh workspace id and fresh run-tab ids (terminal
    // scrollback files are keyed by these).
    new_cfg.id = uuid::Uuid::new_v4().to_string();
    for rt in &mut new_cfg.run_tabs {
        rt.id = uuid::Uuid::new_v4().to_string();
    }
    let base = new_cfg.tab_label();
    new_cfg.custom_tab_label = Some(format!("{base} (copy)"));

    // Give the clone its OWN Claude session: keep the source session id but
    // mark it to fork on first launch (`--resume <id> --fork-session`), so the
    // two tabs branch into independent sessions natively rather than sharing —
    // and possibly corrupting — one. No-op when there is no source session yet.
    new_cfg.fork_session = new_cfg.agent_1_session_id.is_some();

    // Persist the clone's config and replayed history before materializing,
    // so the new workspace loads the conversation on creation.
    session::save_chat_history(&new_cfg.id, &history);
    session::save_workspace_config(&new_cfg);

    let label_text = new_cfg.tab_label();
    let slot = TabSlot::new_ready(new_cfg, Rc::clone(theme), Rc::clone(notification_level));
    let label = create_tab_label(
        &label_text,
        &slot.spinner,
        notebook,
        &slot.wrapper,
        app_state,
        theme,
        notification_level,
    );
    let page_num = notebook.append_page(&slot.wrapper, Some(&label));
    notebook.set_tab_reorderable(&slot.wrapper, true);

    {
        let mut state = app_state.borrow_mut();
        state.config.workspace_ids.push(slot.workspace_id());
        state.slots.push(slot);
    }
    // Switch after releasing the borrow (switch_page re-borrows app_state).
    notebook.set_current_page(Some(page_num));
}

/// Load the app logo for the About dialog.
fn load_about_logo() -> Option<gtk::gdk::Texture> {
    let exe_dir = std::env::current_exe().ok()?;
    let base = exe_dir.parent()?;
    let candidates = [
        base.join("data/about-logo.png"),
        base.join("../data/about-logo.png"),
        std::path::PathBuf::from("/usr/share/flycrys/data/about-logo.png"),
        std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/data/about-logo.png")),
    ];
    for path in &candidates {
        if path.is_file() {
            let file = gtk::gio::File::for_path(path);
            return gtk::gdk::Texture::from_file(&file).ok();
        }
    }
    None
}
