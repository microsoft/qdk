# Composing and running a chain

For those who compose parts into a chain and run shots on it: how `>>`
joins parts, what a run does, what is checked where and when, how a seed
makes a run repeatable, and how a run fails. The terms are those of the
[README](../README.md#terms), whose
[diagram](../README.md#composing-and-running-a-chain) shows a run's calls.

## Joining parts with `>>`

Each part is wrapped in its role's type, [`Driver`](../src/composition/driver.rs),
[`Adapter`](../src/composition/adapter.rs) or [`Executor`](../src/composition/executor.rs),
and the parts are joined with `>>`, top to bottom. The role of each joined
pair follows from its two ends:

| `>>`                  | gives                                                                                                                                     |
| --------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| driver `>>` adapter   | a driver                                                                                                                                  |
| adapter `>>` adapter  | an adapter                                                                                                                                |
| adapter `>>` executor | an executor                                                                                                                               |
| driver `>>` executor  | a [`Runner`](../src/composition/runner.rs), or every fault of the chain ([`CompositionFaults`](../src/composition/composition_faults.rs)) |

The brackets do not matter: `(d >> a) >> e` and `d >> (a >> e)` are the
same chain, with the same faults and the same readouts. Two parts joined
are a [`Composed`](../src/composition/composed.rs), which a runner's type
names, such as `Runner<Composed<D, A>, E>`. Each `>>` checks the fit of
the boundary it makes and keeps its faults, which the joined part's
`check`, such as [`Driver::check`](../src/composition.rs), reports on a
chain still being built; the `>>` that makes a runner also checks the
admission and, when the chain fits and is admitted, gives a runner, which
keeps the needs that reach each part, for the part to get ready for on
every run.

## Running shots

[`Runner::run`](../src/composition/runner.rs) runs a
[`ShotSet`](../src/batch/shot_set.rs) with a seed, a `u64` or `None`, blocking
the calling thread, and [`Runner::run_async`](../src/composition/runner.rs) is the
same run as a future; either gives the driver's outputs, one per shot by
`Drive`'s rule, which the runner does not check, or a
[`RunError`](../src/composition/runner/run_error.rs), as
[Failures and panics](#failures-and-panics) says. The rustdoc of
[`Runner::run_async`](../src/composition/runner.rs) gives the order in which a run
asks each part whether it is `ready` and ends its shots, and when the
future is `Send`.

## Seeds

A run with a fixed seed gives the same outputs each time, from parts that
draw only from it; a run with no seed draws one, which is not reported.
How a part draws from the run's seed is in
[Writing a part](writing-a-part.md#seeds).

## Where and when each check runs

Each row is a check the protocol makes, where and when it makes it, and
what a fault gives; the last row is the conformance kit's, which checks,
in an implementer's tests, rules a part keeps that the protocol does not
check as it runs:
[Writing a part](writing-a-part.md#checking-a-parts-rules) says which.

| Check                                                                                                                                                                                   | Where                                                                      | When                                                                            | A fault gives                                                                                                                                |
| --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------- | ------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| An instruction set's declaration                                                                                                                                                        | `InstructionSet::new`, `instruction_set!`                                  | when the set is built                                                           | every [`DeclarationFault`](../src/instruction_set.rs) (the `set()` of an `instruction_set!` panics)                                          |
| An instruction built by name                                                                                                                                                            | `InstructionSet::instruction`                                              | when it is built                                                                | the first [`InstructionFault`](../src/instruction_set/instruction.rs)                                                                        |
| A shot set: not empty, no shot twice                                                                                                                                                    | `ShotSet::try_ids`, `ShotSet::ids`                                         | when the set is made                                                            | [`ShotSetFault`](../src/batch/shot_set.rs) (`ids` panics)                                                                                    |
| The fit of the instructions sent to those accepted                                                                                                                                      | each `>>`                                                                  | when the chain is composed                                                      | [`FitFaults`](../src/instruction_set/fit.rs), one per boundary                                                                               |
| The needs at each boundary against the offer there                                                                                                                                      | the `>>` that makes a runner                                               | when the chain is composed                                                      | [`AdmissionFaults`](../src/composition/admission.rs), with the fit faults in [`CompositionFaults`](../src/composition/composition_faults.rs) |
| A run's shots against the fewest any part offers, and a fixed seed against whether every part is seedable                                                                               | [`Runner::run_async`](../src/composition/runner.rs), which `run` waits for | each run, before any part is asked                                              | [`RunRefusal`](../src/composition/runner/run_error.rs): `TooManyShots`, `NotSeedable`                                                        |
| Whether each part can take a run now                                                                                                                                                    | each part's `ready`, bottom to top                                         | each run, before the driver starts                                              | [`NotReady`](../src/roles/not_ready.rs), as `RunRefusal::PartNotReady`, naming the part                                                      |
| A batch's count of instructions against the most its receiver offers                                                                                                                    | every boundary                                                             | each batch, checked or not, before the receiver runs any batch of the `execute` | [`ExecuteRefusal::TooManyInstructions`](../src/roles/execute.rs)                                                                             |
| A batch's instructions against the receiver's set and the shots' needs, and its [readout slots](batches-and-readouts.md#readout-slots): countable in a `u32`, and equal to those stated | every boundary                                                             | each checked batch, before the receiver runs any batch of the `execute`         | [`ExecuteRefusal`](../src/roles/execute.rs): `Instruction`, `TooManyReadoutSlots`, `ReadoutSlotsMisstated`                                   |
| The rules a part keeps that the kit checks                                                                                                                                              | the [conformance kit](writing-a-part.md#checking-a-parts-rules)            | when the implementer's tests run                                                | a [`Nonconformance`](../../executor_protocol_conformance/src/nonconformance.rs): the rules broken, each place, or why it cannot judge        |

## Failures and panics

A run gives the driver's outputs, or a
[`RunError`](../src/composition/runner/run_error.rs): `Refused`, with a
[`RunRefusal`](../src/composition/runner/run_error.rs), when nothing of the run ran, so that it
can be corrected and sent again; or `Failed`, with a
[`DriveError`](../src/roles/drive.rs) that names the shot and its
[`Failure`](../src/roles/failure.rs). An `execute` answers `Ok`, or an
[`ExecuteError`](../src/roles/execute.rs): `Refused`, with an
[`ExecuteRefusal`](../src/roles/execute.rs), when nothing of it ran, or
`Failed`, naming the shot and its `Failure`. A driver passes either up
with `?`, and a refusal from below then fails the run as
`Failure::Refused`.

A `Failure` is a part's own error (`Part`), such as a program's trap; a
refusal once the run started (`Refused`); or a part's panic (`Panicked`),
where a panic unwinds. A panic names the part whose call panicked, by its
position and its name; the rustdoc of [`Failure`](../src/roles/failure.rs) lists
the few that name instead the part above, whose call they unwound
through, and that of [`Runner::run_async`](../src/composition/runner.rs) those raised
again from the run, such as one in a part's `ready`.
