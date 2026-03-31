# FlyCrys Launch Plan: 4-Week Sprint

Last updated: 2026-03-27

---

## Pre-Launch Checklist (do before Week 1)

- [ ] **README polish**: Ensure it has a compelling one-liner, 2-3 screenshots, a GIF or short video of the app in action, clear install instructions, and a "Why this exists" section that tells a story
- [ ] **Social preview image**: Create 1280x640 image for GitHub (logo + screenshot + tagline). This shows when anyone shares the repo link anywhere.
- [ ] **GitHub topic tags**: `claude-code`, `gtk4`, `rust`, `linux`, `gnome`, `ai-coding`, `developer-tools`, `gui`, `terminal`
- [ ] **GitHub Discussions**: Enable on the repo (Settings > Features > Discussions)
- [ ] **Release with .deb**: Ensure latest release has a downloadable .deb and clear release notes
- [ ] **Demo video/GIF**: 30-60 second screen recording showing: app launch (<1s), open a project, ask Claude a question, see streaming response, click file in tree, view syntax-highlighted code, switch tabs
- [ ] **GUADEC 2026 CFP**: Submit talk proposal TODAY (March 27 deadline). Title: "Building a Native AI Workspace with Rust and GTK4". Even if not accepted, the submission costs nothing.

---

## Week 1: Launch

### Monday (prep day)
- [ ] Final README review. Read it as a stranger — does it answer "what is this?" in 10 seconds?
- [ ] Verify .deb install works on a clean Ubuntu system
- [ ] Prepare HN founder's comment (draft it, don't wing it)
- [ ] Prepare Reddit posts for r/ClaudeAI and r/rust (different angles for each)

### Tuesday or Wednesday (launch day — pick one)
**Morning (8-10 AM Pacific / 6-8 PM Kyiv):**
- [ ] Post "Show HN: FlyCrys – Native Linux GUI for Claude Code (Rust + GTK4)" on news.ycombinator.com
- [ ] Immediately post founder's comment:
  - Why you built it (no good Linux GUI for Claude Code)
  - Tech choices (Rust + GTK4, not Electron — why)
  - What it does (workspace, not just chat wrapper)
  - Current limitations (be honest)
  - What feedback you want
- [ ] Monitor HN for 2-3 hours, respond to every comment thoughtfully

**30 minutes after HN post:**
- [ ] Post to r/ClaudeAI — angle: "I built a native desktop app for Claude Code on Linux"
- [ ] Post to r/rust — angle: "Show project: GTK4 + Rust desktop app for AI coding"

**Evening:**
- [ ] Tweet/toot thread with screenshots (Twitter/X, Mastodon, Bluesky)
- [ ] Tag @rustlang, @AnthropicAI, use #rust #linux #gnome #opensource hashtags

### Thursday-Friday
- [ ] Post to r/linux — angle: "Native Linux desktop app for AI-assisted coding (no Electron)"
- [ ] Post to r/gnome — angle: "GTK4 + Rust app with native theme integration"
- [ ] Respond to all comments, issues, and DMs
- [ ] Fix any bugs that come up from new users (fast response to issues = trust)

### Weekend
- [ ] Review feedback, note feature requests
- [ ] Submit PR to This Week in Rust (github.com/rust-lang/this-week-in-rust) for next week's issue
- [ ] Nominate for "Crate of the Week" on users.rust-lang.org/t/crate-of-the-week/2704

---

## Week 2: Amplify

### Monday-Tuesday
- [ ] Send outreach emails to Linux tech media (use template from promotion-strategy.md):
  - It's FOSS / It's FOSS News
  - OMG! Ubuntu
  - Linux Uprising
  - 9to5Linux
- [ ] Submit PRs to awesome lists:
  - awesome-rust (github.com/rust-unofficial/awesome-rust)
  - awesome-gtk (github.com/valpackett/awesome-gtk)
  - Search for awesome-ai, awesome-llm, awesome-linux lists

### Wednesday-Thursday
- [ ] Post to GNOME Discourse (discourse.gnome.org) in Applications category
- [ ] Share in gtk-rs Matrix room
- [ ] Post in Rust Discord #showcase channel
- [ ] Post on Rust Users Forum (users.rust-lang.org) in Showcase

