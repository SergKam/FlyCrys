use gtk::glib;
use gtk::prelude::*;
use gtk4 as gtk;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

pub struct GitPanel {
    pub container: gtk::Box,
    working_dir: PathBuf,
    list_box: gtk::ListBox,
    on_open_file: Rc<dyn Fn(&str)>,
}

impl GitPanel {
    pub fn new(working_dir: &Path, on_open_file: Rc<dyn Fn(&str)>) -> Option<Self> {
        // Check if this is a git repo
        let out = Command::new("git")
            .args(["-C", &working_dir.to_string_lossy(), "rev-parse", "--git-dir"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }

        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        container.set_vexpand(false);

        let header = gtk::Label::new(Some("Changes"));
        header.set_xalign(0.0);
        header.set_margin_start(8);
        header.set_margin_top(4);
        header.set_margin_bottom(2);
        header.add_css_class("heading");

        let list_box = gtk::ListBox::new();
        list_box.set_selection_mode(gtk::SelectionMode::None);

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .max_content_height(150)
            .propagate_natural_height(true)
            .child(&list_box)
            .build();

        container.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        container.append(&header);
        container.append(&scrolled);

        let panel = GitPanel {
            container,
            working_dir: working_dir.to_path_buf(),
            list_box,
            on_open_file,
        };
        panel.refresh();
        Some(panel)
    }

    pub fn refresh(&self) {
        // Clear existing rows
        while let Some(child) = self.list_box.first_child() {
            self.list_box.remove(&child);
        }

        let output = Command::new("git")
            .args([
                "-C",
                &self.working_dir.to_string_lossy(),
                "status",
                "--porcelain",
            ])
            .output();

        let lines = match output {
            Ok(out) => String::from_utf8_lossy(&out.stdout).to_string(),
            Err(_) => {
                self.container.set_visible(false);
                return;
            }
        };

        let entries: Vec<(String, String)> = lines
            .lines()
            .filter(|l| l.len() >= 3)
            .map(|l| {
                let status = l[..2].trim().to_string();
                let path = l[3..].to_string();
                (status, path)
            })
            .collect();

        if entries.is_empty() {
            self.container.set_visible(false);
            return;
        }

        self.container.set_visible(true);

        for (status, path) in &entries {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            row.set_margin_start(8);
            row.set_margin_end(8);
            row.set_margin_top(1);
            row.set_margin_bottom(1);

            let status_label = gtk::Label::new(Some(status));
            status_label.set_width_chars(2);
            status_label.add_css_class("monospace");
            match status.as_str() {
                "M" | "MM" => status_label.add_css_class("git-modified"),
                "A" | "AM" => status_label.add_css_class("git-added"),
                "D" => status_label.add_css_class("git-deleted"),
                "??" => status_label.add_css_class("git-untracked"),
                _ => status_label.add_css_class("git-modified"),
            }

            let path_label = gtk::Label::new(Some(path));
            path_label.set_xalign(0.0);
            path_label.set_hexpand(true);
            path_label.set_ellipsize(gtk::pango::EllipsizeMode::Start);
            path_label.add_css_class("monospace");

            row.append(&status_label);
            row.append(&path_label);

            let full_path = self.working_dir.join(path).to_string_lossy().to_string();
            let on_open = self.on_open_file.clone();
            let gesture = gtk::GestureClick::new();
            gesture.set_button(1);
            gesture.connect_pressed(move |gesture, _, _, _| {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                on_open(&full_path);
            });
            row.add_controller(gesture);

            self.list_box.append(&row);
        }
    }
}

/// Start a 5-second refresh timer for the git panel.
pub fn start_refresh_timer(panel: &Rc<RefCell<GitPanel>>) {
    let panel = Rc::clone(panel);
    glib::timeout_add_local(std::time::Duration::from_secs(5), move || {
        panel.borrow().refresh();
        glib::ControlFlow::Continue
    });
}

/// Check if a file has uncommitted git changes.
pub fn is_file_modified(file_path: &str, working_dir: &Path) -> bool {
    let rel = Path::new(file_path)
        .strip_prefix(working_dir)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| file_path.to_string());

    let output = Command::new("git")
        .args([
            "-C",
            &working_dir.to_string_lossy(),
            "status",
            "--porcelain",
            "--",
            &rel,
        ])
        .output();

    match output {
        Ok(out) => !out.stdout.is_empty(),
        Err(_) => false,
    }
}

/// Get the git diff for a file. Returns None if no changes.
pub fn get_file_diff(file_path: &str, working_dir: &Path) -> Option<String> {
    let rel = Path::new(file_path)
        .strip_prefix(working_dir)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| file_path.to_string());
    let wd = working_dir.to_string_lossy();

    // Try HEAD (all uncommitted changes)
    if let Some(diff) = run_git_diff(&wd, &["diff", "HEAD", "--", &rel]) {
        return Some(diff);
    }
    // Try unstaged only
    if let Some(diff) = run_git_diff(&wd, &["diff", "--", &rel]) {
        return Some(diff);
    }
    // Try staged only
    run_git_diff(&wd, &["diff", "--cached", "--", &rel])
}

fn run_git_diff(wd: &str, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(wd)
        .args(args)
        .output()
        .ok()?;
    if output.stdout.is_empty() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// Load diff text into a TextBuffer with colored tags.
pub fn load_diff_into_buffer(buffer: &gtk::TextBuffer, diff: &str, is_dark: bool) {
    buffer.set_text(diff);

    let tag_table = buffer.tag_table();

    // Remove old diff tags if they exist
    for name in &["diff-add", "diff-remove", "diff-header"] {
        if let Some(tag) = tag_table.lookup(name) {
            tag_table.remove(&tag);
        }
    }

    let (add_color, remove_color, header_color) = if is_dark {
        ("#57e389", "#ff7b63", "#99c1f1")
    } else {
        ("#26a269", "#c01c28", "#1a5fb4")
    };

    let add_tag = gtk::TextTag::builder()
        .name("diff-add")
        .foreground(add_color)
        .build();
    tag_table.add(&add_tag);

    let remove_tag = gtk::TextTag::builder()
        .name("diff-remove")
        .foreground(remove_color)
        .build();
    tag_table.add(&remove_tag);

    let header_tag = gtk::TextTag::builder()
        .name("diff-header")
        .foreground(header_color)
        .weight(700)
        .build();
    tag_table.add(&header_tag);

    for (i, line) in diff.lines().enumerate() {
        let Some(start) = buffer.iter_at_line(i as i32) else {
            continue;
        };
        let mut end = start;
        end.forward_to_line_end();

        let tag_name = if line.starts_with("@@")
            || line.starts_with("+++")
            || line.starts_with("---")
            || line.starts_with("diff ")
            || line.starts_with("index ")
        {
            Some("diff-header")
        } else if line.starts_with('+') {
            Some("diff-add")
        } else if line.starts_with('-') {
            Some("diff-remove")
        } else {
            None
        };

        if let Some(name) = tag_name {
            buffer.apply_tag_by_name(name, &start, &end);
        }
    }
}
