mod file_entry;
mod terminal;
mod textview;
mod tree;

use file_entry::FileEntry;
use gtk4 as gtk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

const APP_ID: &str = "com.flycristal.app";

fn main() -> glib::ExitCode {
    let app = gtk::Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &gtk::Application) {
    // Left pane: file tree
    let (tree_scroll, list_view, selection) = tree::create_file_tree();

    // Right pane top: text view
    let (text_container, text_view, path_label) = textview::create_text_view();

    // Right pane bottom: terminal (initially hidden)
    let (terminal_container, vte_terminal) = terminal::create_terminal_panel();
    terminal_container.set_visible(false);

    let right_paned = gtk::Paned::new(gtk::Orientation::Vertical);
    right_paned.set_start_child(Some(&text_container));
    right_paned.set_end_child(Some(&terminal_container));
    right_paned.set_resize_start_child(true);
    right_paned.set_resize_end_child(true);
    right_paned.set_shrink_start_child(false);
    right_paned.set_shrink_end_child(false);

    // Main horizontal split
    let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
    paned.set_position(300);
    paned.set_start_child(Some(&tree_scroll));
    paned.set_end_child(Some(&right_paned));

    // Single-click: open files / toggle directories
    list_view.connect_activate(glib::clone!(
        #[weak] selection,
        #[weak] text_view,
        #[weak] path_label,
        move |_view, position| {
            let Some(item) = selection.item(position) else {
                return;
            };
            let row = item.downcast_ref::<gtk::TreeListRow>().unwrap();
            let entry = row.item().and_downcast::<FileEntry>().unwrap();

            if entry.is_dir() {
                row.set_expanded(!row.is_expanded());
            } else {
                textview::load_file(&text_view, &path_label, &entry.path());
            }
        }
    ));

    // --- Right-click context menu ---
    let ctx_path: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
    let ctx_is_dir: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));

    let menu = gio::Menu::new();
    menu.append(Some("Copy Path"), Some("win.copy-path"));
    menu.append(Some("Open Terminal Here"), Some("win.open-terminal-here"));

    let popover = gtk::PopoverMenu::from_model(Some(&menu));
    popover.set_parent(&list_view);
    popover.set_has_arrow(false);

    let right_click = gtk::GestureClick::new();
    right_click.set_button(3);
    right_click.connect_pressed(glib::clone!(
        #[weak] list_view,
        #[weak] popover,
        #[strong] ctx_path,
        #[strong] ctx_is_dir,
        move |_gesture, _n_press, x, y| {
            // Walk up from the picked widget to find a TreeExpander
            let Some(widget) = list_view.pick(x, y, gtk::PickFlags::DEFAULT) else {
                return;
            };
            let mut current = Some(widget);
            while let Some(w) = current {
                if let Some(expander) = w.downcast_ref::<gtk::TreeExpander>() {
                    if let Some(item) = expander.item() {
                        if let Some(entry) = item.downcast_ref::<FileEntry>() {
                            *ctx_path.borrow_mut() = entry.path();
                            *ctx_is_dir.borrow_mut() = entry.is_dir();

                            popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(
                                x as i32, y as i32, 1, 1,
                            )));
                            popover.popup();
                        }
                    }
                    return;
                }
                current = w.parent();
            }
        }
    ));
    list_view.add_controller(right_click);

    // Window
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("FlyCristal")
        .default_width(1200)
        .default_height(800)
        .child(&paned)
        .build();

    // Action: Copy Path
    let action_copy = gio::SimpleAction::new("copy-path", None);
    action_copy.connect_activate(glib::clone!(
        #[weak] window,
        #[strong] ctx_path,
        move |_, _| {
            let path = ctx_path.borrow().clone();
            if !path.is_empty() {
                window.clipboard().set_text(&path);
            }
        }
    ));
    window.add_action(&action_copy);

    // Action: Open Terminal Here
    let action_terminal = gio::SimpleAction::new("open-terminal-here", None);
    action_terminal.connect_activate(glib::clone!(
        #[weak] terminal_container,
        #[weak] vte_terminal,
        #[strong] ctx_path,
        #[strong] ctx_is_dir,
        move |_, _| {
            let path = ctx_path.borrow().clone();
            let is_dir = *ctx_is_dir.borrow();
            if path.is_empty() {
                return;
            }
            let dir = if is_dir {
                path
            } else {
                std::path::Path::new(&path)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| "/".to_string())
            };
            terminal_container.set_visible(true);
            terminal::spawn_shell(&vte_terminal, &dir);
        }
    ));
    window.add_action(&action_terminal);

    window.present();
}
