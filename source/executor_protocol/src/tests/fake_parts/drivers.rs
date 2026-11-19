// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fake drivers.

use std::cell::{Cell, RefCell};
use std::future::poll_fn;
use std::pin::pin;
use std::rc::Rc;
use std::task::Poll;

use super::{CoinGates, Log, Values, coin_gate, coin_gates};
use crate::{
    Batch, BatchReadouts, Drive, DriveError, DriverSetup, Execute, FinalRow, Instruction,
    InstructionSet, InstructionStream, Needs, NotReady, OpcodeMap, ReadoutValue, Readouts, Seed,
    ShotConditions, ShotId, ShotSet,
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

impl DriverSetup for CoinCopy {
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
}

impl Drive for CoinCopy {
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
        below: &mut impl Execute,
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

/// How a [`Program`] sends its instructions below.
#[derive(Clone)]
pub(crate) enum Sending {
    /// As batches of one `execute`.
    Whole,
    /// As batches of one `execute_each`, taking no row.
    Each,
    /// Its one sequence cut before each of these positions, in order, each
    /// piece one `execute`, with no readout read between them: as a sender
    /// whose program is too long for one call sends it.
    Pieces(Vec<usize>),
    /// As [`Pieces`](Self::Pieces), each piece one `execute_each`, each
    /// row given kept in the rows given, its batch the piece's position.
    PiecesEach(Vec<usize>, RowsGiven),
    /// Its one sequence as one `execute_stream`, each instruction made only
    /// once pulled, and counted then in `produced`; the iterator panics
    /// when pulled for the instruction at `panics_at`, if any.
    Stream {
        produced: Rc<Cell<usize>>,
        panics_at: Option<usize>,
    },
}

impl Sending {
    /// As one `execute_stream`, counting into a counter of its own.
    pub(crate) fn stream() -> Self {
        Self::stream_counted(&Rc::default())
    }

    /// As one `execute_stream`, counting the instructions made into
    /// `produced`.
    pub(crate) fn stream_counted(produced: &Rc<Cell<usize>>) -> Self {
        Self::Stream {
            produced: Rc::clone(produced),
            panics_at: None,
        }
    }
}

/// A driver that sends its instructions to every shot, as one batch of one
/// `execute`, or its two sequences to each half of the shots, as two
/// batches of one `execute`, unchecked unless it is told, and as
/// [`Sending::Whole`] unless it is told another way; each shot's output is
/// its row and its conditions.
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
    /// The readout slots of each instruction of its first sequence,
    /// counted when it is bound: what its pieces' batches state.
    instruction_slots: Vec<u32>,
    sending: Sending,
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
            instruction_slots: Vec::new(),
            sending: Sending::Whole,
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
            instruction_slots: Vec::new(),
            sending: Sending::Whole,
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

    /// This program, sending as `sending` says.
    pub(crate) fn sending_as(mut self, sending: Sending) -> Self {
        assert!(
            matches!(sending, Sending::Whole | Sending::Each) || self.sequences.len() == 1,
            "a program of one sequence"
        );
        self.sending = sending;
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

impl DriverSetup for Program {
    fn sends(&self) -> &InstructionSet {
        &self.set
    }

    fn bind(&mut self, below: OpcodeMap) {
        self.slots = self
            .sequences
            .iter()
            .map(|sequence| self.set.readout_slots_of(sequence).unwrap_or(0))
            .collect();
        self.instruction_slots = self.sequences[0]
            .iter()
            .map(|instruction| self.set.readout_slots(instruction.opcode).unwrap_or(0))
            .collect();
        for instruction in self.sequences.iter_mut().flatten() {
            instruction.opcode = below.map(instruction.opcode);
        }
    }

    fn needs(&self) -> Needs {
        self.needs.clone()
    }
}

impl Drive for Program {
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
        below: &mut impl Execute,
    ) -> Result<Vec<Self::Output>, DriveError> {
        let outputs = match &self.sending {
            Sending::Whole | Sending::Each => self.send_batches(shots, seed, below).await?,
            Sending::Pieces(cuts) => self.send_pieces(cuts, None, shots, seed, below).await?,
            Sending::PiecesEach(cuts, given) => {
                self.send_pieces(cuts, Some(given), shots, seed, below)
                    .await?
            }
            Sending::Stream {
                produced,
                panics_at,
            } => {
                let (produced, panics_at) = (Rc::clone(produced), *panics_at);
                let instructions = self.sequences[0].iter().enumerate().map(|(at, made)| {
                    assert!(panics_at != Some(at), "the driver's stream panicked");
                    produced.set(produced.get() + 1);
                    made.clone()
                });
                let stream =
                    InstructionStream::new(shots, instructions).checked_if(self.checked[0]);
                let mut readouts = BatchReadouts::new(0, 0);
                below.execute_stream(stream, seed, &mut readouts).await?;
                rows_of(&[readouts])
            }
        };
        if self.ends_its_shots {
            below.end_shots(shots);
        }
        Ok(outputs)
    }
}

impl Program {
    /// Sends each sequence as a batch of one `execute`, or of one
    /// `execute_each`, as [`Sending`] says.
    async fn send_batches(
        &self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
    ) -> Result<Vec<(Values, ShotConditions)>, DriveError> {
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
        if matches!(self.sending, Sending::Each) {
            below
                .execute_each(&batches, seed, readouts, &mut |_| {})
                .await?;
        } else {
            below.execute(&batches, seed, readouts).await?;
        }
        Ok(readouts
            .iter()
            .flat_map(|rows| (0..rows.shots()).map(|i| (rows.row(i).to_vec(), rows.conditions(i))))
            .collect())
    }

    /// Sends its one sequence cut before each of `cuts`, each piece one
    /// `execute` on every shot, or one `execute_each` keeping each row
    /// given in `given`, and joins each shot's rows in order, and its
    /// conditions.
    async fn send_pieces(
        &self,
        cuts: &[usize],
        given: Option<&RowsGiven>,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
    ) -> Result<Vec<(Values, ShotConditions)>, DriveError> {
        let sequence = &self.sequences[0];
        let ends = cuts.iter().copied().chain([sequence.len()]);
        let mut start = 0;
        let mut pieces = Vec::new();
        for end in ends {
            let piece_slots = self.instruction_slots[start..end].iter().sum();
            let batch =
                Batch::new(shots, &sequence[start..end], piece_slots).checked_if(self.checked[0]);
            let mut readouts = BatchReadouts::new(0, 0);
            let readouts_of_it = std::slice::from_mut(&mut readouts);
            if let Some(given) = given {
                let piece = pieces.len();
                let mut each = |row: FinalRow<'_>| {
                    given.borrow_mut().push((
                        piece,
                        row.position,
                        row.values.to_vec(),
                        row.conditions,
                    ));
                };
                below
                    .execute_each(&[batch], seed, readouts_of_it, &mut each)
                    .await?;
            } else {
                below.execute(&[batch], seed, readouts_of_it).await?;
            }
            pieces.push(readouts);
            start = end;
        }
        Ok(rows_of(&pieces))
    }
}

/// Each shot's row in `pieces`, the readouts of consecutive calls on the
/// same shots, joined in order, with the union of its conditions.
fn rows_of(pieces: &[BatchReadouts]) -> Vec<(Values, ShotConditions)> {
    let shots = pieces.first().map_or(0, BatchReadouts::shots);
    (0..shots)
        .map(|position| {
            pieces.iter().fold(
                (Vec::new(), ShotConditions::NONE),
                |(mut row, raised), piece| {
                    row.extend_from_slice(piece.row(position));
                    (row, raised | piece.conditions(position))
                },
            )
        })
        .collect()
}

/// Each row given to a driver's `each`: its batch, its position, its
/// values and its conditions.
pub(crate) type RowsGiven = Rc<RefCell<Vec<(usize, usize, Values, ShotConditions)>>>;

/// A driver that tosses a coin on qubit 0, reads it, flips it and reads it
/// again, on each half of the shots, as two batches of one
/// `execute_each`: it logs `row <batch> <position>` for each row
/// given and keeps it, with its conditions, in `given`, and gives each
/// shot's output, its row's values, to `drive_each`'s `each` as soon as
/// its row is given.
pub(crate) struct OutputsAsRowsCome {
    instructions: Vec<Instruction>,
    slots: u32,
    checked: bool,
    /// The row of each call, counted from 0 in the order given, at which
    /// `each` panics.
    panics_in_each_at: Option<usize>,
    drops_its_call: bool,
    given: RowsGiven,
    log: Log,
}

impl OutputsAsRowsCome {
    pub(crate) fn new(given: &RowsGiven, log: &Log) -> Self {
        let instructions = vec![
            coin_gate("h", 0),
            coin_gate("mz", 0),
            coin_gate("x", 0),
            coin_gate("mz", 0),
        ];
        Self {
            slots: coin_gates()
                .readout_slots_of(&instructions)
                .expect("instructions of the set"),
            instructions,
            checked: false,
            panics_in_each_at: None,
            drops_its_call: false,
            given: Rc::clone(given),
            log: log.clone(),
        }
    }

    /// This driver, sending also an `x` on qubit 1, beyond its needs, in
    /// checked batches.
    pub(crate) fn beyond_its_needs(mut self) -> Self {
        self.instructions.insert(2, coin_gate("x", 1));
        self.checked = true;
        self
    }

    /// This driver, its `each` panicking on the first row.
    pub(crate) fn panicking_in_each(self) -> Self {
        self.panicking_in_each_after(0)
    }

    /// This driver, its `each` panicking on the row after the first
    /// `rows`, once it gave their shots' outputs.
    pub(crate) fn panicking_in_each_after(mut self, rows: usize) -> Self {
        self.panics_in_each_at = Some(rows);
        self
    }

    /// This driver, dropping its `execute_each` unfinished once a row was
    /// given, and answering with the readouts as they are then.
    pub(crate) fn dropping_its_call(mut self) -> Self {
        self.drops_its_call = true;
        self
    }

    /// Sends the two halves of `shots` below, by `execute_each`, and calls
    /// `on_row` with each shot and its values as its row is given; answers
    /// with every shot's row, in the order of `shots`.
    async fn send(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
        mut on_row: impl FnMut(ShotId, &[ReadoutValue]),
    ) -> Result<Vec<Values>, DriveError> {
        let half = shots.len().div_ceil(2);
        let halves: Vec<ShotSet> = [0..half, half..shots.len()]
            .into_iter()
            .filter_map(|positions| shots.select(&positions.collect::<Vec<_>>()))
            .collect();
        let batches: Vec<Batch<'_>> = halves
            .iter()
            .map(|half| Batch::new(half, &self.instructions, self.slots).checked_if(self.checked))
            .collect();
        let mut kept = Readouts::new();
        let readouts = kept.readouts_for(&batches);
        let (given, log, panics_at) = (&self.given, &self.log, self.panics_in_each_at);
        let mut rows = 0;
        let mut each = |row: FinalRow<'_>| {
            log.push(format!("row {} {}", row.batch, row.position));
            given
                .borrow_mut()
                .push((row.batch, row.position, row.values.to_vec(), row.conditions));
            rows += 1;
            assert!(panics_at != Some(rows - 1), "the driver's `each` panicked");
            let id = halves[row.batch]
                .iter()
                .nth(row.position)
                .expect("a row's position is below its batch's shots");
            on_row(id, row.values);
        };
        let drops_its_call = self.drops_its_call;
        {
            let mut call = pin!(below.execute_each(&batches, seed, readouts, &mut each));
            if drops_its_call {
                poll_fn(|context| match call.as_mut().poll(context) {
                    Poll::Pending if given.borrow().is_empty() => Poll::Pending,
                    Poll::Pending => Poll::Ready(Ok(())),
                    Poll::Ready(answer) => Poll::Ready(answer),
                })
                .await?;
            } else {
                call.await?;
            }
        }
        below.end_shots(shots);
        Ok(readouts
            .iter()
            .flat_map(|rows| (0..rows.shots()).map(|i| rows.row(i).to_vec()))
            .collect())
    }
}

