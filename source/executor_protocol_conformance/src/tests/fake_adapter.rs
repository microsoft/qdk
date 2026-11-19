// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`FakeAdapter`]: a toy adapter that sends each batch below as it got it,
//! and answers with the readouts and the conditions read below, flipped by
//! each shot's noise if it is noisy; it keeps every rule of an adapter, or
//! breaks one ([`Breaks`]).

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::pin::pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use executor_protocol::{
    Adapt, AdapterSetup, Batch, BatchReadouts, Execute, ExecuteError, ExecuteRefusal, Instruction,
    InstructionSet, Offer, OpcodeMap, ReadoutValue, Readouts, Seed, ShotConditions, ShotId,
    ShotSet,
};

use super::fake_executor::{CoinGatesAndReset, Draws};
use super::{CoinGates, QUBITS};
use crate::shared_run::every_kind;
use crate::splitmix64::SplitMix64;
use crate::trials::detection_rates;

/// The rule a [`FakeAdapter`] breaks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Breaks {
    /// It states one readout slot more below than each batch's.
    OverstatesSlotsBelow,
    /// It leaves the last readout slot of shot 1's rows unwritten.
    LeavesASlotUnwritten,
    /// It drops the conditions read below.
    DropsConditions,
    /// Of the conditions read below on a row in its calls of one
    /// `execute`, it keeps only the last call's.
    KeepsTheLastCallsConditions,
    /// Of the conditions read below on a row in its calls of one
    /// `execute`, it keeps only the first call's.
    KeepsTheFirstCallsConditions,
    /// Of the conditions read below on a row in its calls of one
    /// `execute`, it keeps only the first two calls'.
    KeepsTheFirstTwoCallsConditions,
    /// Of the conditions read below on a row in its calls of one
    /// `execute`, it clears those of the earlier calls whenever a call
    /// reads none, as a flag reset by a clean call would.
    ClearsWhenACallReadsNone,
    /// Of the conditions read below, it passes up `REJECTED` only.
    KeepsOnlyRejected,
    /// Of the conditions read below on a row in one call, it passes up
    /// the first of `REJECTED`, `INCONSISTENT` and `UNRESOLVED` it holds,
    /// as a status of one value per shot would.
    KeepsTheWorstKind,
    /// Of the conditions read below on a row in one call, it passes up
    /// every kind if it holds all three, and otherwise the first of
    /// `REJECTED`, `INCONSISTENT` and `UNRESOLVED` it holds, as a status
    /// with one value per kind and one for all would.
    KeepsOneKindUnlessEvery,
    /// It reports in each row the conditions read below on the row two
    /// further in its batch, wrapping around.
    GivesAnotherRowsConditions,
    /// It adds to each row the conditions read below on the row two
    /// further in its batch, wrapping around.
    AddsTheConditionsOfTheRowTwoFurther,
    /// It adds to each row this kind, if read below on the next row of its
    /// batch, wrapping around.
    AddsOneKindOfTheNextRow(ShotConditions),
    /// It reports in each row the conditions read below on every row of
    /// its batch.
    MergesItsBatchsConditions,
    /// It reports in each row every condition read on its shot so far.
    CarriesConditionsAcrossExecutes,
    /// It reports in each row every `INCONSISTENT` and `UNRESOLVED` read
    /// on its shot so far, as a decoder keeping a flag per shot would.
    CarriesInconsistentAndUnresolved,
    /// It reports in each row this kind, if read on its shot so far.
    CarriesOneKind(ShotConditions),
    /// It reports in each row every condition read on its shot so far,
    /// from its second `execute` sent below on, as a decoder that clears
    /// its flags after the first round only would.
    CarriesConditionsFromItsSecondExecute,
    /// It reports in each row the conditions read on its shot in its first
    /// `execute` sent below, as a decoder that keeps its first round's
    /// flags would.
    CarriesConditionsOfItsFirstExecute,
    /// It reports `INCONSISTENT` in each later row of a shot it read
    /// `INCONSISTENT` on without `REJECTED`, as a decoder that stops
    /// tracking rejected shots would.
    CarriesInconsistentUnlessRejected,
    /// It raises its own `INCONSISTENT` on a shot in each `execute` after
    /// one where it read `REJECTED` on it below, as a decoder that no
    /// longer trusts a rejected shot's rounds might.
    FlagsLaterRoundsOfARejectedShot,
    /// It raises its own `INCONSISTENT` on a shot's row where it read
    /// `REJECTED` on it below, as a decoder that distrusts a rejected
    /// shot's round might.
    FlagsARejectedShot,
    /// It merges into each row that read a condition below every
    /// condition read on any row of its batch.
    MergesIntoRaisedRows,
    /// It adds to a row that read a condition below those read on the row
    /// two further.
    AddsTwoFurtherIfRaised,
    /// It sends its batches below unchecked.
    UnchecksBelow,
    /// It answers each batch's rows in reverse order.
    SwapsRows,
    /// It sends below, beside each `execute`'s batches, one of a shot it
    /// did not get, [`STRANGER`].
    SendsAShotItDidNotGet,
    /// It sends below, beside each `execute`'s batches, one of the shots
    /// it got in earlier `execute`s that this one does not hold.
    SendsShotsOfEarlierExecutes,
    /// It sends below, after each `execute`'s batches, the first again.
    PutsAShotInTwoBatchesBelow,
    /// It sends below a seed derived from the one it got.
    ReseedsBelow,
    /// It makes each shot's noise again at each `execute`.
    RestartsItsNoise,
    /// Its shots draw their noise from one stream per `execute`, shared,
    /// so a shot's noise depends on the shots before it.
    SharesItsNoise,
    /// Each `execute` draws the noise of every shot it holds that the
    /// `execute` does not name.
    DrawsNoiseOfOtherShots,
    /// It draws an `execute`'s noise when called, before its future is
    /// polled.
    DrawsNoiseWhenCalled,
    /// It relays an `execute` when called, and if that call's future is
    /// dropped unpolled, answers the next call with nothing written,
    /// taking the dropped call's work for it.
    RelaysWhenCalledAndReuses,
    /// Once it ended a shot, it refuses every later `execute`.
    RefusesAfterEndingShots,
    /// It ignores `end_shots`, keeping each shot's noise past its end.
    KeepsEndedNoise,
    /// It panics in `end_shots` on a shot it did not get, or ended, as a
    /// part taking each ended shot out of a table with `expect` would.
    PanicsEndingShotsItDoesNotHold,
    /// It sends each instruction below on qubits past the shot's needs.
    SendsQubitsBeyondNeeds,
    /// It sends below, for each instruction, `reset`, which the part below
    /// does not accept.
    SendsAnInstructionNotAcceptedBelow,
}

