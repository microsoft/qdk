// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A stream run in pieces ([`StreamPieces`]): how the default
//! `execute_stream` and `adapt_stream` run a stream, each piece one
//! `execute` or one `adapt` of one batch, holding one piece at a time.

use crate::{
    Batch, BatchReadouts, ExecuteError, ExecuteRefusal, Instruction, InstructionSet,
    InstructionStream, ReadoutFailure, ReadoutValue, ShotConditions, ShotSet,
};

/// The most instructions of any piece, whatever the receiver's most
/// ([`Offer::max_instructions`](crate::Offer::max_instructions)): enough
/// that the cost of a call per piece is spread thin (the bench's `stream`
/// section times it, and is flat from about 100 per piece up), few enough
/// that a piece stays small beside a shot's state, so that a stream held a
/// piece at a time takes flat memory even to a receiver taking millions of
/// instructions a batch. The bench compares its pieces with batches of the
/// same size, stated there again.
const PIECE: usize = 1024;

/// The most instructions of a piece for a receiver whose most is `most`:
/// `most`, or 1 when it is 0, and at most [`PIECE`], which it is when the
/// receiver sets none. A receiver that takes no instruction a batch thus
/// gets pieces of 1, which its `execute` may refuse, rather than a stream
/// that runs nothing and answers `Ok`. The pieces a boundary checks a
/// checked stream in are of the same size, so that each piece the default
/// cuts is one piece checked whole.
pub(crate) fn most_per_piece(most: Option<u32>) -> usize {
    most.map_or(PIECE, |most| (most.max(1) as usize).min(PIECE))
}

/// A stream's instructions on `shots`, run in pieces of at most `most`, in
/// order ([`run`](Self::run)): it [`pull`](Self::pull)s a piece, runs its
/// [`batch`](Self::batch) (one `execute` or one `adapt`), takes how the
/// piece [`ran`](Self::ran), and once no piece is left,
/// [`join_into`](Self::join_into) fills the stream's readouts: each shot's
/// row is its rows in the pieces, in order, and its conditions their union.
///
/// It holds one piece of instructions at a time, pulled from the stream
/// only once the piece before it has answered, and its readouts; and the
/// rows of the pieces that ran, joined as they come ([`JoinedRows`]): the
/// stream's readouts so far, and at most as much again of room for those
/// to come, however small the pieces. An empty stream runs nothing.
pub(crate) struct StreamPieces<I: Iterator<Item = Instruction>> {
    instructions: I,
    most: usize,
    /// The piece pulled last.
    piece: Vec<Instruction>,
    /// Whether the stream gave a short piece, or none: it has ended.
    ended: bool,
    /// The readouts of the piece pulled last, shaped by [`Self::batch`].
    piece_readouts: BatchReadouts,
    /// The rows of the pieces that ran, joined.
    joined: JoinedRows,
    /// The instructions of the pieces that ran.
    sent: usize,
}

impl<I: Iterator<Item = Instruction>> StreamPieces<I> {
    /// Runs `stream` in pieces of at most `most` on `part`, which accepts
    /// [`accepts`](crate::ExecutorSetup::accepts)`(part)`, each piece by
    /// `run_piece` (one `execute` or one `adapt` of its batch), and fills
    /// `readouts` with the stream's, as the default `execute_stream` says.
    /// A piece's error ends the stream, returned with its positions in the
    /// stream ([`ran`](Self::ran)).
    pub(crate) async fn run<P: ?Sized>(
        stream: InstructionStream<'_, I>,
        most: usize,
        part: &mut P,
        accepts: fn(&P) -> &InstructionSet,
        mut run_piece: impl AsyncFnMut(
            &mut P,
            &Batch<'_>,
            &mut BatchReadouts,
        ) -> Result<(), ExecuteError>,
        readouts: &mut BatchReadouts,
    ) -> Result<(), ExecuteError> {
        let checked = stream.is_checked();
        let InstructionStream {
            shots,
            instructions,
            ..
        } = stream;
        let mut pieces = Self::new(instructions, most);
        while pieces.pull() {
            let answer = match pieces.batch(accepts(part), shots, checked) {
                Ok((batch, piece_readouts)) => run_piece(part, &batch, piece_readouts).await,
                Err(refused) => Err(refused),
            };
            pieces.ran(answer)?;
        }
        pieces.join_into(shots, readouts);
        Ok(())
    }

    /// `instructions`, to be run in pieces of at most `most`.
    fn new(instructions: I, most: usize) -> Self {
        Self {
            instructions,
            most,
            piece: Vec::new(),
            ended: false,
            piece_readouts: BatchReadouts::new(0, 0),
            joined: JoinedRows::new(),
            sent: 0,
        }
    }

    /// Pulls the next piece: `false` once the stream has ended.
    fn pull(&mut self) -> bool {
        if self.ended {
            return false;
        }
        self.piece.clear();
        self.piece
            .extend(self.instructions.by_ref().take(self.most));
        // A short piece is the last: the source is not asked again once it
        // has ended, fused or not.
        self.ended = self.piece.len() < self.most;
        !self.piece.is_empty()
    }

