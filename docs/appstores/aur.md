# AUR (Arch User Repository) Submission Guide for FlyCrys

## Overview

The AUR is a community-driven repository for Arch Linux users. Packages are described
by PKGBUILD files that users build locally with `makepkg`. There is no binary
distribution -- AUR helpers like `yay` or `paru` automate the build-from-source process.

Two package variants are appropriate for FlyCrys:
- `flycrys` -- builds from a tagged release tarball
- `flycrys-git` -- builds from the latest git HEAD (VCS package)

---

## PKGBUILD for Release Package (`flycrys`)

```bash
# Maintainer: Sergii Kamenskyi <sergukam@gmail.com>
pkgname=flycrys
pkgver=0.2.1
pkgrel=1
pkgdesc='Lightning-fast, Linux-native agentic UI on top of Claude Code CLI'
arch=('x86_64')
url='https://github.com/SergKam/FlyCrys'
license=('MIT')
depends=(
    'gtk4'
    'vte4'
    'webkitgtk-6.0'
    'gcc-libs'
    'glibc'
)
makedepends=('cargo' 'pkg-config')
optdepends=('nodejs: required for Claude Code CLI')
source=("$pkgname-$pkgver.tar.gz::https://github.com/SergKam/FlyCrys/archive/refs/tags/v$pkgver.tar.gz")
sha256sums=('SKIP')  # Replace with actual hash: makepkg -g

prepare() {
    cd "FlyCrys-$pkgver"
    export RUSTUP_TOOLCHAIN=stable
    cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
    cd "FlyCrys-$pkgver"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    cargo build --frozen --release --all-features
}

check() {
    cd "FlyCrys-$pkgver"
    export RUSTUP_TOOLCHAIN=stable
    cargo test --frozen --all-features
}

package() {
    cd "FlyCrys-$pkgver"
    install -Dm755 target/release/flycrys "$pkgdir/usr/bin/flycrys"
    install -Dm644 com.flycrys.app.desktop "$pkgdir/usr/share/applications/flycrys.desktop"
    install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"

    # Icons
    install -Dm644 data/icons/hicolor/48x48/apps/flycrys.png \
        "$pkgdir/usr/share/icons/hicolor/48x48/apps/flycrys.png"
    install -Dm644 data/icons/hicolor/128x128/apps/flycrys.png \
        "$pkgdir/usr/share/icons/hicolor/128x128/apps/flycrys.png"
    install -Dm644 data/icons/hicolor/256x256/apps/flycrys.png \
        "$pkgdir/usr/share/icons/hicolor/256x256/apps/flycrys.png"
    install -Dm644 data/icons/hicolor/512x512/apps/flycrys.png \
        "$pkgdir/usr/share/icons/hicolor/512x512/apps/flycrys.png"
}
```

## PKGBUILD for Git Package (`flycrys-git`)

```bash
# Maintainer: Sergii Kamenskyi <sergukam@gmail.com>
pkgname=flycrys-git
pkgver=0.2.1.r0.g52331d5
pkgrel=1
pkgdesc='Lightning-fast, Linux-native agentic UI on top of Claude Code CLI (git version)'
arch=('x86_64')
url='https://github.com/SergKam/FlyCrys'
license=('MIT')
depends=(
    'gtk4'
    'vte4'
    'webkitgtk-6.0'
    'gcc-libs'
    'glibc'
)
makedepends=('cargo' 'pkg-config' 'git')
optdepends=('nodejs: required for Claude Code CLI')
provides=('flycrys')
conflicts=('flycrys')
source=("$pkgname::git+https://github.com/SergKam/FlyCrys.git")
sha256sums=('SKIP')

pkgver() {
    cd "$pkgname"
    git describe --long --tags | sed 's/^v//;s/\([^-]*-g\)/r\1/;s/-/./g'
}

prepare() {
    cd "$pkgname"
    export RUSTUP_TOOLCHAIN=stable
    cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
    cd "$pkgname"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    cargo build --frozen --release --all-features
}

check() {
    cd "$pkgname"
    export RUSTUP_TOOLCHAIN=stable
    cargo test --frozen --all-features
}

package() {
    cd "$pkgname"
    install -Dm755 target/release/flycrys "$pkgdir/usr/bin/flycrys"
    install -Dm644 com.flycrys.app.desktop "$pkgdir/usr/share/applications/flycrys.desktop"
    install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"

    install -Dm644 data/icons/hicolor/48x48/apps/flycrys.png \
        "$pkgdir/usr/share/icons/hicolor/48x48/apps/flycrys.png"
    install -Dm644 data/icons/hicolor/128x128/apps/flycrys.png \
        "$pkgdir/usr/share/icons/hicolor/128x128/apps/flycrys.png"
    install -Dm644 data/icons/hicolor/256x256/apps/flycrys.png \
        "$pkgdir/usr/share/icons/hicolor/256x256/apps/flycrys.png"
    install -Dm644 data/icons/hicolor/512x512/apps/flycrys.png \
        "$pkgdir/usr/share/icons/hicolor/512x512/apps/flycrys.png"
}
```

