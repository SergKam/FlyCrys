mod chat_factory;
mod event_handler;
mod state;

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk4 as gtk;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc;

use crate::chat_webview::ChatWebView;
use crate::config::constants::{AGENT_PANEL_MIN_WIDTH, DEFAULT_CONTEXT_WINDOW, INPUT_MAX_HEIGHT};
use crate::config::types::{NotificationLevel, Theme};
use crate::models::agent_config::AgentConfig;
use crate::models::chat::ChatMessage;
use crate::services::cli::claude::ClaudeBackend;
use crate::services::cli::{AgentBackend, AgentDomainEvent, AgentSpawnConfig, ImageAttachment};

use state::{AgentProcessState, ChatState, PanelConfig, PanelState, TokenState};

/// How many history entries to show initially / per "Load previous" click.
const PAGE_SIZE: usize = 100;

struct AttachedImage {
    bytes: Vec<u8>,
    mime_type: String,
    texture: gtk::gdk::Texture,
}

#[allow(clippy::too_many_arguments)]
pub fn create_agent_panel(
    on_open_file: Rc<dyn Fn(&str)>,
    theme: Rc<Cell<Theme>>,
    notification_level: Rc<Cell<NotificationLevel>>,
    tab_spinner: gtk::Spinner,
    working_dir: &std::path::Path,
    title_text: &str,
    agent_configs: Vec<AgentConfig>,
    initial_profile: &str,
    resume_session_id: Option<String>,
    on_profile_change: Rc<dyn Fn(&str)>,
    on_session_id_change: Rc<dyn Fn(Option<String>)>,
    chat_history: Rc<RefCell<Vec<ChatMessage>>>,
    on_tool_result: Option<Rc<dyn Fn()>>,
    // External labels living in the workspace status bar — the panel updates their text.
    token_label: gtk::Label,
    cost_label: gtk::Label,
) -> (gtk::Box, gtk::TextView) {
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.set_width_request(AGENT_PANEL_MIN_WIDTH);

    // Header
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    header.set_margin_start(8);
    header.set_margin_end(8);
    header.set_margin_top(6);
    header.set_margin_bottom(6);

    let title = gtk::Label::new(Some(title_text));
    title.add_css_class("heading");
    title.set_xalign(0.0);

    // Agent profile dropdown
    let profile_names: Vec<&str> = agent_configs.iter().map(|c| c.name.as_str()).collect();
    let string_list = gtk::StringList::new(&profile_names);
    let dropdown = gtk::DropDown::new(Some(string_list), gtk::Expression::NONE);
    dropdown.set_tooltip_text(Some("Agent profile"));

    let initial_idx = agent_configs
        .iter()
        .position(|c| c.name == initial_profile)
        .unwrap_or(0);
    dropdown.set_selected(initial_idx as u32);

    header.append(&title);
    header.append(&dropdown);

    let gear_btn = gtk::Button::from_icon_name("emblem-system-symbolic");
    gear_btn.set_tooltip_text(Some("Configure agents"));
    gear_btn.set_has_frame(false);
    header.append(&gear_btn);

    // --- Chat area: WebKitGTK WebView ---
    let is_dark = theme.get().is_dark();
    let webview = ChatWebView::new(is_dark, on_open_file.clone());
    let webview_widget = webview.widget().clone();
    webview_widget.set_vexpand(true);
    webview_widget.set_hexpand(true);

    // Attachments preview bar (hidden when empty)
    let attach_bar = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    attach_bar.set_margin_start(8);
    attach_bar.set_margin_end(8);
    attach_bar.set_margin_top(2);
    attach_bar.set_margin_bottom(2);
    attach_bar.set_visible(false);

    let attachments: Rc<RefCell<Vec<AttachedImage>>> = Rc::new(RefCell::new(Vec::new()));

    // Input area
    let input_frame = gtk::Frame::new(None);
    input_frame.set_hexpand(true);
    input_frame.set_margin_start(8);
    input_frame.set_margin_end(8);
    input_frame.set_margin_top(2);

    let input_view = gtk::TextView::new();
    input_view.set_wrap_mode(gtk::WrapMode::WordChar);
    input_view.set_left_margin(6);
    input_view.set_right_margin(6);
    input_view.set_top_margin(4);
    input_view.set_bottom_margin(4);
    input_view.set_height_request(50);

    let input_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .max_content_height(INPUT_MAX_HEIGHT)
        .propagate_natural_height(true)
        .child(&input_view)
        .build();

    input_frame.set_child(Some(&input_scroll));

    // --- Bottom toolbar ---
    let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    toolbar.set_margin_start(8);
    toolbar.set_margin_end(8);
    toolbar.set_margin_top(2);
    toolbar.set_margin_bottom(6);

    let attach_menu = gio::Menu::new();
    attach_menu.append(Some("Attach Image…"), Some("panel.attach-image"));
    attach_menu.append(Some("Select Files…"), Some("panel.pick-files"));
    attach_menu.append(Some("Select Folder…"), Some("panel.pick-folder"));
    let attach_btn = gtk::MenuButton::new();
    attach_btn.set_icon_name("mail-attachment-symbolic");
    attach_btn.set_tooltip_text(Some("Attach image / insert file path"));
    attach_btn.set_has_frame(false);
    attach_btn.set_direction(gtk::ArrowType::Up);
    attach_btn.set_menu_model(Some(&attach_menu));

    let pause_btn = gtk::Button::from_icon_name("media-playback-pause-symbolic");
    pause_btn.set_tooltip_text(Some("Pause"));
    pause_btn.set_has_frame(false);
    pause_btn.set_sensitive(false);

    let stop_btn = gtk::Button::from_icon_name("media-playback-stop-symbolic");
    stop_btn.set_tooltip_text(Some("Stop"));
    stop_btn.set_has_frame(false);
    stop_btn.set_sensitive(false);

    let compact_btn = gtk::Button::from_icon_name("edit-cut-symbolic");
    compact_btn.set_tooltip_text(Some("Compact conversation to save tokens"));
    compact_btn.set_has_frame(false);

    let quick_menu = gio::Menu::new();
    let quick_btn = gtk::MenuButton::new();
    quick_btn.set_icon_name("user-bookmarks-symbolic");
    quick_btn.set_tooltip_text(Some("Bookmarks"));
    quick_btn.set_has_frame(false);
    quick_btn.set_direction(gtk::ArrowType::Up);
    quick_btn.set_menu_model(Some(&quick_menu));

    // Populate bookmark menu from disk (also called after dialog save).
    fn rebuild_bookmark_menu(menu: &gio::Menu) {
        menu.remove_all();
        let bookmarks_section = gio::Menu::new();
        for (i, bm) in crate::session::load_bookmarks().iter().enumerate() {
            bookmarks_section.append(Some(&bm.name), Some(&format!("panel.bookmark-{i}")));
        }
        menu.append_section(None, &bookmarks_section);

        let config_section = gio::Menu::new();
        let config_item = gio::MenuItem::new(Some("Configure…"), Some("panel.bookmark-configure"));
        config_item.set_icon(&gio::ThemedIcon::new("emblem-system-symbolic"));
        config_section.append_item(&config_item);
        menu.append_section(None, &config_section);
    }
    rebuild_bookmark_menu(&quick_menu);

    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);

    let send_btn = gtk::Button::from_icon_name("go-next-symbolic");
    send_btn.set_tooltip_text(Some("Send (Ctrl+Enter)"));
    send_btn.add_css_class("suggested-action");

    toolbar.append(&attach_btn);
    toolbar.append(&pause_btn);
    toolbar.append(&stop_btn);
    toolbar.append(&compact_btn);
    toolbar.append(&quick_btn);
    toolbar.append(&spacer);
    toolbar.append(&send_btn);

    panel.append(&header);
    panel.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    panel.append(&webview_widget);
    panel.append(&attach_bar);
    panel.append(&input_frame);
    panel.append(&toolbar);

    // --- Compute first page boundaries ---
    let total_history = chat_history.borrow().len();
    let page_start = total_history.saturating_sub(PAGE_SIZE);

    // --- State ---
    let state = Rc::new(RefCell::new(PanelState {
        process: AgentProcessState {
            process: ClaudeBackend::new(),
            session_id: resume_session_id,
            working_dir: working_dir.to_path_buf(),
        },
        tokens: TokenState {
            context_tokens: 0,
            context_window_max: DEFAULT_CONTEXT_WINDOW,
            total_cost_usd: 0.0,
            token_label: token_label.clone(),
            cost_label: cost_label.clone(),
        },
        chat: ChatState {
            webview,
            oldest_rendered_idx: total_history, // nothing rendered yet
            current_streaming: false,
            current_stream_id: None,
            current_text: String::new(),
            pending_tools: HashMap::new(),
            thinking_id: None,
            chat_history,
        },
        config: PanelConfig {
            agent_configs,
            selected_profile_idx: initial_idx,
            theme,
            notification_level,
        },
        tab_spinner,
        on_open_file,
        on_session_id_change,
        on_profile_change,
        on_tool_result,
    }));

    // Show "Load previous" link if there are older entries
    if page_start > 0 {
        state.borrow().chat.webview.show_load_prev_button();
    }

    // --- Deferred first-page load ---
    {
        let state_load = Rc::clone(&state);
        glib::idle_add_local_once(move || {
            let mut s = state_load.borrow_mut();
            let history = s.chat.chat_history.borrow();
            let total = history.len();
            let dark = s.config.theme.get().is_dark();

            for i in page_start..total {
                chat_factory::render_history_message(&s.chat.webview, &history[i], dark);
            }
            drop(history);
            s.chat.oldest_rendered_idx = page_start;
            drop(s);
        });
    }

    // --- "Load previous" via flycrys://load-prev ---
    {
        let state_prev = Rc::clone(&state);
        state
            .borrow()
            .chat
            .webview
            .set_on_load_prev(Rc::new(move || {
                let mut s = state_prev.borrow_mut();
                let current_start = s.chat.oldest_rendered_idx;
                if current_start == 0 {
                    s.chat.webview.hide_load_prev_button();
                    return;
                }
                let batch = PAGE_SIZE.min(current_start);
                let new_start = current_start - batch;
                let history = s.chat.chat_history.borrow();
                let dark = s.config.theme.get().is_dark();

                // Prepend older entries (build HTML, inject before existing content).
                // We use a temporary approach: prepend each message in reverse order
                // so they end up in the correct chronological order.
                // The JS `prependMsg` inserts at the top of #chat.
                for i in (new_start..current_start).rev() {
                    chat_factory::render_history_message_prepend(
                        &s.chat.webview,
                        &history[i],
                        dark,
                    );
                }
                drop(history);
                s.chat.oldest_rendered_idx = new_start;
                if new_start == 0 {
                    s.chat.webview.hide_load_prev_button();
                }
            }));
    }

    // Dropdown selection change
    {
        let state = Rc::clone(&state);
        dropdown.connect_selected_notify(move |dd| {
            let idx = dd.selected() as usize;
            let mut s = state.borrow_mut();
            s.config.selected_profile_idx = idx;
            s.process.session_id = None;
            (s.on_session_id_change)(None);
            if let Some(cfg) = s.config.agent_configs.get(idx) {
                (s.on_profile_change)(&cfg.name);
            }
        });
    }

    // Gear button: open agent config dialog
    {
        let state = Rc::clone(&state);
        let dropdown = dropdown.clone();
        gear_btn.connect_clicked(move |btn| {
            let Some(window) = btn.root().and_downcast::<gtk::Window>() else {
                return;
            };
            let state_clone = Rc::clone(&state);
            let dropdown_clone = dropdown.clone();
            crate::agent_config_dialog::show(&window, move |new_configs| {
                let names: Vec<&str> = new_configs.iter().map(|c| c.name.as_str()).collect();
                let new_model = gtk::StringList::new(&names);
                dropdown_clone.set_model(Some(&new_model));

                let mut s = state_clone.borrow_mut();
                let current_name = s
                    .config
                    .agent_configs
                    .get(s.config.selected_profile_idx)
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                let new_idx = new_configs
                    .iter()
                    .position(|c| c.name == current_name)
                    .unwrap_or(0);

                s.config.agent_configs = new_configs;
                s.config.selected_profile_idx = new_idx;
                dropdown_clone.set_selected(new_idx as u32);
            });
        });
    }

    // (Attach image / file / folder picker actions are registered in the action group below.)

    // --- Send handler ---
    let do_send = {
        let state = Rc::clone(&state);
        let input_view = input_view.clone();
        let send_btn = send_btn.clone();
        let pause_btn = pause_btn.clone();
        let stop_btn = stop_btn.clone();
        let attachments = Rc::clone(&attachments);
        let attach_bar = attach_bar.clone();

        move || {
            let buffer = input_view.buffer();
            let text = buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), false)
                .to_string();
            let text = text.trim().to_string();

            let has_images = !attachments.borrow().is_empty();
            if text.is_empty() && !has_images {
                return;
            }
            buffer.set_text("");

            let images: Vec<AttachedImage> = attachments.borrow_mut().drain(..).collect();
            rebuild_attach_bar(&attachments, &attach_bar);

            // Append user message to WebView
            {
                let s = state.borrow();
                // For images: convert to data URIs
                let data_uris: Vec<String> = images
                    .iter()
                    .map(|img| {
                        let b64 = base64::Engine::encode(
                            &base64::engine::general_purpose::STANDARD,
                            &img.bytes,
                        );
                        format!("data:{};base64,{}", img.mime_type, b64)
                    })
                    .collect();
                s.chat.webview.append_user_message(&text, &data_uris);
            }

            // Record in history
            if !text.is_empty() {
                state
                    .borrow()
                    .chat
                    .chat_history
                    .borrow_mut()
                    .push(ChatMessage::User { text: text.clone() });
            }

            let api_images: Vec<ImageAttachment> = images
                .into_iter()
                .map(|img| ImageAttachment {
                    bytes: img.bytes,
                    media_type: img.mime_type,
                })
                .collect();

            let mut s = state.borrow_mut();

            if !s.process.process.is_alive() {
                let (sender, receiver) = mpsc::channel::<AgentDomainEvent>();
                let receiver = Rc::new(RefCell::new(Some(receiver)));

                let spawn_config = {
                    let cfg = s.config.agent_configs.get(s.config.selected_profile_idx);
                    AgentSpawnConfig {
                        system_prompt: cfg.map(|c| c.system_prompt.clone()),
                        allowed_tools: cfg.map(|c| c.allowed_tools.clone()).unwrap_or_default(),
                        model: cfg.and_then(|c| c.model.clone()),
                        resume_session_id: s.process.session_id.clone(),
                    }
                };

                let wd = s.process.working_dir.clone();
                if let Err(err_msg) = s.process.process.spawn(sender, &wd, &spawn_config) {
                    s.chat
                        .webview
                        .append_system_message(&format!("Failed to start claude CLI: {err_msg}"));
                    return;
                }

                let state_rx = Rc::clone(&state);
                let send_btn_rx = send_btn.clone();
                let pause_btn_rx = pause_btn.clone();
                let stop_btn_rx = stop_btn.clone();

                glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
                    let rx_ref = receiver.borrow();
                    let Some(ref rx) = *rx_ref else {
                        return glib::ControlFlow::Break;
                    };
                    while let Ok(event) = rx.try_recv() {
                        event_handler::handle_domain_event(
                            &state_rx,
                            &send_btn_rx,
                            &pause_btn_rx,
                            &stop_btn_rx,
                            event,
                        );
                    }
                    glib::ControlFlow::Continue
                });
            }

            let _ = s.process.process.send_message(&text, &api_images);

            // Show thinking indicator in WebView
            let thinking_id = s.chat.webview.show_thinking();
            s.chat.thinking_id = Some(thinking_id);
            s.tab_spinner.set_spinning(true);
            drop(s);

            send_btn.set_sensitive(false);
            pause_btn.set_sensitive(true);
            stop_btn.set_sensitive(true);
        }
    };

    // Send button click
    let do_send_click = do_send.clone();
    send_btn.connect_clicked(move |_| do_send_click());

    // Ctrl+Enter to send, Ctrl+V to paste image from clipboard
    let key_ctrl = gtk::EventControllerKey::new();
    let do_send_key = do_send.clone();
    let att_key = Rc::clone(&attachments);
    let bar_key = attach_bar.clone();
    let input_for_key = input_view.clone();
    key_ctrl.connect_key_pressed(move |_, key, _, modifiers| {
        let is_ctrl = modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK);

        if key == gtk::gdk::Key::Return && is_ctrl {
            do_send_key();
            glib::Propagation::Stop
        } else if key == gtk::gdk::Key::v && is_ctrl {
            let clipboard = input_for_key.clipboard();
            let formats = clipboard.formats();
            if formats.contains_type(gtk::gdk::Texture::static_type()) {
                let att = Rc::clone(&att_key);
                let bar = bar_key.clone();
                clipboard.read_texture_async(None::<&gtk::gio::Cancellable>, move |result| {
                    if let Ok(Some(texture)) = result {
                        let png_bytes = texture.save_to_png_bytes();
                        att.borrow_mut().push(AttachedImage {
                            bytes: png_bytes.to_vec(),
                            mime_type: "image/png".to_string(),
                            texture,
                        });
                        rebuild_attach_bar(&att, &bar);
                    }
                });
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        } else {
            glib::Propagation::Proceed
        }
    });
    input_view.add_controller(key_ctrl);

    // Pause button
    {
        let state = Rc::clone(&state);
        pause_btn.connect_clicked(move |btn| {
            let mut s = state.borrow_mut();
            if s.process.process.is_running() {
                s.process.process.pause();
                btn.set_icon_name("media-playback-start-symbolic");
                btn.set_tooltip_text(Some("Resume"));
            } else if s.process.process.is_paused() {
                s.process.process.resume();
                btn.set_icon_name("media-playback-pause-symbolic");
                btn.set_tooltip_text(Some("Pause"));
            }
        });
    }

    // Stop button
    {
        let state = Rc::clone(&state);
        let send_btn = send_btn.clone();
        let pause_btn = pause_btn.clone();
        let stop_btn_clone = stop_btn.clone();
        stop_btn_clone.connect_clicked(move |btn| {
            let mut s = state.borrow_mut();
            s.process.process.stop();
            event_handler::remove_thinking(&mut s);
            s.tab_spinner.set_spinning(false);
            s.chat.chat_history.borrow_mut().push(ChatMessage::System {
                text: "\u{23f9} Stopped".to_string(),
            });
            s.chat.webview.append_system_message("\u{23f9} Stopped");
            drop(s);
            send_btn.set_sensitive(true);
            pause_btn.set_sensitive(false);
            pause_btn.set_icon_name("media-playback-pause-symbolic");
            pause_btn.set_tooltip_text(Some("Pause"));
            btn.set_sensitive(false);
        });
    }

    // Compact button
    {
        let do_send_compact = do_send.clone();
        let input_view_compact = input_view.clone();
        compact_btn.connect_clicked(move |_| {
            input_view_compact.buffer().set_text("/compact");
            do_send_compact();
        });
    }

    // Bookmark actions — dynamic based on loaded bookmarks
    {
        let action_group = gio::SimpleActionGroup::new();

        // Register actions for each bookmark (bookmark-0, bookmark-1, …)
        let bookmarks = crate::session::load_bookmarks();
        for (i, bm) in bookmarks.iter().enumerate() {
            let action = gio::SimpleAction::new(&format!("bookmark-{i}"), None);
            let iv = input_view.clone();
            let ds = do_send.clone();
            let text = bm.prompt.clone();
            action.connect_activate(move |_, _| {
                iv.buffer().set_text(&text);
                ds();
            });
            action_group.add_action(&action);
        }

        // "Configure…" action — opens bookmark editor dialog
        let configure_action = gio::SimpleAction::new("bookmark-configure", None);
        {
            let panel_ref = panel.clone();
            let quick_menu = quick_menu.clone();
            let action_group = action_group.clone();
            let input_view = input_view.clone();
            let do_send = do_send.clone();
            configure_action.connect_activate(move |_, _| {
                let win = panel_ref
                    .root()
                    .and_then(|r| r.downcast::<gtk::Window>().ok());
                let Some(win) = win.as_ref() else { return };

                let qm = quick_menu.clone();
                let ag = action_group.clone();
                let iv = input_view.clone();
                let ds = do_send.clone();
                crate::bookmark_dialog::show(win, move |new_bookmarks| {
                    // Re-register all bookmark actions with updated prompts
                    // First remove old bookmark-N actions
                    for i in 0..200 {
                        let name = format!("bookmark-{i}");
                        if ag.lookup_action(&name).is_some() {
                            ag.remove_action(&name);
                        } else {
                            break;
                        }
                    }

                    for (i, bm) in new_bookmarks.iter().enumerate() {
                        let action = gio::SimpleAction::new(&format!("bookmark-{i}"), None);
                        let iv = iv.clone();
                        let ds = ds.clone();
                        let text = bm.prompt.clone();
                        action.connect_activate(move |_, _| {
                            iv.buffer().set_text(&text);
                            ds();
                        });
                        ag.add_action(&action);
                    }

                    rebuild_bookmark_menu(&qm);
                });
            });
        }
        action_group.add_action(&configure_action);

        // Attach image action
        let attach_image_action = gio::SimpleAction::new("attach-image", None);
        {
            let att = Rc::clone(&attachments);
            let bar = attach_bar.clone();
            let panel_ref = panel.clone();
            attach_image_action.connect_activate(move |_, _| {
                let filter = gtk::FileFilter::new();
                filter.set_name(Some("Images"));
                for mime in crate::config::constants::SUPPORTED_IMAGE_MIME {
                    filter.add_mime_type(mime);
                }
                let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
                filters.append(&filter);

                let dialog = gtk::FileDialog::builder()
                    .title("Attach Image")
                    .modal(true)
                    .build();
                dialog.set_filters(Some(&filters));
                dialog.set_default_filter(Some(&filter));

                let window = panel_ref.root().and_downcast::<gtk::Window>();
                let att = Rc::clone(&att);
                let bar = bar.clone();
                dialog.open(
                    window.as_ref(),
                    None::<&gtk::gio::Cancellable>,
                    move |result| {
                        if let Ok(file) = result
                            && let Some(path) = file.path()
                            && let Ok(bytes) = std::fs::read(&path)
                        {
                            let mime = mime_for_path(&path);
                            let gbytes = glib::Bytes::from(&bytes);
                            if let Ok(texture) = gtk::gdk::Texture::from_bytes(&gbytes) {
                                att.borrow_mut().push(AttachedImage {
                                    bytes,
                                    mime_type: mime,
                                    texture,
                                });
                                rebuild_attach_bar(&att, &bar);
                            }
                        }
                    },
                );
            });
        }
        action_group.add_action(&attach_image_action);

        // File/folder picker actions
        fn insert_paths(iv: &gtk::TextView, files: &gtk::gio::ListModel) {
            let buf = iv.buffer();
            let mut end = buf.end_iter();
            for i in 0..files.n_items() {
                if let Some(obj) = files.item(i)
                    && let Ok(file) = obj.downcast::<gtk::gio::File>()
                    && let Some(path) = file.path()
                {
                    let text = buf.text(&buf.start_iter(), &end, false);
                    if !text.is_empty() && !text.ends_with(' ') && !text.ends_with('\n') {
                        buf.insert(&mut end, " ");
                    }
                    buf.insert(&mut end, &path.to_string_lossy());
                }
            }
            iv.grab_focus();
        }

        let pick_files_action = gio::SimpleAction::new("pick-files", None);
        {
            let iv = input_view.clone();
            let panel_ref = panel.clone();
            pick_files_action.connect_activate(move |_, _| {
                let dialog = gtk::FileDialog::builder()
                    .title("Select files")
                    .modal(true)
                    .build();
                let window = panel_ref.root().and_downcast::<gtk::Window>();
                let iv = iv.clone();
                dialog.open_multiple(window.as_ref(), None::<&gtk::gio::Cancellable>, move |r| {
                    if let Ok(files) = r {
                        insert_paths(&iv, &files);
                    }
                });
            });
        }
        action_group.add_action(&pick_files_action);

        let pick_folder_action = gio::SimpleAction::new("pick-folder", None);
        {
            let iv = input_view.clone();
            let panel_ref = panel.clone();
            pick_folder_action.connect_activate(move |_, _| {
                let dialog = gtk::FileDialog::builder()
                    .title("Select folder")
                    .modal(true)
                    .build();
                let window = panel_ref.root().and_downcast::<gtk::Window>();
                let iv = iv.clone();
                dialog.select_folder(window.as_ref(), None::<&gtk::gio::Cancellable>, move |r| {
                    if let Ok(file) = r {
                        let list = gtk::gio::ListStore::new::<gtk::gio::File>();
                        list.append(&file);
                        insert_paths(&iv, list.upcast_ref());
                    }
                });
            });
        }
        action_group.add_action(&pick_folder_action);

        panel.insert_action_group("panel", Some(&action_group));
    }

    (panel, input_view)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn rebuild_attach_bar(attachments: &Rc<RefCell<Vec<AttachedImage>>>, attach_bar: &gtk::Box) {
    while let Some(child) = attach_bar.first_child() {
        attach_bar.remove(&child);
    }
    let att = attachments.borrow();
    attach_bar.set_visible(!att.is_empty());
    for (idx, img) in att.iter().enumerate() {
        let thumb_box = gtk::Overlay::new();
        let picture = gtk::Picture::for_paintable(&img.texture);
        picture.set_content_fit(gtk::ContentFit::Contain);
        let frame = gtk::Frame::new(None);
        frame.set_size_request(64, 64);
        frame.set_overflow(gtk::Overflow::Hidden);
        frame.set_child(Some(&picture));
        frame.add_css_class("attach-thumb");
        thumb_box.set_child(Some(&frame));
        let remove_btn = gtk::Button::from_icon_name("window-close-symbolic");
        remove_btn.add_css_class("circular");
        remove_btn.add_css_class("osd");
        remove_btn.set_halign(gtk::Align::End);
        remove_btn.set_valign(gtk::Align::Start);
        thumb_box.add_overlay(&remove_btn);
        let att_clone = Rc::clone(attachments);
        let bar_clone = attach_bar.clone();
        remove_btn.connect_clicked(move |_| {
            let mut v = att_clone.borrow_mut();
            if idx < v.len() {
                v.remove(idx);
            }
            drop(v);
            rebuild_attach_bar(&att_clone, &bar_clone);
        });
        attach_bar.append(&thumb_box);
    }
}

