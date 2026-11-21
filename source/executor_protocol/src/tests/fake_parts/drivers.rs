// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fake drivers.

use std::sync::OnceLock;

use super::{CoinGates, Log, Values, coin_gate, coin_gates, qubits};
use crate::prelude::*;
use crate::{
    Batch, Capabilities, DriveError, Features, Instruction, NotReady, OpcodeMap, ReadoutValue,
    Readouts, Resources, RunProvision, Seed, ShotConditions, ShotSet,
};

/// A coin tossed on qubit 0, copied onto qubit 1: a toss for every shot in
/// one batch, then, by the coin, a flip and a read, or a read, as two
/// batches of one `execute`. Each shot's output is its two values.
pub(crate) struct CoinCopy {
    sequences: [Vec<Instruction>; 3],
    /// Each sequence's readout slots.
    slots: [u32; 3],
    /// Where it logs that it gets ready, if anywhere.
    log: Option<Log>,
    /// Why it refuses every run in `ready`, if it does.
    busy: Option<&'static str>,
}

impl CoinCopy {
    pub(crate) fn new() -> Self {
        let sequences = [
            vec![coin_gate("h", 0), coin_gate("mz", 0)],
            vec![coin_gate("x", 1), coin_gate("mz", 1)],
            vec![coin_gate("mz", 1)],
        ];
        let slots = sequences.each_ref().map(|sequence| {
            coin_gates()
                .readout_slots_of(sequence)
                .expect("instructions of the set")
        });
        Self {
            sequences,
            slots,
            log: None,
            busy: None,
        }
    }

    /// This driver, logging to `log` when it gets ready.
    pub(crate) fn logging(self, log: &Log) -> Self {
        Self {
            log: Some(log.clone()),
            ..self
        }
    }

    /// This driver, refusing every run in `ready`, for `why`.
    pub(crate) fn busy(self, why: &'static str) -> Self {
        Self {
            busy: Some(why),
            ..self
        }
    }
}

impl Driver for CoinCopy {
    /// [`CoinGates`], on 2 qubits.
    fn requires(&self) -> &Capabilities {
        static REQUIRES: OnceLock<Capabilities> = OnceLock::new();
        REQUIRES.get_or_init(|| Capabilities::new(CoinGates::set()).with_resources(qubits(2)))
    }

    fn bind(&mut self, below: OpcodeMap) {
        for instruction in self.sequences.iter_mut().flatten() {
            instruction.opcode = below.map(instruction.opcode);
        }
    }

    type Output = Values;

    async fn ready(&mut self) -> Result<(), NotReady> {
        if let Some(log) = &self.log {
            log.push("CoinCopy ready");
        }
        match self.busy {
            Some(why) => Err(NotReady::new(why)),
            None => Ok(()),
        }
    }

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor,
    ) -> Result<Vec<Values>, DriveError> {
        let mut tossed = Readouts::new();
        let toss = [Batch::new(shots, &self.sequences[0], self.slots[0])];
        let tossed = tossed.readouts_for(&toss);
        below.execute(&toss, seed, tossed).await?;
        let coin = &tossed[0];
        let mut outputs: Vec<Values> = (0..shots.len()).map(|i| coin.row(i).to_vec()).collect();
        let at = |value: ReadoutValue| -> Vec<usize> {
            (0..shots.len())
                .filter(|&i| coin.row(i) == [value])
                .collect()
        };
        let (heads, tails) = (at(ReadoutValue::One), at(ReadoutValue::Zero));
        let sets: Vec<(&[usize], ShotSet, &[Instruction], u32)> = [
            (&heads, &self.sequences[1], self.slots[1]),
            (&tails, &self.sequences[2], self.slots[2]),
        ]
        .into_iter()
        .filter_map(|(positions, sequence, slots)| {
            Some((
                positions.as_slice(),
                shots.select(positions)?,
                sequence.as_slice(),
                slots,
            ))
        })
        .collect();
        let batches: Vec<Batch<'_>> = sets
            .iter()
            .map(|(_, shots, instructions, slots)| Batch::new(shots, instructions, *slots))
            .collect();
        let mut read = Readouts::new();
        let read = read.readouts_for(&batches);
        below.execute(&batches, seed, read).await?;
        for ((positions, _, _, _), rows) in sets.iter().zip(&*read) {
            for (j, &i) in positions.iter().enumerate() {
                outputs[i].extend_from_slice(rows.row(j));
            }
        }
        below.end_shots(shots);
        Ok(outputs)
    }
}

/// A driver that sends its instructions to every shot, as one batch of one
/// `execute`, or its two sequences to each half of the shots, as two
/// batches of one `execute`, unchecked unless it is told; each shot's
/// output is its row and its conditions.
pub(crate) struct Program {
    sequences: Vec<Vec<Instruction>>,
    /// Each sequence's readout slots in the set it requires, counted when
    /// it is bound; 0 for one beyond that set, which a checked batch
    /// refuses for its opcode.
    slots: Vec<u32>,
    requires: Capabilities,
    /// What of a run it serves itself: nothing, unless it is told.
    provides: RunProvision,
    /// Whether it ends its shots below once it has their readouts.
    ends_its_shots: bool,
    /// Whether each of its batches is checked, by sequence.
    checked: Vec<bool>,
}

