use gtk::glib;
use gtk4 as gtk;
use std::cell::{Cell, RefCell};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::rc::Rc;
use vte4::prelude::*;

use crate::config::constants::{TERMINAL_FONT, TERMINAL_SCROLLBACK_LINES};
use crate::config::types::Theme;
use crate::models::workspace_config::{RunTabConfig, RunTabType};
use crate::services::storage;
use crate::terminal;

// ── Task status ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum TaskStatus {
    Running,
    Done,
    Failed,
}

impl TaskStatus {
    fn indicator(self) -> &'static str {
        match self {
            TaskStatus::Running => "\u{23f3}", // ⏳
            TaskStatus::Done => "\u{2713}",    // ✓
            TaskStatus::Failed => "\u{2717}",  // ✗
        }
    }

    fn css_class(self) -> &'static str {
        match self {
            TaskStatus::Running => "task-running",
            TaskStatus::Done => "task-done",
            TaskStatus::Failed => "task-failed",
        }
    }
}

// ── Per-tab kind-specific state ─────────────────────────────────────────────

enum TabKind {
    Shell {
        terminal: RefCell<Option<vte4::Terminal>>,
        has_child: Rc<Cell<bool>>,
    },
    Task {
        tool_id: String,
        text_view: RefCell<Option<gtk::TextView>>,
        output_file: RefCell<Option<PathBuf>>,
        status: Cell<TaskStatus>,
        status_label: gtk::Label,
        file_offset: Cell<u64>,
    },
}

// ── Per-tab common state ────────────────────────────────────────────────────

struct RunTabSlot {
    id: String,
    name: Rc<RefCell<String>>,
    content_box: gtk::Box,
    kind: TabKind,
    dirty: Rc<Cell<bool>>,
    materialized: Cell<bool>,
}

// ── RunPanel (Rc wrapper for cheap cloning into closures) ───────────────────

#[derive(Clone)]
pub struct RunPanel {
    inner: Rc<RunPanelInner>,
}

struct RunPanelInner {
    container: gtk::Box,
    notebook: gtk::Notebook,
    tabs: RefCell<Vec<RunTabSlot>>,
    next_bash_number: Cell<u32>,
    working_directory: PathBuf,
    workspace_id: String,
    theme: Rc<Cell<Theme>>,
    on_add_to_chat: Rc<dyn Fn(String)>,
}

