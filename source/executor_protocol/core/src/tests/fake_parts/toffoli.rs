// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fake parts across a lowering of [`WithToffoli`] to
//! [`WithoutToffoli`]: a driver of the first, an executor of the second,
//! and the lowerings between them, for the tests of [`Lowered`].
//!
//! [`Lowered`]: crate::Lowered

use super::Log;
use crate::prelude::*;
use crate::tests::example_sets::{CliffordT, Qubits, ToffoliGate, WithToffoli, WithoutToffoli};
use crate::{
    Batch, BatchReadouts, Capabilities, DriveError, ExecuteError, Features, ReadoutValue,
    Resources, RunRequirement, Seed, ShotConditions, ShotSet,
};

/// The capabilities of `S` on `qubits` qubits, and unconstrained
/// otherwise.
fn on_qubits<S: InstructionSet<Capabilities = Qubits>>(qubits: u32) -> Capabilities<S> {
    Capabilities::new(
        Resources::unconstrained(),
        Features::unconstrained(),
        Qubits(qubits),
    )
}

/// A driver sending `gates`, one batch of every shot, checked if
/// `checked`, and requiring `qubits` qubits. Each shot's output is its
/// readouts and conditions.
pub(crate) struct ToffoliProgram {
    gates: Vec<ToffoliGate>,
    qubits: u32,
    checked: bool,
}

impl ToffoliProgram {
    pub(crate) fn new(gates: &[ToffoliGate], qubits: u32) -> Self {
        Self {
            gates: gates.to_vec(),
            qubits,
            checked: false,
        }
    }

    /// This program, its batch checked if `checked`.
    pub(crate) fn checked_if(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }
}

impl Driver for ToffoliProgram {
    type Sends = WithToffoli;
    type Output = (Vec<ReadoutValue>, ShotConditions);

    fn sends(&self) -> &WithToffoli {
        &WithToffoli
    }

    /// The run's, on `qubits` qubits.
    fn requires(&self, run: &RunRequirement) -> Capabilities<WithToffoli> {
        Capabilities::new(run.resources, run.features, Qubits(self.qubits))
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
/// nothing: it logs each batch it receives, and writes `reading` into
/// every row, as many of its values as the row has slots, and
/// [`ShotConditions::REJECTED`] on every other shot, from the second.
pub(crate) struct CliffordTRegister {
    provides: Capabilities<WithoutToffoli>,
    reading: Vec<ReadoutValue>,
    log: Log,
}

impl CliffordTRegister {
    pub(crate) fn new(qubits: u32, reading: &[ReadoutValue], log: &Log) -> Self {
        Self {
            provides: on_qubits(qubits),
            reading: reading.to_vec(),
            log: log.clone(),
        }
    }
}

impl Executor for CliffordTRegister {
    type Accepts = WithoutToffoli;

    fn accepts(&self) -> &WithoutToffoli {
        &WithoutToffoli
    }

    fn provides(&self) -> &Capabilities<WithoutToffoli> {
        &self.provides
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_, WithoutToffoli>],
        _seed: Seed,
        readouts: &mut [BatchReadouts],
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
                if shot % 2 == 1 {
                    readouts.set_conditions(shot, ShotConditions::REJECTED);
                }
            }
        }
        Ok(())
    }
}

/// `ccz` rewritten as the standard decomposition of seven `t` and `tdg`
/// and six `cx`, and every other gate as its namesake, requiring below
/// what arrives and `ancillas` more qubits, as a decomposition using them
/// would.
pub(crate) struct ToffoliFree {
    ancillas: u32,
}

impl ToffoliFree {
    pub(crate) fn new() -> Self {
        Self { ancillas: 0 }
    }

    /// This lowering, requiring `ancillas` more qubits below.
    pub(crate) fn with_ancillas(ancillas: u32) -> Self {
        Self { ancillas }
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

    fn requires(&self, above: &Capabilities<WithToffoli>) -> Capabilities<WithoutToffoli> {
        let Qubits(qubits) = above.set_capabilities();
        Capabilities::new(
            *above.resources(),
            *above.features(),
            Qubits(qubits + self.ancillas),
        )
    }

    fn bind(&mut self, (): ()) {}

    fn lower(&self, gate: &ToffoliGate, into: &mut Vec<CliffordT>) {
        match *gate {
            ToffoliGate::H(q) => into.push(CliffordT::H(q)),
            ToffoliGate::Mz(q) => into.push(CliffordT::Mz(q)),
            ToffoliGate::Ccz(a, b, c) => into.extend(Self::ccz(a, b, c)),
        }
    }
}

/// Each `mz` read three times below, its readout the majority of the
/// three, as a lowering suppressing readout errors would; every other gate
/// as its namesake. It requires below what arrives.
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
            *above.resources(),
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

    fn read(&self, _gate: &ToffoliGate, lowered: &[ReadoutValue], into: &mut [ReadoutValue]) {
        let ones = lowered.iter().filter(|value| **value == ReadoutValue::One);
        into[0] = if ones.count() >= 2 {
            ReadoutValue::One
        } else {
            ReadoutValue::Zero
        };
    }
}

/// Each `mz` read twice below, keeping the default `read`, which copies
/// one readout for one: a lowering that forgot to read its own.
pub(crate) struct ReadTwiceUnread;

impl Lowering for ReadTwiceUnread {
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
            *above.resources(),
            *above.features(),
            *above.set_capabilities(),
        )
    }

    fn bind(&mut self, (): ()) {}

    fn lower(&self, gate: &ToffoliGate, into: &mut Vec<CliffordT>) {
        match *gate {
            ToffoliGate::Mz(q) => into.extend([CliffordT::Mz(q); 2]),
            other => ToffoliFree::new().lower(&other, into),
        }
    }
}
