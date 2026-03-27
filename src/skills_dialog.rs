use gtk::glib;
use gtk::prelude::*;
use gtk4 as gtk;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use crate::config::constants::{
    SKILLS_DIALOG_HEIGHT, SKILLS_DIALOG_MASTER_WIDTH, SKILLS_DIALOG_WIDTH,
};
use crate::models::slash_command::{SlashCommand, SlashCommandKind, SlashCommandSource};
use crate::services::skills;

/// Filter categories for the master list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum FilterCategory {
    #[default]
    All,
    Plugins,
    GlobalSkills,
    ProjectSkills,
}

/// Open the skills/commands CRUD dialog.
/// Calls `on_close` when the dialog is dismissed so the popover can rescan.
pub fn show(parent: &gtk::Window, working_dir: &Path, on_close: impl Fn() + 'static) {
    let wd = working_dir.to_path_buf();
    let items: Rc<RefCell<Vec<SlashCommand>>> =
        Rc::new(RefCell::new(skills::discover_slash_commands(working_dir)));
    let current_idx: Rc<RefCell<Option<usize>>> = Rc::new(RefCell::new(None));
    let filter: Rc<RefCell<FilterCategory>> = Rc::new(RefCell::new(FilterCategory::All));

    let dialog = gtk::Window::builder()
        .title("Configure Skills & Commands")
        .modal(true)
        .transient_for(parent)
        .default_width(SKILLS_DIALOG_WIDTH)
        .default_height(SKILLS_DIALOG_HEIGHT)
        .build();

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    hbox.set_vexpand(true);

    // ── Master (left) ──
    let master = gtk::Box::new(gtk::Orientation::Vertical, 0);
    master.set_width_request(SKILLS_DIALOG_MASTER_WIDTH);

    // Filter dropdown
    let filter_model = gtk::StringList::new(&["All", "Plugins", "Global Skills", "Project Skills"]);
    let filter_dropdown = gtk::DropDown::new(Some(filter_model), gtk::Expression::NONE);
    filter_dropdown.set_margin_start(4);
    filter_dropdown.set_margin_end(4);
    filter_dropdown.set_margin_top(4);
    filter_dropdown.set_margin_bottom(4);
    master.append(&filter_dropdown);

    let list_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();

    let list_box = gtk::ListBox::new();
    list_box.set_selection_mode(gtk::SelectionMode::Browse);
    list_scroll.set_child(Some(&list_box));
    master.append(&list_scroll);

    // Add / Remove buttons
    let master_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    master_buttons.set_margin_start(4);
    master_buttons.set_margin_end(4);
    master_buttons.set_margin_top(4);
    master_buttons.set_margin_bottom(4);

    let add_btn = gtk::MenuButton::new();
    add_btn.set_icon_name("list-add-symbolic");
    add_btn.set_tooltip_text(Some("Add new command or skill"));
    add_btn.set_direction(gtk::ArrowType::Up);

    let add_menu = gtk::gio::Menu::new();
    add_menu.append(Some("New Global Command"), Some("skills.add-user-cmd"));
    add_menu.append(Some("New Global Skill"), Some("skills.add-user-skill"));
    add_menu.append(Some("New Project Command"), Some("skills.add-project-cmd"));
    add_menu.append(Some("New Project Skill"), Some("skills.add-project-skill"));
    add_btn.set_menu_model(Some(&add_menu));

    let remove_btn = gtk::Button::from_icon_name("list-remove-symbolic");
    remove_btn.set_tooltip_text(Some("Delete selected"));
    remove_btn.set_sensitive(false);

    master_buttons.append(&add_btn);
    master_buttons.append(&remove_btn);
    master.append(&master_buttons);

    master.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    hbox.append(&master);
    hbox.append(&gtk::Separator::new(gtk::Orientation::Vertical));

    // ── Detail (right) ──
    let detail = gtk::Box::new(gtk::Orientation::Vertical, 6);
    detail.set_hexpand(true);
    detail.set_margin_start(12);
    detail.set_margin_end(12);
    detail.set_margin_top(8);
    detail.set_margin_bottom(8);

    let name_entry = labeled_entry(&detail, "Name");
    let desc_entry = labeled_entry(&detail, "Description");
    let hint_entry = labeled_entry(&detail, "Argument hint");

    let source_label = gtk::Label::new(None);
    source_label.set_xalign(0.0);
    source_label.add_css_class("dim-label");
    source_label.set_margin_bottom(4);
    detail.append(&source_label);

    let body_label = gtk::Label::new(Some("Body"));
    body_label.set_xalign(0.0);
    detail.append(&body_label);

    let body_frame = gtk::Frame::new(None);
    let body_view = gtk::TextView::new();
    body_view.set_monospace(true);
    body_view.set_wrap_mode(gtk::WrapMode::WordChar);
    body_view.set_left_margin(6);
    body_view.set_right_margin(6);
    body_view.set_top_margin(4);
    body_view.set_bottom_margin(4);
    let body_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .child(&body_view)
        .build();
    body_frame.set_child(Some(&body_scroll));
    detail.append(&body_frame);

    // Save item button
    let detail_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    detail_buttons.set_halign(gtk::Align::End);
    detail_buttons.set_margin_top(4);

    let save_item_btn = gtk::Button::with_label("Save Item");
    save_item_btn.add_css_class("suggested-action");
    save_item_btn.set_sensitive(false);
    detail_buttons.append(&save_item_btn);
    detail.append(&detail_buttons);

    hbox.append(&detail);
    root.append(&hbox);

    // ── Bottom bar ──
    root.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    let bottom_bar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    bottom_bar.set_margin_start(8);
    bottom_bar.set_margin_end(8);
    bottom_bar.set_margin_top(8);
    bottom_bar.set_margin_bottom(8);

    let terminal_btn = gtk::Button::with_label("Install Plugins in Terminal");
    terminal_btn.set_tooltip_text(Some("Open terminal for 'claude plugin install'"));
    bottom_bar.append(&terminal_btn);

    let bottom_spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    bottom_spacer.set_hexpand(true);
    bottom_bar.append(&bottom_spacer);

    let close_btn = gtk::Button::with_label("Close");
    bottom_bar.append(&close_btn);
    root.append(&bottom_bar);

    dialog.set_child(Some(&root));

    // ── Helper closures ──

    let populate_list = {
        let list_box = list_box.clone();
        let items = Rc::clone(&items);
        let filter = Rc::clone(&filter);
        Rc::new(move || {
            while let Some(row) = list_box.row_at_index(0) {
                list_box.remove(&row);
            }
            let items = items.borrow();
            let f = *filter.borrow();
            for (i, cmd) in items.iter().enumerate() {
                if !matches_filter(cmd, f) {
                    continue;
                }
                let row = build_master_row(cmd);
                row.set_widget_name(&i.to_string());
                list_box.append(&row);
            }
        })
    };

    let show_detail = {
        let name_entry = name_entry.clone();
        let desc_entry = desc_entry.clone();
        let hint_entry = hint_entry.clone();
        let source_label = source_label.clone();
        let body_view = body_view.clone();
        let save_item_btn = save_item_btn.clone();
        let remove_btn = remove_btn.clone();
        let current_idx = Rc::clone(&current_idx);
        let items = Rc::clone(&items);
        Rc::new(move || {
            let idx = current_idx.borrow();
            let Some(i) = *idx else {
                name_entry.set_text("");
                desc_entry.set_text("");
                hint_entry.set_text("");
                source_label.set_text("");
                body_view.buffer().set_text("");
                set_detail_editable(&name_entry, &desc_entry, &hint_entry, &body_view, false);
                save_item_btn.set_sensitive(false);
                remove_btn.set_sensitive(false);
                return;
            };
            let items = items.borrow();
            let Some(cmd) = items.get(i) else {
                return;
            };

            name_entry.set_text(&cmd.name);
            desc_entry.set_text(&cmd.description);
            hint_entry.set_text(&cmd.argument_hint);
            source_label.set_text(&source_display(cmd));

            // Load body from disk
            let body = cmd
                .path
                .as_ref()
                .and_then(|p| skills::read_command_body(p).ok())
                .map(|content| skills::extract_body(&content))
                .unwrap_or_default();
            body_view.buffer().set_text(&body);

            let editable = is_editable(cmd);
            set_detail_editable(&name_entry, &desc_entry, &hint_entry, &body_view, editable);
            save_item_btn.set_sensitive(editable);
            remove_btn.set_sensitive(cmd.source != SlashCommandSource::BuiltIn);
        })
    };

    // ── Initial populate ──
    populate_list();

    // ── Signal wiring ──

    // Selection change
    {
        let current_idx = Rc::clone(&current_idx);
        let items = Rc::clone(&items);
        let filter = Rc::clone(&filter);
        let show_detail = Rc::clone(&show_detail);
        list_box.connect_row_selected(move |_, row| {
            if let Some(row) = row {
                // The widget name stores the original index into items
                if let Ok(i) = row.widget_name().parse::<usize>() {
                    *current_idx.borrow_mut() = Some(i);
                } else {
                    // Fallback: find by visible position
                    let vis_idx = row.index() as usize;
                    let items_ref = items.borrow();
                    let f = *filter.borrow();
                    let real_idx = items_ref
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| matches_filter(c, f))
                        .nth(vis_idx)
                        .map(|(i, _)| i);
                    *current_idx.borrow_mut() = real_idx;
                }
            } else {
                *current_idx.borrow_mut() = None;
            }
            show_detail();
        });
    }

    // Filter change
    {
        let filter = Rc::clone(&filter);
        let populate_list = Rc::clone(&populate_list);
        let current_idx = Rc::clone(&current_idx);
        let show_detail = Rc::clone(&show_detail);
        filter_dropdown.connect_selected_notify(move |dd| {
            *filter.borrow_mut() = match dd.selected() {
                1 => FilterCategory::Plugins,
                2 => FilterCategory::GlobalSkills,
                3 => FilterCategory::ProjectSkills,
                _ => FilterCategory::All,
            };
            *current_idx.borrow_mut() = None;
            populate_list();
            show_detail();
        });
    }

    // Save item
    {
        let name_entry = name_entry.clone();
        let desc_entry = desc_entry.clone();
        let hint_entry = hint_entry.clone();
        let body_view = body_view.clone();
        let items = Rc::clone(&items);
        let current_idx = Rc::clone(&current_idx);
        let wd = wd.clone();
        let populate_list = Rc::clone(&populate_list);
        let show_detail = Rc::clone(&show_detail);
        save_item_btn.connect_clicked(move |_| {
            let idx = *current_idx.borrow();
            let Some(i) = idx else { return };
            let cmd = {
                let items = items.borrow();
                items.get(i).cloned()
            };
            let Some(cmd) = cmd else { return };

            let name = name_entry.text().to_string();
            let desc = desc_entry.text().to_string();
            let hint = hint_entry.text().to_string();
            let buf = body_view.buffer();
            let body = buf
                .text(&buf.start_iter(), &buf.end_iter(), false)
                .to_string();

            let Some(base_dir) = skills::claude_dir_for_source(cmd.source, &wd) else {
                return;
            };

            // If name changed and old file exists, delete old
            if let Some(ref old_path) = cmd.path
                && name != cmd.name
            {
                let _ = skills::delete_command(old_path, cmd.kind);
            }

            match skills::save_command(&base_dir, &name, &desc, &hint, &body, cmd.kind) {
                Ok(new_path) => {
                    let mut items = items.borrow_mut();
                    if let Some(item) = items.get_mut(i) {
                        item.name = name;
                        item.description = desc;
                        item.argument_hint = hint;
                        item.path = Some(new_path);
                    }
                    drop(items);
                    populate_list();
                    show_detail();
                }
                Err(e) => eprintln!("Save failed: {e}"),
            }
        });
    }

    // Remove
    {
        let items = Rc::clone(&items);
        let current_idx = Rc::clone(&current_idx);
        let populate_list = Rc::clone(&populate_list);
        let show_detail = Rc::clone(&show_detail);
        remove_btn.connect_clicked(move |_| {
            let idx = *current_idx.borrow();
            let Some(i) = idx else { return };
            let cmd = {
                let items = items.borrow();
                items.get(i).cloned()
            };
            let Some(cmd) = cmd else { return };
            let Some(ref path) = cmd.path else { return };

            if let Err(e) = skills::delete_command(path, cmd.kind) {
                eprintln!("Delete failed: {e}");
                return;
            }

            items.borrow_mut().remove(i);
            *current_idx.borrow_mut() = None;
            populate_list();
            show_detail();
        });
    }

    // Add actions
    {
        let action_group = gtk::gio::SimpleActionGroup::new();
        let combos: &[(&str, SlashCommandSource, SlashCommandKind)] = &[
            (
                "add-user-cmd",
                SlashCommandSource::User,
                SlashCommandKind::Command,
            ),
            (
                "add-user-skill",
                SlashCommandSource::User,
                SlashCommandKind::Skill,
            ),
            (
                "add-project-cmd",
                SlashCommandSource::Project,
                SlashCommandKind::Command,
            ),
            (
                "add-project-skill",
                SlashCommandSource::Project,
                SlashCommandKind::Skill,
            ),
        ];
        for &(action_name, source, kind) in combos {
            let items = Rc::clone(&items);
            let current_idx = Rc::clone(&current_idx);
            let wd = wd.clone();
            let populate_list = Rc::clone(&populate_list);
            let show_detail = Rc::clone(&show_detail);
            let action = gtk::gio::SimpleAction::new(action_name, None);
            action.connect_activate(move |_, _| {
                let name = new_unique_name(&items.borrow(), "new-command");
                let Some(base_dir) = skills::claude_dir_for_source(source, &wd) else {
                    return;
                };
                match skills::save_command(&base_dir, &name, "", "", "", kind) {
                    Ok(path) => {
                        let mut its = items.borrow_mut();
                        its.push(SlashCommand {
                            name: name.clone(),
                            description: String::new(),
                            argument_hint: String::new(),
                            source,
                            kind,
                            path: Some(path),
                        });
                        let new_idx = its.len() - 1;
                        drop(its);
                        *current_idx.borrow_mut() = Some(new_idx);
                        populate_list();
                        show_detail();
                    }
                    Err(e) => eprintln!("Create failed: {e}"),
                }
            });
            action_group.add_action(&action);
        }
        dialog.insert_action_group("skills", Some(&action_group));
    }

    // Install Plugins in Terminal
    {
        let dialog_ref = dialog.clone();
        terminal_btn.connect_clicked(move |_| {
            if let Err(e) = std::process::Command::new("x-terminal-emulator")
                .arg("-e")
                .arg("bash -c 'echo \"Use: claude plugin install <name>\"; exec bash'")
                .spawn()
            {
                // Fallback: try gnome-terminal
                if let Err(e2) = std::process::Command::new("gnome-terminal")
                    .arg("--")
                    .arg("bash")
                    .arg("-c")
                    .arg("echo \"Use: claude plugin install <name>\"; exec bash")
                    .spawn()
                {
                    eprintln!("Failed to open terminal: {e}, {e2}");
                    let alert = gtk::AlertDialog::builder()
                        .message("Cannot open terminal")
                        .detail(e.to_string())
                        .build();
                    alert.show(Some(&dialog_ref));
                }
            }
        });
    }

    // Close button
    {
        let dialog_ref = dialog.clone();
        let on_close = Rc::new(on_close);
        let on_close2 = Rc::clone(&on_close);
        close_btn.connect_clicked(move |_| {
            dialog_ref.close();
            on_close();
        });
        dialog.connect_close_request(move |_| {
            on_close2();
            glib::Propagation::Proceed
        });
    }

    dialog.present();
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn labeled_entry(parent: &gtk::Box, label_text: &str) -> gtk::Entry {
    let label = gtk::Label::new(Some(label_text));
    label.set_xalign(0.0);
    parent.append(&label);
    let entry = gtk::Entry::new();
    entry.set_margin_bottom(4);
    parent.append(&entry);
    entry
}

