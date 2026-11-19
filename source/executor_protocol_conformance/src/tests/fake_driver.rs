// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`FakeDriver`]: a toy driver of a few `execute`s, which keeps every
//! rule of a driver, or breaks one ([`Breaks`]).

use executor_protocol::{
    Batch, BatchReadouts, Drive, DriveError, DriverSetup, Execute, Instruction, InstructionSet,
    Needs, OpcodeMap, ReadoutValue, Readouts, Seed, ShotConditions, ShotId, ShotSet,
};

use super::{CoinGates, QUBITS, gates, needs};
use crate::trials::detection_rates;

/// The rule a [`FakeDriver`] breaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Breaks {
    /// It gives its outputs in the order of the shots' ids.
    OutputsInSortedOrder,
    /// It states one readout slot fewer than each batch of its second
    /// `execute` reads.
    UnderstatesSlots,
    /// It reports in each output only its last `execute`'s conditions.
    ForgetsEarlierConditions,
    /// It reports in each output only the latest conditions read on its
    /// shot, replacing what it held whenever a row reads some.
    KeepsTheLatestConditions,
    /// It clears an output's conditions whenever a row of its shot reads
    /// none, and merges them otherwise, as a flag reset by a clean round
    /// would.
    ClearsWhenARowReadsNone,
    /// It sends in its second `execute`, beside its batches, one of a shot
    /// not the run's, [`STRANGER`].
    SendsAShotNotItsOwn,
    /// It puts the run's first shot in both batches of its second
    /// `execute`.
    PutsAShotInTwoBatches,
    /// It merges into each output the conditions of its batch's first
    /// row, not its own.
    TakesTheFirstRowsConditions,
    /// It merges into each output the conditions of its batch's first
    /// row, beside its own.
    AddsTheFirstRowsConditions,
    /// It merges into each output the conditions of this kind read on its
    /// batch's first row, beside its own.
    AddsOneKindOfTheFirstRow(ShotConditions),
    /// It drops the last shot's output.
    DropsTheLastOutput,
    /// It drops the first shot's output when the shots do not come in the
    /// order of their ids, so each output after it sits one position early.
    DropsTheFirstOutputUnlessInOrder,
    /// It drops the last shot's output when no output holds a condition,
    /// so it gives one output per shot in each run that raises some.
    DropsTheLastOutputWhenNoneRaised,
    /// It merges into each output that read a condition the conditions of
    /// every row of its batch.
    MergesIntoRaisedOutputs,
    /// It gives the output at each position the conditions of the shot of
    /// that rank by id, its values staying the shot's own: as a driver that
    /// merges conditions in a map keyed by id and zips it with its outputs
    /// would. So it keeps the rule only when the shots come in the order
    /// of their ids.
    MergesConditionsInIdOrder,
    /// It raises its own `INCONSISTENT` on a shot where it read `REJECTED`
    /// on it below, as a driver that distrusts a rejected shot's round
    /// might.
    FlagsARejectedShot,
    /// It drops the output of each shot `REJECTED` on, as a driver that
    /// post-selects would.
    DropsRejectedShots,
    /// Run on one shot, it gives two outputs.
    GivesTwoOutputsForOneShot,
    /// Run on two shots or more, it gives one output more than its shots,
    /// while it gives one for a shot run alone.
    GivesAnExtraOutputForSeveralShots,
    /// It sends its second `execute` with a seed derived from the run's.
    ReseedsItsSecondExecute,
    /// It sends each instruction on qubits past its needs.
    SendsQubitsBeyondNeeds,
}

/// The shot a [`FakeDriver`] breaking [`Breaks::SendsAShotNotItsOwn`]
/// sends.
pub(crate) const STRANGER: u32 = 999;

/// What a [`FakeDriver`] gives for each shot: the values it read, in
/// order, and its conditions.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Record {
    pub(crate) values: Vec<ReadoutValue>,
    pub(crate) conditions: ShotConditions,
}

