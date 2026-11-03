# Getting started: six steps to a checked chain

[Protocol overview](../README.md).

Your part takes only the steps it needs, up to six
([Which steps for your part](#which-steps-for-your-part)).
Each step shows what you write, in an excerpt copied from a test, and
links to the page that has the details; what the protocol and its
facilities do for you is gathered in
[What you get for free](#what-you-get-for-free).

1. [Define your instruction set](#1-define-your-instruction-set)
2. [Write your program and its driver](#2-write-your-program-and-its-driver)
3. [Write your executor](#3-write-your-executor)
4. [Add adapters](#4-add-adapters)
5. [Run many shots and read the results](#5-run-many-shots-and-read-the-results)
6. [Check your parts](#6-check-your-parts)

[The interfaces](#the-interfaces-for-a-part-written-by-hand), at the end,
are for a part written by hand. The tests that pin what this page states
are listed in [`docs/internals/`](internals/), beside each rule; those
of the statements this page adds, in
[`docs/internals/composition.md`](internals/composition.md), "From Getting
started".

## Which steps for your part

Take the steps your task needs; the last column links its details.

| Task                                                                                    | What you write                                                                                         | Steps                                                                  | Details                                                                                                                                                            |
| --------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| run a program of fixed batches (`StaticProgram`)                                        | the program, run on the chain `static_driver() >> … >> executor` with `chain.run(program, shots)`      | 1 (the calls of the chain's set), 2, 5                                 | [A program of fixed batches](#a-program-of-fixed-batches)                                                                                                          |
| run a program that branches on its readouts, or your own runtime (QIR, a circuit form)  | a program, `Program`, and its driver, `Driver<P>`, made the chain's prefix with `runtime.into_chain()` | 1 (its instruction type), 2, 5; the kit has no check of a driver       | [Writing a driver](#writing-a-driver)                                                                                                                              |
| plug in a device, or a simulator with a state per shot                                  | an executor, with `executor_fn`                                                                        | 1, 3, 6                                                                | [Write your executor](#3-write-your-executor)                                                                                                                      |
| plug in a backend written before the protocol                                           | an executor around it, a bridge, with `executor_fn`                                                    | 1, 3, 6                                                                | [Running an existing backend](guides.md#running-an-existing-backend)                                                                                               |
| run an executor's logic on a thread of its own                                          | `.run_on(placement)`                                                                                   | 3                                                                      | [Where an executor runs](executors.md#where-an-executor-runs-run_on)                                                                                               |
| plug in a simulator whose shots share one state (an ensemble, such as an MPS simulator) | an executor written by hand, on `ExecutorLifecycle`                                                    | 1, 6, and [the interfaces](#the-interfaces-for-a-part-written-by-hand) | [An executor written by hand](executors.md#an-executor-written-by-hand-executorlifecycle)                                                                          |
| compose your own executor type with `>>`, in either grouping                            | its `>>` for each type that may be on its left, through `Compose<_, WithExecutor>`                     | [the interfaces](#the-interfaces-for-a-part-written-by-hand)           | [An executor written by hand](executors.md#an-executor-written-by-hand-executorlifecycle); guide: [`tests/executor_by_hand.rs`](../tests/executor_by_hand.rs)      |
| add a noise model, a code (static or run-time ISA) or an observer                       | an adapter: `adapter_fn`, `owning_adapter_fn`, or by hand                                              | 1 (the sets it accepts and sends), 4, 6                                | [Add adapters](#4-add-adapters); guides: [a static ISA](guides.md#a-static-isa-a-repetition-code), [a run-time ISA](guides.md#a-run-time-isa-a-code-given-as-data) |
| hold work and complete it from another thread                                           | an executor returning an `IncrementalReply`, filled later by its `ReplySender`                         | 6, and [the interfaces](#the-interfaces-for-a-part-written-by-hand)    | [A debugger](guides.md#a-debugger-holding-and-completing-work)                                                                                                     |
| check that a part keeps the rules                                                       | the `conformance` checks, in your tests                                                                | 6                                                                      | [Check your parts](#6-check-your-parts)                                                                                                                            |
| write a part by hand                                                                    | the traits                                                                                             | 6, and [the interfaces](#the-interfaces-for-a-part-written-by-hand)    | [The interfaces](#the-interfaces-for-a-part-written-by-hand)                                                                                                       |

## What you get for free

The facilities (`static_driver`, `executor_fn`, `adapter_fn`) keep the
protocol's rules, so you write only your own logic: the instructions, how a
shot's state is made, and what a call does to it. Written with them, a part
gets the following; each step ends with the items it brings.

- **Checks when the chain is built**: a part of another instruction type
  does not compile into the chain; an adapter's sets are matched against
  the set below, a mismatch refusing the chain, and the calls of an adapter
  that states the set it sends (`.sends(set)`) are renumbered into the set
  below; adapters that draw each draw under their own id, a repeated id
  refused, and one that draws over no run's seed is refused, the refusal
  naming it.
- **Checks when the job is given**, before any call: the needs of the
  program and of its driver, combined, against what the chain offers, so a
  driver cannot drop the program's, every unmet need reported at once and
  an unmet capability naming the part that lacks it; a static program's
  calls outside its set, and its set against the chain's. The static
  driver adds `resumable_shots` itself to a program that has calls before
  its last batch
  ([What is checked, and when](composition.md#what-is-checked-and-when)).
- **Checks for an executor's `.check`**: `InstructionSet::check_batch`, a
  batch of calls against the executor's set, and `check_unique_tags`, which
  refuses a readout tag repeated in a batch at the instruction that
  repeats it.
- **A state and a seed per shot**, as options: with `.shots(OnePerShot)`,
  a state per shot, made at the shot's first call and kept for its later
  calls; with the run's seed given by `.seed(s)`, each shot's seed, fixed
  by that seed and the shot's id, so that a run whose states draw only
  from their shot's seed is reproducible, and no two shots of a run share
  a seed (without `.seed(s)`, a state that draws panics). Over an
  `adapter_fn` with a state per shot, the shots' equal k-th sends go below
  as one call, but for a shot's kept sends, which go alone at its `run`.
- **An executor's life**: a call whose logic fails ends it, a closed one
  refuses calls, and a refusal for the call's own fault leaves it usable.
- **A thread**, as an option: `.run_on(OwnThread)` places the logic on a
  thread of its own, with the same outcomes and report as on the caller's
  ([Where an executor runs](executors.md#where-an-executor-runs-run_on)).
- **Shot conditions**: those raised below reach the caller through an
  `adapter_fn`, and one the adapter reads from below during a call marks
  what its logic writes for the shot in that call and the later ones. Two
  [known issues](replies.md#shot-conditions): one that a send gives after
  the adapter's call returned is taken in only once a later call reads it
  from below, and the guides' decoders, written by hand, carry none yet.
- **Reports**: the executor's, `ExecutorReport { calls, log_p, states }`,
  each state's report folded into `states`; above it, each adapter given a
  `.state_report` pairs its own report with the report below,
  `(this adapter's report, the report below)`; and the chain's grouping
  with `>>` does not change the run.
- **A run**, waited for or awaited, that gives the same outcomes either
  way, closes the chain once, after the driver's last call, and gives the
  program's output beside the chain's report.
- **The conformance kit**: the protocol's rules checked on your part, each
  check naming the rule a part breaks, without your writing those tests
  ([step 6](#6-check-your-parts)).

## The names the excerpts use

The excerpts are from the
[guide tests](guides.md#building-on-the-protocol-the-guide-tests), on a toy
of classical bits and seeded fair coins. The names an excerpt uses but
does not define are helpers of the tests it is copied from:

- of the toy, which the guide tests share
  ([`tests/toy/simulator.rs`](../tests/toy/simulator.rs)): its set
  `toy_set()`, its instructions `Op`, a shot's state `ToyState`, its
  simulator on a seed, `toy_simulator(seed)`, of type `ToySimulator`, the
  types of its closures `StartToy`, `RunToy` and `CheckToy`, its qubits
  `QUBITS`, and `readouts`, a batch's readout tags;
- of [`tests/observers.rs`](../tests/observers.rs): `batches()`,
  `the_toys_promise()`, `check_calls`, `toy_ops()`, `Received`, `Shot`,
  the seed `SEED`, the shots `SHOTS` and the message `THE_JOB_FITS`; and
  the profiler's `Counts`, of
  [`tests/observers/profiler.rs`](../tests/observers/profiler.rs);
- of [`tests/existing_tool.rs`](../tests/existing_tool.rs): `on_shot_0`, a
  batch on shot 0; and the profiling executor on a seed,
  `profiling_executor(seed)`, of
  [`tests/existing_tool/profiler.rs`](../tests/existing_tool/profiler.rs);
- of [`tests/runtime_isa/repetition3.rs`](../tests/runtime_isa/repetition3.rs):
  the actions `reset()` and `measure()`;
- of [`tests/existing_tool/bridge.rs`](../tests/existing_tool/bridge.rs):
  `call_tags`, the readout tags of a batch of calls.

## 1. Define your instruction set

An instruction is either a type of your own or a call by opcode
([Instruction sets](instruction-sets.md)).

**Typed.** Mark your type `TypedInstruction`; no `InstructionSet` is
needed. A chain of typed parts may describe no set: the compiler checks that
each part takes the type the part above sends. The toy's instructions
([`tests/toy/simulator.rs`](../tests/toy/simulator.rs)):

```rust
/// The toy's instructions, as a type: an executor of
/// `InstructionBatch<Op>` accepts these and nothing else; [`toy_set`]
/// describes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// Sets the qubit to a fair random bit, drawn from the shot's seed.
    Coin(u32),
    /// Flips the qubit.
    X(u32),
    /// Flips the target (second) when the control (first) is 1.
    Cx(u32, u32),
    /// Reads the qubit, and names the outcome `tag`.
    Measure { qubit: u32, tag: u32 },
    /// Sets the qubit to 0.
    Reset(u32),
}

/// A toy instruction names itself by its variant, so a program of them
/// runs on a chain with no adapter between them.
impl TypedInstruction for Op {}
```

A typed part may still describe a set, as the toy does with `toy_set()`,
so that an adapter above it that states the set it sends, such as the
repetition code of the
[static-ISA guide](guides.md#a-static-isa-a-repetition-code), is checked
against it when the chain is built.

**Calls by opcode.** An `InstructionCall` is an opcode and its operands,
numbered by an `InstructionSet` that its sender states. A set is a name and
its instructions, each a mnemonic, the kinds of its operands and, if
declared, what it does; `set.call(mnemonic, operands)` makes a call. This
program states its set
([`tests/runtime_isa.rs`](../tests/runtime_isa.rs)):

```rust
fn prepare_and_read() -> StaticProgram<InstructionCall> {
    let set = InstructionSet::new("program")
        .with_action("prepare_z", [Block], reset())
        .with_action("__quantum__qis__m__body", [Block, Tag], measure());
    let call = |mnemonic, operands: Vec<Operand>| {
        (set.call(mnemonic, operands)).expect("a call of the program's set")
    };
    let batch = vec![
        call("prepare_z", vec![Operand::Block(0)]),
        call(
            "__quantum__qis__m__body",
            vec![Operand::Block(0), Operand::Tag(0)],
        ),
    ];
    let needs = Needs::default().with_resets(true);
    StaticProgram::new(needs.with_instruction_set(Some(Arc::new(set))), vec![batch])
}
```

A set known only at run time, parsed from a file or given by a service, is
built with the same calls, `InstructionSet::new(name)` then `with` or
`with_action` for each instruction parsed; the crate has no parser. A
mnemonic given twice replaces the first at its opcode, so a parser that
must refuse duplicates checks them itself
([A run-time set](instruction-sets.md#a-run-time-set-a-description)).

You get for free here: the **checks when the chain is built** and **when
the job is given**, and the **checks for an executor's `.check`**
([What you get for free](#what-you-get-for-free)).

## 2. Write your program and its driver

### A program of fixed batches

`static_driver()` runs a `StaticProgram`: what the program needs of the
chain (`Needs`), and its batches, run in turn on every shot; you write no
driver. The guide's program is its two batches on two qubits of the toy's
set ([`tests/observers.rs`](../tests/observers.rs)):

```rust
/// The program each run runs on every shot: its [`batches`], on two
/// qubits, of the toy's set.
fn program() -> StaticProgram<InstructionCall> {
    program_of(batches())
}

/// A program of `batches`, on two qubits, of the toy's set.
fn program_of(batches: Vec<Vec<InstructionCall>>) -> StaticProgram<InstructionCall> {
    let needs = Needs::default()
        .with_qubits(2)
        .with_instruction_set(Some(Arc::new(toy_set())));
    StaticProgram::new(needs, batches)
}
```

The chain is `static_driver() >> adapters >> executor`
([step 5](#5-run-many-shots-and-read-the-results) runs it).

### Writing a driver

A program that branches on its readouts, or a program form such as QIR or
a circuit, is run by a driver of its form. The program is data: it
implements `Program`, its instructions, its output and what its content
needs. The driver implements `Driver<P>` for it
([the interfaces](#the-interfaces-for-a-part-written-by-hand)): what its
way of sending adds to the program's needs (`needs`), the faults it finds
in the program before any call (`check`), and `drive`, in which it runs
the program, making its calls on the one executor it is given: the
adapters and the executor below look to it like that single executor. A
driver is written once and runs any program of its form; a chain runs one
job (running many jobs on one chain is [planned](planned.md)). This program branches on a readout: a batch on
every shot, then another on the shots whose readout is 1
([`tests/driver/if_one.rs`](../tests/driver/if_one.rs)):

```rust
pub struct IfOne {
    /// What its instructions need: its qubits, and anything else.
    pub needs: Needs,
    /// The batch every shot runs.
    pub first: Vec<Op>,
    /// The readout of `first` the branch reads.
    pub tag: u32,
    /// The batch the shots whose readout `tag` is 1 run next.
    pub then: Vec<Op>,
}

impl Program for IfOne {
    type Instruction = Op;
    /// The readouts of both batches, or why a call was refused or failed.
    type Output = Result<Vec<ShotOutcome>, String>;

    fn needs(&self) -> Needs {
        self.needs.clone()
    }
}
```

Its driver sends the first batch to every shot, reads each shot's
readout, and sends the second batch to the shots that read 1. Those
continue where the first batch left them, so the driver adds
`resumable_shots` to the program's needs:

```rust
pub struct IfOneDriver;

impl Driver<IfOne> for IfOneDriver {
    /// `then` continues the shots `first` ran, so the executor keeps each
    /// shot's state between calls, whatever the program states.
    fn needs(&self, _: &IfOne) -> Needs {
        Needs::default().with_resumable_shots(true)
    }

    async fn drive<E>(
        &self,
        program: IfOne,
        executor: &mut E,
        shots: &ShotSet,
    ) -> Result<Vec<ShotOutcome>, String>
    where
        E: Executor<InstructionBatch<Op>>,
    {
        let first = InstructionBatchRef {
            shots,
            instructions: &program.first,
        };
        let reply = executor.call(first).map_err(|r| r.to_string())?;
        let (mut outcomes, end) = wait_async(reply).await;
        end?;
        let read_one = |j| {
            (outcomes.iter()).any(|o| o.shot == j && o.tag == program.tag && o.value == Value::One)
        };
        let Some(ones) = shots.subset(read_one) else {
            return Ok(outcomes);
        };
        let then = InstructionBatchRef {
            shots: &ones,
            instructions: &program.then,
        };
        let reply = executor.call(then).map_err(|r| r.to_string())?;
        let (then_read, end) = wait_async(reply).await;
        outcomes.extend(then_read);
        end.map(|()| outcomes)
    }
}
```

A program tosses a coin, then, on the shots that read 1, tosses a second
coin and reads the first again; it states its qubits
([`tests/driver.rs`](../tests/driver.rs)):

```rust
fn toss_again_on_one() -> IfOne {
    IfOne {
        needs: Needs::default().with_qubits(2),
        first: vec![Op::Coin(0), Op::Measure { qubit: 0, tag: 0 }],
        tag: 0,
        then: vec![Op::Coin(1), Op::Measure { qubit: 0, tag: 1 }],
    }
}
```

The driver heads the chain, made its prefix by `into_chain()` (the trait
`DriverExt`, in scope): `driver.into_chain() >> adapters >> executor`,
here with no adapter, and `chain.run(program, shots)` runs the program,
here on eight shots of the toy's simulator:

```rust
let shots = ShotSet::range(0..=7).expect("not empty");
let chain = (IfOneDriver.into_chain() >> toy_simulator(3)).expect("nothing between to refuse");
let (outcomes, _) = chain
    .run(toss_again_on_one(), shots)
    .expect("the toy has the qubits and resumable shots")
    .wait();
```

On an executor that does not keep a shot between calls, the job is
refused before any call, for the need the driver adds (`resumable_shots`),
which the program does not state. A driver of several program types, or
one generic over its program, is given the type of the program it runs,
`DriverExt::<P>::into_chain(driver)`
([Composing a run](composition.md#composing-a-run)).

You get for free here: the **checks when the job is given** and **a run**
([What you get for free](#what-you-get-for-free)).

## 3. Write your executor

`executor_fn(promise, make_state, logic)` makes an executor from what it
promises (`Capabilities`, which offers nothing it does not state), how a
shot's state is made, and the logic that runs a call on it; `.check(f)`
refuses a call before the state is made and the logic runs. Each option,
and when it acts, is in
[Making an executor](executors.md#making-an-executor-executor_fn) and
[Where an executor runs](executors.md#where-an-executor-runs-run_on). The
toy simulator takes the typed `Op`; its state per shot, `ToyState`, is
made by `ToyState::start` and run by `ToyState::run`
([`tests/toy/simulator.rs`](../tests/toy/simulator.rs)).
`toy_simulator(seed)` calls `toy_simulator_described_with(seed, toy_set())`:

```rust
pub fn toy_simulator_described_with(seed: u64, set: InstructionSet) -> ToySimulator {
    let promise = Capabilities::default()
        .with_qubits(QUBITS)
        .with_resumable_shots(true)
        .with_replayable(true)
        .with_resets(true)
        .with_instruction_set(Some(Arc::new(set)));
    executor_fn(
        promise,
        ToyState::start as StartToy,
        ToyState::run as RunToy,
    )
    .check(check as CheckToy)
    .seed(seed)
    .shots(OnePerShot)
}
```

`.seed(seed)` gives the last executor the run's seed (`chain.run` takes
none), and `ToyState::start` makes a shot's state from that shot's seed,
`ShotContext::seed()`
([Seeds, not generators](executors.md#seeds-not-generators)).

The simulator of the [README's example](../README.md#an-example),
`simulator()` ([`tests/observers.rs`](../tests/observers.rs)), runs the toy
on calls by opcode, with a state per shot. It calls
`simulator_recording`, which also records what each shot receives, for the
test's checks, with a record it drops; `toy_ops()` maps each opcode of the
toy's set to the toy's `Op`, and `check_calls` checks the calls first:

```rust
fn simulator_recording(
    received: Received,
) -> OnePerShotExecutor<
    InstructionCall,
    Shot,
    impl MakeState<Shot>,
    impl ExecutorLogic<InstructionCall, Shot>,
> {
    let ops = toy_ops();
    let start = |context: ShotContext| Ok((context.shot(), ToyState::start(context)?));
    let logic = move |(shot, toy): &mut Shot,
                      input: &CallInput<'_, InstructionCall>,
                      outcomes: &mut Outcomes<'_>| {
        received.record(*shot, input.instructions());
        for call in input.instructions() {
            toy.apply(ops[call.opcode](call), outcomes);
        }
        Ok(())
    };
    let check: fn(&CallInput<'_, InstructionCall>) -> Result<(), Rejected> = check_calls;
    executor_fn(the_toys_promise(), start, logic)
        .check(check)
        .seed(SEED)
        .shots(OnePerShot)
}
```

A function that makes a part for `>>` returns the type the facility gives,
its closures named by role, as here, not `impl Executor`
([Naming a made part's type](executors.md#naming-a-made-parts-type)).

**What the executor checks itself**, in its `.check`, before it runs any
of a batch:

| Check                                 | Typed executor                     | Executor of calls by opcode   |
| ------------------------------------- | ---------------------------------- | ----------------------------- |
| each instruction is one of its own    | none: the compiler checks the type | `InstructionSet::check_batch` |
| each instruction is on a qubit it has | its own rule                       | its own rule                  |
| no readout tag repeats in the batch   | `check_unique_tags`                | `check_unique_tags`           |

A driver other than the static one may send any call, so the executor
checks even what a static program had checked. The existing-tool bridge
checks calls by opcode against its set and their tags; the end of
`backend_bridge(..)`, whose `bridge` is the executor `executor_fn` made and
`set` the instruction set it is given
([`tests/existing_tool/bridge.rs`](../tests/existing_tool/bridge.rs)):

```rust
    Ok(bridge.check(move |input| {
        set.check_batch(input.instructions())?;
        check_unique_tags(call_tags(input.instructions()))
    }))
```

and the toy checks its qubits and tags
([`tests/toy/simulator.rs`](../tests/toy/simulator.rs)):

```rust
pub fn check(input: &CallInput<'_, Op>) -> Result<(), Rejected> {
    let batch = input.instructions();
    if let Some(at) = batch.iter().position(|op| op.qubits().any(|q| q >= QUBITS)) {
        return Err(Rejected::Instruction {
            at,
            reason: format!("the toy has {QUBITS} qubits"),
        });
    }
    check_unique_tags(readouts(batch))
}
```

**Shot conditions.** An executor that offers `resumable_shots` must keep
each shot's conditions from one call to the next: the shot's outcomes in a
later call carry every condition raised on the shot before. An
`executor_fn` keeps none across calls by itself: its logic keeps them in
the shot's state and raises them again in each call
([Shot conditions](replies.md#shot-conditions)). Neither its type, nor the
chain's build, nor the job checks it; the conformance kit's check across
calls checks, at run time, those that an earlier outcome of the shot
carried ([step 6](#6-check-your-parts)).

You get for free here: **a state and a seed per shot**, **an executor's
life**, **a thread** and the executor's **reports**
([What you get for free](#what-you-get-for-free)).

## 4. Add adapters

`adapter_fn(make_state, logic)` makes an adapter from how its state is made
and the logic that serves each call, given its way below, a `Below`:
`below.send(..)` passes a call below, its reply passing up as it comes, and
`below.run(..)` gives the readouts below by shot, which pass up only as the
logic writes them. Each option, and when it acts, is in
[A state and a logic](adapters.md#a-state-and-a-logic-adapter_fn). The
profiler of the [README's example](../README.md#an-example)
([`tests/observers/profiler.rs`](../tests/observers/profiler.rs)) counts
each shot's instructions and sends the call below as it is:

```rust
fn profile(
    counts: &mut Counts,
    input: &CallInput<'_, InstructionCall>,
    below: &mut Below<'_, InstructionCall>,
    _: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    for call in input.instructions() {
        *counts.entry(call.opcode).or_default() += 1;
    }
    below.send(input.instructions())
}
```

```rust
pub fn profiler() -> OnePerShotAdapter<
    InstructionCall,
    InstructionCall,
    Counts,
    impl MakeState<Counts>,
    impl AdapterLogic<InstructionCall, InstructionCall, Counts>,
    NoCheck<InstructionCall>,
    impl FnMut(&Counts) -> Counts,
> {
    adapter_fn(|_| Ok(Counts::default()), profile)
        .state_report(|counts: &Counts| counts.clone())
        .shots(OnePerShot)
}
```

Its type is the facility's, as the executor's is; given no check, its
check is `NoCheck<I>`, named because the report's parameter comes after
it. The logic's error, an `AdapterError`, is a message made from a
`String` or a `&str`: the logic returns `Err("…".into())`, and `?` on a
`Result` whose error is a `String` passes it up
([Refusals and failures](adapters.md#refusals-and-failures)).

An adapter of calls by opcode that takes one set and sends another states
both, `.accepts(set)` and `.sends(set)`, and, since `.accepts` does not
check calls yet, checks them in `.check` with `InstructionSet::check_batch`.
An adapter whose states draw states the id it draws with, `.drawing(id)`
([Seeds](adapters.md#seeds-drawingid)).

A state that keeps its way below for its whole life, such as a code's
runtime, is made with `owning_adapter_fn`
([A state that owns its way below](adapters.md#a-state-that-owns-its-way-below-owning_adapter_fn)).
An adapter written by hand (`Adapter` and `Apply`) maps the whole executor
below once: it can change what the chain offers beyond its instruction
sets, as a code offers a logical qubit per block of qubits below
([An adapter written by hand](adapters.md#an-adapter-written-by-hand)).
Guides: [a static ISA](guides.md#a-static-isa-a-repetition-code),
[a run-time ISA](guides.md#a-run-time-isa-a-code-given-as-data).

You get for free here: the **checks when the chain is built**, **shot
conditions** carried up, the equal sends of **a state per shot** merged
(but for kept sends), and **reports** paired ([What you get for free](#what-you-get-for-free)).

## 5. Run many shots and read the results

The chain is built, given the program on the job's shots, then waited for
(`run.wait()`, or `.await` in async code). The run gives the program's
output and the chain's report; a static program's output is its outcomes,
or the error that ended the run (`StaticProgramError`), each outcome a `ShotOutcome { shot, tag, value, conditions }`, every shot's
readouts in one list. Sort or filter them by `shot` to read each shot's
([`tests/observers.rs`](../tests/observers.rs)):

```rust
fn run_alone() -> (Vec<ShotOutcome>, ExecutorReport<()>) {
    let chain = (static_driver() >> simulator()).expect("nothing between to refuse");
    let (outcomes, report) = chain.run(program(), SHOTS).expect(THE_JOB_FITS).wait();
    let outcomes = by_shot(outcomes);
    for shot in SHOTS {
        let read: Vec<_> = outcomes.iter().filter(|o| o.shot == shot).collect();
        let [coin, copy, again] = read[..] else {
            panic!("shot {shot} reads tags 0, 1 and 2 once each: {read:?}");
        };
        assert_eq!(
            [coin.tag, copy.tag, again.tag],
            [0, 1, 2],
            "shot {shot} reads tags 0, 1 and 2"
        );
        assert!(
            copy.value == coin.value && again.value == coin.value,
            "shot {shot} reads its coin at each tag: {read:?}"
        );
    }
    (outcomes, report)
}
```

`by_shot` takes the outcomes of a run that succeeded (it panics on the
error) and sorts them by shot, keeping each shot's in the order read. The
shots are anything `IntoShotSet` (a `ShotSet`, a range, or an array or
`Vec` of ids); here the array `SHOTS`, which is not a range.

- **Frequencies** of outcomes are counted by the caller, from the outcomes
  grouped by shot: the crate gives each shot's readouts, not their
  statistics.
- **Conditions:** a shot's conditions (flagged for rejection, such as a
  failed post-selection; inconsistent checks; an unresolved value) are the
  union of those its outcomes carry, tested with
  `ShotConditions::contains`
  ([Shot conditions](replies.md#shot-conditions)).
- **Reports:** the executor's is `ExecutorReport { calls, log_p, states }`;
  above it, each adapter given a `.state_report` pairs its own with the
  report below, so the README's example reads the profiler's counts as
  `(outcomes, (counts, _))`
  ([Reports](adapters.md#reports-state_reportf)).

Building a chain gives a `Chain`, or every fault of the parts built
together (`BuildFaults`), and `chain.run` a `Run`, or every fault of the
job (`JobFaults`), both
[before any call](composition.md#what-is-checked-and-when); a chain runs
one job ([A chain and its job](composition.md#a-chain-and-its-job)).

You get for free here: **a run** and its **reports**
([What you get for free](#what-you-get-for-free)).

## 6. Check your parts

The rules the compiler cannot check are checks in `conformance`, which you
run on your parts in your own tests, with the `conformance` feature (from
the module's rustdoc, `src/conformance.rs`):

```toml
[dev-dependencies.executor_protocol]
path = "../executor_protocol"
features = ["conformance"]
```

Each check panics, naming the rule broken, when the part does not keep it;
an adapter is checked composed with an executor
([`tests/runtime_isa.rs`](../tests/runtime_isa.rs)). The check that a
refusal for the call's own fault leaves an executor usable, run on the
existing-tool guide's profiling executor, `profiling_executor(seed)`, here
on seed 0, made with `executor_fn`, which counts the instructions its shots
run; its check refuses the first batch, as it reads two qubits under one
tag ([`tests/existing_tool.rs`](../tests/existing_tool.rs)):

```rust
let read = |qubit| Op::Measure { qubit, tag: 0 };
conformance::accepts_calls_after_a_refusal::<InstructionBatch<Op>, _>(
    &mut profiling_executor(0),
    on_shot_0(&[read(0), read(1)]),
    on_shot_0(&[read(0)]),
);
```

The checks, the parts each applies to, and what to pass; each check's
rustdoc, in `src/conformance/`, has the details. Four checks take `make`,
which builds the same executor, on the same seed, at each call:
`gives_each_shot_alone_what_it_gives_among_others`,
`continues_a_shot_from_one_call_to_the_next`,
`changes_no_shot_a_call_does_not_name` and
`starts_its_shots_as_made_whatever_an_earlier_executor_left`; the others
take the executor, or a report:

| Check                                                        | Applies to                                                              | Pass                                                                                                                                                                                                                                                                                            |
| ------------------------------------------------------------ | ----------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `rejects_calls_once_closed`                                  | any executor that closes                                                | a message it accepts while open                                                                                                                                                                                                                                                                 |
| `accepts_calls_after_a_refusal`                              | any executor                                                            | a message it refuses for its own fault, of a class other than `Resend`, and one that runs to its end                                                                                                                                                                                            |
| `refuses_calls_after_a_failure`                              | an executor whose state carries over between calls                      | a message whose reply fails, with a reason, and a later one; on several shots, include shots the failing one does not name; it waits for the failing reply, so run it under a time limit                                                                                                        |
| `ends_calls_in_flight_at_an_earlier_failure`                 | an executor whose state carries over between calls                      | a failing message and a later one; it waits for both replies, so run it under a time limit                                                                                                                                                                                                      |
| `reports_once`                                               | an executor that closes with a report of `PartialEq + Debug`            | the executor after it did work, so that its report is not empty; it does not apply to a `()` report, which is always empty                                                                                                                                                                      |
| `default_is_empty`                                           | any report of `Fold + Clone + PartialEq + Debug`                        | a report                                                                                                                                                                                                                                                                                        |
| `folds_in_any_order`                                         | any report of `Fold + Clone + PartialEq + Debug`                        | three reports, two by two different; for floats, values whose sums are exact                                                                                                                                                                                                                    |
| `refuses_calls_outside_its_set`                              | an executor of calls by opcode                                          | its shots, its set, and a call of the set that makes no readout, which runs three times in one batch                                                                                                                                                                                            |
| `refuses_a_tag_repeated_within_a_batch`                      | any executor                                                            | two instructions each making one readout, under different tags, and a third reading under the first's tag on another qubit                                                                                                                                                                      |
| `refuses_a_repeated_tag_at_the_index_of_its_instruction`     | an executor with an instruction that makes no readout                   | a reading instruction, a silent one, and another reading under the first's tag                                                                                                                                                                                                                  |
| `gives_each_shot_alone_what_it_gives_among_others`           | a `replayable` executor                                                 | at least 3 shots, and batches whose readouts differ between some two shots (a coin)                                                                                                                                                                                                             |
| `continues_a_shot_from_one_call_to_the_next`                 | an executor with `resumable_shots`                                      | a first batch that changes the shot, and a second that reads the change under other tags; without `replayable`, batches that draw nothing                                                                                                                                                       |
| `changes_no_shot_a_call_does_not_name`                       | an executor with `resumable_shots`                                      | at least 4 shots, a first batch that flips, and a second that reads; without `replayable`, batches that draw nothing                                                                                                                                                                            |
| `starts_its_shots_as_made_whatever_an_earlier_executor_left` | any executor that closes; no capability needed                          | a first batch that changes the shot, and a second that reads it under other tags                                                                                                                                                                                                                |
| `keeps_each_shots_conditions_from_one_call_to_the_next`      | an executor with `resumable_shots`, or an adapter that keeps conditions | batches over at least two calls: one that gives a shot an outcome carrying a condition, and a later one that reads that shot; it panics when none is raised, so it does not apply to a part that raises none; for an adapter over an executor whose replies end after the call, see its rustdoc |

Models of their use: [`tests/existing_tool.rs`](../tests/existing_tool.rs)
(an executor and a bridge), [`tests/runtime_isa.rs`](../tests/runtime_isa.rs)
and [`tests/static_isa.rs`](../tests/static_isa.rs) (adapters composed with
an executor), [`tests/simulator.rs`](../tests/simulator.rs) and
[`tests/per_shot_executor.rs`](../tests/per_shot_executor.rs).
[Conformance checks](composition.md#conformance-checks) states the rules.

You get for free here: **the conformance kit**
([What you get for free](#what-you-get-for-free)).

## The interfaces, for a part written by hand

These traits, and the data they exchange: a facility implements the traits for
the parts it makes, a part written by hand implements them itself, a program,
data, implements `Program`, and a driver of programs of form `P`,
`Driver<P>`. Condensed from `src/`: doc comments and
attributes trimmed but for `#[non_exhaustive]`, a default's body shown as
`{ ... }`, short items on one line:

```rust
pub trait Program {
    type Instruction: Instruction;
    type Output;
    fn needs(&self) -> Needs { ... }
}
pub trait Driver<P: Program> {
    fn needs(&self, program: &P) -> Needs { ... }
    fn check(&self, program: &P) -> Vec<Rejected> { ... }
    fn drive<E>(
        &self,
        program: P,
        executor: &mut E,
        shots: &ShotSet,
    ) -> impl Future<Output = P::Output>
    where
        E: Executor<InstructionBatch<P::Instruction>>;
}
pub trait Executor<M: Message> {
    type Reply: Reply<Item = M::Event>;
    fn call(&mut self, message: M::Ref<'_>) -> Result<Self::Reply, Rejected>;
}
pub trait Describe { fn capabilities(&self) -> Capabilities; }
pub trait Close {
    type Report: Fold;
    fn close(&mut self) -> Self::Report;
}
pub trait Fold: Default { fn fold(&mut self, other: Self); }
pub trait Reply {
    type Item;
    fn poll_events(&mut self, out: &mut Vec<Self::Item>, waker: &Waker) -> ReplyStatus;
}
#[non_exhaustive]
pub enum ReplyStatus { Pending, Done, Failed(String), Cancelled }
pub struct InstructionBatchRef<'a, I> { pub shots: &'a ShotSet, pub instructions: &'a [I] }
#[non_exhaustive]
pub struct ShotOutcome {
    pub shot: ShotId,
    pub tag: u32,
    pub value: Value,
    pub conditions: ShotConditions,
}
pub trait Adapter {
    type Accepts: Instruction;
    type Sends: Instruction;
    fn sends(&self) -> Option<Arc<InstructionSet>> { ... }
    fn accepts(&self) -> Option<Arc<InstructionSet>> { ... }
    fn draws(&self) -> Option<RngId> { ... }
    fn build(self, boundary: &mut AdapterBoundary<'_>) -> Self where Self: Sized { ... }
}
pub trait Apply<E>: Adapter {
    type Output: Executor<InstructionBatch<Self::Accepts>> + Close;
    fn apply(self, downstream: E) -> Self::Output;
}
```

- An executor is an `Executor<InstructionBatch<I>>`, a `Describe` and a
  `Close`: a call is an `InstructionBatchRef`
  ([one message](../README.md#the-protocol)), each event a `ShotOutcome`,
  made with `ShotOutcome::new`, which gives it no condition, and
  `.with_conditions(..)` for its shot's conditions
  ([Shot conditions](replies.md#shot-conditions)). `ExecutorLifecycle`
  keeps for it the rules of failure and close
  ([An executor written by hand](executors.md#an-executor-written-by-hand-executorlifecycle)),
  and its reply can be the crate's `ImmediateReply` or `IncrementalReply`
  ([The facilities](../README.md#the-facilities)).
- An instruction type `I` is `InstructionCall`, numbered by the set its
  sender states ([Instruction sets](instruction-sets.md)), or a type
  of your own marked `impl TypedInstruction for Op {}`, with no set; a
  chain of typed parts may describe none; `Instruction` is sealed.
- `()`, integers, ordered maps (`BTreeMap`) and pairs of reports `Fold`
  already. A fold's default is empty, and any order or grouping of the
  reports gives the same total, up to rounding for sums in floating point
  (`LogP`'s may differ in their last bits when regrouped):
  `conformance::default_is_empty` and
  `conformance::folds_in_any_order` check a report of your own.
- An adapter's defaults are for one that passes its calls down as they are
  ([Choosing a way](adapters.md#choosing-a-way)).
