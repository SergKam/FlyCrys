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

        // --- Widget cache (not GObject properties) ---
        /// The cached top-level widget for this entry.
        pub(super) cached_widget: RefCell<Option<gtk::Widget>>,
        /// For assistant entries: the Label used for streaming updates.
        pub(super) text_label: RefCell<Option<gtk::Label>>,
        /// For tool entries: spinner, triangle, content box.
        pub(super) tool_spinner: RefCell<Option<gtk::Spinner>>,
        pub(super) tool_triangle: RefCell<Option<gtk::Label>>,
        pub(super) tool_content_box: RefCell<Option<gtk::Box>>,

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

    // --- Widget cache accessors ---

    pub fn cached_widget(&self) -> Option<gtk::Widget> {
        self.imp().cached_widget.borrow().clone()
    }

    pub fn set_cached_widget(&self, widget: Option<gtk::Widget>) {
        *self.imp().cached_widget.borrow_mut() = widget;
    }

    pub fn text_label(&self) -> Option<gtk::Label> {
        self.imp().text_label.borrow().clone()
    }

    pub fn set_text_label(&self, label: Option<gtk::Label>) {
        *self.imp().text_label.borrow_mut() = label;
    }

    pub fn tool_spinner_widget(&self) -> Option<gtk::Spinner> {
        self.imp().tool_spinner.borrow().clone()
    }

    pub fn set_tool_spinner_widget(&self, spinner: Option<gtk::Spinner>) {
        *self.imp().tool_spinner.borrow_mut() = spinner;
    }

    pub fn tool_triangle_widget(&self) -> Option<gtk::Label> {
        self.imp().tool_triangle.borrow().clone()
    }

    pub fn set_tool_triangle_widget(&self, label: Option<gtk::Label>) {
        *self.imp().tool_triangle.borrow_mut() = label;
    }

    pub fn tool_content_box_widget(&self) -> Option<gtk::Box> {
        self.imp().tool_content_box.borrow().clone()
    }

    pub fn set_tool_content_box_widget(&self, bx: Option<gtk::Box>) {
        *self.imp().tool_content_box.borrow_mut() = bx;
    }

    pub fn textures(&self) -> Vec<gtk::gdk::Texture> {
        self.imp().textures.borrow().clone()
    }
}
