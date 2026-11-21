// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The shots of a run whose `drive` started, which the parts below the
//! driver may hold: [`UnendedShots`], which ends them once the run ends,
//! whether it gave outputs, failed, or its future was dropped. The runner
//! ends them so on the executor, and a joined driver on its adapters, so
//! that freeing a run's shots never depends on the driver.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use crate::ShotSet;

/// The shots of a run, unended on `part`, the parts below a driver, which
/// lends `part` to the driver's `drive`; [`end`](Self::end) ends them, and
/// so does a drop, such as that of the run's future, if nothing ended them
/// before.
pub(crate) struct UnendedShots<'a, P, F: FnOnce(&mut P, &ShotSet)> {
    /// The parts that may hold the shots.
    pub(crate) part: P,
    shots: &'a ShotSet,
    /// How `part`'s shots end, until they do.
    end: Option<F>,
}

impl<'a, P, F: FnOnce(&mut P, &ShotSet)> UnendedShots<'a, P, F> {
    /// `shots`, unended on `part` until `end` ends them.
    pub(crate) fn new(part: P, shots: &'a ShotSet, end: F) -> Self {
        Self {
            part,
            shots,
            end: Some(end),
        }
    }

    /// Ends the shots, in a catch: the payload of a panic ending them, for
    /// the caller to raise once it has ended every other part.
    /// `AssertUnwindSafe`, as for the protocol's other catches of a part's
    /// panic (`catch_panic`).
    pub(crate) fn end(mut self) -> Result<(), Box<dyn Any + Send>> {
        self.end_caught().unwrap_or(Ok(()))
    }

    /// Ends the shots, in a catch, unless they are ended already.
    fn end_caught(&mut self) -> Option<Result<(), Box<dyn Any + Send>>> {
        let end = self.end.take()?;
        let (part, shots) = (&mut self.part, self.shots);
        Some(catch_unwind(AssertUnwindSafe(|| end(part, shots))))
    }
}

/// Ends the shots if nothing ended them, and raises a panic ending them
/// again, unless the thread is already unwinding, such as when a caller's
/// panic drops the run's future: a second panic would abort the process,
/// so its payload is then dropped.
impl<P, F: FnOnce(&mut P, &ShotSet)> Drop for UnendedShots<'_, P, F> {
    fn drop(&mut self) {
        if let Some(Err(payload)) = self.end_caught()
            && !std::thread::panicking()
        {
            resume_unwind(payload);
        }
    }
}
