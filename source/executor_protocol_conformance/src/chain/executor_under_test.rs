// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`ExecutorUnderTest`]: the wrapper of an executor under test, which
//! sees what crosses the executor; and [`ExecutorMount`], which builds the
//! kit's chain around a new executor.

use executor_protocol::{
    Batch, BatchReadouts, Execute, ExecuteError, Executor, ExecutorSetup, InstructionSet, Needs,
    NotReady, Offer, Runner, Seed, ShotSet,
};

use super::{
    HeldShots, Mount, ScriptedDriver, compose, driver_for, log_rows, name_part, note_offer,
};
use crate::nonconformance::{Fault, Place};
use crate::readout_prefills::prefilled;
use crate::shared_run::SharedRun;

/// The executor under test, wrapped: it forwards each call, and around
/// each `execute`, prefills the readouts in a prefill run, may first drop a
/// call unpolled, and logs the rows the executor answered. It logs, too,
/// each time `accepts` or `offers` reads otherwise than before `>>`, once
/// the executor got ready and at each `execute`, and each `end_shots` that
/// panics on shots the executor did not hold, and keeps every `end_shots`
/// from the executor when the setup says.
pub(crate) struct ExecutorUnderTest<E> {
    part: E,
    run: SharedRun,
    /// `accepts`, as read before `>>`, written out: an instruction set has
    /// no `PartialEq`.
    accepts: String,
    /// `offers`, as read before `>>`.
    offers: Offer,
    held: HeldShots,
}

impl<E: Execute> ExecutorUnderTest<E> {
    /// `part`, wrapped, logging to `run`.
    pub(crate) fn new(part: E, run: SharedRun) -> Self {
        Self {
            accepts: format!("{:?}", part.accepts()),
            offers: part.offers(),
            part,
            run,
            held: HeldShots::default(),
        }
    }

    /// Logs a fault at `place` for each of `accepts` and `offers` that
    /// reads otherwise than before `>>`.
    fn check_accepts_and_offers(&self, place: &Place) {
        let mut faults = Vec::new();
        if format!("{:?}", self.part.accepts()) != self.accepts {
            faults.push(Fault {
                place: place.clone(),
                found: "`accepts` gives another set than before `>>`".to_owned(),
            });
        }
        let offers = self.part.offers();
        if offers != self.offers {
            faults.push(Fault {
                place: place.clone(),
                found: format!(
                    "`offers` gives {offers:?}, and gave {:?} before `>>`",
                    self.offers
                ),
            });
        }
        self.run
            .write(|log| log.accepts_and_offers_faults.extend(faults));
    }
}

impl<E: Execute> ExecutorSetup for ExecutorUnderTest<E> {
    fn name(&self) -> &str {
        self.part.name()
    }

    fn accepts(&self) -> &InstructionSet {
        self.part.accepts()
    }

    fn offers(&self) -> Offer {
        self.part.offers()
    }
}

impl<E: Execute> Execute for ExecutorUnderTest<E> {
    async fn ready(&mut self, needs: &Needs) -> Result<(), NotReady> {
        self.part.ready(needs).await?;
        self.check_accepts_and_offers(&Place::Ready);
        Ok(())
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        let execute = self.run.begin_execute();
        self.check_accepts_and_offers(&Place::Execute { execute });
        let prefills = prefilled(&self.run, execute, readouts);
        self.held.hold(batches);
        if self.run.setup().drops_a_call_unpolled {
            // On a copy, so that a part that reuses the dropped call's
            // work instead of doing the polled one's shows.
            let mut scratch = readouts.to_vec();
            drop(self.part.execute(batches, seed, &mut scratch));
        }
        self.part.execute(batches, seed, readouts).await?;
        log_rows(&self.run, execute, batches, readouts, prefills.as_deref());
        Ok(())
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        let part = &mut self.part;
        self.held.end(&self.run, shots, || part.end_shots(shots));
    }
}

/// Builds the kit's chain, the kit's driver over the wrapper, around each
/// executor `make` builds, its driver needing `needs`.
pub(crate) struct ExecutorMount<M> {
    pub(crate) make: M,
    pub(crate) needs: Needs,
}

impl<M, E> Mount for ExecutorMount<M>
where
    M: FnMut() -> E,
    E: Execute,
{
    type Below = ExecutorUnderTest<E>;

    fn chain(&mut self, run: &SharedRun) -> Result<Runner<ScriptedDriver, Self::Below>, String> {
        let part = ExecutorUnderTest::new((self.make)(), run.clone());
        name_part(run, part.name());
        note_offer(run, &part.offers);
        let driver = driver_for(run, part.accepts(), self.needs.clone())?;
        compose(driver, Executor::new(part))
    }
}
