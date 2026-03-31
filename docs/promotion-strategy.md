# Promotion Strategy: FlyCrys

Last updated: 2026-03-27

---

## Channel 1: Hacker News (Show HN)

**Priority: CRITICAL — do this first**

The single highest-ROI action for any OSS dev tool. Zed got 13K stars from one HN post. Warp got 10K signups in 24 hours. Open Interpreter hit 44K stars largely through HN virality.

**How to do it**:
- Title: `Show HN: FlyCrys – Native Linux GUI for Claude Code (Rust + GTK4)`
- Post between 8:00–10:00 AM Pacific Time (6:00–8:00 PM Kyiv time), Tuesday–Thursday
- Immediately post a founder's comment explaining:
  - Why you built it (Claude Code is CLI-only, no good Linux GUI exists)
  - Tech stack choices (Rust + GTK4, why not Electron/Tauri)
  - What makes it different (native, <1s startup, 20MB memory, full workspace)
  - Current limitations (honest — HN respects this)
  - What feedback you want
- Do NOT ask anyone to upvote — HN detects vote rings and will kill the post
- Have a polished README with screenshots and a GIF/video before posting
- Show HN posts appear on the `/show` tab even if they fall off `/new` — longer tail than regular posts

**Expected impact**: 50–500 stars if it hits front page. 1000+ if it really resonates.
**Effort**: Low (one post + one comment). High prep (README, screenshots must be perfect).

---

## Channel 2: Reddit

**Priority: HIGH**

**Target subreddits** (ordered by relevance):

| Subreddit | Subscribers | Rules/Notes |
|-----------|------------|-------------|
| r/ClaudeAI | ~150K | Directly relevant. Post as "tool showcase." Very active community. |
| r/rust | ~350K | "Show" flair for project announcements. Must be genuine, no marketing speak. |
| r/linux | ~900K | Allows app announcements. Flair as "Software." Linux-native angle plays well. |
| r/gnome | ~50K | GTK4 native app — this is their audience. Respectful, small community. |
| r/commandline | ~250K | Terminal + GUI hybrid angle. |
| r/LocalLLaMA | ~400K | AI tool audience. Some overlap with Claude users. |
| r/programming | ~6M | Broad reach but hard to stand out. Better as a follow-up. |
| r/opensource | ~100K | General OSS audience. |
| r/voidlinux, r/archlinux, r/fedora | Various | Distro-specific communities. Post after you have packaging for their distro. |

**How to post**:
- Title format: "FlyCrys: a native Linux GUI for Claude Code, built with Rust + GTK4 [open source]"
- Include 2-3 screenshots in the post
- Write a comment explaining the motivation and tech decisions
- Don't cross-post to all subreddits on the same day — space them out over 1-2 weeks
- r/ClaudeAI and r/rust first (highest signal-to-noise), then r/linux, then others
- Respond to every comment. Reddit rewards engagement.

**Expected impact**: 20–200 stars per subreddit post. r/ClaudeAI and r/rust are highest conversion.
**Effort**: Medium (need to craft different angles for each community).

---

## Channel 3: Newsletters

**Priority: HIGH**

### This Week in Rust
- **Audience**: Rust developers (10K+ subscribers)
- **How to submit**: Two paths:
  1. Open a PR on github.com/rust-lang/this-week-in-rust adding FlyCrys to the "Project/Tooling Updates" section in the current draft
  2. Nominate for "Crate of the Week" at users.rust-lang.org/t/crate-of-the-week/2704 — post a comment describing FlyCrys with a link
- **Timing**: Submit PR early in the week (Mon-Tue) for inclusion that week
- **Expected impact**: High credibility signal + 50-200 stars from Rust community

### Console.dev
- **Audience**: CTOs, engineering managers, senior devs (68% sign up for featured tools)
- **How to submit**: Email hello@console.dev with project details
- **Criteria**: Must be developer-focused, actively maintained, good docs. Pre-1.0 / beta projects are eligible (FlyCrys at 0.2.x qualifies)
- **Expected impact**: Quality audience, moderate traffic, good backlink

### Changelog Weekly / Changelog News
- **Audience**: Broad developer community
- **How to submit**: changelog.com/news — submit link for community voting
- **Expected impact**: Moderate reach, long-tail traffic

### Rust Bytes (Substack)
- **Audience**: Rust developers
- **How to submit**: Contact via Substack
- **Expected impact**: Niche but highly relevant audience

---

## Channel 4: Linux Tech Media

**Priority: HIGH**

