# Capabilities and their fit

For those who state what a part requires or provides, write an adapter
that changes either, or read a chain's composition faults or a run's
refusal: what a part states, what it leaves to the parts around it, and
where each statement is checked. The terms are those of the
[README](../README.md#terms).

## Terms

A boundary has two sides, and one type states both, so that one check
compares them:

- **Capabilities** ([`Capabilities`](../src/capabilities.rs)): an
  instruction set, amounts ([`Resources`](../src/capabilities/resources.rs))
  and yes-or-no features ([`Features`](../src/capabilities/features.rs)).
- **Requirement**: the capabilities a part requires of the part below it:
  the instructions it sends, and the resources and features its shots and
  runs use.
- **Provision**: the capabilities a part provides the part above it: the
  instructions it accepts, and the resources and features it serves.
- **Fit**: the check that a requirement is served by a provision,
  [`BoundaryCompatible::fits`](../src/boundary_compatible.rs), one relation
  for the instructions, and for the capabilities, whose fit compares their
  resources and features too.
- **Forwarded**: a resource or a feature a part does not state. Its
  statement is then the one of the part next to it, as
  [Forwarding](#forwarding) says.

## What a part states

A driver states its requirement (`Driver::requires`), an executor its
provision (`Executor::provides`), and an adapter both, since it is a
receiver above and a sender below. For instance, the example's executor,
from [`examples/compose_and_run.rs`](../examples/compose_and_run.rs),
provides its set on 64 qubits, at most 1000 shots a run, and no readout
parity:

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

A replayed shot runs again with its run's seed, so a part that requires
replay, or provides it, states seeded runs too, and one that states no
seeded runs states no replay (`Features::with_replay`,
`Features::with_seeded_runs`).

An executor that cannot host a type of block states 0 of it: an amount a
provider does not state is not checked. A checked batch, though, may use
only the blocks of the types its sender's requirement, as forwarded to
that boundary, states: a type it does not state, it uses none of.

## Forwarding

An adapter states only what it changes: a noise model, the instructions
alone; a code's lowering, its logical and physical qubits; a job queue,
its most shots. What an adapter does not state passes through it. A
requirement it does not state is the requirement of the part above it,
and a provision it does not state that of the part below it, so each
part's statement meets the statements it reaches:

| What an adapter states of a field | For instance                                                  | The part above it is checked against  | The part below it gets ready for |
| --------------------------------- | ------------------------------------------------------------- | ------------------------------------- | -------------------------------- |
| neither side                      | a noise model, a profiler                                     | the provisions below it               | the requirements above it        |
| a provision alone                 | a job queue's most shots                                      | its provision, and those below it     | the requirements above it        |
| a requirement alone               | a splitter's instructions per batch                           | nothing, for that field: it serves it | its requirement alone            |
| both sides                        | a lowering's qubits; an adapter that replays its shots itself | its provision alone                   | its requirement alone            |

So a requirement is checked against every provision below it, down to the
first part that requires the field again, which serves the field itself.
A feature required `false` is a requirement too: an adapter that states
that it does not use seeded runs, and so no replay, answers both for the
parts above it. A run's limits follow, with no rule of their own: a run's
shots, and a fixed seed, are checked against what the parts below the
driver provide, forwarded this way, such as the fewest shots of those
that state them, down to the first adapter that requires shots again; a
field the driver requires, it serves itself, as such an adapter does. A
joined driver checks a run against its own parts by the same rule, so the
verdict does not depend on the brackets, and holds inside a part of the
user's, which the runner cannot see into, as
[Composing and running](composing-and-running.md#joining-parts-with-through-and-on)
says. An adapter may lower a provision but never raise it, unless it
requires the field too: an adapter that replays its shots provides
replay, and requires none below.

The faults, and the parts they name, do not depend on how the joins are
bracketed: each requirement is checked against the same provisions, and a
joined part requires and provides what its parts forward.

## Where each statement is checked

| What                                                                                                                        | When                                  | A fault gives                                                                                                                            |
| --------------------------------------------------------------------------------------------------------------------------- | ------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| each part's requirement against each provision it reaches                                                                   | the `on` that makes a runner          | [`CompositionFaults`](../src/composition/composition_faults.rs): each requiring part and the part that falls short, by position and name |
| a run's shots, and a fixed seed, against what the parts below the driver provide, of the fields the driver does not require | each run, before any part is asked    | [`RunRefusal::NotProvided`](../src/composition/runner/run_error.rs), with the mismatch                                                   |
| the same, against a joined driver's own parts, inside a part of the user's                                                  | each run, as the joined driver drives | [`Failure::NotProvided`](../src/roles/failure.rs), with the mismatch, at the run's first shot                                            |
| a batch's count of instructions against the instructions per batch provided                                                 | each batch, at every boundary         | [`BatchFault::TooManyInstructions`](../src/capabilities/boundary/batch_limits.rs)                                                        |
| a checked batch's blocks and readout parities against what its sender requires                                              | each checked batch, at every boundary | [`BatchFault::Instruction`](../src/capabilities/boundary/batch_limits.rs)                                                                |

Each part gets ready, on every run, for the requirement that reaches it
(`ready`): an executor, for instance, holds the qubits the parts above
require.