/// A toy driver sending [`CoinGates`] in two `execute`s: the first tosses
/// and reads a bit on every shot, in one batch; the second tosses and
/// reads another, in two batches, the shots at even positions and those
/// at odd ones. A driver of one `execute` sends the first alone; one of
/// three sends the shots at even positions alone in the second, then
/// tosses and reads a third bit on every shot; one of none sends nothing.
/// One that repeats until success sends its second `execute` only to the
/// shots whose first bit read `One`; one that skips rejected shots, only
/// to those not read `REJECTED`.
pub(crate) struct FakeDriver {
    set: InstructionSet,
    below: Option<OpcodeMap>,
    breaks: Option<Breaks>,
    checked: bool,
    executes: usize,
    /// The shot whose output it marks [`ShotConditions::INCONSISTENT`]
    /// itself, if any.
    marks: Option<ShotId>,
    /// Whether its second `execute` sends only the shots whose first bit
    /// read `One`, in one batch, and is not sent when none did.
    repeats_until_success: bool,
    /// Whether its second `execute` sends only the shots not read
    /// `REJECTED`, in one batch, and is not sent when every shot was.
    skips_rejected: bool,
}

impl FakeDriver {
    /// A driver that keeps every rule, sending checked batches.
    pub(crate) fn new() -> Self {
        let made = Self {
            set: CoinGates::set(),
            below: None,
            breaks: None,
            checked: true,
            executes: 2,
            marks: None,
            repeats_until_success: false,
            skips_rejected: false,
        };
        detection_rates::name_the_part("FakeDriver::new");
        made
    }

    /// A driver that keeps every rule, and sends its second `execute` only
    /// to the shots it did not read `REJECTED` on, as a program that stops
    /// running a rejected shot does: so in a run that rejects every shot
    /// in its first `execute`, it sends no shot again.
    pub(crate) fn skipping_rejected_shots() -> Self {
        let made = Self {
            skips_rejected: true,
            ..Self::new()
        };
        detection_rates::name_the_part("FakeDriver::skipping_rejected_shots");
        made
    }

    /// A driver that keeps every rule, and tosses again only the shots
    /// whose first toss read `One`, as a repeat-until-success driver
    /// does: so which shots it sends twice depends on what it reads, and
    /// in some runs it sends none twice.
    pub(crate) fn repeating_until_success() -> Self {
        let made = Self {
            repeats_until_success: true,
            ..Self::new()
        };
        detection_rates::name_the_part("FakeDriver::repeating_until_success");
        made
    }

    /// A driver of one `execute`, that keeps every rule.
    pub(crate) fn of_one_execute() -> Self {
        let made = Self {
            executes: 1,
            ..Self::new()
        };
        detection_rates::name_the_part("FakeDriver::of_one_execute");
        made
    }

    /// A driver that keeps every rule, and gives each shot an empty record
    /// without sending anything, as a driver that knows its outputs would.
    pub(crate) fn sending_nothing() -> Self {
        let made = Self {
            executes: 0,
            ..Self::new()
        };
        detection_rates::name_the_part("FakeDriver::sending_nothing");
        made
    }

    /// A driver that keeps every rule, and marks `shot`'s output
    /// [`ShotConditions::INCONSISTENT`] itself, as a driver that checks
    /// its outputs would.
    pub(crate) fn marking(shot: u32) -> Self {
        let made = Self {
            marks: Some(ShotId::new(shot)),
            ..Self::new()
        };
        detection_rates::name_the_part(format!("FakeDriver::marking({shot})"));
        made
    }

    /// A driver of three `execute`s, whose second skips the shots at odd
    /// positions, that breaks `breaks`.
    pub(crate) fn of_three_executes_breaking(breaks: Breaks) -> Self {
        let made = Self {
            executes: 3,
            ..Self::breaking(breaks)
        };
        detection_rates::name_the_part(format!(
            "FakeDriver::of_three_executes_breaking({breaks:?})"
        ));
        made
    }

    /// A driver that breaks `breaks`.
    pub(crate) fn breaking(breaks: Breaks) -> Self {
        let made = Self {
            breaks: Some(breaks),
            ..Self::new()
        };
        detection_rates::name_the_part(format!("FakeDriver::breaking({breaks:?})"));
        made
    }

    /// A driver sending unchecked batches, that breaks `breaks`.
    pub(crate) fn unchecked_breaking(breaks: Breaks) -> Self {
        let made = Self {
            checked: false,
            ..Self::breaking(breaks)
        };
        detection_rates::name_the_part(format!("FakeDriver::unchecked_breaking({breaks:?})"));
        made
    }

    fn breaks(&self, breaks: Breaks) -> bool {
        self.breaks == Some(breaks)
    }

