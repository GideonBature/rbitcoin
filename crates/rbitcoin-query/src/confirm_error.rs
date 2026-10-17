//! Confirm pipeline error. Rule rejects stay [`ConsensusError`]. Disk, io_uring,
//! and cancellation stay [`StoreError`] and must not be cached as a failed block.

use rbitcoin_consensus::ConsensusError;
use rbitcoin_store::StoreError;
use std::fmt;

#[derive(Debug)]
pub enum ConfirmError {
    Rules(ConsensusError),
    Store(StoreError),
}

impl fmt::Display for ConfirmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfirmError::Rules(e) => write!(f, "{e}"),
            ConfirmError::Store(e) => write!(f, "store: {e}"),
        }
    }
}

impl std::error::Error for ConfirmError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfirmError::Rules(e) => Some(e),
            ConfirmError::Store(e) => Some(e),
        }
    }
}

impl From<ConsensusError> for ConfirmError {
    fn from(e: ConsensusError) -> Self {
        ConfirmError::Rules(e)
    }
}

impl From<StoreError> for ConfirmError {
    fn from(e: StoreError) -> Self {
        // A missing parent on an untrusted block is not store corruption.
        // Cancellation is a rule-side abort so it is not cached as a bad block.
        match e {
            StoreError::Cancelled(_) => ConfirmError::Rules(ConsensusError::Cancelled),
            StoreError::Corrupt(m) if m.contains("parent create_fk unresolved") => {
                ConfirmError::from(rbitcoin_consensus::block::unresolved_parent_create())
            }
            other => ConfirmError::Store(other),
        }
    }
}

impl ConfirmError {
    pub fn is_uring_session_fault(&self) -> bool {
        matches!(self, ConfirmError::Store(se) if se.is_uring_session_fault())
    }

    pub fn rules(&self) -> Option<&ConsensusError> {
        match self {
            ConfirmError::Rules(e) => Some(e),
            ConfirmError::Store(_) => None,
        }
    }

    /// Script-pool jobs only return [`ConsensusError`]. A store fault becomes
    /// [`ConsensusError::Internal`] so the pool does not grow a second taxonomy.
    /// io_uring text stays recognizable to [`StoreError::is_uring_session_fault_msg`].
    pub(crate) fn into_pool(self) -> ConsensusError {
        match self {
            ConfirmError::Rules(e) => e,
            ConfirmError::Store(StoreError::Corrupt(m))
                if StoreError::is_uring_session_fault_msg(m) =>
            {
                ConsensusError::Internal("io_uring submit failed")
            }
            ConfirmError::Store(StoreError::Corrupt(m)) => ConsensusError::Internal(m),
            ConfirmError::Store(_) => ConsensusError::Internal("index assemble store fault"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uring_session_fault_covers_leftover_and_submit() {
        for m in [
            "invariant: io_uring leftover cqe",
            "io_uring submit failed",
            "invariant: io_uring undrained",
        ] {
            let e = ConfirmError::Store(StoreError::Corrupt(m));
            assert!(e.is_uring_session_fault(), "{m}");
        }
        assert!(!ConfirmError::Store(StoreError::NotFound).is_uring_session_fault());
        assert!(!ConfirmError::Rules(ConsensusError::PrevoutSpent).is_uring_session_fault());
    }

    #[test]
    fn archive_unresolved_parent_is_missing_prevout_not_corrupt() {
        let e = ConfirmError::from(StoreError::Corrupt(
            "archive: parent create_fk unresolved (contiguous batch required)",
        ));
        assert!(
            matches!(e, ConfirmError::Rules(ConsensusError::MissingPrevout)),
            "got {e:?}"
        );
        assert!(!e.to_string().contains("corrupt"));
    }

    #[test]
    fn from_store_error_keeps_not_found() {
        let e = ConfirmError::from(StoreError::NotFound);
        assert!(matches!(e, ConfirmError::Store(StoreError::NotFound)));
    }

    #[test]
    fn cancelled_is_not_a_store_fault_string() {
        let e = ConfirmError::from(StoreError::Cancelled("stop"));
        assert!(matches!(e, ConfirmError::Rules(ConsensusError::Cancelled)));
        assert_eq!(e.to_string(), "confirm cancelled");
    }
}
