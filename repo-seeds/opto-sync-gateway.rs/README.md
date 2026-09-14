# opto-sync-gateway.rs repository seed

Initial repository seed for the realtime replication/protocol plane tracked by `opto-sync-monorepo#6` and `opto-sync-interfaces#27`.

`syncer.rs` remains the deterministic I/O-free merge and causal-ordering engine. This gateway owns WebSocket lifecycle, authenticated connection scope, resume cursors, protocol negotiation, connection caps and backpressure boundaries.

## Implemented now

- `/v1/sync` authenticated WebSocket endpoint plus `/healthz` and `/readyz`.
- Canonical lowercase `x-ores-*` internal/scope headers; payloads cannot select tenant/principal/device authority.
- Non-loopback bind fails closed without an internal auth secret.
- Constant-time internal-auth comparison.
- Bounded connection count and WebSocket frame/message size.
- Required protocol-v1 `hello` frame with strict unknown-field rejection.
- Resume cursor retention-floor check with typed `resync_required` response.
- Heartbeat round-trip and strict text/JSON protocol.
- Mutation identifiers and cursor tokens are bounded/canonical.
- **No false delivery guarantee:** mutations receive retryable `persistence_not_wired`; no mutation ACK is emitted until a durable outbox/checkpoint path exists.

## Delivery semantics target

The completed gateway will provide at-least-once transport with idempotent mutation IDs and durable ACK/checkpoint state. It must never claim network-wide exactly-once semantics. Durable reconciliation will consume `syncer.rs`; it must not fork that algorithm into the gateway.

## Configuration

Secrets are environment-only. The seed accepts no CLI arguments.

- `OPTO_SYNC_BIND` (default `127.0.0.1:8091`)
- `OPTO_SYNC_INTERNAL_AUTH` (required for non-loopback bind)
- `OPTO_SYNC_RETENTION_FLOOR` (default 0)
- `OPTO_SYNC_MAX_SESSIONS` (default 1024)
- `OPTO_SYNC_MAX_MESSAGE_BYTES` (default 1 MiB, hard ceiling 4 MiB)

Next slices: durable cursor/ACK store, canonical outbox, `syncer.rs` reconciliation adapter, server-push subscriptions, replay/idempotency corpus and cross-language E2E clients.
