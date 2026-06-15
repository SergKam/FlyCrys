mod chat_factory;
mod event_handler;
mod slash_popover;
pub(crate) mod state;

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
use crate::services::cli::{
    AgentBackend, AgentDomainEvent, AgentSpawnConfig, ImageAttachment, ModelInfo,
};

use state::{
    AgentProcessState, BackgroundTaskResultCb, ChatState, PanelConfig, PanelState, TaskCompletedCb,
    TokenState,
};

/// How many history entries to show initially / per "Load previous" click.
const PAGE_SIZE: usize = 100;

/// How many history messages to render per idle tick when first populating the
/// chat. Spreading the work lets GTK interleave redraws/input between batches so
/// the panel stays responsive instead of freezing while a long history renders.
const HISTORY_RENDER_CHUNK: usize = 20;

struct AttachedImage {
    bytes: Vec<u8>,
    mime_type: String,
    texture: gtk::gdk::Texture,
}

/// What [`create_agent_panel`] hands back: the panel widget, the input text view,
/// a theme-change callback, a history-hydration callback (called once the
/// persisted chat history has been parsed off-thread), and a model-list setter
/// (called once the CLI's model list has been probed off-thread).
type AgentPanelParts = (
    gtk::Box,
    gtk::TextView,
    Rc<dyn Fn(bool)>,
    Rc<dyn Fn(Vec<ChatMessage>)>,
    Rc<dyn Fn(Vec<ModelInfo>)>,
);