    /// The shots, of `ids`, whose `records` so far it sends its second
    /// `execute` to, in one batch, if it chooses them by what it read.
    fn second_execute_to(&self, ids: &[ShotId], records: &[Record]) -> Option<Vec<u32>> {
        if self.repeats_until_success {
            Some(read_one(ids, records))
        } else if self.skips_rejected {
            Some(not_rejected(ids, records))
        } else {
            None
        }
    }

    /// The conditions it holds for a shot that held `held` and read `read`
    /// in a row: their union, unless it breaks how it merges them.
    fn merged(&self, held: ShotConditions, read: ShotConditions) -> ShotConditions {
        match self.breaks {
            Some(Breaks::KeepsTheLatestConditions) if !read.is_empty() => read,
            Some(Breaks::KeepsTheLatestConditions) => held,
            Some(Breaks::ClearsWhenARowReadsNone) if read.is_empty() => ShotConditions::NONE,
            _ => held | read,
        }
    }

    /// `instructions`, in the receiver's opcodes.
    fn mapped(&self, instructions: &[Instruction]) -> Vec<Instruction> {
        let map = self.below.as_ref().expect("`>>` binds before any run");
        instructions
            .iter()
            .map(|instruction| {
                let mut mapped = instruction.clone();
                mapped.opcode = map.map(instruction.opcode);
                mapped
            })
            .collect()
    }
}

impl DriverSetup for FakeDriver {
    fn sends(&self) -> &InstructionSet {
        &self.set
    }

    fn needs(&self) -> Needs {
        needs()
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.below = Some(below);
    }
}

impl Drive for FakeDriver {
    type Output = Record;

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
    ) -> Result<Vec<Record>, DriveError> {
        let ids: Vec<ShotId> = shots.iter().collect();
        let mut records: Vec<Record> = ids
            .iter()
            .map(|_| Record {
                values: Vec::new(),
                conditions: ShotConditions::NONE,
            })
            .collect();
        let beyond = if self.breaks(Breaks::SendsQubitsBeyondNeeds) {
            QUBITS
        } else {
            0
        };
        let toss = |qubit| {
            let qubit = qubit + beyond;
            self.mapped(&gates(&[("h", qubit), ("mz", qubit)]))
        };
        let (first, second, third) = (toss(0), toss(1), toss(2));
        let even = ShotSet::ids(ids.iter().step_by(2).map(|shot| shot.get()));
        let mut odd: Vec<u32> = ids
            .iter()
            .skip(1)
            .step_by(2)
            .map(|shot| shot.get())
            .collect();
        if self.breaks(Breaks::PutsAShotInTwoBatches) {
            odd.push(ids[0].get());
        }
        if self.executes == 3 {
            odd.clear();
        }
        let odd = (!odd.is_empty()).then(|| ShotSet::ids(odd));
        let stranger = ShotSet::ids([STRANGER]);
        let readout_slots = 1 - u32::from(self.breaks(Breaks::UnderstatesSlots));
        let executes: [Vec<(&ShotSet, &[Instruction], u32)>; 3] = [
            vec![(shots, &first, 1)],
            [Some(&even), odd.as_ref()]
                .into_iter()
                .flatten()
                .chain(
                    self.breaks(Breaks::SendsAShotNotItsOwn)
                        .then_some(&stranger),
                )
                .map(|shots| (shots, second.as_slice(), readout_slots))
                .collect(),
            vec![(shots, &third, 1)],
        ];
        let mut readouts = Readouts::new();
        for (execute, batches) in executes.into_iter().take(self.executes).enumerate() {
            let again = (execute == 1)
                .then(|| self.second_execute_to(&ids, &records))
                .flatten();
            if again.as_ref().is_some_and(Vec::is_empty) {
                break;
            }
            let again = again.map(ShotSet::ids);
            let batches = match &again {
                Some(again) => vec![(again, second.as_slice(), readout_slots)],
                None => batches,
            };
            let batches: Vec<Batch<'_>> = batches
                .into_iter()
                .map(|(shots, instructions, slots)| {
                    Batch::new(shots, instructions, slots).checked_if(self.checked)
                })
                .collect();
            let answered = readouts.readouts_for(&batches);
            let seed = if execute == 1 && self.breaks(Breaks::ReseedsItsSecondExecute) {
                seed.derive("program")
            } else {
                seed
            };
            below.execute(&batches, seed, answered).await?;
            if self.breaks(Breaks::ForgetsEarlierConditions) {
                for record in &mut records {
                    record.conditions = ShotConditions::NONE;
                }
            }
            for (batch, answered) in batches.iter().zip(answered.iter()) {
                for (position, shot) in batch.shots.iter().enumerate() {
                    let Some(index) = ids.iter().position(|id| *id == shot) else {
                        continue;
                    };
                    let record = &mut records[index];
                    record.values.extend_from_slice(answered.row(position));
                    let conditions = self.conditions_read(answered, position);
                    record.conditions = self.merged(record.conditions, conditions);
                }
            }
        }
        Ok(self.outputs(ids, records))
    }
}

