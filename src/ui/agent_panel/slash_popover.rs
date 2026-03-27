use gtk::prelude::*;
use gtk4 as gtk;
use std::cell::RefCell;
use std::rc::Rc;

use crate::models::slash_command::SlashCommand;

/// Autocomplete popover for slash commands.
pub(crate) struct SlashPopover {
    pub popover: gtk::Popover,
    list_box: gtk::ListBox,
    all_commands: Rc<RefCell<Vec<SlashCommand>>>,
    filtered: Rc<RefCell<Vec<SlashCommand>>>,
    on_select: Rc<dyn Fn(&SlashCommand)>,
    on_configure: Rc<dyn Fn()>,
}

impl SlashPopover {
    pub fn new(
        anchor: &gtk::Widget,
        commands: Vec<SlashCommand>,
        on_select: impl Fn(&SlashCommand) + 'static,
        on_configure: impl Fn() + 'static,
    ) -> Rc<Self> {
        let popover = gtk::Popover::new();
        popover.set_parent(anchor);
        popover.set_position(gtk::PositionType::Top);
        popover.set_autohide(false);
        popover.set_has_arrow(false);

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        vbox.set_width_request(600);

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .max_content_height(400)
            .propagate_natural_height(true)
            .build();

        let list_box = gtk::ListBox::new();
        list_box.set_selection_mode(gtk::SelectionMode::Browse);
        list_box.add_css_class("rich-list");
        scrolled.set_child(Some(&list_box));

        vbox.append(&scrolled);
        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        // "Configure..." row
        let configure_btn = gtk::Button::with_label("Configure\u{2026}");
        configure_btn.set_has_frame(false);
        configure_btn.set_halign(gtk::Align::Start);
        configure_btn.set_margin_start(8);
        configure_btn.set_margin_top(4);
        configure_btn.set_margin_bottom(4);
        vbox.append(&configure_btn);

        popover.set_child(Some(&vbox));

        let sp = Rc::new(Self {
            popover,
            list_box,
            all_commands: Rc::new(RefCell::new(commands)),
            filtered: Rc::new(RefCell::new(Vec::new())),
            on_select: Rc::new(on_select),
            on_configure: Rc::new(on_configure),
        });

        // Wire list_box row activation
        {
            let sp2 = Rc::clone(&sp);
            sp.list_box.connect_row_activated(move |_, row| {
                let idx = row.index() as usize;
                let filtered = sp2.filtered.borrow();
                if let Some(cmd) = filtered.get(idx) {
                    (sp2.on_select)(cmd);
                }
                sp2.popover.popdown();
            });
        }

        // Wire configure button
        {
            let sp2 = Rc::clone(&sp);
            configure_btn.connect_clicked(move |_| {
                sp2.popover.popdown();
                (sp2.on_configure)();
            });
        }

        sp
    }

    /// Update the displayed list, filtering by prefix match on command name.
    pub fn update_filter(&self, query: &str) {
        let query_lower = query.to_lowercase();
        let all = self.all_commands.borrow();
        let mut results: Vec<SlashCommand> = if query_lower.is_empty() {
            all.to_vec()
        } else {
            all.iter()
                .filter(|c| c.name.to_lowercase().starts_with(&query_lower))
                .cloned()
                .collect()
        };

        // Add /rescan-skills as a synthetic command if it matches
        if "rescan-skills".starts_with(&query_lower) || query_lower.is_empty() {
            // Only add if not already in the truncated list (unlikely)
            if !results.iter().any(|c| c.name == "rescan-skills") {
                results.push(SlashCommand {
                    name: "rescan-skills".to_string(),
                    description: "Rescan skill directories for new commands".to_string(),
                    argument_hint: String::new(),
                    source: crate::models::slash_command::SlashCommandSource::BuiltIn,
                });
            }
        }

        *self.filtered.borrow_mut() = results;
        self.rebuild_rows();
    }

    /// Show the popover (call `update_filter` first).
    pub fn show(&self) {
        if !self.filtered.borrow().is_empty() {
            self.popover.popup();
        }
    }

    /// Hide the popover.
    pub fn hide(&self) {
        self.popover.popdown();
    }

    pub fn is_visible(&self) -> bool {
        self.popover.is_visible()
    }

    /// Move selection down.
    pub fn select_next(&self) {
        let current = self
            .list_box
            .selected_row()
            .map(|r| r.index())
            .unwrap_or(-1);
        let n = self.filtered.borrow().len() as i32;
        let next = (current + 1).min(n - 1);
        if let Some(row) = self.list_box.row_at_index(next) {
            self.list_box.select_row(Some(&row));
            // Scroll the row into view
            row.grab_focus();
        }
    }

    /// Move selection up.
    pub fn select_prev(&self) {
        let current = self.list_box.selected_row().map(|r| r.index()).unwrap_or(0);
        let prev = (current - 1).max(0);
        if let Some(row) = self.list_box.row_at_index(prev) {
            self.list_box.select_row(Some(&row));
            row.grab_focus();
        }
    }

    /// Activate (select) the currently highlighted row.
    pub fn activate_selected(&self) {
        if let Some(row) = self.list_box.selected_row() {
            let idx = row.index() as usize;
            let filtered = self.filtered.borrow();
            if let Some(cmd) = filtered.get(idx) {
                (self.on_select)(cmd);
            }
        }
        self.popover.popdown();
    }

    /// Replace the command list (for /rescan-skills).
    pub fn reload(&self, commands: Vec<SlashCommand>) {
        *self.all_commands.borrow_mut() = commands;
    }

    // ── internal ─────────────────────────────────────────────────────────────

    fn rebuild_rows(&self) {
        // Remove all existing rows
        while let Some(row) = self.list_box.row_at_index(0) {
            self.list_box.remove(&row);
        }

        let filtered = self.filtered.borrow();
        for cmd in filtered.iter() {
            let row = build_command_row(cmd);
            self.list_box.append(&row);
        }

        // Select first row
        if let Some(first) = self.list_box.row_at_index(0) {
            self.list_box.select_row(Some(&first));
        }
    }
}

fn build_command_row(cmd: &SlashCommand) -> gtk::ListBoxRow {
    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    hbox.set_margin_start(8);
    hbox.set_margin_end(8);
    hbox.set_margin_top(4);
    hbox.set_margin_bottom(4);

    // Left: /name <hint>
    let left = if cmd.argument_hint.is_empty() {
        format!("/{}", cmd.name)
    } else {
        format!("/{} {}", cmd.name, cmd.argument_hint)
    };
    let name_label = gtk::Label::new(Some(&left));
    name_label.set_xalign(0.0);
    name_label.add_css_class("monospace");
    hbox.append(&name_label);

    // Spacer
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    hbox.append(&spacer);

    // Right: description (dimmed, ellipsized)
    if !cmd.description.is_empty() {
        let desc_label = gtk::Label::new(Some(&cmd.description));
        desc_label.set_xalign(1.0);
        desc_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        desc_label.set_max_width_chars(60);
        desc_label.add_css_class("dim-label");
        hbox.append(&desc_label);
    }

    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&hbox));
    row.set_tooltip_text(Some(&cmd.description));
    row
}
