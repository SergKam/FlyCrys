# Snapcraft / Snap Store Submission Guide for FlyCrys

## Overview

Snaps are universal Linux packages distributed via the Snap Store (snapcraft.io).
Packaging involves a `snapcraft.yaml` file and building with the `snapcraft` CLI tool.
FlyCrys, as a GTK4 desktop app that needs to execute Claude Code (which itself needs
filesystem and network access), will likely need **classic confinement**.

**Snap name:** `flycrys`

---

## snapcraft.yaml

### Complete Template for FlyCrys

```yaml
name: flycrys
version: '0.2.1'
title: FlyCrys
summary: Lightning-fast, Linux-native UI for Claude Code AI agent
description: |
  FlyCrys is a GTK4-native Linux desktop application that provides a
  graphical interface for Claude Code, Anthropic's AI coding agent CLI.
  Features multi-workspace tabs, file tree browser, syntax-highlighted
  viewer, and embedded terminal.

  Requires Claude Code CLI (npm install -g @anthropic-ai/claude-code)
  to be installed on the host system.

base: core24
grade: stable
confinement: classic
license: MIT

contact: https://github.com/SergKam/FlyCrys/issues
website: https://github.com/SergKam/FlyCrys
source-code: https://github.com/SergKam/FlyCrys
issues: https://github.com/SergKam/FlyCrys/issues

architectures:
  - build-on: [amd64]
    build-for: [amd64]

apps:
  flycrys:
    command: bin/flycrys
    desktop: share/applications/flycrys.desktop
    # Classic confinement: no plugs needed (full host access)

parts:
  flycrys:
    plugin: rust
    source: .
    source-type: local
    # Or for remote builds:
    # source: https://github.com/SergKam/FlyCrys.git
    # source-tag: v0.2.1
    build-packages:
      - pkg-config
      - libgtk-4-dev
      - libvte-2.91-gtk4-dev
      - libwebkitgtk-6.0-dev
      - libadwaita-1-dev
    stage-packages:
      - libgtk-4-1
      - libvte-2.91-gtk4-0
      - libwebkitgtk-6.0-4
      - libadwaita-1-0
    override-build: |
      snapcraftctl build
      # Install desktop file and icons
      install -Dm644 $SNAPCRAFT_PART_SRC/com.flycrys.app.desktop \
        $SNAPCRAFT_PART_INSTALL/share/applications/flycrys.desktop
      install -Dm644 $SNAPCRAFT_PART_SRC/data/icons/hicolor/256x256/apps/flycrys.png \
        $SNAPCRAFT_PART_INSTALL/share/icons/hicolor/256x256/apps/flycrys.png
      install -Dm644 $SNAPCRAFT_PART_SRC/data/icons/hicolor/512x512/apps/flycrys.png \
        $SNAPCRAFT_PART_INSTALL/share/icons/hicolor/512x512/apps/flycrys.png
```

### Strict Confinement Alternative

If trying strict confinement instead of classic (more limited but easier review):

```yaml
confinement: strict

apps:
  flycrys:
    command: bin/flycrys
    desktop: share/applications/flycrys.desktop
    extensions: [gnome]
    plugs:
      - home
      - network
      - desktop
      - desktop-legacy
      - wayland
      - x11
      - opengl
      - process-control
```

**Warning:** Strict confinement will likely break FlyCrys because Claude Code CLI
needs unrestricted filesystem and process access. Classic confinement is the
practical choice.

---

## Confinement Levels

| Level | Description | Review Required |
|---|---|---|
| `devmode` | No restrictions, for development only. Not published to stable channel. | No |
| `strict` | Full sandbox. Only declared interfaces (plugs/slots) are allowed. | Automated review |
| `classic` | No sandbox. Full host access, like a .deb package. | Manual review required |

### Why FlyCrys Needs Classic Confinement

1. Claude Code CLI runs arbitrary commands on the host filesystem
2. VTE terminal emulator needs unrestricted PTY and process spawning
3. User projects can be anywhere on the filesystem
4. Claude Code itself may need to install/run npm, node, etc. from host paths

---

## Rust Plugin Configuration

The Snapcraft Rust plugin (for core24/core22) supports:

| Key | Description |
|---|---|
| `rust-features` | List of cargo features to enable |
| `rust-path` | Relative path to crate in workspace (default: `.`) |

The plugin automatically:
- Detects `rust-toolchain` files
- Downloads stable Rust toolchain
- Runs `cargo build --release`

---

## Submission Process (Step-by-Step)

### 1. Install Snapcraft

```bash
sudo snap install snapcraft --classic
```

### 2. Register Your Snap Name

```bash
# Create account at https://snapcraft.io/account
snapcraft login
snapcraft register flycrys
```

Name rules: lowercase, 40 chars max, letters + numbers + hyphens, at least one letter.

### 3. Build the Snap

```bash
cd /path/to/flycrys
snapcraft
# Produces flycrys_0.2.1_amd64.snap
```

Build happens in a clean LXD or Multipass VM by default.

### 4. Test Locally

```bash
# For classic snaps:
sudo snap install flycrys_0.2.1_amd64.snap --classic --dangerous
flycrys
```

### 5. Upload to Snap Store

```bash
snapcraft upload flycrys_0.2.1_amd64.snap --release=edge
```

Channels: `edge` -> `beta` -> `candidate` -> `stable`

Start with `edge`, promote after testing:
```bash
snapcraft release flycrys 1 beta
snapcraft release flycrys 1 stable
```

### 6. Request Classic Confinement Review

1. Go to https://forum.snapcraft.io
2. Create a post in the **store-requests** category
3. Title: "Classic confinement request: flycrys"
4. Explain why classic is needed:
   - FlyCrys wraps Claude Code CLI which executes arbitrary shell commands
   - VTE terminal emulator spawns processes on the host
   - Users need to access project files anywhere on the filesystem
   - Similar to IDEs and terminal emulators that have classic confinement
5. Provide publisher information for vetting

### 7. Wait for Review

- Review should start within ~2 weeks of filing the request
- Timeline depends on completeness of justification and publisher vetting
- Snapcrafters team members are considered pre-vetted
- Once approved, a snap declaration is issued and subsequent uploads do not need re-review

---

## GTK4/VTE4/WebKitGTK-Specific Gotchas

1. **Build dependencies:** GTK4, VTE4, and WebKitGTK dev packages are large. Builds will be slow.
2. **GNOME extension:** For strict confinement, the `gnome` extension provides GTK4 runtime, themes, and desktop integration. Not needed for classic.
3. **Library bundling:** With classic confinement, the snap can access host libraries, but it is still recommended to bundle dependencies via `stage-packages` for portability across distros.
4. **WebKitGTK sandbox:** WebKitGTK has its own internal sandbox (bubblewrap). Inside a strict snap, this can conflict with the snap sandbox. Classic avoids this issue.
5. **Icon theme:** The snap's desktop file should reference the icon by name (not path). Place the icon in the standard hicolor location within the snap.
6. **core24 vs core22:** Use `core24` for the latest Ubuntu 24.04 base which has newer GTK4/VTE4 packages.

---

## Timeline Expectations

| Phase | Duration |
|---|---|
| Name registration | Instant (if name is available) |
| First build | 30-60 min (dependency download + compile) |
| Upload to edge | Minutes |
| Automated review (strict) | Minutes to hours |
| Classic confinement request | 2+ weeks for initial response |
| Classic confinement approval | Weeks to months |
| Promotion to stable | Immediate once approved |

---

## Post-Acceptance Maintenance

- **Updates:** Build new snap, upload, release to channels
- **Automated builds:** Set up CI (GitHub Actions) to build and upload on tag push
- **No external review needed** for updates after initial classic approval
- **Monitor:** Snap Store dashboard shows install counts, errors, and reviews
- **Multiple architectures:** Can add `arm64` builds later
- **Maintenance burden:** Low -- just rebuild and upload on new releases

### GitHub Actions Example

```yaml
name: Snap Build
on:
  push:
    tags: ['v*']

jobs:
  snap:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: snapcore/action-build@v1
        id: build
      - uses: snapcore/action-publish@v1
        env:
          SNAPCRAFT_STORE_CREDENTIALS: ${{ secrets.SNAP_TOKEN }}
        with:
          snap: ${{ steps.build.outputs.snap }}
          release: edge
```