#[allow(clippy::too_many_arguments)]
pub fn create_agent_panel(
    on_open_file: Rc<dyn Fn(&str)>,
    theme: Rc<Cell<Theme>>,
    notification_level: Rc<Cell<NotificationLevel>>,
    tab_spinner: gtk::Spinner,
    working_dir: &std::path::Path,
    workspace_label: Rc<dyn Fn() -> String>,
    agent_configs: Vec<AgentConfig>,
    initial_profile: &str,
    resume_session_id: Option<String>,
    fork_session: bool,
    on_profile_change: Rc<dyn Fn(&str)>,
    on_session_id_change: Rc<dyn Fn(Option<String>)>,
    chat_history: Rc<RefCell<Vec<ChatMessage>>>,
    on_tool_result: Option<Rc<dyn Fn()>>,
    on_background_task: Option<Rc<dyn Fn(String, String)>>,
    on_background_task_result: BackgroundTaskResultCb,
    on_task_completed: TaskCompletedCb,
    // External labels living in the workspace status bar — the panel updates their text.
    token_label: gtk::Label,
    cost_label: gtk::Label,
    agent_name_label: gtk::Label,
    // Status-bar label showing the effective model · effort.
    model_status_label: gtk::Label,
    // Persisted session model override (CLI model `value`) and effort, plus a
    // callback to persist changes to the workspace config.
    initial_model: Option<String>,
    initial_effort: Option<String>,
    on_model_effort_change: Rc<dyn Fn(Option<String>, Option<String>)>,
) -> AgentPanelParts {
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.set_width_request(AGENT_PANEL_MIN_WIDTH);

    // Set initial agent name in status bar
    if let Some(cfg) = agent_configs.get(
        agent_configs
            .iter()
            .position(|c| c.name == initial_profile)
            .unwrap_or(0),
    ) {
        agent_name_label.set_text(&cfg.name);
    }

    // Agent profile menu button (in toolbar, created here for reference)
    let agent_menu = gio::Menu::new();
    let agent_btn = gtk::MenuButton::new();
    agent_btn.set_icon_name("system-users-symbolic");
    agent_btn.set_has_frame(false);
    agent_btn.set_direction(gtk::ArrowType::Up);
    agent_btn.set_menu_model(Some(&agent_menu));

    let initial_idx = agent_configs
        .iter()
        .position(|c| c.name == initial_profile)
        .unwrap_or(0);

    // Selected agent index — shared between menu rebuild and action handlers.
    let selected_agent_idx: Rc<Cell<usize>> = Rc::new(Cell::new(initial_idx));

    if let Some(cfg) = agent_configs.get(initial_idx) {
        agent_btn.set_tooltip_text(Some(&format!("Agent: {}", cfg.name)));
    }
    let agent_name_status = agent_name_label.clone();

    fn rebuild_agent_menu(menu: &gio::Menu, configs: &[AgentConfig], selected: usize) {
        menu.remove_all();
        let agents_section = gio::Menu::new();
        for (i, cfg) in configs.iter().enumerate() {
            let label = if i == selected {
                format!("\u{2714} {}", cfg.name)
            } else {
                format!("   {}", cfg.name)
            };
            agents_section.append(Some(&label), Some(&format!("panel.agent-select-{i}")));
        }
        menu.append_section(None, &agents_section);

        let config_section = gio::Menu::new();
        config_section.append(Some("Configure\u{2026}"), Some("panel.agent-configure"));
        menu.append_section(None, &config_section);
    }
    rebuild_agent_menu(&agent_menu, &agent_configs, initial_idx);

    // --- Model & effort switcher (menu populated from the CLI's model list) ---
    // The list and each model's valid effort levels are fetched at runtime via
    // `set_models` (returned below); nothing is hardcoded.
    let model_menu = gio::Menu::new();
    let model_btn = gtk::MenuButton::new();
    model_btn.set_icon_name("preferences-system-symbolic");
    model_btn.set_has_frame(false);
    model_btn.set_direction(gtk::ArrowType::Up);
    model_btn.set_tooltip_text(Some("Model & effort"));
    model_btn.set_menu_model(Some(&model_menu));
    rebuild_model_menu(
        &model_menu,
        &[],
        initial_model.as_deref(),
        initial_effort.as_deref(),
    );
    model_status_label.set_text(&model_effort_status(
        &[],
        initial_model.as_deref(),
        initial_effort.as_deref(),
    ));

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

    let slash_btn = gtk::Button::with_label("/");
    slash_btn.set_tooltip_text(Some("Slash commands"));
    slash_btn.set_has_frame(false);

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
        config_section.append(Some("Configure\u{2026}"), Some("panel.bookmark-configure"));
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
    toolbar.append(&slash_btn);
    toolbar.append(&quick_btn);
    toolbar.append(&agent_btn);
    toolbar.append(&model_btn);
    toolbar.append(&spacer);
    toolbar.append(&send_btn);

    panel.append(&webview_widget);
    panel.append(&attach_bar);
    panel.append(&input_frame);
    panel.append(&toolbar);

    // --- State ---
    // History starts empty; it is parsed off-thread by the caller and handed to
    // `load_history` (returned below) once ready, so the panel appears instantly.
    let state = Rc::new(RefCell::new(PanelState {
        process: AgentProcessState {
            process: ClaudeBackend::new(),
            session_id: resume_session_id,
            fork_session,
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
            oldest_rendered_idx: 0, // populated by load_history once parsed
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
            model_override: initial_model,
            effort: initial_effort,
            models: Vec::new(),
        },
        model_status_label: model_status_label.clone(),
        tab_spinner,
        workspace_label,
        on_open_file,
        on_session_id_change,
        on_profile_change,
        on_model_effort_change,
        on_tool_result,
        on_background_task,
        on_background_task_result,
        on_task_completed,
        pending_background_tasks: std::collections::HashSet::new(),
    }));

    // --- Asynchronous history hydration ---
    // The persisted chat history is parsed off the main thread by the caller and
    // handed to this closure, which populates the shared buffer and renders the
    // first page in chunks across idle ticks (so a long history streams in
    // instead of freezing the panel on tab open).
    let load_history: Rc<dyn Fn(Vec<ChatMessage>)> = {
        let state = Rc::clone(&state);
        Rc::new(move |history: Vec<ChatMessage>| {
            let total = history.len();
            let page_start = total.saturating_sub(PAGE_SIZE);
            {
                let mut s = state.borrow_mut();
                *s.chat.chat_history.borrow_mut() = history;
                // We commit to rendering page_start..total, so the oldest rendered
                // index is page_start from the outset (keeps "Load previous"
                // correct even while the first page is still streaming in).
                s.chat.oldest_rendered_idx = page_start;
                if page_start > 0 {
                    s.chat.webview.show_load_prev_button();
                }
            }

            let state_render = Rc::clone(&state);
            let mut cursor = page_start;
            glib::idle_add_local(move || {
                let s = state_render.borrow();
                let history = s.chat.chat_history.borrow();
                let total = history.len();
                let dark = s.config.theme.get().is_dark();

                let end = (cursor + HISTORY_RENDER_CHUNK).min(total);
                for i in cursor..end {
                    chat_factory::render_history_message(&s.chat.webview, &history[i], dark);
                }
                cursor = end;

                if cursor >= total {
                    glib::ControlFlow::Break
                } else {
                    glib::ControlFlow::Continue
                }
            });
        })
    };

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

    // --- AskUserQuestion answers via flycrys://answer-question ---
    {
        let state_ans = Rc::clone(&state);
        state.borrow().chat.webview.set_on_answer_question(Rc::new(
            move |request_id: String, data_json: String| {
                let updated_input: serde_json::Value =
                    serde_json::from_str(&data_json).unwrap_or(serde_json::Value::Null);
                let mut s = state_ans.borrow_mut();
                let _ = s
                    .process
                    .process
                    .answer_question(&request_id, updated_input);
            },
        ));
    }

    // --- AskUserQuestion "none of these" via flycrys://reject-question ---
    {
        let state_rej = Rc::clone(&state);
        state
            .borrow()
            .chat
            .webview
            .set_on_reject_question(Rc::new(move |request_id: String| {
                let mut s = state_rej.borrow_mut();
                let _ = s.process.process.reject_question(&request_id);
            }));
    }

    // (Agent selection and configure actions are registered in the action group below.)

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
                s.chat.webview.append_user_message(&text, &data_uris, true);
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
                        // Session override (toolbar switcher) wins; else the
                        // agent profile's model; else the CLI default.
                        model: s
                            .config
                            .model_override
                            .clone()
                            .or_else(|| cfg.and_then(|c| c.model.clone())),
                        effort: s.config.effort.clone(),
                        resume_session_id: s.process.session_id.clone(),
                        fork_session: s.process.fork_session,
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

    // --- Slash commands popover ---
    let slash_commands = crate::services::skills::discover_slash_commands(working_dir);
    let slash_popover = {
        let iv = input_view.clone();
        slash_popover::SlashPopover::new(input_frame.upcast_ref(), slash_commands, move |cmd| {
            let text = format!("/{} ", cmd.name);
            iv.buffer().set_text(&text);
            let end = iv.buffer().end_iter();
            iv.buffer().place_cursor(&end);
            iv.grab_focus();
        })
    };

    // Wire "Configure…" to open the skills dialog
    {
        let sp = Rc::clone(&slash_popover);
        let wd = working_dir.to_path_buf();
        let input_frame_ref = input_frame.clone();
        slash_popover.set_on_configure(move || {
            let win = input_frame_ref
                .root()
                .and_then(|r| r.downcast::<gtk::Window>().ok());
            if let Some(win) = win.as_ref() {
                let sp2 = Rc::clone(&sp);
                let wd2 = wd.clone();
                crate::skills_dialog::show(win, &wd, move || {
                    let cmds = crate::services::skills::discover_slash_commands(&wd2);
                    sp2.reload(cmds);
                });
            }
        });
    }

    // "/" button opens the full slash command list
    {
        let sp = Rc::clone(&slash_popover);
        let iv = input_view.clone();
        slash_btn.connect_clicked(move |_| {
            sp.update_filter("");
            sp.show();
            iv.grab_focus();
        });
    }

    // Monitor text changes to detect "/" prefix
    {
        let sp = Rc::clone(&slash_popover);
        let buf = input_view.buffer();
        buf.connect_changed(move |buf| {
            let text = buf.text(&buf.start_iter(), &buf.end_iter(), false);
            let text = text.as_str();
            if text.starts_with('/') && !text.contains('\n') {
                let query = &text[1..];
                if !query.contains(' ') {
                    sp.update_filter(query);
                    sp.show();
                } else {
                    sp.hide();
                }
            } else {
                sp.hide();
            }
        });
    }

    // Ctrl+Enter to send, Ctrl+V to paste image from clipboard
    let key_ctrl = gtk::EventControllerKey::new();
    let do_send_key = do_send.clone();
    let att_key = Rc::clone(&attachments);
    let bar_key = attach_bar.clone();
    let input_for_key = input_view.clone();
    let sp_key = Rc::clone(&slash_popover);
    let wd_for_rescan = working_dir.to_path_buf();
    key_ctrl.connect_key_pressed(move |_, key, _, modifiers| {
        let is_ctrl = modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK);

        // Slash popover keyboard navigation
        if sp_key.is_visible() {
            match key {
                gtk::gdk::Key::Down => {
                    sp_key.select_next();
                    return glib::Propagation::Stop;
                }
                gtk::gdk::Key::Up => {
                    sp_key.select_prev();
                    return glib::Propagation::Stop;
                }
                gtk::gdk::Key::Tab | gtk::gdk::Key::Return if !is_ctrl => {
                    // Check if selected command is /rescan-skills
                    let buf = input_for_key.buffer();
                    sp_key.activate_selected();
                    let new_text = buf.text(&buf.start_iter(), &buf.end_iter(), false);
                    if new_text.trim() == "/rescan-skills" {
                        let cmds = crate::services::skills::discover_slash_commands(&wd_for_rescan);
                        sp_key.reload(cmds);
                        buf.set_text("");
                    }
                    return glib::Propagation::Stop;
                }
                gtk::gdk::Key::Escape => {
                    sp_key.hide();
                    return glib::Propagation::Stop;
                }
                _ => {}
            }
        }

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

        // Agent selection actions (agent-select-0, agent-select-1, …)
        {
            let cfgs = state.borrow().config.agent_configs.clone();
            for i in 0..cfgs.len() {
                let action = gio::SimpleAction::new(&format!("agent-select-{i}"), None);
                let state_ref = Rc::clone(&state);
                let sel = Rc::clone(&selected_agent_idx);
                let am = agent_menu.clone();
                let ab = agent_btn.clone();
                let anl = agent_name_status.clone();
                action.connect_activate(move |_, _| {
                    sel.set(i);
                    let mut s = state_ref.borrow_mut();
                    s.config.selected_profile_idx = i;
                    s.process.session_id = None;
                    (s.on_session_id_change)(None);
                    if let Some(cfg) = s.config.agent_configs.get(i) {
                        (s.on_profile_change)(&cfg.name);
                        ab.set_tooltip_text(Some(&format!("Agent: {}", cfg.name)));
                        anl.set_text(&cfg.name);
                    }
                    rebuild_agent_menu(&am, &s.config.agent_configs, i);
                });
                action_group.add_action(&action);
            }
        }

        // Model + effort selection. Parameterized string actions (target = the
        // model `value` / effort level) because the list and per-model effort
        // levels are fetched at runtime, so we can't pre-register N actions.
        {
            let model_action =
                gio::SimpleAction::new("model-select", Some(glib::VariantTy::STRING));
            let state_ref = Rc::clone(&state);
            let mm = model_menu.clone();
            model_action.connect_activate(move |_, param| {
                let Some(value) = param.and_then(|p| p.get::<String>()) else {
                    return;
                };
                {
                    let mut s = state_ref.borrow_mut();
                    s.config.model_override = Some(value.clone());
                    // Drop an effort the newly selected model doesn't support.
                    let keep_effort = s
                        .config
                        .models
                        .iter()
                        .find(|m| m.value == value)
                        .zip(s.config.effort.clone())
                        .is_some_and(|(m, e)| {
                            m.supports_effort && m.supported_effort_levels.contains(&e)
                        });
                    if !keep_effort {
                        s.config.effort = None;
                    }
                }
                apply_model_effort(&state_ref, &mm);
            });
            action_group.add_action(&model_action);

            let effort_action =
                gio::SimpleAction::new("effort-select", Some(glib::VariantTy::STRING));
            let state_ref = Rc::clone(&state);
            let mm = model_menu.clone();
            effort_action.connect_activate(move |_, param| {
                let level = param.and_then(|p| p.get::<String>()).unwrap_or_default();
                {
                    let mut s = state_ref.borrow_mut();
                    s.config.effort = if level.is_empty() { None } else { Some(level) };
                }
                apply_model_effort(&state_ref, &mm);
            });
            action_group.add_action(&effort_action);
        }

        // Agent configure action
        let agent_configure = gio::SimpleAction::new("agent-configure", None);
        {
            let panel_ref = panel.clone();
            let state_ref = Rc::clone(&state);
            let am = agent_menu.clone();
            let ab = agent_btn.clone();
            let sel = Rc::clone(&selected_agent_idx);
            let ag = action_group.clone();
            let anl_outer = agent_name_status.clone();
            agent_configure.connect_activate(move |_, _| {
                let win = panel_ref
                    .root()
                    .and_then(|r| r.downcast::<gtk::Window>().ok());
                let Some(win) = win.as_ref() else { return };

                let state_c = Rc::clone(&state_ref);
                let am = am.clone();
                let ab = ab.clone();
                let sel = sel.clone();
                let ag = ag.clone();
                let anl_cfg = anl_outer.clone();
                crate::agent_config_dialog::show(win, move |new_configs| {
                    let mut s = state_c.borrow_mut();
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

                    // Remove old agent-select-N actions
                    for j in 0..200 {
                        let name = format!("agent-select-{j}");
                        if ag.lookup_action(&name).is_some() {
                            ag.remove_action(&name);
                        } else {
                            break;
                        }
                    }

                    // Re-register with new configs
                    for (i, _cfg) in new_configs.iter().enumerate() {
                        let action = gio::SimpleAction::new(&format!("agent-select-{i}"), None);
                        let state_inner = Rc::clone(&state_c);
                        let sel_inner = sel.clone();
                        let am_inner = am.clone();
                        let ab_inner = ab.clone();
                        let anl_inner = anl_cfg.clone();
                        action.connect_activate(move |_, _| {
                            sel_inner.set(i);
                            let mut s = state_inner.borrow_mut();
                            s.config.selected_profile_idx = i;
                            s.process.session_id = None;
                            (s.on_session_id_change)(None);
                            if let Some(cfg) = s.config.agent_configs.get(i) {
                                (s.on_profile_change)(&cfg.name);
                                ab_inner.set_tooltip_text(Some(&format!("Agent: {}", cfg.name)));
                                anl_inner.set_text(&cfg.name);
                            }
                            rebuild_agent_menu(&am_inner, &s.config.agent_configs, i);
                        });
                        ag.add_action(&action);
                    }

                    s.config.agent_configs = new_configs;
                    s.config.selected_profile_idx = new_idx;
                    sel.set(new_idx);
                    rebuild_agent_menu(&am, &s.config.agent_configs, new_idx);
                    if let Some(cfg) = s.config.agent_configs.get(new_idx) {
                        ab.set_tooltip_text(Some(&format!("Agent: {}", cfg.name)));
                        anl_cfg.set_text(&cfg.name);
                    }
                });
            });
        }
        action_group.add_action(&agent_configure);

        panel.insert_action_group("panel", Some(&action_group));
    }

    // Theme-change callback so the workspace can re-theme the chat WebView.
    let on_theme_change: Rc<dyn Fn(bool)> = {
        let state = Rc::clone(&state);
        Rc::new(move |is_dark: bool| {
            state.borrow().chat.webview.set_theme(is_dark);
        })
    };

    // Model-list setter — called once the CLI model list has been probed
    // off-thread. Populates the switcher and self-heals a persisted choice that
    // is no longer offered (e.g. a renamed model, or an effort the now-known
    // model doesn't support), persisting any correction.
    let set_models: Rc<dyn Fn(Vec<ModelInfo>)> = {
        let state = Rc::clone(&state);
        let model_menu = model_menu.clone();
        Rc::new(move |models: Vec<ModelInfo>| {
            {
                let mut s = state.borrow_mut();
                s.config.models = models;
                // Drop an override no longer in the list.
                if let Some(v) = s.config.model_override.clone()
                    && !s.config.models.iter().any(|m| m.value == v)
                {
                    s.config.model_override = None;
                }
                // Keep effort only if the effective model supports it.
                let keep_effort = match s.config.effort.clone() {
                    Some(e) => {
                        effective_model_value(&s.config.models, s.config.model_override.as_deref())
                            .and_then(|v| s.config.models.iter().find(|m| m.value == v))
                            .is_some_and(|m| {
                                m.supports_effort && m.supported_effort_levels.contains(&e)
                            })
                    }
                    None => true,
                };
                if !keep_effort {
                    s.config.effort = None;
                }
            }
            apply_model_effort(&state, &model_menu);
        })
    };

    (panel, input_view, on_theme_change, load_history, set_models)
}

