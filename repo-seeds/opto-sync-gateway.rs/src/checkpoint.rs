use serde::{Deserialize, Serialize};

use crate::Cursor;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointKey {
    pub tenant_id: String,
    pub principal_id: String,
    pub device_id: String,
    pub stream_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DurableCheckpoint {
    pub key: CheckpointKey,
    pub cursor: Cursor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointDecision {
    FirstCommit,
    Advance,
    Duplicate,
    RejectRegression,
    RejectTokenConflict,
}

pub fn decide_checkpoint(current: Option<&Cursor>, proposed: &Cursor) -> CheckpointDecision {
    let Some(current) = current else {
        return CheckpointDecision::FirstCommit;
    };

    match proposed.sequence.cmp(&current.sequence) {
        std::cmp::Ordering::Greater => CheckpointDecision::Advance,
        std::cmp::Ordering::Less => CheckpointDecision::RejectRegression,
        std::cmp::Ordering::Equal if proposed.token == current.token => {
            CheckpointDecision::Duplicate
        }
        std::cmp::Ordering::Equal => CheckpointDecision::RejectTokenConflict,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeDecision {
    Fresh,
    Resume,
    ResyncRequired,
    RejectAheadOfDurableCheckpoint,
    RejectTokenConflict,
}

pub fn decide_resume(
    requested: Option<&Cursor>,
    durable: Option<&Cursor>,
    retention_floor: u64,
) -> ResumeDecision {
    let Some(requested) = requested else {
        return ResumeDecision::Fresh;
    };
    if requested.sequence < retention_floor {
        return ResumeDecision::ResyncRequired;
    }

    let Some(durable) = durable else {
        return if requested.sequence == 0 {
            ResumeDecision::Resume
        } else {
            ResumeDecision::RejectAheadOfDurableCheckpoint
        };
    };

    match requested.sequence.cmp(&durable.sequence) {
        std::cmp::Ordering::Less => ResumeDecision::Resume,
        std::cmp::Ordering::Greater => ResumeDecision::RejectAheadOfDurableCheckpoint,
        std::cmp::Ordering::Equal if requested.token == durable.token => ResumeDecision::Resume,
        std::cmp::Ordering::Equal => ResumeDecision::RejectTokenConflict,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cursor(sequence: u64, token: &str) -> Cursor {
        Cursor {
            sequence,
            token: token.into(),
        }
    }

    #[test]
    fn checkpoint_is_monotonic_and_idempotent() {
        let current = cursor(10, "cursor_10");
        assert_eq!(
            decide_checkpoint(None, &current),
            CheckpointDecision::FirstCommit
        );
        assert_eq!(
            decide_checkpoint(Some(&current), &cursor(11, "cursor_11")),
            CheckpointDecision::Advance
        );
        assert_eq!(
            decide_checkpoint(Some(&current), &current),
            CheckpointDecision::Duplicate
        );
        assert_eq!(
            decide_checkpoint(Some(&current), &cursor(9, "cursor_9")),
            CheckpointDecision::RejectRegression
        );
        assert_eq!(
            decide_checkpoint(Some(&current), &cursor(10, "different")),
            CheckpointDecision::RejectTokenConflict
        );
    }

    #[test]
    fn resume_fails_closed_for_expired_or_impossible_cursors() {
        let durable = cursor(10, "cursor_10");
        assert_eq!(
            decide_resume(Some(&cursor(4, "cursor_4")), Some(&durable), 5),
            ResumeDecision::ResyncRequired
        );
        assert_eq!(
            decide_resume(Some(&cursor(11, "cursor_11")), Some(&durable), 5),
            ResumeDecision::RejectAheadOfDurableCheckpoint
        );
        assert_eq!(
            decide_resume(Some(&cursor(10, "other")), Some(&durable), 5),
            ResumeDecision::RejectTokenConflict
        );
    }

    #[test]
    fn resume_allows_fresh_and_known_durable_history() {
        let durable = cursor(10, "cursor_10");
        assert_eq!(
            decide_resume(None, Some(&durable), 5),
            ResumeDecision::Fresh
        );
        assert_eq!(
            decide_resume(Some(&cursor(8, "cursor_8")), Some(&durable), 5),
            ResumeDecision::Resume
        );
        assert_eq!(
            decide_resume(Some(&durable), Some(&durable), 5),
            ResumeDecision::Resume
        );
    }
}
