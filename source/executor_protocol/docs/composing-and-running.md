# Composing and running a chain

For those who compose parts into a chain and run shots on it: how the
joins join parts, what a run does, what is checked where and when, how a
seed makes a run repeatable, and how a run fails. The terms are those of
the [README](../README.md#terms), whose
[diagram](../README.md#composing-and-running-a-chain) shows a run's calls.

## Joining parts with `through` and `on`

Each part implements its role's trait,
[`Driver`](../src/roles/driver.rs), [`Adapter`](../src/roles/adapter.rs)
or [`Executor`](../src/roles/executor.rs), and heads the joins of its
role, [`DriverJoins`](../src/composition/driver_joins.rs) or
[`AdapterJoins`](../src/composition/adapter_joins.rs), which every driver
and every adapter has and the [prelude](../src/prelude.rs) brings into
scope. The parts are joined top to bottom: `through` an adapter, `on` an
executor. The role of each joined pair follows from its two ends:

| Join                       | Gives                                                                                                                                                 |
| -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `driver.through(adapter)`  | a driver                                                                                                                                              |
| `adapter.through(adapter)` | an adapter                                                                                                                                            |
| `adapter.on(executor)`     | an executor                                                                                                                                           |
| `driver.on(executor)`      | a [`Runner`](../src/composition/runner.rs), or every fault of the chain ([`CompositionFaults`](../src/composition/chain_check/composition_faults.rs)) |

The brackets do not matter: `d.through(a).on(e)` and `d.on(a.on(e))` are
the same chain, with the same faults and the same readouts. A joined
part's type cannot be named: it is used by its role alone, and a function
that returns a runner names it `Runner<impl Driver<Output = O>, E>`. Each
join checks the fit of the boundary it makes and keeps its faults; the
`on` that makes a runner also checks the admission and reports every
fault of the chain at once, top to bottom. When the chain fits and is
admitted, it gives a runner, which keeps the needs that reach each part,
for the part to get ready for on every run.

A joined part keeps its role's contract through its role's methods alone:
held inside a part of the user's that forwards only those methods, or lent
by `&mut` to another chain, it runs the same. The `on` that makes a runner
cannot reach the parts inside a part of the user's, so a joined part there
whose own parts do not fit refuses every run at `ready`, with its fit
faults, rather than running.

## Running shots

[`Runner::run`](../src/composition/runner.rs) runs a
[`ShotSet`](../src/batch/shot_set.rs) with a seed, a `u64` or `None`, blocking
the calling thread, and [`Runner::run_async`](../src/composition/runner.rs) is
the same run as a future; either gives the driver's outputs, one per shot by
[`Driver::drive`](../src/roles/driver.rs)'s rule, which the runner does not
check, or a [`RunError`](../src/composition/runner/run_error.rs), as [Failures
and panics](#failures-and-panics) says. The rustdoc of
[`Runner::run_async`](../src/composition/runner.rs) gives the order in which a
run asks each part whether it is `ready` and ends its shots, and when the future
is `Send`.

## Seeds

A run with a fixed seed gives the same outputs each time, from parts that
draw only from it; a run with no seed draws one, which is not reported.
How a part draws from the run's seed is in
[Writing a part](writing-a-part.md#seeds).

## Where and when each check runs

Each row is a check the protocol makes, where and when it makes it, and
what a fault gives. The rules a part keeps, which the protocol does not
check, are in
[Writing a part](writing-a-part.md#the-rules-a-part-keeps).

| Check                                                                                                                                                                                   | Where                                                                      | When                                                                            | A fault gives                                                                                                                                                        |
| --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------- | ------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| An instruction set's declaration                                                                                                                                                        | `InstructionSet::new`, `instruction_set!`                                  | when the set is built                                                           | every [`DeclarationFault`](../src/instruction_set.rs) (the `set()` of an `instruction_set!` panics)                                                                  |
| An instruction built by name                                                                                                                                                            | `InstructionSet::instruction`                                              | when it is built                                                                | the first [`InstructionFault`](../src/instruction_set/instruction.rs)                                                                                                |
| A shot set: not empty, no shot twice                                                                                                                                                    | `ShotSet::try_ids`, `ShotSet::ids`                                         | when the set is made                                                            | [`ShotSetFault`](../src/batch/shot_set.rs) (`ids` panics)                                                                                                            |
| The fit of the instructions sent to those accepted                                                                                                                                      | each join                                                                  | when the parts are joined                                                       | [`FitFaults`](../src/instruction_set/fit.rs), one per boundary; inside a part of the user's, a refusal of every run at `ready`                                       |
| The needs at each boundary against the offer there                                                                                                                                      | the `on` that makes a runner                                               | when the parts are joined                                                       | [`AdmissionFaults`](../src/composition/chain_check/admission.rs), with the fit faults in [`CompositionFaults`](../src/composition/chain_check/composition_faults.rs) |
| A run's shots against the fewest any part offers, and a fixed seed against whether every part is seedable                                                                               | [`Runner::run_async`](../src/composition/runner.rs), which `run` waits for | each run, before any part is asked                                              | [`RunRefusal`](../src/composition/runner/run_error.rs): `TooManyShots`, `NotSeedable`                                                                                |
| Whether each part can take a run now                                                                                                                                                    | each part's `ready`, bottom to top                                         | each run, before the driver starts                                              | [`NotReady`](../src/roles/not_ready.rs), as `RunRefusal::PartNotReady`, naming the part                                                                              |
| A batch's count of instructions against the most its receiver offers                                                                                                                    | every boundary                                                             | each batch, checked or not, before the receiver runs any batch of the `execute` | [`ExecuteRefusal::TooManyInstructions`](../src/roles/executor.rs)                                                                                                    |
| A batch's instructions against the receiver's set and the shots' needs, and its [readout slots](batches-and-readouts.md#readout-slots): countable in a `u32`, and equal to those stated | every boundary                                                             | each checked batch, before the receiver runs any batch of the `execute`         | [`ExecuteRefusal`](../src/roles/executor.rs): `Instruction`, `TooManyReadoutSlots`, `ReadoutSlotsMisstated`                                                          |

## Failures and panics

A run gives the driver's outputs, or a
[`RunError`](../src/composition/runner/run_error.rs): `Refused`, with a
[`RunRefusal`](../src/composition/runner/run_error.rs), when nothing of the run ran, so that it
can be corrected and sent again; or `Failed`, with a
[`DriveError`](../src/roles/driver.rs) that names the shot and its
[`Failure`](../src/roles/failure.rs). An `execute` answers `Ok`, or an
[`ExecuteError`](../src/roles/executor.rs): `Refused`, with an
[`ExecuteRefusal`](../src/roles/executor.rs), when nothing of it ran, or
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