fn is_editable(cmd: &SlashCommand) -> bool {
    matches!(
        cmd.source,
        SlashCommandSource::User | SlashCommandSource::Project
    )
}

fn set_detail_editable(
    name: &gtk::Entry,
    desc: &gtk::Entry,
    hint: &gtk::Entry,
    body: &gtk::TextView,
    editable: bool,
) {
    name.set_editable(editable);
    name.set_can_focus(editable);
    desc.set_editable(editable);
    desc.set_can_focus(editable);
    hint.set_editable(editable);
    hint.set_can_focus(editable);
    body.set_editable(editable);
    body.set_cursor_visible(editable);
}

fn source_display(cmd: &SlashCommand) -> String {
    let kind_str = match cmd.kind {
        SlashCommandKind::Command => "Command",
        SlashCommandKind::Skill => "Skill",
    };
    let source_str = match cmd.source {
        SlashCommandSource::BuiltIn => "Built-in",
        SlashCommandSource::User => "Global (User)",
        SlashCommandSource::Project => "Project",
        SlashCommandSource::Plugin => "Plugin",
    };
    format!("{source_str} {kind_str}")
}

fn icon_for_source(source: SlashCommandSource) -> &'static str {
    match source {
        SlashCommandSource::BuiltIn => "application-x-executable-symbolic",
        SlashCommandSource::User => "user-home-symbolic",
        SlashCommandSource::Project => "folder-symbolic",
        SlashCommandSource::Plugin => "application-x-addon-symbolic",
    }
}

