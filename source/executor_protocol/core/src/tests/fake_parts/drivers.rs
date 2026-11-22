// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fake drivers.

use super::{BEYOND_THE_SET, Log, Values, batches_of, capabilities, coin_gate, coin_gates};
use crate::prelude::*;
use crate::tests::example_sets::{Gate, Measuring};
use crate::{
    Batch, Binding, Capabilities, DriveError, Features, NotReady, ReadoutValue, Readouts,
    Resources, RunRequirement, Seed, ShotConditions, ShotSet,
};

/// A coin tossed on qubit 0, copied onto qubit 1: a toss for every shot in
/// one batch, then, by the coin, a flip and a read, or a read, as two
/// batches of one `execute`. Each shot's output is its two values.
pub(crate) struct CoinCopy {
    sequences: [Vec<Gate>; 3],
    /// Each sequence's readout slots.
    slots: [u32; 3],
    /// Where it logs that it gets ready, and for what, if anywhere.
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
        let slots = sequences
            .each_ref()
            .map(|sequence| slots_of(coin_gates(), sequence));
        Self {
            sequences,
            slots,
            log: None,
            busy: None,
        }
    }

    /// This driver, logging to `log` when it gets ready, as
    /// [`ready_for`](Self::ready_for) the run it gets.
    pub(crate) fn logging(self, log: &Log) -> Self {
        Self {
            log: Some(log.clone()),
            ..self
        }
    }

    /// What it logs when it gets ready for a run that requires `run`.
    pub(crate) fn ready_for(run: &RunRequirement) -> String {
        format!("CoinCopy ready for {run:?}")
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
    type Sends = Measuring;

    fn sends(&self) -> &Measuring {
        coin_gates()
    }

    /// The run's, in batches of 2 instructions, its longest.
    fn requires(&self, run: &RunRequirement) -> Capabilities<Measuring> {
        Capabilities::from_run(run)
            .map_resources(|resources| resources.with_instructions_per_batch(2))
    }

    fn bind(&mut self, below: Vec<u32>) {
        for gate in self.sequences.iter_mut().flatten() {
            *gate = Measuring::bound(&below, *gate);
        }
    }

    type Output = Values;

    async fn ready(&mut self, run: &RunRequirement) -> Result<(), NotReady> {
        if let Some(log) = &self.log {
            log.push(Self::ready_for(run));
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
        below: &mut impl Executor<Accepts = Measuring>,
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
        let sets: Vec<(&[usize], ShotSet, &[Gate], u32)> = [
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
        let batches: Vec<Batch<'_, Measuring>> = sets
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

/// `D`, keeping the default `requires`: it passes the run below as it
/// arrives, stating nothing of its own; it does as `D` does otherwise.
pub(crate) struct PassesTheRun<D>(pub(crate) D);

impl<D: Driver> Driver for PassesTheRun<D> {
    type Sends = D::Sends;

    fn sends(&self) -> &D::Sends {
        self.0.sends()
    }

    fn bind(&mut self, below: Binding<D::Sends>) {
        self.0.bind(below);
    }

    type Output = D::Output;

    fn ready(&mut self, run: &RunRequirement) -> impl Future<Output = Result<(), NotReady>> {
        self.0.ready(run)
    }

    fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor<Accepts = D::Sends>,
    ) -> impl Future<Output = Result<Vec<D::Output>, DriveError>> {
        self.0.drive(shots, seed, below)
    }
}

/// A driver that sends its instructions to every shot, as one batch of one
/// `execute`, or its two sequences to each half of the shots, as two
/// batches of one `execute`, unchecked unless it is told; each shot's
/// output is its row and its conditions.
pub(crate) struct Program {
    sequences: Vec<Vec<Gate>>,
    /// Each sequence's readout slots in the set it sends, counted when it
    /// is bound.
    slots: Vec<u32>,
    /// The sequence it ends with [`BEYOND_THE_SET`] once bound, if any.
    beyond_its_set: Option<usize>,
    sends: Measuring,
    /// What it requires besides the run's shots and seeded runs.
    resources: Resources,
    features: Features,
    /// Whether it ends its shots below once it has their readouts.
    ends_its_shots: bool,
    /// Whether each of its batches is checked, by sequence.
    checked: Vec<bool>,
}

impl Program {
    /// The instructions `text`, each a gate of [`coin_gates`] and its
    /// qubit, requiring batches of as many instructions.
    pub(crate) fn new(text: &[(&str, u32)]) -> Self {
        Self::of(&[text])
    }

    /// `first` on the first half of the shots, rounded down, and `second`
    /// on the others, as two batches of one `execute`.
    pub(crate) fn halves(first: &[(&str, u32)], second: &[(&str, u32)]) -> Self {
        Self::of(&[first, second])
    }

    fn of(texts: &[&[(&str, u32)]]) -> Self {
        let longest = texts.iter().map(|text| text.len()).max().unwrap_or(0);
        Self {
            sequences: texts
                .iter()
                .map(|text| {
                    text.iter()
                        .map(|&(name, qubit)| coin_gate(name, qubit))
                        .collect()
                })
                .collect(),
            slots: Vec::new(),
            beyond_its_set: None,
            sends: *coin_gates(),
            resources: batches_of(u32::try_from(longest).expect("a short program")),
            features: Features::unconstrained(),
            ends_its_shots: true,
            checked: vec![false; texts.len()],
        }
    }

    /// This program, requiring `resources` in place of its own.
    pub(crate) fn with_resources(mut self, resources: Resources) -> Self {
        self.resources = resources;
        self
    }

    /// This program, requiring `features` in place of its own.
    pub(crate) fn with_features(mut self, features: Features) -> Self {
        self.features = features;
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

    /// This program, its sequence `sequence` (0 for the first half)
    /// ending with [`BEYOND_THE_SET`], as a faulty driver's would.
    pub(crate) fn ending_beyond_its_set(mut self, sequence: usize) -> Self {
        assert!(sequence < self.sequences.len(), "a sequence of the program");
        self.beyond_its_set = Some(sequence);
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
    type Sends = Measuring;

    fn sends(&self) -> &Measuring {
        &self.sends
    }

    /// Its own resources and features, with the run's shots, and seeded
    /// runs if the run's seed is fixed.
    fn requires(&self, run: &RunRequirement) -> Capabilities<Measuring> {
        let mut resources = self.resources;
        if let Some(shots) = run.resources.shots {
            resources = resources.with_shots(shots);
        }
        let mut features = self.features;
        if run.features.seeded_runs == Some(true) {
            features = features.with_seeded_runs(true);
        }
        capabilities(resources, features)
    }

    fn bind(&mut self, below: Vec<u32>) {
        let set = &self.sends;
        self.slots = (self.sequences.iter())
            .map(|sequence| slots_of(set, sequence))
            .collect();
        for gate in self.sequences.iter_mut().flatten() {
            *gate = Measuring::bound(&below, *gate);
        }
        if let Some(sequence) = self.beyond_its_set {
            self.sequences[sequence].push(BEYOND_THE_SET);
        }
    }

    type Output = (Values, ShotConditions);

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor<Accepts = Measuring>,
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
        let batches: Vec<Batch<'_, Measuring>> = parts
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

/// The readout slots of `sequence` in `set`.
fn slots_of(set: &Measuring, sequence: &[Gate]) -> u32 {
    sequence.iter().map(|gate| set.readout_slots(gate)).sum()
}