/// What a [`FakeAdapter`] does that breaks no rule, though a check meets
/// it: a run it fails. Set by its constructors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Allowed {
    /// It panics in every `end_shots`.
    PanicsEndingShots,
}

/// The shot a [`FakeAdapter`] breaking [`Breaks::SendsAShotItDidNotGet`]
/// sends below.
pub(crate) const STRANGER: u32 = 999;

/// Each shot's noise, by shot: an adapter's own, or that of a device that
/// several adapters share.
pub(crate) type Noise = Rc<RefCell<BTreeMap<ShotId, SplitMix64>>>;

/// A toy adapter accepting and sending [`CoinGates`]. A noisy one flips
/// each readout value it passes up by a draw from its shot's noise, a
/// stream made at the shot's first batch and kept until its end, one draw
/// per instruction: so its readouts depend on each shot's state, as an
/// executor's do.
pub(crate) struct FakeAdapter {
    set: InstructionSet,
    below: Option<OpcodeMap>,
    breaks: Option<Breaks>,
    noise: Option<Noise>,
    /// How its shots draw their noise, which sets whether it offers them
    /// replayable.
    draws: Draws,
    /// Each shot's conditions so far, for
    /// [`Breaks::CarriesConditionsAcrossExecutes`].
    carried: BTreeMap<ShotId, ShotConditions>,
    /// Whether a call's work is done and its future was dropped unpolled,
    /// for [`Breaks::RelaysWhenCalledAndReuses`]: set at the call, cleared
    /// when its future is polled.
    pending: Rc<Cell<bool>>,
    ended: bool,
    /// The calls in which it sends each `execute` below: one, or each
    /// batch's instructions split among two or three.
    calls: usize,
    /// Whether it sends each batch below in a call of its own.
    batches_apart: bool,
    /// The `execute`s it answers itself, before it sends any below.
    answers_alone: usize,
    /// The `execute`s it sent below so far.
    relayed: usize,
    executes: usize,
    /// The shot on which it raises a condition of its own in every
    /// `execute`, if any.
    marks: Option<ShotId>,
    /// Whether it stops raising its own condition on a shot once it read
    /// `REJECTED` on it below.
    unmarks_rejected: bool,
    /// The shots it read `REJECTED` on below, until their end.
    rejected: BTreeSet<ShotId>,
    /// The shots it got so far, until their end.
    got: BTreeSet<ShotId>,
    /// Where it keeps the seed of its first `execute`, shared by the
    /// adapters of one test, if it sends that seed below for every later
    /// one.
    first_seed: Option<Rc<Cell<Option<Seed>>>>,
    /// What it does that breaks no rule, if anything: set by its
    /// constructors.
    allowed: Option<Allowed>,
}

