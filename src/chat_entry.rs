use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk4 as gtk;
use std::cell::{Cell, RefCell};

/// Message type discriminator constants.
pub const MSG_TYPE_USER: u32 = 0;
pub const MSG_TYPE_ASSISTANT: u32 = 1;
pub const MSG_TYPE_TOOL_CALL: u32 = 2;
pub const MSG_TYPE_SYSTEM: u32 = 3;
pub const MSG_TYPE_THINKING: u32 = 4;

mod imp {
    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::ChatEntry)]
    pub struct ChatEntry {
        /// 0=User, 1=Assistant, 2=ToolCall, 3=System, 4=Thinking
        #[property(get, set)]
        msg_type: Cell<u32>,

        /// Text content (user text / assistant markdown / system text)
        #[property(get, set)]
        text: RefCell<String>,

        // --- Tool call data ---
        #[property(get, set)]
        tool_name: RefCell<String>,
        #[property(get, set)]
        tool_input: RefCell<String>,
        #[property(get, set)]
        tool_output: RefCell<String>,
        #[property(get, set)]
        tool_display_hint: RefCell<String>,
        /// File path extracted from tool input (empty string = none)
        #[property(get, set)]
        file_path: RefCell<String>,
        #[property(get, set)]
        tool_is_error: Cell<bool>,
        #[property(get, set)]
        tool_complete: Cell<bool>,
        #[property(get, set)]
        tool_expanded: Cell<bool>,

        /// True while this assistant entry is actively being streamed.
        #[property(get, set)]
        is_streaming: Cell<bool>,

        /// For user entries with images.
        pub(super) textures: RefCell<Vec<gtk::gdk::Texture>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ChatEntry {
        const NAME: &'static str = "FlyCrysChatEntry";
        type Type = super::ChatEntry;
        type ParentType = glib::Object;
    }

    #[glib::derived_properties]
    impl ObjectImpl for ChatEntry {}
}

glib::wrapper! {
    pub struct ChatEntry(ObjectSubclass<imp::ChatEntry>);
}

impl ChatEntry {
    pub fn new_user(text: &str) -> Self {
        glib::Object::builder()
            .property("msg-type", MSG_TYPE_USER)
            .property("text", text)
            .build()
    }

    pub fn new_user_with_images(text: &str, textures: Vec<gtk::gdk::Texture>) -> Self {
        let entry: Self = glib::Object::builder()
            .property("msg-type", MSG_TYPE_USER)
            .property("text", text)
            .build();
        *entry.imp().textures.borrow_mut() = textures;
        entry
    }

    pub fn new_assistant(text: &str) -> Self {
        glib::Object::builder()
            .property("msg-type", MSG_TYPE_ASSISTANT)
            .property("text", text)
            .build()
    }

    pub fn new_assistant_streaming() -> Self {
        glib::Object::builder()
            .property("msg-type", MSG_TYPE_ASSISTANT)
            .property("is-streaming", true)
            .build()
    }

    pub fn new_tool_call(
        name: &str,
        input_json: &str,
        display_hint: &str,
        file_path: &str,
    ) -> Self {
        glib::Object::builder()
            .property("msg-type", MSG_TYPE_TOOL_CALL)
            .property("tool-name", name)
            .property("tool-input", input_json)
            .property("tool-display-hint", display_hint)
            .property("file-path", file_path)
            .build()
    }

    pub fn new_system(text: &str) -> Self {
        glib::Object::builder()
            .property("msg-type", MSG_TYPE_SYSTEM)
            .property("text", text)
            .build()
    }

    pub fn new_thinking() -> Self {
        glib::Object::builder()
            .property("msg-type", MSG_TYPE_THINKING)
            .build()
    }

    pub fn textures(&self) -> Vec<gtk::gdk::Texture> {
        self.imp().textures.borrow().clone()
    }
}
