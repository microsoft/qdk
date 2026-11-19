// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`AdapterUnderTest`]: the wrapper of an adapter under test, which sees
//! what crosses the adapter from above; and [`AdapterMount`], which builds
//! the kit's chain around a new adapter, the kit's executor below it
//! logging what it sends below.

use executor_protocol::{
    Adapt, Adapter, AdapterSetup, Batch, BatchReadouts, Composed, Execute, ExecuteError, Executor,
    ExecutorSetup, InstructionSet, Needs, NotReady, Offer, OpcodeMap, Runner, Seed, ShotId,
    ShotSet,
};

use super::{
    HeldShots, Mount, RecordingExecutor, ScriptedDriver, compose, driver_for, log_rows, name_part,
    note_offer,
};
use crate::readout_prefills::prefilled;
use crate::shared_run::SharedRun;

/// The adapter under test, wrapped: it forwards each call, and around each
/// `adapt`, logs the seed it got, prefills the readouts in a prefill run,
/// may first drop a call unpolled, and logs the rows the adapter answered.
/// It logs, too, each `end_shots` that panics ending shots among which some
/// the adapter did not hold, and keeps every `end_shots` from the adapter
/// when the setup says.
pub(crate) struct AdapterUnderTest<A> {
    part: A,
    run: SharedRun,
    held: HeldShots,
}

impl<A: Adapt> AdapterSetup for AdapterUnderTest<A> {
    fn name(&self) -> &str {
        self.part.name()
    }

    fn accepts(&self) -> &InstructionSet {
        self.part.accepts()
    }

    fn sends(&self) -> &InstructionSet {
        self.part.sends()
    }

    fn needs_below(&self, above: Needs) -> Needs {
        self.part.needs_below(above)
    }

    fn offers_above(&self, below: Offer) -> Offer {
        self.part.offers_above(below)
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.part.bind(below);
    }
}

impl<A: Adapt> Adapt for AdapterUnderTest<A> {
    fn ready(&mut self, above: &Needs) -> impl Future<Output = Result<(), NotReady>> {
        self.part.ready(above)
    }

    async fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        let execute = self.run.begin_execute();
        self.run
            .write(|log| log.seeds_above.push((execute, seed.shot(ShotId::new(0)))));
        let prefills = prefilled(&self.run, execute, readouts);
        self.held.hold(batches);
        if self.run.setup().drops_a_call_unpolled {
            // On a copy, so that a part that reuses the dropped call's
            // work instead of doing the polled one's shows.
            let mut scratch = readouts.to_vec();
            drop(self.part.adapt(batches, seed, below, &mut scratch));
        }
        self.part.adapt(batches, seed, below, readouts).await?;
        log_rows(&self.run, execute, batches, readouts, prefills.as_deref());
        Ok(())
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        let part = &mut self.part;
        self.held.end(&self.run, shots, || part.end_shots(shots));
    }
}

/// Builds the kit's chain, the kit's driver over the wrapper over the
/// kit's executor, around each adapter `make` builds, its driver needing
/// `needs`.
pub(crate) struct AdapterMount<M> {
    pub(crate) make: M,
    pub(crate) needs: Needs,
}

impl<M, A> Mount for AdapterMount<M>
where
    M: FnMut() -> A,
    A: Adapt,
{
    type Below = Composed<AdapterUnderTest<A>, RecordingExecutor>;

    fn chain(&mut self, run: &SharedRun) -> Result<Runner<ScriptedDriver, Self::Below>, String> {
        let part = AdapterUnderTest {
            part: (self.make)(),
            run: run.clone(),
            held: HeldShots::default(),
        };
        name_part(run, part.name());
        let driver = driver_for(run, part.accepts(), self.needs.clone())?;
        let below = RecordingExecutor::below_an_adapter(run.clone(), part.sends().clone());
        note_offer(run, &part.offers_above(below.offers()));
        compose(driver, Adapter::new(part) >> Executor::new(below))
    }
}
