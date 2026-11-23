// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fake parts across a lowering of [`WithToffoli`] to
//! [`WithoutToffoli`]: a driver of the first, an adapter and an executor
//! of the second, and the lowerings between them, for the tests of
//! [`Lowered`].
//!
//! [`Lowered`]: crate::Lowered

use super::Log;
use crate::prelude::*;
use crate::tests::example_sets::{BitReadout, ShotFlags};
use crate::tests::example_sets::{
    CliffordT, Qubits, ToffoliGate, WithToffoli, WithoutToffoli, on_qubits,
};
use crate::{
    Batch, BatchReadouts, Capabilities, DriveError, ExecuteError, NotReady, Resources,
    RunRequirement, Seed, ShotSet, StatesLimits,
};

/// A driver sending `gates`, one batch of every shot, checked if
/// `checked`, and requiring `qubits` qubits and batches of its gates'
/// count. Each shot's output is its readouts and conditions.
pub(crate) struct ToffoliProgram {
    gates: Vec<ToffoliGate>,
    qubits: u32,
    checked: bool,
}

impl ToffoliProgram {
    pub(crate) fn new(gates: &[ToffoliGate], qubits: u32) -> StatesLimits<Self> {
        Self {
            gates: gates.to_vec(),
            qubits,
            checked: false,
        }
        .stating_limits()
    }
}

