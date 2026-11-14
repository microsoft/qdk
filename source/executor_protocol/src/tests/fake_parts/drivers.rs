// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fake drivers.

use super::{CoinGates, Values, coin_gate, coin_gates};
use crate::{
    Batch, Drive, DriveError, Execute, Instruction, InstructionSet, Needs, OpcodeMap, ReadoutValue,
    Seed, ShotConditions, ShotSet,
};

/// A coin tossed on qubit 0, copied onto qubit 1: a toss for every shot in
/// one batch, then, by the coin, a flip and a read, or a read, as two
/// batches of one `execute`. Each shot's output is its two values.
pub(crate) struct CoinCopy {
    sequences: [Vec<Instruction>; 3],
}

impl CoinCopy {
    pub(crate) fn new() -> Self {
        Self {
            sequences: [
                vec![coin_gate("h", 0), coin_gate("mz", 0)],
                vec![coin_gate("x", 1), coin_gate("mz", 1)],
                vec![coin_gate("mz", 1)],
            ],
        }
    }
}

impl Drive for CoinCopy {
    type Output = Values;

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

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
    ) -> Result<Vec<Values>, DriveError> {
        let tossed = below
            .execute(
                &[Batch {
                    shots,
                    instructions: &self.sequences[0],
                }],
                seed,
            )
            .await?;
        let coin = &tossed[0];
        let mut outputs: Vec<Values> = (0..shots.len()).map(|i| coin.row(i).to_vec()).collect();
        let at = |value: ReadoutValue| -> Vec<usize> {
            (0..shots.len())
                .filter(|&i| coin.row(i) == [value])
                .collect()
        };
        let (heads, tails) = (at(ReadoutValue::One), at(ReadoutValue::Zero));
        let sets: Vec<(&[usize], ShotSet, &[Instruction])> =
            [(&heads, &self.sequences[1]), (&tails, &self.sequences[2])]
                .into_iter()
                .filter_map(|(positions, sequence)| {
                    Some((
                        positions.as_slice(),
                        shots.select(positions)?,
                        sequence.as_slice(),
                    ))
                })
                .collect();
        let batches: Vec<Batch<'_>> = sets
            .iter()
            .map(|(_, shots, instructions)| Batch {
                shots,
                instructions,
            })
            .collect();
        let read = below.execute(&batches, seed).await?;
        for ((positions, _, _), rows) in sets.iter().zip(&read) {
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
/// batches of one `execute`; each shot's output is its row and its
/// conditions.
pub(crate) struct Program {
    set: InstructionSet,
    sequences: Vec<Vec<Instruction>>,
    needs: Needs,
    /// The needs it states once provisioned, breaking `needs`' rule.
    needs_once_provisioned: Option<Needs>,
    /// Whether it ends its shots below once it has their readouts.
    ends_its_shots: bool,
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
            needs: Needs::qubits(qubits),
            needs_once_provisioned: None,
            ends_its_shots: true,
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
            needs,
            needs_once_provisioned: None,
            ends_its_shots: true,
        }
    }

    /// This program, stating `needs` instead.
    pub(crate) fn needing(mut self, needs: Needs) -> Self {
        self.needs = needs;
        self
    }

    /// This program, never ending its shots below, which it leaves to the
    /// runner.
    pub(crate) fn not_ending(mut self) -> Self {
        self.ends_its_shots = false;
        self
    }

    /// This program, stating `needs` once provisioned, which a driver must
    /// not do.
    pub(crate) fn needing_once_provisioned(mut self, needs: Needs) -> Self {
        self.needs_once_provisioned = Some(needs);
        self
    }
}

impl Drive for Program {
    type Output = (Values, ShotConditions);

    fn sends(&self) -> &InstructionSet {
        &self.set
    }

    fn bind(&mut self, below: OpcodeMap) {
        for instruction in self.sequences.iter_mut().flatten() {
            instruction.opcode = below.map(instruction.opcode);
        }
    }

    fn needs(&self) -> Needs {
        self.needs.clone()
    }

    fn provision(&mut self) {
        if let Some(needs) = self.needs_once_provisioned.take() {
            self.needs = needs;
        }
    }

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
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
            .zip(&self.sequences)
            .map(|(shots, instructions)| Batch {
                shots,
                instructions,
            })
            .collect();
        let readouts = below.execute(&batches, seed).await?;
        if self.ends_its_shots {
            below.end_shots(shots);
        }
        Ok(readouts
            .iter()
            .flat_map(|rows| (0..rows.shots()).map(|i| (rows.row(i).to_vec(), rows.conditions(i))))
            .collect())
    }
}
