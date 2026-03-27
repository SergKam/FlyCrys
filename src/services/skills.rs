use std::path::{Path, PathBuf};

use crate::models::slash_command::{SlashCommand, SlashCommandSource};

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

/// Scan a `commands/` directory for `*.md` files.
fn scan_commands_dir(dir: &Path, source: SlashCommandSource, out: &mut Vec<SlashCommand>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        if let Some(cmd) = parse_command_file(&path, source) {
            out.push(cmd);
        }
    }
}

/// Scan a `skills/` directory for `*/SKILL.md` subdirectories.
fn scan_skills_dir(dir: &Path, source: SlashCommandSource, out: &mut Vec<SlashCommand>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let skill_file = entry.path().join("SKILL.md");
        if skill_file.exists()
            && let Some(cmd) = parse_command_file(&skill_file, source)
        {
            out.push(cmd);
        }
    }
}

/// Recursively scan plugin cache for commands/ and skills/ directories.
fn scan_plugins(cache_dir: &Path, out: &mut Vec<SlashCommand>) {
    let Ok(entries) = std::fs::read_dir(cache_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let plugin_dir = entry.path();
        if !plugin_dir.is_dir() {
            continue;
        }
        // Plugin cache structure can be nested; search recursively up to depth 4
        find_skill_dirs(&plugin_dir, 0, out);
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

/// Parse a `.md` file for YAML frontmatter (name, description, argument-hint).
/// Falls back to filename stem as name if no frontmatter.
fn parse_command_file(path: &PathBuf, source: SlashCommandSource) -> Option<SlashCommand> {
    let content = std::fs::read_to_string(path).ok()?;
    let file_stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    // For SKILL.md, use the parent directory name as fallback
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
    })
}

/// Extract name, description, and argument-hint from YAML frontmatter.
/// Returns (name, description, argument_hint).
fn parse_frontmatter(content: &str, default_name: &str) -> (String, String, String) {
    let mut name = default_name.to_string();
    let mut description = String::new();
    let mut argument_hint = String::new();

    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        // No frontmatter; try first heading as description
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

    // Find closing ---
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

    // If description was multiline (|), grab the first non-empty indented line
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

/// Deduplicate commands by name. Higher-priority sources override lower ones.
/// Priority: Project > User > Plugin > BuiltIn.
fn deduplicate_and_sort(commands: &mut Vec<SlashCommand>) {
    fn priority(source: SlashCommandSource) -> u8 {
        match source {
            SlashCommandSource::Project => 3,
            SlashCommandSource::User => 2,
            SlashCommandSource::Plugin => 1,
            SlashCommandSource::BuiltIn => 0,
        }
    }

    // Sort by name, then by priority descending so highest-priority comes first
    commands.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| priority(b.source).cmp(&priority(a.source)))
    });

    // Deduplicate: keep first occurrence (highest priority) of each name
    commands.dedup_by(|b, a| a.name.eq_ignore_ascii_case(&b.name));
}

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
            },
            SlashCommand {
                name: "test".into(),
                description: "project".into(),
                argument_hint: String::new(),
                source: SlashCommandSource::Project,
            },
        ];
        deduplicate_and_sort(&mut cmds);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].description, "project");
    }

    #[test]
    fn test_builtin_commands_present() {
        // Use a temp dir with no .claude/ to get only builtins
        let tmp = std::env::temp_dir().join("flycrys_test_empty_dir");
        let _ = std::fs::create_dir_all(&tmp);
        let cmds = discover_slash_commands(&tmp);
        assert!(cmds.iter().any(|c| c.name == "help"));
        assert!(cmds.iter().any(|c| c.name == "compact"));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
