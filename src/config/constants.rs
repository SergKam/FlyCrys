// Centralized constants — every magic number from the codebase lives here.

// --- Timing ---
pub const AUTOSAVE_INTERVAL_SECS: u64 = 5;
pub const GIT_REFRESH_INTERVAL_SECS: u64 = 5;
pub const FILE_WATCHER_SYNC_MS: u64 = 200;
pub const TERMINAL_SAVE_INTERVAL_SECS: u64 = 30;

// --- UI Dimensions ---
pub const DEFAULT_WINDOW_WIDTH: i32 = 1200;
pub const DEFAULT_WINDOW_HEIGHT: i32 = 800;
pub const AGENT_PANEL_MIN_WIDTH: i32 = 420;
pub const TREE_PANE_DEFAULT_WIDTH: i32 = 300;
pub const INPUT_MAX_HEIGHT: i32 = 120;
pub const IMAGE_THUMBNAIL_WIDTH: i32 = 160;
pub const IMAGE_THUMBNAIL_HEIGHT: i32 = 120;
pub const AUTO_SCROLL_THRESHOLD: f64 = 20.0;

// --- Terminal ---
pub const TERMINAL_SCROLLBACK_LINES: i64 = 10_000;
pub const TERMINAL_FONT: &str = "Monospace 11";
pub const DEFAULT_SHELL: &str = "/bin/bash";

// --- Agent ---
pub const DEFAULT_CONTEXT_WINDOW: u64 = 200_000;

// --- Text/Display ---
pub const MAX_FILE_SIZE_BYTES: u64 = 10 * 1024 * 1024;
pub const DISPLAY_TRUNCATE_AT: usize = 60;
pub const DISPLAY_TRUNCATE_KEEP: usize = 57;
pub const OUTPUT_COLLAPSE_THRESHOLD: usize = 2000;
pub const OUTPUT_HEAD_TAIL_LINES: usize = 5;

// --- Chat History Pagination ---
pub const CHAT_TAIL_COUNT: usize = 20;
pub const CHAT_BATCH_SIZE: usize = 20;

// --- File Tree ---
pub const TREE_MAX_EXPAND_PASSES: usize = 30;

// --- Gutter ---
pub const GUTTER_CHAR_WIDTH_PX: i32 = 10;
pub const GUTTER_PADDING_PX: i32 = 12;

// --- Pane defaults (session.rs WorkspaceConfig) ---
pub const EDITOR_TERMINAL_SPLIT_DEFAULT: i32 = -1;

// --- Platform: known editors and browsers ---
pub const KNOWN_EDITORS: &[&str] = &[
    "gnome-text-editor",
    "gedit",
    "kate",
    "code",
    "xed",
    "pluma",
    "mousepad",
];

pub const KNOWN_BROWSERS: &[&str] = &[
    "sensible-browser",
    "x-www-browser",
    "google-chrome",
    "chromium",
    "chromium-browser",
    "firefox",
];

// --- File type mappings ---

/// Syntax alias mappings for file extensions not directly recognized by syntect.
/// Extension -> extension that syntect knows about.
/// Used as a fallback after syntect's built-in `find_syntax_by_extension` fails.
pub const SYNTAX_ALIASES: &[(&str, &str)] = &[
    ("mjs", "js"),
    ("cjs", "js"),
    ("jsx", "js"),
    ("tsx", "ts"),
    ("yml", "yaml"),
    ("mdx", "md"),
];

/// File extensions that support markdown preview.
pub const MARKDOWN_EXTENSIONS: &[&str] = &["md", "mdx"];

/// File extensions that are images (for preview).
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "svg"];

/// Supported image MIME types for attachment.
pub const SUPPORTED_IMAGE_MIME: &[&str] = &["image/png", "image/jpeg", "image/gif", "image/webp"];

/// File extensions considered highlightable (text/code files worth highlighting).
pub const HIGHLIGHTABLE_EXTENSIONS: &[&str] = &[
    "js",
    "mjs",
    "cjs",
    "jsx",
    "ts",
    "tsx",
    "json",
    "css",
    "scss",
    "less",
    "html",
    "htm",
    "xml",
    "svg",
    "yaml",
    "yml",
    "toml",
    "rs",
    "py",
    "rb",
    "go",
    "java",
    "c",
    "h",
    "cpp",
    "hpp",
    "sh",
    "bash",
    "zsh",
    "md",
    "mdx",
    "sql",
    "dockerfile",
    "makefile",
    "lua",
];

/// Quick command definition.
pub struct QuickCommand {
    pub label: &'static str,
    pub action_name: &'static str,
    pub prompt: &'static str,
}

/// Quick commands for the agent panel menu.
pub const QUICK_COMMANDS: &[QuickCommand] = &[
    QuickCommand {
        label: "Commit changes",
        action_name: "quick-commit",
        prompt: "commit all changes with a meaningful message",
    },
    QuickCommand {
        label: "Create GitHub PR",
        action_name: "quick-pr",
        prompt: "create a pull request for current branch",
    },
    QuickCommand {
        label: "Update documentation",
        action_name: "quick-docs",
        prompt: "update documentation to reflect recent changes",
    },
    QuickCommand {
        label: "Run lint, build, tests",
        action_name: "quick-test",
        prompt: "run lint, build, and tests, fix any errors",
    },
];
