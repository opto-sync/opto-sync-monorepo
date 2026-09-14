use opto_sync_gateway::{ClientFrame, PROTOCOL_VERSION};
use serde_json::json;

#[test]
fn hello_rejects_unknown_authority_fields() {
    let value = json!({
        "type": "hello",
        "session_id": "session_1",
        "stream_id": "stream_1",
        "protocol_version": PROTOCOL_VERSION,
        "resume_cursor": null,
        "tenant_id": "payload_must_not_choose_tenant"
    });
    assert!(serde_json::from_value::<ClientFrame>(value).is_err());
}

#[test]
fn unknown_frame_type_is_rejected() {
    let value = json!({"type": "admin_override", "enabled": true});
    assert!(serde_json::from_value::<ClientFrame>(value).is_err());
}

#[test]
fn cursor_rejects_unknown_fields() {
    let value = json!({
        "type": "ack",
        "cursor": {"sequence": 12, "token": "cursor_12", "tenant_id": "spoof"}
    });
    assert!(serde_json::from_value::<ClientFrame>(value).is_err());
}

#[test]
fn mutation_rejects_extra_execution_material() {
    let value = json!({
        "type": "mutation",
        "mutation_id": "mutation_1",
        "envelope": {"entity_id": "entity_1"},
        "command": "rm -rf /"
    });
    assert!(serde_json::from_value::<ClientFrame>(value).is_err());
}

#[test]
fn protocol_version_must_be_numeric() {
    let value = json!({
        "type": "hello",
        "session_id": "session_1",
        "stream_id": "stream_1",
        "protocol_version": "1",
        "resume_cursor": null
    });
    assert!(serde_json::from_value::<ClientFrame>(value).is_err());
}
