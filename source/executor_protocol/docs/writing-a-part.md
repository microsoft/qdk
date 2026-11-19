# Writing a part

For those who write a part, a driver, an adapter or an executor: what each
role's methods do at each phase of a chain's life, an executor in full,
the rules a part keeps that the protocol does not check, the
[streaming forms](#streaming) and their defaults, which panics are whose,
and how a part draws from the run's seed, or says it cannot replay a
shot. What crosses a boundary is in
[Batches and readouts](batches-and-readouts.md); what `>>` checks of a
part, in [Instruction sets and their fit](instruction-sets.md) and
[Needs, offers and the admission](admission.md). The terms are those of
the [README](../README.md#terms).

## The roles' methods, phase by phase

A part implements its role's two traits: a setup trait, whose methods
`>>` calls when it composes the chain (the first two phases below), and
a run-time trait, whose methods the runner calls on each run (the last
three). Each method is called at one phase of a chain's life:

| Phase                                              | [`DriverSetup`](../src/roles/driver_setup.rs) and [`Drive`](../src/roles/drive.rs), the driver        | [`AdapterSetup`](../src/roles/adapter_setup.rs) and [`Adapt`](../src/roles/adapt.rs), an adapter                                                                  | [`ExecutorSetup`](../src/roles/executor_setup.rs) and [`Execute`](../src/roles/execute.rs), the executor                                                    |
| -------------------------------------------------- | ----------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Each `>>`: the fit                                 | `sends`, and `bind`, which takes the receiver's opcodes                                               | `accepts`, `sends`, and `bind`                                                                                                                                    | `accepts`                                                                                                                                                   |
| The `>>` that makes a runner: the admission        | `needs`                                                                                               | `needs_below` and `offers_above`, which adapt the needs passed down and the offer passed up, unchanged by default                                                 | `offers`                                                                                                                                                    |
| Each run, before any shot                          | `ready()`, asked last: a driver knows its own needs                                                   | `ready(above)`, for the needs of the part above, asked after the parts below it                                                                                   | `ready(needs)`, for the needs that reach it, asked first                                                                                                    |
| The run                                            | `drive(shots, seed, below)`: calls `below.execute` until each shot ends, and gives each shot's output | `adapt(batches, seed, below, readouts)`: answers each `execute` by calling `below.execute` as often as it needs, and maps the readouts back                       | `execute(batches, seed, readouts)`: runs each batch on its shots' states, and fills their readouts                                                          |
| The run, through the [streaming forms](#streaming) | `drive_each(shots, seed, below, each)`: as `drive`, giving each shot's output to `each`               | `adapt_each(batches, seed, below, readouts, each)`: as `adapt`, giving each row to `each`; `adapt_stream(stream, seed, below, readouts)`: as `adapt`, on a stream | `execute_each(batches, seed, readouts, each)`: as `execute`, giving each row to `each`; `execute_stream(stream, seed, readouts)`: as `execute`, on a stream |
| Once the run ends                                  | nothing: the runner ends the shots below, and a driver may end a shot earlier                         | `end_shots`, called top to bottom, which frees the shots' state, and never passes the end below                                                                   | `end_shots`, called last, which frees the shots' state                                                                                                      |

Each setup trait also has `name`, the part's name in messages, by default
its type's.

## An executor, in full

The executor of
[`examples/compose_and_run.rs`](../examples/compose_and_run.rs), `Bits`,
implements [`ExecutorSetup`](../src/roles/executor_setup.rs) and
[`Execute`](../src/roles/execute.rs). `CoinGates` is the
example's instruction set, and `RandomGenerator` the random-number
generator each shot draws from, both in the file;
`without_readout_parity` says it cannot resolve an argument that
is the parity of the shot's earlier readouts, which the coin never sends;
and only the example's test reads its counts of checked and unchecked
batches, as an executor need not count them:

```rust
impl ExecutorSetup for Bits {
    fn accepts(&self) -> &InstructionSet {
        &self.set
    }

    fn offers(&self) -> Offer {
        Offer::unlimited()
            .max_qubits(64)
            .max_shots(1000)
            .without_readout_parity()
    }
}

impl Execute for Bits {
    async fn ready(&mut self, needs: &Needs) -> Result<(), NotReady> {
        self.qubits = needs.blocks_of(&BlockType::qubit()) as usize;
        Ok(())
    }

    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        // The protocol shaped each batch's readouts from the batch: a row
        // per shot, of a value per `mz`, in order.
        for (batch, batch_readouts) in batches.iter().zip(readouts) {
            if batch.is_checked() {
                self.checked_batches += 1;
            } else {
                self.unchecked_batches += 1;
            }
            for (position, id) in batch.shots.iter().enumerate() {
                let qubits = self.qubits;
                let (bits, random) = self
                    .shots
                    .entry(id)
                    .or_insert_with(|| (vec![false; qubits], RandomGenerator(seed.shot(id))));
                let mut row = batch_readouts.row_mut(position).iter_mut();
                for instruction in batch.instructions {
                    // Below the shot's qubits, as the parts above send them;
                    // a checked batch's checks refuse any other.
                    let qubit = instruction.blocks[0] as usize;
                    match instruction.opcode {
                        CoinGates::H => bits[qubit] = random.chance(0.5),
                        CoinGates::X => bits[qubit] = !bits[qubit],
                        CoinGates::MZ => {
                            *row.next().expect("a readout slot per `mz`") = bits[qubit].into();
                        }
                        _ => unreachable!("the parts above send only instructions of the set"),
                    }
                }
            }
        }
        Ok(())
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        for id in shots.iter() {
            self.shots.remove(&id);
        }
    }
}
```

`Bits` never waits, yet writes `async fn execute`, so that its work runs
when the future is polled ([the rules](#the-rules-a-part-keeps)).

The example's adapter, `BitFlip`, in the same file, shows an adapter's
rules in code: it draws from `seed.derive("bit_flip")`, maps each
instruction's opcode with the `OpcodeMap` of its `bind`, sends each shot
below in a batch of its own, checked if the batch it came from was
(`checked_if`), reuses its `Readouts` with `readouts_for`, and copies each
row and its conditions back.

## The rules a part keeps

The protocol does not check these; the rustdoc of each role, and of
[`Seed`](../src/seed.rs) for the seeds, states them:

- its instruction sets, needs and offer do not change once it is composed,
  since the fit and the admission rely on those contracts staying the
  same, as do the needs each part gets ready for on every run;
- a part that calls `execute` puts a shot in at most one of its batches,
  and only shots it was given: the run's, for a driver, those of the
  batches it got, for an adapter;
- shots are independent: an executor may run an `execute`'s batches in any
  order, or at once, and may share work between shots, such as one state
  for shots whose instructions are equal so far, as long as each shot's
  readouts are those it would get run alone; it leaves untouched every
  shot that is in none of the batches;
- an executor, and an adapter, writes every value of the readouts it
  answers, and keeps their shape;
- an adapter sends the batches it makes for a checked batch checked
  ([`Batch::checked_if`](../src/batch.rs)), so that its sender's checks
  reach every boundary below;
- a driver gives one output per shot, in the order of the run's shots;
  in `drive_each` ([Streaming](#streaming)), it gives each shot the
  output `drive` would give it, at most once, and every shot's when it
  answers `Ok`, in any order, on the thread that polls the call's future
  ([`Drive::drive_each`](../src/roles/drive.rs) says what holds after an
  error);
- a part that gives rows as they are final ([Streaming](#streaming))
  gives `each` each row at most once, and every row when it answers `Ok`,
  each row holding, after an `Ok`, the values and conditions its readouts
  hold when the call answers, on the thread that polls the call's future,
  so that `each` need not be `Send`
  ([`Execute::execute_each`](../src/roles/execute.rs) says what holds
  after an error);
- a part that takes a stream whole ([Streaming](#streaming)) answers
  with the readouts the same instructions would get as one batch, keeps
  the shots' states after it as after an `execute`, and pulls the
  stream's instructions only as it runs them, so that its sender holds no
  more of the program than it would by the default;
- a part that draws seeds its random-number generators from the run's
  seed, one per shot if it is replayable, as [Seeds](#seeds) says;
- a `drive`, `adapt` or `execute` does its work when its future is first
  polled, not when it is called;
- a part takes calls again after it failed or panicked, since the next run
  calls every part as before.

A conformance kit, planned, will check the rules on readouts and on
checked batches in an implementer's tests, such as a simulator's or a
code's lowering's.

## Streaming

A shot's program and its results need not cross a boundary whole, as the
[README](../README.md#streaming) says: each role's run-time method has
forms that give results out as they come (`_each`) or take instructions
in as they are made (`_stream`). Each form has a default, written in
terms of the role's method, so every part has every form, whether or not
it overrides it.

### Results out as they come

A caller may want each shot's result while the other shots still run,
and a driver may want a shot's readouts before its `execute` answers, to
start the shot's classical work. Through an `_each` form, a part that
finishes its shots one at a time gives each shot's result as soon as it
has it: a row ([`FinalRow`](../src/roles/execute/final_row.rs)), once
[final](../README.md#terms), to the `each` of the part above, or an
output, once its shot ends, to the `each` of the caller of
[`Runner::run_each`](../src/composition/runner.rs) or `run_each_async`:

| Method                                             | By default                                                                                                                                                 | A part overrides it when                                                                  |
| -------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| [`Drive::drive_each`](../src/roles/drive.rs)       | `drive`, then, after an `Ok`, each output, in the order of the run's shots                                                                                 | its shots end at different times, such as in rounds                                       |
| [`Adapt::adapt_each`](../src/roles/adapt.rs)       | `adapt`, then, after an `Ok`, every row, batch by batch, each batch in its shots' order, so rows from below reach the caller only once the adapter answers | it can give a row as soon as the rows below that make it are given, such as a noise model |
| [`Execute::execute_each`](../src/roles/execute.rs) | `execute`, then, after an `Ok`, every row, batch by batch, each batch in its shots' order                                                                  | it finishes its shots one by one, or a group or a shard at a time                         |

A row is its batch's in that call: for a shot whose instructions come
over several calls, each `execute_each` gives the row of that call's
instructions, and the shot's rows, in the order of the calls, make its
row of the instructions sent whole.

A panic in `each` is the caller's, not that of the part that calls it:
`each` is not called again, and the panic is raised again through the
caller's call once the part below answers; a call dropped before then
drops that panic. So a panic in the `each` a driver gives to
`execute_each` fails the run naming the driver, and one in the `each`
given to `run_each` or `run_each_async` is raised from the run once its
shots are ended, unless a part panics while they end, whose panic is
raised instead.

### Instructions in as they are made

A shot's state is kept from its first batch until its shots are ended,
across any number of `execute`s, so a sender may send a shot's program
in as many batches as it likes, each within the receiver's most
instructions per batch ([`Offer`](../src/offer.rs)), and gets the
readouts of the program sent whole. The protocol holds only the batches
of one `execute`; the sender chooses how much of its program it holds.

A sender whose program is too long to hold, or that makes it as it goes,
such as an interpreter that stops every so many instructions, need not
cut it itself: it passes an iterator of its instructions, as an
[`InstructionStream`](../src/batch/instruction_stream.rs) on a shot set,
to `execute_stream`, and gets one row per shot of every readout of the
stream, as one batch would:

| Method                                               | By default                                                                                                                                                                                                                                                                                | A part overrides it when                                                                                                                                                                 |
| ---------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [`Execute::execute_stream`](../src/roles/execute.rs) | `execute` of one piece at a time, each one batch of at most the executor's most instructions per batch (1 if it takes none) and at most about a thousand, pulled from the iterator only once the piece before it answered; each piece's readouts are joined to those before as it answers | it applies instructions as they come, such as a simulator, or passes the stream on whole, such as a link to a remote executor                                                            |
| [`Adapt::adapt_stream`](../src/roles/adapt.rs)       | `adapt` of one piece at a time, each of at most the most instructions per batch it offers above, as for `execute_stream`                                                                                                                                                                  | its instructions below follow from those it gets one by one, such as a code that lowers each logical instruction: it passes its own stream below, so that an executor's own form gets it |

So by default a part holds one piece of the stream, never the whole
program, and the stream's readouts, joined as each piece answers; the
rustdoc of each method gives the bound. The default's pieces hold at
most the receiver's most instructions per batch, or 1 for a receiver
taking none, and at most about a thousand however many it takes, so no
boundary checks their count. Each boundary checks a
[checked](batches-and-readouts.md) stream in those pieces, each before
any of it reaches the receiver, the first before the receiver is called,
so it holds one piece however much its receiver takes at once. A refusal
ends the stream: of its first piece, as a refusal, nothing having run;
of a later piece, as a failure, the pieces before it having run, and a
piece refused by the boundary running nothing if the receiver cuts the
stream by the default. A receiver that takes the stream whole gets it a
checked piece at a time too, and may have run what it pulled before the
refused piece. A panic in the stream's iterator is the sender's, as one
in `each` is.

## Seeds

Every part passes the runner's [`Seed`](../src/seed.rs) on, unchanged, to
each `execute` it makes, and a part that draws seeds its random-number
generators from it.
A replayable part draws each shot from a stream of its own:
`seed.shot(id)` for the executor, and `seed.derive(key).shot(id)` for
another part, such as a noise model, with a key no other part uses, such
as `"bit_flip"`. [`seed.rs`](../src/seed.rs) says which stream each part
takes, why, and how the streams are computed.

A part whose draws for a shot depend on the shots run beside it offers
`Offer::not_replayable`, so that the admission refuses it where a need for
replay reaches it, such as that of a driver that runs a shot again alone,
as [Needs, offers and the admission](admission.md#needs-and-offers) says.
For example, a tensor-network sampler draws a run's shots from one
stream, to draw them in one call rather than one call per shot: a shot
run again alone draws other readouts.
