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

| `compose`                      | gives    | as                                                                      |
| ------------------------------ | -------- | ----------------------------------------------------------------------- |
| adapter, executor              | executor | the executor, its calls through the adapters                            |
| adapter, adapter               | adapter  | `ComposedAdapters`, the left one upstream                               |
| driver, adapter                | driver   | the driver (`InterpreterDriver` or `Driver`), the adapter its own       |
| interpreter's driver, executor | chain    | `Chain`, which runs the job given to it ([below](#a-chain-and-its-job)) |
| driver of a program, executor  | run      | `Run`, which runs when it is waited for                                 |

- The **executor**, on the right, is any executor of the run's batches
  that describes what it offers and closes with a report: the last
  executor, built with its seed, or adapters composed with it. A driver
  with no adapter runs on any executor, an ensemble included
  (`a_driver_composes_with_any_executor`).
- An **adapter** maps the executor downstream of it to the executor
  upstream. It is made by `adapter_fn`, by `owning_adapter_fn` for a state
  that keeps its way below, or by hand (`Apply`)
  ([Adapters](adapters.md)).
- A **driver** heads the chain. The crate's interpreter, `interpreter()`,
  makes a `Chain`, which runs the job given to it
  ([A chain and its job](#a-chain-and-its-job)).
  `Driver::new(program, shots)` binds a program that drives itself
  (`Drive`) to its shots, and composed with an executor gives its run
  directly. Either way, the run's seed is the one the last executor is
  built with ([Seeds](executors.md#seeds-not-generators)).
- A **run** owns the chain, and runs nothing until it is waited for or
  awaited (`a_run_runs_nothing_until_it_is_waited_for`,
  `a_run_turned_into_a_future_runs_nothing_until_polled`). Then
  `run.wait()`, or `run.await` for an asynchronous caller, drives the
  executor, closes it once its program ends, and gives the program's output
  and the executor's report
  (`a_run_gives_the_programs_output_and_the_executors_report`). An
  awaited run, and a run stopped before its program ends, have limits
  ([Known limits](internals/composition.md#known-limits)).
- A call on a shot the driver did not name is refused before the executor
  sees it
  (`a_call_on_a_shot_outside_the_drivers_is_refused_before_the_executor_sees_it`).

From `a_debugger_composed_into_a_chain_hands_out_its_handle`
(`tests/debugger.rs`): a driver, a paused debugger and the toy simulator,
composed into a run that waits on its own thread while this one steps
the debugger through its handle:

```rust
let shots = ShotSet::range(0..=1).expect("0..=1 is not empty");
let debuggers = Debuggers::paused();
let run = (Driver::new(FlipACoin, shots.clone()) >> debuggers.clone() >> toy_simulator(9))
    .expect("the debugger changes nothing the toy checks");
let driver = thread::spawn(move || run.wait());
```

The rules of composition:

- **Left is upstream, and the grouping does not matter.**
  `driver >> (a >> b) >> executor` is `driver >> a >> b >> executor`
  (`adapters_composed_put_the_left_upstream_of_the_right`,
  `the_grouping_of_a_drivers_chain_does_not_matter`).
- **The chain is built once, from the executor upstream, and every fault
  is reported at once**, the most upstream adapter's first, each named
  (`BuildFaults`), but for a part composed with the executor first, built on
  its own: `compose` gives it as a `Result`, which is unwrapped before the
  part is composed further, so its faults come back before what is upstream
  of it is built, or a driver's needs are checked
  (`a_part_composed_with_the_executor_first_reports_its_faults_before_upstream_is_built`,
  `a_part_composed_with_the_executor_first_reports_its_faults_before_a_drivers_needs_are_checked`;
  [The rules of composition](internals/composition.md#the-rules-of-composition)).
  From `a_chain_reports_every_fault_at_once_upstream_first`, where both
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

- **The driver is checked when it is composed with an executor**: every
  need the chain lacks is reported, before anything runs
  (`every_need_of_a_driver_that_the_chain_lacks_is_reported`).
- **Where an adapter's calls meet the executor downstream, the boundary is
  decided once, when the chain is built.** The compiler checks typed
  instructions. Calls by opcode are checked against the set downstream,
  and copied into its numbering where they are numbered otherwise
  (`calls_in_another_numbering_are_renumbered_on_their_way_down`). What
  is not compatible is refused when composed
  (`an_adapter_accepting_what_the_downstream_set_lacks_is_refused_when_built`).

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
`interpreter() >> adapters >> executor` builds and checks the chain
(`BuildFaults`), and `chain.run(program, shots)` gives the run of one job.
The crate's interpreter runs a `StaticProgram`, whose batches go each to
every shot of the job in one call, in order
(`a_static_program_gives_every_shots_outcomes_batch_after_batch`). It also
runs any program that drives itself (`Drive`), as its `Driver` runs on the
same chain (`a_drive_program_given_to_a_chain_runs_as_its_driver_runs`).

- **The job is checked when given, before any call**: its shots, its
  program's needs and its program's calls, every fault together
  (`JobFaults`)
  (`a_jobs_shot_faults_are_reported_with_its_programs_needs_and_calls`),
  but for shots named twice: of several, only the least is reported
  (`a_job_that_names_several_shots_twice_reports_the_least`).
- **A job calls only its own shots**: a call on another is refused before
  the executor sees it (`a_job_that_calls_a_shot_not_its_own_is_refused`).
- **The run gives the outcomes with the chain's report**, waited for or
  awaited alike (`a_job_awaited_gives_what_it_gives_waited_for`). A
  `StaticProgram` stops at the first batch that the chain refuses or that
  fails (`a_static_program_stops_at_the_first_batch_that_fails`).
- **A chain runs one job.** A function that builds the chain builds it
  again for the next job
  (`a_replayable_chain_built_again_runs_a_shot_again_with_the_same_outcomes`).
  A chain that runs many jobs is planned, with no milestone yet.

Each rule's edges and every test are in
[A chain and its job: the rules](internals/composition.md#a-chain-and-its-job-the-rules).

## What is checked, and when

| When                           | What                                                                                                                                                                                                                                                                                        | How                                                                                                                                                                   |
| ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| compile time                   | an executor wraps only an executor of the messages it sends; a reply's events are its message's events                                                                                                                                                                                      | `Executor<M>` bounds; `Reply: Reply<Item = M::Event>` (compile errors, which no test pins)                                                                            |
| compile time                   | a batch's instructions are of the type the executor accepts                                                                                                                                                                                                                                 | the instruction type, `InstructionBatch<I>` (a compile error, which no test pins)                                                                                     |
| when the shots are built       | a set of shots names at least one shot, and each shot once                                                                                                                                                                                                                                  | `ShotSet::new` (`Rejected::{NoShots, RepeatedShot}`), `ShotSet::range` (`Rejected::NoShots`); `a_shot_set_names_at_least_one_shot`, `a_shot_set_names_each_shot_once` |
| when a chain is built          | what an adapter sends is in the described set downstream, matched once; no two components draw with one RNG id; every fault of a chain at once (a part composed with the executor first reports only its own faults, before what is upstream is built: [Composing a run](#composing-a-run)) | `compose` (`BuildFaults`), or `Capabilities::drawing` by hand                                                                                                         |
| when a driver is composed      | the caller's needs fit what each component offers, the instructions it will send included, and its shots the states the executor holds                                                                                                                                                      | `compose` (`BuildError::Unmet`, every need at once); by hand, `Capabilities::admit` (`Rejected::Unmet`, naming the component)                                         |
| when a job is given to a chain | its shots are a set, no more than the executor holds; its program's needs fit what the chain offers; the interpreter finds no fault in its program                                                                                                                                          | `Chain::run` (`JobFaults`, every fault at once, but only the least of the shots named twice): [A chain and its job](#a-chain-and-its-job)                             |
| at a shot's first call         | the shot's state can be made                                                                                                                                                                                                                                                                | `make_state` of `executor_fn` or `adapter_fn` (its refusal refuses the call)                                                                                          |
| at each call                   | a batch's shots against the driver's or the job's; a batch of `InstructionCall`s against the executor's set; a batch's shots against those the executor holds; a closed or failed executor                                                                                                  | `Run` (`Rejected::ShotNotHeld`); `InstructionSet::check_batch`; `Rejected::{ShotNotHeld, Closed, EarlierFailure}` (`executor_fn`, `ExecutorLifecycle`)                |
| at each call                   | within a batch, a tag names one readout of each shot, where an executor reports by tag ([readout tags](replies.md#readout-tags))                                                                                                                                                            | `check_unique_tags` (`Rejected::Instruction`)                                                                                                                         |

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
  `gives_each_shot_alone_what_it_gives_among_others`) and `feedback` (a
  second call continues a shot, `continues_a_shot_from_one_call_to_the_next`,
  and changes no shot it does not name,
  `changes_no_shot_a_call_does_not_name`);
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

| User                                    | Builds                                                                                                 | Shown by                                                                              |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------- |
| a program with feedback, on one shot    | `coin_then_feedback(&mut toy_simulator(seed), j)`, which calls with `ShotSet::one(j)`                  | `the_debugger_changes_no_outcome_and_no_seed` (`tests/debugger.rs`)                   |
| a static program, observed              | `(interpreter() >> profiler() >> debugger(log.clone()) >> simulator())?.run(program(), SHOTS)?.wait()` | `stacked_observers_each_see_what_they_see_alone` (`tests/observers.rs`)               |
| a driver of many shots, with a debugger | `(Driver::new(program, shots) >> debuggers.clone() >> toy_simulator(seed))?`                           | `a_debugger_composed_into_a_chain_hands_out_its_handle` (`tests/debugger.rs`)         |
| a logical driver                        | `(Driver::new(program, shots) >> RepetitionCode::default() >> toy_simulator(seed))?.wait()`            | `a_logical_run_draws_each_shots_coins_from_its_physical_seed` (`tests/static_isa.rs`) |

Another user, a driver whose executor is on another machine, is planned,
with no milestone yet: the link and its layers L1 and L0 are inside the
remote executor
([Levels, roles and laws](internals/composition.md#levels-roles-and-laws)), so
its chain is written as the others are.

A program with feedback needs each call's outcomes before it builds the
next, so it may drive one shot at a time: called on shot `j` alone, a
replayable executor gives shot `j` the outcomes it gives among the others
(law 2, "a shot alone", of
[Levels, roles and laws](internals/composition.md#levels-roles-and-laws)).
An executor called on one shot only (here a `toy_simulator(seed)` called on
shot `j` alone) holds that shot's state. Run one after another, such
executors hold one state at a time; run concurrently, they hold one state
each. This follows by construction from `OnePerShot`, which makes a shot's
state at the shot's first call
(`a_shots_state_is_made_at_its_first_call_from_the_runs_seed_and_the_shot`,
`src/tests/executor_fn/one_per_shot.rs`).

The protocol's rules that these pages build on, the levels, roles and laws,
and the other detailed rules, for maintainers of the crate, are in
[Composition: the detailed rules](internals/composition.md).

What is planned, with what to do until then, is in
[Planned features](planned.md).