impl FakeAdapter {
    /// An adapter that keeps every rule, without noise.
    pub(crate) fn new() -> Self {
        let made = Self {
            set: CoinGates::set(),
            below: None,
            breaks: None,
            noise: None,
            draws: Draws::ShotByShot,
            carried: BTreeMap::new(),
            pending: Rc::default(),
            ended: false,
            calls: 1,
            batches_apart: false,
            answers_alone: 0,
            relayed: 0,
            executes: 0,
            marks: None,
            unmarks_rejected: false,
            rejected: BTreeSet::new(),
            got: BTreeSet::new(),
            first_seed: None,
            allowed: None,
        };
        detection_rates::name_the_part("FakeAdapter::new");
        made
    }

    /// An adapter without noise that sends below, in every `execute`, the
    /// seed of the first `execute` any adapter sharing `first_seed` got, as
    /// an adapter whose generator is seeded once, at its first use, would.
    pub(crate) fn keeping_its_first_seed(first_seed: &Rc<Cell<Option<Seed>>>) -> Self {
        let made = Self {
            first_seed: Some(first_seed.clone()),
            ..Self::new()
        };
        detection_rates::name_the_part("FakeAdapter::keeping_its_first_seed");
        made
    }

    /// An adapter without noise that keeps every rule, sending each `execute`
    /// below in two calls: the first half of each batch's instructions,
    /// then the rest, as an adapter that sends a measurement apart would.
    pub(crate) fn splitting() -> Self {
        let made = Self {
            calls: 2,
            ..Self::new()
        };
        detection_rates::name_the_part("FakeAdapter::splitting");
        made
    }

    /// An adapter without noise, sending each `execute` below in two calls,
    /// that breaks `breaks`.
    pub(crate) fn splitting_breaking(breaks: Breaks) -> Self {
        let made = Self {
            breaks: Some(breaks),
            ..Self::splitting()
        };
        detection_rates::name_the_part(format!("FakeAdapter::splitting_breaking({breaks:?})"));
        made
    }

    /// An adapter without noise that keeps every rule, sending each `execute`
    /// below in three calls, each with a third of each batch's
    /// instructions (a call left with none is not sent).
    pub(crate) fn in_three_calls() -> Self {
        let made = Self {
            calls: 3,
            ..Self::new()
        };
        detection_rates::name_the_part("FakeAdapter::in_three_calls");
        made
    }

    /// An adapter without noise, sending each `execute` below in three calls,
    /// that breaks `breaks`.
    pub(crate) fn in_three_calls_breaking(breaks: Breaks) -> Self {
        let made = Self {
            breaks: Some(breaks),
            ..Self::in_three_calls()
        };
        detection_rates::name_the_part(format!("FakeAdapter::in_three_calls_breaking({breaks:?})"));
        made
    }

    /// An adapter without noise that keeps every rule, sending each batch of
    /// an `execute` below in a call of its own, as an adapter that sends
    /// each batch to another device would.
    pub(crate) fn sending_batches_apart() -> Self {
        let made = Self {
            batches_apart: true,
            ..Self::new()
        };
        detection_rates::name_the_part("FakeAdapter::sending_batches_apart");
        made
    }

    /// An adapter without noise that keeps every rule, and raises
    /// [`ShotConditions::UNRESOLVED`] of its own on `shot` in every
    /// `execute`, as a noise model that marks a shot would.
    pub(crate) fn marking(shot: u32) -> Self {
        let made = Self {
            marks: Some(ShotId::new(shot)),
            ..Self::new()
        };
        detection_rates::name_the_part(format!("FakeAdapter::marking({shot})"));
        made
    }

