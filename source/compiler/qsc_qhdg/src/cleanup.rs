// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case", tag = "state", content = "evidence")]
pub enum CleanupState {
    CleanZ,
    PendingRestore {
        delta: u64,
        history: Vec<String>,
    },
    PendingFragment {
        delta: u64,
        fragment_count: usize,
        history: Vec<String>,
    },
    Restored {
        delta: u64,
    },
    LiveUnknown,
    Escaped,
    Released,
}

impl CleanupState {
    #[must_use]
    pub const fn is_live(&self) -> bool {
        !matches!(self, Self::Released)
    }

    #[must_use]
    pub fn split(&self, fragment_count: usize) -> Self {
        match self {
            Self::PendingRestore { delta, history } => Self::PendingFragment {
                delta: *delta,
                fragment_count,
                history: history.clone(),
            },
            state => state.clone(),
        }
    }

    #[must_use]
    pub fn join_fragments<'a>(
        states: impl Iterator<Item = &'a Self>,
        expected_count: usize,
    ) -> Self {
        let states: Vec<_> = states.collect();
        let Some(Self::PendingFragment {
            delta,
            fragment_count,
            ..
        }) = states.first()
        else {
            return join_cleanup(states.into_iter());
        };
        if states.len() == expected_count
            && *fragment_count == expected_count
            && states.iter().all(|state| {
                matches!(state, Self::PendingFragment { delta: candidate, fragment_count: count, .. }
                    if candidate == delta && count == fragment_count)
            })
        {
            let history = states
                .iter()
                .filter_map(|state| match state {
                    Self::PendingFragment { history, .. } => Some(history.iter().cloned()),
                    _ => None,
                })
                .flatten()
                .collect();
            Self::PendingRestore {
                delta: *delta,
                history,
            }
        } else {
            Self::LiveUnknown
        }
    }

    #[must_use]
    pub fn quantum_transition(&self, operation: impl Into<String>) -> Self {
        let operation = operation.into();
        match self {
            Self::PendingRestore { delta, history } => {
                let mut history = history.clone();
                history.push(operation);
                Self::PendingRestore {
                    delta: *delta,
                    history,
                }
            }
            Self::PendingFragment {
                delta,
                fragment_count,
                history,
            } => {
                let mut history = history.clone();
                history.push(operation);
                Self::PendingFragment {
                    delta: *delta,
                    fragment_count: *fragment_count,
                    history,
                }
            }
            Self::CleanZ | Self::Restored { .. } | Self::LiveUnknown => Self::LiveUnknown,
            Self::Escaped => Self::Escaped,
            Self::Released => Self::Released,
        }
    }

    #[must_use]
    pub fn measure(&self, standard_basis_zero: bool) -> Self {
        match self {
            Self::CleanZ if standard_basis_zero => Self::CleanZ,
            Self::Escaped => Self::Escaped,
            Self::Released => Self::Released,
            _ => Self::LiveUnknown,
        }
    }

    #[must_use]
    pub fn reset(&self, fresh_origin: bool) -> Self {
        if fresh_origin {
            Self::CleanZ
        } else {
            Self::LiveUnknown
        }
    }

    pub fn prove_restored(self) -> Result<Self, CleanupError> {
        let _ = self;
        Err(CleanupError::DirtyRestorationDisabled)
    }

    pub fn release(self) -> Result<Self, CleanupError> {
        match self {
            Self::CleanZ => Err(CleanupError::FailureCleanupDisabled),
            Self::Restored { .. } => Err(CleanupError::DirtyRestorationDisabled),
            _ => Err(CleanupError::NotReleasable),
        }
    }
}

#[must_use]
pub fn join_cleanup<'a>(mut states: impl Iterator<Item = &'a CleanupState>) -> CleanupState {
    let Some(first) = states.next() else {
        return CleanupState::LiveUnknown;
    };
    let remaining: Vec<_> = states.collect();
    if remaining.iter().all(|state| *state == first) {
        first.clone()
    } else if matches!(first, CleanupState::Released | CleanupState::Escaped)
        || remaining
            .iter()
            .any(|state| matches!(state, CleanupState::Released | CleanupState::Escaped))
    {
        CleanupState::Escaped
    } else {
        CleanupState::LiveUnknown
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExitKind {
    Branch,
    Theta,
    Return,
    Handler,
    Failure,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CleanupJoin {
    pub kind: ExitKind,
    pub state: CleanupState,
}

impl CleanupJoin {
    pub fn checked(kind: ExitKind, exits: &[CleanupState]) -> Result<Self, CleanupError> {
        if kind == ExitKind::Failure && exits.iter().any(CleanupState::is_live) {
            return Err(CleanupError::FailureCleanupDisabled);
        }
        if exits
            .iter()
            .any(|state| matches!(state, CleanupState::Released))
            && exits.iter().any(CleanupState::is_live)
        {
            return Err(CleanupError::JoinMismatch);
        }
        Ok(Self {
            kind,
            state: join_cleanup(exits.iter()),
        })
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum CleanupError {
    #[error("QHDG-OWNERSHIP-DIRTY-RESTORE-001")]
    DirtyRestorationDisabled,
    #[error("QHDG-EXIT-CLEANUP-POLICY-001")]
    FailureCleanupDisabled,
    #[error("cleanup obligation is not releasable")]
    NotReleasable,
    #[error("ownership exits disagree between live and released states")]
    JoinMismatch,
}
