// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A stream run in pieces ([`StreamPieces`]): how the default
//! `execute_stream` and `adapt_stream` run a stream, each piece one
//! `execute` or one `adapt` of one batch, holding one piece at a time.

use std::mem;

use crate::{
    Batch, BatchReadouts, ExecuteError, ExecuteRefusal, Failure, Instruction, InstructionSet,
    InstructionStream, ShotConditions, ShotId, ShotSet,
};

/// The most instructions of a piece when the receiver's offer sets no most
/// ([`Offer::max_instructions`](crate::Offer::max_instructions)): enough
/// that the cost of a call per piece is spread thin (the bench's `stream`
/// section times it, and is flat from about 100 per piece up), few enough
/// that a piece stays small beside a shot's state. The bench compares its
/// pieces with batches of the same size, stated there again.
const PIECE: usize = 1024;

/// The most instructions of a piece for a receiver whose most is `most`:
/// `most`, or 1 when it is 0, or [`PIECE`] when it sets none. A receiver
/// that takes no instruction a batch thus gets pieces of 1, which its
/// `execute` may refuse, rather than a stream that runs nothing and
/// answers `Ok`.
pub(crate) fn most_per_piece(most: Option<u32>) -> usize {
    most.map_or(PIECE, |most| most.max(1) as usize)
}

/// A stream's instructions on `shots`, run in pieces of at most `most`, in
/// order ([`run`](Self::run)): it [`pull`](Self::pull)s a piece, runs its
/// [`batch`](Self::batch) (one `execute` or one `adapt`), takes how the
/// piece [`ran`](Self::ran), and once no piece is left,
/// [`join_into`](Self::join_into) fills the stream's readouts: each shot's
/// row is its rows in the pieces, in order, and its conditions their union.
///
/// It holds one piece of instructions at a time, pulled from the stream
/// only once the piece before it has answered; the readouts of each piece
/// that reads anything, until the end, where they are joined; and, for the
/// pieces that read nothing, the union of their conditions per shot. An
/// empty stream runs nothing.
pub(crate) struct StreamPieces<I: Iterator<Item = Instruction>> {
    instructions: I,
    most: usize,
    /// The piece pulled last.
    piece: Vec<Instruction>,
    /// Whether the stream gave a short piece, or none: it has ended.
    ended: bool,
    /// The readouts of the piece pulled last, shaped by [`Self::batch`].
    piece_readouts: BatchReadouts,
    /// The readouts of each piece that ran and read anything, in order.
    read: Vec<BatchReadouts>,
    /// The readout slots of `read`, all together.
    read_slots: u32,
    /// Per shot, the conditions of the pieces that ran and read nothing;
    /// empty until a shot has any.
    unread_conditions: Vec<ShotConditions>,
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
            read: Vec::new(),
            read_slots: 0,
            unread_conditions: Vec::new(),
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
            .filter(|slots| slots.checked_add(self.read_slots).is_some())
        else {
            let room = u64::from(u32::MAX - self.read_slots);
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
                shot: first_shot(shots),
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

    /// Takes how the piece pulled last ran: its readouts kept, or its
    /// error, which ends the stream, returned with its positions in the
    /// stream: a refusal of the first piece as it is, since nothing of the
    /// stream ran; a refusal of a later one as [`Failure::Refused`], since
    /// the pieces before it ran.
    fn ran(&mut self, answer: Result<(), ExecuteError>) -> Result<(), ExecuteError> {
        answer.map_err(|error| in_the_stream(error, self.sent))?;
        self.sent += self.piece.len();
        if self.piece_readouts.readout_slots() > 0 {
            self.read_slots += self.piece_readouts.readout_slots();
            self.read.push(mem::replace(
                &mut self.piece_readouts,
                BatchReadouts::new(0, 0),
            ));
        } else {
            union_conditions(&mut self.unread_conditions, &self.piece_readouts);
        }
        Ok(())
    }

    /// Fills `readouts` with the readouts of every piece that ran, joined,
    /// for `shots`.
    fn join_into(self, shots: &ShotSet, readouts: &mut BatchReadouts) {
        readouts.shape(shots.len(), self.read_slots);
        readouts.concat_from(&self.read);
        for (position, &raised) in self.unread_conditions.iter().enumerate() {
            readouts.set_conditions(position, readouts.conditions(position) | raised);
        }
    }
}

/// The error of a piece after `sent` instructions of its stream ran: an
/// instruction's position counted from the stream's first, and a refusal
/// made a failure once any piece ran.
fn in_the_stream(error: ExecuteError, sent: usize) -> ExecuteError {
    let ExecuteError::Refused(mut refusal) = error else {
        return error;
    };
    if let ExecuteRefusal::Instruction { at, .. } | ExecuteRefusal::TooManyReadoutSlots { at, .. } =
        &mut refusal
    {
        *at += sent;
    }
    if sent == 0 {
        ExecuteError::Refused(refusal)
    } else {
        ExecuteError::Failed {
            shot: refusal.shot(),
            failure: Failure::Refused(refusal),
        }
    }
}

/// Adds the conditions of `piece_readouts`, a piece's that reads nothing,
/// to `conditions`, one per shot, made only once a shot has any.
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

/// The first of `shots`, in its order.
fn first_shot(shots: &ShotSet) -> ShotId {
    shots.iter().next().expect("a shot set is never empty")
}