    /// An adapter without noise that breaks `breaks`, and raises
    /// [`ShotConditions::UNRESOLVED`] of its own on `shot` in every
    /// `execute`.
    pub(crate) fn marking_breaking(shot: u32, breaks: Breaks) -> Self {
        let made = Self {
            breaks: Some(breaks),
            ..Self::marking(shot)
        };
        detection_rates::name_the_part(format!(
            "FakeAdapter::marking_breaking({shot}, {breaks:?})"
        ));
        made
    }

    /// An adapter without noise that keeps every rule, and raises
    /// [`ShotConditions::UNRESOLVED`] of its own on `shot` in every
    /// `execute` until it reads `REJECTED` on it below, as a decoder that
    /// stops decoding a rejected shot would.
    pub(crate) fn marking_until_rejected(shot: u32) -> Self {
        let made = Self {
            unmarks_rejected: true,
            ..Self::marking(shot)
        };
        detection_rates::name_the_part(format!("FakeAdapter::marking_until_rejected({shot})"));
        made
    }

    /// An adapter without noise that keeps every rule, answering its first
    /// `executes` itself, reading zero on every slot and sending nothing
    /// below, as an adapter that knows the answer would.
    pub(crate) fn answering_alone(executes: usize) -> Self {
        let made = Self {
            answers_alone: executes,
            ..Self::new()
        };
        detection_rates::name_the_part(format!("FakeAdapter::answering_alone({executes})"));
        made
    }

    /// A noisy adapter that keeps every rule.
    pub(crate) fn noisy() -> Self {
        let made = Self::sharing_noise(&Noise::default());
        detection_rates::name_the_part("FakeAdapter::noisy");
        made
    }

    /// A noisy adapter holding its shots' noise in `noise`, which keeps a
    /// shot no adapter ended: a device reset only at some point.
    pub(crate) fn sharing_noise(noise: &Noise) -> Self {
        let made = Self {
            noise: Some(noise.clone()),
            ..Self::new()
        };
        detection_rates::name_the_part("FakeAdapter::sharing_noise");
        made
    }

    /// A noisy adapter whose shots draw their noise from one stream per
    /// `execute`, shared, as those breaking [`Breaks::SharesItsNoise`] do,
    /// and that offers shots that are not replayable above whatever the
    /// part below offers, as a noise model drawing a run's shots in one
    /// call does: a shot's records depend on the shots run beside it, which
    /// the protocol allows such a part.
    pub(crate) fn noisy_drawing_shots_together() -> Self {
        let made = Self {
            draws: Draws::Together,
            ..Self::noisy()
        };
        detection_rates::name_the_part("FakeAdapter::noisy_drawing_shots_together");
        made
    }

    /// An adapter without noise that panics in every `end_shots`, as a
    /// part whose release of a shot's state fails would: a fault of its
    /// own, not of the ends it is sent, which fails the run and breaks no
    /// rule the kit checks.
    pub(crate) fn panicking_ending_shots() -> Self {
        let made = Self {
            allowed: Some(Allowed::PanicsEndingShots),
            ..Self::new()
        };
        detection_rates::name_the_part("FakeAdapter::panicking_ending_shots");
        made
    }

    /// An adapter without noise that breaks `breaks`.
    pub(crate) fn breaking(breaks: Breaks) -> Self {
        let made = Self {
            breaks: Some(breaks),
            ..Self::new()
        };
        detection_rates::name_the_part(format!("FakeAdapter::breaking({breaks:?})"));
        made
    }

    /// A noisy adapter that breaks `breaks`.
    pub(crate) fn noisy_breaking(breaks: Breaks) -> Self {
        let made = Self {
            breaks: Some(breaks),
            ..Self::noisy()
        };
        detection_rates::name_the_part(format!("FakeAdapter::noisy_breaking({breaks:?})"));
        made
    }

    fn breaks(&self, breaks: Breaks) -> bool {
        self.breaks == Some(breaks)
    }

