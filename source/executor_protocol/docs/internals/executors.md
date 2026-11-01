# Executors: the detailed rules

[Protocol overview](../../README.md).

This page is for maintainers of the crate, and for those who need a rule
at its edge: how a capability is added; the rules of `executor_fn`, its
panics, drops and reports; those of a state per shot and of a placed
executor, and how the order of `.shots` and `.run_on` changes them; how an
executor's close and failure are kept; an executor written by hand and an
ensemble; and how seeds are derived. A user writing an executor needs none
of it to start, and starts at [Executors](../executors.md).

## Adding a capability

A property proposed for the protocol is first placed as one of four:

- a **law** if every component must keep it, as laws 1 and 3 of
  [Levels, roles and laws](composition.md#levels-roles-and-laws);
- a **type or trait bound** if the chain's types already know it and a
  component that lacks it is worth a compile error, as an executor
  accepts only the instructions of its type
  (`Executor<InstructionBatch<I>>`);
- a **capability and a need** if it depends on the component or on how it
  is configured, some components lack it, and a driver relies on it for a
  whole run: the table of
  [Capabilities and needs](../executors.md#capabilities-and-needs);
- a **field of the message** (`InstructionBatch`) if it changes from call
  to call.

`Capabilities` and `Needs` are `#[non_exhaustive]` and built from their
default and `with_` methods, so a new capability is a new field whose
default keeps every existing executor and caller as they were; `Need` is
`#[non_exhaustive]` too, so a new variant breaks no match outside this
crate. Adding a capability takes five steps:

1. A field in `Capabilities`, whose default is what an executor that says
   nothing of it offers (as `probabilities` defaults to `false`).
2. A field in `Needs`, whose default states nothing (`false`, `None`), so
   no existing driver changes.
3. A `Need` variant, its check in `Capabilities::admit`, and its code in
   `need_code` and `need_of`: a new code, never one used before
   (Codes, in [What this crate does](composition.md#what-this-crate-does)).
4. Each adapter passes the new field on, maps it, or clears it. What an
   adapter offers upstream starts as what downstream offers, so an
   adapter that does not touch the field passes it on unchanged
   (`an_adapter_keeps_the_capabilities_it_does_not_change`,
   `src/tests/compose/adapters.rs`), and so promises it, whether or not
   it can keep it. An adapter that cannot keep the promise clears it in
   its `build` (`AdapterBoundary::offered_upstream`), as a sampler that
   returns values only clears `probabilities`
   (`an_adapter_turning_probabilities_off_offers_none`,
   `src/tests/compose/adapters.rs`); where no adapter of a kind can keep
   it, `compose` clears it before the adapter is built, as it clears
   `probabilities` upstream of an adapter that draws
   (`an_adapter_that_draws_reads_no_probabilities_in_its_offer_when_built`,
   `src/tests/adapter/boundary.rs`;
   `a_driver_needing_probabilities_is_refused_upstream_of_an_adapter_that_draws`,
   `src/tests/compose/adapters.rs`). So adding the field means going
   through each existing adapter, the guides' included.
5. The tests: a driver with the need, composed with an executor that
   offers it and with one that does not (`BuildError::Unmet`, naming the
   component); through an adapter; the refusal at the call, if a call can
   need it without the driver stating it; and across a link, once the
   link is built (planned).

Two capabilities are planned, with no milestone yet, each added by these
steps:

| Capability (planned)  | What it offers                                                                                                  | Who could offer it                                                                                                        | A case that would need it                                                                                                                                      |
| --------------------- | --------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `recovers`            | a call that fails does not fail the executor: its shots' states are restored, and the same call sent again runs | an executor that keeps a copy of each shot's state before each call, or a replayable one that runs its shots' calls again | a long run on a device whose calls fail now and then (a GPU out of memory), which would rather send a failed call again than lose the shots that ran before it |
| `event_probabilities` | each outcome carries its probability, as it comes                                                               | a simulator that computes probabilities                                                                                   | a driver that reads each outcome's probability as the outcomes stream, before the run ends; today each shot's log probability comes in the report, at `close`  |

## Making an executor: the rules

`executor_fn(promise, make_state, logic)` makes an executor (`ExecutorFn`)
from the implementer's logic alone:

- `promise` is what the executor offers (`Describe`), changed in two ways
  only. It offers `probabilities` exactly when `.log_probability` (below)
  gives it how to read a state's, whatever `promise` says
  (`probabilities_are_promised_only_with_a_log_probability`); and without
  a shot strategy, it lowers `states` to at most one (below). The rest is
  offered as given, under `OnePerShot` too
  (`describes_the_promise_as_given`), and wherever it is
  placed (`a_placed_executor_promises_what_the_executor_it_places_does`).
- `make_state` makes the state a shot's calls run on, the executor's
  working state (a simulator's qubits), from the context (`ShotContext`)
  of the shot it is made for, lazily, at the first call that needs it. An
  `executor_fn`'s state always belongs to one shot: the one-state form
  makes one, for a single shot, and `OnePerShot` one per shot (below).
  Its `Err` refuses the call, which then runs nothing and leaves no state
  made, and a later call tries again, for its own shots (any shot, while the
  one-state form holds none): for example `Rejected::Unavailable` when its
  device is busy (`a_state_that_cannot_be_made_refuses_the_call`,
  `a_state_refused_at_a_first_call_is_made_at_the_next_for_its_shot`).
- `logic` runs a call's instructions on the state (`ProgramInput`), and
  pushes each readout to `Outcomes`, each named by the shot it ran on
  (`each_outcome_is_named_by_the_shot_it_ran_on`). Its `Err` fails the
  call: the outcomes pushed before it are handed over, then the reply ends
  with its reason (`a_failed_logic_keeps_the_outcomes_before_it`, in
  `src/tests/executor_fn.rs`), and
  the
  executor refuses every later call, since the state is then part-way
  through a batch (the conformance check `refuses_calls_after_a_failure`).

The closures need no type annotations
(`closures_need_no_type_annotations`), and a plain function is passed by
name (`plain_functions_are_passed_by_name`). Three methods shape the
executor further:

- `.check(check)` states what the executor admits, beside what `promise`
  offers (the Promise axis: [Glossary](../glossary.md#a-parts-axes)): it
  refuses the calls `check` refuses, before the state is made and the
  logic runs: what the logic cannot run whatever the state, such as a
  qubit beyond the device's, or a repeated readout tag
  (`the_check_refuses_a_call_before_the_state_is_made_and_the_logic_runs`,
  in `src/tests/executor_fn.rs`). Planned: the facility checking each
  call against the instruction set `promise` states, so that `.check`
  holds only the executor's other rules
  ([Planned features](../planned.md)).
- `.log_probability(read)`, the
  [report closure](../glossary.md#making-and-placing-an-executor), reads a
  state's log probability, once, when the executor closes, into its report
  (`ExecutorReport::log_p`). It is the only way to promise `probabilities`
  (`probabilities_are_promised_only_with_a_log_probability`,
  `closing_reports_the_log_probability_of_the_state_it_made`).
- `.state_report(report)`, the other report closure, reads what a state
  tracked across its calls (a peak, a count), once, when the executor
  closes; each state's report is folded into the executor's report
  (`ExecutorReport::states`)
  (`a_state_reports_what_it_tracked_across_its_calls`,
  `closing_reports_each_state_once_as_its_calls_left_it`). Neither report
  closure runs on a state made for a failed call
  (`the_report_closures_never_read_a_state_made_for_a_failed_call`;
  [An executor fails](#an-executor-fails)).

A panic inside `call` (the check's, `make_state`'s or the logic's)
reaches the caller and fails the executor, as a `Mutex` is poisoned:
every later call is refused (`the executor panicked during the call`), in
every form, placed or given a state per shot
(`after_a_panic_inside_call_it_refuses_every_later_call`, in each form's
tests). `drop` never panics: a state's drop that panics is absorbed, after
the panic hook reported it
(`dropped_it_frees_its_state_raising_no_panic_of_its_drop`, in
`src/tests/executor_fn.rs`), and so is
the drop of what its closures capture, each closure in a catch of its
own, during an unwind too
(`dropped_it_frees_what_its_closures_capture_raising_no_panic_of_their_drops`,
`dropped_as_a_panic_unwinds_it_frees_what_its_closures_capture_raising_none`,
in `src/tests/executor_fn.rs`; per shot,
`dropped_it_frees_what_each_closure_captures_raising_no_panic_of_their_drops`,
in `src/tests/executor_fn/one_per_shot.rs`).
`close` raises the state's drop panic once the state is freed
(`close_frees_its_state_then_raises_its_drops_panic`), as it raises a
report closure's
(`a_report_closures_panic_reaches_the_closer_though_the_states_drop_panics_too`).
In every form, placed or not, a panic of the `Default` of the type that
`.state_report`'s closure returns (the implementer's code, which `close`
calls to start the states' report) in a `close` run outside an unwind
reaches the closer once every call accepted has ended and every state is
freed; a later `close` then reports none of those states (it calls that
`Default` again, which may panic again)
(`a_close_whose_reports_default_panics_ends_every_call_queued_first`,
`a_close_whose_reports_default_panics_frees_every_state_first`,
`a_close_after_its_reports_default_panicked_reports_nothing`, in
`src/tests/executor_fn.rs`).

Without a shot strategy, the executor holds one state, for one shot: it
lowers its promise to at most one state, so that a driver of more shots is
refused when composed
(`without_a_shot_strategy_the_promise_is_lowered_to_one_state`); it
refuses a call on another shot
(`a_call_on_a_shot_other_than_the_one_it_holds_is_refused`), and a first
call on two shots, which makes no state
(`a_first_call_on_two_shots_is_refused_and_makes_no_state`), after its
check, as under `OnePerShot`
(`the_check_refuses_a_call_before_its_shots_are_checked`).

`.seed(seed)` gives the executor the run's seed when it is built, in
every form: shot
`j`'s state is made from `ShotContext::new(seed, j)`, so that it draws
from its shot's seed
(`a_state_given_the_runs_seed_draws_from_its_shots_seed`), the same
values whatever the other shots of the run and whatever the shot form
(`a_shot_draws_the_same_values_in_every_shot_form`). A later `.seed`
replaces the one before, for the states made after it
(`a_later_seed_replaces_the_one_given_before`), and the methods that
shape the logic keep it
(`the_seed_is_kept_by_the_methods_that_shape_the_logic`). Without it, no
run's seed reaches the states, so a state that draws panics
(`without_the_runs_seed_a_state_has_none_to_draw_from`), and an adapter
that draws upstream of it is refused when built (`BuildError::NoRunSeed`;
[Seeds, not generators](../executors.md#seeds-not-generators)): that serves logic that
draws nothing, such as a device's or an existing tool's. The tests of
these rules are in `src/tests/executor_fn.rs`. The seed is the run's, not
the executor's, so a run with another seed builds the executor again;
the run is planned to take the seed with the job
([Planned features](../planned.md)).

## A state per shot: the rules

`.shots(OnePerShot)` makes a `OnePerShotExecutor`, whose rules are
pinned in `src/tests/executor_fn/one_per_shot.rs`:

- Shot `j`'s state is made at the shot's first call, from
  `ShotContext::new(seed, j)`, so that it draws the same values whatever the
  other shots of the run
  (`a_shots_state_is_made_at_its_first_call_from_the_runs_seed_and_the_shot`),
  and kept for the shot's later calls until the executor closes
  (`a_later_call_continues_each_shot_from_its_state`).
- A call runs the logic on each of its shots in the batch's order,
  inside `call` (`a_batch_runs_on_each_of_its_shots_in_its_order`).
- It describes its promise as given (`describes_the_promise_as_given`). A
  call that would hold
  more states than the promise's `states` is refused, with the first shot
  it has no room for
  (`a_call_beyond_the_promised_states_is_refused_with_the_first_shot_it_has_no_room_for`).
- A state that cannot be made refuses the whole call: no shot runs, and
  the states made for the call are dropped
  (`a_state_that_cannot_be_made_refuses_the_whole_call_with_its_refusal`,
  `the_states_made_for_a_refused_call_are_not_kept`).
- A logic that fails on a shot fails the call: the outcomes pushed before
  it are handed over, the later shots of the batch do not run, and the
  executor refuses every later call
  (`a_failed_logic_keeps_the_outcomes_before_it`,
  `after_a_shots_failure_the_later_shots_of_its_batch_do_not_run`).
- It continues the executor it is given: the state it holds, its calls and
  its refusals (`a_shot_strategy_continues_the_executor_it_is_given`,
  `given_a_state_per_shot_after_close_it_still_refuses`,
  `given_a_state_per_shot_after_a_failure_it_still_refuses`).

## A placed executor: the rules

By default an executor that `executor_fn` makes runs inside `call`, on the
caller's thread. `.run_on(placement)` places it (`PlacedExecutor`): its
calls are queued and run, in order, in the jobs it hands its placement
(`PlacementJob`); a call returns at once when the placement schedules the
job to run elsewhere
(`a_placement_is_handed_one_job_which_runs_every_call_queued_in_order`),
and after its work when the placement runs the job inline
(`|job: PlacementJob| job()`:
`on_a_placement_that_runs_its_job_inline_a_call_returns_with_its_work_done`). The placement is `OwnThread`, a
thread of the executor's own, for an executor that must run on one thread
(a device's driver)
(`on_its_own_thread_an_executor_runs_every_call_on_one_thread_not_the_callers`),
provided it and its state are `Send`, since the executor is moved to that
thread (the compiler refuses one that is not, which no test pins), or a
closure that runs each job where it chooses, such as a thread pool
(`a_placement_is_handed_one_job_which_runs_every_call_queued_in_order`).
A placement that serves the far end of a link, `Remote(link)`, is planned,
with no milestone yet: the compiler's error for a value that is not a
placement names it (`Placement`'s diagnostic, which no test pins).
Two more placements are planned, with no milestone yet, as a prototype of
this crate did them:

- Placing any other executor that is `Send`, one written by hand, an
  adapter's executor or a chain: only the executors `executor_fn` makes
  have `.run_on`. It would serve a device's driver written by hand that
  must run on one thread. Until then, a chain runs on a thread of its own
  (`a_chain_of_observers_runs_on_another_thread`).
- Placing a state bound to the thread that made it, which is not `Send`,
  such as an engine's GPU context or a state built on `Rc`: under
  `.run_on(OwnThread)`, the state closure would run on that thread, so
  that the state never leaves it. Today the compiler refuses it, which no
  test pins.

The rules, pinned in `src/tests/placement.rs`:

- Its reply is an `IncrementalReply`, whose events come as the executor
  hands them over on its thread. A placed executor and its replies move to
  other threads (`a_placed_executor_and_its_replies_move_to_other_threads`),
  and an asynchronous task awaits its reply
  (`an_async_task_awaits_the_reply_of_an_executor_on_its_own_thread`).
- A refusal on the executor's thread fails the call's reply
  (`a_call_its_executor_refuses_on_its_thread_fails_its_reply`), and
  every later call is refused
  (`after_a_refusal_on_its_thread_it_refuses_every_later_call`). After a
  failure, the calls queued behind it fail without running
  (`a_call_queued_behind_a_failed_one_fails_without_running`), and every
  later call is refused (`after_a_failed_call_it_refuses_every_later_call`),
  also by a caller that the failed reply woke
  (`a_caller_woken_by_a_failed_reply_finds_the_executor_failed`). Design
  question, open: a refusal of class `Later` on the executor's thread
  fails the executor too
  ([Rejections and retries](../rejections.md#what-a-rejection-says)).
- A panic stays in its job: it fails its call and the executor
  (`a_panic_on_the_executors_thread_is_a_failure`), and does not
  reach the placement's thread, which may serve other executors
  (`a_panic_on_the_executors_thread_does_not_reach_its_placement`). It
  stops the placement: the calls queued behind it fail as never run
  (`the call never ran: its placement stopped`), and `close` does not wait
  for them (`a_panic_stops_the_calls_queued_behind_it`).
- A placement runs or drops each job it is handed. A job dropped unrun
  stops the placement too: the calls it held fail as never run
  (`a_job_its_placement_drops_fails_the_calls_it_held`), and every later
  call is refused, naming why they failed
  (`after_its_placement_drops_a_job_it_refuses_every_later_call`). A job
  kept unrun holds the calls, their replies and `close`; and since `close`
  blocks its thread until a job ran the close, a placement must not defer
  a job to run later on the thread that closes (a one-thread runtime's
  spawn, under the task that closes), or `close` waits for ever; a job run
  inline has run before `close` waits (`Placement`'s doc, which no test
  pins).
- `close` waits for the calls queued, then closes the executor it places
  and returns its report
  (`closing_waits_for_the_calls_queued_then_gives_the_placed_executors_report`),
  also before any call
  (`closing_before_any_call_gives_the_placed_executors_report`). Once its
  first call started the placement, the executor it places is closed in a
  job on its placement, as its calls ran: under `OwnThread`, on their one
  thread, so that an executor bound to a thread is closed there
  (`once_called_closing_closes_the_executor_it_places_on_its_thread`).
  Before any call, it is closed on the caller's thread
  (`before_any_call_closing_closes_the_executor_it_places_on_the_callers_thread`).
  The report is that of the executor it places, so it counts only the
  calls that reached it, not those that failed before
  (`its_report_counts_only_the_calls_that_reached_the_executor_it_places`);
  it is empty once the placement stopped, after a panic or a job dropped
  unrun (`a_panic_stops_the_calls_queued_behind_it`,
  `a_job_its_placement_drops_fails_the_calls_it_held`).
- Dropped without `close`, it cancels the calls queued and not yet
  started: none of them runs, and each one's reply ends
  `ReplyStatus::Cancelled`
  (`dropped_it_cancels_the_calls_not_yet_started`), as dropping tokio's
  `JoinSet` cancels the work not yet started; a call already running
  finishes, and its reply ends as it would have
  (`dropped_it_lets_the_call_running_finish`). Then a last job drops the
  executor it places on its placement, where its calls ran
  (`dropped_it_drops_the_executor_it_places_on_its_placement`; dropped
  before its first call, it drops that executor where it is), so that
  its states are freed after the call running, not after every call
  queued (`dropped_it_frees_its_states_after_the_call_running_without_close`).
  `drop` does not wait for that last job: call `close`, which waits, to
  free what the states hold (a device, GPU memory) before building another
  executor on it. Known issue, open: once its placement stopped, no job
  runs any more, so the executor it places is dropped with the last of the
  placed executor and its jobs, usually on the thread that drops it, not
  where its calls ran
  (`once_stopped_it_drops_the_executor_it_places_where_it_is_dropped`);
  `close` frees it before it returns
  (`once_stopped_its_close_frees_the_states_of_the_executor_it_places`).
- `drop` never panics, during an unwind too, where a panic would abort
  the process: a caller's waker or an instruction's drop that panics as a
  call is cancelled, a state's drop that panics as the executor it places
  is freed, and the drop of what the placement captures, are absorbed,
  after the panic hook reported them
  (`dropped_it_raises_no_panic_of_a_callers_waker`,
  `dropped_it_raises_no_panic_of_an_instructions_drop`,
  `once_stopped_its_drop_frees_the_states_raising_no_panic_of_their_drops`,
  `before_its_first_call_its_drop_frees_what_its_placement_captures_raising_no_panic_of_its_drop`,
  `once_started_its_drop_frees_what_its_placement_captures_raising_no_panic_of_its_drop`;
  per shot, with what the closures capture
  (`src/tests/executor_fn/one_per_shot/placed.rs`:
  `dropped_it_frees_what_its_closures_and_placement_capture_raising_no_panic_of_their_drops`)).
  `close` raises such a state's drop panic in its caller instead, once
  the states are freed, but not a capture's
  (`once_stopped_its_close_raises_the_first_panic_of_a_states_drop`,
  `once_stopped_its_close_frees_what_the_closures_capture_raising_no_panic_of_their_drops`;
  placed whole (`src/tests/executor_fn/one_per_shot/placed.rs`:
  `placed_whole_close_frees_every_state_then_raises_the_first_drops_panic`)),
  as it does a report closure's panic made on its placement
  (`a_panic_making_the_report_at_close_reaches_the_closer`).

## The order of `.shots` and `.run_on`

The order of `.shots` and `.run_on` says where the shots run.
`.shots(OnePerShot).run_on(OwnThread)` runs every shot on the one
thread (`placed_after_one_per_shot_every_shot_runs_on_the_one_thread`,
`src/tests/placement.rs`), and `.run_on(OwnThread).shots(OnePerShot)`
each shot on a thread of its own, so the shots of a batch run in parallel
(`placed_before_one_per_shot_each_shot_runs_on_a_thread_of_its_own`, in
`src/tests/executor_fn/one_per_shot/placed.rs`). That pays only for shots
that share nothing and whose calls cost much more than a thread hop, about
9 µs per hop (each shot's call is one) on one core and 15–20 µs on three
(measured on an earlier prototype of the protocol; not measured on this
crate yet), and never for a grouped executor (planned), whose shots share
work that each part would repeat. Split k ways, a run repeats the work its
shots share and divides the rest,
$T_k \approx T_{\mathrm{shared}} + T_{\mathrm{split}}/k$:
on that prototype, 256 shots that shared nothing ran 4.6 times faster on
12 cores, and an ensemble split per core at best 1.8 times (on 6), since
each part repeated the gates its shots shared; a sharded executor
(planned) meets the same bound. Work inside an executor, such as a
GPU's kernels running each gate on many cores, is the executor's own, and
the protocol does not see it. With a thread per shot
(`.run_on(OwnThread).shots(OnePerShot)`), each shot's state is made on the
caller's thread, from the run's seed and the shot, then moved to its
placement
(`each_shots_state_is_made_on_the_callers_thread_from_the_runs_seed_and_the_shot`).
For a run that does not fail, of a logic that keeps its state in the shot's
state, whose `make_state` makes the same state whatever thread it runs on, and
whose `state_report` and `log_probability` read only the shot's state and what
they captured when built, both orders give the same outcomes and report, the
ones a run gives on the caller's thread
(`a_run_gives_the_same_outcomes_and_report_wherever_the_shots_states_run`,
`tests/per_shot_executor.rs`). The two orders, as
`both_orders_of_the_wrappers_give_the_same_outcomes_and_report`
(`src/tests/executor_fn/one_per_shot/placed.rs`) builds them, are shown in
[Where an executor runs](../executors.md#where-an-executor-runs-run_on).

The orders may differ otherwise:

- A logic that fails on a shot fails the call. In both orders the
  failure is recorded as it happens on the shot's thread, whether or not
  its reply was polled, and every later call is refused, whatever shots it
  names; with a thread per shot, before its check, before any state is
  made and before any shot runs
  (`after_a_shots_failure_a_call_is_refused_before_its_check_and_states`,
  `after_a_shots_failure_a_call_is_refused_before_any_shot_runs`,
  `after_a_shots_failure_a_call_on_other_shots_is_refused`,
  `a_failure_recorded_by_its_reply_refuses_a_later_call_on_other_shots`, all in
  `src/tests/executor_fn/one_per_shot/placed.rs`). The batch's other shots
  may have run. A call accepted before the failure, and queued on the
  failed shot behind the failing call, does not run there: its reply
  fails, and its other shots may run
  (`a_call_queued_behind_a_shots_failure_fails_while_its_other_shots_run`).
  A call being started as the failure happens can miss it: it is refused
  still when the failed shot comes first in its batch and the failure is
  recorded before that shot is given the call
  (`a_failure_before_its_shot_gets_the_call_refuses_it_if_first`);
  otherwise it is accepted, the shots before the failed one may run, and
  its reply fails
  (`a_failure_before_its_shot_gets_the_call_fails_its_reply_if_later`).
- Dropped, each shot cancels its calls not yet started, as a placed
  executor does, so a call's reply ends `ReplyStatus::Cancelled` once a
  shot of it was cancelled (`dropped_it_cancels_the_calls_not_yet_started`,
  in `src/tests/executor_fn/one_per_shot/placed.rs`),
  unless a shot of it failed: a failure outranks a cancellation, whatever
  the shots' order (`a_failed_shot_outranks_a_cancelled_one`); and a
  cancelled call fails no later call
  (`a_cancelled_call_fails_no_later_call`, all three in
  `src/tests/executor_fn/one_per_shot/placed.rs`).
- A logic that keeps state of its own (a counter it captures) keeps one
  copy per shot with a thread per shot, one for every shot on one thread
  (`placed_before_one_per_shot_each_shot_runs_a_copy_of_the_logic_of_its_own`,
  `placed_after_one_per_shot_every_shot_runs_the_one_logic`).
- With a thread per shot, the report counts every call it accepted
  (`its_report_counts_every_call_it_accepted_those_failed_behind_a_failure_included`,
  `src/tests/executor_fn/one_per_shot/placed.rs`);
  on one thread, only the calls that reached the executor it places
  (`its_report_counts_only_the_calls_that_reached_the_executor_it_places`,
  `src/tests/placement.rs`).

In parallel, the outcomes of a batch's shots interleave
(`an_executor_hands_over_each_shots_events_as_they_come`), each shot's in
the order it gives them, not sorted by tag
(`a_shots_late_outcomes_keep_the_order_of_its_reads_not_of_their_tags`,
both in `src/tests/shot_states.rs`). Known issue, open: each shot's reply
wakes the caller, at most once per sleep, as the shot's events come and
when it
ends; the caller polls the reply once per wake, and each poll polls every
shot still pending, so a batch of n shots, woken once per shot that ends,
costs O(n²) polls of shots
([Performance](../performance.md#shots-on-placements-of-their-own)).

## An executor's close

The run closes the executor once, whether its program called it
(`a_run_closes_its_executor_once_after_the_drivers_last_call`), never
called it (`a_run_whose_program_never_calls_still_closes_its_executor`)
or saw a call fail (`a_run_closes_its_executor_once_after_a_call_failed`),
and through adapters it closes the last executor of its chain once
(`a_run_through_an_adapter_closes_the_last_executor_once_after_the_drivers_last_call`;
all in `src/tests/compose/run.rs`). A run dropped before its program ends,
or unwound by a panic, does not close its executor (`Run`'s doc, which no
test pins; an open question that goes with the planned cancellation). A
call after close is refused (`Rejected::Closed`, the conformance check
`rejects_calls_once_closed`).

## An executor fails

An executor whose state carries over from one call to the next can keep the
earliest failure of its calls on a lifecycle (`ExecutorLifecycle`; the
`Executor` contract lists the other ways). A failure it learns of inside
`call`, it records with `fail`. A reply that may fail after `call` returned,
it binds to the lifecycle (`ExecutorLifecycle::bind`), and the bound reply
(`LifecycleBoundReply`) records its own failure when it is polled. A driver
with several calls in flight may have had later calls admitted before that
poll: their replies end at their next poll, without more events, and the
next `call` is refused (tests `a_bound_reply_that_fails_fails_the_executor`
and `a_bound_reply_ends_at_an_earlier_calls_failure`, in
`src/tests/lifecycle.rs`). A poll that runs as the failure is recorded may
still leave its reply pending and hand over that poll's own events; the
next poll ends it (a known issue, noted at `LifecycleBoundReply`'s source;
`a_reply_left_pending_as_an_earlier_failure_is_recorded_ends_at_its_next_poll`).
The executors that `executor_fn` makes keep the same rules for the shots'
states they hold. Until the failure is recorded as the executor's, a
later call's reply ends as its own work does: placed per shot, a call on
other shots ends done while the failing call's reply is not polled,
though the executor already refuses a new call (refusing one ends no
call already in flight), and one queued behind the failing shot
hands over its other shots' events before it fails
(`a_call_on_other_shots_ends_done_while_the_failing_reply_is_unpolled`,
`a_call_queued_behind_a_shots_failure_fails_while_its_other_shots_run`,
in `src/tests/executor_fn/one_per_shot/placed.rs`).

A wrapper never forgets a refusal: an executor that `executor_fn` makes,
closed or failed, then placed (`run_on`) or given a state per shot
(`shots`), in either order, refuses the next call inside `call`, as a
wrapper ([Glossary](../glossary.md#a-parts-axes)) continues what it wraps,
failures included, as iterator adapters and Tower's `Buffer` do (tests
`placed_after_a_failure_it_refuses_at_call`,
`given_a_state_per_shot_then_placed_after_a_failure_it_refuses_at_call`,
`given_a_state_per_shot_a_placed_executor_keeps_its_own_refusal`). A
failed executor keeps the states made before the failed call until it
closes, and `close` reports them (tests
`a_failed_executor_still_reports_its_state`, in `src/tests/executor_fn.rs`,
and `a_failed_executor_still_reports_the_states_made_before_the_failed_call`,
in `src/tests/executor_fn/one_per_shot.rs`). And a failed call reports no
shot of it: `close` reports no state made for the
failed call, whether its shot ran or not, nor runs a report closure on one,
so every order of the wrappers reports the same states, as `futures`'
`try_join_all` drops the others' results (tests
`a_failed_call_reports_no_state_made_for_it_in_every_order` and
`the_report_closures_never_read_a_state_made_for_a_failed_call`), unless a
placed state's placement stopped (its logic panicked, or its job was
dropped unrun), which leaves that state out (test
`a_state_whose_logic_panicked_is_left_out_of_the_report`, in
`src/tests/executor_fn/one_per_shot/placed.rs`).

```mermaid
sequenceDiagram
  participant D as driver
  participant E as executor
  participant L as ExecutorLifecycle (earliest failure)
  participant R1 as reply 1: LifecycleBoundReply
  participant R2 as reply 2: LifecycleBoundReply
  D->>E: call(message 1)
  E->>L: admit() (call 1)
  E->>L: bind(reply 1)
  E-->>D: Ok(R1)
  D->>E: call(message 2)
  E->>L: admit() (call 2)
  E->>L: bind(reply 2)
  E-->>D: Ok(R2)
  D->>R1: poll_events
  Note over R1: the reply it binds: Failed(reason)
  R1->>L: fail(reason) (recorded for call 1)
  R1-->>D: Failed(reason)
  D->>R2: poll_events
  R2->>L: an earlier call failed?
  L-->>R2: reason (call 1 < call 2)
  Note over R2: names the reason, as EarlierFailure(reason) displays it
  R2-->>D: Failed("an earlier call failed: reason")
  D->>E: call(message 3)
  E->>L: admit()
  L-->>E: Err(EarlierFailure(reason))
  E-->>D: Err(EarlierFailure(reason))
  Note over E: A failure learnt inside call: lifecycle.fail, and the call returns its reply, failed.
```

## An executor written by hand: the rules

An executor that holds one state shared by its shots (an ensemble, such
as a simulator in another crate that samples every shot at once)
implements `Executor` itself, with `Describe` and `Close`, and keeps the
rules of failure and close with `ExecutorLifecycle`: `admit` at each call
refuses once closed (`a_closed_executor_refuses_every_call`) or failed
(`a_failed_executor_refuses_every_later_call`), `fail` records a failure,
`bind` binds a reply that may fail later, and `close` refuses every later
call. The crate's ensemble double, `SharedState` (`src/tests.rs`), is
built on it and passes the conformance checks
(`a_shared_state_executor_refuses_calls_after_its_device_stops`,
`a_shared_state_executor_refuses_calls_once_closed`,
`a_shared_state_executor_reports_once`, in `src/tests/lifecycle.rs`). The
executors that `executor_fn` makes keep their rules on it too.

## An ensemble executor

An ensemble executor runs the shots of a batch together, from one state
shared by its shots, and does the work inside `call`. One poll ends the
batch, and the waker is not kept. An ensemble executor that ran elsewhere
(a GPU) would return an `IncrementalReply` and push from there, with no
change to its callers.

```mermaid
sequenceDiagram
  participant D as driver
  participant E as ensemble executor
  participant R as ImmediateReply
  D->>E: call(InstructionBatchRef { shots, instructions })
  Note over E: runs the batch on every shot named
  E-->>D: Ok(ImmediateReply(shot outcomes))
  D->>R: poll_events(out, waker)
  R-->>D: every shot's outcomes, Done (the waker is not kept)
```

## Seeds: the derivation

Shot `j` draws, in component `c` (a 32-bit `ComponentId`; the last
executor's is 0) built with seed `s`, from the seed

$$
\mathrm{shot\_seed}(s,j)=\mathrm{SM}\big(\mathrm{SM}(s)\oplus j\big),\qquad
\mathrm{rng\_seed}(s,j,c)=\mathrm{SM}\big(\mathrm{SM}(s)\oplus(2^{32}c+j)\big)
$$

so $\mathrm{rng\_seed}(s,j,0)=\mathrm{shot\_seed}(s,j)$, where `SM` is one step
of `SplitMix64` (`shot_seed`, `ShotContext::rng_seed`). Each step, and the XOR
with a constant, is a bijection of `u64`, and $2^{32}c+j$ is one-to-one over
32-bit shots and ids, so, for one `s`:

- no two shots of a run share a seed, nor two (shot, component) pairs
  (`a_runs_shots_never_share_a_seed`,
  `each_component_of_a_shot_has_its_own_seed`);
- a seed depends only on `(s, j, c)`, not on the other shots of the run
  or on which other components draw.

Each component that draws is given the run's `s`. An executor is built
with it (`.seed(s)`, `ExecutorFn::seed`) and gives shot `j`'s state
`ShotContext::new(s, j)`
(`a_shots_state_is_made_at_its_first_call_from_the_runs_seed_and_the_shot`).
An adapter that draws is given the same `s` when it is built against what
is downstream of it, bound to the id it draws with
(`AdapterBoundary::drawing_seeds`), and draws for shot `j` from
`AdapterSeeds::shot(j)`
(`an_adapter_reads_when_built_the_runs_seed_the_last_executor_is_given`,
`src/tests/adapter/boundary.rs`). So one seed fixes the whole chain's
draws (`a_chain_with_a_drawing_adapter_replays_from_its_one_seed`). Where
no run's seed reaches an adapter that draws (an executor written by hand
cannot state one yet, planned), it is refused when composed
(`BuildError::NoRunSeed`,
`an_adapter_drawing_with_no_run_seed_below_is_refused`).

Seed separation prevents a component from consuming another component's
generator draws. That a shot's outcomes do not depend on the other
shots of the run additionally requires an executor that does not share
state between shots (replayable), such as one under `OnePerShot`. An
adapter that changes instructions can still change outcomes.

`a_shot_run_alone_gives_what_it_gives_among_others`
(`src/tests/compose/run.rs`) checks it on a run, and the conformance check
`gives_each_shot_alone_what_it_gives_among_others` on each executor that
promises it. That other components do not shift a shot's draws follows
from the formula, and `each_component_of_a_shot_has_its_own_seed`
(`src/tests/seeds.rs`) checks that their seeds differ. One test runs an
adapter that draws upstream of an executor that draws, and checks that
the executor's outcomes are unchanged
(`a_drawing_adapter_leaves_the_last_executors_draws_as_they_are`,
`src/tests/compose/adapters.rs`), and one runs two adapters that draw
from the one seed, each under its own id
(`two_drawing_adapters_seed_from_the_one_seed_each_with_its_own_id`,
`src/tests/adapter/adapter_fn/one_per_shot.rs`); but each keeps a
generator of its own, so these tests could not see a shift, and no test
checks that one component's draws do not shift another's.
`src/tests/seeds.rs` also pins known answers.

`ShotContext` gives seeds, not generators. A method that returned a
`StdRng` would make this crate depend on `rand`, and the crate depends on
`std` and `futures-core` only, so that every part of qdk can use it.
`rand` with its default features brings `getrandom`, which needs a
backend chosen for each target, so a wasm build of every crate that uses
this one would have to choose one. And `rand` does not promise to keep
`StdRng`'s algorithm across versions, so a shot's draws would change with
`rand`'s version, where its seeds do not. Nothing is lost: a component
that wants a generator writes `StdRng::seed_from_u64(ctx.rng_seed(c))`
itself. These seeds are not those that qdk's `run` or `run_qir` give each
shot for the same user seed: each of those has a formula of its own.