impl Program {
    /// The instructions `text`, each a mnemonic of [`CoinGates`] and its
    /// qubit, requiring the qubits they name.
    pub(crate) fn new(text: &[(&str, u32)]) -> Self {
        Self::of(&[text])
    }

    /// `first` on the first half of the shots, rounded down, and `second`
    /// on the others, as two batches of one `execute`.
    pub(crate) fn halves(first: &[(&str, u32)], second: &[(&str, u32)]) -> Self {
        Self::of(&[first, second])
    }

    fn of(texts: &[&[(&str, u32)]]) -> Self {
        let qubits = texts
            .iter()
            .copied()
            .flatten()
            .map(|&(_, qubit)| qubit + 1)
            .max()
            .unwrap_or(0);
        Self {
            sequences: texts
                .iter()
                .map(|text| {
                    text.iter()
                        .map(|&(mnemonic, qubit)| coin_gate(mnemonic, qubit))
                        .collect()
                })
                .collect(),
            slots: Vec::new(),
            requires: Capabilities::new(CoinGates::set()).with_resources(super::qubits(qubits)),
            provides: RunProvision::new(),
            ends_its_shots: true,
            checked: vec![false; texts.len()],
        }
    }

    /// `instructions`, as one batch for every shot, requiring `requires`,
    /// whose instructions they are.
    pub(crate) fn sending(instructions: Vec<Instruction>, requires: Capabilities) -> Self {
        Self {
            sequences: vec![instructions],
            slots: Vec::new(),
            requires,
            provides: RunProvision::new(),
            ends_its_shots: true,
            checked: vec![false],
        }
    }

    /// This program, requiring `resources` in place of its own.
    pub(crate) fn with_resources(mut self, resources: Resources) -> Self {
        self.requires = self.requires.with_resources(resources);
        self
    }

    /// This program, requiring `features` in place of its own.
    pub(crate) fn with_features(mut self, features: Features) -> Self {
        self.requires = self.requires.with_features(features);
        self
    }

    /// This program, saying it serves `provides` of each run, as one that
    /// splits each run into runs the parts below take would; it passes
    /// each run below whole, splitting none.
    pub(crate) fn serving_runs(mut self, provides: RunProvision) -> Self {
        self.provides = provides;
        self
    }

    /// This program, its batches checked.
    pub(crate) fn with_checked_batches(mut self) -> Self {
        self.checked.fill(true);
        self
    }

    /// This program of [`halves`](Self::halves), its first half's batch
    /// checked if `first`, its second's if `second`.
    pub(crate) fn with_checked_halves(mut self, first: bool, second: bool) -> Self {
        assert_eq!(self.checked.len(), 2, "a program of halves");
        self.checked = vec![first, second];
        self
    }

    /// This program, never ending its shots below, which it leaves to the
    /// runner.
    pub(crate) fn not_ending(mut self) -> Self {
        self.ends_its_shots = false;
        self
    }
}

impl Driver for Program {
    fn requires(&self) -> &Capabilities {
        &self.requires
    }

    fn provides(&self) -> &RunProvision {
        &self.provides
    }

    fn bind(&mut self, below: OpcodeMap) {
        let set = self.requires.instructions();
        self.slots = self
            .sequences
            .iter()
            .map(|sequence| set.readout_slots_of(sequence).unwrap_or(0))
            .collect();
        for instruction in self.sequences.iter_mut().flatten() {
            instruction.opcode = below.map(instruction.opcode);
        }
    }

    type Output = (Values, ShotConditions);

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor,
    ) -> Result<Vec<Self::Output>, DriveError> {
        let parts: Vec<ShotSet> = if let [_] = self.sequences.as_slice() {
            vec![shots.clone()]
        } else {
            let half = shots.len() / 2;
            [0..half, half..shots.len()]
                .into_iter()
                .map(|positions| {
                    let positions: Vec<usize> = positions.collect();
                    shots.select(&positions).expect("a half holds a shot")
                })
                .collect()
        };
        let batches: Vec<Batch<'_>> = parts
            .iter()
            .zip(self.sequences.iter().zip(&self.slots).zip(&self.checked))
            .map(|(shots, ((instructions, &slots), &checked))| {
                Batch::new(shots, instructions, slots).checked_if(checked)
            })
            .collect();
        let mut kept = Readouts::new();
        let readouts = kept.readouts_for(&batches);
        below.execute(&batches, seed, readouts).await?;
        if self.ends_its_shots {
            below.end_shots(shots);
        }
        Ok(readouts
            .iter()
            .flat_map(|rows| (0..rows.shots()).map(|i| (rows.row(i).to_vec(), rows.conditions(i))))
            .collect())
    }
}

/// A user's driver holding `driver`, which forwards every method of its
/// role but [`provides`](Driver::provides), as one written before the
/// method existed: it says it serves nothing of a run, whatever `driver`
/// serves or limits.
pub(crate) struct HidesRunProvision<D>(pub(crate) D);

impl<D: Driver> Driver for HidesRunProvision<D> {
    fn requires(&self) -> &Capabilities {
        self.0.requires()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.0.bind(below);
    }

    type Output = D::Output;

    fn ready(&mut self) -> impl Future<Output = Result<(), NotReady>> {
        self.0.ready()
    }

    fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor,
    ) -> impl Future<Output = Result<Vec<D::Output>, DriveError>> {
        self.0.drive(shots, seed, below)
    }
}
