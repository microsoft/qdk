# Writing a part

For those who write a part, a driver, an adapter or an executor: what each
role's methods do at each phase of a chain's life, an executor in full,
the rules a part keeps that the protocol does not check and how the
conformance kit checks them, and how a part draws from the run's seed, or
says it cannot replay a shot. What crosses a boundary is in
[Batches and readouts](batches-and-readouts.md); what `>>` checks of a
part, in [Instruction sets and their fit](instruction-sets.md) and
[Needs, offers and the admission](admission.md). The terms are those of
the [README](../README.md#terms).

## The roles' methods, phase by phase

A part implements its role's two traits: a setup trait, whose methods
`>>` calls when it composes the chain (the first two phases below), and
a run-time trait, whose methods the runner calls on each run (the last
three). Each method is called at one phase of a chain's life:

| Phase                                       | [`DriverSetup`](../src/roles/driver_setup.rs) and [`Drive`](../src/roles/drive.rs), the driver        | [`AdapterSetup`](../src/roles/adapter_setup.rs) and [`Adapt`](../src/roles/adapt.rs), an adapter                                            | [`ExecutorSetup`](../src/roles/executor_setup.rs) and [`Execute`](../src/roles/execute.rs), the executor |
| ------------------------------------------- | ----------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| Each `>>`: the fit                          | `sends`, and `bind`, which takes the receiver's opcodes                                               | `accepts`, `sends`, and `bind`                                                                                                              | `accepts`                                                                                                |
| The `>>` that makes a runner: the admission | `needs`                                                                                               | `needs_below` and `offers_above`, which adapt the needs passed down and the offer passed up, unchanged by default                           | `offers`                                                                                                 |
| Each run, before any shot                   | `ready()`, asked last: a driver knows its own needs                                                   | `ready(above)`, for the needs of the part above, asked after the parts below it                                                             | `ready(needs)`, for the needs that reach it, asked first                                                 |
| The run                                     | `drive(shots, seed, below)`: calls `below.execute` until each shot ends, and gives each shot's output | `adapt(batches, seed, below, readouts)`: answers each `execute` by calling `below.execute` as often as it needs, and maps the readouts back | `execute(batches, seed, readouts)`: runs each batch on its shots' states, and fills their readouts       |
| Once the run ends                           | nothing: the runner ends the shots below, and a driver may end a shot earlier                         | `end_shots`, called top to bottom, which frees the shots' state, and never passes the end below                                             | `end_shots`, called last, which frees the shots' state                                                   |

Each setup trait also has `name`, the part's name in messages, by default
its type's.

## An executor, in full

The executor of
[`examples/compose_and_run.rs`](../examples/compose_and_run.rs), `Bits`,
implements [`ExecutorSetup`](../src/roles/executor_setup.rs) and
[`Execute`](../src/roles/execute.rs). `CoinGates` is the
example's instruction set, and `Stream` a shot's random stream, both in
the file; `without_readout_parity` says it cannot resolve an argument that
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
                let (bits, stream) = self
                    .shots
                    .entry(id)
                    .or_insert_with(|| (vec![false; qubits], Stream(seed.shot(id))));
                let mut row = batch_readouts.row_mut(position).iter_mut();
                for instruction in batch.instructions {
                    // Below the shot's qubits, as the parts above send them;
                    // a checked batch's checks refuse any other.
                    let qubit = instruction.blocks[0] as usize;
                    match instruction.opcode {
                        CoinGates::H => bits[qubit] = stream.chance(0.5),
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

The protocol does not check these, except where one says so; the rustdoc
of each role, and of [`Seed`](../src/seed.rs) for the seeds, states them:

- its instruction sets, needs and offer do not change once it is composed,
  since the fit and the admission rely on those contracts staying the
  same, as do the needs each part gets ready for on every run;
- a part that calls `execute` puts a shot in at most one of its batches,
  and only shots it was given: the run's, for a driver, those of the
  batches it got, for an adapter;
- a part that calls `execute` sends no shot again once it ended it below,
  or once the shot failed in an earlier `execute`, which the part below
  may refuse ([`ExecuteRefusal`](../src/roles/execute.rs): `ShotNotHeld`,
  `EarlierFailure`);
- a part that calls `execute` states each batch's readout slots as its
  instructions count them, since the part below shapes its readouts by
  that count; the protocol checks it only on a checked batch, as
  [Where and when each check runs](composing-and-running.md#where-and-when-each-check-runs)
  says;
- shots are independent: an executor may run an `execute`'s batches in any
  order, or at once, and may share work between shots, such as one state
  for shots whose instructions are equal so far, as long as each shot's
  readouts are those it would get run alone; it leaves untouched every
  shot that is in none of the batches;
- for a replayable part, a shot's records depend only on the chain, the
  run's seed and the shot's id: not on the shots run beside it, or how its
  instructions are put in batches;
- an executor makes a shot's state at its first batch, from the needs it
  got ready for and the shot's seed, and keeps it from one `execute` to
  the next until `end_shots`;
- a part's `end_shots` does nothing for shots it does not hold, or no
  longer holds, since the runner ends every shot of a run, some of which
  the driver may have ended already, and in a failed run, some never sent;
- an adapter's `needs_below` is enough to serve the needs above, and
  states every block it takes below, since a refusal names the parts by
  it;
- an executor, and an adapter, writes every value of the readouts it
  answers, and keeps their shape;
- an adapter sends the batches it makes for a checked batch checked
  ([`Batch::checked_if`](../src/batch.rs)), so that its sender's checks
  reach every boundary below;
- a row reports only the conditions raised on its shot in that `execute`
  ([`ShotConditions`](../src/batch/batch_readouts/shot_conditions.rs)): an
  adapter merges into each row those read below on its shot with its own,
  and a driver merges each `execute`'s into the shot's output; a part,
  adapter or driver, raises its own for what it finds in its readouts,
  never because of a condition read below, in that `execute` or an
  earlier one, since the driver already holds that condition and a part
  that wants to flag a shot reads its reason in the readouts;
- a driver gives one output per shot, in the order of the run's shots;
- a part that draws takes its streams from the run's seed, one per shot
  if it is replayable, as [Seeds](#seeds) says;
- a `drive`, `adapt` or `execute` does its work when its future is first
  polled, not when it is called;
- a part takes calls again after it failed or panicked, since the next run
  calls every part as before.

### Checking a part's rules

The conformance kit,
[`executor_protocol_conformance`](../../executor_protocol_conformance/README.md),
checks most of these rules in an implementer's tests (the kit's
[limits](../../executor_protocol_conformance/docs/limits.md) lists those it
does not), and reports each place where a part breaks one. Anyone writing
a driver, or a seedable executor or adapter, runs it: every run of a check
fixes its seed, so the kit cannot judge an executor or an adapter that is
not seedable.
[The checks](../../executor_protocol_conformance/README.md#the-checks), in
the kit's README, lists what each role's checks cover.

## Seeds

Every part passes the runner's [`Seed`](../src/seed.rs) on, unchanged, to
each `execute` it makes, and a part that draws takes its streams from it.
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
