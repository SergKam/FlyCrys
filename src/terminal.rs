use gtk4 as gtk;
use gtk::glib;
use gtk::prelude::*;
use vte4::prelude::*;

pub fn create_terminal_panel() -> (gtk::Box, vte4::Terminal) {
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    header.set_margin_start(8);
    header.set_margin_end(4);
    header.set_margin_top(2);
    header.set_margin_bottom(2);

    let label = gtk::Label::new(Some("Terminal"));
    label.set_hexpand(true);
    label.set_xalign(0.0);

    let close_btn = gtk::Button::from_icon_name("window-close-symbolic");
    close_btn.set_has_frame(false);
    close_btn.set_tooltip_text(Some("Close terminal"));

    header.append(&label);
    header.append(&close_btn);

    let terminal = vte4::Terminal::new();
    terminal.set_vexpand(true);
    terminal.set_hexpand(true);
    terminal.set_scrollback_lines(10000);

    let font_desc = gtk::pango::FontDescription::from_string("Monospace 11");
    terminal.set_font(Some(&font_desc));

    vbox.append(&header);
    vbox.append(&terminal);

    // Close button hides the panel
    close_btn.connect_clicked(glib::clone!(
        #[weak] vbox,
        move |_| {
            vbox.set_visible(false);
        }
    ));

    (vbox, terminal)
}

pub fn spawn_shell(terminal: &vte4::Terminal, working_directory: &str) {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());

    terminal.spawn_async(
        vte4::PtyFlags::DEFAULT,
        Some(working_directory),
        &[shell.as_str()],
        &[] as &[&str],
        glib::SpawnFlags::DEFAULT,
        || {},
        -1,
        None::<&gtk::gio::Cancellable>,
        |_result| {},
    );
}