impl FakeDriver {
    /// The conditions it takes for the shot at `position` of a batch whose
    /// readouts are `answered`: the row's own, or others if it breaks so.
    fn conditions_read(&self, answered: &BatchReadouts, position: usize) -> ShotConditions {
        let row = if self.breaks(Breaks::TakesTheFirstRowsConditions) {
            0
        } else {
            position
        };
        let mut conditions = answered.conditions(row);
        if self.breaks(Breaks::MergesIntoRaisedOutputs) && !conditions.is_empty() {
            for other in 0..answered.shots() {
                conditions |= answered.conditions(other);
            }
        }
        if self.breaks(Breaks::AddsTheFirstRowsConditions) {
            conditions |= answered.conditions(0);
        }
        if let Some(Breaks::AddsOneKindOfTheFirstRow(kind)) = self.breaks
            && answered.conditions(0).contains(kind)
        {
            conditions |= kind;
        }
        if self.breaks(Breaks::FlagsARejectedShot) && conditions.contains(ShotConditions::REJECTED)
        {
            conditions |= ShotConditions::INCONSISTENT;
        }
        conditions
    }

    /// The outputs it gives for `ids`, from the `records` it built: marked,
    /// reordered, dropped or doubled if it does so.
    fn outputs(&self, ids: Vec<ShotId>, mut records: Vec<Record>) -> Vec<Record> {
        let in_order = ids.is_sorted();
        if let Some(index) = self
            .marks
            .and_then(|marked| ids.iter().position(|id| *id == marked))
        {
            records[index].conditions |= ShotConditions::INCONSISTENT;
        }
        if self.breaks(Breaks::MergesConditionsInIdOrder) {
            let mut by_id: Vec<(ShotId, ShotConditions)> = ids
                .iter()
                .copied()
                .zip(records.iter().map(|record| record.conditions))
                .collect();
            by_id.sort_by_key(|(shot, _)| *shot);
            for (record, (_, conditions)) in records.iter_mut().zip(by_id) {
                record.conditions = conditions;
            }
        }
        if self.breaks(Breaks::OutputsInSortedOrder) {
            let mut sorted: Vec<(ShotId, Record)> = ids.into_iter().zip(records).collect();
            sorted.sort_by_key(|(shot, _)| *shot);
            records = sorted.into_iter().map(|(_, record)| record).collect();
        }
        let none_raised = records.iter().all(|record| record.conditions.is_empty());
        if self.breaks(Breaks::DropsTheLastOutput)
            || self.breaks(Breaks::DropsTheLastOutputWhenNoneRaised) && none_raised
        {
            records.pop();
        }
        if self.breaks(Breaks::DropsTheFirstOutputUnlessInOrder) && !in_order {
            records.remove(0);
        }
        if self.breaks(Breaks::DropsRejectedShots) {
            records.retain(|record| !record.conditions.contains(ShotConditions::REJECTED));
        }
        if self.breaks(Breaks::GivesTwoOutputsForOneShot) && records.len() == 1 {
            records.push(records[0].clone());
        }
        if self.breaks(Breaks::GivesAnExtraOutputForSeveralShots) && records.len() >= 2 {
            records.push(records[records.len() - 1].clone());
        }
        records
    }
}

/// The shots, of `ids`, whose `records` read `One` first.
fn read_one(ids: &[ShotId], records: &[Record]) -> Vec<u32> {
    ids.iter()
        .zip(records)
        .filter(|(_, record)| record.values.first() == Some(&ReadoutValue::One))
        .map(|(shot, _)| shot.get())
        .collect()
}

/// The shots, of `ids`, whose `records` do not hold `REJECTED`.
fn not_rejected(ids: &[ShotId], records: &[Record]) -> Vec<u32> {
    ids.iter()
        .zip(records)
        .filter(|(_, record)| !record.conditions.contains(ShotConditions::REJECTED))
        .map(|(shot, _)| shot.get())
        .collect()
}
