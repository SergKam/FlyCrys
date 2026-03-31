# Flathub Submission Guide for FlyCrys

## Overview

Flathub is the primary Flatpak app store for Linux. Submissions are made via PR to the
flathub GitHub organization. All reviewers are volunteers, so timelines vary.

**App ID for FlyCrys:** `io.github.SergKam.FlyCrys`
(Uses `io.github.` prefix since the repo is hosted on GitHub; must have at least 4 components.)

---

## Required Files

### 1. Flatpak Manifest (`io.github.SergKam.FlyCrys.yml`)

Must be at repository root, named after the app ID with `.json`, `.yml`, or `.yaml` extension.

```yaml
id: io.github.SergKam.FlyCrys
runtime: org.gnome.Platform
runtime-version: '47'
sdk: org.gnome.Sdk
sdk-extensions:
  - org.freedesktop.Sdk.Extension.rust-stable
command: flycrys

finish-args:
  # Display
  - --socket=wayland
  - --socket=fallback-x11
  - --share=ipc
  - --device=dri
  # Terminal (VTE) needs PTY access
  - --talk-name=org.freedesktop.Flatpak
  # Network for WebKitGTK and Claude Code API
  - --share=network
  # Access to user projects (Claude Code operates on files)
  - --filesystem=home
  # For Claude Code CLI
  - --filesystem=host-os:ro
  # DBus for desktop integration
  - --talk-name=org.freedesktop.Notifications

build-options:
  append-path: /usr/lib/sdk/rust-stable/bin
  env:
    CARGO_HOME: /run/build/flycrys/cargo
    RUST_BACKTRACE: '1'

modules:
  - name: flycrys
    buildsystem: simple
    build-commands:
      - cargo --offline fetch --manifest-path Cargo.toml --verbose
      - cargo --offline build --release --verbose
      - install -Dm755 target/release/flycrys /app/bin/flycrys
      - install -Dm644 com.flycrys.app.desktop /app/share/applications/io.github.SergKam.FlyCrys.desktop
      - install -Dm644 data/icons/hicolor/128x128/apps/flycrys.png /app/share/icons/hicolor/128x128/apps/io.github.SergKam.FlyCrys.png
      - install -Dm644 data/icons/hicolor/256x256/apps/flycrys.png /app/share/icons/hicolor/256x256/apps/io.github.SergKam.FlyCrys.png
      - install -Dm644 data/icons/hicolor/512x512/apps/flycrys.png /app/share/icons/hicolor/512x512/apps/io.github.SergKam.FlyCrys.png
      - install -Dm644 io.github.SergKam.FlyCrys.metainfo.xml /app/share/metainfo/io.github.SergKam.FlyCrys.metainfo.xml
    sources:
      - type: archive
        url: https://github.com/SergKam/FlyCrys/archive/refs/tags/v0.2.1.tar.gz
        sha256: FILL_IN_HASH
      # Generated with flatpak-cargo-generator.py
      - flycrys-cargo-sources.json
```

**Generating cargo sources for offline build:**

```bash
# Install the generator
pip install aiohttp toml
wget https://raw.githubusercontent.com/nickel-lang/nickel/main/scripts/flatpak-cargo-generator.py
# Or use the one from flathub-infra:
wget https://raw.githubusercontent.com/nickel-lang/nickel/main/scripts/flatpak-cargo-generator.py

# More commonly used:
pip install flatpak-builder-tools
# Or clone https://github.com/nickel-lang/nickel and use their script

# Generate from Cargo.lock
python3 flatpak-cargo-generator.py ./Cargo.lock -o flycrys-cargo-sources.json
```

The tool from `https://github.com/nickel-lang/nickel` or `flatpak-builder-tools` reads
`Cargo.lock` and produces a JSON manifest listing every crate as a Flatpak source.

