# Nixpkgs Submission Guide for FlyCrys

## Overview

Nixpkgs is the package collection for the Nix package manager and NixOS. Packages are
submitted as pull requests to the `NixOS/nixpkgs` GitHub repository. The Nix ecosystem
uses a functional approach -- packages are defined as Nix derivations.

For Rust + GTK4 applications, nixpkgs provides `rustPlatform.buildRustPackage` combined
with `wrapGAppsHook4` for GTK4 desktop integration.

---

## Package Derivation (`package.nix`)

### Location in nixpkgs

```
pkgs/by-name/fl/flycrys/package.nix
```

The `by-name` structure uses the first two lowercase letters of the package name as the
directory prefix. Packages placed here are automatically discovered -- no need to edit
`all-packages.nix`.

### Complete package.nix

```nix
{
  lib,
  fetchFromGitHub,
  rustPlatform,
  pkg-config,
  wrapGAppsHook4,
  gtk4,
  vte-gtk4,
  webkitgtk_6_0,
  glib,
  cairo,
  pango,
  gdk-pixbuf,
  graphene,
}:

rustPlatform.buildRustPackage (finalAttrs: {
  pname = "flycrys";
  version = "0.2.1";

  src = fetchFromGitHub {
    owner = "SergKam";
    repo = "FlyCrys";
    tag = "v${finalAttrs.version}";
    hash = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
    # Use lib.fakeHash first, then replace with real hash from error message
  };

  cargoHash = "sha256-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB=";
  # Use lib.fakeHash first, then replace with real hash from error message

  nativeBuildInputs = [
    pkg-config
    wrapGAppsHook4
  ];

  buildInputs = [
    gtk4
    vte-gtk4
    webkitgtk_6_0
    glib
    cairo
    pango
    gdk-pixbuf
    graphene
  ];

  postInstall = ''
    # Desktop file
    install -Dm644 com.flycrys.app.desktop $out/share/applications/flycrys.desktop

    # Icons
    for size in 48 128 256 512; do
      install -Dm644 data/icons/hicolor/''${size}x''${size}/apps/flycrys.png \
        $out/share/icons/hicolor/''${size}x''${size}/apps/flycrys.png
    done
  '';

  meta = {
    description = "Lightning-fast, Linux-native agentic UI on top of Claude Code CLI";
    homepage = "https://github.com/SergKam/FlyCrys";
    license = lib.licenses.mit;
    maintainers = with lib.maintainers; [ /* your nixpkgs maintainer handle */ ];
    platforms = lib.platforms.linux;
    mainProgram = "flycrys";
  };
})
```

### Key Components Explained

| Attribute | Purpose |
|---|---|
| `rustPlatform.buildRustPackage` | Standard helper for Rust packages with Cargo |
| `cargoHash` | Hash of vendored cargo dependencies (SRI format) |
| `pkg-config` | Finds C library metadata (GTK4, VTE, WebKitGTK) at build time |
| `wrapGAppsHook4` | Wraps the binary with GTK4 environment variables (schemas, themes, icons) |
| `gtk4` | GTK4 toolkit |
| `vte-gtk4` | VTE terminal emulator widget for GTK4 |
| `webkitgtk_6_0` | WebKitGTK 6.0 for GTK4 |
| `postInstall` | Installs desktop file and icons after cargo install |

### Finding the Correct Hashes

1. Set both hashes to `lib.fakeHash` initially:
   ```nix
   hash = lib.fakeHash;
   cargoHash = lib.fakeHash;
   ```

2. Try to build:
   ```bash
   nix-build -A flycrys
   ```

3. The build will fail twice -- once for `hash` (source), once for `cargoHash` (deps).
   Each error message includes the correct hash. Replace `lib.fakeHash` with the real
   values one at a time.

### Cargo.lock Requirement

The repository must include a `Cargo.lock` file. If it does not, add it during
`postPatch`:

```nix
postPatch = ''
  ln -s ${./Cargo.lock} Cargo.lock
'';
```

FlyCrys already includes `Cargo.lock` in the repository, so this is not needed.

---

## PR Submission Process (Step-by-Step)

### 1. Fork and Clone nixpkgs

```bash
git clone https://github.com/YOUR_USERNAME/nixpkgs.git
cd nixpkgs
git remote add upstream https://github.com/NixOS/nixpkgs.git
git fetch upstream
git checkout -b flycrys upstream/master
```

### 2. Create the Package Directory

```bash
mkdir -p pkgs/by-name/fl/flycrys
```

### 3. Write package.nix

Copy the derivation above into `pkgs/by-name/fl/flycrys/package.nix`.