impl RunPanel {
    pub fn new(
        config: &crate::session::WorkspaceConfig,
        theme: Rc<Cell<Theme>>,
        on_add_to_chat: Rc<dyn Fn(String)>,
    ) -> Self {
        let notebook = gtk::Notebook::new();
        notebook.set_scrollable(true);
        notebook.set_show_border(false);
        notebook.set_show_tabs(true);
        notebook.add_css_class("run-panel-notebook");

        let mut max_num = 0u32;
        for cfg in &config.run_tabs {
            if let Some(n) = parse_bash_number(&cfg.name) {
                max_num = max_num.max(n);
            }
        }

        let inner = Rc::new(RunPanelInner {
            container: gtk::Box::new(gtk::Orientation::Vertical, 0),
            notebook,
            tabs: RefCell::new(Vec::new()),
            next_bash_number: Cell::new(max_num + 1),
            working_directory: PathBuf::from(&config.working_directory),
            workspace_id: config.id.clone(),
            theme,
            on_add_to_chat,
        });

        let panel = RunPanel { inner };

        let tab_configs = if config.run_tabs.is_empty() {
            vec![RunTabConfig {
                id: uuid::Uuid::new_v4().to_string(),
                name: "bash(1)".to_string(),
                tab_type: RunTabType::Shell,
            }]
        } else {
            config.run_tabs.clone()
        };

        let active_idx = config
            .active_run_tab
            .min(tab_configs.len().saturating_sub(1));

        for cfg in &tab_configs {
            panel.add_tab_from_config(cfg);
        }

        panel.materialize_tab(active_idx);

        // Lazy materialization on tab switch
        {
            let p = panel.clone();
            panel
                .inner
                .notebook
                .connect_switch_page(move |_nb, _page, page_num| {
                    p.materialize_tab(page_num as usize);
                });
        }

        // [+] button
        let add_btn = gtk::Button::from_icon_name("list-add-symbolic");
        add_btn.set_has_frame(false);
        add_btn.set_tooltip_text(Some("New terminal tab"));
        {
            let p = panel.clone();
            add_btn.connect_clicked(move |_| {
                let idx = p.add_shell_tab();
                p.inner.notebook.set_current_page(Some(idx as u32));
            });
        }
        panel
            .inner
            .notebook
            .set_action_widget(&add_btn, gtk::PackType::End);

        panel.inner.container.set_visible(config.terminal_visible);
        panel.inner.container.append(&panel.inner.notebook);
        panel.inner.container.set_vexpand(true);
        panel.inner.container.set_hexpand(true);

        if active_idx < panel.inner.notebook.n_pages() as usize {
            panel
                .inner
                .notebook
                .set_current_page(Some(active_idx as u32));
        }

        // Background task output polling timer (500 ms)
        {
            let p = panel.clone();
            glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
                p.poll_task_outputs();
                glib::ControlFlow::Continue
            });
        }

        panel
    }

    // ── Public API ──────────────────────────────────────────────────────────

    pub fn container(&self) -> &gtk::Box {
        &self.inner.container
    }

    /// Show the run panel and cd the active terminal to `dir`.
    pub fn show_and_cd(&self, dir: &str) {
        self.inner.container.set_visible(true);

        let idx = self.inner.notebook.current_page().unwrap_or(0) as usize;
        self.materialize_tab(idx);

        let tabs = self.inner.tabs.borrow();
        if let Some(tab) = tabs.get(idx)
            && let TabKind::Shell {
                terminal,
                has_child,
            } = &tab.kind
            && let Some(ref vte) = *terminal.borrow()
        {
            if has_child.get() {
                terminal::send_cd(vte, dir);
            } else {
                terminal::spawn_shell(vte, dir);
                has_child.set(true);
            }
            vte.grab_focus();
        }
    }

    /// Apply theme colors to all materialized terminals.
    pub fn apply_colors(&self, theme: Theme) {
        let tabs = self.inner.tabs.borrow();
        for tab in tabs.iter() {
            if let TabKind::Shell { terminal, .. } = &tab.kind
                && let Some(ref vte) = *terminal.borrow()
            {
                terminal::apply_colors(vte, theme);
            }
        }
    }

    /// Save scrollback for all dirty shell tabs.
    pub fn save_dirty_tabs(&self) {
        let tabs = self.inner.tabs.borrow();
        for tab in tabs.iter() {
            if tab.dirty.get()
                && let TabKind::Shell { terminal, .. } = &tab.kind
                && let Some(ref vte) = *terminal.borrow()
            {
                let path = storage::terminal_tab_content_path(&self.inner.workspace_id, &tab.id);
                terminal::save_scrollback(vte, &path);
                tab.dirty.set(false);
            }
        }
    }

    /// Return the current shell tab layout for persistence.
    /// Background task tabs are ephemeral and not persisted.
    pub fn run_tab_configs(&self) -> Vec<RunTabConfig> {
        let tabs = self.inner.tabs.borrow();
        let n = self.inner.notebook.n_pages();
        let mut configs = Vec::with_capacity(n as usize);
        for i in 0..n {
            if let Some(page) = self.inner.notebook.nth_page(Some(i))
                && let Some(tab) = tabs.iter().find(|t| t.content_box == page)
                && matches!(tab.kind, TabKind::Shell { .. })
            {
                configs.push(RunTabConfig {
                    id: tab.id.clone(),
                    name: tab.name.borrow().clone(),
                    tab_type: RunTabType::Shell,
                });
            }
        }
        configs
    }

    pub fn active_run_tab(&self) -> usize {
        self.inner.notebook.current_page().unwrap_or(0) as usize
    }

    pub fn is_any_dirty(&self) -> bool {
        self.inner.tabs.borrow().iter().any(|t| t.dirty.get())
    }

    pub fn is_visible(&self) -> bool {
        self.inner.container.is_visible()
    }

    pub fn set_visible(&self, visible: bool) {
        self.inner.container.set_visible(visible);
    }

    // ── Background task API (Phase 2) ───────────────────────────────────────

    /// Create a new background-task tab. Called when the agent panel detects
    /// `Bash` tool_use with `run_in_background: true`.
    pub fn add_background_task_tab(&self, command: &str, tool_id: &str) {
        let label_name = truncate_command(command, 30);
        let tab_id = uuid::Uuid::new_v4().to_string();

        let content_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content_box.set_vexpand(true);
        content_box.set_hexpand(true);

        let status_label = gtk::Label::new(Some(TaskStatus::Running.indicator()));
        status_label.add_css_class(TaskStatus::Running.css_class());

        let header = self.create_task_tab_header(&label_name, &tab_id, &status_label);

        let page_num = self.inner.notebook.append_page(&content_box, Some(&header));
        self.inner.notebook.set_tab_reorderable(&content_box, true);

        let slot = RunTabSlot {
            id: tab_id,
            name: Rc::new(RefCell::new(label_name)),
            content_box,
            kind: TabKind::Task {
                tool_id: tool_id.to_string(),
                text_view: RefCell::new(None),
                output_file: RefCell::new(None),
                status: Cell::new(TaskStatus::Running),
                status_label,
                file_offset: Cell::new(0),
            },
            dirty: Rc::new(Cell::new(false)),
            materialized: Cell::new(false),
        };

        // Materialize immediately (task tabs are lightweight)
        materialize_task_tab(&slot, command);

        self.inner.tabs.borrow_mut().push(slot);
        self.update_close_button_sensitivity();

        // Show the panel and switch to the new tab
        self.inner.container.set_visible(true);
        self.inner.notebook.set_current_page(Some(page_num as u32));
    }

    /// Called when a `task_notification` system event arrives — the task is
    /// genuinely finished. Updates status and ensures the output file is set.
    pub fn complete_task(&self, tool_use_id: &str, status: &str, output_file: Option<&str>) {
        let tabs = self.inner.tabs.borrow();
        for tab in tabs.iter() {
            if let TabKind::Task {
                tool_id,
                output_file: of,
                status: st,
                status_label,
                ..
            } = &tab.kind
            {
                if tool_id != tool_use_id {
                    continue;
                }
                // Set the output file if we didn't get it from ToolResult
                if of.borrow().is_none()
                    && let Some(path) = output_file
                {
                    *of.borrow_mut() = Some(PathBuf::from(path));
                }
                let new_status = match status {
                    "completed" => TaskStatus::Done,
                    "failed" | "stopped" => TaskStatus::Failed,
                    _ => TaskStatus::Done,
                };
                set_task_status(st, status_label, new_status);
                break;
            }
        }
    }

    /// Called when `ToolResult` arrives for a background task.
    /// Extracts the output file path and starts tailing it.
    /// The ToolResult text itself is just Claude Code boilerplate
    /// ("Command running in background...") — we don't show it.
    pub fn update_task_result(&self, tool_id: &str, output: &str, is_error: bool) {
        let tabs = self.inner.tabs.borrow();
        for tab in tabs.iter() {
            if let TabKind::Task {
                tool_id: tid,
                output_file,
                status,
                status_label,
                ..
            } = &tab.kind
            {
                if tid != tool_id {
                    continue;
                }

                // Extract the output file path — this is what we'll tail
                if let Some(path) = extract_task_output_path(output) {
                    *output_file.borrow_mut() = Some(PathBuf::from(path));
                }

                if is_error {
                    set_task_status(status, status_label, TaskStatus::Failed);
                }

                break;
            }
        }
    }

    // ── Private: shell tab operations ───────────────────────────────────────

    fn add_shell_tab(&self) -> usize {
        let num = self.inner.next_bash_number.get();
        self.inner.next_bash_number.set(num + 1);
        let name = format!("bash({num})");
        let id = uuid::Uuid::new_v4().to_string();

        let cfg = RunTabConfig {
            id,
            name,
            tab_type: RunTabType::Shell,
        };

        let page_idx = self.add_tab_from_config(&cfg);
        self.materialize_tab(page_idx);
        self.update_close_button_sensitivity();
        page_idx
    }

    fn add_tab_from_config(&self, cfg: &RunTabConfig) -> usize {
        let content_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content_box.set_vexpand(true);
        content_box.set_hexpand(true);

        let header = self.create_shell_tab_header(&cfg.name, &cfg.id);

        let page_num = self.inner.notebook.append_page(&content_box, Some(&header));
        self.inner.notebook.set_tab_reorderable(&content_box, true);

        let slot = RunTabSlot {
            id: cfg.id.clone(),
            name: Rc::new(RefCell::new(cfg.name.clone())),
            content_box,
            kind: TabKind::Shell {
                terminal: RefCell::new(None),
                has_child: Rc::new(Cell::new(false)),
            },
            dirty: Rc::new(Cell::new(false)),
            materialized: Cell::new(false),
        };

        self.inner.tabs.borrow_mut().push(slot);
        self.update_close_button_sensitivity();

        page_num as usize
    }

    fn materialize_tab(&self, index: usize) {
        let tabs = self.inner.tabs.borrow();
        let Some(tab) = tabs.get(index) else { return };
        if tab.materialized.get() {
            // Already materialized — grab focus
            match &tab.kind {
                TabKind::Shell { terminal, .. } => {
                    if let Some(ref vte) = *terminal.borrow() {
                        vte.grab_focus();
                    }
                }
                TabKind::Task { text_view, .. } => {
                    if let Some(ref tv) = *text_view.borrow() {
                        tv.grab_focus();
                    }
                }
            }
            return;
        }

        match &tab.kind {
            TabKind::Shell {
                terminal,
                has_child,
            } => {
                let vte = vte4::Terminal::new();
                vte.set_vexpand(true);
                vte.set_hexpand(true);
                vte.set_scrollback_lines(TERMINAL_SCROLLBACK_LINES);

                let font_desc = gtk::pango::FontDescription::from_string(TERMINAL_FONT);
                vte.set_font(Some(&font_desc));
                terminal::apply_colors(&vte, self.inner.theme.get());

                let term_path =
                    storage::terminal_tab_content_path(&self.inner.workspace_id, &tab.id);
                terminal::restore_scrollback(&vte, &term_path);

                let wd = self.inner.working_directory.to_string_lossy().to_string();
                terminal::spawn_shell(&vte, &wd);
                has_child.set(true);

                {
                    let dirty = Rc::clone(&tab.dirty);
                    vte.connect_contents_changed(move |_| dirty.set(true));
                }
                {
                    let hc = Rc::clone(has_child);
                    vte.connect_child_exited(move |_, _| hc.set(false));
                }

                tab.content_box.append(&vte);
                vte.grab_focus();
                *terminal.borrow_mut() = Some(vte);
            }
            TabKind::Task { .. } => {
                // Task tabs are materialized eagerly in add_background_task_tab
            }
        }
        tab.materialized.set(true);
    }

    // ── Private: task output polling ────────────────────────────────────────

    /// Called every 500 ms — reads new bytes from task output files.
    /// Does NOT detect completion — that comes from `task_notification` events.
    fn poll_task_outputs(&self) {
        let tabs = self.inner.tabs.borrow();
        for tab in tabs.iter() {
            if let TabKind::Task {
                text_view,
                output_file,
                file_offset,
                ..
            } = &tab.kind
            {
                let file_ref = output_file.borrow();
                let Some(ref path) = *file_ref else {
                    continue;
                };

                let metadata = match std::fs::metadata(path) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let file_len = metadata.len();
                let offset = file_offset.get();
                if file_len <= offset {
                    continue;
                }

                // Read only the new bytes
                let mut file = match std::fs::File::open(path) {
                    Ok(f) => f,
                    Err(_) => continue,
                };
                if file.seek(SeekFrom::Start(offset)).is_err() {
                    continue;
                }
                let mut buf = vec![0u8; (file_len - offset) as usize];
                if file.read_exact(&mut buf).is_err() {
                    continue;
                }

                let new_text = String::from_utf8_lossy(&buf);
                if let Some(ref tv) = *text_view.borrow() {
                    let buffer = tv.buffer();
                    let mut end = buffer.end_iter();
                    buffer.insert(&mut end, &new_text);
                    // Auto-scroll to bottom
                    let mut end = buffer.end_iter();
                    tv.scroll_to_iter(&mut end, 0.0, false, 0.0, 0.0);
                }
                file_offset.set(file_len);
            }
        }
    }

    // ── Private: tab headers ────────────────────────────────────────────────

    fn create_shell_tab_header(&self, name: &str, tab_id: &str) -> gtk::Box {
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 4);

        let icon = gtk::Image::from_icon_name("utilities-terminal-symbolic");
        icon.set_pixel_size(14);
        hbox.append(&icon);

        let label = gtk::Label::new(Some(name));
        label.set_hexpand(true);
        label.set_xalign(0.0);
        hbox.append(&label);

        let close_btn = make_close_button();
        hbox.append(&close_btn);

        {
            let p = self.clone();
            let id = tab_id.to_string();
            close_btn.connect_clicked(move |_| p.close_tab_by_id(&id));
        }

        self.wire_tab_header_context_menu(&hbox, tab_id, &label);
        hbox
    }

    fn create_task_tab_header(
        &self,
        name: &str,
        tab_id: &str,
        status_label: &gtk::Label,
    ) -> gtk::Box {
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 4);

        let icon = gtk::Image::from_icon_name("system-run-symbolic");
        icon.set_pixel_size(14);
        hbox.append(&icon);

        let label = gtk::Label::new(Some(name));
        label.set_hexpand(true);
        label.set_xalign(0.0);
        hbox.append(&label);

        hbox.append(status_label);

        let close_btn = make_close_button();
        hbox.append(&close_btn);

        {
            let p = self.clone();
            let id = tab_id.to_string();
            close_btn.connect_clicked(move |_| p.close_tab_by_id(&id));
        }

        // Right-click context menu (rename + copy + close — no "Add to Chat" for task tabs)
        self.wire_tab_header_context_menu(&hbox, tab_id, &label);
        hbox
    }

    fn wire_tab_header_context_menu(&self, header: &gtk::Box, tab_id: &str, label: &gtk::Label) {
        let popover = gtk::Popover::new();
        popover.set_parent(header);
        popover.set_autohide(true);

        let menu_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        menu_box.set_margin_top(4);
        menu_box.set_margin_bottom(4);
        menu_box.set_margin_start(4);
        menu_box.set_margin_end(4);

        let rename_btn = gtk::Button::with_label("Rename");
        rename_btn.set_has_frame(false);
        rename_btn.add_css_class("flat");
        menu_box.append(&rename_btn);

        let copy_btn = gtk::Button::with_label("Copy All Text");
        copy_btn.set_has_frame(false);
        copy_btn.add_css_class("flat");
        menu_box.append(&copy_btn);

        let chat_btn = gtk::Button::with_label("Add Selected to Chat");
        chat_btn.set_has_frame(false);
        chat_btn.add_css_class("flat");
        menu_box.append(&chat_btn);

        menu_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let close_btn = gtk::Button::with_label("Close Tab");
        close_btn.set_has_frame(false);
        close_btn.add_css_class("flat");
        menu_box.append(&close_btn);

        popover.set_child(Some(&menu_box));

        {
            let p = self.clone();
            let id = tab_id.to_string();
            let lbl = label.clone();
            let pop = popover.clone();
            rename_btn.connect_clicked(move |_| {
                pop.popdown();
                p.show_rename_popover(&lbl, &id);
            });
        }
        {
            let p = self.clone();
            let id = tab_id.to_string();
            let hdr = header.clone();
            let pop = popover.clone();
            copy_btn.connect_clicked(move |_| {
                pop.popdown();
                p.copy_all_text(&id, &hdr);
            });
        }
        {
            let p = self.clone();
            let id = tab_id.to_string();
            let pop = popover.clone();
            chat_btn.connect_clicked(move |_| {
                pop.popdown();
                p.add_selection_to_chat(&id);
            });
        }
        {
            let p = self.clone();
            let id = tab_id.to_string();
            let pop = popover.clone();
            close_btn.connect_clicked(move |_| {
                pop.popdown();
                p.close_tab_by_id(&id);
            });
        }

        let gesture = gtk::GestureClick::new();
        gesture.set_button(3);
        {
            let pop = popover.clone();
            gesture.connect_released(move |gest, _n, x, y| {
                gest.set_state(gtk::EventSequenceState::Claimed);
                let rect = gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1);
                pop.set_pointing_to(Some(&rect));
                pop.popup();
            });
        }
        header.add_controller(gesture);
    }

    // ── Private: tab actions ────────────────────────────────────────────────

    fn show_rename_popover(&self, label: &gtk::Label, tab_id: &str) {
        let popover = gtk::Popover::new();
        popover.set_parent(label);
        popover.set_autohide(true);

        let entry = gtk::Entry::new();
        entry.set_text(&label.text());
        entry.set_width_chars(20);
        popover.set_child(Some(&entry));

        {
            let p = self.clone();
            let id = tab_id.to_string();
            let lbl = label.clone();
            let pop = popover.clone();
            entry.connect_activate(move |e| {
                let new_name = e.text().to_string();
                if !new_name.is_empty() {
                    lbl.set_text(&new_name);
                    p.rename_tab(&id, &new_name);
                }
                pop.popdown();
            });
        }

        popover.popup();
        entry.grab_focus();
    }

    fn rename_tab(&self, tab_id: &str, new_name: &str) {
        let tabs = self.inner.tabs.borrow();
        if let Some(tab) = tabs.iter().find(|t| t.id == tab_id) {
            *tab.name.borrow_mut() = new_name.to_string();
        }
    }

    fn copy_all_text(&self, tab_id: &str, widget: &gtk::Box) {
        let tabs = self.inner.tabs.borrow();
        let Some(tab) = tabs.iter().find(|t| t.id == tab_id) else {
            return;
        };
        match &tab.kind {
            TabKind::Shell { terminal, .. } => {
                if let Some(ref vte) = *terminal.borrow() {
                    let mem_stream = gtk::gio::MemoryOutputStream::new_resizable();
                    if vte
                        .write_contents_sync(
                            &mem_stream,
                            vte4::WriteFlags::Default,
                            None::<&gtk::gio::Cancellable>,
                        )
                        .is_ok()
                        && mem_stream.close(None::<&gtk::gio::Cancellable>).is_ok()
                    {
                        let bytes = mem_stream.steal_as_bytes();
                        let text = String::from_utf8_lossy(&bytes);
                        widget.clipboard().set_text(text.trim());
                    }
                }
            }
            TabKind::Task { text_view, .. } => {
                if let Some(ref tv) = *text_view.borrow() {
                    let buffer = tv.buffer();
                    let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
                    widget.clipboard().set_text(text.trim());
                }
            }
        }
    }

    fn add_selection_to_chat(&self, tab_id: &str) {
        let tabs = self.inner.tabs.borrow();
        let Some(tab) = tabs.iter().find(|t| t.id == tab_id) else {
            return;
        };
        match &tab.kind {
            TabKind::Shell { terminal, .. } => {
                if let Some(ref vte) = *terminal.borrow()
                    && vte.has_selection()
                {
                    vte.copy_clipboard_format(vte4::Format::Text);
                    let clipboard = vte.clipboard();
                    let on_chat = Rc::clone(&self.inner.on_add_to_chat);
                    clipboard.read_text_async(None::<&gtk::gio::Cancellable>, move |result| {
                        if let Ok(Some(text)) = result {
                            let trimmed = text.trim().to_string();
                            if !trimmed.is_empty() {
                                on_chat(trimmed);
                            }
                        }
                    });
                }
            }
            TabKind::Task { text_view, .. } => {
                if let Some(ref tv) = *text_view.borrow() {
                    let buffer = tv.buffer();
                    if buffer.has_selection() {
                        let (start, end) = buffer.selection_bounds().unwrap();
                        let text = buffer.text(&start, &end, false).trim().to_string();
                        if !text.is_empty() {
                            (self.inner.on_add_to_chat)(text);
                        }
                    }
                }
            }
        }
    }

    fn close_tab_by_id(&self, tab_id: &str) {
        // Count shell tabs — we need at least one shell tab to remain
        let (shell_count, is_shell) = {
            let tabs = self.inner.tabs.borrow();
            let count = tabs
                .iter()
                .filter(|t| matches!(t.kind, TabKind::Shell { .. }))
                .count();
            let is_shell = tabs
                .iter()
                .find(|t| t.id == tab_id)
                .is_some_and(|t| matches!(t.kind, TabKind::Shell { .. }));
            (count, is_shell)
        };

        // If closing the last shell tab, hide the panel instead
        if is_shell && shell_count <= 1 {
            self.inner.container.set_visible(false);
            return;
        }

        let idx = {
            let tabs = self.inner.tabs.borrow();
            tabs.iter().position(|t| t.id == tab_id)
        };

        if let Some(idx) = idx {
            self.inner.notebook.remove_page(Some(idx as u32));
            let removed = self.inner.tabs.borrow_mut().remove(idx);
            if matches!(removed.kind, TabKind::Shell { .. }) {
                storage::delete_terminal_tab_content(&self.inner.workspace_id, &removed.id);
            }
            self.update_close_button_sensitivity();
        }
    }

    fn update_close_button_sensitivity(&self) {
        let shell_count = self
            .inner
            .tabs
            .borrow()
            .iter()
            .filter(|t| matches!(t.kind, TabKind::Shell { .. }))
            .count();
        let n_pages = self.inner.notebook.n_pages();
        for i in 0..n_pages {
            if let Some(child) = self.inner.notebook.nth_page(Some(i))
                && let Some(header) = self.inner.notebook.tab_label(&child)
                && let Some(header_box) = header.downcast_ref::<gtk::Box>()
            {
                // Find if this is a shell tab
                let is_shell = self
                    .inner
                    .tabs
                    .borrow()
                    .iter()
                    .find(|t| t.content_box == child)
                    .is_some_and(|t| matches!(t.kind, TabKind::Shell { .. }));

                let mut w = header_box.first_child();
                while let Some(widget) = w {
                    if let Some(btn) = widget.downcast_ref::<gtk::Button>()
                        && btn.has_css_class("run-tab-close")
                    {
                        let tip = if is_shell && shell_count <= 1 {
                            "Hide panel"
                        } else {
                            "Close tab"
                        };
                        btn.set_tooltip_text(Some(tip));
                    }
                    w = widget.next_sibling();
                }
            }
        }
    }
}