impl StatesLimits<ToffoliProgram> {
    /// This program, its batch checked if `checked`.
    pub(crate) fn checked_if(mut self, checked: bool) -> Self {
        self.part.checked = checked;
        self
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
        let batches = [Batch::new(shots, &self.gates, readout_slots).checked_if(self.checked)];
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
/// every row, as many of its values as the row has slots, and
/// [`ShotFlags::REJECTED`] on every other shot of a batch, from the
/// second.
pub(crate) struct CliffordTRegister {
    provides: Capabilities<WithoutToffoli>,
    /// The reading of each batch of an `execute`, by its position, the
    /// last repeated for the batches beyond.
    readings: Vec<Vec<BitReadout>>,
    log: Log,
}

impl CliffordTRegister {
    /// The register writing `reading` into every row.
    pub(crate) fn new(qubits: u32, reading: &[BitReadout], log: &Log) -> StatesLimits<Self> {
        Self::with_readings(qubits, &[reading], log)
    }

    /// The register writing `readings[b]` into every row of the `b`-th
    /// batch of each `execute`, and the last reading into the rows of the
    /// batches beyond.
    pub(crate) fn with_readings(
        qubits: u32,
        readings: &[&[BitReadout]],
        log: &Log,
    ) -> StatesLimits<Self> {
        Self {
            provides: on_qubits(qubits),
            readings: readings.iter().map(|reading| reading.to_vec()).collect(),
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
    fn provides(&self) -> &Capabilities<WithoutToffoli> {
        &self.provides
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
        for (at, (batch, readouts)) in batches.iter().zip(readouts).enumerate() {
            self.log.push(format!(
                "runs {:?}, checked: {}",
                batch.instructions,
                batch.is_checked()
            ));
            let reading = &self.readings[at.min(self.readings.len() - 1)];
            for shot in 0..readouts.shots() {
                let row = readouts.row_mut(shot);
                let slots = row.len();
                row.copy_from_slice(&reading[..slots]);
                if shot % 2 == 1 {
                    readouts.set_conditions(shot, ShotFlags::REJECTED);
                }
            }
        }
        Ok(())
    }
}

/// An adapter of [`WithoutToffoli`] that passes each batch below as it
/// came: a part below a lowering, in the lowered set.
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

/// `ccz` rewritten as the standard decomposition of seven `t` and `tdg`
/// and six `cx`, and every other gate as its namesake, requiring below
/// what arrives, with 13 instructions a batch for each one that arrives
/// (its longest rewriting) and `ancillas` more qubits, as a decomposition
/// using them would; it takes at most the qubits it provides, and is
/// ready unless it is told why not.
pub(crate) struct ToffoliFree {
    ancillas: u32,
    provides: Capabilities<WithToffoli>,
    not_ready: Option<&'static str>,
}

impl ToffoliFree {
    pub(crate) fn new() -> Self {
        Self {
            ancillas: 0,
            provides: WithToffoli::unconstrained().clone(),
            not_ready: None,
        }
    }

    /// This lowering, requiring `ancillas` more qubits below.
    pub(crate) fn with_ancillas(ancillas: u32) -> Self {
        Self {
            ancillas,
            ..Self::new()
        }
    }

    /// This lowering, taking programs of at most `qubits` qubits.
    pub(crate) fn for_qubits(qubits: u32) -> Self {
        Self {
            provides: on_qubits(qubits),
            ..Self::new()
        }
    }

    /// This lowering, not ready, for `why`.
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

impl Lowering for ToffoliFree {
    type From = WithToffoli;
    type To = WithoutToffoli;

    fn accepts(&self) -> &WithToffoli {
        &WithToffoli
    }

    fn sends(&self) -> &WithoutToffoli {
        &WithoutToffoli
    }

    fn provides(&self) -> &Capabilities<WithToffoli> {
        &self.provides
    }

    fn requires(&self, above: &Capabilities<WithToffoli>) -> Capabilities<WithoutToffoli> {
        let Qubits(qubits) = above.set_capabilities();
        Capabilities::new(
            longest_rewriting_per_instruction(above.resources()),
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

    fn lower(&self, gate: &ToffoliGate, into: &mut Vec<CliffordT>) {
        match *gate {
            ToffoliGate::H(q) => into.push(CliffordT::H(q)),
            ToffoliGate::Mz(q) => into.push(CliffordT::Mz(q)),
            ToffoliGate::Ccz(a, b, c) => into.extend(Self::ccz(a, b, c)),
        }
    }

    fn read(&self, _gate: &ToffoliGate, lowered: &[BitReadout], into: &mut [BitReadout]) {
        into.copy_from_slice(lowered);
    }

    fn read_conditions(&self, lowered: ShotFlags) -> ShotFlags {
        lowered
    }
}

/// Each `mz` read three times below, its readout the majority of the
/// three, as a lowering suppressing readout errors would; every other gate
/// as [`ToffoliFree`] lowers it. It requires below what arrives, with 13
/// instructions a batch for each one that arrives (`ccz`'s rewriting, its
/// longest).
pub(crate) struct ReadThrice;

impl Lowering for ReadThrice {
    type From = WithToffoli;
    type To = WithoutToffoli;

    fn accepts(&self) -> &WithToffoli {
        &WithToffoli
    }

    fn sends(&self) -> &WithoutToffoli {
        &WithoutToffoli
    }

    fn requires(&self, above: &Capabilities<WithToffoli>) -> Capabilities<WithoutToffoli> {
        Capabilities::new(
            longest_rewriting_per_instruction(above.resources()),
            *above.features(),
            *above.set_capabilities(),
        )
    }

    fn bind(&mut self, (): ()) {}

    fn lower(&self, gate: &ToffoliGate, into: &mut Vec<CliffordT>) {
        match *gate {
            ToffoliGate::Mz(q) => into.extend([CliffordT::Mz(q); 3]),
            other => ToffoliFree::new().lower(&other, into),
        }
    }

    fn read(&self, _gate: &ToffoliGate, lowered: &[BitReadout], into: &mut [BitReadout]) {
        let ones = lowered.iter().filter(|value| **value == BitReadout::One);
        into[0] = if ones.count() >= 2 {
            BitReadout::One
        } else {
            BitReadout::Zero
        };
    }

    fn read_conditions(&self, lowered: ShotFlags) -> ShotFlags {
        lowered
    }
}

/// A faulty lowering: each `h` written 3 qubits higher, on a qubit it does
/// not require below, and every other gate as [`ReadThrice`] lowers it,
/// requiring below what [`ReadThrice`] does. A checked batch of its
/// lowering is refused below.
pub(crate) struct UsesUnstatedQubits;

impl Lowering for UsesUnstatedQubits {
    type From = WithToffoli;
    type To = WithoutToffoli;

    fn accepts(&self) -> &WithToffoli {
        &WithToffoli
    }

    fn sends(&self) -> &WithoutToffoli {
        &WithoutToffoli
    }

    fn requires(&self, above: &Capabilities<WithToffoli>) -> Capabilities<WithoutToffoli> {
        ReadThrice.requires(above)
    }

    fn bind(&mut self, (): ()) {}

    fn lower(&self, gate: &ToffoliGate, into: &mut Vec<CliffordT>) {
        match *gate {
            ToffoliGate::H(q) => into.push(CliffordT::H(q + 3)),
            other => ReadThrice.lower(&other, into),
        }
    }

    fn read(&self, gate: &ToffoliGate, lowered: &[BitReadout], into: &mut [BitReadout]) {
        ReadThrice.read(gate, lowered, into);
    }

    fn read_conditions(&self, lowered: ShotFlags) -> ShotFlags {
        lowered
    }
}

/// `above`, with 13 instructions a batch for each one it states: a batch
/// of the lowerings above, each instruction rewritten as at most 13.
fn longest_rewriting_per_instruction(above: &Resources) -> Resources {
    Resources {
        instructions_per_batch: (above.instructions_per_batch).map(|n| n.saturating_mul(13)),
        ..*above
    }
}
