// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fake parts across a translation of [`WithToffoli`] to
//! [`WithoutToffoli`]: drivers of the first, an adapter and an executor
//! of the second, and [`CczDecomposition`], the translating adapter
//! between them, for the tests of translating parts, and of readouts kept
//! from one round to the next.

use std::cell::Cell;
use std::future::poll_fn;
use std::pin::pin;
use std::rc::Rc;
use std::task::Poll;

use super::Log;
use crate::prelude::*;
use crate::roles::lent_translating_adapter;
use crate::tests::allocations;
use crate::tests::example_sets::{BitReadout, ShotFlags};
use crate::tests::example_sets::{
    CliffordT, Qubits, ToffoliGate, WithToffoli, WithoutToffoli, on_qubits,
};
use crate::{
    Batch, BatchReadouts, Capabilities, DriveError, ExecuteError, InstructionSet, NotReady,
    Readouts, Resources, RunRequirement, Seed, ShotSet, StatesLimits, TranslatingAdapter,
};

/// A driver sending `gates`, one batch of every shot, and requiring
/// `qubits` qubits and batches of its gates' count. Each shot's output is
/// its readouts and conditions.
pub(crate) struct ToffoliProgram {
    gates: Vec<ToffoliGate>,
    qubits: u32,
}

impl ToffoliProgram {
    pub(crate) fn new(gates: &[ToffoliGate], qubits: u32) -> StatesLimits<Self> {
        Self {
            gates: gates.to_vec(),
            qubits,
        }
        .stating_limits()
    }
}

impl DriverRequirements for ToffoliProgram {
    /// The run's, on `qubits` qubits, in batches of its gates' count.
    fn requires(&self, run: &RunRequirement) -> Capabilities<WithToffoli> {
        let gates = u32::try_from(self.gates.len()).expect("fewer than 2^32 gates");
        let resources = run.resources.with_instructions_per_batch(gates);
        Capabilities::new(resources, run.features, Qubits(self.qubits))
    }
}

impl Driver for ToffoliProgram {
    type Sends = WithToffoli;
    type Output = (Vec<BitReadout>, ShotFlags);

    fn sends(&self) -> &WithToffoli {
        &WithToffoli
    }

    fn bind(&mut self, (): ()) {}

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor<Accepts = WithToffoli>,
    ) -> Result<Vec<Self::Output>, DriveError> {
        let readout_slots = self
            .gates
            .iter()
            .map(|gate| WithToffoli.readout_slots(gate))
            .sum();
        let batches = [Batch::new(shots, &self.gates, readout_slots)];
        let mut readouts = [BatchReadouts::new(0, 0)];
        below.execute(&batches, seed, &mut readouts).await?;
        let [readouts] = readouts;
        Ok((0..shots.len())
            .map(|shot| (readouts.row(shot).to_vec(), readouts.conditions(shot)))
            .collect())
    }
}

/// An executor of [`WithoutToffoli`] on `qubits` qubits, which runs
/// nothing: it logs each batch it receives, and writes a reading into
/// every row, as many of its values as the row has slots.
pub(crate) struct CliffordTRegister {
    provides: Capabilities<WithoutToffoli>,
    reading: Vec<BitReadout>,
    log: Log,
}

impl CliffordTRegister {
    /// The register writing `reading` into every row.
    pub(crate) fn new(qubits: u32, reading: &[BitReadout], log: &Log) -> StatesLimits<Self> {
        Self {
            provides: on_qubits(qubits),
            reading: reading.to_vec(),
            log: log.clone(),
        }
        .stating_limits()
    }
}

impl StatesLimits<CliffordTRegister> {
    /// This register, taking batches of at most `n` instructions.
    pub(crate) fn taking(mut self, n: u32) -> Self {
        self.part.provides = (self.part.provides)
            .map_resources(|resources| resources.with_instructions_per_batch(n));
        self
    }
}

impl ExecutorLimits for CliffordTRegister {
    fn provides(&self) -> Option<&Capabilities<WithoutToffoli>> {
        Some(&self.provides)
    }
}

impl Executor for CliffordTRegister {
    type Accepts = WithoutToffoli;