/// Persist the chosen model/effort to the workspace config, then refresh the
/// switcher menu and status label from current state.
fn apply_model_effort(state: &Rc<RefCell<PanelState>>, menu: &gio::Menu) {
    let (cb, model, effort) = {
        let s = state.borrow();
        (
            Rc::clone(&s.on_model_effort_change),
            s.config.model_override.clone(),
            s.config.effort.clone(),
        )
    };
    cb(model, effort);
    let s = state.borrow();
    rebuild_model_menu(
        menu,
        &s.config.models,
        s.config.model_override.as_deref(),
        s.config.effort.as_deref(),
    );
    update_model_status(&s);
}

/// Set the model status-bar label from current state (model · effort).
fn update_model_status(s: &PanelState) {
    s.model_status_label.set_text(&model_effort_status(
        &s.config.models,
        s.config.model_override.as_deref(),
        s.config.effort.as_deref(),
    ));
}

/// The model whose effort levels / checkmark apply: the explicit override (if
/// still offered), else the CLI `default` entry, else the first model. Lets the
/// effort section and a check appear even when nothing is explicitly picked.
fn effective_model_value<'a>(
    models: &'a [ModelInfo],
    model_override: Option<&str>,
) -> Option<&'a str> {
    // Prefer the explicit override if it's still offered; resolve every result
    // to a string borrowed from `models` (not from `model_override`).
    if let Some(v) = model_override
        && let Some(m) = models.iter().find(|m| m.value == v)
    {
        return Some(m.value.as_str());
    }
    models
        .iter()
        .find(|m| m.value == "default")
        .or_else(|| models.first())
        .map(|m| m.value.as_str())
}