### 2. MetaInfo File (`io.github.SergKam.FlyCrys.metainfo.xml`)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!-- Copyright 2026 Sergii Kamenskyi -->
<component type="desktop-application">
  <id>io.github.SergKam.FlyCrys</id>
  <metadata_license>CC0-1.0</metadata_license>
  <project_license>MIT</project_license>

  <name>FlyCrys</name>
  <summary>Lightning-fast, Linux-native UI for Claude Code AI agent</summary>

  <developer id="io.github.SergKam">
    <name>Sergii Kamenskyi</name>
  </developer>

  <description>
    <p>
      FlyCrys is a GTK4-native Linux desktop application that provides a
      graphical interface for Claude Code, Anthropic's AI coding agent CLI.
    </p>
    <p>
      Features include multi-workspace tabs, a file tree browser with
      syntax-highlighted viewer, embedded terminal with VTE4, and a
      WebKitGTK-powered markdown rendering panel.
    </p>
  </description>

  <launchable type="desktop-id">io.github.SergKam.FlyCrys.desktop</launchable>

  <url type="homepage">https://github.com/SergKam/FlyCrys</url>
  <url type="bugtracker">https://github.com/SergKam/FlyCrys/issues</url>
  <url type="vcs-browser">https://github.com/SergKam/FlyCrys</url>

  <content_rating type="oars-1.1" />

  <branding>
    <color type="primary" scheme_preference="light">#3584e4</color>
    <color type="primary" scheme_preference="dark">#1a5fb4</color>
  </branding>

  <supports>
    <control>keyboard</control>
    <control>pointing</control>
  </supports>

  <screenshots>
    <screenshot type="default">
      <image>https://raw.githubusercontent.com/SergKam/FlyCrys/v0.2.1/screenshots/main.png</image>
      <caption>FlyCrys main workspace with file tree and agent panel</caption>
    </screenshot>
  </screenshots>

  <releases>
    <release version="0.2.1" date="2026-03-25">
      <description>
        <p>Bug fixes and improvements.</p>
      </description>
    </release>
  </releases>
