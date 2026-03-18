mod common;
use common::HOME_LOCK;
use flycrys::session::{self, AppConfig, WorkspaceConfig};

// ──────────────────────────────────────────────────────────────────────
// Session: WorkspaceConfig
// ──────────────────────────────────────────────────────────────────────

#[test]
fn workspace_config_defaults() {
    let ws = WorkspaceConfig::new("/home/user/project");
    assert_eq!(ws.working_directory, "/home/user/project");
    assert_eq!(ws.tree_pane_width, 300);
    assert_eq!(ws.agent_pane_width, 420);
    assert!(ws.open_file.is_none());
    assert!(!ws.terminal_visible);
    // UUID should be valid
    assert_eq!(ws.id.len(), 36);
}

#[test]
fn workspace_tab_label() {
    let ws = WorkspaceConfig::new("/home/user/my-project");
    assert_eq!(ws.tab_label(), "my-project");
}

#[test]
fn workspace_tab_label_root() {
    let mut ws = WorkspaceConfig::new("/");
    ws.working_directory = "/".to_string();
    assert_eq!(ws.tab_label(), "/");
}

#[test]
fn workspace_unique_ids() {
    let ws1 = WorkspaceConfig::new("/a");
    let ws2 = WorkspaceConfig::new("/b");
    assert_ne!(ws1.id, ws2.id, "each workspace should get a unique UUID");
}

// ──────────────────────────────────────────────────────────────────────
// Session: dedup_labels
// ──────────────────────────────────────────────────────────────────────

#[test]
fn dedup_labels_no_duplicates() {
    let configs = vec![
        WorkspaceConfig::new("/home/user/alpha"),
        WorkspaceConfig::new("/home/user/beta"),
        WorkspaceConfig::new("/home/user/gamma"),
    ];
    let labels = session::dedup_labels(&configs);
    assert_eq!(labels, vec!["alpha", "beta", "gamma"]);
}

#[test]
fn dedup_labels_with_duplicates() {
    let configs = vec![
        WorkspaceConfig::new("/home/user/project"),
        WorkspaceConfig::new("/var/lib/project"),
        WorkspaceConfig::new("/opt/project"),
    ];
    let labels = session::dedup_labels(&configs);
    assert_eq!(labels, vec!["project (1)", "project (2)", "project (3)"]);
}

#[test]
fn dedup_labels_mixed() {
    let configs = vec![
        WorkspaceConfig::new("/home/user/alpha"),
        WorkspaceConfig::new("/home/user/beta"),
        WorkspaceConfig::new("/var/lib/alpha"),
    ];
    let labels = session::dedup_labels(&configs);
    assert_eq!(labels, vec!["alpha (1)", "beta", "alpha (2)"]);
}

#[test]
fn dedup_labels_empty() {
    let labels = session::dedup_labels(&[]);
    assert!(labels.is_empty());
}

#[test]
fn dedup_labels_single() {
    let configs = vec![WorkspaceConfig::new("/home/user/solo")];
    let labels = session::dedup_labels(&configs);
    assert_eq!(labels, vec!["solo"]);
}

// ──────────────────────────────────────────────────────────────────────
// Session: AppConfig defaults
// ──────────────────────────────────────────────────────────────────────

#[test]
fn app_config_defaults() {
    let config = AppConfig::default();
    assert_eq!(config.active_tab, 0);
    assert!(config.workspace_ids.is_empty());
    assert_eq!(config.window_width, 1400);
    assert_eq!(config.window_height, 800);
    assert!(!config.is_dark);
}

// ──────────────────────────────────────────────────────────────────────
// Session: serialization roundtrip
// ──────────────────────────────────────────────────────────────────────

#[test]
fn app_config_serde_roundtrip() {
    let config = AppConfig {
        active_tab: 2,
        workspace_ids: vec!["abc-123".into(), "def-456".into()],
        window_width: 1920,
        window_height: 1080,
        is_dark: true,
    };
    let json = serde_json::to_string(&config).unwrap();
    let restored: AppConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.active_tab, 2);
    assert_eq!(restored.workspace_ids.len(), 2);
    assert_eq!(restored.window_width, 1920);
    assert!(restored.is_dark);
}

