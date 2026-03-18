use gtk4 as gtk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc;

use crate::agent_process::{AgentEvent, AgentProcess, AgentSpawnConfig, ImageAttachment, ProcessState};
use crate::agent_widgets;
use crate::session::{AgentConfig, ChatMessage};

const CONTEXT_WINDOW_MAX: u64 = 200_000;

struct ToolInfo {
    content_box: gtk::Box,
    spinner: gtk::Spinner,
    expander: gtk::Expander,
    tool_name: String,
    tool_input: String,
}

struct PanelState {
    process: AgentProcess,
    working_dir: std::path::PathBuf,
    is_dark: Rc<Cell<bool>>,
    current_text_label: Option<gtk::Label>,
    current_text: String,
    current_tool_name: Option<String>,
    current_tool_input: String,
    current_tool_use_id: Option<String>,
    pending_tools: HashMap<String, ToolInfo>,
    on_open_file: Rc<dyn Fn(&str)>,
    thinking_spinner: Option<gtk::Box>,
    tab_spinner: gtk::Spinner,
    chat_history: Rc<RefCell<Vec<ChatMessage>>>,
    agent_configs: Vec<AgentConfig>,
    selected_profile_idx: usize,
    session_id: Option<String>,
    on_session_id_change: Rc<dyn Fn(Option<String>)>,
    on_profile_change: Rc<dyn Fn(&str)>,
    // Token & cost tracking
    context_tokens: u64,
    total_cost_usd: f64,
    token_label: gtk::Label,
    cost_label: gtk::Label,
}

struct AttachedImage {
    bytes: Vec<u8>,
    mime_type: String,
    texture: gtk::gdk::Texture,
}

