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
    notebook.popup_enable();

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
                    );
                    let page_num = notebook.append_page(&slot.wrapper, Some(&label));
                    notebook.set_current_page(Some(page_num as u32));
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
fn create_tab_label(
    text: &str,
    tab_spinner: &gtk::Spinner,
    notebook: &gtk::Notebook,
    page_widget: &gtk::Box,
    app_state: &Rc<RefCell<AppState>>,
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