impl DriverSetup for OutputsAsRowsCome {
    fn sends(&self) -> &InstructionSet {
        coin_gates()
    }

    fn bind(&mut self, below: OpcodeMap) {
        for instruction in &mut self.instructions {
            instruction.opcode = below.map(instruction.opcode);
        }
    }

    fn needs(&self) -> Needs {
        Needs::qubits(1)
    }
}

impl Drive for OutputsAsRowsCome {
    type Output = Values;

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
    ) -> Result<Vec<Values>, DriveError> {
        self.send(shots, seed, below, |_, _| {}).await
    }

    async fn drive_each(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Execute,
        each: &mut impl FnMut(ShotId, Values),
    ) -> Result<(), DriveError> {
        self.send(shots, seed, below, |id, values| each(id, values.to_vec()))
            .await
            .map(drop)
    }
}

/// The rows `given` holds, sorted by batch and position, with their values
/// alone: each row's values in the order of the shots, as `drive` answers.
pub(crate) fn sorted_values(given: &RowsGiven) -> Vec<Values> {
    let mut rows = given.borrow().clone();
    rows.sort_by_key(|&(batch, position, ..)| (batch, position));
    rows.into_iter().map(|(_, _, values, _)| values).collect()
}

/// The entries of `log` naming a row: the bits running it (`Bits row …`),
/// the driver given it (`row …`), and a run's caller given its shot's
/// output (`output …`), where the test logs it.
pub(crate) fn rows_logged(log: &Log) -> Vec<String> {
    log.entries()
        .into_iter()
        .filter(|entry| {
            ["row ", "Bits row ", "output "]
                .iter()
                .any(|kind| entry.starts_with(kind))
        })
        .collect()
}
