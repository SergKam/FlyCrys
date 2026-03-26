use serde::{Deserialize, Serialize};

/// Visual theme. Currently two variants, but structured for future extension
/// (e.g., HighContrast, Solarized, user-defined).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}

impl Theme {
    pub fn is_dark(self) -> bool {
        matches!(self, Theme::Dark)
    }

    pub fn toggle(self) -> Self {
        match self {
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::Light,
        }
    }
}

// Custom Serialize/Deserialize so old boolean values ("is_dark": true/false)
// are transparently handled alongside new string values ("theme": "Dark").
impl Serialize for Theme {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Theme::Light => serializer.serialize_str("Light"),
            Theme::Dark => serializer.serialize_str("Dark"),
        }
    }
}

impl<'de> Deserialize<'de> for Theme {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ThemeVisitor;

        impl serde::de::Visitor<'_> for ThemeVisitor {
            type Value = Theme;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(r#""Light", "Dark", or a boolean"#)
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Theme, E> {
                Ok(if v { Theme::Dark } else { Theme::Light })
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Theme, E> {
                match v {
                    "Dark" | "dark" => Ok(Theme::Dark),
                    "Light" | "light" => Ok(Theme::Light),
                    _ => Err(E::unknown_variant(v, &["Light", "Dark"])),
                }
            }
        }
        deserializer.deserialize_any(ThemeVisitor)
    }
}

/// How to display the current file: source code, rendered preview, or git diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanelMode {
    #[default]
    Source,
    Preview,
    Diff,
}

impl PanelMode {
    pub fn is_source(self) -> bool {
        matches!(self, PanelMode::Source)
    }
    pub fn is_preview(self) -> bool {
        matches!(self, PanelMode::Preview)
    }
    pub fn is_diff(self) -> bool {
        matches!(self, PanelMode::Diff)
    }
}

impl Serialize for PanelMode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            PanelMode::Source => serializer.serialize_str("Source"),
            PanelMode::Preview => serializer.serialize_str("Preview"),
            PanelMode::Diff => serializer.serialize_str("Diff"),
        }
    }
}

impl<'de> Deserialize<'de> for PanelMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl serde::de::Visitor<'_> for Visitor {
            type Value = PanelMode;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(r#""Source", "Preview", or "Diff""#)
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<PanelMode, E> {
                match v {
                    "Source" | "source" => Ok(PanelMode::Source),
                    "Preview" | "preview" => Ok(PanelMode::Preview),
                    "Diff" | "diff" => Ok(PanelMode::Diff),
                    _ => Err(E::unknown_variant(v, &["Source", "Preview", "Diff"])),
                }
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

// ── Legacy types (used only for backward-compatible deserialization) ─────────

/// Deprecated: use `PanelMode` instead. Kept for config migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ViewMode {
    #[default]
    Source,
    Preview,
}

impl<'de> Deserialize<'de> for ViewMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl serde::de::Visitor<'_> for V {
            type Value = ViewMode;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(r#""Source" or "Preview""#)
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<ViewMode, E> {
                Ok(if v {
                    ViewMode::Preview
                } else {
                    ViewMode::Source
                })
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<ViewMode, E> {
                match v {
                    "Source" | "source" => Ok(ViewMode::Source),
                    "Preview" | "preview" => Ok(ViewMode::Preview),
                    _ => Err(E::unknown_variant(v, &["Source", "Preview"])),
                }
            }
        }
        deserializer.deserialize_any(V)
    }
}

/// Deprecated: use `PanelMode` instead. Kept for config migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum DiffMode {
    #[default]
    Hidden,
    Visible,
}

impl<'de> Deserialize<'de> for DiffMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl serde::de::Visitor<'_> for V {
            type Value = DiffMode;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(r#""Hidden" or "Visible""#)
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<DiffMode, E> {
                Ok(if v {
                    DiffMode::Visible
                } else {
                    DiffMode::Hidden
                })
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<DiffMode, E> {
                match v {
                    "Hidden" | "hidden" => Ok(DiffMode::Hidden),
                    "Visible" | "visible" => Ok(DiffMode::Visible),
                    _ => Err(E::unknown_variant(v, &["Hidden", "Visible"])),
                }
            }
        }
        deserializer.deserialize_any(V)
    }
}

/// Notification preferences. Boolean today, but ready for levels like
/// ErrorsOnly, All, None, or per-agent granularity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NotificationLevel {
    #[default]
    All,
    Disabled,
}

impl NotificationLevel {
    pub fn is_enabled(self) -> bool {
        matches!(self, NotificationLevel::All)
    }

    pub fn toggle(self) -> Self {
        match self {
            NotificationLevel::All => NotificationLevel::Disabled,
            NotificationLevel::Disabled => NotificationLevel::All,
        }
    }
}

impl Serialize for NotificationLevel {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            NotificationLevel::All => serializer.serialize_str("All"),
            NotificationLevel::Disabled => serializer.serialize_str("Disabled"),
        }
    }
}

impl<'de> Deserialize<'de> for NotificationLevel {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl serde::de::Visitor<'_> for Visitor {
            type Value = NotificationLevel;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(r#""All", "Disabled", or a boolean"#)
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<NotificationLevel, E> {
                Ok(if v {
                    NotificationLevel::All
                } else {
                    NotificationLevel::Disabled
                })
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<NotificationLevel, E> {
                match v {
                    "All" | "all" => Ok(NotificationLevel::All),
                    "Disabled" | "disabled" => Ok(NotificationLevel::Disabled),
                    _ => Err(E::unknown_variant(v, &["All", "Disabled"])),
                }
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

/// Agent completion result — not just error/success.
/// Could later include Cancelled, TimedOut, ContextExhausted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentOutcome {
    Success,
    Error,
}

/// What kind of item in the file tree context menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeItemKind {
    File,
    Directory,
}

impl TreeItemKind {
    pub fn is_dir(self) -> bool {
        matches!(self, TreeItemKind::Directory)
    }
}
