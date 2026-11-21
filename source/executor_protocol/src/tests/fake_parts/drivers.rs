// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fake drivers.

use super::{CoinGates, Log, Values, coin_gate, coin_gates};
use crate::prelude::*;
use crate::{
    Batch, DriveError, Instruction, InstructionSet, Needs, NotReady, OpcodeMap, ReadoutValue,
    Readouts, Seed, ShotConditions, ShotSet,
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
    fn sends(&self) -> &InstructionSet {
        coin_gates()
    }

    fn bind(&mut self, below: OpcodeMap) {
        for instruction in self.sequences.iter_mut().flatten() {
            instruction.opcode = below.map(instruction.opcode);
        }
    }

    fn needs(&self) -> Needs {
        Needs::qubits(2)
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
    set: InstructionSet,
    sequences: Vec<Vec<Instruction>>,
    /// Each sequence's readout slots in `set`, counted when it is bound;
    /// 0 for one beyond `set`, which a checked batch refuses for its
    /// opcode.
    slots: Vec<u32>,
    needs: Needs,
    /// The needs it states once ready, breaking `needs`' rule.
    needs_once_ready: Option<Needs>,
    /// Whether it ends its shots below once it has their readouts.
    ends_its_shots: bool,
    /// Whether each of its batches is checked, by sequence.
    checked: Vec<bool>,
}

impl Program {
    /// The instructions `text`, each a mnemonic of [`CoinGates`] and its
    /// qubit, needing the qubits they name.
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
            set: CoinGates::set(),
            sequences: texts
                .iter()
                .map(|text| {
                    text.iter()
                        .map(|&(mnemonic, qubit)| coin_gate(mnemonic, qubit))
                        .collect()
                })
                .collect(),
            slots: Vec::new(),
            needs: Needs::qubits(qubits),
            needs_once_ready: None,
            ends_its_shots: true,
            checked: vec![false; texts.len()],
        }
    }

    /// `instructions` of `set`, as one batch for every shot, needing
    /// `needs`.
    pub(crate) fn sending(
        set: InstructionSet,
        instructions: Vec<Instruction>,
        needs: Needs,
    ) -> Self {
        Self {
            set,
            sequences: vec![instructions],
            slots: Vec::new(),
            needs,
            needs_once_ready: None,
            ends_its_shots: true,
            checked: vec![false],
        }
    }

    /// This program, stating `needs` instead.
    pub(crate) fn needing(mut self, needs: Needs) -> Self {
        self.needs = needs;
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

    /// This program, stating `needs` once ready, which a driver must not
    /// do.
    pub(crate) fn needing_once_ready(mut self, needs: Needs) -> Self {
        self.needs_once_ready = Some(needs);
        self
    }
}

impl Driver for Program {
    fn sends(&self) -> &InstructionSet {
        &self.set
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.slots = self
            .sequences
            .iter()
            .map(|sequence| self.set.readout_slots_of(sequence).unwrap_or(0))
            .collect();
        for instruction in self.sequences.iter_mut().flatten() {
            instruction.opcode = below.map(instruction.opcode);
        }
    }

    fn needs(&self) -> Needs {
        self.needs.clone()
    }

    type Output = (Values, ShotConditions);

    async fn ready(&mut self) -> Result<(), NotReady> {
        if let Some(needs) = self.needs_once_ready.take() {
            self.needs = needs;
        }
        Ok(())
    }

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
