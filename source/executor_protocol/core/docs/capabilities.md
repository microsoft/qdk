# Capabilities and their fit

For those who state what a part requires or provides, or read a chain's
composition faults or a run's refusal: what a part states, what arrives at
each part, and where each statement is checked. The terms are those of the
[README](../README.md#terms).

## By default, nothing

A part that does not state a capability composes with any part on it, so most
writers write only the instructions their part accepts and sends
(`accepts`, `sends`, as [Instruction sets](instruction-sets.md) says). The
three defaults:

| Method                                        | By default                                        | So the part                         |
| --------------------------------------------- | ------------------------------------------------- | ----------------------------------- |
| `Driver::requires(run)`                       | `run`                                             | passes the run below, needs nothing |
| `Adapter::requires(above)`                    | `above`                                           | passes below what arrives           |
| `Adapter::provides()`, `Executor::provides()` | the set's `unconstrained()`, which states nothing | limits nothing                      |

What a part leaves unstated, composition cannot refuse a chain on: a
driver that does not state the batches it sends composes over an
executor that takes smaller ones, and its batches are refused as it runs.

## Terms

- **Capabilities** ([`Capabilities`](../src/capabilities.rs)): amounts
  ([`Resources`](../src/capabilities/resources.rs)) and yes-or-no features
  ([`Features`](../src/capabilities/features.rs)), which every chain
  states, each built from its `unconstrained()`, which states nothing; and
  the instruction set's own capabilities, such as the qubits a shot holds,
  as [Instruction sets](instruction-sets.md) says. `Capabilities::new`
  takes all three.
- **Requirement**: what a part requires of the part below it, given what
  arrives at it (`Adapter::requires(above)`); the driver's, given the run
  (`Driver::requires(run)`): the run's shots, and seeded runs when its seed
  is fixed, which the runner states
  ([`RunRequirement`](../src/capabilities/run_requirement.rs), of no
  instruction set), with what its shots use of the set it sends.
- **Provision**: a part's fixed limits on what arrives at it
  (`Adapter::provides`, `Executor::provides`). A field it does not state,
  it does not limit.
- **What arrives** at a part: the requirement of the part above it, given
  what arrived there; at the part below the driver, the driver's
  requirement.
- **Fit**: the check that what arrives at a part is met by its provision,
  [`BoundaryCompatible::fits`](../src/boundary_compatible.rs), the one
  relation the instructions are checked by too.

## The rule

What arrives at each part must fit what it provides. A part states its
fixed limits in `provides`, and what it passes below in `requires`, a
function of what arrives at it. What a writer states depends only on what
the part does, not on the rest of the chain:

| Your part…                                                  | `provides`           | `requires(above)`                              |
| ----------------------------------------------------------- | -------------------- | ---------------------------------------------- |
| passes a field through untouched, such as a logger          | nothing              | `above`, the default                           |
| takes at most v of it, such as a job queue or a decoder     | v                    | `above`, the default                           |
| needs at least v of it below, such as an interleaver        | nothing              | `above`, with the larger of the field and v    |
| needs more below than arrives, such as an ancilla's reserve | nothing, or the most | `above`, with the field it needs below         |
| handles it itself, such as a job splitter or a replayer     | nothing, or the most | `above`, with what it sends below of the field |

So a value a part passes unchanged arrives at each part below it, down to
the first that changes it, and is checked at each. The `on` that makes a
runner reports every part it does not fit at once, naming the part whose
requirement it is, and the part that falls short: for a value passed
unchanged, the part that first required it, so a logger inserted between
them moves only the positions the fault names. Each part then gets ready,
on every run, for what arrives at it.

A `requires` is deterministic, and monotone: given what arrives requiring
no more than another, as [`Capabilities`](../src/capabilities.rs) defines
it, it requires no more than given the other. The protocol does not check
either, and the checks rely on both:

- monotone, because the `on` that makes a runner checks the chain it sees
  with nothing arriving at its top, which requires no more than any run: a
  part short then is short on every run, so the chain is refused once,
  before any run (the joins before it fit only the instructions);
- deterministic, because each run's start calls each `requires` of the
  chain the `on` sees again, for what arrives on that run, and checks it,
  and a run so refused walks that chain again to list the faults, which
  must find what the start found; and each joined part, at `ready`, calls
  its upper half's `requires` again for what arrives at its lower half,
  checks it against what that half provides, and only if it fits gets
  that half ready for it, which must be, where the start sees that half,
  what the start checked.

For instance, the example's driver requires the run on 2 qubits, its
set's own capability, and its executor provides 64 qubits and at most
1000 shots a run (from
[`examples/compose_and_run.rs`](../examples/compose_and_run.rs)); the
noise between them states nothing:

```rust
fn requires(&self, run: &RunRequirement) -> Capabilities<CoinGates> {
    Capabilities::new(run.resources, run.features, Qubits(2))
}
```

```rust
provides: Capabilities::new(
    Resources::unconstrained().with_shots(1000),
    Features::unconstrained(),
    Qubits(64),
),
```

| Kind          | Each field                            | Fits when                                                                     | A mismatch                                                                        |
| ------------- | ------------------------------------- | ----------------------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| Resources     | shots per run, instructions per batch | the amount that arrives is at most the amount provided, or either is unstated | [`ResourceMismatch`](../src/capabilities/resources.rs), with both amounts         |
| Features      | replay, seeded runs                   | the feature is not required, or the provider does not say it lacks it         | [`FeatureMismatch`](../src/capabilities/features.rs)                              |
| The set's own | as the set defines them               | as the set's `fits` says                                                      | the set's own mismatch, boxed in [`CapabilitiesMismatch`](../src/capabilities.rs) |

A replayed shot runs again with its run's seed, so stating replay `true`
states seeded runs `true` too, and stating seeded runs `false` states
replay `false` (`Features::with_replay`, `Features::with_seeded_runs`).

The set's own capabilities that arrive at a part are also what its
checked batches are checked against on each run, as the set's `check`
says ([Instruction sets](instruction-sets.md#a-checked-batchs-instructions)).

## The cases

An interleaver that sends batches of 3 instructions at least, under a
program of batches of 2, requires 3 below: over bits taking 2 the
interleaver falls short, and over bits taking 4 the chain composes (from
[`src/tests/composition/driver_joins.rs`](../src/tests/composition/driver_joins.rs)):

```rust
let d = program(2, Features::unconstrained());
let a = Stating::interleaving(3, &log);
let e = bits_over(bits, &log);
```

```rust
if short {
    vec![fault(
        part::<Stating>(1),
        part::<Bits>(2),
        short_batches(3, 2),
    )]
} else {
    vec![]
},
```

Other cases, each in the same tests:

| Chain                                                                                | Verdict                                                  |
| ------------------------------------------------------------------------------------ | -------------------------------------------------------- |
| a program of batches of 5, through a logger, over bits taking 4                      | refused, naming the program and the bits                 |
| a program of batches of 5, through the interleaver, over bits taking 4               | refused, naming the program and the bits                 |
| a program of batches of 5, through a decoder taking 4, over bits taking 10           | refused, naming the program and the decoder              |
| a program of runs of 12 shots, through a queue taking 10, over bits taking 3         | refused twice: the program at the queue, and at the bits |
| a debugger requiring replay, through a replayer, over bits that do not replay        | composes                                                 |
| a decomposer accepting the program's set and sending another, over bits accepting it | composes: a part is checked below for what it sends      |

A run's shots, and seeded runs if its seed is fixed, arrive at the
driver's `requires`, and from it at each part, as the `Runner` checks when
each run starts. A run of 1000 shots, over bits that take 100, is taken
through a job splitter that requires runs of 100 shots below, whatever run
arrives (the splitter only declares the split), and is refused without it
(from
[`src/tests/composition/runner.rs`](../src/tests/composition/runner.rs)):

```rust
let d = CoinCopy::new();
let splitter = Stating::splitting_runs(100, &log);
let e = bits();
```

## A joined part

The joins keep every part inside a joined part declared, so the `on` that
makes a runner checks what arrives at each, and its verdict, the positions
its faults name and their mismatches do not depend on how the joins are
bracketed. The names a fault gives are read by the final `on`, so they
agree across bracketings when each part's name stays the same through
composition, its `bind` included.

A joined part's own `requires(above)` gives what its lower part requires
of what its upper part requires of `above`; its `provides()` gives its
upper part's limits alone, since what its two parts take together is not
one value. So a part of the user's that holds a joined part and forwards
its role's public methods has the joined part's top limits checked by the
`on`, and its inner ones at its `ready`, on each run that reaches it: the
joined part refuses the run, naming the user's part, with the mismatch of
the first boundary inside it found not to fit, the one between its halves
checked first. For instance an
interleaver joined with a decoder taking batches of 4, so wrapped, under
a program of batches of 5, composes over bits taking 10, and refuses every
run
(from
[`src/tests/composition/runner.rs`](../src/tests/composition/runner.rs)):

```rust
let held = Renamed::new(
    Stating::interleaving(3, &log).through(Stating::capping(4, &log)),
    "user's adapter",
);
```

## Where each statement is checked

| What                                                                                                             | When                                   | A fault gives                                                                                                                            |
| ---------------------------------------------------------------------------------------------------------------- | -------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| what arrives at each part against its provision                                                                  | the `on` that makes a runner           | [`CompositionFaults`](../src/composition/composition_faults.rs): each requiring part and the part that falls short, by position and name |
| a joined part's halves, and what arrives at its lower part, inside a part of the user's the `on` cannot see into | each run, by the joined part's `ready` | [`RunRefusal::PartNotReady`](../src/composition/runner/run_error.rs), with the mismatch                                                  |
| what a run's shots, and seeded runs if its seed is fixed, make arrive at each part, against its provision        | each run, before any part is asked     | [`RunRefusal::NotProvided`](../src/composition/runner/run_error.rs), with every fault                                                    |
| a batch's count of instructions against the instructions per batch its receiver provides                         | each batch, at every boundary          | [`BatchFault::TooManyInstructions`](../src/capabilities/boundary/batch_limits.rs)                                                        |
| a checked batch's instructions against the receiver's set, and the set's own capabilities that arrive at it      | each checked batch, at every boundary  | [`BatchFault::Instruction`](../src/capabilities/boundary/batch_limits.rs), with the set's fault                                          |
