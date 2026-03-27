use std::path::{Path, PathBuf};

use crate::models::slash_command::{SlashCommand, SlashCommandKind, SlashCommandSource};

/// Built-in Claude CLI slash commands relevant to a GUI context.
/// TUI-only commands (vim, terminal-setup, bug, login/logout, doctor,
/// config, permissions) are excluded — they require interactive terminal access.
const BUILTIN_COMMANDS: &[(&str, &str)] = &[
    ("clear", "Clear conversation history"),
    ("compact", "Compact conversation to save tokens"),
    ("cost", "Show token usage and cost for this session"),
    ("help", "Show available commands"),
    ("init", "Initialize a new CLAUDE.md project file"),
    ("memory", "Edit CLAUDE.md memory files"),
    ("model", "Switch AI model"),
    ("review", "Review a pull request"),
    ("status", "Show account and session status"),
];

// ── Discovery ────────────────────────────────────────────────────────────────

/// Discover all available slash commands from built-ins, user, project, and plugins.
pub fn discover_slash_commands(working_dir: &Path) -> Vec<SlashCommand> {
    let mut commands = Vec::new();

    // 1. Built-in commands
    for (name, desc) in BUILTIN_COMMANDS {
        commands.push(SlashCommand {
            name: (*name).to_string(),
            description: (*desc).to_string(),
            argument_hint: String::new(),
            source: SlashCommandSource::BuiltIn,
            kind: SlashCommandKind::Command,
            path: None,
        });
    }

    // 2. User-level commands and skills
    if let Some(home) = dirs::home_dir() {
        let claude_dir = home.join(".claude");
        scan_commands_dir(
            &claude_dir.join("commands"),
            SlashCommandSource::User,
            &mut commands,
        );
        scan_skills_dir(
            &claude_dir.join("skills"),
            SlashCommandSource::User,
            &mut commands,
        );

        // 3. Plugin commands and skills
        let plugins_cache = claude_dir.join("plugins").join("cache");
        scan_plugins(&plugins_cache, &mut commands);
    }

    // 4. Project-level commands and skills
    let project_claude = working_dir.join(".claude");
    scan_commands_dir(
        &project_claude.join("commands"),
        SlashCommandSource::Project,
        &mut commands,
    );
    scan_skills_dir(
        &project_claude.join("skills"),
        SlashCommandSource::Project,
        &mut commands,
    );

    // Sort alphabetically, deduplicate (project > user > plugin > built-in)
    deduplicate_and_sort(&mut commands);
    commands
}

// ── CRUD ─────────────────────────────────────────────────────────────────────

/// Read the full body content of a command/skill .md file.
pub fn read_command_body(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))
}

/// Write a command .md file with YAML frontmatter.
/// Returns the path of the written file.
pub fn save_command(
    base_dir: &Path,
    name: &str,
    description: &str,
    argument_hint: &str,
    body: &str,
    kind: SlashCommandKind,
) -> Result<PathBuf, String> {
    let safe_name = sanitize_filename(name);

    let file_path = match kind {
        SlashCommandKind::Command => {
            let dir = base_dir.join("commands");
            std::fs::create_dir_all(&dir)
                .map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;
            dir.join(format!("{safe_name}.md"))
        }
        SlashCommandKind::Skill => {
            let dir = base_dir.join("skills").join(&safe_name);
            std::fs::create_dir_all(&dir)
                .map_err(|e| format!("Failed to create {}: {e}", dir.display()))?;
            dir.join("SKILL.md")
        }
    };

    let mut content = String::from("---\n");
    content.push_str(&format!("name: {name}\n"));
    if !description.is_empty() {
        content.push_str(&format!(
            "description: \"{}\"\n",
            description.replace('"', "\\\"")
        ));
    }
    if !argument_hint.is_empty() {
        content.push_str(&format!(
            "argument-hint: \"{}\"\n",
            argument_hint.replace('"', "\\\"")
        ));
    }
    content.push_str("---\n");
    if !body.is_empty() {
        content.push('\n');
        content.push_str(body);
        if !body.ends_with('\n') {
            content.push('\n');
        }
    }

    std::fs::write(&file_path, &content)
        .map_err(|e| format!("Failed to write {}: {e}", file_path.display()))?;

    Ok(file_path)
}

/// Delete a command or skill from disk.
pub fn delete_command(path: &Path, kind: SlashCommandKind) -> Result<(), String> {
    match kind {
        SlashCommandKind::Command => std::fs::remove_file(path)
            .map_err(|e| format!("Failed to delete {}: {e}", path.display())),
        SlashCommandKind::Skill => {
            // path points to SKILL.md; remove parent directory
            let parent = path
                .parent()
                .ok_or_else(|| "Invalid skill path".to_string())?;
            std::fs::remove_dir_all(parent)
                .map_err(|e| format!("Failed to delete {}: {e}", parent.display()))
        }
    }
}

