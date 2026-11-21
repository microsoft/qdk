# Capabilities and their fit

For those who state what a part requires or provides, or read a chain's
composition faults or a run's refusal: what a part states, how far each
statement reaches down the chain, and where each is checked. The terms
are those of the [README](../README.md#terms).

## Terms

A boundary has two sides, and one type states both, so that one check
compares them:

- **Capabilities** ([`Capabilities`](../src/capabilities.rs)): an
  instruction set, amounts ([`Resources`](../src/capabilities/resources.rs))
  and yes-or-no features ([`Features`](../src/capabilities/features.rs)).
  Only the instruction set is mandatory: resources and features start
  with nothing stated, so `Capabilities::new(set)` fits any part on them.
- **Requirement**: the capabilities a part requires of the parts below
  it: the instructions it sends, and the resources and features its shots
  and runs use. A field it does not state, it does not need.
- **Provision**: the capabilities a part provides the parts above it: the
  instructions it accepts, and the resources and features it takes. A
  field it does not state, it does not limit.
- **Limit**: a provision of a resource or a feature that the part passes
  through to the parts below, which still apply: a job queue that takes
  at most 10 shots a run.
- **Served**: a provision of a resource or a feature that the part
  handles itself ([`Statements::serving`](../src/capabilities/statements.rs)),
  so that what is below does not bound it: a splitter that sends a batch
  of any count as batches of one. An executor's provisions are served,
  since nothing is below it.
- **Fit**: the check that a requirement is met by a provision,
  [`BoundaryCompatible::fits`](../src/boundary_compatible.rs), one relation
  for the instructions, and for the capabilities, whose fit compares their
  resources and features too.

## What a part states

Each field of a resource or a feature is stated on its own, and what a
writer states of it depends only on what the part does with it, not on
the rest of the chain:

| Your part…                                             | States of the field                                               |
| ------------------------------------------------------ | ----------------------------------------------------------------- |
| passes the field through untouched                     | nothing                                                           |
| passes it through, but takes at most v                 | a limit of v, provided                                            |
| passes it through, and needs v below for its own work  | a requirement of v                                                |
| handles it itself: splits, lowers, replays or seeds it | that it serves up to v (or any), and requires what it sends below |

A driver states its requirement (`Driver::requires`), and what of a run
it serves itself (`Driver::provides`, nothing by default); an executor
states its provision (`Executor::provides`); an adapter both
(`Adapter::requires`, `Adapter::provides`), since it is a receiver above
and a sender below. For instance, the example's executor, from
[`examples/compose_and_run.rs`](../examples/compose_and_run.rs), provides
its set on 64 qubits, at most 1000 shots a run, and no readout parity:

```rust
let resources = Resources::new().with_qubits(64).with_shots(1000);
let features = Features::new().with_readout_parity(false);
Self {
    provides: Capabilities::new(CoinGates::set())
        .with_resources(resources)
        .with_features(features),
```

| Kind         | Each field                                                                     | Fits when                                                                         | A mismatch                                                                |
| ------------ | ------------------------------------------------------------------------------ | --------------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| Instructions | the set sent or accepted                                                       | each instruction sent is accepted, as [the fit](instruction-sets.md#the-fit) says | each [`InstructionMismatch`](../src/instruction_set/fit.rs)               |
| Resources    | blocks of each type (qubits among them), shots per run, instructions per batch | the amount required is at most the amount provided, or either is not stated       | [`ResourceMismatch`](../src/capabilities/resources.rs), with both amounts |
| Features     | readout parity, replay, seeded runs                                            | the feature is not required, or the provider does not say it lacks it             | [`FeatureMismatch`](../src/capabilities/features.rs)                      |

A replayed shot runs again with its run's seed, so stating replay `true`
states seeded runs `true` too, and stating seeded runs `false` states
replay `false` (`Features::with_replay`, `Features::with_seeded_runs`).
Where a part provides replay over parts that provide no seeded runs, the
chain provides no replay.

An executor that cannot host a type of block states 0 of it: an amount a
provider does not state is not checked. A checked batch, though, may use
only the blocks of the types its sender's requirement, as it reaches that
boundary, states: a type it does not state, it uses none of.

## How far a requirement reaches

A requirement reaches every provision stated below it, down to and
including the first part that serves the field, and must fit each of
them: like a path through a network, where every link's limit applies end
to end, and a router that splits packets handles their size itself. The
`on` that makes a runner reports every pair that fails at once, naming the
part that requires and the part that falls short. Each part then gets
ready, on every run, for the largest requirement that reaches it.

For instance, a program on 6 qubits, through a decoder limiting them to
4, then a part reserving an ancilla, which serves 6 and requires 7, over
bits of 5 (from
[`src/tests/composition/driver_joins.rs`](../src/tests/composition/driver_joins.rs)):
the program falls short at the decoder, and the ancilla's part at the
bits; the program's 6 qubits stop at the part that serves them.

```rust
let d = program(6, Features::new());
let c = Stating::restating(4, &log);
let w = Stating::reserving_an_ancilla(6, &log);
let e = bits_over(5, &log);
```

```rust
vec![
    fault(part::<Program>(0), part::<Stating>(1), short_qubits(6, 4)),
    fault(part::<Stating>(2), part::<Bits>(3), short_qubits(7, 5)),
],
```

Other cases, each in the same tests:

| Chain, of one field                                                                          | Verdict                                                  |
| -------------------------------------------------------------------------------------------- | -------------------------------------------------------- |
| a program on 5 qubits, through a profiler stating nothing, over bits of 4                    | refused, naming the program and the bits                 |
| the same, over bits stating no qubits                                                        | composes; the bits get ready for 5                       |
| a program of runs of 5 shots, through a queue limiting them to 10, over bits of 3            | refused, naming the program and the bits                 |
| the same, with runs of 12                                                                    | refused twice: the program at the queue, and at the bits |
| a program on 5 qubits, through an interleaver requiring 3, over bits of 4                    | refused, naming the program and the bits                 |
| a program of batches of 100 instructions, through a splitter serving any, over bits taking 1 | composes                                                 |
| the same, through a part that requires batches of 1 and serves none                          | refused, naming the program and the bits                 |
| a debugger requiring replay, through a replayer serving it, over bits that do not replay     | composes                                                 |

A run's shots, and a fixed seed, are the topmost requirement: they reach
the driver's own provision first (`Driver::provides`), then each part
below it down to the first that serves them, as the `Runner` checks when
each run starts. A driver that splits a run into runs of the 4 shots it
requires serves a run's shots, so a run of 10 runs (from
[`src/tests/composition/runner.rs`](../src/tests/composition/runner.rs)):

```rust
let serves_shots =
    RunProvision::new().with_resources(Resources::new().serving(Resource::Shots));
let program = Program::new(&[("mz", 0)])
    .with_resources(qubits(1).with_shots(4))
    .serving_runs(serves_shots);
```

A joined part, a stack of adapters or a driver joined through some,
provides what its parts provide together, and serves a field if any part
inside it serves it; it requires what its parts' requirements, each
reaching down to the first part inside that serves it, ask of the parts
below. So the verdict, the positions the faults name and their
mismatches do not depend on how the joins are bracketed, and they hold
for a part of the user's that holds a joined part and forwards what it
requires and provides, the driver's `provides` included. The names a
fault gives are read by the final `on`, so they agree across bracketings
when each part's name stays the same through composition, its `bind`
included.

## Where each statement is checked

| What                                                                                                        | When                                   | A fault gives                                                                                                                            |
| ----------------------------------------------------------------------------------------------------------- | -------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| each part's requirement against each provision it reaches                                                   | the `on` that makes a runner           | [`CompositionFaults`](../src/composition/composition_faults.rs): each requiring part and the part that falls short, by position and name |
| a joined part's halves against each other, inside a part of the user's, which the `on` cannot reach         | each run, by the joined part's `ready` | [`RunRefusal::PartNotReady`](../src/composition/runner/run_error.rs), with the halves' mismatch                                          |
| a run's shots, and a fixed seed, against the driver's provision and each provision below it they reach      | each run, before any part is asked     | [`RunRefusal::NotProvided`](../src/composition/runner/run_error.rs), with the mismatch                                                   |
| the same, against a joined driver's own parts, inside a part of the user's that does not forward `provides` | each run, as the joined driver drives  | [`Failure::NotProvided`](../src/roles/failure.rs), with the mismatch, at the run's first shot                                            |
| a batch's count of instructions against the instructions per batch provided                                 | each batch, at every boundary          | [`BatchFault::TooManyInstructions`](../src/capabilities/boundary/batch_limits.rs)                                                        |
| a checked batch's blocks and readout parities against what its sender requires                              | each checked batch, at every boundary  | [`BatchFault::Instruction`](../src/capabilities/boundary/batch_limits.rs)                                                                |