| Publication | Focus | How to reach | Notes |
|-------------|-------|-------------|-------|
| It's FOSS / It's FOSS News | Linux apps, open source | Submit via news.itsfoss.com or tweet @itaborahy | Covers GTK/GNOME apps regularly. Covered Amberol, Shortwave. |
| OMG! Ubuntu | Ubuntu/GNOME desktop apps | Tips form at omgubuntu.co.uk | Covered Amberol, Warp Linux launch. Perfect fit. |
| Phoronix | Linux/hardware/benchmarks | michael@phoronix.com | More likely to cover if there's a performance angle (Rust speed). |
| Linux Uprising | Linux desktop news | Contact form on site | Covered Shortwave 2.0, GTK4 apps. |
| 9to5Linux | Linux news | Tips via site | Covered GNOME ecosystem apps. |
| GamingOnLinux | Linux (broader) | Contact form | Lower priority — not dev-focused. |
| LWN.net | Deep Linux/OSS technical | Submit via lwn.net | High credibility but selective. Best after some traction. |

**How to approach**: Short email with:
- One-paragraph description
- 2 screenshots
- Link to repo + release page
- Why it's newsworthy ("first native Linux GUI for Claude Code" angle)

**Expected impact**: 100-1000 visitors per article. It's FOSS and OMG Ubuntu are highest volume.
**Effort**: Low (template email, send to all).

---

## Channel 5: YouTube / Content Creators

**Priority: MEDIUM**

| Creator | Subscribers | Focus | How to reach |
|---------|------------|-------|-------------|
| The Linux Experiment | ~250K | Linux desktop, app reviews | nick@thelinuxexperiment.com (check channel) |
| Brodie Robertson | ~200K | Linux news, OSS | Check channel description for contact |
| DistroTube (Derek Taylor) | ~250K | Linux distros, CLI tools, tiling WMs | Check channel description |
| Chris Titus Tech | ~2M | Linux, Windows, tech | Business email on channel |
| TechHut | ~200K | Linux desktop | Check channel description |
| Veronica Explains | ~100K | Linux, sysadmin | Check channel description |
| Switched to Linux | ~100K | Linux desktops | Check channel description |

**How to approach**:
- Check each channel's "About" tab for business/contact email
- Short email: "Built a native Linux GUI for AI coding in Rust + GTK4. Would you be interested in covering it?"
- Attach or link a 30-second demo video showing: install, launch (<1 sec), open project, chat with agent, see file tree
- Offer to answer questions or do a live demo
- Do NOT ask them to "promote" — frame as "thought this might interest your audience"

**Expected impact**: One video = 5K-50K views = 50-500 stars. Brodie Robertson and The Linux Experiment are most likely to cover niche Linux apps.
**Effort**: Medium (need a good demo video first).

---

## Channel 6: Social Media (Twitter/X, Mastodon, Bluesky)

**Priority: MEDIUM**