    /// The noise of `batches`, drawn from each shot's stream: one draw per
    /// instruction, by batch and by row; none without noise.
    /// Relays an `execute` at once, for
    /// [`Breaks::RelaysWhenCalledAndReuses`]: the part below, the kit's,
    /// answers when first polled.
    fn relay_now(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        let noise = self.draw_noise(batches, seed);
        let done = {
            let relayed = pin!(self.relay(batches, seed, below, readouts, &noise));
            match relayed.poll(&mut Context::from_waker(Waker::noop())) {
                Poll::Ready(done) => done,
                Poll::Pending => panic!("the part below answers when first polled"),
            }
        };
        self.relayed += 1;
        done
    }

    fn draw_noise(&mut self, batches: &[Batch<'_>], seed: Seed) -> Vec<Vec<Vec<u64>>> {
        let Some(noise) = &self.noise else {
            return Vec::new();
        };
        let mut streams = noise.borrow_mut();
        if self.breaks(Breaks::RestartsItsNoise) {
            streams.clear();
        }
        if self.breaks(Breaks::DrawsNoiseOfOtherShots) {
            let named: BTreeSet<ShotId> = batches
                .iter()
                .flat_map(|batch| batch.shots.iter())
                .collect();
            for (shot, stream) in streams.iter_mut() {
                if !named.contains(shot) {
                    stream.next_u64();
                }
            }
        }
        let seed = seed.derive("noise");
        let together = self.draws == Draws::Together || self.breaks(Breaks::SharesItsNoise);
        let mut shared = SplitMix64::new(seed.shot(ShotId::new(0)));
        batches
            .iter()
            .map(|batch| {
                batch
                    .shots
                    .iter()
                    .map(|shot| {
                        let stream = streams
                            .entry(shot)
                            .or_insert_with(|| SplitMix64::new(seed.shot(shot)));
                        let stream = if together { &mut shared } else { stream };
                        batch
                            .instructions
                            .iter()
                            .map(|_| stream.next_u64())
                            .collect()
                    })
                    .collect()
            })
            .collect()
    }

    /// Flips each value of `row`, written by `instructions`, by its bit of
    /// the draw of its instruction in `draws`.
    fn add_noise(&self, instructions: &[Instruction], draws: &[u64], row: &mut [ReadoutValue]) {
        let mut slots = row.iter_mut();
        for (instruction, draw) in instructions.iter().zip(draws) {
            let count = self.set.readout_slots(instruction.opcode).unwrap_or(0);
            for (bit, slot) in (0..count).zip(slots.by_ref()) {
                if draw >> (bit % 64) & 1 == 1 {
                    *slot = match *slot {
                        ReadoutValue::Zero => ReadoutValue::One,
                        ReadoutValue::One => ReadoutValue::Zero,
                        missing => missing,
                    };
                }
            }
        }
    }

    /// `batches`' instructions, each mapped to the instruction set below.
    fn mapped(&self, batches: &[Batch<'_>]) -> Vec<Vec<Instruction>> {
        let map = self
            .below
            .as_ref()
            .expect("`>>` binds before any `execute`");
        batches
            .iter()
            .map(|batch| {
                batch
                    .instructions
                    .iter()
                    .map(|instruction| {
                        let mut mapped = instruction.clone();
                        mapped.opcode = map.map(instruction.opcode);
                        if self.breaks(Breaks::SendsAnInstructionNotAcceptedBelow) {
                            mapped.opcode = CoinGatesAndReset::RESET;
                        }
                        if self.breaks(Breaks::SendsQubitsBeyondNeeds) {
                            for block in &mut mapped.blocks {
                                *block += QUBITS;
                            }
                        }
                        mapped
                    })
                    .collect()
            })
            .collect()
    }

    /// The calls below for `batches`: each a list of a batch's index and
    /// the range of its instructions sent in that call.
    fn calls(&self, batches: &[Batch<'_>]) -> Vec<Vec<(usize, Range<usize>)>> {
        if self.batches_apart {
            return batches
                .iter()
                .enumerate()
                .map(|(index, batch)| vec![(index, 0..batch.instructions.len())])
                .collect();
        }
        (0..self.calls)
            .map(|part| {
                batches
                    .iter()
                    .enumerate()
                    .filter_map(|(index, batch)| {
                        let count = batch.instructions.len();
                        let range = count * part / self.calls..count * (part + 1) / self.calls;
                        (!range.is_empty()).then_some((index, range))
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|call| !call.is_empty())
            .collect()
    }

    /// The seed it sends below for an `execute` that got `seed`: `seed`,
    /// unless it breaks how it passes the seed on.
    fn seed_below(&self, seed: Seed) -> Seed {
        if self.breaks(Breaks::ReseedsBelow) {
            seed.derive("relay")
        } else if let Some(first_seed) = &self.first_seed {
            let kept = first_seed.get().unwrap_or(seed);
            first_seed.set(Some(kept));
            kept
        } else {
            seed
        }
    }

    /// Relays `batches` below, and answers them with what it read there,
    /// flipped by `noise`.
    async fn relay(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut [BatchReadouts],
        noise: &[Vec<Vec<u64>>],
    ) -> Result<(), ExecuteError> {
        let sent = self.mapped(batches);
        let calls = self.calls(batches);
        let seed = self.seed_below(seed);
        // Each row's values and conditions read below, by batch and by row.
        let mut read: Vec<Vec<(Vec<ReadoutValue>, ShotConditions)>> = batches
            .iter()
            .map(|batch| vec![(Vec::new(), ShotConditions::NONE); batch.shots.len()])
            .collect();
        let stranger = ShotSet::ids([STRANGER]);
        let earlier = self.note_got(batches);
        let extra = u32::from(self.breaks(Breaks::OverstatesSlotsBelow));
        let checked = !self.breaks(Breaks::UnchecksBelow);
        for (number, call) in calls.iter().enumerate() {
            let mut below_batches: Vec<Batch<'_>> = call
                .iter()
                .map(|(index, range)| {
                    let batch = &batches[*index];
                    let slots = if self.calls > 1 {
                        self.set
                            .readout_slots_of(&batch.instructions[range.clone()])
                            .expect("CoinGates count their slots")
                    } else {
                        batch.readout_slots() + extra
                    };
                    Batch::new(batch.shots, &sent[*index][range.clone()], slots)
                        .checked_if(checked && batch.is_checked())
                })
                .collect();
            if self.breaks(Breaks::SendsAShotItDidNotGet) {
                below_batches.push(
                    Batch::new(&stranger, &sent[0], batches[0].readout_slots())
                        .checked_if(batches[0].is_checked()),
                );
            }
            if let Some(earlier) = &earlier {
                below_batches.push(
                    Batch::new(earlier, &sent[0], batches[0].readout_slots())
                        .checked_if(batches[0].is_checked()),
                );
            }
            if self.breaks(Breaks::PutsAShotInTwoBatchesBelow) {
                below_batches.push(below_batches[0]);
            }
            let mut below_readouts = Readouts::new();
            let below_readouts = below_readouts.readouts_for(&below_batches);
            below.execute(&below_batches, seed, below_readouts).await?;
            for ((index, _), from) in call.iter().zip(below_readouts.iter()) {
                let rows = from.shots();
                for (position, (values, conditions)) in read[*index].iter_mut().enumerate() {
                    let source = if self.breaks(Breaks::SwapsRows) {
                        rows - 1 - position
                    } else {
                        position
                    };
                    values.extend_from_slice(from.row(source));
                    if self.breaks(Breaks::KeepsTheLastCallsConditions) {
                        *conditions = from.conditions(source);
                    } else if self.breaks(Breaks::KeepsTheFirstCallsConditions) {
                        if conditions.is_empty() {
                            *conditions = from.conditions(source);
                        }
                    } else if self.breaks(Breaks::KeepsTheFirstTwoCallsConditions) {
                        if number < 2 {
                            *conditions |= from.conditions(source);
                        }
                    } else if self.breaks(Breaks::ClearsWhenACallReadsNone)
                        && from.conditions(source).is_empty()
                    {
                        *conditions = ShotConditions::NONE;
                    } else {
                        *conditions |= from.conditions(source);
                    }
                }
            }
        }
        for (index, ((batch, above), read)) in batches.iter().zip(readouts).zip(read).enumerate() {
            let read_on_rows: Vec<ShotConditions> =
                read.iter().map(|(_, conditions)| *conditions).collect();
            for (position, (shot, (values, read_conditions))) in
                batch.shots.iter().zip(read).enumerate()
            {
                let read_conditions = self.misread(&read_on_rows, position, read_conditions);
                let mut slots = above.readout_slots() as usize;
                if self.breaks(Breaks::LeavesASlotUnwritten) && shot == ShotId::new(1) {
                    slots = slots.saturating_sub(1);
                }
                above.row_mut(position)[..slots].copy_from_slice(&values[..slots]);
                if let Some(draws) = noise.get(index) {
                    self.add_noise(
                        batch.instructions,
                        &draws[position],
                        above.row_mut(position),
                    );
                }
                let conditions = self.conditions_up(shot, read_conditions);
                above.set_conditions(position, conditions);
            }
        }
        Ok(())
    }

    /// Notes the shots of `batches` as got; gives, if it breaks
    /// [`Breaks::SendsShotsOfEarlierExecutes`], those it got earlier only.
    fn note_got(&mut self, batches: &[Batch<'_>]) -> Option<ShotSet> {
        let here: BTreeSet<ShotId> = batches
            .iter()
            .flat_map(|batch| batch.shots.iter())
            .collect();
        let earlier: Vec<u32> = self.got.difference(&here).map(|shot| shot.get()).collect();
        self.got.extend(&here);
        (self.breaks(Breaks::SendsShotsOfEarlierExecutes) && !earlier.is_empty())
            .then(|| ShotSet::ids(earlier))
    }

    /// The conditions it reports on `shot`'s row, which read `read` below:
    /// those, and its own.
    fn conditions_up(&mut self, shot: ShotId, read: ShotConditions) -> ShotConditions {
        let mut conditions = if self.breaks(Breaks::DropsConditions) {
            ShotConditions::NONE
        } else {
            read
        };
        if self.marks == Some(shot) && !(self.unmarks_rejected && self.rejected.contains(&shot)) {
            conditions |= ShotConditions::UNRESOLVED;
        }
        if self.breaks(Breaks::FlagsLaterRoundsOfARejectedShot) && self.rejected.contains(&shot) {
            conditions |= ShotConditions::INCONSISTENT;
        }
        if self.breaks(Breaks::FlagsARejectedShot) && read.contains(ShotConditions::REJECTED) {
            conditions |= ShotConditions::INCONSISTENT;
        }
        if read.contains(ShotConditions::REJECTED) {
            self.rejected.insert(shot);
        }
        if self.breaks(Breaks::CarriesConditionsAcrossExecutes) {
            let carried = self.carried.entry(shot).or_insert(ShotConditions::NONE);
            *carried |= conditions;
            conditions = *carried;
        }
        if self.breaks(Breaks::CarriesConditionsFromItsSecondExecute) && self.relayed > 0 {
            let carried = self.carried.entry(shot).or_insert(ShotConditions::NONE);
            *carried |= conditions;
            conditions = *carried;
        }
        if self.breaks(Breaks::CarriesConditionsOfItsFirstExecute) {
            let carried = self.carried.entry(shot).or_insert(ShotConditions::NONE);
            if self.relayed == 0 {
                *carried |= conditions;
            }
            conditions |= *carried;
        }
        if self.breaks(Breaks::CarriesInconsistentUnlessRejected) {
            let carried = self.carried.entry(shot).or_insert(ShotConditions::NONE);
            conditions |= *carried;
            if read.contains(ShotConditions::INCONSISTENT)
                && !read.contains(ShotConditions::REJECTED)
            {
                *carried |= ShotConditions::INCONSISTENT;
            }
        }
        if let Some(Breaks::CarriesOneKind(kind)) = self.breaks {
            let carried = self.carried.entry(shot).or_insert(ShotConditions::NONE);
            if conditions.contains(kind) {
                *carried |= kind;
            }
            conditions |= *carried;
        }
        if self.breaks(Breaks::CarriesInconsistentAndUnresolved) {
            let carried = self.carried.entry(shot).or_insert(ShotConditions::NONE);
            conditions |= *carried;
            for kind in [ShotConditions::INCONSISTENT, ShotConditions::UNRESOLVED] {
                if conditions.contains(kind) {
                    *carried |= kind;
                }
            }
        }
        conditions
    }

    /// The conditions it passes up for the row at `position`, which read
    /// `read` below, of `read_on_rows`, those read on each row of its
    /// batch: `read`, unless it breaks how it reads them.
    fn misread(
        &self,
        read_on_rows: &[ShotConditions],
        position: usize,
        read: ShotConditions,
    ) -> ShotConditions {
        match self.breaks {
            Some(Breaks::KeepsOnlyRejected) if read.contains(ShotConditions::REJECTED) => {
                ShotConditions::REJECTED
            }
            Some(Breaks::KeepsOnlyRejected) => ShotConditions::NONE,
            Some(Breaks::KeepsOneKindUnlessEvery) if read.contains(every_kind()) => every_kind(),
            Some(Breaks::KeepsTheWorstKind | Breaks::KeepsOneKindUnlessEvery) => [
                ShotConditions::REJECTED,
                ShotConditions::INCONSISTENT,
                ShotConditions::UNRESOLVED,
            ]
            .into_iter()
            .find(|kind| read.contains(*kind))
            .unwrap_or(ShotConditions::NONE),
            Some(Breaks::GivesAnotherRowsConditions) => {
                read_on_rows[(position + 2) % read_on_rows.len()]
            }
            Some(Breaks::AddsTheConditionsOfTheRowTwoFurther) => {
                read | read_on_rows[(position + 2) % read_on_rows.len()]
            }
            Some(Breaks::AddsOneKindOfTheNextRow(kind))
                if read_on_rows[(position + 1) % read_on_rows.len()].contains(kind) =>
            {
                read | kind
            }
            Some(Breaks::MergesItsBatchsConditions) => read_on_rows
                .iter()
                .fold(ShotConditions::NONE, |all, conditions| all | *conditions),
            Some(Breaks::MergesIntoRaisedRows) if !read.is_empty() => read_on_rows
                .iter()
                .fold(ShotConditions::NONE, |all, conditions| all | *conditions),
            Some(Breaks::AddsTwoFurtherIfRaised) if !read.is_empty() => {
                read | read_on_rows[(position + 2) % read_on_rows.len()]
            }
            _ => read,
        }
    }
}

impl AdapterSetup for FakeAdapter {
    fn accepts(&self) -> &InstructionSet {
        &self.set
    }

    fn sends(&self) -> &InstructionSet {
        &self.set
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.below = Some(below);
    }

    fn offers_above(&self, below: Offer) -> Offer {
        if self.draws == Draws::Together {
            below.not_replayable()
        } else {
            below
        }
    }
}

impl Adapt for FakeAdapter {
    fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut [BatchReadouts],
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        let pending = self.pending.clone();
        let reused = pending.replace(false);
        let relayed_now = (!reused && self.breaks(Breaks::RelaysWhenCalledAndReuses)).then(|| {
            pending.set(true);
            self.relay_now(batches, seed, below, readouts)
        });
        let drawn = self
            .breaks(Breaks::DrawsNoiseWhenCalled)
            .then(|| self.draw_noise(batches, seed));
        async move {
            pending.set(false);
            if reused {
                return Ok(());
            }
            if let Some(done) = relayed_now {
                return done;
            }
            if self.executes < self.answers_alone {
                self.executes += 1;
                for (batch, readouts) in batches.iter().zip(readouts) {
                    for position in 0..batch.shots.len() {
                        readouts.row_mut(position).fill(ReadoutValue::Zero);
                    }
                }
                return Ok(());
            }
            if self.ended && self.breaks(Breaks::RefusesAfterEndingShots) {
                let shot = batches[0]
                    .shots
                    .iter()
                    .next()
                    .expect("a batch holds a shot");
                return Err(ExecuteError::Refused(ExecuteRefusal::ShotNotHeld(shot)));
            }
            let noise = match drawn {
                Some(noise) => noise,
                None => self.draw_noise(batches, seed),
            };
            self.relay(batches, seed, below, readouts, &noise).await?;
            self.relayed += 1;
            Ok(())
        }
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        if self.breaks(Breaks::KeepsEndedNoise) {
            return;
        }
        assert!(
            self.allowed != Some(Allowed::PanicsEndingShots),
            "the shots' release failed"
        );
        if self.breaks(Breaks::PanicsEndingShotsItDoesNotHold)
            && let Some(shot) = shots.iter().find(|shot| !self.got.contains(shot))
        {
            panic!("shot {shot} is not held");
        }
        for shot in shots.iter() {
            self.carried.remove(&shot);
            self.rejected.remove(&shot);
            self.got.remove(&shot);
            if let Some(noise) = &self.noise {
                noise.borrow_mut().remove(&shot);
            }
        }
        self.ended = true;
    }
}