pub fn create_agent_panel(
    on_open_file: Rc<dyn Fn(&str)>,
    is_dark: Rc<Cell<bool>>,
    tab_spinner: gtk::Spinner,
    working_dir: &std::path::Path,
    title_text: &str,
    agent_configs: Vec<AgentConfig>,
    initial_profile: &str,
    resume_session_id: Option<String>,
    on_profile_change: Rc<dyn Fn(&str)>,
    on_session_id_change: Rc<dyn Fn(Option<String>)>,
    chat_history: Rc<RefCell<Vec<ChatMessage>>>,
) -> (gtk::Box, gtk::TextView) {
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.set_width_request(420);

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

    // Set initial selection
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

    // Chat history
    let message_list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    message_list.set_valign(gtk::Align::End);

    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .child(&message_list)
        .build();

    // Auto-scroll: when content grows, scroll to bottom (unless user scrolled up)
    {
        let adj = scrolled.vadjustment();
        let at_bottom = Rc::new(Cell::new(true));

        let flag = Rc::clone(&at_bottom);
        adj.connect_value_changed(move |adj| {
            flag.set(adj.value() >= adj.upper() - adj.page_size() - 20.0);
        });

        let flag = Rc::clone(&at_bottom);
        adj.connect_changed(move |adj| {
            if flag.get() {
                adj.set_value(adj.upper() - adj.page_size());
            }
        });
    }

    // Attachments preview bar (hidden when empty)
    let attach_bar = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    attach_bar.set_margin_start(8);
    attach_bar.set_margin_end(8);
    attach_bar.set_margin_top(2);
    attach_bar.set_margin_bottom(2);
    attach_bar.set_visible(false);

    let attachments: Rc<RefCell<Vec<AttachedImage>>> = Rc::new(RefCell::new(Vec::new()));

    // Input area: full-width text input (no side buttons)
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
        .max_content_height(120)
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

    // Left group: action buttons
    let attach_btn = gtk::Button::from_icon_name("mail-attachment-symbolic");
    attach_btn.set_tooltip_text(Some("Attach image (Ctrl+V to paste)"));
    attach_btn.set_has_frame(false);

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

    // Quick commands drop-up menu
    let quick_menu = gio::Menu::new();
    quick_menu.append(Some("Commit changes"), Some("panel.quick-commit"));
    quick_menu.append(Some("Create GitHub PR"), Some("panel.quick-pr"));
    quick_menu.append(Some("Update documentation"), Some("panel.quick-docs"));
    quick_menu.append(Some("Run lint, build, tests"), Some("panel.quick-test"));

    let quick_btn = gtk::MenuButton::new();
    quick_btn.set_icon_name("view-more-symbolic");
    quick_btn.set_tooltip_text(Some("Quick commands"));
    quick_btn.set_has_frame(false);
    quick_btn.set_direction(gtk::ArrowType::Up);
    quick_btn.set_menu_model(Some(&quick_menu));

    // Spacer pushes right group to end
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);

    // Right group: info labels + send
    let token_label = gtk::Label::new(Some("–"));
    token_label.set_tooltip_text(Some("Context window usage"));
    token_label.add_css_class("toolbar-info");

    let cost_label = gtk::Label::new(Some("$0.00"));
    cost_label.set_tooltip_text(Some("Session cost"));
    cost_label.add_css_class("toolbar-info");

    let send_btn = gtk::Button::from_icon_name("go-next-symbolic");
    send_btn.set_tooltip_text(Some("Send (Ctrl+Enter)"));
    send_btn.add_css_class("suggested-action");

    // Assemble toolbar
    toolbar.append(&attach_btn);
    toolbar.append(&pause_btn);
    toolbar.append(&stop_btn);
    toolbar.append(&compact_btn);
    toolbar.append(&quick_btn);
    toolbar.append(&spacer);
    toolbar.append(&token_label);
    toolbar.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    toolbar.append(&cost_label);
    toolbar.append(&send_btn);

    // Assemble panel
    panel.append(&header);
    panel.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    panel.append(&scrolled);
    panel.append(&attach_bar);
    panel.append(&input_frame);
    panel.append(&toolbar);

    // --- State ---
    let state = Rc::new(RefCell::new(PanelState {
        process: AgentProcess::new(),
        working_dir: working_dir.to_path_buf(),
        is_dark,
        current_text_label: None,
        current_text: String::new(),
        current_tool_name: None,
        current_tool_input: String::new(),
        current_tool_use_id: None,
        pending_tools: HashMap::new(),
        on_open_file,
        thinking_spinner: None,
        tab_spinner,
        chat_history,
        agent_configs,
        selected_profile_idx: initial_idx,
        session_id: resume_session_id,
        on_session_id_change,
        on_profile_change,
        context_tokens: 0,
        total_cost_usd: 0.0,
        token_label: token_label.clone(),
        cost_label: cost_label.clone(),
    }));

    // Render restored chat history
    {
        let s = state.borrow();
        let history = s.chat_history.borrow();
        let on_open = s.on_open_file.clone();
        let dark = s.is_dark.get();
        for msg in history.iter() {
            match msg {
                ChatMessage::User { text } => {
                    message_list.append(&agent_widgets::create_user_message(text));
                }
                ChatMessage::AssistantText { text } => {
                    let (container, label) = agent_widgets::create_assistant_text();
                    let cb = on_open.clone();
                    label.connect_activate_link(move |_label, uri| {
                        if let Some(path) = uri.strip_prefix("file://") {
                            cb(path);
                            glib::Propagation::Stop
                        } else {
                            glib::Propagation::Proceed
                        }
                    });
                    agent_widgets::update_assistant_text(&label, text, dark);
                    message_list.append(&container);
                }
                ChatMessage::ToolCall {
                    tool_name,
                    tool_input,
                    output,
                    is_error,
                } => {
                    let input_text = extract_tool_display(tool_name, tool_input);
                    let file_path = extract_file_path(tool_input);
                    let (container, content_box, spinner, expander) =
                        agent_widgets::create_tool_call(
                            tool_name,
                            &input_text,
                            file_path.as_deref(),
                            on_open.clone(),
                        );
                    agent_widgets::fill_tool_result(
                        &content_box, &spinner, &expander, output, *is_error, tool_name, tool_input,
                    );
                    message_list.append(&container);
                }
                ChatMessage::System { text } => {
                    message_list.append(&agent_widgets::create_system_message(text));
                }
            }
        }
        drop(history);
        drop(s);
        scroll_to_bottom(&scrolled);
    }

    // Dropdown selection change
    {
        let state = Rc::clone(&state);
        dropdown.connect_selected_notify(move |dd| {
            let idx = dd.selected() as usize;
            let mut s = state.borrow_mut();
            s.selected_profile_idx = idx;
            // Clear session_id when profile changes (new conversation)
            s.session_id = None;
            (s.on_session_id_change)(None);
            if let Some(cfg) = s.agent_configs.get(idx) {
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
                // Update dropdown model
                let names: Vec<&str> = new_configs.iter().map(|c| c.name.as_str()).collect();
                let new_model = gtk::StringList::new(&names);
                dropdown_clone.set_model(Some(&new_model));

                // Find the currently selected profile in the new list
                let mut s = state_clone.borrow_mut();
                let current_name = s
                    .agent_configs
                    .get(s.selected_profile_idx)
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                let new_idx = new_configs
                    .iter()
                    .position(|c| c.name == current_name)
                    .unwrap_or(0);

                s.agent_configs = new_configs;
                s.selected_profile_idx = new_idx;
                dropdown_clone.set_selected(new_idx as u32);
            });
        });
    }

    // --- Attach button: file chooser ---
    {
        let att = Rc::clone(&attachments);
        let bar = attach_bar.clone();
        attach_btn.connect_clicked(move |btn| {
            let filter = gtk::FileFilter::new();
            filter.set_name(Some("Images"));
            filter.add_mime_type("image/png");
            filter.add_mime_type("image/jpeg");
            filter.add_mime_type("image/gif");
            filter.add_mime_type("image/webp");

            let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
            filters.append(&filter);

            let dialog = gtk::FileDialog::builder()
                .title("Attach Image")
                .modal(true)
                .build();
            dialog.set_filters(Some(&filters));
            dialog.set_default_filter(Some(&filter));

            let window = btn.root().and_downcast::<gtk::Window>();
            let att = Rc::clone(&att);
            let bar = bar.clone();

            dialog.open(
                window.as_ref(),
                None::<&gtk::gio::Cancellable>,
                move |result| {
                    if let Ok(file) = result {
                        if let Some(path) = file.path() {
                            if let Ok(bytes) = std::fs::read(&path) {
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
                        }
                    }
                },
            );
        });
    }

    // --- Send handler ---
    let do_send = {
        let state = Rc::clone(&state);
        let message_list = message_list.clone();
        let scrolled = scrolled.clone();
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

            // Drain attached images
            let images: Vec<AttachedImage> = attachments.borrow_mut().drain(..).collect();
            rebuild_attach_bar(&attachments, &attach_bar);

            // Build user bubble
            let textures: Vec<gtk::gdk::Texture> =
                images.iter().map(|i| i.texture.clone()).collect();
            let user_widget = if textures.is_empty() {
                agent_widgets::create_user_message(&text)
            } else {
                agent_widgets::create_user_message_with_images(&text, &textures)
            };
            message_list.append(&user_widget);
            scroll_to_bottom(&scrolled);

            // Record in history
            if !text.is_empty() {
                state.borrow().chat_history.borrow_mut().push(ChatMessage::User {
                    text: text.clone(),
                });
            }

            // Build API image attachments
            let api_images: Vec<ImageAttachment> = images
                .into_iter()
                .map(|img| ImageAttachment {
                    bytes: img.bytes,
                    media_type: img.mime_type,
                })
                .collect();

            let mut s = state.borrow_mut();

            if !s.process.is_alive() {
                // Spawn new process with selected agent profile
                let (sender, receiver) = mpsc::channel::<AgentEvent>();
                let receiver = Rc::new(RefCell::new(Some(receiver)));

                let spawn_config = {
                    let cfg = s.agent_configs.get(s.selected_profile_idx);
                    AgentSpawnConfig {
                        system_prompt: cfg.map(|c| c.system_prompt.clone()),
                        allowed_tools: cfg.map(|c| c.allowed_tools.clone()).unwrap_or_default(),
                        model: cfg.and_then(|c| c.model.clone()),
                        resume_session_id: s.session_id.clone(),
                    }
                };

                let wd = s.working_dir.clone();
                if !s.process.spawn(sender, &wd, &spawn_config) {
                    let err = agent_widgets::create_system_message("Failed to start claude CLI");
                    message_list.append(&err);
                    return;
                }

                // Poll receiver on GTK main loop (~60fps)
                let state_rx = Rc::clone(&state);
                let ml = message_list.clone();
                let sc = scrolled.clone();
                let send_btn_rx = send_btn.clone();
                let pause_btn_rx = pause_btn.clone();
                let stop_btn_rx = stop_btn.clone();

                glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
                    let rx_ref = receiver.borrow();
                    let Some(ref rx) = *rx_ref else {
                        return glib::ControlFlow::Break;
                    };
                    while let Ok(event) = rx.try_recv() {
                        handle_event(
                            &state_rx,
                            &ml,
                            &sc,
                            &send_btn_rx,
                            &pause_btn_rx,
                            &stop_btn_rx,
                            event,
                        );
                    }
                    glib::ControlFlow::Continue
                });
            }

            s.process.send_message(&text, &api_images);

            // Show thinking spinner
            let thinking = agent_widgets::create_thinking_spinner();
            message_list.append(&thinking);
            scroll_to_bottom(&scrolled);
            s.thinking_spinner = Some(thinking);
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
                clipboard.read_texture_async(
                    None::<&gtk::gio::Cancellable>,
                    move |result| {
                        if let Ok(Some(texture)) = result {
                            let png_bytes = texture.save_to_png_bytes();
                            att.borrow_mut().push(AttachedImage {
                                bytes: png_bytes.to_vec(),
                                mime_type: "image/png".to_string(),
                                texture,
                            });
                            rebuild_attach_bar(&att, &bar);
                        }
                    },
                );
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
            match s.process.state {
                ProcessState::Running => {
                    s.process.pause();
                    btn.set_icon_name("media-playback-start-symbolic");
                    btn.set_tooltip_text(Some("Resume"));
                }
                ProcessState::Paused => {
                    s.process.resume();
                    btn.set_icon_name("media-playback-pause-symbolic");
                    btn.set_tooltip_text(Some("Pause"));
                }
                _ => {}
            }
        });
    }

    // Stop button
    {
        let state = Rc::clone(&state);
        let send_btn = send_btn.clone();
        let pause_btn = pause_btn.clone();
        let stop_btn_clone = stop_btn.clone();
        let message_list = message_list.clone();
        stop_btn_clone.connect_clicked(move |btn| {
            let mut s = state.borrow_mut();
            s.process.stop();
            remove_thinking_spinner(&mut s);
            s.tab_spinner.set_spinning(false);
            s.chat_history.borrow_mut().push(ChatMessage::System {
                text: "⏹ Stopped".to_string(),
            });
            drop(s);
            let info = agent_widgets::create_system_message("⏹ Stopped");
            message_list.append(&info);
            send_btn.set_sensitive(true);
            pause_btn.set_sensitive(false);
            pause_btn.set_icon_name("media-playback-pause-symbolic");
            pause_btn.set_tooltip_text(Some("Pause"));
            btn.set_sensitive(false);
        });
    }

    // Compact button: sends /compact to Claude CLI
    {
        let do_send_compact = do_send.clone();
        let input_view_compact = input_view.clone();
        compact_btn.connect_clicked(move |_| {
            input_view_compact.buffer().set_text("/compact");
            do_send_compact();
        });
    }

    // Quick command actions
    {
        let action_group = gio::SimpleActionGroup::new();
        let commands: &[(&str, &str)] = &[
            ("quick-commit", "commit all changes with a meaningful message"),
            ("quick-pr", "create a pull request for current branch"),
            ("quick-docs", "update documentation to reflect recent changes"),
            ("quick-test", "run lint, build, and tests, fix any errors"),
        ];
        for (action_name, prompt_text) in commands {
            let action = gio::SimpleAction::new(action_name, None);
            let iv = input_view.clone();
            let ds = do_send.clone();
            let text = prompt_text.to_string();
            action.connect_activate(move |_, _| {
                iv.buffer().set_text(&text);
                ds();
            });
            action_group.add_action(&action);
        }
        panel.insert_action_group("panel", Some(&action_group));
    }

    (panel, input_view)
}