#[test]
fn workspace_config_serde_roundtrip() {
    let mut ws = WorkspaceConfig::new("/home/user/test");
    ws.open_file = Some("/home/user/test/main.rs".into());
    ws.terminal_visible = true;
    ws.tree_pane_width = 400;

    let json = serde_json::to_string(&ws).unwrap();
    let restored: WorkspaceConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.id, ws.id);
    assert_eq!(restored.working_directory, "/home/user/test");
    assert_eq!(restored.open_file.as_deref(), Some("/home/user/test/main.rs"));
    assert!(restored.terminal_visible);
    assert_eq!(restored.tree_pane_width, 400);
}

// ──────────────────────────────────────────────────────────────────────
// Session: filesystem persistence
// ──────────────────────────────────────────────────────────────────────

#[test]
fn session_save_load_roundtrip() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    // SAFETY: guarded by HOME_LOCK so no parallel mutation
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let config = AppConfig {
        active_tab: 1,
        workspace_ids: vec!["ws-1".into()],
        window_width: 1600,
        window_height: 900,
        is_dark: true,
    };
    session::save_app_config(&config);
    let loaded = session::load_app_config();
    assert_eq!(loaded.active_tab, 1);
    assert_eq!(loaded.workspace_ids, vec!["ws-1"]);
    assert!(loaded.is_dark);
}

#[test]
fn workspace_save_load_roundtrip() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    // SAFETY: guarded by HOME_LOCK so no parallel mutation
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let mut ws = WorkspaceConfig::new("/home/user/myproject");
    ws.open_file = Some("/home/user/myproject/lib.rs".into());

    session::save_workspace_config(&ws);
    let loaded = session::load_workspace_config(&ws.id).unwrap();
    assert_eq!(loaded.working_directory, "/home/user/myproject");
    assert_eq!(loaded.open_file.as_deref(), Some("/home/user/myproject/lib.rs"));
}

#[test]
fn workspace_delete() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    // SAFETY: guarded by HOME_LOCK so no parallel mutation
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let ws = WorkspaceConfig::new("/tmp/test");
    session::save_workspace_config(&ws);
    assert!(session::load_workspace_config(&ws.id).is_some());

    session::delete_workspace_config(&ws.id);
    assert!(session::load_workspace_config(&ws.id).is_none());
}

#[test]
fn load_nonexistent_workspace() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    // SAFETY: guarded by HOME_LOCK so no parallel mutation
    unsafe { std::env::set_var("HOME", tmp.path()) };

    assert!(session::load_workspace_config("does-not-exist").is_none());
}

#[test]
fn load_missing_app_config_returns_default() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    // SAFETY: guarded by HOME_LOCK so no parallel mutation
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let config = session::load_app_config();
    assert_eq!(config.active_tab, 0);
    assert!(config.workspace_ids.is_empty());
}

// ──────────────────────────────────────────────────────────────────────
// Integration: workspace + agent config cross-module
// ──────────────────────────────────────────────────────────────────────

#[test]
fn workspace_references_agent_profiles() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    session::ensure_default_agents();

    let mut ws = WorkspaceConfig::new("/home/user/project");
    ws.agent_1_profile = "Security".to_string();
    ws.agent_1_session_id = Some("sess-abc".to_string());
    session::save_workspace_config(&ws);

    let loaded_ws = session::load_workspace_config(&ws.id).unwrap();
    assert_eq!(loaded_ws.agent_1_profile, "Security");
    assert_eq!(loaded_ws.agent_1_session_id.as_deref(), Some("sess-abc"));

    // Verify referenced profiles actually exist
    let agent1 = session::load_agent_config(&loaded_ws.agent_1_profile).unwrap();
    assert!(agent1.system_prompt.contains("security"));
}

