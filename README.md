# FlyCristal

A native GTK4 file viewer built in Rust. Foundation for a Claude Code GUI interface.

## Layout

```
+------------+-----------------------------+
|            |     Text View (file content)|
|  File Tree |                             |
|  (left)    |-----------------------------|
|            |     Terminal (on demand)     |
+------------+-----------------------------+
```

## Features

- **File tree** with folder/file icons, lazy-loaded on expand
- **Single-click** to open files or toggle directories
- **Text viewer** with monospace font, read-only, 10 MB file size guard
- **Right-click context menu** on tree items:
  - Copy Path (to clipboard)
  - Open Terminal Here (VTE terminal in bottom panel)
- **Embedded terminal** using VTE4, spawns `$SHELL` in the selected directory
- Directories sorted first, hidden files excluded

## Prerequisites

```bash
# Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# System libraries (Ubuntu/Debian)
sudo apt install libgtk-4-dev libvte-2.91-gtk4-dev
```

## Build & Run

```bash
cargo build
cargo run
```

## Project Structure

```
src/
  main.rs       - App bootstrap, window layout, signal wiring, context menu
  tree.rs       - File tree (TreeStore/TreeView), lazy-load, sorting
  textview.rs   - Read-only monospace text viewer with path header
  terminal.rs   - VTE4 terminal panel with close button
```

## Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| gtk4  | 0.10    | GTK4 Rust bindings (UI toolkit) |
| vte4  | 0.9     | VTE terminal emulator widget |

## Roadmap

- [ ] Right panel: chat interface
- [ ] Syntax highlighting
- [ ] File search / fuzzy finder
- [ ] Tab support for multiple open files