fn matches_filter(cmd: &SlashCommand, filter: FilterCategory) -> bool {
    match filter {
        FilterCategory::All => true,
        FilterCategory::Plugins => cmd.source == SlashCommandSource::Plugin,
        FilterCategory::GlobalSkills => cmd.source == SlashCommandSource::User,
        FilterCategory::ProjectSkills => cmd.source == SlashCommandSource::Project,
    }
}

fn build_master_row(cmd: &SlashCommand) -> gtk::ListBoxRow {
    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    hbox.set_margin_start(6);
    hbox.set_margin_end(6);
    hbox.set_margin_top(3);
    hbox.set_margin_bottom(3);

    let icon = gtk::Image::from_icon_name(icon_for_source(cmd.source));
    icon.set_pixel_size(16);
    hbox.append(&icon);

    let label = gtk::Label::new(Some(&format!("/{}", cmd.name)));
    label.set_xalign(0.0);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    hbox.append(&label);

    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&hbox));
    row.set_tooltip_text(Some(&source_display(cmd)));
    row
}

fn new_unique_name(items: &[SlashCommand], base: &str) -> String {
    let mut name = base.to_string();
    let mut counter = 1;
    while items.iter().any(|c| c.name == name) {
        counter += 1;
        name = format!("{base}-{counter}");
    }
    name
}
