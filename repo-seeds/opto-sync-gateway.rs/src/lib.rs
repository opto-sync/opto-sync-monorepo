use std::{env, net::SocketAddr, sync::Arc};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use subtle::ConstantTimeEq;
use thiserror::Error;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub const PROTOCOL_VERSION: u16 = 1;
pub const DEFAULT_MAX_MESSAGE_BYTES: usize = 1024 * 1024;
pub const HARD_MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;
pub const DEFAULT_MAX_SESSIONS: usize = 1024;

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub internal_auth: Option<String>,
    pub retention_floor: u64,
    pub max_sessions: usize,
    pub max_message_bytes: usize,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("invalid bind address")]
    InvalidBind,
    #[error("non-loopback bind requires OPTO_SYNC_INTERNAL_AUTH")]
    MissingAuth,
    #[error("invalid numeric configuration: {0}")]
    InvalidNumber(&'static str),
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let bind: SocketAddr = env::var("OPTO_SYNC_BIND")
            .unwrap_or_else(|_| "127.0.0.1:8091".into())
            .parse()
            .map_err(|_| ConfigError::InvalidBind)?;
        let internal_auth = env::var("OPTO_SYNC_INTERNAL_AUTH")
            .ok()
            .filter(|v| !v.is_empty());
        if !bind.ip().is_loopback() && internal_auth.is_none() {
            return Err(ConfigError::MissingAuth);
        }
        Ok(Self {
            bind,
            internal_auth,
            retention_floor: parse_env("OPTO_SYNC_RETENTION_FLOOR", 0, 0, u64::MAX)?,
            max_sessions: parse_env(
                "OPTO_SYNC_MAX_SESSIONS",
                DEFAULT_MAX_SESSIONS as u64,
                1,
                65_536,
            )? as usize,
            max_message_bytes: parse_env(
                "OPTO_SYNC_MAX_MESSAGE_BYTES",
                DEFAULT_MAX_MESSAGE_BYTES as u64,
                1,
                HARD_MAX_MESSAGE_BYTES as u64,
            )? as usize,
        })
    }
}