### 4. Test the Build

```bash
# Build the package
nix-build -A flycrys

# Test the binary
./result/bin/flycrys

# Run any tests
nix-build -A flycrys.tests  # if tests are defined
```

### 5. Verify with nixpkgs-review (Optional but Recommended)

```bash
nix-shell -p nixpkgs-review
nixpkgs-review rev HEAD
```

This builds the package and all its reverse dependencies to check for breakage.

### 6. Enable Sandbox

Ensure sandbox builds work (required for the PR checklist):

```bash
# In /etc/nix/nix.conf:
sandbox = true
```

### 7. Commit and Push

```bash
git add pkgs/by-name/fl/flycrys/package.nix
git commit -m "flycrys: init at 0.2.1"
git push origin flycrys
```

Commit message convention: `<pname>: init at <version>` for new packages.

### 8. Open a Pull Request

Target branch: `master` (for most changes).

Fill out the PR template checklist:
- [ ] Built on Linux x86_64
- [ ] Sandbox enabled
- [ ] Tested the binary
- [ ] Used nixpkgs-review if applicable

---

## Review Timeline

- Nixpkgs receives a very high volume of PRs
- New package additions are generally reviewed faster than complex changes
- Typical timeline: **days to weeks** depending on reviewer availability
- Respond promptly to review comments to keep the PR mergeable
- Having a nixpkgs maintainer handle speeds up the process

---

## GTK4/VTE4/WebKitGTK-Specific Gotchas

1. **wrapGAppsHook4 is essential.** Without it, GTK4 apps fail to find schemas, icons, and themes at runtime. It sets `GDK_PIXBUF_MODULE_FILE`, `GI_TYPELIB_PATH`, `GSETTINGS_SCHEMA_DIR`, etc.

2. **Do NOT use wrapGAppsHook (without the 4).** That is for GTK3. `wrapGAppsHook4` is specifically for GTK4 applications.

3. **pkg-config in nativeBuildInputs.** Rust crates that use `pkg-config` to find C libraries (gtk4-rs, vte4-rs, webkit6-rs) require `pkg-config` as a native build input.

4. **WebKitGTK package name.** In nixpkgs, it is `webkitgtk_6_0` (with underscores). Check with `nix-env -qaP webkitgtk` to find the exact attribute name.

5. **VTE package name.** The GTK4 variant is `vte-gtk4` in nixpkgs (verify with `nix-env -qaP vte`).

6. **Build inputs vs native build inputs:**
   - `nativeBuildInputs`: Tools that run during the build (pkg-config, wrapGAppsHook4)
   - `buildInputs`: Libraries linked against (gtk4, vte-gtk4, webkitgtk_6_0)

7. **Oniguruma dependency.** The `syntect` crate with `regex-onig` feature needs oniguruma. If the build fails, add `oniguruma` to `buildInputs` and `RUSTONIG_SYSTEM_LIBONIG=1` to the environment.

8. **Test phase.** If tests require a display server or DBus, you may need to disable them:
   ```nix
   doCheck = false;  # Tests require display server
   ```
   Document the reason in a comment.

---

## Timeline Expectations

| Phase | Duration |
|---|---|
| Write derivation + test locally | 1-2 hours |
| PR submission | Minutes |
| Review | Days to weeks |
| Merge to master | After approval |
| Available in unstable channel | After merge + hydra build |
| Available in stable release | Next NixOS release (every 6 months) or manual backport |

---

## Post-Acceptance Maintenance

- **Updates:** Submit PRs with commit message `flycrys: 0.2.1 -> 0.3.0`
- **Update cargoHash** on every version bump (the hash changes when dependencies change)
- **Maintainer field:** Add yourself to `maintainers/maintainer-list.nix` to be listed as a maintainer and get pinged on related PRs
- **Nixpkgs-update bot:** An automated bot can detect new GitHub releases and open update PRs, but maintainer review is still needed
- **Maintenance burden:** Low to moderate. Each release requires updating version, hash, and cargoHash.
- **Backward compatibility:** If the package enters a stable NixOS release, you may need to backport security fixes

### Version Update Template

```nix
# Change these three values:
version = "0.3.0";
hash = "sha256-NEWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWW=";
cargoHash = "sha256-NEWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWW=";
```

---

## Alternative: NUR (Nix User Repository)

For faster iteration without the nixpkgs review process, consider publishing to NUR first:

1. Create a repository following https://github.com/nix-community/NUR
2. Define your package there
3. Users add your repo as an overlay
4. Move to nixpkgs once the package is stable and has users

This is the Nix equivalent of AUR -- lower barrier to entry, personal maintenance.
