# Batches and readouts

For those who write a part that calls `execute` or answers it: what a
readout slot is, what a batch holds, how its readouts are shaped and
filled, what a value and a shot's conditions say, and how a caller keeps
its readouts from one `execute` to the next. The terms are those of the
[README](../README.md#terms); the methods that send and answer batches are
in [Writing a part](writing-a-part.md).

## Readout slots

A [`ReadoutSlot`](../src/batch/batch_readouts/readout_slot.rs) is a position in a shot's row.
A batch's slots are each instruction's readouts, its measurement outcomes
then its flags (named bits it also reports), in instruction order, so
slot k is the same readout on every shot of the batch.

## Batches

A driver or an adapter calls `below.execute(&batches, seed, readouts)`.
One `execute` may hold batches of shots at different points of their
programs. A batch is made by
[`Batch::new(shots, instructions, readout_slots)`](../src/batch.rs): its
sender states the readout slots of its instructions, summed, since it
knows them where it writes them
([`InstructionSet::readout_slots_of`](../src/instruction_set.rs) counts
them), so that no boundary below counts them again, unless the batch is
checked.

A part sends its batches checked ([`Batch::checked`](../src/batch.rs))
while it is developed, or when they may hold what the receiver does not
accept, such as a part built from a user's description at run time; what
each boundary then checks is listed in
[Where and when each check runs](composing-and-running.md#where-and-when-each-check-runs).

## Shaped by the protocol, filled by the receiver

Before the receiver runs, the boundary shapes each batch's
[`BatchReadouts`](../src/batch/batch_readouts.rs) from the batch, checked or
not: a row per shot, in the order of its shot set, of one value per
readout slot, each `Missing(ReadoutFailure::Other)`, and each shot clear
of conditions (what a part found suspect in the shot, below). The
receiver (the executor, or an adapter in its `adapt`) fills the rows in
place (`row_mut`, `set_conditions`), and the part above reads a shot's row
by its position in the batch (`row(i)`, `value(i, slot)`), with no check
of its own. A value no part wrote stays missing, never a bit made up for
it. A caller reads its readouts only once its `execute` answers `Ok`.

## Values

A [`ReadoutValue`](../src/batch/batch_readouts/readout_value.rs) is `Zero`,
`One`, or `Missing` with its
[`ReadoutFailure`](../src/batch/batch_readouts/readout_failure.rs): `Loss`, such
as an atom that left its trap, or `Other`. A lost value keeps its slot,
and the shot runs on with its other values.

## Conditions

Each row carries its shot's
[`ShotConditions`](../src/batch/batch_readouts/shot_conditions.rs): what a part
found suspect in the shot without ending it (`REJECTED`, `INCONSISTENT`,
`UNRESOLVED`). A row reports the conditions raised on its shot in that
`execute` only, since each `execute` gets its rows clear. So a part above
merges the conditions it reads below with its own, by union, and passes
them up, raising its own as
[the rules a part keeps](writing-a-part.md#the-rules-a-part-keeps) say;
and a driver that runs a shot over several `execute`s merges each
one's into the shot's.

## Keeping and building readouts

A caller keeps a [`Readouts`](../src/batch/batch_readouts/readouts.rs) from one
`execute` to the next, whose memory `readouts_for` reuses. Outside a
chain, as in an executor's tests, a caller shapes readouts with
[`BatchReadouts::new`](../src/batch/batch_readouts.rs); and
`BatchReadouts::concat_from` joins the readouts of a batch split over
consecutive `execute`s, as a stepping debugger splits one.
