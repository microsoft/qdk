// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`HeldShots`]: the shots the part under test holds, which its wrapper
//! notes to tell an end of shots not held from one of shots it holds.

use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use executor_protocol::{Batch, ShotId, ShotSet};
use rustc_hash::FxHashSet;

use super::panic_message;
use crate::nonconformance::{Fault, Place, shot_list};
use crate::shared_run::SharedRun;

/// The shots the part under test holds, as its wrapper sees them: each
/// shot of a batch it got in an `execute` and that was not ended since.
#[derive(Default)]
pub(crate) struct HeldShots(FxHashSet<ShotId>);

impl HeldShots {
    /// Notes the shots of `batches`, which the part holds from now on.
    pub(crate) fn hold(&mut self, batches: &[Batch<'_>]) {
        self.0
            .extend(batches.iter().flat_map(|batch| batch.shots.iter()));
    }

    /// Ends `shots` on the part, by `end`, unless the setup withholds it.
    /// If `end` panics while some of `shots` are not held, logs it as a
    /// fault of [`Rule::IgnoresEndsOfShotsItDoesNotHold`](crate::Rule)
    /// and goes on; the shots the panic left unended may show as faults of
    /// other rules. If every shot was held, the panic unwinds on, as the
    /// protocol handles it.
    pub(crate) fn end(&mut self, run: &SharedRun, shots: &ShotSet, end: impl FnOnce()) {
        if run.setup().withholds_end_shots {
            return;
        }
        let unheld: Vec<String> = shots
            .iter()
            .filter(|shot| !self.0.remove(shot))
            .map(|shot| shot.to_string())
            .collect();
        let Err(panic) = catch_unwind(AssertUnwindSafe(end)) else {
            return;
        };
        if unheld.is_empty() {
            resume_unwind(panic);
        }
        let ended: Vec<String> = shots.iter().map(|shot| shot.to_string()).collect();
        let fault = Fault {
            place: Place::EndShots {
                after: run.latest_execute(),
            },
            found: format!(
                "panicked ending {}, among which {} it did not hold, or no longer held: {}",
                shot_list(&ended),
                shot_list(&unheld),
                panic_message(&*panic)
            ),
        };
        run.write(|log| log.end_faults.push(fault));
    }
}