/// A concise model name from the CLI's own description (e.g. "Opus 4.8" from
/// "Opus 4.8 with 1M context · Best for everyday, complex tasks"), so the actual
/// model is visible before the first message. Falls back to display name, then
/// the raw value. Reads CLI-provided text — does not transform model ids.
fn model_label_text(m: &ModelInfo) -> String {
    let lead = m.description.split('\u{00b7}').next().unwrap_or("").trim();
    let lead = lead.split(" with ").next().unwrap_or(lead).trim();
    if !lead.is_empty() {
        lead.to_string()
    } else if !m.display_name.is_empty() {
        m.display_name.clone()
    } else {
        m.value.clone()
    }
}

/// Rebuild the switcher menu: a Model section (the fetched list) and an Effort
/// section (the effective model's `supported_effort_levels`, plus "Default").
/// The effective model is checked. Empty list → a "Fetching…" hint.
fn rebuild_model_menu(
    menu: &gio::Menu,
    models: &[ModelInfo],
    model_override: Option<&str>,
    effort: Option<&str>,
) {
    use gtk::glib::prelude::ToVariant;
    menu.remove_all();

    let effective = effective_model_value(models, model_override);

    let model_section = gio::Menu::new();
    if models.is_empty() {
        model_section.append_item(&gio::MenuItem::new(Some("Fetching models\u{2026}"), None));
    } else {
        for m in models {
            let version = model_label_text(m);
            let display = if !version.is_empty() && version != m.display_name {
                format!("{} \u{00b7} {}", m.display_name, version)
            } else if !m.display_name.is_empty() {
                m.display_name.clone()
            } else {
                version
            };
            let label = if effective == Some(m.value.as_str()) {
                format!("\u{2714} {}", display)
            } else {
                format!("   {}", display)
            };
            let item = gio::MenuItem::new(Some(&label), None);
            item.set_action_and_target_value(
                Some("panel.model-select"),
                Some(&m.value.to_variant()),
            );
            model_section.append_item(&item);
        }
    }
    menu.append_section(Some("Model"), &model_section);

    // Effort levels are model-specific; show the section for the effective
    // model when it supports effort.
    let levels: &[String] = effective
        .and_then(|v| models.iter().find(|m| m.value == v))
        .filter(|m| m.supports_effort)
        .map(|m| m.supported_effort_levels.as_slice())
        .unwrap_or(&[]);
    if !levels.is_empty() {
        let effort_section = gio::Menu::new();
        let default_label = if effort.is_none() {
            "\u{2714} Default"
        } else {
            "   Default"
        };
        let default_item = gio::MenuItem::new(Some(default_label), None);
        default_item
            .set_action_and_target_value(Some("panel.effort-select"), Some(&"".to_variant()));
        effort_section.append_item(&default_item);
        for lvl in levels {
            let label = if effort == Some(lvl.as_str()) {
                format!("\u{2714} {lvl}")
            } else {
                format!("   {lvl}")
            };
            let item = gio::MenuItem::new(Some(&label), None);
            item.set_action_and_target_value(Some("panel.effort-select"), Some(&lvl.to_variant()));
            effort_section.append_item(&item);
        }
        menu.append_section(Some("Effort"), &effort_section);
    }
}

/// Status-bar text "model · effort", e.g. "Opus 4.8 · high". The model name is
/// the effective model's CLI description (known before the first message);
/// effort shows the chosen level or "default" when unset. Both are visible up
/// front without running a turn.
fn model_effort_status(
    models: &[ModelInfo],
    model_override: Option<&str>,
    effort: Option<&str>,
) -> String {
    let model_name = match effective_model_value(models, model_override) {
        Some(v) => models
            .iter()
            .find(|m| m.value == v)
            .map(model_label_text)
            .unwrap_or_else(|| v.to_string()),
        None => model_override.unwrap_or("Default").to_string(),
    };
    format!("{model_name} \u{00b7} {}", effort.unwrap_or("default"))
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
