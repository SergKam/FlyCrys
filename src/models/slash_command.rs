use std::path::PathBuf;

/// Where a slash command originates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashCommandSource {
    BuiltIn,
    User,
    Project,
    Plugin,
}

/// Whether this is a single-file command or a directory-based skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashCommandKind {
    /// A `.md` file in a `commands/` directory.
    Command,
    /// A `SKILL.md` inside a `skills/<name>/` directory.
    Skill,
}

/// A slash command / skill available for invocation.
#[derive(Debug, Clone)]
pub struct SlashCommand {
    pub name: String,
    pub description: String,
    pub argument_hint: String,
    pub source: SlashCommandSource,
    pub kind: SlashCommandKind,
    /// Path to the `.md` file on disk. `None` for built-in commands.
    pub path: Option<PathBuf>,
}