### Key accounts to engage with / tag:
- **Rust community**: @rustlang, @ThisWeekInRust, @rust_discussions (Mastodon)
- **GNOME**: @gnome, GNOME Mastodon instance
- **GTK-rs**: @gtk_rs
- **Claude/Anthropic**: @AnthropicAI, @alexalbert__ (Alex Albert, Claude Code lead)
- **Linux**: @itaborahy (It's FOSS), @omaborahy (OMG Ubuntu)
- **Dev tools influencers**: Anyone covering AI dev tools, Rust tooling

### Post strategy:
- Thread format works best on Twitter/X:
  1. Hook: "Built a native Linux GUI for Claude Code. No Electron. Starts in <1 second."
  2. Screenshot/GIF showing the UI
  3. Tech details: "Rust + GTK4. ~20MB memory. Single binary."
  4. Feature highlights (3-4 tweets with screenshots)
  5. Link to repo
- On Mastodon: Post to #rust, #linux, #gnome, #gtk, #opensource hashtags
- On Bluesky: Similar to Twitter, growing dev community there

**Expected impact**: Highly variable. One viral tweet can drive 1000+ stars. Most posts get <50 engagements.
**Effort**: Low per post, but needs consistency.

---

## Channel 7: GNOME / Rust Community Channels

**Priority: MEDIUM**

| Channel | Platform | How to engage |
|---------|----------|--------------|
| GNOME Discourse | discourse.gnome.org | Post in "Applications" category |
| Rust Users Forum | users.rust-lang.org | Post in "Showcase" |
| gtk-rs Matrix room | matrix.org | Share in the GTK-rs dev channel |
| GNOME Matrix rooms | matrix.org | #gnome:gnome.org, #gnome-apps:gnome.org |
| Rust Discord | discord.gg/rust-lang | #showcase channel |
| Rust subreddit Discord | Various | Check sidebar |

**Expected impact**: Small but high-quality audience. These people contribute PRs and file quality bug reports.
**Effort**: Low.

---

## Channel 8: Conferences

**Priority: LOW (long lead time) but HIGH credibility**

### GUADEC 2026
- **When**: July 16-21, 2026, A Coruia, Spain (hybrid)
- **CFP status**: OPEN — deadline extended to March 27 (TODAY). Submit now: events.gnome.org/event/306/abstracts/#submit-abstract
- **Talk angle**: "Building a Native AI Development Workspace with GTK4 and Rust" — interesting to GNOME devs as a case study of gtk4-rs + webkit6 + vte4 in a real app
- **Impact**: GNOME community credibility, potential contributors, media coverage

### RustConf 2026
- **When**: September 8-11, 2026, Montreal (hybrid)
- **CFP status**: CLOSED (deadline was Feb 16, 2026)
- **Alternative**: Attend and do hallway/unconference talks, or poster session if available

### FOSDEM 2027
- **When**: Late Jan / early Feb 2027, Brussels
- **Plan**: Submit to Rust devroom CFP (opens ~Oct 2026). Lightning talk format (15 min).
- **Impact**: 600+ talks, massive OSS audience. Rust devroom is well-attended.

### RustWeek 2026
- **CFP**: Check 2026.rustweek.org/cfp/
- **Talk angle**: Real-world GTK4 + Rust desktop app development

### Local meetups
- Rust meetups (rustup.rs/meetups or meetup.com)
- Linux User Groups (LUGs)
- GNOME meetups
- Lightning talks are low-commitment and great practice

---

## Channel 9: GitHub Discoverability

**Priority: HIGH (one-time setup, permanent benefit)**

### Repo optimizations:
1. **Topic tags**: Add `claude-code`, `gtk4`, `rust`, `linux`, `ai-coding`, `gnome`, `developer-tools`, `gui`, `terminal`
2. **Social preview image**: Create a 1280x640 image with logo + screenshot + tagline. Shows in link previews everywhere.
3. **GitHub Discussions**: Enable for Q&A and feature requests. Lowers barrier vs Issues.
4. **Releases with notes**: Every release should have detailed notes. GitHub shows these prominently.
5. **Badges in README**: Build status, version, license, downloads

### Awesome lists (submit PRs):
- **awesome-rust** (github.com/rust-unofficial/awesome-rust) — under "Applications > Developer Tools" or "Applications > Text Editors"
- **awesome-gtk** (github.com/valpackett/awesome-gtk) — under relevant category
- **awesome-linux** lists — search GitHub for "awesome-linux"
- **awesome-ai-tools** lists — search for "awesome-ai", "awesome-llm"
- **claude-code topic** on GitHub — ensure repo appears in github.com/topics/claude-code

**Expected impact**: Awesome-rust alone drives steady discovery traffic. Topic tags improve GitHub search.
**Effort**: Low (a few PRs + repo settings).

---

## Channel 10: Flathub

**Priority: HIGH for Linux desktop apps**

Publishing to Flathub is how GNOME/GTK apps get discovered by Linux desktop users. Every successful GTK4/Rust app (Amberol, Shortwave, Apostrophe) is on Flathub.

**Steps**:
1. Create a Flatpak manifest (com.flycrys.app.yml)
2. Submit to github.com/flathub/flathub — follow their submission guidelines
3. Once accepted, FlyCrys appears in GNOME Software, KDE Discover, and flathub.org
4. Flathub has its own "trending" and "new apps" sections

**Expected impact**: Ongoing organic discovery by Linux desktop users.
**Effort**: Medium (Flatpak packaging has a learning curve, but one-time).

---

## Outreach Email Template

Subject: FlyCrys — native Linux GUI for Claude Code (open source, Rust + GTK4)

```
Hi [Name],

I built FlyCrys, a native Linux desktop app for working with Claude Code
agents. It's open source (MIT), built with Rust and GTK4 — no Electron,
no browser runtime, starts in under a second.

Key features:
- Multi-workspace tabs with file tree, text viewer, embedded terminal
- Streaming chat with markdown rendering and tool call display
- Agent profiles (security, research, custom) with system prompts
- Session persistence across restarts
- Git status panel, syntax highlighting, image preview

Repository: https://github.com/SergKam/FlyCrys
Screenshots: [link to README]

I thought this might interest [your readers / your audience / the
community] since [reason — e.g., "you cover Linux desktop apps" or
"it's a real-world GTK4 + Rust application"].

Happy to answer any questions or provide more details.

Best,
Sergii Kamenskyi
```

---

## Priority Ranking Summary

| # | Channel | Impact | Effort | When |
|---|---------|--------|--------|------|
| 1 | Hacker News (Show HN) | Very High | Low | Week 1 |
| 2 | Reddit (r/ClaudeAI, r/rust) | High | Medium | Week 1 |
| 3 | GitHub optimizations | Medium | Low | Week 1 (before HN) |
| 4 | GUADEC 2026 CFP | High | Medium | NOW (deadline today) |
| 5 | This Week in Rust | High | Low | Week 1-2 |
| 6 | Linux tech media | High | Low | Week 2 |
| 7 | Awesome lists | Medium | Low | Week 2 |
| 8 | Flathub | Medium | Medium | Week 2-3 |
| 9 | Twitter/Mastodon/Bluesky | Variable | Low | Ongoing |
| 10 | YouTube creators | High | Medium | Week 3-4 |
| 11 | Console.dev, Changelog | Medium | Low | Week 3 |
| 12 | GNOME/Rust community | Low-Med | Low | Ongoing |
