# Adapters

[Protocol overview](../README.md).

An **adapter** maps the executor downstream of it to the executor upstream,
and describes what it gives (`Adapter`): what it accepts and sends, the
random number id it draws with, and the capabilities it changes. How adapters
are put in a chain, and what building it checks, is in
[Composition](composition.md#composing-a-run); this page is about writing
one.

## Choosing a way

There are three ways to write an adapter:

- **`adapter_fn(make_state, logic)`**, a state and a logic, for an adapter
  that serves each call: a profiler, a debugger, a code that lowers a call
  and decodes what it reads
  ([A state and a logic](#a-state-and-a-logic-adapter_fn));
- **`owning_adapter_fn(make_state, logic)`**, the same adapter, for a state
  that keeps its way below for its whole life, such as a code's runtime
  ([A state that owns its way below](#a-state-that-owns-its-way-below-owning_adapter_fn));
- **by hand** (`Adapter` and `Apply`), mapping the whole executor downstream
  once ([An adapter written by hand](#an-adapter-written-by-hand)).

## A state and a logic: `adapter_fn`

The smallest adapter is a state and a logic (`adapter_fn(make_state, logic)`).
`make_state` makes the adapter's working state (a profiler's counts, a decoder's
history) at the first call, from the context of that call's first shot
(`MakeState`), and that one state serves every call of every shot. So it belongs
to the adapter, not to a shot: a profiler's counts sum all the shots it served.
For a state per shot, use `.shots(OnePerShot)`
([below](#a-state-per-shot-shotsonepershot)). An executor's state, by contrast,
always belongs to one shot ([Executors](executors.md)).

A state is made from its shot's context and what `make_state` captured when
the adapter was built; what varies per call goes to the logic
([What a state depends on](internals/adapters.md#what-a-state-depends-on),
in the detailed rules).

The logic serves each call. With the call it is given, it may:

- **send it below as it is**, with `below.send(..)`: it observes, as a profiler
  does, and the chain gives the outcomes it gives with no adapter;
- **send other instructions below**, tags kept, with `below.send(..)`: it
  rewrites, and states its sets with `.accepts(set)` and `.sends(set)`;
- **run instructions below**, with `below.run(..)`: it is given their readouts
  by shot and decides on them, as a code that decodes does, or records them, as
  a debugger does;
- **write outcomes**, with `outcomes`: what a run reads passes up only as the
  logic writes it.

The [tests of `adapter_fn`](../src/tests/adapter/adapter_fn.rs) show each of
these in code, and the [observer guide](../tests/observers.rs) shows a debugger
that records what it reads.

What each costs: a `send` is one call below on the call's shots, and does not
wait for its reply: once the call below returns, the reply passes up as it
comes; an executor below that does its work within the call holds the thread
until the call returns. A `run` returns the readouts, so it holds the thread
until they are in; with a state per shot, a shot's run goes below on that shot
alone. A consumer matches readouts by their tags, not by their place in the
reply ([The order of a run's
readouts](internals/adapters.md#the-order-of-a-runs-readouts)).

The repetition code of the [static instruction set
guide](../tests/static_isa.rs) is a logic that lowers a call, runs it and writes
each block's majority. Its logic, in full apart from the majorities
([`tests/static_isa/repetition_fn.rs`](../tests/static_isa/repetition_fn.rs)):

```rust
pub fn lower_then_decode(
    (): &mut (),
    input: &ProgramInput<'_, Logical>,
    below: &mut Below<'_, Op>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), String> {
    let lowered: Vec<Op> = (input.instructions().iter())
        .flat_map(|&instruction| gadget(instruction))
        .collect();
    let read = below.run(&lowered)?;
    outcomes.extend(majorities(&read)?);
    Ok(())
}
```

Unless its states draw, it offers what is below it, log probabilities included.

## A state per shot: `.shots(OnePerShot)`

Use a state per shot when the state is a shot's, such as a code's runtime whose
results name no shot
([below](#a-state-that-owns-its-way-below-owning_adapter_fn)). With
`.shots(OnePerShot)`, the same per-shot form as an executor's, `make_state`
makes a state per shot, at the shot's first call, from its context, and keeps it
for the shot's later calls. Below a replayable executor, a shot gives alone what
it gives among others.

A shot's `k`-th send goes below with the other shots' `k`-th sends equal to it,
as one call over the shots that sent them, rank by rank; sends of different
ranks, and a shot's kept sends at its run, go apart ([Merging a call's
sends](internals/adapters.md#merging-a-calls-sends-rank-by-rank)). For a
profiler, the shot form changes what it counts, per shot or per call, not the
outcomes.

## Seeds: `.drawing(id)`

An adapter whose states draw states the id it draws with, `.drawing(id)`, and
its shot `j`'s state draws from shot `j`'s seed in the run bound to that id, so
that it offers replayable shots as the executor below does, and a shot draws
alone what it draws among the others; with one state, the state seeds from the
first shot it serves, and the adapter offers no replayable shots.

The run's seed is the one the last executor below is given with `.seed(seed)`,
through any adapters between ([Seeds](executors.md#seeds-not-generators)), so
one seed replays the chain, another changes the adapter's draws, and two
adapters that draw each seed under their own id. An id drawn with below, however
far down, refuses the chain when it is built, and so does a chain where no run's
seed reaches the adapter, as below an executor written by hand, which cannot
state one yet (`BuildError::NoRunSeed`).

## Reports: `.state_report(f)`

Given `.state_report(f)`, the same option as an executor's, an adapter reports
with `f` each state it holds when it closes, folded, beside the report below, as
a pair: its own first. A state made for a call that failed before it returned,
or panicked, is not reported ([Reports of failed
calls](internals/adapters.md#reports-of-failed-calls)).

A chain's report holds one such pair per adapter given a `state_report`, nested
in the chain's order, and an adapter given none adds none, passing the report
below up as it is, so the report is the same under every grouping of `>>`, and
`run.wait()` gives it. Through an adapter that sends each call down as one, the
report's `calls` are those the executor alone reports.

No adapter has a log probability of its own, whether a facility made it or it
was written by hand: a run's report passes up the last executor's, each shot's
log probability included. An adapter that draws offers no `probabilities`, since
the log probability downstream does not hold its draws: a driver that needs them
is refused. Known issue: the run of a driver that does not need them still
reports the log probabilities of the executor downstream, which miss the
adapter's draws, for either kind of adapter (a test of the fix is written, and
ignored until the fix lands); it goes with `.log_probability` on adapters,
planned, with no milestone yet, for a noise model whose run reports each shot's
log probability ([Planned features](planned.md)).

## A state that owns its way below: `owning_adapter_fn`

A state that keeps its way below for its whole life, such as a code's runtime
made once per shot over a backend it owns, is made by
`owning_adapter_fn(make_state, logic)`: `make_state` is given the way below
(`OwnedBelow`) to keep, and the logic is lent none. It is the same adapter, with
the same options and per-shot form: what the state sends or runs through it goes
below as through a lent one, by the same refusal and failure rules ([The way
below of an owning
state](internals/adapters.md#the-way-below-of-an-owning-state)).

A runtime whose results name no shot, as a code's runtime's, needs a state per
shot (`.shots(OnePerShot)`): with one state, a run below gives it the readouts
of every shot where it expects one shot's, and the guide's glue, which checks
that the reads match its readouts one to one, fails the call at its first batch
that reads.

The guide's runtime, which owns its backend as a Qodec runtime does, is made a
shot's state over the way below it owns
([`tests/static_isa/code_runtime.rs`](../tests/static_isa/code_runtime.rs)):

```rust
pub fn instantiated(
    plan: CodePlan,
) -> impl FnMut(ShotContext, OwnedBelow<Op>) -> Result<CodeRuntime<Lower>, Rejected> {
    move |_, below| Ok(plan.instantiate(Lower(below)))
}
```

and made an adapter, one state per shot, with no change to it
([`tests/static_isa.rs`](../tests/static_isa.rs)):

```rust
let as_a_runtime = owning_adapter_fn(instantiated(CodePlan::default()), through_the_runtime)
    .accepts(logical_set())
    .sends(gadget_instructions())
    .check(within(toy::QUBITS / 3))
    .shots(OnePerShot);
```

The way below reaches below only while its state's logic serves a call,
on that call's thread, and not inside the owning logic of another chain
that its logic runs; elsewhere it gives an `Err` that says why
([The way below of an owning state](internals/adapters.md#the-way-below-of-an-owning-state)).

## An adapter written by hand

An adapter written by hand maps the whole executor downstream of it once
(`Apply`), when the chain is built, and its executor is called with each batch
on all its shots: `RepetitionCode` (`tests/static_isa/repetition_code.rs`) puts
a `Repetition3` upstream of the executor it is given, and `Debuggers`
(`tests/debugger/debuggers.rs`) a `Debugger`. An adapter is a few lines around
the executor it makes. Each method of `Adapter` has a default for an adapter
that changes nothing (the debugger implements none); the repetition code states
what it accepts and sends, and, built once against the executor downstream
(`Adapter::build`), what it does to the qubits there:

```rust
impl Adapter for RepetitionCode {
    type Accepts = Logical;
    type Sends = Op;

    fn sends(&self) -> Option<Arc<InstructionSet>> {
        Some(Arc::new(gadget_instructions()))
    }

    fn accepts(&self) -> Option<Arc<InstructionSet>> {
        Some(Arc::new(logical_set()))
    }

    /// Built against the executor downstream, once: it offers a logical
    /// qubit per block of three qubits there.
    fn build(self, boundary: &mut AdapterBoundary<'_>) -> Self {
        let blocks = boundary.downstream().qubits.map(|q| q / 3);
        boundary.offered_upstream().qubits = blocks;
        Self { blocks }
    }
}
```

## Refusals and failures

An adapter keeps an executor's rules:

- **Its check refuses a call before its state is made.** `.check(check)` states
  what the adapter admits beside what it offers (the Promise axis:
  [Glossary](glossary.md#a-parts-axes)), and refuses a call before its state is
  made and its logic runs, as an executor's check does. A `make_state` that
  refuses refuses the call too, before the logic runs, and leaves nothing made.
- **A refusal below refuses the call.** The first call below, a send or a run,
  refused, refuses the call with the same refusal. The index it names is that of
  the instruction in the call below, not in the call (planned: a way for the
  logic to map it). A call refused below is refused though its logic ran, so a
  logic changes its state only after `send(..)?` or `run(..)?`. With a state per
  shot, a send gives no such order: `send` returns `Ok` before the shots' sends
  go below merged, so a call refused there leaves the kept states as its logic
  left them.
- **An error or a failure below fails the adapter.** The logic's error, or a
  failure below, fails the call and the adapter, and nothing more goes down.
- **Closing closes below once.** Closing it gives the report below, closing what
  is below once, whatever failed.

So a profiler or a decoder over a simulator fails as the simulator alone does:
closed, it refuses every call. The other failures, and the rest of what the
conformance kit checks, are in the detailed rules ([Failures in
detail](internals/adapters.md#failures-in-detail), [The conformance
kit](internals/adapters.md#the-conformance-kit)).

The detailed rules, for maintainers of the crate, are in
[Adapters: the detailed rules](internals/adapters.md).
