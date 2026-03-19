use flycrys::agent_process::AgentEvent;

// ──────────────────────────────────────────────────────────────────────
// AgentEvent: JSON deserialization
// ──────────────────────────────────────────────────────────────────────

#[test]
fn agent_event_system() {
    let json = r#"{"type":"system","subtype":"init","session_id":"sess-123"}"#;
    let event: AgentEvent = serde_json::from_str(json).unwrap();
    match event {
        AgentEvent::System {
            subtype,
            session_id,
            ..
        } => {
            assert_eq!(subtype.as_deref(), Some("init"));
            assert_eq!(session_id.as_deref(), Some("sess-123"));
        }
        _ => panic!("expected System event"),
    }
}

#[test]
fn agent_event_stream_text_delta() {
    let json = r#"{
        "type": "stream_event",
        "event": {
            "type": "content_block_delta",
            "index": 0,
            "delta": {
                "type": "text_delta",
                "text": "Hello world"
            }
        }
    }"#;
    let event: AgentEvent = serde_json::from_str(json).unwrap();
    match event {
        AgentEvent::StreamEvent { event } => {
            assert_eq!(event.event_type, "content_block_delta");
            assert_eq!(event.index, Some(0));
            let delta = event.delta.unwrap();
            assert_eq!(delta.text.as_deref(), Some("Hello world"));
        }
        _ => panic!("expected StreamEvent"),
    }
}

#[test]
fn agent_event_stream_content_block_start() {
    let json = r#"{
        "type": "stream_event",
        "event": {
            "type": "content_block_start",
            "index": 1,
            "content_block": {
                "type": "tool_use",
                "id": "tool_123",
                "name": "Read"
            }
        }
    }"#;
    let event: AgentEvent = serde_json::from_str(json).unwrap();
    match event {
        AgentEvent::StreamEvent { event } => {
            assert_eq!(event.event_type, "content_block_start");
            let block = event.content_block.unwrap();
            assert_eq!(block.block_type, "tool_use");
            assert_eq!(block.name.as_deref(), Some("Read"));
            assert_eq!(block.id.as_deref(), Some("tool_123"));
        }
        _ => panic!("expected StreamEvent"),
    }
}

#[test]
fn agent_event_assistant() {
    let json = r#"{
        "type": "assistant",
        "message": {
            "content": [
                {"type": "text", "text": "Here is my response"}
            ]
        }
    }"#;
    let event: AgentEvent = serde_json::from_str(json).unwrap();
    match event {
        AgentEvent::Assistant { message } => {
            let content = message.content.unwrap();
            assert_eq!(content.len(), 1);
            assert_eq!(content[0].block_type, "text");
            assert_eq!(content[0].text.as_deref(), Some("Here is my response"));
        }
        _ => panic!("expected Assistant event"),
    }
}

#[test]
fn agent_event_user_tool_result() {
    let json = r#"{
        "type": "user",
        "tool_use_result": {
            "stdout": "file contents here",
            "stderr": "",
            "is_error": false,
            "tool_use_id": "tool_456",
            "content": "file contents here"
        },
        "message": {}
    }"#;
    let event: AgentEvent = serde_json::from_str(json).unwrap();
    match event {
        AgentEvent::User {
            tool_use_result, ..
        } => {
            let result = tool_use_result.unwrap();
            assert_eq!(result.stdout, "file contents here");
            assert!(!result.is_error);
            assert_eq!(result.tool_use_id.as_deref(), Some("tool_456"));
        }
        _ => panic!("expected User event"),
    }
}

#[test]
fn agent_event_result() {
    let json = r#"{
        "type": "result",
        "result": "Task completed successfully",
        "total_cost_usd": 0.0342,
        "num_turns": 5,
        "is_error": false
    }"#;
    let event: AgentEvent = serde_json::from_str(json).unwrap();
    match event {
        AgentEvent::Result {
            result,
            total_cost_usd,
            num_turns,
            is_error,
            ..
        } => {
            assert_eq!(result.as_deref(), Some("Task completed successfully"));
            assert!((total_cost_usd - 0.0342).abs() < 0.0001);
            assert_eq!(num_turns, 5);
            assert!(!is_error);
        }
        _ => panic!("expected Result event"),
    }
}