### Friday
- [ ] Email Console.dev (hello@console.dev) with project submission
- [ ] Submit to Changelog News (changelog.com/news)
- [ ] Write a short blog post / dev.to article: "Why I Built a Native Linux GUI for Claude Code with Rust and GTK4"
  - Focus on technical decisions (why GTK4 over Electron, vte4 for terminal, webkit6 for chat rendering)
  - Include architecture diagram
  - This becomes shareable content for others to link to

---

## Week 3: Content & Packaging

### Monday-Tuesday
- [ ] Start Flatpak packaging (create com.flycrys.app.yml manifest)
- [ ] Test Flatpak build locally
- [ ] Submit to Flathub (github.com/flathub/flathub)

### Wednesday-Thursday
- [ ] Reach out to 3-4 YouTube creators (pick from promotion-strategy.md list):
  - Brodie Robertson (most likely to cover niche Linux apps)
  - The Linux Experiment
  - TechHut
- [ ] Create a 2-minute demo video for YouTube outreach (screen recording with voiceover or text annotations)
- [ ] Record and publish a standalone YouTube video on your own channel (even if small — it's linkable content)

### Friday
- [ ] Post to r/commandline and r/LocalLLaMA (these are secondary communities)
- [ ] Share the dev.to article on relevant subreddits as a discussion piece
- [ ] Address accumulated GitHub issues and feature requests
- [ ] Release a minor version with any quick wins from user feedback (shows active development)

---

## Week 4: Sustain & Community

### Monday-Tuesday
- [ ] Review all analytics: GitHub traffic, referrers, star trajectory
- [ ] Identify which channels drove the most stars/traffic — double down on those
- [ ] Follow up with any media outlets that haven't responded

### Wednesday-Thursday
- [ ] Create a CONTRIBUTING.md with "good first issue" labels on GitHub
- [ ] Label 5-10 issues as "good first issue" to attract contributors
- [ ] Set up a simple way for users to report issues (GitHub Issues template)
- [ ] If Flathub PR is merged, announce on all channels

### Friday
- [ ] Write a "Week 4 retrospective" post (Twitter thread or short blog):
  - How many stars, downloads, contributors
  - Top feedback themes
  - What's coming next
  - Thank the community
- [ ] Plan the next month: focus on features users actually asked for

---

## Ongoing (After Week 4)

### Monthly
- [ ] Release updates with changelog (GitHub Releases)
- [ ] Post updates to r/ClaudeAI and r/rust when there's a meaningful new feature
- [ ] Share progress on Twitter/Mastodon

### Quarterly
- [ ] Submit to relevant conferences (FOSDEM Rust devroom CFP opens ~Oct for Feb event)
- [ ] Review and update awesome list entries if categories changed
- [ ] Check if new Claude Code features need FlyCrys updates — be first to support them

### Opportunistic
- [ ] When Anthropic announces Claude Code updates, post about FlyCrys support on the same day
- [ ] When "what's the best Linux AI tool?" threads appear on Reddit, comment with FlyCrys (don't be spammy — be helpful, mention it naturally)
- [ ] When Rust or GTK4 newsletters feature you, reshare and thank them

---

## Success Metrics

| Metric | Week 1 target | Week 4 target | 3-month target |
|--------|---------------|---------------|----------------|
| GitHub stars | 50-200 | 300-800 | 1000-3000 |
| Open issues (engagement) | 5-10 | 20-40 | 50+ |
| Contributors | 0-1 | 2-5 | 5-10 |
| .deb downloads | 20-50 | 100-300 | 500+ |
| Flathub installs | N/A | 0 (pending) | 50-200 |
| Media mentions | 1-2 | 3-5 | 5-10 |

These are realistic for a niche OSS project. For reference: most GTK4/Rust apps have 100-1000 stars. Opcode hit 21K because it was the first Claude Code GUI. FlyCrys can realistically aim for 500-3000 stars in the first 3 months if the HN launch goes well.

---

## What NOT to Do

1. **Don't spam** — posting to 10 subreddits on the same day looks desperate and gets flagged
2. **Don't ask for upvotes** — HN and Reddit detect this and penalize you
3. **Don't oversell** — "revolutionary AI IDE" invites ridicule. "Native Linux GUI for Claude Code" is honest and sufficient
4. **Don't ignore feedback** — every issue comment, Reddit reply, and HN thread is a chance to build trust
5. **Don't wait for perfection** — ship v0.2.x, gather feedback, iterate. The app is already functional.
6. **Don't create fake hype** — the Rust + GTK4 + Linux-native angle is genuinely interesting. Let the tech speak.
