use std::sync::Mutex;

/// Tests that mutate HOME must hold this lock to avoid racing each other.
#[allow(dead_code)]
pub static HOME_LOCK: Mutex<()> = Mutex::new(());

/// Set HOME and XDG_CONFIG_HOME to a temp dir so `dirs::config_dir()` resolves there.
/// Must be called while holding HOME_LOCK.
#[allow(dead_code)]
pub unsafe fn set_test_home(path: &std::path::Path) {
    std::env::set_var("HOME", path);
    std::env::set_var("XDG_CONFIG_HOME", path.join(".config"));
}

/// Helper: call md_to_html with is_dark=false (light mode)
#[allow(dead_code)]
pub fn md_to_html_light(md: &str) -> String {
    flycrys::markdown::md_to_html(md, false)
}
