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

The smallest adapter is a state and a logic
(`adapter_fn(make_state, logic)`). `make_state` makes the adapter's working
state (a profiler's counts, a decoder's history) at the first call, from the
context of that call's first shot (`MakeState`), and that one state serves
every call of every shot. So it belongs to the adapter, not to a shot: a
profiler's counts sum all the shots it served. For a state per shot, use
`.shots(OnePerShot)` ([below](#a-state-per-shot-shotsonepershot)). An
executor's state, by contrast, always belongs to one shot
([Executors](executors.md)). Tests:
`one_state_serves_every_call_of_every_shot`,
`a_one_state_adapters_state_spans_the_shots_of_its_calls`.

A state is made from its shot's context and what `make_state` captured when
the adapter was built; what varies per call goes to the logic
([What a state depends on](internals/adapters.md#what-a-state-depends-on),
in the detailed rules).

The logic serves each call. With the call it is given, it may:

- **send it below as it is**, with `below.send(..)`: it observes, as a
  profiler does, and the chain gives the outcomes it gives with no
  adapter
  (`a_logic_that_sends_each_call_gives_the_outcomes_of_no_adapter`);
- **send other instructions below**, tags kept, with `below.send(..)`: it
  rewrites, and states its sets with `.accepts(set)` and `.sends(set)`
  (`a_logic_that_rewrites_each_call_keeps_its_tags`);
- **run instructions below**, with `below.run(..)`: it is given their
  readouts by shot and decides on them, as a code that decodes does, or
  records them, as a debugger does
  (`a_run_gives_its_readouts_by_shot_each_in_the_order_read`,
  `the_debugger_logs_each_shots_calls_with_what_they_read`);
- **write outcomes**, with `outcomes`: what a run reads passes up only as
  the logic writes it
  (`what_a_run_reads_passes_up_only_as_the_logic_writes_it`).

What each costs: a `send` is one call below on the call's shots
(`each_send_is_one_call_below`), and does not wait for its reply: once the call
below returns, the reply passes up as it comes
(`a_reply_below_passes_up_as_it_comes`); an executor below that does its work
within the call holds the thread until the call returns. A `run` returns the
readouts, so it holds the thread until they are in; with a state per shot, a
shot's run goes below on that shot alone
(`a_shots_run_goes_below_on_that_shot_alone`). A consumer matches readouts by
their tags, not by their place in the reply
([The order of a run's readouts](internals/adapters.md#the-order-of-a-runs-readouts)).

The repetition code is a logic that lowers a call, runs it and writes each
block's majority
(`the_code_as_a_logic_reads_what_its_adapter_reads`, `tests/static_isa.rs`).
Its logic, in full
apart from the majorities (`tests/static_isa/repetition_fn.rs`):

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

Unless its states draw, it offers what is below it, log probabilities
included (`it_offers_what_the_executor_below_offers`).

## A state per shot: `.shots(OnePerShot)`

Use a state per shot when the state is a shot's, such as a code's runtime
whose results name no shot
([below](#a-state-that-owns-its-way-below-owning_adapter_fn)). With
`.shots(OnePerShot)`, the same per-shot form as an executor's, `make_state`
makes a state per shot, at the shot's first call, from its context, and
keeps it for the shot's later calls
(`each_shot_has_a_state_of_its_own_kept_for_its_later_calls`). Below a
replayable executor, a shot gives alone what it gives among others
(`a_shot_gives_alone_what_it_gives_among_the_others`).

A shot's `k`-th send goes below with the other shots' `k`-th sends equal to
it, as one call over the shots that sent them, rank by rank
(`equal_sends_go_below_as_one_call_over_their_shots`); sends of different
ranks, and a shot's kept sends at its run, go apart
([Merging a call's sends](internals/adapters.md#merging-a-calls-sends-rank-by-rank)).
For a profiler, the shot form changes what it counts, per shot or per call, not
the outcomes
(`the_profilers_shot_form_changes_its_counts_not_the_outcomes`).

## Seeds: `.drawing(id)`

An adapter whose states draw states the id it draws with, `.drawing(id)`,
and its shot `j`'s state draws from shot `j`'s seed in the run bound to that
id (`a_drawing_adapters_states_seed_from_their_shot_bound_to_its_id`), so
that it offers replayable shots as the executor below does
(`a_drawing_adapter_offers_replayable_shots`), and a shot draws alone
what it draws among the others
(`a_drawing_adapters_shot_draws_alone_what_it_draws_among_the_others`);
with one state, the state seeds from the first shot it serves, and the
adapter offers no replayable shots
(`a_drawing_adapter_is_not_replayable`).

The run's seed is the one the last executor below is given with
`.seed(seed)`, through any adapters between
([Seeds](executors.md#seeds-not-generators)), so one seed replays the
chain (`a_chain_with_a_drawing_adapter_replays_from_its_one_seed`),
another changes the adapter's draws
(`a_change_of_the_runs_seed_changes_a_drawing_adapters_draws`), and two
adapters that draw each seed under their own id
(`two_drawing_adapters_seed_from_the_one_seed_each_with_its_own_id`).
An id drawn with below, however far down, refuses the chain when it is
built
(`an_adapter_drawing_with_an_id_an_adapter_below_draws_with_is_refused`),
and so does a chain where no run's seed reaches the adapter, as below an
executor written by hand, which cannot state one yet
(`BuildError::NoRunSeed`,
`an_adapter_drawing_with_no_run_seed_below_is_refused`).

## Reports: `.state_report(f)`

Given `.state_report(f)`, the same option as an executor's, an adapter
reports with `f` each state it holds when it closes, folded, beside the
report below, as a pair: its own first
(`a_one_state_adapters_report_is_its_state_beside_the_report_below`,
`a_counting_adapters_report_is_folded_over_its_shots`). A state made for a
call that failed before it returned, or panicked, is not reported
(`a_failed_call_reports_no_state_made_for_it`,
`a_call_that_panicked_reports_no_state_made_for_it`;
[Reports of failed calls](internals/adapters.md#reports-of-failed-calls)).

A chain's report holds one such pair per adapter given a `state_report`,
nested in the chain's order, and an adapter given none adds none, passing
the report below up as it is
(`an_adapter_given_no_state_report_adds_no_pair_to_the_report`), so the
report is the same under every grouping of `>>`
(`a_runs_report_is_the_same_under_every_grouping_of_shr`), and
`run.wait()` gives it
(`a_runs_wait_gives_the_adapters_report_beside_the_one_below`). Through
an adapter that sends each call down as one, the report's `calls` are
those the executor alone reports
(`a_run_through_an_adapter_reports_the_calls_its_executor_alone_reports`).

No adapter has a log probability of its own, whether a facility made it
or it was written by hand: a run's report
passes up the last executor's, each shot's log probability included
(`a_run_reports_the_last_executors_log_probability_through_its_adapters`).
An adapter that draws offers no `probabilities`, since the log probability
downstream does not hold its draws: a driver that needs them is refused
(`a_driver_needing_probabilities_is_refused_upstream_of_an_adapter_that_draws`).
Known issue: the run of a driver that does not need them still reports
the log probabilities of the executor downstream, which miss the
adapter's draws
(`a_run_through_an_adapter_that_draws_reports_no_log_probability`,
ignored, for either kind of adapter); it goes with `.log_probability` on
adapters, planned, with no milestone yet, for a noise model whose run
reports each shot's log probability
([Planned features](planned.md)).

## A state that owns its way below: `owning_adapter_fn`

A state that keeps its way below for its whole life, such as a code's
runtime made once per shot over a backend it owns, is made by
`owning_adapter_fn(make_state, logic)`: `make_state` is given the way
below (`OwnedBelow`) to keep, and the logic is lent none. It is the same
adapter, with the same options and per-shot form: what the state sends
or runs through it goes below as through a lent one
(`owned_sends_go_below_as_lent_ones_do`), by the same refusal and failure
rules
([The way below of an owning state](internals/adapters.md#the-way-below-of-an-owning-state)).

A runtime whose results name no shot, as a code's runtime's, needs a
state per shot (`.shots(OnePerShot)`): with one state, a run below gives
it the readouts of every shot where it expects one shot's, and the
guide's glue, which checks that the reads match its readouts one to
one, fails the call at its first batch that reads (`tests/static_isa.rs`:
`a_runtime_with_one_state_fails_its_first_reading_call`).

The guide's runtime, which owns its backend as a Qodec runtime does, is
made a shot's state over the way below it owns
(`tests/static_isa/code_runtime.rs`):

```rust
pub fn instantiated(
    plan: CodePlan,
) -> impl FnMut(ShotContext, OwnedBelow<Op>) -> Result<CodeRuntime<Lower>, Rejected> {
    move |_, below| Ok(plan.instantiate(Lower(below)))
}
```

and made an adapter, one state per shot, with no change to it
(`tests/static_isa.rs`:
`a_runtime_that_owns_its_backend_reads_what_the_code_as_a_logic_reads`):

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
(`Apply`), when the chain is built, and its executor is called with each
batch on all its shots
(`an_adapter_written_by_hand_is_applied_once_to_the_whole_executor`):
`RepetitionCode` (`tests/static_isa/repetition_code.rs`) puts a
`Repetition3` upstream of the executor it is given, and `Debuggers`
(`tests/debugger/debuggers.rs`) a `Debugger`. An adapter is a few lines
around the executor it makes. Each method of `Adapter` has a default for an
adapter that changes nothing (the debugger implements none); the
repetition code states what it accepts and sends, and, built once against
the executor downstream (`Adapter::build`), what it does to the qubits
there:

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

An adapter keeps an executor's rules. The tests of this section are in
`src/tests/adapter/adapter_fn.rs` unless named elsewhere; a test of the
same name in `src/tests/adapter/adapter_fn/one_per_shot.rs`, where there
is one, pins the rule for a state per shot.

- **Its check refuses a call before its state is made.** `.check(check)`
  states what the adapter admits beside what it offers (the Promise axis:
  [Glossary](glossary.md#a-parts-axes)), and refuses a call before its
  state is made and its logic runs, as an executor's check does
  (`the_check_refuses_a_call_before_the_state_is_made_and_the_logic_runs`).
  A `make_state` that refuses refuses the call too, before the logic runs,
  and leaves nothing made
  (`a_state_that_cannot_be_made_refuses_the_call_before_the_logic_runs`).
- **A refusal below refuses the call.** The first call below, a send or a
  run, refused, refuses the call with the same refusal
  (`a_refused_first_send_refuses_the_call_whatever_the_logic_returns`,
  `a_first_run_refused_refuses_the_call_whatever_the_logic_returns`). The
  index it names is that of the instruction in the call below, not in the
  call (`a_refusal_below_names_the_instruction_as_sent`; planned, a way for
  the logic to map it). A call refused below is refused though its logic
  ran, so a logic changes its state only after `send(..)?` or `run(..)?`.
  With a state per shot, a send gives no such order: `send` returns `Ok`
  before the shots' sends go below merged
  (`a_send_returns_ok_since_nothing_goes_down_until_every_shot_ran`), so a
  call refused there leaves the kept states as its logic left them
  (`a_refused_call_leaves_the_kept_states_as_its_logic_left_them`, both in
  `src/tests/adapter/adapter_fn/one_per_shot.rs`).
- **An error or a failure below fails the adapter.** The logic's error, or
  a failure below, fails the call and the adapter
  (`a_logic_that_fails_fails_the_adapter`,
  `a_failure_below_fails_the_adapter`), and nothing more goes down
  (`nothing_goes_down_after_a_send_whose_reply_failed`).
- **Closing closes below once.** Closing it gives the report below
  (`closing_gives_the_report_below`), closing what is below once, whatever
  failed (`a_second_close_closes_nothing_below`,
  `a_failed_adapter_closes_what_is_below`).

So a profiler or a decoder over a simulator fails as the simulator alone
does: closed, it refuses every call
(`a_closed_profiler_refuses_every_call`). The other failures, and the rest
of what the conformance kit checks, are in the detailed rules
([Failures in detail](internals/adapters.md#failures-in-detail),
[The conformance kit](internals/adapters.md#the-conformance-kit)).

The detailed rules, for maintainers of the crate, are in
[Adapters: the detailed rules](internals/adapters.md).