#[test]
fn agent_event_result_with_error() {
    let json = r#"{
        "type": "result",
        "result": "Something went wrong",
        "total_cost_usd": 0.01,
        "num_turns": 1,
        "is_error": true
    }"#;
    let event: AgentEvent = serde_json::from_str(json).unwrap();
    match event {
        AgentEvent::Result { is_error, .. } => assert!(is_error),
        _ => panic!("expected Result event"),
    }
}

#[test]
fn agent_event_unknown_type() {
    let json = r#"{"type":"some_future_event","data":123}"#;
    let event: AgentEvent = serde_json::from_str(json).unwrap();
    assert!(matches!(event, AgentEvent::Unknown));
}

#[test]
fn agent_event_result_missing_optional_fields() {
    let json = r#"{"type":"result"}"#;
    let event: AgentEvent = serde_json::from_str(json).unwrap();
    match event {
        AgentEvent::Result {
            result,
            total_cost_usd,
            num_turns,
            is_error,
            ..
        } => {
            assert!(result.is_none());
            assert_eq!(total_cost_usd, 0.0);
            assert_eq!(num_turns, 0);
            assert!(!is_error);
        }
        _ => panic!("expected Result event"),
    }
}

#[test]
fn agent_event_system_minimal() {
    let json = r#"{"type":"system"}"#;
    let event: AgentEvent = serde_json::from_str(json).unwrap();
    match event {
        AgentEvent::System {
            subtype,
            session_id,
            ..
        } => {
            assert!(subtype.is_none());
            assert!(session_id.is_none());
        }
        _ => panic!("expected System event"),
    }
}

// ──────────────────────────────────────────────────────────────────────
// AgentEvent: complex multi-turn conversations
// ──────────────────────────────────────────────────────────────────────

#[test]
fn e2e_agent_multi_tool_conversation() {
    // Simulate: init -> text -> tool_use(Read) -> result(Read) -> tool_use(Edit) -> result(Edit) -> final text -> result
    let events_json = [
        r#"{"type":"system","subtype":"init","session_id":"multi-1"}"#,
        // First text block
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Let me read the file."}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
        // Tool use: Read
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"t1","name":"Read"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"file_path\":\"/src/main.rs\"}"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_stop","index":1}}"#,
        // Tool result
        r#"{"type":"user","tool_use_result":{"stdout":"fn main() {}","stderr":"","is_error":false,"tool_use_id":"t1","content":"fn main() {}"},"message":{}}"#,
        // Second text block
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Now editing."}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
        // Tool use: Edit
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"t2","name":"Edit"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"old\":\"main\",\"new\":\"app\"}"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_stop","index":1}}"#,
        // Tool result
        r#"{"type":"user","tool_use_result":{"stdout":"fn app() {}","stderr":"","is_error":false,"tool_use_id":"t2","content":"fn app() {}"},"message":{}}"#,
        // Final result
        r#"{"type":"result","result":"Done","total_cost_usd":0.02,"num_turns":3,"is_error":false}"#,
    ];

    let mut session_id = String::new();
    let mut tool_names = Vec::new();
    let mut tool_results = Vec::new();
    let mut text_chunks = Vec::new();
    let mut total_cost = 0.0;
    let mut turns = 0;

    for json in events_json {
        let event: AgentEvent = serde_json::from_str(json).unwrap();
        match event {
            AgentEvent::System {
                session_id: sid, ..
            } => {
                session_id = sid.unwrap_or_default();
            }
            AgentEvent::StreamEvent { event } => {
                if let Some(block) = event.content_block {
                    if let Some(name) = block.name {
                        tool_names.push(name);
                    }
                }
                if let Some(delta) = event.delta {
                    if let Some(text) = delta.text {
                        text_chunks.push(text);
                    }
                }
            }
            AgentEvent::User {
                tool_use_result, ..
            } => {
                if let Some(r) = tool_use_result {
                    tool_results.push(r.stdout);
                }
            }
            AgentEvent::Result {
                total_cost_usd,
                num_turns,
                is_error,
                ..
            } => {
                assert!(!is_error);
                total_cost = total_cost_usd;
                turns = num_turns;
            }
            _ => {}
        }
    }

    assert_eq!(session_id, "multi-1");
    assert_eq!(tool_names, vec!["Read", "Edit"]);
    assert_eq!(tool_results, vec!["fn main() {}", "fn app() {}"]);
    assert_eq!(text_chunks.join(""), "Let me read the file.Now editing.");
    assert!(total_cost > 0.0);
    assert_eq!(turns, 3);
}