fn parse_env(name: &'static str, default: u64, min: u64, max: u64) -> Result<u64, ConfigError> {
    let Some(raw) = env::var(name).ok() else {
        return Ok(default);
    };
    let value = raw
        .parse::<u64>()
        .map_err(|_| ConfigError::InvalidNumber(name))?;
    if !(min..=max).contains(&value) {
        return Err(ConfigError::InvalidNumber(name));
    }
    Ok(value)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthContext {
    pub tenant_id: String,
    pub principal_id: String,
    pub device_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    pub sequence: u64,
    pub token: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientFrame {
    Hello {
        session_id: String,
        stream_id: String,
        protocol_version: u16,
        resume_cursor: Option<Cursor>,
    },
    Mutation {
        mutation_id: String,
        envelope: Value,
    },
    Ack {
        cursor: Cursor,
    },
    Heartbeat {
        nonce: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerFrame {
    HelloAck {
        session_id: String,
        stream_id: String,
        tenant_id: String,
        principal_id: String,
        device_id: String,
        accepted_cursor: Option<Cursor>,
    },
    ResyncRequired {
        reason: &'static str,
        retention_floor: u64,
    },
    HeartbeatAck {
        nonce: String,
    },
    Error {
        code: &'static str,
        retryable: bool,
    },
}

#[derive(Debug, Clone, Copy)]
struct RequestError {
    status: StatusCode,
    code: &'static str,
}

impl RequestError {
    const fn new(status: StatusCode, code: &'static str) -> Self {
        Self { status, code }
    }

    fn into_response(self) -> Response {
        error(self.status, self.code)
    }
}

#[derive(Clone)]
struct GatewayState {
    config: Config,
    sessions: Arc<Semaphore>,
}

pub fn router(config: Config) -> Router {
    let state = GatewayState {
        sessions: Arc::new(Semaphore::new(config.max_sessions)),
        config,
    };
    Router::new()
        .route("/healthz", get(|| async { StatusCode::OK }))
        .route("/readyz", get(|| async { StatusCode::OK }))
        .route("/v1/sync", get(websocket))
        .with_state(state)
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: &'static str,
}

async fn websocket(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let auth = match authorize(&state.config, &headers) {
        Ok(auth) => auth,
        Err(request_error) => return request_error.into_response(),
    };
    let permit = match state.sessions.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => return error(StatusCode::TOO_MANY_REQUESTS, "gateway_overloaded"),
    };
    let max_message_bytes = state.config.max_message_bytes;
    ws.max_message_size(max_message_bytes)
        .max_frame_size(max_message_bytes)
        .on_upgrade(move |socket| run_session(socket, auth, state, permit))
}

async fn run_session(
    mut socket: WebSocket,
    auth: AuthContext,
    state: GatewayState,
    _permit: OwnedSemaphorePermit,
) {
    let Some(Ok(Message::Text(text))) = socket.next().await else {
        return;
    };
    let hello = match serde_json::from_str::<ClientFrame>(text.as_str()) {
        Ok(frame) => frame,
        Err(_) => {
            let _ = send(
                &mut socket,
                ServerFrame::Error {
                    code: "invalid_frame",
                    retryable: false,
                },
            )
            .await;
            return;
        }
    };

    let (session_id, stream_id, resume_cursor) = match hello {
        ClientFrame::Hello {
            session_id,
            stream_id,
            protocol_version,
            resume_cursor,
        } => {
            if protocol_version != PROTOCOL_VERSION
                || !valid_id(&session_id)
                || !valid_id(&stream_id)
                || resume_cursor
                    .as_ref()
                    .is_some_and(|cursor| !valid_cursor(cursor))
            {
                let _ = send(
                    &mut socket,
                    ServerFrame::Error {
                        code: "invalid_hello",
                        retryable: false,
                    },
                )
                .await;
                return;
            }
            (session_id, stream_id, resume_cursor)
        }
        _ => {
            let _ = send(
                &mut socket,
                ServerFrame::Error {
                    code: "hello_required",
                    retryable: false,
                },
            )
            .await;
            return;
        }
    };

    if resume_cursor
        .as_ref()
        .is_some_and(|cursor| cursor.sequence < state.config.retention_floor)
    {
        let _ = send(
            &mut socket,
            ServerFrame::ResyncRequired {
                reason: "cursor_expired",
                retention_floor: state.config.retention_floor,
            },
        )
        .await;
        return;
    }

    if send(
        &mut socket,
        ServerFrame::HelloAck {
            session_id,
            stream_id,
            tenant_id: auth.tenant_id,
            principal_id: auth.principal_id,
            device_id: auth.device_id,
            accepted_cursor: resume_cursor,
        },
    )
    .await
    .is_err()
    {
        return;
    }

    while let Some(message) = socket.next().await {
        match message {
            Ok(Message::Text(text)) => {
                let frame = match serde_json::from_str::<ClientFrame>(text.as_str()) {
                    Ok(frame) => frame,
                    Err(_) => {
                        let _ = send(
                            &mut socket,
                            ServerFrame::Error {
                                code: "invalid_frame",
                                retryable: false,
                            },
                        )
                        .await;
                        continue;
                    }
                };
                match frame {
                    ClientFrame::Heartbeat { nonce } if valid_id(&nonce) => {
                        if send(&mut socket, ServerFrame::HeartbeatAck { nonce })
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    ClientFrame::Mutation {
                        mutation_id,
                        envelope,
                    } => {
                        if !valid_id(&mutation_id) || !envelope.is_object() {
                            let _ = send(
                                &mut socket,
                                ServerFrame::Error {
                                    code: "invalid_mutation",
                                    retryable: false,
                                },
                            )
                            .await;
                        } else {
                            // Never ACK before durable outbox/checkpoint wiring exists.
                            let _ = send(
                                &mut socket,
                                ServerFrame::Error {
                                    code: "persistence_not_wired",
                                    retryable: true,
                                },
                            )
                            .await;
                        }
                    }
                    ClientFrame::Ack { cursor } => {
                        if !valid_cursor(&cursor) {
                            let _ = send(
                                &mut socket,
                                ServerFrame::Error {
                                    code: "invalid_ack",
                                    retryable: false,
                                },
                            )
                            .await;
                        } else {
                            let _ = send(
                                &mut socket,
                                ServerFrame::Error {
                                    code: "server_push_not_wired",
                                    retryable: true,
                                },
                            )
                            .await;
                        }
                    }
                    ClientFrame::Hello { .. } => {
                        let _ = send(
                            &mut socket,
                            ServerFrame::Error {
                                code: "duplicate_hello",
                                retryable: false,
                            },
                        )
                        .await;
                    }
                    ClientFrame::Heartbeat { .. } => {
                        let _ = send(
                            &mut socket,
                            ServerFrame::Error {
                                code: "invalid_heartbeat",
                                retryable: false,
                            },
                        )
                        .await;
                    }
                }
            }
            Ok(Message::Ping(bytes)) => {
                if socket.send(Message::Pong(bytes)).await.is_err() {
                    break;
                }
            }
            Ok(Message::Pong(_)) => {}
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(Message::Binary(_)) => {
                let _ = send(
                    &mut socket,
                    ServerFrame::Error {
                        code: "text_json_required",
                        retryable: false,
                    },
                )
                .await;
                break;
            }
        }
    }
}

async fn send(socket: &mut WebSocket, frame: ServerFrame) -> Result<(), axum::Error> {
    let json = serde_json::to_string(&frame).map_err(axum::Error::new)?;
    socket.send(Message::Text(json.into())).await
}

fn authorize(config: &Config, headers: &HeaderMap) -> Result<AuthContext, RequestError> {
    if let Some(expected) = config.internal_auth.as_deref() {
        let Some(actual) = headers
            .get("x-ores-internal-auth")
            .and_then(|v| v.to_str().ok())
        else {
            return Err(RequestError::new(
                StatusCode::UNAUTHORIZED,
                "missing_internal_auth",
            ));
        };
        if expected.as_bytes().ct_eq(actual.as_bytes()).unwrap_u8() != 1 {
            return Err(RequestError::new(
                StatusCode::UNAUTHORIZED,
                "invalid_internal_auth",
            ));
        }
    }
    Ok(AuthContext {
        tenant_id: required_identity(headers, "x-ores-tenant-id")?,
        principal_id: required_identity(headers, "x-ores-principal-id")?,
        device_id: required_identity(headers, "x-ores-device-id")?,
    })
}

fn required_identity(headers: &HeaderMap, name: &'static str) -> Result<String, RequestError> {
    let value = headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| RequestError::new(StatusCode::BAD_REQUEST, "missing_scope_header"))?;
    if !valid_id(value) {
        return Err(RequestError::new(
            StatusCode::BAD_REQUEST,
            "invalid_scope_header",
        ));
    }
    Ok(value.to_owned())
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

fn valid_cursor(cursor: &Cursor) -> bool {
    valid_id(&cursor.token)
}

fn error(status: StatusCode, code: &'static str) -> Response {
    (status, Json(ErrorBody { error: code })).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn config(secret: Option<&str>) -> Config {
        Config {
            bind: "127.0.0.1:8091".parse().unwrap(),
            internal_auth: secret.map(str::to_owned),
            retention_floor: 10,
            max_sessions: 4,
            max_message_bytes: DEFAULT_MAX_MESSAGE_BYTES,
        }
    }

    #[test]
    fn authenticated_headers_are_the_authority_scope() {
        let mut headers = HeaderMap::new();
        headers.insert("x-ores-internal-auth", HeaderValue::from_static("secret"));
        headers.insert("x-ores-tenant-id", HeaderValue::from_static("tenant_1"));
        headers.insert("x-ores-principal-id", HeaderValue::from_static("user_1"));
        headers.insert("x-ores-device-id", HeaderValue::from_static("device_1"));
        let auth = authorize(&config(Some("secret")), &headers).unwrap();
        assert_eq!(auth.tenant_id, "tenant_1");
        assert_eq!(auth.principal_id, "user_1");
        assert_eq!(auth.device_id, "device_1");
    }

    #[test]
    fn bad_auth_or_identity_fails_closed() {
        let mut headers = HeaderMap::new();
        headers.insert("x-ores-internal-auth", HeaderValue::from_static("wrong"));
        headers.insert("x-ores-tenant-id", HeaderValue::from_static("tenant_1"));
        headers.insert("x-ores-principal-id", HeaderValue::from_static("user_1"));
        headers.insert("x-ores-device-id", HeaderValue::from_static("device_1"));
        assert!(authorize(&config(Some("secret")), &headers).is_err());
        headers.insert("x-ores-internal-auth", HeaderValue::from_static("secret"));
        headers.insert("x-ores-device-id", HeaderValue::from_static("bad device"));
        assert!(authorize(&config(Some("secret")), &headers).is_err());
    }

    #[test]
    fn hello_cannot_smuggle_authority_or_credentials() {
        let raw = r#"{"type":"hello","session_id":"s1","stream_id":"st1","protocol_version":1,"resume_cursor":null,"tenant_id":"other","bearer_token":"secret"}"#;
        assert!(serde_json::from_str::<ClientFrame>(raw).is_err());
    }

    #[test]
    fn cursor_and_mutation_identifiers_are_bounded() {
        assert!(valid_cursor(&Cursor {
            sequence: 10,
            token: "cursor_10".into()
        }));
        assert!(!valid_cursor(&Cursor {
            sequence: 10,
            token: " ".into()
        }));
        assert!(valid_id("mutation:42"));
        assert!(!valid_id("mutation 42"));
        assert!(!valid_id(&"x".repeat(129)));
    }
}
