use gtk4 as gtk;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::session::{self, AgentConfig};

/// Open a master/detail dialog for CRUD on agent profiles.
/// Calls `on_save` with the updated config list when the user saves.
pub fn show(
    parent: &gtk::Window,
    on_save: impl Fn(Vec<AgentConfig>) + 'static,
) {
    let configs: Rc<RefCell<Vec<AgentConfig>>> = Rc::new(RefCell::new(session::list_agent_configs()));
    let deleted_names: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let current_idx: Rc<RefCell<Option<usize>>> = Rc::new(RefCell::new(None));

    let dialog = gtk::Window::builder()
        .title("Configure Agents")
        .modal(true)
        .transient_for(parent)
        .default_width(700)
        .default_height(480)
        .build();

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);

    // Main content: master | detail
    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    hbox.set_vexpand(true);

    // ── Master (left) ──
    let master = gtk::Box::new(gtk::Orientation::Vertical, 0);
    master.set_width_request(180);

    let list_box = gtk::ListBox::new();
    list_box.set_selection_mode(gtk::SelectionMode::Single);
    let list_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .child(&list_box)
        .build();

    let btn_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    btn_row.set_margin_start(4);
    btn_row.set_margin_end(4);
    btn_row.set_margin_top(4);
    btn_row.set_margin_bottom(4);
    let add_btn = gtk::Button::from_icon_name("list-add-symbolic");
    add_btn.set_tooltip_text(Some("New agent"));
    let del_btn = gtk::Button::from_icon_name("list-remove-symbolic");
    del_btn.set_tooltip_text(Some("Delete agent"));
    btn_row.append(&add_btn);
    btn_row.append(&del_btn);

    master.append(&list_scroll);
    master.append(&btn_row);

    // ── Detail (right) ──
    let detail = gtk::Box::new(gtk::Orientation::Vertical, 8);
    detail.set_margin_start(12);
    detail.set_margin_end(12);
    detail.set_margin_top(12);
    detail.set_margin_bottom(8);
    detail.set_hexpand(true);

    let name_entry = gtk::Entry::new();
    name_entry.set_placeholder_text(Some("Agent name"));
    detail.append(&labeled("Name", &name_entry));

    let model_entry = gtk::Entry::new();
    model_entry.set_placeholder_text(Some("(default model)"));
    detail.append(&labeled("Model", &model_entry));

    let prompt_view = gtk::TextView::new();
    prompt_view.set_wrap_mode(gtk::WrapMode::WordChar);
    prompt_view.set_top_margin(4);
    prompt_view.set_bottom_margin(4);
    prompt_view.set_left_margin(4);
    prompt_view.set_right_margin(4);
    let prompt_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .child(&prompt_view)
        .build();
    let prompt_frame = gtk::Frame::new(None);
    prompt_frame.set_child(Some(&prompt_scroll));
    let prompt_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    prompt_box.set_vexpand(true);
    prompt_box.append(&gtk::Label::builder().label("System Prompt").xalign(0.0).build());
    prompt_box.append(&prompt_frame);
    detail.append(&prompt_box);

    let tools_entry = gtk::Entry::new();
    tools_entry.set_placeholder_text(Some("Read, Grep, Bash, ... (empty = all tools)"));
    detail.append(&labeled("Allowed Tools", &tools_entry));

    // Separator between master and detail
    let sep = gtk::Separator::new(gtk::Orientation::Vertical);

    hbox.append(&master);
    hbox.append(&sep);
    hbox.append(&detail);

    // ── Bottom buttons ──
    let bottom = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    bottom.set_halign(gtk::Align::End);
    bottom.set_margin_start(12);
    bottom.set_margin_end(12);
    bottom.set_margin_top(8);
    bottom.set_margin_bottom(12);

    let cancel_btn = gtk::Button::with_label("Cancel");
    let save_btn = gtk::Button::with_label("Save");
    save_btn.add_css_class("suggested-action");
    bottom.append(&cancel_btn);
    bottom.append(&save_btn);

    root.append(&hbox);
    root.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    root.append(&bottom);
    dialog.set_child(Some(&root));

    // ── Populate list ──
    fn populate_list(list_box: &gtk::ListBox, configs: &[AgentConfig]) {
        while let Some(child) = list_box.first_child() {
            list_box.remove(&child);
        }
        for cfg in configs {
            let label = gtk::Label::new(Some(&cfg.name));
            label.set_xalign(0.0);
            label.set_margin_start(8);
            label.set_margin_end(8);
            label.set_margin_top(4);
            label.set_margin_bottom(4);
            list_box.append(&label);
        }
    }

    populate_list(&list_box, &configs.borrow());

    // ── Load detail from config ──
    fn load_detail(
        cfg: &AgentConfig,
        name_entry: &gtk::Entry,
        model_entry: &gtk::Entry,
        prompt_view: &gtk::TextView,
        tools_entry: &gtk::Entry,
    ) {
        name_entry.set_text(&cfg.name);
        model_entry.set_text(cfg.model.as_deref().unwrap_or(""));
        prompt_view.buffer().set_text(&cfg.system_prompt);
        tools_entry.set_text(&cfg.allowed_tools.join(", "));
    }

    // ── Save detail to config ──
    fn save_detail(
        configs: &Rc<RefCell<Vec<AgentConfig>>>,
        idx: usize,
        name_entry: &gtk::Entry,
        model_entry: &gtk::Entry,
        prompt_view: &gtk::TextView,
        tools_entry: &gtk::Entry,
        deleted_names: &Rc<RefCell<Vec<String>>>,
    ) {
        let mut cfgs = configs.borrow_mut();
        if idx >= cfgs.len() {
            return;
        }
        let old_name = cfgs[idx].name.clone();
        let new_name = name_entry.text().trim().to_string();
        let name = if new_name.is_empty() { old_name.clone() } else { new_name };

        if name != old_name {
            deleted_names.borrow_mut().push(old_name);
        }

        let model_text = model_entry.text().trim().to_string();
        let buf = prompt_view.buffer();
        let prompt = buf.text(&buf.start_iter(), &buf.end_iter(), false).to_string();
        let tools_text = tools_entry.text().to_string();
        let tools: Vec<String> = tools_text
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        cfgs[idx] = AgentConfig {
            name,
            system_prompt: prompt,
            allowed_tools: tools,
            model: if model_text.is_empty() { None } else { Some(model_text) },
        };
    }

    // Select first item
    if !configs.borrow().is_empty() {
        if let Some(row) = list_box.row_at_index(0) {
            list_box.select_row(Some(&row));
            *current_idx.borrow_mut() = Some(0);
            load_detail(
                &configs.borrow()[0],
                &name_entry,
                &model_entry,
                &prompt_view,
                &tools_entry,
            );
        }
    }

    // ── Selection change ──
    {
        let configs = Rc::clone(&configs);
        let deleted_names = Rc::clone(&deleted_names);
        let current_idx = Rc::clone(&current_idx);
        let name_entry = name_entry.clone();
        let model_entry = model_entry.clone();
        let prompt_view = prompt_view.clone();
        let tools_entry = tools_entry.clone();
        list_box.connect_row_selected(move |_, row| {
            let Some(row) = row else { return };
            let new_idx = row.index() as usize;

            // Save current before switching
            if let Some(old_idx) = *current_idx.borrow() {
                save_detail(&configs, old_idx, &name_entry, &model_entry, &prompt_view, &tools_entry, &deleted_names);
            }

            let cfgs = configs.borrow();
            if new_idx < cfgs.len() {
                load_detail(&cfgs[new_idx], &name_entry, &model_entry, &prompt_view, &tools_entry);
                *current_idx.borrow_mut() = Some(new_idx);
            }
        });
    }

    // ── Add button ──
    {
        let configs = Rc::clone(&configs);
        let deleted_names = Rc::clone(&deleted_names);
        let current_idx = Rc::clone(&current_idx);
        let list_box = list_box.clone();
        let name_entry = name_entry.clone();
        let model_entry = model_entry.clone();
        let prompt_view = prompt_view.clone();
        let tools_entry = tools_entry.clone();
        add_btn.connect_clicked(move |_| {
            // Save current
            if let Some(old_idx) = *current_idx.borrow() {
                save_detail(&configs, old_idx, &name_entry, &model_entry, &prompt_view, &tools_entry, &deleted_names);
            }

            // Find unique name
            let cfgs = configs.borrow();
            let mut n = 1;
            let name = loop {
                let candidate = format!("New Agent {n}");
                if !cfgs.iter().any(|c| c.name == candidate) {
                    break candidate;
                }
                n += 1;
            };
            drop(cfgs);

            let new_cfg = AgentConfig {
                name: name.clone(),
                system_prompt: String::new(),
                allowed_tools: Vec::new(),
                model: None,
            };
            configs.borrow_mut().push(new_cfg);
            populate_list(&list_box, &configs.borrow());

            let new_idx = configs.borrow().len() - 1;
            if let Some(row) = list_box.row_at_index(new_idx as i32) {
                list_box.select_row(Some(&row));
            }
        });
    }

    // ── Delete button ──
    {
        let configs = Rc::clone(&configs);
        let deleted_names = Rc::clone(&deleted_names);
        let current_idx = Rc::clone(&current_idx);
        let list_box = list_box.clone();
        let name_entry = name_entry.clone();
        let model_entry = model_entry.clone();
        let prompt_view = prompt_view.clone();
        let tools_entry = tools_entry.clone();
        del_btn.connect_clicked(move |_| {
            let Some(idx) = *current_idx.borrow() else { return };
            let cfgs_len = configs.borrow().len();
            if cfgs_len <= 1 {
                return; // Don't delete the last agent
            }

            let removed_name = configs.borrow()[idx].name.clone();
            deleted_names.borrow_mut().push(removed_name);
            configs.borrow_mut().remove(idx);

            populate_list(&list_box, &configs.borrow());

            let new_idx = if idx >= configs.borrow().len() {
                configs.borrow().len().saturating_sub(1)
            } else {
                idx
            };
            *current_idx.borrow_mut() = Some(new_idx);
            if let Some(row) = list_box.row_at_index(new_idx as i32) {
                list_box.select_row(Some(&row));
                load_detail(&configs.borrow()[new_idx], &name_entry, &model_entry, &prompt_view, &tools_entry);
            }
        });
    }

    // ── Cancel ──
    {
        let dialog = dialog.clone();
        cancel_btn.connect_clicked(move |_| {
            dialog.close();
        });
    }

    // ── Save ──
    {
        let dialog = dialog.clone();
        let configs = Rc::clone(&configs);
        let deleted_names = Rc::clone(&deleted_names);
        let current_idx = Rc::clone(&current_idx);
        let on_save = Rc::new(on_save);
        save_btn.connect_clicked(move |_| {
            // Save current form values
            if let Some(idx) = *current_idx.borrow() {
                save_detail(&configs, idx, &name_entry, &model_entry, &prompt_view, &tools_entry, &deleted_names);
            }

            // Delete removed configs from disk
            for name in deleted_names.borrow().iter() {
                session::delete_agent_config(name);
            }

            // Save all configs to disk
            let cfgs = configs.borrow();
            for cfg in cfgs.iter() {
                session::save_agent_config(cfg);
            }

            on_save(cfgs.clone());
            dialog.close();
        });
    }

    dialog.present();
}

fn labeled(text: &str, widget: &impl IsA<gtk::Widget>) -> gtk::Box {
    let bx = gtk::Box::new(gtk::Orientation::Vertical, 2);
    bx.append(&gtk::Label::builder().label(text).xalign(0.0).build());
    bx.append(widget);
    bx
}
