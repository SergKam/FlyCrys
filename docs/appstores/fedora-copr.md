# Fedora COPR Submission Guide for FlyCrys

## Overview

COPR (Community Projects) is a free build system for RPM packages. It is **not** the
official Fedora repos -- anyone with a Fedora Account can create a COPR project and
publish RPM packages. Users add the repo with `dnf copr enable`. COPR packages do not
need to follow Fedora Packaging Guidelines (though they are recommended).

COPR is the fastest path to RPM distribution. No review process is required.

---

## .spec File for FlyCrys

```spec
Name:           flycrys
Version:        0.2.1
Release:        1%{?dist}
Summary:        Lightning-fast, Linux-native agentic UI on top of Claude Code CLI
SourceLicense:  MIT
License:        MIT
URL:            https://github.com/SergKam/FlyCrys
Source0:        %{url}/archive/refs/tags/v%{version}.tar.gz#/FlyCrys-%{version}.tar.gz

BuildRequires:  cargo-rpm-macros
BuildRequires:  pkg-config
BuildRequires:  gtk4-devel
BuildRequires:  vte291-gtk4-devel
BuildRequires:  webkit6gtk-devel
BuildRequires:  gcc
BuildRequires:  openssl-devel

Requires:       gtk4
Requires:       vte291-gtk4
Requires:       webkit6gtk
Recommends:     nodejs

%description
FlyCrys is a GTK4-native Linux desktop application that provides a
graphical interface for Claude Code, Anthropic's AI coding agent CLI.
Features multi-workspace tabs, file tree browser, syntax-highlighted
viewer, and embedded terminal.

Requires Claude Code CLI (npm install -g @anthropic-ai/claude-code).

%prep
%autosetup -n FlyCrys-%{version} -p1
%cargo_prep

%generate_buildrequires
%cargo_generate_buildrequires

%build
%cargo_build
%{cargo_license_summary}
%{cargo_license} > LICENSE.dependencies

%install
install -Dpm 0755 target/rpm/flycrys -t %{buildroot}%{_bindir}

# Desktop file
install -Dpm 0644 com.flycrys.app.desktop %{buildroot}%{_datadir}/applications/flycrys.desktop

# Icons
install -Dpm 0644 data/icons/hicolor/48x48/apps/flycrys.png \
    %{buildroot}%{_datadir}/icons/hicolor/48x48/apps/flycrys.png
install -Dpm 0644 data/icons/hicolor/128x128/apps/flycrys.png \
    %{buildroot}%{_datadir}/icons/hicolor/128x128/apps/flycrys.png
install -Dpm 0644 data/icons/hicolor/256x256/apps/flycrys.png \
    %{buildroot}%{_datadir}/icons/hicolor/256x256/apps/flycrys.png
install -Dpm 0644 data/icons/hicolor/512x512/apps/flycrys.png \
    %{buildroot}%{_datadir}/icons/hicolor/512x512/apps/flycrys.png

%check
%cargo_test

%files
%license LICENSE
%license LICENSE.dependencies
%doc README.md
%{_bindir}/flycrys
%{_datadir}/applications/flycrys.desktop
%{_datadir}/icons/hicolor/*/apps/flycrys.png

%changelog
%autochangelog
```

### Key Fedora RPM Macros for Rust

| Macro | Section | Purpose |
|---|---|---|
| `%cargo_prep` | `%prep` | Sets up cargo build environment, vendors dependencies |
| `%cargo_generate_buildrequires` | `%generate_buildrequires` | Auto-generates crate dependency BuildRequires |
| `%cargo_build` | `%build` | Runs `cargo build --release` with correct flags |
| `%cargo_test` | `%check` | Runs `cargo test` |
| `%cargo_install` | `%install` | Installs crate sources (for library crates only) |
| `%cargo_license_summary` | `%build` | Generates license summary |
| `%cargo_license` | `%build` | Collects all dependency license texts |

**Important:** For non-crate applications (like FlyCrys), do NOT use `%cargo_install`.
Instead, manually install from `target/rpm/` using the `install` command.

---

## Alternative: Vendored Dependencies Approach

If `%cargo_generate_buildrequires` causes issues (missing crates in Fedora), use vendored dependencies:

```spec
# In %prep, replace %cargo_prep with:
%cargo_prep -v vendor

# Remove %generate_buildrequires section entirely

# In %build, add before %cargo_build:
%cargo_vendor_manifest

# In %files, add:
%license cargo-vendor.txt
```

To create the vendor tarball:
```bash
cd FlyCrys-0.2.1
cargo vendor > vendor-config.toml
tar czf flycrys-0.2.1-vendor.tar.gz vendor/
```

Add as `Source1` in the spec file.

---

## COPR Submission Process (Step-by-Step)

### 1. Create a Fedora Account

Register at https://accounts.fedoraproject.org

### 2. Set Up COPR CLI

```bash
sudo dnf install copr-cli
```

Get your API token from https://copr.fedorainfracloud.org/api/ and save to `~/.config/copr`:

```ini
[copr-cli]
login = your-login
username = your-username
token = your-token
copr_url = https://copr.fedorainfracloud.org
```

