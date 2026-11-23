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
executor, each where the instruction set the upper part sends is, as a
type, the set the lower part accepts (otherwise a
[translating adapter](instruction-sets.md#running-one-set-on-another)
joins them).
The role of each joined pair follows from its two ends:

| Join                       | Gives                                                                                                                                     |
| -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `driver.through(adapter)`  | a driver                                                                                                                                  |
| `adapter.through(adapter)` | an adapter                                                                                                                                |
| `adapter.on(executor)`     | an executor                                                                                                                               |
| `driver.on(executor)`      | a [`Runner`](../src/composition/runner.rs), or every fault of the chain ([`CompositionFaults`](../src/composition/composition_faults.rs)) |

The brackets do not matter: `d.through(a).on(e)` and `d.on(a.on(e))` are
the same chain, with the same faults and the same readouts (inside a part
of the user's, which boundary a run's refusal reports may differ, as below
says). A fault names
its parts by position, the same in each bracketing, and by the name each
part gives at the final `on`, the same where a part's name does not change
as it is joined, its `bind` included. A joined
part's type cannot be named: it is used by its role alone, and a function
that returns a runner names it
`Runner<impl Driver<Sends = S, Output = O>, E>`. Each
join fits the boundary it makes and binds its sender where it fits; the
`on` that makes a runner checks what arrives at each part against what it
provides, as [Capabilities](capabilities.md) says, and reports every fault
it can see at once, top to bottom (inside a part of the user's, the checks
run on each run, as below says). When the chain fits, it gives a runner,
and on every run each part gets ready for what arrives at it.

A joined part keeps its role's contract through its role's methods alone.
Held inside a part of the user's that forwards only those methods, it gives
the same outputs, gets ready for the same requirement and has its shots
ended alike; a panic or a refusal inside it is named by the user's part,
at its position. The `on` that makes a runner sees such a joined part by
its public methods alone: its `requires` gives what its parts require in
turn, so what it passes below is checked as unheld, but its `provides`
gives its top part's limits alone. So the joined part checks the rest
itself at `ready`, on each run that reaches it: its halves' instructions,
by the fit its join saved, and what arrives at its lower part on that run
against what that part provides. A fault there, it reports as a refusal
at `ready`, with every mismatch of the boundary it checks, which can
include a run's shots or seed that the `on`, checking with nothing
arriving, does not see. It checks the boundary between its halves first,
and one inside a half only once that fits, so two bracketings of a nest
so held may report different boundaries; and the run's check, or a part
asked before it, may refuse the run first. The same parts unheld are
refused by the `on`, or by the run's check, whose faults do not depend on
how the joins are bracketed. A joined
adapter or executor lent by `&mut` to another chain is checked again by
that chain's `on`.

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

| Check                                                                                                                                                                                               | Where                                                                      | When                                                                            | A fault gives                                                                                                                                      |
| --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------- | ------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| A part's role; the set a part sends is, as a type, the set the part below accepts; a joined part's type cannot be named                                                                             | the compiler                                                               | when the chain is compiled                                                      | a compile error                                                                                                                                    |
| A shot set: not empty, no shot twice                                                                                                                                                                | `ShotSet::try_ids`, `ShotSet::ids`                                         | when the set is made                                                            | [`ShotSetFault`](../src/batch/shot_set.rs) (`ids` panics)                                                                                          |
| The instructions each part sends against those the part below accepts, and what arrives at each part against its provision ([Capabilities](capabilities.md))                                        | the `on` that makes a runner                                               | when the parts are joined                                                       | every fault it can see, [`CompositionFaults`](../src/composition/composition_faults.rs)                                                            |
| The same, inside a joined part held by a part of the user's: its halves' instructions, by the fit its join saved, and what arrives at its lower part on the run                                     | the joined part's `ready`                                                  | each run that reaches it, its halves' boundary first                            | [`RunRefusal::PartNotReady`](../src/composition/runner/run_error.rs), with the mismatch                                                            |
| What a run's shots, and seeded runs if its seed is fixed, make arrive at each part, against its provision                                                                                           | [`Runner::run_async`](../src/composition/runner.rs), which `run` waits for | each run, before any part is asked                                              | [`RunRefusal::NotProvided`](../src/composition/runner/run_error.rs), with every fault                                                              |
| Whether each part can take a run now                                                                                                                                                                | each part's `ready`, bottom to top                                         | each run, before the driver starts                                              | [`NotReady`](../src/roles/not_ready.rs), as `RunRefusal::PartNotReady`, naming the part                                                            |
| A batch's count of instructions against the instructions per batch its receiver provides                                                                                                            | every boundary                                                             | each batch, checked or not, before the receiver runs any batch of the `execute` | [`BatchFault::TooManyInstructions`](../src/capabilities/boundary/batch_limits.rs)                                                                  |
| A batch's instructions against the receiver's set and what arrives at the receiver, and its [readout slots](batches-and-readouts.md#readout-slots): countable in a `u32`, and equal to those stated | every boundary                                                             | each checked batch, before the receiver runs any batch of the `execute`         | [`BatchFault`](../src/capabilities/boundary/batch_limits.rs): `Instruction` (with the set's fault), `TooManyReadoutSlots`, `ReadoutSlotsMisstated` |

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
refusal once the run started (`Refused`); or a part's panic
(`Panicked`), where a panic unwinds. A panic names the part whose
call panicked, by its position and its name; the rustdoc of
[`Failure`](../src/roles/failure.rs) lists the few that name instead the
part above, whose call they unwound through, and that of
[`Runner::run_async`](../src/composition/runner.rs) those raised again from
the run, such as one in a part's `ready`.
