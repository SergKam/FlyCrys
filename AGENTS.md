# AGENTS.md - FlyCristal

Instructions for AI agents working on this codebase.

## Rules

- **NEVER use deprecated GTK4 APIs.** No TreeView, TreeStore, CellRenderer*, ListStore (deprecated one). Use modern replacements: ListView, ColumnView, TreeListModel, SignalListItemFactory, TreeExpander, gio::ListStore. No `#[allow(deprecated)]` annotations.

## Tech Stack

- **Language**: Rust (edition 2024)
- **UI toolkit**: GTK4 via `gtk4` crate (v0.10) with `v4_12` feature
- **Terminal**: VTE4 via `vte4` crate (v0.9)
- **System deps**: `libgtk-4-dev`, `libvte-2.91-gtk4-dev`

## Architecture

Single-window GTK4 application. Uses one GObject subclass (`FileEntry`) for the file tree model; all other widgets are composed directly in `build_ui()`.

### Module Responsibilities

- **`main.rs`** — App entry point. Creates window, horizontal Paned layout, wires ListView activate signal (single-click), right-click context menu (GestureClick + PopoverMenu), window actions (copy-path, open-terminal-here).
- **`file_entry.rs`** — GObject subclass with properties: `name`, `path`, `icon_name`, `is_dir`. Used as item type in `gio::ListStore` for the tree model. Uses `#[derive(glib::Properties)]` + `#[glib::derived_properties]`.
- **`tree.rs`** — File tree using `ListView` + `TreeListModel` + `SignalListItemFactory` + `TreeExpander`. Lazy-loads via TreeListModel's `create_func`. Sorts directories first, skips hidden files.
- **`textview.rs`** — Read-only monospace `TextView` with path label header. 10 MB size guard.
- **`terminal.rs`** — VTE4 terminal in a `Box` with close button. `spawn_shell()` starts `$SHELL` in a given directory.

### Layout Hierarchy

```
ApplicationWindow
  Paned (Horizontal)
    ScrolledWindow            # left pane
      ListView
        TreeListModel
          gio::ListStore<FileEntry>
    Paned (Vertical)          # right pane
      Box                     # text container
        Label                 # file path
        ScrolledWindow
          TextView
      Box                     # terminal container (initially hidden)
        Box                   # header with close button
        vte4::Terminal
```

### Key Patterns

- **GObject model**: `FileEntry` is a GObject subclass (`mod imp` pattern with `ObjectSubclass`, `Properties` derive, `glib::wrapper!`). Required because `gio::ListStore` needs `IsA<glib::Object>` items.
- **TreeListModel lazy loading**: `TreeListModel::new(root, false, false, create_func)` where `create_func` returns `Some(child_ListStore)` for directories, `None` for files. Called automatically on expand — no manual expand/collapse handlers needed.
- **ListView item access**: With `passthrough: false`, `list_item.item()` returns a `TreeListRow`, NOT the `FileEntry`. Chain: `TreeListRow::item() -> FileEntry`. The `TreeExpander::item()` returns the unwrapped `FileEntry` directly.
- **Single-click**: `list_view.set_single_click_activate(true)` + `connect_activate(|view, position|)`. Toggle expand with `row.set_expanded(!row.is_expanded())`.
- **Right-click context menu**: `GestureClick` (button 3) + `Widget::pick(x, y)` + walk up parents to find `TreeExpander` + read `FileEntry` from it. `PopoverMenu` + `gio::SimpleAction` on window.
- **Signal closures**: `glib::clone!` macro with `#[weak]` for GObject references, `#[strong]` for `Rc<RefCell<>>`.

## Build Commands

```bash
cargo build
cargo run
cargo build --release
```

## Conventions

- Use `gtk4 as gtk` alias throughout
- Import `gtk::prelude::*` for extension traits
- Use `glib::clone!` macro with `#[weak]`/`#[strong]` for signal closures
- GObject property names use kebab-case in builder (e.g., `"icon-name"`, `"is-dir"`)
- No `unsafe` code — all GTK FFI goes through safe bindings
- Keep modules focused: one widget concept per file

## When Making Changes

- Run `cargo build` after every change — GTK binding errors can be cryptic
- If adding new GTK4 features, check if they require a version feature flag (e.g., `v4_14`)
- VTE `spawn_async` requires a `child_setup: impl Fn()` parameter before `timeout`
- The terminal panel starts hidden; "Open Terminal Here" makes it visible and spawns a shell
- When creating new list/tree widgets, use ListView/ColumnView + gio::ListStore + SignalListItemFactory