    fn accepts(&self) -> &WithoutToffoli {
        &WithoutToffoli
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_, WithoutToffoli>],
        _seed: Seed,
        readouts: &mut [BatchReadouts<WithoutToffoli>],
    ) -> Result<(), ExecuteError> {
        for (batch, readouts) in batches.iter().zip(readouts) {
            self.log.push(format!(
                "runs {:?}, checked: {}",
                batch.instructions,
                batch.is_checked()
            ));
            for shot in 0..readouts.shots() {
                let row = readouts.row_mut(shot);
                let slots = row.len();
                row.copy_from_slice(&self.reading[..slots]);
            }
        }
        Ok(())
    }
}

/// An adapter of [`WithoutToffoli`] that passes each batch below as it
/// came: a part below a translating adapter, in the set it sends.
pub(crate) struct CliffordTPassThrough;

impl Adapter for CliffordTPassThrough {
    type Set = WithoutToffoli;

    fn accepts(&self) -> &WithoutToffoli {
        &WithoutToffoli
    }

    fn sends(&self) -> &WithoutToffoli {
        &WithoutToffoli
    }

    fn bind(&mut self, (): ()) {}

    async fn adapt(
        &mut self,
        batches: &[Batch<'_, WithoutToffoli>],
        seed: Seed,
        below: &mut impl Executor<Accepts = WithoutToffoli>,
        readouts: &mut [BatchReadouts<WithoutToffoli>],
    ) -> Result<(), ExecuteError> {
        below.execute(batches, seed, readouts).await
    }
}

/// A translating adapter of [`WithToffoli`] to [`WithoutToffoli`]: each
/// `ccz` sent below as the standard decomposition of seven `t` and `tdg`
/// and six `cx`, every other gate as its namesake, and each shot's
/// readouts and conditions read back as they came, since only `mz`, sent
/// as itself, has readout slots. It requires below what arrives, with 13
/// instructions a batch for each one that arrives (its longest
/// decomposition) and `ancillas` more qubits, as a decomposition using
/// them would; it takes at most the qubits it provides, and is ready
/// unless it is told why not. It keeps the batches it sends and their
/// readouts from one `adapt` to the next.
pub(crate) struct CczDecomposition {
    ancillas: u32,
    provides: Option<Capabilities<WithToffoli>>,
    not_ready: Option<&'static str>,
    /// The instructions of each batch of the last `adapt`, then any kept
    /// from an earlier `adapt` of more batches.
    sent: Vec<Vec<CliffordT>>,
    readouts: Readouts<WithoutToffoli>,
}

impl CczDecomposition {
    pub(crate) fn new() -> Self {
        Self {
            ancillas: 0,
            provides: None,
            not_ready: None,
            sent: Vec::new(),
            readouts: Readouts::new(),
        }
    }

    /// This decomposition, requiring `ancillas` more qubits below.
    pub(crate) fn with_ancillas(ancillas: u32) -> Self {
        Self {
            ancillas,
            ..Self::new()
        }
    }

    /// This decomposition, taking programs of at most `qubits` qubits.
    pub(crate) fn for_qubits(qubits: u32) -> Self {
        Self {
            provides: Some(on_qubits(qubits)),
            ..Self::new()
        }
    }

    /// This decomposition, not ready, for `why`.
    pub(crate) fn not_ready(why: &'static str) -> Self {
        Self {
            not_ready: Some(why),
            ..Self::new()
        }
    }

    /// The decomposition of `ccz(a, b, c)`.
    pub(crate) fn ccz(a: u32, b: u32, c: u32) -> [CliffordT; 13] {
        use CliffordT::{Cx, T, Tdg};
        [
            Cx(b, c),
            Tdg(c),
            Cx(a, c),
            T(c),
            Cx(b, c),
            Tdg(c),
            Cx(a, c),
            T(b),
            T(c),
            Cx(a, b),
            T(a),
            Tdg(b),
            Cx(a, b),
        ]
    }
}

impl TranslatingAdapter for CczDecomposition {
    type Accepts = WithToffoli;
    type Sends = WithoutToffoli;

    fn accepts(&self) -> &WithToffoli {
        &WithToffoli
    }

    fn sends(&self) -> &WithoutToffoli {
        &WithoutToffoli
    }

    fn provides(&self) -> Option<&Capabilities<WithToffoli>> {
        self.provides.as_ref()
    }

    fn requires(&self, above: &Capabilities<WithToffoli>) -> Capabilities<WithoutToffoli> {
        let Qubits(qubits) = above.set_capabilities();
        Capabilities::new(
            longest_decomposition_per_instruction(above.resources()),
            *above.features(),
            Qubits(qubits + self.ancillas),
        )
    }