    /// The batch of the piece pulled last on `shots`, checked if `checked`,
    /// its readout slots counted with `set`, the receiver's, and the
    /// readouts it is to fill, shaped for it.
    ///
    /// An opcode beyond `set` counts no slot: an unchecked batch runs as
    /// sent, as [`Batch::new`] says, and a checked one was checked at the
    /// boundary the stream crossed. Refused, as a checked batch is, when
    /// the stream's slots would no longer fit a `u32`, so that its readouts
    /// can hold them.
    fn batch<'a>(
        &'a mut self,
        set: &InstructionSet,
        shots: &'a ShotSet,
        checked: bool,
    ) -> Result<(Batch<'a>, &'a mut BatchReadouts), ExecuteError> {
        let slots_of = |instruction: &Instruction| {
            u64::from(set.readout_slots(instruction.opcode).unwrap_or(0))
        };
        // In a `u64`, a piece's slots cannot overflow: a piece holds at
        // most a `u32`'s count of instructions, each of at most a `u32`'s
        // count of slots.
        let piece_slots: u64 = self.piece.iter().map(slots_of).sum();
        let Some(piece_slots) = u32::try_from(piece_slots)
            .ok()
            .filter(|slots| slots.checked_add(self.joined.slots).is_some())
        else {
            let room = u64::from(u32::MAX - self.joined.slots);
            let mut summed = 0;
            let at = self
                .piece
                .iter()
                .position(|instruction| {
                    summed += slots_of(instruction);
                    summed > room
                })
                .expect("the piece's slots go beyond the room left");
            return Err(ExecuteError::Refused(ExecuteRefusal::TooManyReadoutSlots {
                shot: shots.first(),
                batch: 0,
                at,
            }));
        };
        self.piece_readouts.shape(shots.len(), piece_slots);
        Ok((
            Batch::new(shots, &self.piece, piece_slots).checked_if(checked),
            &mut self.piece_readouts,
        ))
    }

    /// Takes how the piece pulled last ran: its readouts joined to those
    /// before, or its error, which ends the stream, a refusal made the
    /// stream's ([`ExecuteRefusal::of_a_stream_after`]).
    fn ran(&mut self, answer: Result<(), ExecuteError>) -> Result<(), ExecuteError> {
        answer.map_err(|error| match error {
            ExecuteError::Refused(refusal) => refusal.of_a_stream_after(self.sent),
            failed @ ExecuteError::Failed { .. } => failed,
        })?;
        self.sent += self.piece.len();
        self.joined.append(&self.piece_readouts);
        Ok(())
    }

    /// Fills `readouts` with the readouts of every piece that ran, joined,
    /// for `shots`.
    fn join_into(self, shots: &ShotSet, readouts: &mut BatchReadouts) {
        *readouts = self.joined.into_readouts(shots.len());
    }
}

/// The rows of a stream's pieces that ran, joined as they come, in one
/// buffer: each shot's row is its rows in the pieces, in order, and its
/// conditions their union. A row has room for more values than it holds,
/// and the room doubles when a piece's rows outgrow it, so the buffer holds
/// the stream's readouts so far, and at most as much again, however small
/// its pieces: no piece's readouts are kept whole.
struct JoinedRows {
    /// The rows, one after another, each of `room` values: shot i's is
    /// `values[i * room..i * room + slots]`, the rest room for the pieces
    /// to come.
    values: Vec<ReadoutValue>,
    /// The values each row has room for.
    room: usize,
    /// The values each row holds: the readout slots of the pieces that
    /// ran, all together.
    slots: u32,
    /// Each shot's conditions, by its position; empty until a shot has
    /// any.
    conditions: Vec<ShotConditions>,
}

impl JoinedRows {
    /// No rows yet.
    fn new() -> Self {
        Self {
            values: Vec::new(),
            room: 0,
            slots: 0,
            conditions: Vec::new(),
        }
    }

    /// Appends each shot's row of `piece`, a piece's readouts on the
    /// stream's shots, to its row so far, and adds its conditions to the
    /// shot's. Its slots fit a `u32` with those before, as
    /// [`StreamPieces::batch`] checked.
    fn append(&mut self, piece: &BatchReadouts) {
        let shots = piece.shots();
        let (held, added) = (self.slots as usize, piece.readout_slots() as usize);
        if held + added > self.room {
            let room = (held + added).max(2 * self.room);
            self.values
                .resize(shots * room, ReadoutValue::Missing(ReadoutFailure::Other));
            // The rows move to their wider places last first, so that none
            // is written over before it has moved.
            for position in (1..shots).rev() {
                let from = position * self.room;
                self.values.copy_within(from..from + held, position * room);
            }
            self.room = room;
        }
        for position in 0..shots {
            let at = position * self.room + held;
            self.values[at..at + added].copy_from_slice(piece.row(position));
        }
        self.slots += piece.readout_slots();
        union_conditions(&mut self.conditions, piece);
    }

    /// The rows joined, as the readouts of `shots` shots, in the same
    /// buffer: each row moved next to the one before it.
    fn into_readouts(mut self, shots: usize) -> BatchReadouts {
        let held = self.slots as usize;
        for position in 1..shots {
            let from = position * self.room;
            self.values.copy_within(from..from + held, position * held);
        }
        self.values.truncate(shots * held);
        BatchReadouts::from_rows(shots, self.slots, self.values, self.conditions)
    }
}

/// Adds the conditions of `piece_readouts`, a piece's, to `conditions`,
/// one per shot, made only once a shot has any.
fn union_conditions(conditions: &mut Vec<ShotConditions>, piece_readouts: &BatchReadouts) {
    for position in 0..piece_readouts.shots() {
        let raised = piece_readouts.conditions(position);
        if raised.is_empty() {
            continue;
        }
        if conditions.is_empty() {
            conditions.resize(piece_readouts.shots(), ShotConditions::NONE);
        }
        conditions[position] |= raised;
    }
}
