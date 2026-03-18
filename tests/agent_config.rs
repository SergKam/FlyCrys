mod common;
use common::HOME_LOCK;
use flycrys::session;

// ──────────────────────────────────────────────────────────────────────
// Agent config persistence
// ──────────────────────────────────────────────────────────────────────

#[test]
fn agent_config_save_load_roundtrip() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let config = session::AgentConfig {
        name: "TestAgent".to_string(),
        system_prompt: "You are a test agent.".to_string(),
        allowed_tools: vec!["Read".into(), "Grep".into()],
        model: Some("claude-sonnet-4-20250514".into()),
    };
    session::save_agent_config(&config);

    let loaded = session::load_agent_config("TestAgent").unwrap();
    assert_eq!(loaded.name, "TestAgent");
    assert_eq!(loaded.system_prompt, "You are a test agent.");
    assert_eq!(loaded.allowed_tools, vec!["Read", "Grep"]);
    assert_eq!(loaded.model.as_deref(), Some("claude-sonnet-4-20250514"));
}

#[test]
fn agent_config_case_insensitive_path() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let config = session::AgentConfig {
        name: "MyAgent".to_string(),
        system_prompt: String::new(),
        allowed_tools: Vec::new(),
        model: None,
    };
    session::save_agent_config(&config);

    // File is saved as lowercase, so loading by lowercase name should work
    let loaded = session::load_agent_config("myagent").unwrap();
    assert_eq!(loaded.name, "MyAgent");
}

#[test]
fn agent_config_nonexistent_returns_none() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    assert!(session::load_agent_config("NoSuchAgent").is_none());
}

#[test]
fn ensure_default_agents_creates_three_profiles() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    session::ensure_default_agents();

    let configs = session::list_agent_configs();
    assert_eq!(configs.len(), 3);

    let names: Vec<&str> = configs.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"Default"));
    assert!(names.contains(&"Security"));
    assert!(names.contains(&"Research"));
}

#[test]
fn ensure_default_agents_idempotent() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    session::ensure_default_agents();
    // Modify one agent
    let mut custom = session::load_agent_config("Default").unwrap();
    custom.system_prompt = "Custom prompt".to_string();
    session::save_agent_config(&custom);

    // Re-run: should NOT overwrite the existing file
    session::ensure_default_agents();

    let loaded = session::load_agent_config("Default").unwrap();
    assert_eq!(loaded.system_prompt, "Custom prompt");
}

#[test]
fn list_agent_configs_sorted_alphabetically() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    for name in ["Zeta", "Alpha", "Middle"] {
        session::save_agent_config(&session::AgentConfig {
            name: name.to_string(),
            system_prompt: String::new(),
            allowed_tools: Vec::new(),
            model: None,
        });
    }

    let configs = session::list_agent_configs();
    let names: Vec<&str> = configs.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["Alpha", "Middle", "Zeta"]);
}

#[test]
fn agent_config_overwrite_updates_fields() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let v1 = session::AgentConfig {
        name: "Evolving".to_string(),
        system_prompt: "v1".to_string(),
        allowed_tools: vec!["Read".into()],
        model: None,
    };
    session::save_agent_config(&v1);

    let v2 = session::AgentConfig {
        name: "Evolving".to_string(),
        system_prompt: "v2".to_string(),
        allowed_tools: vec!["Read".into(), "Write".into()],
        model: Some("opus".into()),
    };
    session::save_agent_config(&v2);

    let loaded = session::load_agent_config("Evolving").unwrap();
    assert_eq!(loaded.system_prompt, "v2");
    assert_eq!(loaded.allowed_tools.len(), 2);
    assert_eq!(loaded.model.as_deref(), Some("opus"));
}

// ──────────────────────────────────────────────────────────────────────
// AgentConfig serialization edge cases
// ──────────────────────────────────────────────────────────────────────

#[test]
fn agent_config_empty_tools_and_no_model() {
    let config = session::AgentConfig {
        name: "Minimal".to_string(),
        system_prompt: String::new(),
        allowed_tools: Vec::new(),
        model: None,
    };
    let json = serde_json::to_string(&config).unwrap();
    let restored: session::AgentConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.name, "Minimal");
    assert!(restored.system_prompt.is_empty());
    assert!(restored.allowed_tools.is_empty());
    assert!(restored.model.is_none());
}

#[test]
fn agent_config_with_unicode_prompt() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let config = session::AgentConfig {
        name: "Unicode".to_string(),
        system_prompt: "Ви — помічник з коду. 你好世界. 🎉".to_string(),
        allowed_tools: Vec::new(),
        model: None,
    };
    session::save_agent_config(&config);

    let loaded = session::load_agent_config("Unicode").unwrap();
    assert_eq!(loaded.system_prompt, "Ви — помічник з коду. 你好世界. 🎉");
}
