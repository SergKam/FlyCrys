use std::sync::Mutex;

/// Tests that mutate HOME must hold this lock to avoid racing each other.
#[allow(dead_code)]
pub static HOME_LOCK: Mutex<()> = Mutex::new(());

/// Helper: call md_to_pango with is_dark=false (light mode)
#[allow(dead_code)]
pub fn md_to_pango_light(md: &str) -> String {
    flycrys::markdown::md_to_pango(md, false)
}