---

## .SRCINFO

Must be generated and committed alongside the PKGBUILD. Regenerate whenever metadata changes:

```bash
makepkg --printsrcinfo > .SRCINFO
```

---

## Naming Conventions

| Variant | Name | Suffix |
|---|---|---|
| Stable release | `flycrys` | None |
| Git HEAD | `flycrys-git` | `-git` |
| Binary (prebuilt) | `flycrys-bin` | `-bin` |

FlyCrys should use lowercase only. The package name matches the binary name.

---

## Submission Process (Step-by-Step)

### 1. Create an AUR Account

Register at https://aur.archlinux.org/register

### 2. Set Up SSH Key

```bash
ssh-keygen -t ed25519 -f ~/.ssh/aur -C "AUR access"
```

Add to `~/.ssh/config`:
```
Host aur.archlinux.org
    IdentityFile ~/.ssh/aur
    User aur
```

Upload the public key at https://aur.archlinux.org/account (SSH Public Key field).

### 3. Create the AUR Repository

```bash
git -c init.defaultBranch=master clone ssh://aur@aur.archlinux.org/flycrys.git
cd flycrys
```

If the package does not exist yet, this creates an empty repo.

### 4. Add PKGBUILD and .SRCINFO

```bash
# Copy your PKGBUILD into the repo
cp /path/to/PKGBUILD .

# Test the build first!
makepkg -si

# Generate .SRCINFO
makepkg --printsrcinfo > .SRCINFO

# Commit and push
git add PKGBUILD .SRCINFO
git commit -m "Initial upload: flycrys 0.2.1"
git push
```

**Only the `master` branch is used.** The AUR rejects pushes to other branches.

### 5. Verify

Visit https://aur.archlinux.org/packages/flycrys to confirm the package appears.

---

## Rules and Requirements

1. **No duplicates in official repos.** Check `pacman -Ss flycrys` and the official package database first.
2. **Must support x86_64.** Packages that do not support x86_64 are not allowed.
3. **Must include .SRCINFO** in every commit that changes metadata.
4. **Must include LICENSE.** Packages without proper licensing cannot be promoted to official repos.
5. **General usefulness.** Ask: "Will anyone else want to use this package?"
6. **Use `conflicts` for variants**, not `replaces`. The `-git` variant should have `conflicts=('flycrys')` and `provides=('flycrys')`.

---

## GTK4/VTE4/WebKitGTK-Specific Gotchas

1. **Dependencies:** Arch has `gtk4`, `vte4`, and `webkitgtk-6.0` in the official repos. Use exact package names.
2. **pkg-config:** Required as a makedepend for Rust crates that link to C libraries.
3. **Cargo fetch:** Use `--locked` to respect the lock file. Use `--frozen` during build to ensure no network access.
4. **WebKitGTK versions:** Arch has both `webkitgtk-6.0` (for GTK4) and `webkit2gtk-4.1` (for GTK3). Make sure to depend on the correct one.
5. **VTE4 package name:** The GTK4 variant is `vte4` (not `vte3` which is GTK3).
6. **Runtime detection:** FlyCrys needs `claude` CLI at runtime. Add `nodejs` as an optdepend since Claude Code requires Node.js.

---

## Timeline Expectations

| Phase | Duration |
|---|---|
| Account creation | Instant |
| SSH setup | 5 minutes |
| PKGBUILD creation + testing | 1-2 hours |
| Package appears on AUR | Instant after push |
| Community feedback | Days to weeks |
| Promotion to official repos | Unlikely for niche apps; requires a Trusted User sponsor |

---

## Post-Acceptance Maintenance

- **Active maintenance expected.** "It is the maintainer's job to maintain the package by checking for updates and improving the PKGBUILD."
- **Version updates:** Bump `pkgver` for new upstream releases. Regenerate `.SRCINFO`. Push.
- **Do not update `pkgrel`** for minor PKGBUILD corrections like typos -- only for changes that affect the built package.
- **Engage with users.** Review comments and flagged-out-of-date notifications on the AUR web interface.
- **Disowning:** If you can no longer maintain, use the web interface to disown the package. After 180 days of being flagged out-of-date, anyone can request orphaning.
- **Automation:** Tools like `aurpublish` or GitHub Actions can automate updates, but automated updates carry risk -- malfunctioning accounts may face removal.
- **Maintenance burden:** Low. Update PKGBUILD version + hash on each release, push.

### Automated Update Script

```bash
#!/bin/bash
# update-aur.sh -- run after tagging a new release
NEW_VERSION="$1"
cd /path/to/aur/flycrys
sed -i "s/pkgver=.*/pkgver=$NEW_VERSION/" PKGBUILD
updpkgsums  # updates sha256sums
makepkg --printsrcinfo > .SRCINFO
git add PKGBUILD .SRCINFO
git commit -m "Update to $NEW_VERSION"
git push
```
