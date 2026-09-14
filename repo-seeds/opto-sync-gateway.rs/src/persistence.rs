use std::{future::Future, pin::Pin};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::{checkpoint::CheckpointKey, Cursor};

pub type StoreFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, StoreError>> + Send + 'a>>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MutationRecord {
    pub key: CheckpointKey,
    pub mutation_id: String,
    pub envelope: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointWrite {
    Applied,
    Duplicate,
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationWrite {
    Inserted,
    Duplicate,
    Conflict,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum StoreError {
    #[error("durable store is unavailable")]
    Unavailable,
    #[error("durable store rejected invalid input")]
    InvalidInput,
    #[error("durable store operation timed out")]
    Timeout,
}

pub trait DurableStore: Send + Sync {
    fn load_checkpoint<'a>(&'a self, key: &'a CheckpointKey) -> StoreFuture<'a, Option<Cursor>>;

    fn compare_and_set_checkpoint<'a>(
        &'a self,
        key: &'a CheckpointKey,
        expected: Option<&'a Cursor>,
        next: &'a Cursor,
    ) -> StoreFuture<'a, CheckpointWrite>;

    fn append_mutation<'a>(
        &'a self,
        mutation: &'a MutationRecord,
    ) -> StoreFuture<'a, MutationWrite>;
}

#[derive(Debug, Default)]
pub struct UnavailableDurableStore;

impl DurableStore for UnavailableDurableStore {
    fn load_checkpoint<'a>(&'a self, _key: &'a CheckpointKey) -> StoreFuture<'a, Option<Cursor>> {
        Box::pin(async { Err(StoreError::Unavailable) })
    }

    fn compare_and_set_checkpoint<'a>(
        &'a self,
        _key: &'a CheckpointKey,
        _expected: Option<&'a Cursor>,
        _next: &'a Cursor,
    ) -> StoreFuture<'a, CheckpointWrite> {
        Box::pin(async { Err(StoreError::Unavailable) })
    }

    fn append_mutation<'a>(
        &'a self,
        _mutation: &'a MutationRecord,
    ) -> StoreFuture<'a, MutationWrite> {
        Box::pin(async { Err(StoreError::Unavailable) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> CheckpointKey {
        CheckpointKey {
            tenant_id: "tenant_1".into(),
            principal_id: "user_1".into(),
            device_id: "device_1".into(),
            stream_id: "stream_1".into(),
        }
    }

    #[tokio::test]
    async fn unavailable_store_fails_closed_for_every_operation() {
        let store = UnavailableDurableStore;
        let cursor = Cursor {
            sequence: 1,
            token: "cursor_1".into(),
        };
        let mutation = MutationRecord {
            key: key(),
            mutation_id: "mutation_1".into(),
            envelope: serde_json::json!({"value": 1}),
        };

        assert_eq!(
            store.load_checkpoint(&key()).await,
            Err(StoreError::Unavailable)
        );
        assert_eq!(
            store
                .compare_and_set_checkpoint(&key(), None, &cursor)
                .await,
            Err(StoreError::Unavailable)
        );
        assert_eq!(
            store.append_mutation(&mutation).await,
            Err(StoreError::Unavailable)
        );
    }

    #[test]
    fn mutation_record_rejects_unknown_authority_fields() {
        let raw = serde_json::json!({
            "key": {
                "tenant_id": "tenant_1",
                "principal_id": "user_1",
                "device_id": "device_1",
                "stream_id": "stream_1"
            },
            "mutation_id": "mutation_1",
            "envelope": {"value": 1},
            "bearer_token": "secret"
        });
        assert!(serde_json::from_value::<MutationRecord>(raw).is_err());
    }
}