#[test]
fn full_app_state_restore_flow() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    session::ensure_default_agents();

    // Simulate creating an app with 3 workspaces
    let ws1 = WorkspaceConfig::new("/home/user/frontend");
    let mut ws2 = WorkspaceConfig::new("/home/user/backend");
    ws2.agent_1_profile = "Security".to_string();
    ws2.open_file = Some("/home/user/backend/src/main.rs".into());
    ws2.terminal_visible = true;
    let ws3 = WorkspaceConfig::new("/home/user/docs");

    session::save_workspace_config(&ws1);
    session::save_workspace_config(&ws2);
    session::save_workspace_config(&ws3);

    let app = session::AppConfig {
        active_tab: 1,
        workspace_ids: vec![ws1.id.clone(), ws2.id.clone(), ws3.id.clone()],
        window_width: 1920,
        window_height: 1080,
        is_dark: true,
    };
    session::save_app_config(&app);

    // Full restore
    let restored_app = session::load_app_config();
    assert_eq!(restored_app.active_tab, 1);
    assert!(restored_app.is_dark);

    let workspaces: Vec<WorkspaceConfig> = restored_app
        .workspace_ids
        .iter()
        .filter_map(|id| session::load_workspace_config(id))
        .collect();
    assert_eq!(workspaces.len(), 3);

    // The active workspace (index 1) should have Security profile and open file
    let active = &workspaces[1];
    assert_eq!(active.agent_1_profile, "Security");
    assert_eq!(active.open_file.as_deref(), Some("/home/user/backend/src/main.rs"));
    assert!(active.terminal_visible);

    // Agent profiles referenced by workspaces should be loadable
    let profile = session::load_agent_config(&active.agent_1_profile).unwrap();
    assert!(!profile.system_prompt.is_empty());

    // Tab labels should be correct
    let labels = session::dedup_labels(&workspaces);
    assert_eq!(labels, vec!["frontend", "backend", "docs"]);
}

// ──────────────────────────────────────────────────────────────────────
// Config corruption & recovery
// ──────────────────────────────────────────────────────────────────────

#[test]
fn corrupt_app_config_returns_default() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    // Write garbage to config file
    let dir = tmp.path().join(".config").join("flycrys");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("config.json"), "{{not valid json!!}").unwrap();

    let config = session::load_app_config();
    assert_eq!(config.active_tab, 0);
    assert!(config.workspace_ids.is_empty());
    assert_eq!(config.window_width, 1400);
}

#[test]
fn corrupt_workspace_config_returns_none() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let dir = tmp.path().join(".config").join("flycrys").join("sessions");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("bad-id.json"), "BROKEN").unwrap();

    assert!(session::load_workspace_config("bad-id").is_none());
}

#[test]
fn corrupt_agent_config_returns_none() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let dir = tmp.path().join(".config").join("flycrys").join("agents");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("broken.json"), "not json").unwrap();

    assert!(session::load_agent_config("broken").is_none());
}

#[test]
fn partial_app_config_fills_defaults() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let dir = tmp.path().join(".config").join("flycrys");
    std::fs::create_dir_all(&dir).unwrap();
    // JSON with only some fields
    std::fs::write(
        dir.join("config.json"),
        r#"{"active_tab":5,"workspace_ids":["x"],"window_width":800,"window_height":600,"is_dark":true}"#,
    ).unwrap();

    let config = session::load_app_config();
    assert_eq!(config.active_tab, 5);
    assert_eq!(config.workspace_ids, vec!["x"]);
    assert!(config.is_dark);
}

#[test]
fn workspace_config_missing_optional_fields_uses_defaults() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let dir = tmp.path().join(".config").join("flycrys").join("sessions");
    std::fs::create_dir_all(&dir).unwrap();
    // Minimal valid workspace JSON — missing agent profiles and session IDs
    std::fs::write(
        dir.join("minimal.json"),
        r#"{
            "id":"minimal",
            "working_directory":"/tmp/test",
            "tree_pane_width":250,
            "editor_terminal_split":-1,
            "agent_pane_width":900,
            "agent_split_position":-1,
            "open_file":null,
            "terminal_visible":false
        }"#,
    ).unwrap();

    let ws = session::load_workspace_config("minimal").unwrap();
    assert_eq!(ws.agent_1_profile, "Default");
    assert!(ws.agent_1_session_id.is_none());
}

