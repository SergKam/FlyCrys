use gtk::prelude::*;
use gtk4 as gtk;
use std::cell::RefCell;
use std::rc::Rc;

use crate::session::{self, Bookmark};

/// Open a master/detail dialog for CRUD on prompt bookmarks.
/// Calls `on_save` with the updated bookmark list when the user saves.
pub fn show(parent: &gtk::Window, on_save: impl Fn(Vec<Bookmark>) + 'static) {
    let bookmarks: Rc<RefCell<Vec<Bookmark>>> = Rc::new(RefCell::new(session::load_bookmarks()));
    let current_idx: Rc<RefCell<Option<usize>>> = Rc::new(RefCell::new(None));

    let dialog = gtk::Window::builder()
        .title("Configure Bookmarks")
        .modal(true)
        .transient_for(parent)
        .default_width(600)
        .default_height(420)
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
    add_btn.set_tooltip_text(Some("New bookmark"));
    let del_btn = gtk::Button::from_icon_name("list-remove-symbolic");
    del_btn.set_tooltip_text(Some("Delete bookmark"));
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
    name_entry.set_placeholder_text(Some("Bookmark name"));
    detail.append(&labeled("Name", &name_entry));

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
    prompt_box.append(&gtk::Label::builder().label("Prompt").xalign(0.0).build());
    prompt_box.append(&prompt_frame);
    detail.append(&prompt_box);

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
    fn populate_list(list_box: &gtk::ListBox, bookmarks: &[Bookmark]) {
        while let Some(child) = list_box.first_child() {
            list_box.remove(&child);
        }
        for bm in bookmarks {
            let label = gtk::Label::new(Some(&bm.name));
            label.set_xalign(0.0);
            label.set_margin_start(8);
            label.set_margin_end(8);
            label.set_margin_top(4);
            label.set_margin_bottom(4);
            list_box.append(&label);
        }
    }

    populate_list(&list_box, &bookmarks.borrow());

    // ── Load / save detail ──
    fn load_detail(bm: &Bookmark, name_entry: &gtk::Entry, prompt_view: &gtk::TextView) {
        name_entry.set_text(&bm.name);
        prompt_view.buffer().set_text(&bm.prompt);
    }

    fn save_detail(
        bookmarks: &Rc<RefCell<Vec<Bookmark>>>,
        idx: usize,
        name_entry: &gtk::Entry,
        prompt_view: &gtk::TextView,
    ) {
        let mut bms = bookmarks.borrow_mut();
        if idx >= bms.len() {
            return;
        }
        let new_name = name_entry.text().trim().to_string();
        let name = if new_name.is_empty() {
            bms[idx].name.clone()
        } else {
            new_name
        };
        let buf = prompt_view.buffer();
        let prompt = buf
            .text(&buf.start_iter(), &buf.end_iter(), false)
            .to_string();
        bms[idx] = Bookmark { name, prompt };
    }

    // Select first item
    if !bookmarks.borrow().is_empty()
        && let Some(row) = list_box.row_at_index(0)
    {
        list_box.select_row(Some(&row));
        *current_idx.borrow_mut() = Some(0);
        load_detail(&bookmarks.borrow()[0], &name_entry, &prompt_view);
    }

    // ── Selection change ──
    {
        let bookmarks = Rc::clone(&bookmarks);
        let current_idx = Rc::clone(&current_idx);
        let name_entry = name_entry.clone();
        let prompt_view = prompt_view.clone();
        list_box.connect_row_selected(move |_, row| {
            let Some(row) = row else { return };
            let new_idx = row.index() as usize;

            // Save current before switching
            if let Some(old_idx) = *current_idx.borrow() {
                save_detail(&bookmarks, old_idx, &name_entry, &prompt_view);
            }

            let bms = bookmarks.borrow();
            if new_idx < bms.len() {
                load_detail(&bms[new_idx], &name_entry, &prompt_view);
                *current_idx.borrow_mut() = Some(new_idx);
            }
        });
    }

    // ── Add button ──
    {
        let bookmarks = Rc::clone(&bookmarks);
        let current_idx = Rc::clone(&current_idx);
        let list_box = list_box.clone();
        let name_entry = name_entry.clone();
        let prompt_view = prompt_view.clone();
        add_btn.connect_clicked(move |_| {
            // Save current
            if let Some(old_idx) = *current_idx.borrow() {
                save_detail(&bookmarks, old_idx, &name_entry, &prompt_view);
            }

            // Find unique name
            let bms = bookmarks.borrow();
            let mut n = 1;
            let name = loop {
                let candidate = format!("New Bookmark {n}");
                if !bms.iter().any(|b| b.name == candidate) {
                    break candidate;
                }
                n += 1;
            };
            drop(bms);

            bookmarks.borrow_mut().push(Bookmark {
                name,
                prompt: String::new(),
            });
            populate_list(&list_box, &bookmarks.borrow());

            let new_idx = bookmarks.borrow().len() - 1;
            if let Some(row) = list_box.row_at_index(new_idx as i32) {
                list_box.select_row(Some(&row));
            }
        });
    }

    // ── Delete button ──
    {
        let bookmarks = Rc::clone(&bookmarks);
        let current_idx = Rc::clone(&current_idx);
        let list_box = list_box.clone();
        let name_entry = name_entry.clone();
        let prompt_view = prompt_view.clone();
        del_btn.connect_clicked(move |_| {
            let Some(idx) = *current_idx.borrow() else {
                return;
            };
            if bookmarks.borrow().is_empty() {
                return;
            }

            bookmarks.borrow_mut().remove(idx);
            populate_list(&list_box, &bookmarks.borrow());

            let len = bookmarks.borrow().len();
            if len == 0 {
                *current_idx.borrow_mut() = None;
                name_entry.set_text("");
                prompt_view.buffer().set_text("");
            } else {
                let new_idx = idx.min(len - 1);
                *current_idx.borrow_mut() = Some(new_idx);
                if let Some(row) = list_box.row_at_index(new_idx as i32) {
                    list_box.select_row(Some(&row));
                    load_detail(&bookmarks.borrow()[new_idx], &name_entry, &prompt_view);
                }
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
        let bookmarks = Rc::clone(&bookmarks);
        let current_idx = Rc::clone(&current_idx);
        let on_save = Rc::new(on_save);
        save_btn.connect_clicked(move |_| {
            // Save current form values
            if let Some(idx) = *current_idx.borrow() {
                save_detail(&bookmarks, idx, &name_entry, &prompt_view);
            }

            // Persist to disk
            let bms = bookmarks.borrow();
            session::save_bookmarks(&bms);

            on_save(bms.clone());
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
