# Notes for coding agents

For an agent that implements a part (a driver, an executor, an adapter) on
this crate, or changes the crate itself. The how is in
[Getting started](docs/getting-started.md); this page lists where things
are, what to read, the mistakes to avoid, how to test, and the gates.

## Where things are

| Path                                                               | What is there                                                                                                                                                         | Read it                                                                                                                                                                                                                                |
| ------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [`README.md`](README.md)                                           | what the crate is and why                                                                                                                                             | first: its first two paragraphs and the TL;DR are enough to start                                                                                                                                                                      |
| [`src/`](src/)                                                     | the protocol, one concept per file: `program.rs`, `driver.rs`, `executor.rs`, `adapter.rs`, `compose.rs`, `reply.rs`, `needs.rs`, `capabilities.rs`, `rejected.rs`, … | for a trait's exact contract, in its rustdoc                                                                                                                                                                                           |
| [`src/facilities/`](src/facilities/)                               | what most parts use day to day: `executor_fn`, `adapter_fn`, `static_driver`, `.run_on`, …                                                                            | when writing a part; a part they do not fit is written by hand on the traits of `src/` ([The interfaces](docs/getting-started.md#the-interfaces-for-a-part-written-by-hand), [`tests/executor_by_hand.rs`](tests/executor_by_hand.rs)) |
| [`src/conformance/`](src/conformance/)                             | the conformance kit (feature `conformance`)                                                                                                                           | before calling a check: its rustdoc says what it needs                                                                                                                                                                                 |
| [`tests/`](tests/)                                                 | the guide tests: complete parts and their tests                                                                                                                       | the model for your kind of part (Read order, 2), to copy                                                                                                                                                                               |
| [`src/tests/`](src/tests/)                                         | the unit tests, one file per module with run-time behaviour, one rule per test, each name stating its rule: they document the protocol's guarantees                   | to learn exactly what a rule promises                                                                                                                                                                                                  |
| [`docs/`](docs/)                                                   | the rules, by topic; [`docs/glossary.md`](docs/glossary.md) is the index of terms                                                                                     | for the rule you need                                                                                                                                                                                                                  |
| [`docs/internals/`](docs/internals/)                               | the promise tables: each rule with the tests that pin it                                                                                                              | the quickest way from a rule to its exact behaviour and a test of it                                                                                                                                                                   |
| [`docs/internals/design-cases.md`](docs/internals/design-cases.md) | why each abstraction exists: a case that needs it                                                                                                                     | before changing or removing an abstraction                                                                                                                                                                                             |
| [`docs/planned.md`](docs/planned.md)                               | what does not exist yet                                                                                                                                               | before looking for a feature: what is listed there is not built, so do not search for it or invent it                                                                                                                                  |
| [`benches/`](benches/)                                             | the benchmarks; [`docs/performance.md`](docs/performance.md) gives what they measure and their results                                                                | when cost matters                                                                                                                                                                                                                      |

## Read order

1. [Getting started](docs/getting-started.md), in full: up to six steps,
   each with a tested excerpt, and which your task takes.
2. The guide test for your kind of part, and copy its structure rather
   than the prose:

   | Part                                  | Model                                                                                                                                                      |
   | ------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
   | typed executor, a state per shot      | [`tests/toy/simulator.rs`](tests/toy/simulator.rs), [`tests/simulator.rs`](tests/simulator.rs), [`tests/per_shot_executor.rs`](tests/per_shot_executor.rs) |
   | driver of a program that branches     | [`tests/driver/if_one.rs`](tests/driver/if_one.rs), [`tests/driver.rs`](tests/driver.rs)                                                                   |
   | executor of calls by opcode, a bridge | [`tests/existing_tool/bridge.rs`](tests/existing_tool/bridge.rs), [`tests/runtime_isa/device.rs`](tests/runtime_isa/device.rs)                             |
   | adapter: observer                     | [`tests/observers/profiler.rs`](tests/observers/profiler.rs)                                                                                               |
   | adapter: code, static or run-time set | [`tests/static_isa.rs`](tests/static_isa.rs), [`tests/runtime_isa.rs`](tests/runtime_isa.rs) with [`tests/runtime_isa/code.rs`](tests/runtime_isa/code.rs) |
   | executor written by hand              | [`tests/executor_by_hand.rs`](tests/executor_by_hand.rs)                                                                                                   |
   | work held and completed later         | [`tests/debugger.rs`](tests/debugger.rs)                                                                                                                   |

3. `docs/` only for the rule you need; the
   [Glossary](docs/glossary.md) names the section of each term.

## Pitfalls

- Typed instructions: `impl TypedInstruction for Op {}`. A typed chain
  needs no `InstructionSet`; the compiler checks the type.
- Calls by opcode: build the `OpcodeTable` once (`set.table(..)`). Give
  an executor's `.check` a function that runs `InstructionSet::check_batch`,
  checks its own qubit range, and runs `check_unique_tags`; an adapter's
  `.accepts(set)` does not check calls yet, so give its `.check` one that
  runs `check_batch` too.
- Seeds: the run's seed is the last executor's `.seed(s)`;
  `chain.run(program, shots)` takes none. A state draws from
  `ShotContext::seed()`; `MakeState` takes the `ShotContext` by value.
- Shot conditions: an executor that offers `resumable_shots` keeps each
  shot's conditions from one call to the next, and nothing checks it
  before the run. An `executor_fn` does not by itself: its logic keeps
  them in the shot's state and raises them again in each call
  ([Shot conditions](docs/replies.md#shot-conditions)).
- A driver that implements `Driver<P>` for several `P` (or generic over
  its program) needs `DriverExt::<P>::into_chain(driver)`;
  `driver.into_chain()` fails with E0283.
- A function that makes a part for `>>` returns the facility's type
  (`OnePerShotExecutor<…, impl MakeState<S>, impl ExecutorLogic<I, S>>`),
  never `impl Executor`, or `>>` does not compose. Where Clippy finds the
  type too complex, add `#[expect(clippy::type_complexity, reason = "…")]`.
- Reports: `run.wait()` gives `(output, report)`. An adapter with a
  `.state_report` pairs its report with the one below,
  `(this adapter's report, the report below)`, nested in the chain's
  order: two over an `executor_fn` give
  `(first's, (second's, ExecutorReport { calls, log_p, states }))`; an
  adapter with none adds no pair.

## Testing your part

Your behaviour tests check your part's own logic (what it computes, sends
and reports); the conformance kit checks the protocol's rules on it. You
need both: when mistakes were planted in parts written from Getting
started, behaviour tests caught most of them, and the kit the rest.

- **Behaviour tests:** one rule per test, through the public API, its name
  stating the rule; both sides and the edges (refused and accepted, one
  shot and several, another order, shots that are not a range). Break
  each rule on purpose and see its test fail, then restore it. Model:
  [`tests/observers.rs`](tests/observers.rs), one rule per test (e.g.
  `the_profiler_changes_no_outcome`).
- **The kit** (feature `conformance`): run it on every part you write, the
  executors below your adapters and the adapters composed over them
  included, not only the first ones. It catches what behaviour tests
  miss: `refuses_calls_outside_its_set`, run on an adapter composed with
  its executor, fails unless each call outside the set is refused at its
  index. An `adapter_fn` that states `.sends` and sends a call on as it is
  refuses an opcode the set lacks when it renumbers it, but a logic that
  indexes a table by opcode can panic on it if its `.check` drops
  `check_batch`. Read each check's rustdoc before calling it
  ([Check your parts](docs/getting-started.md#6-check-your-parts) says
  what each needs). `keeps_each_shots_conditions_from_one_call_to_the_next`
  does not apply to a part that raises no condition.

## Lints and gates

The workspace's Clippy lints are `pedantic`, with `unwrap_used` and
`mod_module_files`: expect `must_use_candidate` on constructors, write no
`mod.rs` (a module `a` with children is `a.rs` beside `a/`), and use
`expect("why it holds")`, not `unwrap()`. Before you commit:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
```

## Changing this crate

- Each promise of the docs and rustdoc is listed with the tests that pin
  it in `docs/internals/`: change the row with the rule.
- Unit tests: one `src/tests/<module>.rs` per module with run-time
  behaviour, nested as in `src/`; one rule per test, named for the rule.
- Mutation-check each rule: break it, see its test fail, restore the file
  from a copy and check it with `cmp`.
- Code excerpts in the docs are copied from tests (`doctest = false`: no
  doc block is compiled); change the test, then the excerpt.
- The README and the user pages of `docs/` name no tests, only test
  files; test names go in `docs/internals/`.
