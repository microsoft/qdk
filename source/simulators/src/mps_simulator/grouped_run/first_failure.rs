// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The first failure of a run: a run names its first failing shot in the
//! order of its shots, as `QirResumer::run` requires.
//!
//! [`GroupedRun`] runs its groups in the order of its stack, not of the
//! shots, so a later shot may fail first. [`FirstFailure`] keeps the
//! failure of the earliest shot so far, and tells the run which groups it
//! may drop: those whose shots all come after that shot.

use crate::qir::{QirResumeError, QirShotId, QirTrap};

#[cfg(doc)]
use super::GroupedRun;
use crate::mps_simulator::mps_resumer_error::MpsResumerError;

/// The failure of a run's first failing shot so far, in the order of the
/// run's shots; none until a shot fails.
pub(super) struct FirstFailure<E>(Option<ShotFailure<E>>);

impl<E> FirstFailure<E> {
    /// A run's first failure before any shot failed.
    pub(super) fn none() -> Self {
        Self(None)
    }

    /// Keeps `failure` as the run's when the shot at `place` comes before
    /// every shot that failed so far.
    pub(super) fn keep(&mut self, place: usize, failure: GroupFailure<E>) {
        if self.0.as_ref().is_none_or(|first| place < first.place) {
            self.0 = Some(ShotFailure { place, failure });
        }
    }

    /// Whether a shot before the shot at `place` failed. The run's result
    /// is then that failure, whatever the shot at `place` does.
    pub(super) fn comes_before(&self, place: usize) -> bool {
        self.0.as_ref().is_some_and(|first| first.place < place)
    }

    /// The run's error, which names its failing shot among `shots`; `None`
    /// when no shot failed.
    pub(super) fn into_error(
        self,
        shots: &[QirShotId],
    ) -> Option<QirResumeError<MpsResumerError<E>>> {
        self.0
            .map(|ShotFailure { place, failure }| failure.into_resume_error(shots[place]))
    }
}

/// The first failing shot of a run: its place in the order of the run's
/// shots, and why it failed.
struct ShotFailure<E> {
    /// The failing shot's index in the run's shots.
    place: usize,
    /// Why it failed.
    failure: GroupFailure<E>,
}

/// Why a group failed: its context trapped, or the resumer failed it.
#[derive(Debug)]
pub(super) enum GroupFailure<E> {
    /// The machine trapped the group's context.
    Trapped(QirTrap),
    /// The resumer failed the group, for one of the causes
    /// [`MpsResumerError`] lists.
    Resumer(MpsResumerError<E>),
}

impl<E> GroupFailure<E> {
    /// The run's error for this failure of `shot`, which names it.
    fn into_resume_error(self, shot: QirShotId) -> QirResumeError<MpsResumerError<E>> {
        match self {
            Self::Trapped(trap) => QirResumeError::Trapped { shot, trap },
            Self::Resumer(error) => QirResumeError::Resumer { shot, error },
        }
    }
}

impl<E> From<MpsResumerError<E>> for GroupFailure<E> {
    fn from(error: MpsResumerError<E>) -> Self {
        Self::Resumer(error)
    }
}

/// The group's failure for an engine's `error`. Every engine call fails
/// its group alike, so each maps its error through this.
pub(super) fn engine_failure<E>(error: E) -> GroupFailure<E> {
    GroupFailure::Resumer(MpsResumerError::Engine(error))
}

#[cfg(test)]
mod tests;