fn rebuild_attach_bar(
    attachments: &Rc<RefCell<Vec<AttachedImage>>>,
    attach_bar: &gtk::Box,
) {
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

fn handle_event(
    state: &Rc<RefCell<PanelState>>,
    message_list: &gtk::Box,
    scrolled: &gtk::ScrolledWindow,
    send_btn: &gtk::Button,
    pause_btn: &gtk::Button,
    stop_btn: &gtk::Button,
    event: AgentEvent,
) {
    match event {
        AgentEvent::System { session_id, .. } => {
            // Capture session_id for continuation
            if let Some(ref id) = session_id {
                let mut s = state.borrow_mut();
                s.session_id = Some(id.clone());
                (s.on_session_id_change)(Some(id.clone()));
            }
        }

        AgentEvent::StreamEvent { event: ev } => {
            handle_stream_event(state, message_list, scrolled, &ev);
        }

        AgentEvent::User {
            tool_use_result: Some(ref result),
            ref message,
            ..
        } => {
            // tool_use_id is in message.content[0].tool_use_id, not in tool_use_result
            let tool_id = message
                .get("content")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.first())
                .and_then(|item| item.get("tool_use_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let mut s = state.borrow_mut();
            if let Some(info) = s.pending_tools.remove(&tool_id) {
                let output = if !result.stdout.is_empty() {
                    &result.stdout
                } else if let Some(ref c) = result.content {
                    c.as_str()
                } else {
                    &result.stderr
                };
                agent_widgets::fill_tool_result(
                    &info.content_box,
                    &info.spinner,
                    &info.expander,
                    output,
                    result.is_error,
                    &info.tool_name,
                    &info.tool_input,
                );
                // Record in history
                s.chat_history.borrow_mut().push(ChatMessage::ToolCall {
                    tool_name: info.tool_name.clone(),
                    tool_input: info.tool_input.clone(),
                    output: output.to_string(),
                    is_error: result.is_error,
                });
            }
            scroll_to_bottom(scrolled);
        }

        AgentEvent::Result {
            total_cost_usd,
            is_error,
            ..
        } => {
            let msg = if is_error {
                "✗ Error".to_string()
            } else {
                format!("✓ Done (${:.2})", total_cost_usd)
            };
            let info = agent_widgets::create_system_message(&msg);
            message_list.append(&info);

            send_btn.set_sensitive(true);
            pause_btn.set_sensitive(false);
            pause_btn.set_icon_name("media-playback-pause-symbolic");
            pause_btn.set_tooltip_text(Some("Pause"));
            stop_btn.set_sensitive(false);

            let mut s = state.borrow_mut();
            remove_thinking_spinner(&mut s);
            s.tab_spinner.set_spinning(false);

            // Update cost display
            s.total_cost_usd = total_cost_usd;
            s.cost_label.set_text(&format!("${:.2}", total_cost_usd));

            // Stop any tool spinners that never received a result
            let orphaned: Vec<ToolInfo> = s.pending_tools.drain().map(|(_, v)| v).collect();
            for ti in &orphaned {
                ti.spinner.set_spinning(false);
                ti.spinner.set_visible(false);
            }
            {
                let mut hist = s.chat_history.borrow_mut();
                for ti in orphaned {
                    hist.push(ChatMessage::ToolCall {
                        tool_name: ti.tool_name,
                        tool_input: ti.tool_input,
                        output: String::new(),
                        is_error: false,
                    });
                }
                hist.push(ChatMessage::System { text: msg.clone() });
            }
            s.current_text_label = None;
            s.current_text.clear();
            s.process.state = ProcessState::Idle;

            scroll_to_bottom(scrolled);
        }

        _ => {}
    }
}

fn remove_thinking_spinner(state: &mut PanelState) {
    if let Some(spinner) = state.thinking_spinner.take() {
        if let Some(parent) = spinner.parent() {
            if let Some(parent_box) = parent.downcast_ref::<gtk::Box>() {
                parent_box.remove(&spinner);
            }
        }
    }
}

fn handle_stream_event(
    state: &Rc<RefCell<PanelState>>,
    message_list: &gtk::Box,
    scrolled: &gtk::ScrolledWindow,
    ev: &crate::agent_process::StreamEventData,
) {
    match ev.event_type.as_str() {
        "message_start" => {
            // Extract token usage from message.usage.input_tokens
            if let Some(ref msg) = ev.message {
                if let Some(usage) = msg.get("usage") {
                    if let Some(input_tokens) = usage.get("input_tokens").and_then(|v| v.as_u64()) {
                        let mut s = state.borrow_mut();
                        s.context_tokens = input_tokens;
                        let pct = (input_tokens as f64 / CONTEXT_WINDOW_MAX as f64 * 100.0) as u32;
                        s.token_label.set_text(&format!("{} ({}%)", format_token_count(input_tokens), pct));
                    }
                }
            }
        }

        "message_delta" => {
            // Update token display with output_tokens added
            if let Some(ref usage) = ev.usage {
                if usage.output_tokens > 0 {
                    let s = state.borrow();
                    let total = s.context_tokens + usage.output_tokens;
                    let pct = (total as f64 / CONTEXT_WINDOW_MAX as f64 * 100.0) as u32;
                    s.token_label.set_text(&format!("{} ({}%)", format_token_count(total), pct));
                }
            }
        }

        "content_block_start" => {
            let mut s = state.borrow_mut();
            remove_thinking_spinner(&mut s);
            if let Some(ref cb) = ev.content_block {
                match cb.block_type.as_str() {
                    "text" => {
                        let (container, label) = agent_widgets::create_assistant_text();
                        let cb = s.on_open_file.clone();
                        label.connect_activate_link(move |_label, uri| {
                            if let Some(path) = uri.strip_prefix("file://") {
                                cb(path);
                                glib::Propagation::Stop
                            } else {
                                glib::Propagation::Proceed
                            }
                        });
                        message_list.append(&container);
                        s.current_text_label = Some(label);
                        s.current_text.clear();
                    }
                    "tool_use" => {
                        let name = cb.name.as_deref().unwrap_or("Tool");
                        s.current_tool_name = Some(name.to_string());
                        s.current_tool_input.clear();
                        s.current_tool_use_id = cb.id.clone();
                    }
                    _ => {}
                }
            }
        }

        "content_block_delta" => {
            let mut s = state.borrow_mut();
            if let Some(ref delta) = ev.delta {
                match delta.delta_type.as_deref() {
                    Some("text_delta") => {
                        if let Some(ref text) = delta.text {
                            s.current_text.push_str(text);
                            if let Some(ref label) = s.current_text_label {
                                agent_widgets::update_assistant_text(label, &s.current_text, s.is_dark.get());
                            }
                        }
                    }
                    Some("input_json_delta") => {
                        if let Some(ref json) = delta.partial_json {
                            s.current_tool_input.push_str(json);
                        }
                    }
                    _ => {}
                }
            }
            drop(s);
            scroll_to_bottom(scrolled);
        }

        "content_block_stop" => {
            let mut s = state.borrow_mut();
            // If we were building a tool call, finalize it
            if let Some(tool_name) = s.current_tool_name.take() {
                let input_text = extract_tool_display(&tool_name, &s.current_tool_input);
                let file_path = extract_file_path(&s.current_tool_input);
                let on_open = s.on_open_file.clone();
                let (container, content_box, spinner, expander) = agent_widgets::create_tool_call(
                    &tool_name,
                    &input_text,
                    file_path.as_deref(),
                    on_open,
                );
                message_list.append(&container);

                let tool_input_snapshot = s.current_tool_input.clone();
                if let Some(id) = s.current_tool_use_id.take() {
                    s.pending_tools.insert(
                        id,
                        ToolInfo {
                            content_box,
                            spinner,
                            expander,
                            tool_name,
                            tool_input: tool_input_snapshot,
                        },
                    );
                }
                s.current_tool_input.clear();
            } else if !s.current_text.is_empty() {
                // Text block completed — record in history
                s.chat_history.borrow_mut().push(ChatMessage::AssistantText {
                    text: s.current_text.clone(),
                });
            }
            // Clear text tracking (block is done)
            s.current_text_label = None;
            s.current_text.clear();
            drop(s);
            scroll_to_bottom(scrolled);
        }

        _ => {}
    }
}

/// Format token count as human-readable string (e.g., "25K", "1.2M", "500")
fn format_token_count(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{}K", tokens / 1_000)
    } else {
        format!("{}", tokens)
    }
}

/// Extract a short display string from tool input JSON
fn extract_tool_display(tool_name: &str, json_str: &str) -> String {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
        // Common patterns
        if let Some(cmd) = val.get("command").and_then(|v| v.as_str()) {
            return cmd.to_string();
        }
        if let Some(q) = val.get("pattern").and_then(|v| v.as_str()) {
            let path = val
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            return format!("{q} {path}").trim().to_string();
        }
        if let Some(p) = val.get("file_path").and_then(|v| v.as_str()) {
            return p.to_string();
        }
        if let Some(p) = val.get("pattern").and_then(|v| v.as_str()) {
            return p.to_string();
        }
        // Fallback: first string value
        for (_k, v) in val.as_object().into_iter().flatten() {
            if let Some(s) = v.as_str() {
                if s.len() <= 80 {
                    return s.to_string();
                }
            }
        }
    }
    tool_name.to_string()
}

/// Extract file_path from tool input JSON (used by Read, Write, Edit, etc.)
fn extract_file_path(json_str: &str) -> Option<String> {
    let val: serde_json::Value = serde_json::from_str(json_str).ok()?;
    val.get("file_path")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn scroll_to_bottom(scrolled: &gtk::ScrolledWindow) {
    let adj = scrolled.vadjustment();
    adj.set_value(adj.upper() - adj.page_size());
}