/// Return the base `.claude` directory for a given source.
pub fn claude_dir_for_source(source: SlashCommandSource, working_dir: &Path) -> Option<PathBuf> {
    match source {
        SlashCommandSource::User => dirs::home_dir().map(|h| h.join(".claude")),
        SlashCommandSource::Project => Some(working_dir.join(".claude")),
        _ => None,
    }
}

/// Extract the body (everything after the closing `---`) from file content.
pub fn extract_body(content: &str) -> String {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return content.to_string();
    }
    let after_open = trimmed[3..].trim_start_matches(['\n', '\r']);
    if let Some(close_idx) = after_open.find("\n---") {
        let after_close = &after_open[close_idx + 4..];
        after_close.trim_start_matches(['\n', '\r']).to_string()
    } else {
        content.to_string()
    }
}

// ── Scanning (internal) ──────────────────────────────────────────────────────

fn scan_commands_dir(dir: &Path, source: SlashCommandSource, out: &mut Vec<SlashCommand>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        if let Some(cmd) = parse_command_file(&path, source, SlashCommandKind::Command) {
            out.push(cmd);
        }
    }
}

fn scan_skills_dir(dir: &Path, source: SlashCommandSource, out: &mut Vec<SlashCommand>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let skill_file = entry.path().join("SKILL.md");
        if skill_file.exists()
            && let Some(cmd) = parse_command_file(&skill_file, source, SlashCommandKind::Skill)
        {
            out.push(cmd);
        }
    }
}

fn scan_plugins(cache_dir: &Path, out: &mut Vec<SlashCommand>) {
    let Ok(entries) = std::fs::read_dir(cache_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let plugin_dir = entry.path();
        if plugin_dir.is_dir() {
            find_skill_dirs(&plugin_dir, 0, out);
        }
    }
}

fn find_skill_dirs(dir: &Path, depth: u32, out: &mut Vec<SlashCommand>) {
    const MAX_DEPTH: u32 = 5;
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str == "commands" {
            scan_commands_dir(&path, SlashCommandSource::Plugin, out);
        } else if name_str == "skills" {
            scan_skills_dir(&path, SlashCommandSource::Plugin, out);
        } else if name_str != "node_modules" && name_str != ".git" {
            find_skill_dirs(&path, depth + 1, out);
        }
    }
}

fn parse_command_file(
    path: &PathBuf,
    source: SlashCommandSource,
    kind: SlashCommandKind,
) -> Option<SlashCommand> {
    let content = std::fs::read_to_string(path).ok()?;
    let file_stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    let default_name = if file_stem.eq_ignore_ascii_case("SKILL") || file_stem == "skill" {
        path.parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or(file_stem)
    } else {
        file_stem
    };

    let (name, description, argument_hint) = parse_frontmatter(&content, default_name);

    Some(SlashCommand {
        name,
        description,
        argument_hint,
        source,
        kind,
        path: Some(path.clone()),
    })
}

// ── Frontmatter parsing ──────────────────────────────────────────────────────