### 3. Create a COPR Project

```bash
copr-cli create flycrys \
    --chroot fedora-rawhide-x86_64 \
    --chroot fedora-41-x86_64 \
    --chroot fedora-40-x86_64 \
    --description "Lightning-fast, Linux-native UI for Claude Code AI agent" \
    --instructions "sudo dnf copr enable YOUR_USERNAME/flycrys && sudo dnf install flycrys" \
    --homepage https://github.com/SergKam/FlyCrys
```

### 4. Build Methods

**Method A: Upload SRPM**

```bash
# Build SRPM locally
rpmbuild -bs flycrys.spec
copr-cli build flycrys ~/rpmbuild/SRPMS/flycrys-0.2.1-1.fc41.src.rpm
```

**Method B: Build from SCM (Git)**

```bash
copr-cli buildscm flycrys \
    --clone-url https://github.com/SergKam/FlyCrys.git \
    --commit v0.2.1 \
    --subdir . \
    --spec flycrys.spec \
    --method rpkg
```

For SCM builds, the spec file must be in the repository.

**Method C: Build from spec + source URL**

Upload the spec file via the COPR web interface and provide the source URL.

### 5. Monitor Build

```bash
copr-cli watch-build BUILD_ID
# Or check the web interface at https://copr.fedorainfracloud.org
```

Build timeout default: 5 hours (expandable to 30 hours).

### 6. Verify

```bash
# On a Fedora machine:
sudo dnf copr enable YOUR_USERNAME/flycrys
sudo dnf install flycrys
```

---

## How Users Add the Repo

```bash
sudo dnf copr enable YOUR_USERNAME/flycrys
sudo dnf install flycrys
```

Or manually download the `.repo` file from the COPR project page and place it in `/etc/yum.repos.d/`.

---

## GTK4/VTE4/WebKitGTK-Specific Gotchas

1. **Package names differ from Debian.** Fedora uses:
   - `gtk4-devel` (not `libgtk-4-dev`)
   - `vte291-gtk4-devel` (not `libvte-2.91-gtk4-dev`)
   - `webkit6gtk-devel` (not `libwebkitgtk-6.0-dev`)
2. **Verify package names** with `dnf search gtk4` on a Fedora system before submitting.
3. **Rust toolchain:** `cargo-rpm-macros` pulls in the Rust toolchain automatically.
4. **Build time:** GTK4 + WebKitGTK apps with many Rust dependencies can be slow to build. The default 5-hour timeout should be sufficient, but monitor first builds.
5. **Fedora version matrix:** Test across multiple Fedora versions. GTK4 and VTE4 availability may vary in older Fedora releases.
6. **Oniguruma dependency:** The `syntect` crate (used by FlyCrys) depends on `oniguruma`. Ensure `oniguruma-devel` is in BuildRequires if the build fails on regex-onig.

---

## Webhooks for Automated Rebuilds

Set up GitHub webhooks for automatic rebuilds on push:

1. Go to COPR project settings -> Packages -> Add package
2. Select SCM source type
3. Configure clone URL, spec file path, and method
4. Go to project settings -> Webhooks
5. Copy the webhook URL and add it to your GitHub repository settings

---

## Timeline Expectations

| Phase | Duration |
|---|---|
| Account creation | Instant |
| COPR CLI setup | 5 minutes |
| Project creation | Instant |
| First build | 20-60 minutes (Rust compile + deps) |
| Repo available to users | Immediately after successful build |
| No review process | N/A |

---

## Post-Acceptance Maintenance

- **No review or approval needed** for updates -- just trigger new builds
- **Multiple chroots:** Maintain builds for current and previous Fedora versions
- **Automated rebuilds:** Use webhooks or CI to trigger builds on new tags
- **Build retention:** Only the latest successful build is kept indefinitely; others are retained 14 days
- **Content policy:** Must not violate Fedora licensing policy or Code of Conduct
- **Maintenance burden:** Very low. Trigger a build per release per Fedora version.

### GitHub Actions Example

```yaml
name: COPR Build
on:
  push:
    tags: ['v*']

jobs:
  copr:
    runs-on: ubuntu-latest
    container: fedora:latest
    steps:
      - uses: actions/checkout@v4
      - name: Install tools
        run: dnf install -y copr-cli rpm-build
      - name: Configure COPR
        run: |
          mkdir -p ~/.config
          echo "${{ secrets.COPR_CONFIG }}" > ~/.config/copr
      - name: Build SRPM and submit
        run: |
          rpmbuild -bs flycrys.spec --define "_sourcedir $(pwd)"
          copr-cli build flycrys ~/rpmbuild/SRPMS/flycrys-*.src.rpm
```

---

## Path to Official Fedora Repos

COPR is the stepping stone. To get into official Fedora repos:

1. Follow full Fedora Packaging Guidelines (stricter than COPR)
2. Pass package review (https://docs.fedoraproject.org/en-US/package-maintainers/Package_Review_Process/)
3. Requires a Fedora sponsor
4. Significantly more effort than COPR
5. Consider this only after the app gains traction in COPR
