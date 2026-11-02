# Composition

[Protocol overview](../README.md).

A driver, adapters and an executor are put together into a chain, and a
run drives it; the parts and the run are introduced in the README
([The model](../README.md#the-model-a-chain-and-its-run)). This page is
about putting them together: composing a run with `compose` or `>>`, a
chain and its job, what is checked and when, the conformance checks an
implementer runs on its executors, and chains for several users.

## Composing a run

A driver, the adapters downstream of it and an executor are put together by
`compose`, which takes two of them and gives one; `a >> b` is the same as
`compose(a, b)` where `>>` is implemented
([Which parts the operator joins](internals/composition.md#which-parts-the-operator-joins)):

| `compose`            | gives      | as                                                                      |
| -------------------- | ---------- | ----------------------------------------------------------------------- |
| adapter, executor    | executor   | the executor, its calls through the adapters                            |
| adapter, adapter     | adapter    | `ComposedAdapters`, the left one upstream                               |
| chain head, adapter  | chain head | the driver with its adapters so far (`ChainHead`), the adapter the last |
| chain head, executor | chain      | `Chain`, which runs the job given to it ([below](#a-chain-and-its-job)) |

- The **executor**, on the right, is any executor of the run's batches that
  describes what it offers and closes with a report: the last executor, built
  with its seed, or adapters composed with it. A chain's head with no adapter
  runs on any executor, an ensemble included.
- An **adapter** maps the executor downstream of it to the executor
  upstream. It is made by `adapter_fn`, by `owning_adapter_fn` for a state
  that keeps its way below, or by hand (`Apply`)
  ([Adapters](adapters.md)).
- A **driver** (`Driver<P>`) heads the chain, and runs any program of form
  `P` given to it. In the chain being built it sits in a **chain head**
  (`ChainHead`), with the adapters composed below it so far: the crate's
  driver, `driver()`, is a head with no adapter yet; `ChainHead::new(driver)`
  makes one of any driver. A head with adapters below it is a `ChainHead`
  still, not a `Driver<P>` (planned: [Planned features](planned.md)).
  Composed with the executor, a head makes a `Chain`, which runs the job given
  to it ([A chain and its job](#a-chain-and-its-job)), an adaptive program
  (`AdaptiveProgram`) included. The run's seed is the one the last executor is
  built with ([Seeds](executors.md#seeds-not-generators)).
- A **run** owns the chain, and runs nothing until it is waited for or awaited.
  Then `run.wait()`, or `run.await` for an asynchronous caller, drives the
  executor, closes it once its program ends, and gives the program's output and
  the executor's report. An awaited run, and a run stopped before its program
  ends, have limits ([Known limits](internals/composition.md#known-limits)).
- A call on a shot the job did not name is refused before the executor sees
  it.

From the [debugger guide](../tests/debugger.rs): the crate's driver, a
paused debugger and the toy simulator, composed into a chain whose run waits on
its own thread while this one steps the debugger through its handle:

```rust
let shots = ShotSet::range(0..=1).expect("0..=1 is not empty");
let debuggers = Debuggers::paused();
let run = (driver() >> debuggers.clone() >> toy_simulator(9))
    .expect("the debugger changes nothing the toy checks")
    .run(FlipACoin, shots.clone())
    .expect("the program needs nothing");
let run_thread = thread::spawn(move || run.wait());
```

The rules of composition:

- **Left is upstream, and the grouping does not matter.** Both
  `(driver() >> a) >> executor` and `driver() >> (a >> executor)?` (the
  executor `a >> executor` gives, unwrapped, below) build a chain with the
  same run; so does any other grouping of `driver() >> a >> b >> executor`.
  The head, with or without adapters, is a `ChainHead`, not yet a
  `Driver<P>` itself (planned).
- **The chain is built once, from the executor upstream, and every fault is
  reported at once**, the most upstream adapter's first, each named
  (`BuildFaults`), but for a part composed with the executor first, built on its
  own: `compose` gives it as a `Result`, which is unwrapped before the part is
  composed further, so its faults come back before what is upstream of it is
  built ([The rules of
  composition](internals/composition.md#the-rules-of-composition)). From the
  [tests of composed adapters](../src/tests/compose/adapters.rs), where both
  adapters are faulty:

  ```rust
  let sends_h = bits_set().with("h", [Block]);
  let noise =
      map_executor::<InstructionCall, _>(|below| below).drawing(ComponentId::LAST_EXECUTOR);
  let Err(faults) = compose(
      compose(Relabel::new(sends_h, false), noise),
      TestDoubleExecutor::with_model(Bits::default(), 7),
  ) else {
      panic!("both adapters are faulty");
  };
  ```

- **A job is checked when it is given to the chain**: every need of its program
  that the chain lacks is reported, before anything runs
  ([A chain and its job](#a-chain-and-its-job)).
- **Where an adapter's calls meet the executor downstream, the boundary is
  decided once, when the chain is built.** The compiler checks typed
  instructions. Calls by opcode are checked against the set downstream, and
  copied into its numbering where they are numbered otherwise. What is not
  compatible is refused when composed.

The parts in full, each rule with its tests, every boundary of calls by
opcode, and what a boundary costs, are in
[The parts of a run](internals/composition.md#the-parts-of-a-run) and
[The rules of composition](internals/composition.md#the-rules-of-composition).
A caller that pushes calls itself, such as an FFI runner or a Python
binding, holds an executor instead of a run
([The executor form](internals/composition.md#the-executor-form-and-composing-by-hand)).

Since the run owns the chain, a caller reaches a component in it only
through state shared before it was composed: the debugger guide keeps a
clone of its `Debuggers`, which hands out the debugger's handle
(`Debuggers::handle`).

### A chain and its job

The job is the chain's input, not a component:
`driver() >> adapters >> executor` builds and checks the chain
(`BuildFaults`), and `chain.run(program, shots)` gives the run of one job. The
crate's driver runs a `StaticProgram`, whose batches go each to every shot
of the job in one call, in order. It also runs any adaptive program
(`AdaptiveProgram`), which it lets drive.

- **The job is checked when given, before any call**: its shots, its program's
  needs and its program's calls, every fault together (`JobFaults`), but for
  shots named twice: of several, only the least is reported.
- **A job calls only its own shots**: a call on another is refused before the
  executor sees it.
- **The run gives the outcomes with the chain's report**, waited for or awaited
  alike. A `StaticProgram` stops at the first batch that the chain refuses or
  that fails.
- **A chain runs one job.** A function that builds the chain builds it again for
  the next job. A chain that runs many jobs is planned, with no milestone yet.

Each rule's edges and every test are in
[A chain and its job: the rules](internals/composition.md#a-chain-and-its-job-the-rules).

## What is checked, and when

| When                           | What                                                                                                                                                                                                                                                                                        | How                                                                                                                                                                                                                 |
| ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| compile time                   | an executor wraps only an executor of the messages it sends; a reply's events are its message's events                                                                                                                                                                                      | `Executor<M>` bounds; `Reply: Reply<Item = M::Event>` (compile errors, which no test pins)                                                                                                                          |
| compile time                   | a batch's instructions are of the type the executor accepts                                                                                                                                                                                                                                 | the instruction type, `InstructionBatch<I>` (a compile error, which no test pins)                                                                                                                                   |
| when the shots are built       | a set of shots names at least one shot, and each shot once                                                                                                                                                                                                                                  | `ShotSet::new` (`Rejected::{NoShots, RepeatedShot}`), `ShotSet::range` (`Rejected::NoShots`)                                                                                                                        |
| when a chain is built          | what an adapter sends is in the described set downstream, matched once; no two components draw with one RNG id; every fault of a chain at once (a part composed with the executor first reports only its own faults, before what is upstream is built: [Composing a run](#composing-a-run)) | `compose` (`BuildFaults`), or `Capabilities::drawing` by hand                                                                                                                                                       |
| when a job is given to a chain | its shots are a set, no more than the executor holds; its program's needs fit what the chain offers, the instructions it will send included; the driver finds no fault in its program                                                                                                       | `Chain::run` (`JobFaults`, every fault at once, but only the least of the shots named twice): [A chain and its job](#a-chain-and-its-job); by hand, `Capabilities::admit` (`Rejected::Unmet`, naming the component) |
| at a shot's first call         | the shot's state can be made                                                                                                                                                                                                                                                                | `make_state` of `executor_fn` or `adapter_fn` (its refusal refuses the call)                                                                                                                                        |
| at each call                   | a batch's shots against the job's; a batch of `InstructionCall`s against the executor's set; a batch's shots against those the executor holds; a closed or failed executor                                                                                                                  | `Run` (`Rejected::ShotNotHeld`); `InstructionSet::check_batch`; `Rejected::{ShotNotHeld, Closed, EarlierFailure}` (`executor_fn`, `ExecutorLifecycle`)                                                              |
| at each call                   | within a batch, a tag names one readout of each shot, where an executor reports by tag ([readout tags](replies.md#readout-tags))                                                                                                                                                            | `check_unique_tags` (`Rejected::Instruction`)                                                                                                                                                                       |

Placed whole (`.run_on(..)` alone, or after `.shots(..)`), an `executor_fn`
refuses on its own thread, after `call` accepted the call: a refusal of its
check, of its `make_state` or for want of room fails the call's reply, and the
executor refuses every later call ([Where an executor
runs](executors.md#where-an-executor-runs-run_on)).

## Conformance checks

The rules that the compiler cannot check are checks in `conformance` (the
`conformance` feature), which an implementer runs on its executors in its
own tests:

- those of `call`: a refusal leaves the executor usable; a failure is final
  where state carries over, and ends the later calls still in flight once
  it is recorded as the executor's
  (`ends_calls_in_flight_at_an_earlier_failure`, which polls the failing
  call's reply first;
  [An executor fails](internals/executors.md#an-executor-fails) says what
  may pass up before);
- that of a reply: it appends its events, keeping what the caller held,
  checked wherever a check reads readouts;
- those of `close`: a closed executor refuses calls with `Closed`, and a
  report is given once;
- those of `fold`: the empty report adds nothing, and any order gives the
  same total;
- that a call outside the described instruction set, or a readout tag
  repeated within a batch, is refused, the latter at the index of the
  instruction that repeats it
  (`refuses_a_repeated_tag_at_the_index_of_its_instruction`);
- those of the capabilities `replayable` (a shot alone, or among shots
  split into calls in another order, gives what it gives among others,
  `gives_each_shot_alone_what_it_gives_among_others`) and `resumable_shots` (a
  second call continues a shot, `continues_a_shot_from_one_call_to_the_next`,
  and changes no shot it does not name,
  `changes_no_shot_a_call_does_not_name`);
- that of a shot's conditions: each outcome of a shot carries every
  condition its outcomes in earlier calls carried
  (`keeps_each_shots_conditions_from_one_call_to_the_next`); and the checks
  of a shot alone, of the shots a call does not name and of a new
  executor compare each outcome's conditions with its value, so that a
  condition raised on one shot marks no other's outcomes, and a new
  executor's shots start without the conditions an earlier executor left
  ([Shot conditions](replies.md#shot-conditions));
- that a new executor starts its shots as made, whatever an earlier one
  left, closed or dropped
  (`starts_its_shots_as_made_whatever_an_earlier_executor_left`), a rule
  every check that builds several executors relies on.

A check of a rule or a need on each shot names every shot that breaks it,
so that one run reports every fault.

## Chains for several users

One simulator serves several users: each builds the chain it needs on an
executor of its own, made from the toy simulator of the guide tests
(`toy_simulator(seed)`, an executor with a state per shot):

| User                                    | Builds                                                                                            | Shown in                                                   |
| --------------------------------------- | ------------------------------------------------------------------------------------------------- | ---------------------------------------------------------- |
| a program with feedback, on one shot    | `coin_then_feedback(&mut toy_simulator(seed), j)`, which calls with `ShotSet::one(j)`             | the [debugger guide](../tests/debugger.rs)                 |
| a static program, observed              | `(driver() >> profiler() >> debugger(log.clone()) >> simulator())?.run(program(), SHOTS)?.wait()` | the [observer guide](../tests/observers.rs)                |
| a program of many shots, with debugging | `(driver() >> debuggers.clone() >> toy_simulator(seed))?.run(program, shots)?`                    | the [debugger guide](../tests/debugger.rs)                 |
| a logical program                       | `(driver() >> RepetitionCode::default() >> toy_simulator(seed))?.run(program, shots)?.wait()`     | the [static instruction set guide](../tests/static_isa.rs) |

Another user, a program whose executor is on another machine, is planned,
with no milestone yet: the link and its layers L1 and L0 are inside the
remote executor
([Levels, roles and laws](internals/composition.md#levels-roles-and-laws)), so
its chain is written as the others are.

A program with feedback needs each call's outcomes before it builds the next, so
it may drive one shot at a time: called on shot `j` alone, a replayable executor
gives shot `j` the outcomes it gives among the others (law 2, "a shot alone", of
[Levels, roles and laws](internals/composition.md#levels-roles-and-laws)). An
executor called on one shot only (here a `toy_simulator(seed)` called on shot
`j` alone) holds that shot's state. Run one after another, such executors hold
one state at a time; run concurrently, they hold one state each. This follows by
construction from `OnePerShot`, which makes a shot's state at the shot's first
call.

The protocol's rules that these pages build on, the levels, roles and laws,
and the other detailed rules, for maintainers of the crate, are in
[Composition: the detailed rules](internals/composition.md).

What is planned, with what to do until then, is in
[Planned features](planned.md).
