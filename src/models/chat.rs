use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum ChatMessage {
    #[serde(rename = "user")]
    User { text: String },
    #[serde(rename = "assistant")]
    AssistantText { text: String },
    #[serde(rename = "tool")]
    ToolCall {
        tool_name: String,
        tool_input: String,
        output: String,
        is_error: bool,
    },
    #[serde(rename = "system")]
    System { text: String },
}