fn parse_frontmatter(content: &str, default_name: &str) -> (String, String, String) {
    let mut name = default_name.to_string();
    let mut description = String::new();
    let mut argument_hint = String::new();

    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        for line in content.lines() {
            let line = line.trim();
            if let Some(heading) = line.strip_prefix("# ") {
                description = heading.trim().to_string();
                break;
            }
            if !line.is_empty() {
                description = line.to_string();
                break;
            }
        }
        return (name, description, argument_hint);
    }

    let after_open = &trimmed[3..].trim_start_matches(['\n', '\r']);
    let Some(close_idx) = after_open.find("\n---") else {
        return (name, description, argument_hint);
    };
    let frontmatter = &after_open[..close_idx];

    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("name:") {
            let val = val.trim().trim_matches('"').trim_matches('\'');
            if !val.is_empty() {
                name = val.to_string();
            }
        } else if let Some(val) = line.strip_prefix("description:") {
            let val = val.trim().trim_matches('"').trim_matches('\'');
            if !val.is_empty() && val != "|" && val != ">" {
                description = val.to_string();
            }
        } else if let Some(val) = line.strip_prefix("argument-hint:") {
            let val = val.trim().trim_matches('"').trim_matches('\'');
            argument_hint = val.to_string();
        }
    }

    if description.is_empty() {
        let mut in_desc = false;
        for line in frontmatter.lines() {
            let trimmed_line = line.trim();
            if trimmed_line.starts_with("description:") {
                in_desc = true;
                continue;
            }
            if in_desc {
                if line.starts_with(' ') || line.starts_with('\t') {
                    let val = line.trim();
                    if !val.is_empty() {
                        description = val.to_string();
                        break;
                    }
                } else {
                    break;
                }
            }
        }
    }

    (name, description, argument_hint)
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn deduplicate_and_sort(commands: &mut Vec<SlashCommand>) {
    fn priority(source: SlashCommandSource) -> u8 {
        match source {
            SlashCommandSource::Project => 3,
            SlashCommandSource::User => 2,
            SlashCommandSource::Plugin => 1,
            SlashCommandSource::BuiltIn => 0,
        }
    }

    commands.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| priority(b.source).cmp(&priority(a.source)))
    });

    commands.dedup_by(|b, a| a.name.eq_ignore_ascii_case(&b.name));
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .to_lowercase()
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter_basic() {
        let content =
            "---\nname: test-cmd\ndescription: A test command\nargument-hint: <file>\n---\nBody";
        let (name, desc, hint) = parse_frontmatter(content, "fallback");
        assert_eq!(name, "test-cmd");
        assert_eq!(desc, "A test command");
        assert_eq!(hint, "<file>");
    }

    #[test]
    fn test_parse_frontmatter_no_frontmatter() {
        let content = "# My Command\n\nSome body text.";
        let (name, desc, hint) = parse_frontmatter(content, "my-command");
        assert_eq!(name, "my-command");
        assert_eq!(desc, "My Command");
        assert!(hint.is_empty());
    }

    #[test]
    fn test_parse_frontmatter_quoted_values() {
        let content = "---\nname: \"quoted-name\"\ndescription: 'Quoted desc'\n---\n";
        let (name, desc, _) = parse_frontmatter(content, "fallback");
        assert_eq!(name, "quoted-name");
        assert_eq!(desc, "Quoted desc");
    }

    #[test]
    fn test_deduplicate_prefers_project() {
        let mut cmds = vec![
            SlashCommand {
                name: "test".into(),
                description: "built-in".into(),
                argument_hint: String::new(),
                source: SlashCommandSource::BuiltIn,
                kind: SlashCommandKind::Command,
                path: None,
            },
            SlashCommand {
                name: "test".into(),
                description: "project".into(),
                argument_hint: String::new(),
                source: SlashCommandSource::Project,
                kind: SlashCommandKind::Command,
                path: None,
            },
        ];
        deduplicate_and_sort(&mut cmds);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].description, "project");
    }

    #[test]
    fn test_builtin_commands_present() {
        let tmp = std::env::temp_dir().join("flycrys_test_empty_dir");
        let _ = std::fs::create_dir_all(&tmp);
        let cmds = discover_slash_commands(&tmp);
        assert!(cmds.iter().any(|c| c.name == "help"));
        assert!(cmds.iter().any(|c| c.name == "compact"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_extract_body() {
        let content = "---\nname: foo\n---\nHello world\n";
        assert_eq!(extract_body(content), "Hello world\n");
    }

    #[test]
    fn test_extract_body_no_frontmatter() {
        let content = "Just plain text.";
        assert_eq!(extract_body(content), "Just plain text.");
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("My Command!"), "my-command-");
        assert_eq!(sanitize_filename("test_cmd-2"), "test_cmd-2");
    }

    #[test]
    fn test_save_and_read_command() {
        let tmp = std::env::temp_dir().join("flycrys_test_save_cmd");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let path = save_command(
            &tmp,
            "test-cmd",
            "A test",
            "<arg>",
            "Body text",
            SlashCommandKind::Command,
        )
        .unwrap();
        assert!(path.exists());

        let content = read_command_body(&path).unwrap();
        assert!(content.contains("name: test-cmd"));
        assert!(content.contains("Body text"));

        delete_command(&path, SlashCommandKind::Command).unwrap();
        assert!(!path.exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_save_and_delete_skill() {
        let tmp = std::env::temp_dir().join("flycrys_test_save_skill");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let path = save_command(
            &tmp,
            "my-skill",
            "A skill",
            "",
            "Skill body",
            SlashCommandKind::Skill,
        )
        .unwrap();
        assert!(path.exists());
        assert!(path.ends_with("skills/my-skill/SKILL.md"));

        delete_command(&path, SlashCommandKind::Skill).unwrap();
        assert!(!path.parent().unwrap().exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