// ── Free functions ──────────────────────────────────────────────────────────

/// Materialize a task tab — creates a read-only monospace TextView showing
/// the full command, a separator, and (later) the streamed output.
fn materialize_task_tab(tab: &RunTabSlot, command: &str) {
    if tab.materialized.get() {
        return;
    }
    if let TabKind::Task { text_view, .. } = &tab.kind {
        let tv = gtk::TextView::new();
        tv.set_editable(false);
        tv.set_cursor_visible(false);
        tv.set_monospace(true);
        tv.set_wrap_mode(gtk::WrapMode::WordChar);
        tv.set_vexpand(true);
        tv.set_hexpand(true);
        tv.set_top_margin(4);
        tv.set_left_margin(8);
        tv.set_right_margin(8);

        let scroll = gtk::ScrolledWindow::new();
        scroll.set_child(Some(&tv));
        scroll.set_vexpand(true);

        // Command header + separator + blank line for output
        let buffer = tv.buffer();
        let mut end = buffer.end_iter();
        buffer.insert(&mut end, &format!("$ {command}\n"));
        buffer.insert(&mut end, &"\u{2500}".repeat(60)); // ────── separator
        buffer.insert(&mut end, "\n");

        tab.content_box.append(&scroll);
        *text_view.borrow_mut() = Some(tv);
        tab.materialized.set(true);
    }
}

