use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk4 as gtk;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use flycrys::session::{self, AppConfig, WorkspaceConfig};
use flycrys::workspace::Workspace;

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
    "#
}

fn main() -> glib::ExitCode {
    let app = gtk::Application::builder().application_id(APP_ID).build();

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
    });

    app.connect_activate(build_ui);
    app.run()
}

/// Shared app-level state
struct AppState {
    config: AppConfig,
    workspaces: Vec<Workspace>,
}

fn build_ui(app: &gtk::Application) {
    session::ensure_default_agents();
    let mut app_config = session::load_app_config();

    // Theme state
    let is_dark = Rc::new(Cell::new(app_config.is_dark));

    // CSS
    let css = gtk::CssProvider::new();
    css.load_from_string(if app_config.is_dark {
        dark_css()
    } else {
        light_css()
    });
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().unwrap(),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    if app_config.is_dark
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
        workspaces: Vec::new(),
    }));

    // Theme change callback (shared across all workspaces)
    let on_theme_change: Rc<dyn Fn(bool)> = {
        let css = css.clone();
        let is_dark = Rc::clone(&is_dark);
        let app_state = Rc::clone(&app_state);
        Rc::new(move |dark: bool| {
            is_dark.set(dark);
            css.load_from_string(if dark { dark_css() } else { light_css() });
            if let Some(settings) = gtk::Settings::default() {
                settings.set_gtk_application_prefer_dark_theme(dark);
            }
            app_state.borrow_mut().config.is_dark = dark;
            // Re-highlight the current file in every workspace
            let rehighlighters: Vec<_> = app_state
                .borrow()
                .workspaces
                .iter()
                .map(|ws| ws.on_theme_rehighlight.clone())
                .collect();
            for rh in &rehighlighters {
                rh(dark);
            }
        })
    };

    // Burger menu on the left side of the tab bar
    {
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
        theme_switch.set_active(is_dark.get());
        theme_row.append(&theme_label);
        theme_row.append(&theme_switch);
        popover_box.append(&theme_row);

        popover_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        // About button
        let about_btn = gtk::Button::with_label("About");
        about_btn.set_has_frame(false);
        popover_box.append(&about_btn);

        let popover = gtk::Popover::new();
        popover.set_child(Some(&popover_box));
        menu_btn.set_popover(Some(&popover));

        notebook.set_action_widget(&menu_btn, gtk::PackType::Start);

        // Theme switch handler
        {
            let on_theme_change = Rc::clone(&on_theme_change);
            theme_switch.connect_state_set(move |_, dark| {
                on_theme_change(dark);
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

                // Load the app logo from the icons directory
                let logo = load_about_logo();

                let mut builder = gtk::AboutDialog::builder()
                    .program_name("FlyCrys")
                    .version(env!("CARGO_PKG_VERSION"))
                    .comments(format!(
                        "Fast like a fly, solid like a crystal\n\
                         GTK4 workspace with AI agent integration\n\n\
                         Claude CLI: {}",
                        claude_version
                    ))
                    .license_type(gtk::License::MitX11)
                    .modal(true);

                if let Some(ref texture) = logo {
                    builder = builder.logo(texture);
                }

                let about = builder.build();

                if let Some(window) = btn.root().and_downcast::<gtk::Window>() {
                    about.set_transient_for(Some(&window));
                }
                about.present();
            });
        }
    }

    // Load existing workspaces or show folder chooser
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

        let ws = Workspace::new(ws_config, Rc::clone(&is_dark));
        let label = create_tab_label(
            &ws.config.borrow().tab_label(),
            &ws.tab_spinner,
            &notebook,
            &ws.root,
            &app_state,
        );
        notebook.append_page(&ws.root, Some(&label));
        app_state.borrow_mut().workspaces.push(ws);
    } else {
        let labels = session::dedup_labels(&workspace_configs);
        for (i, ws_config) in workspace_configs.into_iter().enumerate() {
            let ws = Workspace::new(ws_config, Rc::clone(&is_dark));
            let label =
                create_tab_label(&labels[i], &ws.tab_spinner, &notebook, &ws.root, &app_state);
            notebook.append_page(&ws.root, Some(&label));
            app_state.borrow_mut().workspaces.push(ws);
        }
    }

    // "+" button to add new workspace
    let add_btn = gtk::Button::from_icon_name("list-add-symbolic");
    add_btn.set_tooltip_text(Some("New workspace"));
    add_btn.set_has_frame(false);
    notebook.set_action_widget(&add_btn, gtk::PackType::End);

    add_btn.connect_clicked(glib::clone!(
        #[weak]
        notebook,
        #[strong]
        app_state,
        #[strong]
        is_dark,
        move |btn| {
            let dialog = gtk::FileDialog::builder()
                .title("Open Folder for New Workspace")
                .modal(true)
                .build();

            let window = btn.root().and_downcast::<gtk::Window>();
            let notebook = notebook.clone();
            let app_state = Rc::clone(&app_state);
            let is_dark = Rc::clone(&is_dark);

            dialog.select_folder(window.as_ref(), None::<&gio::Cancellable>, move |result| {
                if let Ok(folder) = result
                    && let Some(path) = folder.path()
                {
                    let dir = path.to_string_lossy().to_string();
                    let ws_config = WorkspaceConfig::new(&dir);

                    let ws = Workspace::new(ws_config, Rc::clone(&is_dark));
                    let label_text = ws.config.borrow().tab_label();
                    let label = create_tab_label(
                        &label_text,
                        &ws.tab_spinner,
                        &notebook,
                        &ws.root,
                        &app_state,
                    );
                    let page_num = notebook.append_page(&ws.root, Some(&label));
                    notebook.set_current_page(Some(page_num as u32));
                    notebook.set_tab_reorderable(&ws.root, true);

                    let mut state = app_state.borrow_mut();
                    state
                        .config
                        .workspace_ids
                        .push(ws.config.borrow().id.clone());
                    state.workspaces.push(ws);
                }
            });
        }
    ));

    // Track active tab changes
    notebook.connect_switch_page(glib::clone!(
        #[strong]
        app_state,
        move |_nb, _page, page_num| {
            app_state.borrow_mut().config.active_tab = page_num as usize;
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
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("FlyCrys")
        .icon_name(APP_ID)
        .default_width(app_config.window_width)
        .default_height(app_config.window_height)
        .child(&notebook)
        .build();

    // Save window size on close
    window.connect_close_request(glib::clone!(
        #[strong]
        app_state,
        move |win| {
            let (w, h) = win.default_size();
            let mut state = app_state.borrow_mut();
            state.config.window_width = w;
            state.config.window_height = h;

            // Sync workspace order from notebook tabs
            // (in case user reordered tabs — we just save current order)
            let ids: Vec<String> = state
                .workspaces
                .iter()
                .map(|ws| ws.config.borrow().id.clone())
                .collect();
            state.config.workspace_ids = ids;

            // Save everything
            session::save_app_config(&state.config);
            for ws in &state.workspaces {
                session::save_workspace_config(&ws.config.borrow());
                session::save_chat_history(&ws.config.borrow().id, &ws.chat_history.borrow());
            }

            glib::Propagation::Proceed
        }
    ));

    // Autosave every 5 seconds
    {
        let app_state = Rc::clone(&app_state);
        glib::timeout_add_local(std::time::Duration::from_secs(5), move || {
            let state = app_state.borrow();
            session::save_app_config(&state.config);
            for ws in &state.workspaces {
                session::save_workspace_config(&ws.config.borrow());
                session::save_chat_history(&ws.config.borrow().id, &ws.chat_history.borrow());
            }
            glib::ControlFlow::Continue
        });
    }

    window.present();
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
                        // Find and remove workspace from state
                        let mut state = app_state.borrow_mut();
                        if let Some(idx) = state
                            .workspaces
                            .iter()
                            .position(|ws| ws.root == page_widget)
                        {
                            let ws = state.workspaces.remove(idx);
                            let id = ws.config.borrow().id.clone();
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

/// Load the app logo for the About dialog from the icons directory.
fn load_about_logo() -> Option<gtk::gdk::Texture> {
    let exe_dir = std::env::current_exe().ok()?;
    let base = exe_dir.parent()?;
    // Try locations relative to the binary and the source tree
    let candidates = [
        base.join("icons/hicolor/256x256/apps/com.flycrys.app.png"),
        base.join("../icons/hicolor/256x256/apps/com.flycrys.app.png"),
        std::path::PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/icons/hicolor/256x256/apps/com.flycrys.app.png"
        )),
    ];
    for path in &candidates {
        if path.is_file() {
            let file = gtk::gio::File::for_path(path);
            return gtk::gdk::Texture::from_file(&file).ok();
        }
    }
    None
}
