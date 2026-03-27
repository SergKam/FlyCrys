/// Where a slash command originates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashCommandSource {
    BuiltIn,
    User,
    Project,
    Plugin,
}

/// A slash command / skill available for invocation.
#[derive(Debug, Clone)]
pub struct SlashCommand {
    pub name: String,
    pub description: String,
    pub argument_hint: String,
    pub source: SlashCommandSource,
}