    async fn ready(&mut self, _above: &Capabilities<WithToffoli>) -> Result<(), NotReady> {
        match self.not_ready {
            Some(why) => Err(NotReady::new(why)),
            None => Ok(()),
        }
    }

    fn bind(&mut self, (): ()) {}

    async fn adapt(
        &mut self,
        batches: &[Batch<'_, WithToffoli>],
        seed: Seed,
        below: &mut impl Executor<Accepts = WithoutToffoli>,
        readouts: &mut [BatchReadouts<WithToffoli>],
    ) -> Result<(), ExecuteError> {
        if self.sent.len() < batches.len() {
            self.sent.resize_with(batches.len(), Vec::new);
        }
        for (batch, sent) in batches.iter().zip(&mut self.sent) {
            sent.clear();
            for gate in batch.instructions {
                match *gate {
                    ToffoliGate::H(q) => sent.push(CliffordT::H(q)),
                    ToffoliGate::Mz(q) => sent.push(CliffordT::Mz(q)),
                    ToffoliGate::Ccz(a, b, c) => sent.extend(Self::ccz(a, b, c)),
                }
            }
        }
        let sent: Vec<Batch<'_, WithoutToffoli>> = (batches.iter().zip(&self.sent))
            .map(|(batch, sent)| {
                Batch::new(batch.shots, sent, batch.readout_slots()).checked_if(batch.is_checked())
            })
            .collect();
        let sent_readouts = self.readouts.readouts_for(&sent);
        below.execute(&sent, seed, sent_readouts).await?;
        for (below, above) in sent_readouts.iter().zip(readouts) {
            for shot in 0..below.shots() {
                above.row_mut(shot).copy_from_slice(below.row(shot));
                above.set_conditions(shot, below.conditions(shot));
            }
        }
        Ok(())
    }
}

lent_translating_adapter!([] CczDecomposition);

/// A driver sending a round at a time, one unchecked batch of `gates` per
/// shot, in one `execute`, into the readouts it keeps, then the same
/// `execute` again, into the same readouts, whose allocations it notes in
/// `allocated` as it polls it.
pub(crate) struct RoundsIntoItsReadouts {
    gates: Vec<ToffoliGate>,
    allocated: Rc<Cell<Option<usize>>>,
}

impl RoundsIntoItsReadouts {
    pub(crate) fn new(gates: &[ToffoliGate], allocated: &Rc<Cell<Option<usize>>>) -> Self {
        Self {
            gates: gates.to_vec(),
            allocated: Rc::clone(allocated),
        }
    }
}

impl Driver for RoundsIntoItsReadouts {
    type Sends = WithToffoli;
    type Output = ();

    fn sends(&self) -> &WithToffoli {
        &WithToffoli
    }

    fn bind(&mut self, (): ()) {}

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor<Accepts = WithToffoli>,
    ) -> Result<Vec<()>, DriveError> {
        let readout_slots = (self.gates.iter())
            .map(|gate| WithToffoli.readout_slots(gate))
            .sum();
        let each = (0..shots.len())
            .map(|at| shots.subset(&[at]).expect("a shot of the run"))
            .collect::<Vec<_>>();
        let batches = each
            .iter()
            .map(|shot| Batch::new(shot, &self.gates, readout_slots))
            .collect::<Vec<_>>();
        let mut kept = Readouts::new();
        below
            .execute(&batches, seed, kept.readouts_for(&batches))
            .await?;
        let mut execute = pin!(below.execute(&batches, seed, kept.readouts_for(&batches)));
        let (count, answered) = poll_fn(|context| {
            let mut answered = Poll::Pending;
            let count = allocations(|| answered = execute.as_mut().poll(context));
            Poll::Ready((count, answered))
        })
        .await;
        let Poll::Ready(ran) = answered else {
            panic!("the `execute` answers at its first poll");
        };
        ran?;
        self.allocated.set(Some(count));
        Ok(vec![(); shots.len()])
    }
}

/// `above`, with 13 instructions a batch for each one it states: a batch
/// of the decompositions above, each instruction sent as at most 13.
fn longest_decomposition_per_instruction(above: &Resources) -> Resources {
    Resources {
        instructions_per_batch: (above.instructions_per_batch).map(|n| n.saturating_mul(13)),
        ..*above
    }
}