fn mime_for_path(path: &std::path::Path) -> String {
    match path.extension().and_then(|e| e.to_str()) {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "image/png",
    }
    .to_string()
}

/// Format token count as human-readable string
pub(super) fn format_token_count(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{}K", tokens / 1_000)
    } else {
        format!("{tokens}")
    }
}

/// Extract a short display string from tool input JSON
pub(super) fn extract_tool_display(tool_name: &str, json_str: &str) -> String {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
        if let Some(cmd) = val.get("command").and_then(|v| v.as_str()) {
            return cmd.to_string();
        }
        if let Some(q) = val.get("pattern").and_then(|v| v.as_str()) {
            let path = val.get("path").and_then(|v| v.as_str()).unwrap_or("");
            return format!("{q} {path}").trim().to_string();
        }
        if let Some(p) = val.get("file_path").and_then(|v| v.as_str()) {
            return p.to_string();
        }
        if let Some(p) = val.get("pattern").and_then(|v| v.as_str()) {
            return p.to_string();
        }
        for (_k, v) in val.as_object().into_iter().flatten() {
            if let Some(s) = v.as_str()
                && s.len() <= 80
            {
                return s.to_string();
            }
        }
    }
    tool_name.to_string()
}

/// Extract file_path from tool input JSON
pub(super) fn extract_file_path(json_str: &str) -> Option<String> {
    let val: serde_json::Value = serde_json::from_str(json_str).ok()?;
    val.get("file_path")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}