</component>
```

### 3. Desktop File

The existing `com.flycrys.app.desktop` needs to be renamed/adapted to use the Flathub app ID:

```ini
[Desktop Entry]
Name=FlyCrys
Comment=Lightning-fast, Linux-native UI for Claude Code AI agent
Exec=flycrys
Icon=io.github.SergKam.FlyCrys
Type=Application
Categories=Development;Utility;
```

The `Icon` key must match the app ID pattern (`$FLATPAK_ID` or `$FLATPAK_ID-foo`).

### 4. Icons

- Minimum: 128x128 PNG (required for AppStream catalog)
- Recommended: SVG, or 256x256+ PNG
- FlyCrys already has 128x128, 256x256, 512x512 -- all sufficient
- Must be installed as `io.github.SergKam.FlyCrys.png` under hicolor theme

### 5. Screenshots

- Must be real screenshots hosted at stable URLs (use git tags, not branches)
- Must include at least one with `type="default"`
- Add descriptive captions
- PNG format (no SVG/SVGZ)

---

## Submission Process (Step-by-Step)

1. **Prepare all files locally.** Build and test with flatpak-builder:
   ```bash
   flatpak install flathub org.gnome.Sdk//47 org.gnome.Platform//47
   flatpak install flathub org.freedesktop.Sdk.Extension.rust-stable//24.08
   flatpak-builder --user --install --force-clean build-dir io.github.SergKam.FlyCrys.yml
   flatpak run io.github.SergKam.FlyCrys
   ```

2. **Run the linter:**
   ```bash
   flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest io.github.SergKam.FlyCrys.yml
   flatpak run --command=flatpak-builder-lint org.flatpak.Builder repo repo
   ```

3. **Fork the Flathub repository** (keep all branches):
   ```
   https://github.com/flathub/flathub
   ```

4. **Clone using the `new-pr` branch:**
   ```bash
   git clone --branch new-pr https://github.com/YOUR_USERNAME/flathub.git
   cd flathub
   git checkout -b io.github.SergKam.FlyCrys
   ```

5. **Add your manifest and supporting files:**
   ```bash
   cp /path/to/io.github.SergKam.FlyCrys.yml .
   cp /path/to/flycrys-cargo-sources.json .
   git add .
   git commit -m "Add io.github.SergKam.FlyCrys"
   git push origin io.github.SergKam.FlyCrys
   ```

6. **Open a PR** against the `new-pr` base branch (not master/main).
   - Title: `Add io.github.SergKam.FlyCrys`
   - Fill out the template thoroughly

7. **Wait for review.** All reviewers are volunteers; there is no guaranteed timeline.

8. **After merge:** Accept the GitHub invitation to the new repo within one week.
   The build publishes within 1-2 hours; the website listing appears within a few hours.

---

## Sandbox Permissions (finish-args) for GTK4 + VTE4 + WebKitGTK

| Permission | Flag | Reason |
|---|---|---|
| Wayland | `--socket=wayland` | GTK4 native display |
| X11 fallback | `--socket=fallback-x11` | For X11 sessions |
| IPC | `--share=ipc` | Required with X11 |
| GPU | `--device=dri` | WebKitGTK hardware acceleration |
| Network | `--share=network` | Claude Code API calls |
| Home directory | `--filesystem=home` | Claude Code reads/writes project files |
| Notifications | `--talk-name=org.freedesktop.Notifications` | Desktop notifications |

**Important linter rules:**
- Do NOT use both `--socket=x11` and `--socket=wayland` together. Use `--socket=fallback-x11` for X11 fallback.
- Do NOT use `--filesystem=/home` or `--filesystem=/tmp`. Use `--filesystem=home` (without leading slash) or specific XDG paths.
- Do NOT use wildcard DBus names like `org.freedesktop.*`.

---

## Review Criteria and Common Rejection Reasons

- App must be built entirely from source (no precompiled binaries)
- No network access during build (all cargo crates must be pre-vendored)
- MetaInfo must pass `flatpak-builder-lint` validation
- Desktop file must pass `desktop-file-validate`
- Icon must match app ID naming pattern
- Must have OARS content rating
- Must have at least one release in metainfo
- App name/icon must not imply unauthorized affiliation (e.g., do not use Anthropic/Claude branding)
- PRs are rejected for: removing template, not following guidelines, excessive AI-generated content, or spammy activity

---

## GTK4/VTE4/WebKitGTK-Specific Gotchas

1. **Runtime:** Use `org.gnome.Platform` / `org.gnome.Sdk` (provides GTK4, VTE, WebKitGTK)
2. **Rust SDK Extension:** Add `org.freedesktop.Sdk.Extension.rust-stable` and set `append-path`
3. **Cargo offline build:** All crate sources must be listed in the cargo sources JSON. No network access during build.
4. **VTE4 PTY:** VTE terminal emulation needs PTY access. Inside the Flatpak sandbox, this should work by default since /dev/pts is available, but test thoroughly.
5. **WebKitGTK:** Already part of `org.gnome.Platform`. Ensure `--device=dri` and `--share=network` are set.
6. **Icon naming:** Rename all icon files from `flycrys.png` to `io.github.SergKam.FlyCrys.png` at install time.
7. **Desktop file ID:** Must match the app ID: `io.github.SergKam.FlyCrys.desktop`.

---

## Timeline Expectations

| Phase | Duration |
|---|---|
| Prepare manifest + test locally | 1-3 days |
| PR review (volunteer reviewers) | Days to weeks (no guaranteed SLA) |
| Build after merge | 1-2 hours |
| Website listing | Within a few hours of build |
| Accept repo invitation | Must be done within 1 week |

---

## Post-Acceptance Maintenance

- **Updates:** Push new manifest changes to the `master` branch of `flathub/io.github.SergKam.FlyCrys`
- **Permission changes:** Trigger moderation review (any finish-args change or critical metainfo change)
- **Runtime updates:** Keep runtime version current; avoid end-of-life runtimes
- **Quality guidelines:** Passing quality checks enables homepage/trending placement
- **EOL:** If abandoning, set `{"end-of-life": "reason"}` in `flathub.json`
- **Test builds:** PRs trigger automatic test builds; use `bot, build` comments for manual builds