#[test]
fn e2e_agent_error_result() {
    let events_json = [
        r#"{"type":"system","subtype":"init","session_id":"err-1"}"#,
        r#"{"type":"result","result":"API rate limit exceeded","total_cost_usd":0.0,"num_turns":0,"is_error":true}"#,
    ];

    let mut got_error = false;
    let mut error_msg = String::new();

    for json in events_json {
        let event: AgentEvent = serde_json::from_str(json).unwrap();
        if let AgentEvent::Result {
            result, is_error, ..
        } = event
        {
            if is_error {
                got_error = true;
                error_msg = result.unwrap_or_default();
            }
        }
    }

    assert!(got_error, "should detect error result");
    assert_eq!(error_msg, "API rate limit exceeded");
}

#[test]
fn e2e_agent_stop_reason_end_turn() {
    let json = r#"{"type":"stream_event","event":{"type":"message_delta","delta":{"type":"message_delta","stop_reason":"end_turn"}}}"#;
    let event: AgentEvent = serde_json::from_str(json).unwrap();
    match event {
        AgentEvent::StreamEvent { event } => {
            let delta = event.delta.unwrap();
            assert_eq!(delta.stop_reason.as_deref(), Some("end_turn"));
        }
        _ => panic!("expected StreamEvent"),
    }
}

#[test]
fn e2e_agent_event_stream_sequence() {
    // Simulate a realistic sequence of events from Claude CLI
    let events_json = [
        r#"{"type":"system","subtype":"init","session_id":"s1"}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" world"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hello world"}]}}"#,
        r#"{"type":"result","result":"Hello world","total_cost_usd":0.005,"num_turns":1,"is_error":false}"#,
    ];

    let mut text_chunks = Vec::new();
    let mut got_system = false;
    let mut got_assistant = false;
    let mut got_result = false;

    for json in events_json {
        let event: AgentEvent = serde_json::from_str(json).unwrap();
        match event {
            AgentEvent::System { .. } => got_system = true,
            AgentEvent::StreamEvent { event } => {
                if let Some(delta) = event.delta {
                    if let Some(text) = delta.text {
                        text_chunks.push(text);
                    }
                }
            }
            AgentEvent::Assistant { message } => {
                got_assistant = true;
                let content = message.content.unwrap();
                assert_eq!(content[0].text.as_deref(), Some("Hello world"));
            }
            AgentEvent::Result {
                total_cost_usd,
                num_turns,
                is_error,
                ..
            } => {
                got_result = true;
                assert!(!is_error);
                assert_eq!(num_turns, 1);
                assert!(total_cost_usd > 0.0);
            }
            _ => {}
        }
    }

    assert!(got_system, "should receive system event");
    assert!(got_assistant, "should receive assistant event");
    assert!(got_result, "should receive result event");
    assert_eq!(text_chunks.join(""), "Hello world");
}

#[test]
fn e2e_agent_tool_use_flow() {
    // Simulate tool use: content_block_start(tool) -> deltas(json) -> content_block_stop -> user(result)
    let events_json = [
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"t1","name":"Read"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"path\":"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"\"/src/main.rs\"}"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
        r#"{"type":"user","tool_use_result":{"stdout":"fn main() {}","stderr":"","is_error":false,"tool_use_id":"t1","content":"fn main() {}"},"message":{}}"#,
    ];

    let mut tool_name = String::new();
    let mut json_parts = Vec::new();
    let mut tool_result = String::new();

    for json in events_json {
        let event: AgentEvent = serde_json::from_str(json).unwrap();
        match event {
            AgentEvent::StreamEvent { event } => {
                if let Some(block) = event.content_block {
                    if let Some(name) = block.name {
                        tool_name = name;
                    }
                }
                if let Some(delta) = event.delta {
                    if let Some(pj) = delta.partial_json {
                        json_parts.push(pj);
                    }
                }
            }
            AgentEvent::User {
                tool_use_result, ..
            } => {
                if let Some(r) = tool_use_result {
                    tool_result = r.stdout;
                }
            }
            _ => {}
        }
    }

    assert_eq!(tool_name, "Read");
    let full_json = json_parts.join("");
    assert!(full_json.contains("/src/main.rs"));
    assert_eq!(tool_result, "fn main() {}");
}