fn make_close_button() -> gtk::Button {
    let btn = gtk::Button::from_icon_name("window-close-symbolic");
    btn.set_has_frame(false);
    btn.set_tooltip_text(Some("Close tab"));
    btn.add_css_class("flat");
    btn.add_css_class("run-tab-close");
    btn
}

fn set_task_status(status: &Cell<TaskStatus>, label: &gtk::Label, new: TaskStatus) {
    let old = status.get();
    if old == new {
        return;
    }
    label.remove_css_class(old.css_class());
    label.add_css_class(new.css_class());
    label.set_text(new.indicator());
    status.set(new);
}

/// Extract a task output file path from the ToolResult output.
/// Claude Code prints: "Output is being written to: /tmp/claude-1000/.../tasks/ID.output"
fn extract_task_output_path(output: &str) -> Option<String> {
    // Primary: look for "written to: <path>" (Claude Code convention)
    for pattern in ["written to: ", "Output: ", "output: "] {
        if let Some(idx) = output.find(pattern) {
            let rest = &output[idx + pattern.len()..];
            let path = rest
                .split(|c: char| c.is_whitespace() || c == '\'' || c == '"')
                .next()?;
            let path = path.trim();
            if path.starts_with('/') && path.len() > 5 {
                return Some(path.to_string());
            }
        }
    }
    // Fallback: any absolute path containing /tasks/
    for token in output.split(|c: char| c.is_whitespace() || c == '\'' || c == '"' || c == '`') {
        let token = token.trim();
        if token.starts_with('/') && token.contains("/tasks/") && token.len() > 10 {
            return Some(token.to_string());
        }
    }
    None
}

/// Truncate a command string for use as a tab label.
fn truncate_command(cmd: &str, max_len: usize) -> String {
    let first_line = cmd.lines().next().unwrap_or(cmd).trim();
    if first_line.len() <= max_len {
        first_line.to_string()
    } else {
        format!("{}...", &first_line[..max_len.saturating_sub(3)])
    }
}

fn parse_bash_number(name: &str) -> Option<u32> {
    let name = name.trim();
    if let Some(rest) = name.strip_prefix("bash(")
        && let Some(num_str) = rest.strip_suffix(')')
    {
        return num_str.parse().ok();
    }
    None
}