// ──────────────────────────────────────────────────────────────────────
// Concurrent workspace management
// ──────────────────────────────────────────────────────────────────────

#[test]
fn many_workspaces_create_and_selective_delete() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let mut ids = Vec::new();
    for i in 0..10 {
        let ws = WorkspaceConfig::new(&format!("/project/{i}"));
        ids.push(ws.id.clone());
        session::save_workspace_config(&ws);
    }

    // Delete even-indexed workspaces
    for i in (0..10).step_by(2) {
        session::delete_workspace_config(&ids[i]);
    }

    // Odd ones survive
    for i in 0..10 {
        let loaded = session::load_workspace_config(&ids[i]);
        if i % 2 == 0 {
            assert!(loaded.is_none(), "workspace {i} should be deleted");
        } else {
            let ws = loaded.unwrap();
            assert_eq!(ws.working_directory, format!("/project/{i}"));
        }
    }
}

#[test]
fn workspace_config_update_preserves_id() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", tmp.path()) };

    let mut ws = WorkspaceConfig::new("/home/user/project");
    let original_id = ws.id.clone();
    session::save_workspace_config(&ws);

    // Simulate user changing settings
    ws.tree_pane_width = 500;
    ws.agent_pane_width = 700;
    ws.open_file = Some("/home/user/project/README.md".into());
    ws.terminal_visible = true;
    ws.agent_1_profile = "Security".to_string();
    ws.agent_1_session_id = Some("sess-xyz".to_string());
    session::save_workspace_config(&ws);

    let loaded = session::load_workspace_config(&original_id).unwrap();
    assert_eq!(loaded.id, original_id);
    assert_eq!(loaded.tree_pane_width, 500);
    assert_eq!(loaded.agent_pane_width, 700);
    assert_eq!(loaded.open_file.as_deref(), Some("/home/user/project/README.md"));
    assert!(loaded.terminal_visible);
    assert_eq!(loaded.agent_1_profile, "Security");
    assert_eq!(loaded.agent_1_session_id.as_deref(), Some("sess-xyz"));
}

// ──────────────────────────────────────────────────────────────────────
// e2e: session multiple workspaces
// ──────────────────────────────────────────────────────────────────────

#[test]
fn e2e_session_multiple_workspaces() {
    let _lock = HOME_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    // SAFETY: guarded by HOME_LOCK so no parallel mutation
    unsafe { std::env::set_var("HOME", tmp.path()) };

    // Create and save multiple workspaces
    let ws1 = WorkspaceConfig::new("/home/user/project-a");
    let ws2 = WorkspaceConfig::new("/home/user/project-b");
    let ws3 = WorkspaceConfig::new("/opt/project-a");

    session::save_workspace_config(&ws1);
    session::save_workspace_config(&ws2);
    session::save_workspace_config(&ws3);

    let config = AppConfig {
        active_tab: 1,
        workspace_ids: vec![ws1.id.clone(), ws2.id.clone(), ws3.id.clone()],
        window_width: 1920,
        window_height: 1080,
        is_dark: false,
    };
    session::save_app_config(&config);

    // Reload everything
    let loaded_config = session::load_app_config();
    assert_eq!(loaded_config.workspace_ids.len(), 3);

    let loaded_workspaces: Vec<WorkspaceConfig> = loaded_config
        .workspace_ids
        .iter()
        .filter_map(|id| session::load_workspace_config(id))
        .collect();
    assert_eq!(loaded_workspaces.len(), 3);

    // Verify dedup labels work on restored workspaces
    let labels = session::dedup_labels(&loaded_workspaces);
    assert_eq!(labels[0], "project-a (1)");
    assert_eq!(labels[1], "project-b");
    assert_eq!(labels[2], "project-a (2)");

    // Delete one workspace and verify
    session::delete_workspace_config(&ws2.id);
    assert!(session::load_workspace_config(&ws2.id).is_none());
    assert!(session::load_workspace_config(&ws1.id).is_some());
}
