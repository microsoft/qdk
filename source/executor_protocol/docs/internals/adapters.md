# Adapters: the detailed rules

[Protocol overview](../../README.md).

This page is for maintainers of the crate, and for those who need a rule at its
edge: the order of readouts, failures and the reports of failed calls, drops and
panics, the merging of a state per shot's sends, the conformance kit, the owning
way below, and what a state depends on; and, last, the tests of the promises of
[Adapters](../adapters.md), of the observers guide and of the adapters' API docs
that the sections do not cite. A user writing an adapter needs none of it to
start, and starts at [Adapters](../adapters.md).

## The order of a run's readouts

`below.run(..)` gives each shot's readouts in the order the reply below
gives them: the order the shot read them when the executor below keeps it,
as a simulator does
(`a_run_gives_its_readouts_by_shot_each_in_the_order_read`), and not
when it is an `adapter_fn` whose logic sends, then runs
(`an_observer_that_runs_passes_up_each_shots_readouts_in_the_replys_order`),
or sends twice, where a later send's reply gives readouts before an
earlier send's reply gives its own, as a placed executor's can, whether
it ended or not: those pass up before the earlier send's
(`a_later_sends_readouts_pass_up_before_those_an_earlier_send_gives_later`;
the tags still match).

## Failures in detail

The tests of this section are in `src/tests/adapter/adapter_fn.rs`
unless named elsewhere; a test of the same name in
`src/tests/adapter/adapter_fn/one_per_shot.rs`, where there is one, pins
the rule for a state per shot.

Beside the logic's error and a failure below
([Refusals and failures](../adapters.md#refusals-and-failures)), a later
call below refused, or a run's reply that is not whole, fails the call and
the adapter
(`a_send_refused_after_another_fails_the_call_and_the_adapter`,
`a_run_that_reads_a_tag_of_a_shot_twice_fails_the_call_and_the_adapter`);
a send cancelled below fails the adapter too
(`a_send_cancelled_below_fails_the_adapter`); so do a refusal below for
an earlier failure
(`a_refusal_below_for_an_earlier_failure_fails_the_adapter`) and a panic
in the logic, which reaches the caller
(`a_logic_that_panics_fails_the_adapter`). A later call still in flight
then ends with the earlier failure at its next poll
(`a_call_in_flight_ends_at_an_earlier_failure_below`), while an earlier
call's reply is not ended by a later call's failure
(`a_bound_reply_is_unaffected_by_a_later_calls_failure`, in
`src/tests/lifecycle.rs`; for an adapter, the test polled last first
below).

Closing closes what is below after a panic in the logic too
(`an_adapter_whose_logic_panicked_closes_what_is_below`); a panic of its
report closure or of a state's drop at the close reaches the closer once
what is below is closed
(`a_close_whose_state_drop_panics_closes_below_then_raises_it`). Dropped
unclosed, it absorbs the panics of its states' drops and of the drops of
what its closures capture, as an executor does
(`dropped_it_frees_its_state_raising_no_panic_of_its_drop`,
`dropped_it_frees_what_its_closures_capture_raising_no_panic_of_their_drops`).

### Reports of failed calls

A state made for a call that failed before it returned, or panicked, is
not reported (`a_failed_call_reports_no_state_made_for_it`,
`a_call_that_panicked_reports_no_state_made_for_it`, in
`src/tests/adapter/adapter_fn.rs` and, for a state per shot,
`src/tests/adapter/adapter_fn/one_per_shot.rs`); one made for a call
whose send's reply fails only after the call returned is
(`a_state_whose_calls_reply_below_failed_later_is_reported`).

## Drop and poll orders over a placed executor

An adapter over a placed executor that is dropped with two calls' replies
pending may end them unlike the placed executor alone, which ends both
`Cancelled`
(`dropped_its_calls_not_yet_started_end_cancelled_in_either_poll_order`,
in `src/tests/executor_fn/one_per_shot/placed.rs`;
[A placed executor](executors.md#a-placed-executor-the-rules)). Polled first,
the first call's reply ends `Cancelled`, and its send cancelled below
fails the adapter, so the second call's fails with that earlier failure
(`dropped_over_a_placed_executor_its_second_pending_reply_fails_for_the_firsts_cancel`);
polled the other way, both end `Cancelled`
(`dropped_over_a_placed_executor_its_pending_replies_polled_last_first_both_end_cancelled`).

## Merging a call's sends rank by rank

With a state per shot, a call's sends go below merged, rank by rank: a
shot's `k`-th send with the other shots' `k`-th sends equal to it, as one
call over the shots that sent them
(`equal_sends_go_below_as_one_call_over_their_shots`), unequal ones apart
(`unequal_sends_go_below_apart_each_over_the_shots_that_sent_it`), and
equal sends of different ranks apart, so that each shot's sends reach
below in the order it made them
(`a_shots_sends_reach_below_in_the_order_it_made_them`).

A shot's run goes below at once, on the shot alone
(`a_shots_run_goes_below_on_that_shot_alone`), after the shot's sends
kept until then, each alone (`a_shots_sends_go_below_before_its_run`),
while the other shots' kept sends stay merged
(`a_shots_run_leaves_the_other_shots_sends_merged`). Its later sends
merge across shots again, with the others' as if they were its first
(`a_shots_sends_after_its_run_rank_anew`).

A kept send's panic below comes at the run, not at the send, so a catch
around the send alone does not catch it
(`a_kept_sends_panic_below_comes_at_its_shots_run_not_at_the_send`).
Should one of the sends going alone panic below, and the logic catch
the panic, the shot's sends after it are dropped unsent, and none goes
below again; for a logic whose catch holds the calls from the send that
panicked to the run, the calls below are those a one-state adapter
makes with the same logic
(`a_shots_kept_sends_go_below_once_though_one_panics_going_alone`,
`a_shots_send_that_panicked_going_alone_is_not_sent_as_the_call_ends`,
`a_shots_calls_below_around_a_caught_panic_are_a_one_state_adapters`,
in `src/tests/adapter/adapter_fn/owning.rs`). The sends it keeps are
copies, freed by the time the call ends, however it ends (a shot's sends
that go alone, at its run), each in a catch of its own: a copy's drop
that panics is absorbed, as a placement absorbs its copy's, and the
adapter goes on serving
(`a_kept_sends_drop_that_panics_after_its_call_ran_leaves_the_adapter_serving`,
`a_failed_calls_kept_sends_are_freed_raising_no_panic_of_their_drops`,
`a_shots_sends_that_go_alone_are_freed_at_its_run_leaving_the_adapter_serving`,
in `src/tests/adapter/adapter_fn/one_per_shot.rs`).

## The conformance kit

In both shot forms (one state, and `.shots(OnePerShot)`), an adapter
over an executor passes the executor's checks of the conformance kit, so a
profiler or a decoder over a simulator fails as the simulator alone does:
closed, it refuses every call (`a_closed_profiler_refuses_every_call`); it
reports once (`a_profiler_reports_once`); a call refused, by its check or
below, leaves the chain usable
(`a_call_the_adapter_refuses_leaves_the_chain_usable`,
`a_call_refused_below_leaves_the_chain_usable`); after a failure below,
sent, run or on another thread, the chain refuses every later call,
naming it
(`after_a_failure_below_the_chain_refuses_every_later_call`,
`after_a_run_failed_below_the_chain_refuses_every_later_call`,
`after_a_failure_below_on_its_own_thread_the_chain_refuses_every_later_call`),
as it does when composed over an executor that already failed
(`an_adapter_over_a_failed_executor_refuses_with_its_failure`); and the
checks of a repeated tag, of a shot continued, of a shot alone
and of the set below hold
(`a_decoder_refuses_a_tag_repeated_within_a_batch`,
`a_profiler_continues_a_shot_from_one_call_to_the_next`,
`a_profiler_gives_each_shot_alone_what_it_gives_among_others`,
`a_profiler_refuses_calls_outside_the_set_below`; each in
`src/tests/adapter/adapter_fn.rs`, and each but the shot-alone one, for
a state per shot, in `src/tests/adapter/adapter_fn/one_per_shot.rs` too),
and its pairs of reports fold as reports do (`the_default_pair_is_empty`,
`pairs_fold_in_any_order`, in `src/tests/executor.rs`).

## The way below of an owning state

What an owning state sends or runs through its way below goes below as
through a lent one
(`owned_sends_go_below_as_lent_ones_do`,
`owned_sends_of_each_shot_merge_as_lent_ones_do`,
`an_owned_run_goes_below_on_its_shot_alone`), by the same refusal and
failure rules (`an_owned_run_refused_first_refuses_the_call`,
`an_owned_run_refused_after_a_send_fails_the_call_and_the_adapter`,
`an_owned_run_that_fails_below_fails_the_call_and_the_adapter`), and a
send gives its logic the refusal, as a lent one does
(`an_owned_send_refused_below_gives_its_logic_the_refusal`,
`a_shots_owned_send_after_its_refused_run_gives_its_logic_the_refusal`).
Closing drops the states, with their ways below, before closing what
is below
(`closing_drops_the_state_and_its_way_below_before_closing_below`), and
closes it after a panic in a logic or below
(`closing_after_a_logic_panicked_closes_below`,
`closing_after_a_panic_below_closes_below`).

The way below reaches below only while its state's logic serves a call
on the thread, from what that logic runs, but not inside another owning
state's logic:

- It reaches below from what its state's logic runs, outside any other
  owning state's logic. A chain that logic consults counts: when a
  per-shot owning adapter of that chain merges its logics' sends into one
  send, made after those logics return, the executor below it sends
  through the way below outside them, and reaches below
  (`a_way_below_reaches_below_from_a_merged_send_its_logic_runs`).
- Elsewhere it reaches nothing, and the `Err` says why:
  - its state's logic serves no call
    (`a_way_below_reaches_nothing_from_its_state_closure`,
    `a_way_below_kept_between_calls_reaches_nothing`,
    `a_way_below_kept_between_calls_reaches_nothing_from_another_thread`,
    `a_way_below_of_a_state_dropped_with_a_refused_call_reaches_nothing`,
    `a_way_below_of_a_state_dropped_with_a_failed_call_reaches_nothing`,
    `a_way_below_kept_past_close_reaches_nothing`,
    `a_shots_way_below_reaches_nothing_in_another_shots_logic`,
    `a_way_below_reaches_nothing_in_the_logic_of_an_adapter_below`,
    `a_way_below_reaches_nothing_from_inside_a_call_below`), its state's
    own `Drop` included (`a_states_drop_reaches_nothing_below`);
  - it is used on a thread other than the one its state's logic serves on
    (`a_way_below_reaches_nothing_from_another_thread_in_its_logic`);
  - it is used inside the owning logic of another chain that its logic
    runs
    (`a_way_below_reaches_nothing_in_another_chains_logic_its_logic_runs`).

It is refused with an `Err` even in a thread-local's destructor
(`a_way_below_reaches_nothing_from_a_thread_locals_destructor`). The
adapter's executor called there may panic, but it keeps the executor
below, and its close closes it
(`closing_after_a_call_from_a_thread_locals_destructor_closes_below`).
The refusal fails nothing by itself; a logic that returns it fails the
call and the adapter, as with any `Err`, with one state or a state per
shot
(`a_one_state_logic_that_returns_a_refusal_fails_the_call_and_the_adapter`,
`a_per_shot_logic_that_returns_a_refusal_fails_the_call_and_the_adapter`).
After a panic below that its logic caught, it reaches below again, as a
lent one does, with one state or a state per shot
(`a_way_below_reaches_below_after_its_logic_caught_a_panic_there`,
`a_shots_way_below_reaches_below_after_its_logic_caught_a_panic_there`).

The way below is `Send` and `Sync` whatever is below
(`a_way_below_is_send_and_sync_whatever_is_below`), and not `Clone`
(`a_way_below_cannot_be_cloned`). The adapter's executor is `Send` when
what it owns is: two owning adapters whose states share a plan by `Rc`
stack and run
(`owning_adapters_whose_states_are_not_send_stack_either_way`), and one
over a `Send` executor moves to another thread
(`an_owning_adapter_over_a_send_executor_serves_on_another_thread`).

The repetition code as such a runtime, shaped as a Qodec runtime, reads
what the code as a logic reads, with a batch it flushes twice, a run
then a send (`tests/static_isa.rs`:
`a_runtime_that_owns_its_backend_reads_what_the_code_as_a_logic_reads`);
its glue (`tests/static_isa/code_runtime.rs`) fails a call whose runtime
gives fewer results than the call has readouts
(`glue_given_fewer_results_than_readouts_fails_the_call`).

## What a state depends on

A state, an executor's or an adapter's, is made from its shot's context
alone (`ShotContext`: the shot, and the run's seed where one reaches the
component; and, for `owning_adapter_fn`, its way below), plus what the
`make_state` closure captured when the component was built (a code's plan,
a noise rate, a log handle). What varies per call (the instructions, the
way below of an `adapter_fn`, the outcomes) goes to the logic, not to the
state. So a shot starts from the same state alone or among others, in
any shot set (`a_shot_gives_alone_what_it_gives_among_the_others`, and for
an executor, `gives_each_shot_alone_what_it_gives_among_others` of the
conformance kit), and the facility makes states lazily, one or one per
shot, without the logic knowing which.

That covers every state whose start is a function of the component's
configuration, the shot and the seed: simulators, noise models, profilers,
debuggers, a code's runtime made per shot. The one-state forms follow the
same rule from their first shot: a one-state `executor_fn` serves one shot
([Executors](../executors.md#making-an-executor-executor_fn)), and a one-state
`adapter_fn` that draws makes its state from its first call's first shot,
so it offers no replayable shots
(`a_drawing_adapters_state_seeds_from_the_first_shot_it_serves`,
`a_drawing_adapter_is_not_replayable`).

Not covered, open design questions with no milestone yet:

- a state that depends on the job, its program or its number of shots, as
  an ensemble's state shared by a job's shots would
  ([An ensemble executor](executors.md#an-ensemble-executor), written by
  hand meanwhile);
- a state that depends on what is below the component, such as what the
  executor below offers: `make_state` is given the shot's context only
  (and, for an owning state, its way below), so such a state is made from
  what its closure captured when the component was built.

## The tests of the user pages' promises

Each promise of [Adapters](../adapters.md), of the observers guide and of the
API docs of the adapter types that the sections above do not pin, in each page's
order, with the tests that pin it. Those of [Adapters](../adapters.md) are in
`src/tests/adapter/adapter_fn.rs` unless named elsewhere; a test of the same
name in `src/tests/adapter/adapter_fn/one_per_shot.rs`, where there is one, pins
the rule for a state per shot.

| Promise                                                                                                        | Tests                                                                                                                                 |
| -------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| One state serves every call of every shot, spanning the shots of its calls                                     | `one_state_serves_every_call_of_every_shot`, `a_one_state_adapters_state_spans_the_shots_of_its_calls`                                |
| A logic that sends each call as it is gives the outcomes of no adapter                                         | `a_logic_that_sends_each_call_gives_the_outcomes_of_no_adapter`                                                                       |
| A logic that sends other instructions keeps the call's tags                                                    | `a_logic_that_rewrites_each_call_keeps_its_tags`                                                                                      |
| A logic that runs below may record the readouts, as a debugger does                                            | `the_debugger_logs_each_shots_calls_with_what_they_read` (`tests/observers.rs`)                                                       |
| What a run reads passes up only as the logic writes it                                                         | `what_a_run_reads_passes_up_only_as_the_logic_writes_it`                                                                              |
| A send is one call below on the call's shots                                                                   | `each_send_is_one_call_below`                                                                                                         |
| A send does not wait for its reply: the reply below passes up as it comes                                      | `a_reply_below_passes_up_as_it_comes`                                                                                                 |
| The repetition code as a logic reads what the code as an adapter written by hand reads                         | `the_code_as_a_logic_reads_what_its_adapter_reads` (`tests/static_isa.rs`)                                                            |
| With a state per shot, each shot has a state of its own, made at its first call and kept for its later calls   | `each_shot_has_a_state_of_its_own_kept_for_its_later_calls` (`src/tests/adapter/adapter_fn/one_per_shot.rs`)                          |
| For a profiler, the shot form changes its counts, not the outcomes                                             | `the_profilers_shot_form_changes_its_counts_not_the_outcomes` (`src/tests/adapter/adapter_fn/one_per_shot.rs`)                        |
| A drawing adapter's state for shot `j` draws from shot `j`'s seed bound to its id                              | `a_drawing_adapters_states_seed_from_their_shot_bound_to_its_id` (`src/tests/adapter/adapter_fn/one_per_shot.rs`)                     |
| A drawing adapter with a state per shot offers replayable shots when the executor below offers them            | `a_drawing_adapter_offers_replayable_shots` (`src/tests/adapter/adapter_fn/one_per_shot.rs`)                                          |
| With a state per shot, a drawing adapter's shot draws alone what it draws among the others                     | `a_drawing_adapters_shot_draws_alone_what_it_draws_among_the_others` (`src/tests/adapter/adapter_fn/one_per_shot.rs`)                 |
| Another run's seed changes a drawing adapter's draws                                                           | `a_change_of_the_runs_seed_changes_a_drawing_adapters_draws` (`src/tests/adapter/adapter_fn/one_per_shot.rs`)                         |
| With one state, the shots of a call share what a drawing adapter draws                                         | `a_one_state_adapters_draw_is_shared_by_the_shots_of_a_call`                                                                          |
| An id drawn with below, however far down, refuses the chain when it is built                                   | `an_adapter_drawing_with_an_id_an_adapter_below_draws_with_is_refused` (`src/tests/adapter/adapter_fn/one_per_shot.rs`)               |
| An adapter's report folds its states' reports over its shots, beside the report below                          | `a_counting_adapters_report_is_folded_over_its_shots` (`src/tests/adapter/adapter_fn/one_per_shot.rs`)                                |
| A run's report is the same under every grouping of `>>`                                                        | `a_runs_report_is_the_same_under_every_grouping_of_shr` (`src/tests/compose/run.rs`)                                                  |
| `run.wait()` gives the adapter's report beside the one below                                                   | `a_runs_wait_gives_the_adapters_report_beside_the_one_below` (`src/tests/compose/run.rs`)                                             |
| Through an adapter that sends each call down as one, the report's `calls` are those the executor alone reports | `a_run_through_an_adapter_reports_the_calls_its_executor_alone_reports` (`src/tests/compose/adapters.rs`)                             |
| A run's report passes up the last executor's log probabilities through its adapters                            | `a_run_reports_the_last_executors_log_probability_through_its_adapters` (`src/tests/compose/adapters.rs`)                             |
| Known issue: a run through an adapter that draws reports the log probabilities below, which miss its draws     | `a_run_through_an_adapter_that_draws_reports_no_log_probability` (`src/tests/compose/adapters.rs`, ignored until fixed)               |
| The same known issue, for a drawing adapter with a state per shot                                              | `a_drawing_adapters_run_reports_no_log_probability` (`src/tests/adapter/adapter_fn/one_per_shot.rs`, ignored until fixed)             |
| With one state, a runtime whose results name no shot fails its first call that reads                           | `a_runtime_with_one_state_fails_its_first_reading_call` (`tests/static_isa.rs`)                                                       |
| A `make_state` that refuses refuses the call before the logic runs, and leaves nothing made                    | `a_state_that_cannot_be_made_refuses_the_call_before_the_logic_runs`                                                                  |
| The first call below refused, a send or a run, refuses the call with its refusal, whatever the logic returns   | `a_refused_first_send_refuses_the_call_whatever_the_logic_returns`, `a_first_run_refused_refuses_the_call_whatever_the_logic_returns` |
| After such a refusal the adapter serves the next call; one for an earlier failure below fails it               | `a_call_whose_send_is_refused_is_refused_with_the_refusal_below`, `a_refusal_below_for_an_earlier_failure_fails_the_adapter`          |
| The index a refusal below names is that of the instruction in the call below                                   | `a_refusal_below_names_the_instruction_as_sent`                                                                                       |
| With a state per shot, `send` returns `Ok` before the shots' sends go below merged                             | `a_send_returns_ok_since_nothing_goes_down_until_every_shot_ran` (`src/tests/adapter/adapter_fn/one_per_shot.rs`)                     |
| With a state per shot, a call refused below leaves the kept states as its logic left them                      | `a_refused_call_leaves_the_kept_states_as_its_logic_left_them` (`src/tests/adapter/adapter_fn/one_per_shot.rs`)                       |
| A failure below fails the call and the adapter                                                                 | `a_failure_below_fails_the_adapter`                                                                                                   |
| Nothing goes down after a send whose reply failed                                                              | `nothing_goes_down_after_a_send_whose_reply_failed`                                                                                   |
| Closing gives the report below, and closes what is below once, whatever failed                                 | `closing_gives_the_report_below`, `a_second_close_closes_nothing_below`, `a_failed_adapter_closes_what_is_below`                      |

From the guide [Observers: a profiler and a
debugger](../guides.md#observers-a-profiler-and-a-debugger), in
`tests/observers.rs` (that stacked observers each see what they see alone is in
[Composition](composition.md#the-tests-of-the-user-pages-promises)):

| Promise                                                                                                                     | Tests                                                                                                                                                |
| --------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| The profiler sends each call down as the program sent it                                                                    | `the_profiler_sends_each_call_down_as_the_program_sent_it`                                                                                           |
| The debugger sends each shot's call down alone: below it, a call is made per shot and batch                                 | `below_the_debugger_each_shots_batch_is_a_call_of_its_own`                                                                                           |
| The profiler changes no outcome                                                                                             | `the_profiler_changes_no_outcome`                                                                                                                    |
| Below the profiler, equal sends of the shots of a call go below as one call                                                 | `below_the_profiler_the_shots_equal_calls_go_as_one`                                                                                                 |
| The debugger logs each shot's calls, in order, with what each read                                                          | `the_debugger_logs_each_shots_calls_with_what_they_read`                                                                                             |
| The debugger runs each call below and passes its readouts up, changing no outcome over an executor that offers `replayable` | `the_debugger_runs_each_call_below_as_the_program_sent_it`, `the_debugger_changes_no_outcome`                                                        |
| A call that fails below is logged with its reason, fails the run and is the shot's last                                     | `the_debugger_logs_a_call_that_fails_below_with_its_reason`                                                                                          |
| A call that panics below is logged with no outcome                                                                          | `the_debugger_logs_a_call_that_panics_below_without_an_outcome`                                                                                      |
| Two debuggers that share a log each log a call with its own result                                                          | `each_logged_call_holds_its_own_result_when_debuggers_share_a_log`                                                                                   |
| A call the check below refuses whole is logged in the call's first shot alone                                               | `the_debugger_logs_a_refused_call_in_the_calls_first_shot_alone`                                                                                     |
| Over the simulator, the observers pass each shot's readouts up in the order it read them                                    | `the_observers_pass_each_shots_readouts_up_in_the_order_read`                                                                                        |
| The debugger logs each call's readouts in that order                                                                        | `the_debugger_logs_each_calls_readouts_in_the_order_read`                                                                                            |
| A batch the simulator's check refuses whole, and a program that reads a tag twice, are refused alike under each observer    | `a_batch_the_simulators_check_refuses_is_refused_alike_under_each_observer`, `a_program_that_reads_a_tag_twice_is_refused_alike_under_each_observer` |
| The two observers stacked give the same run and log grouped in each of the five ways `>>` allows                            | `the_observers_grouped_any_way_give_the_same_run_and_log`                                                                                            |

From the API docs of `adapter_fn`, its executors and its reply, in
`src/tests/adapter/adapter_fn.rs` for one state and in
`src/tests/adapter/adapter_fn/one_per_shot.rs` for a state per shot:

| Promise                                                                                                                                                                                                                                                                   | One state                                                                                                                                                                                                                                            | A state per shot                                                                                                                                                                                                                                                       |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A send's reply that fails only after the call returned ends the call's reply when it is polled, with the call's own failure if it has one                                                                                                                                 | `a_call_that_failed_ends_with_its_own_failure_though_a_send_fails_after_it_returned`                                                                                                                                                                 | `a_call_that_failed_ends_with_its_own_failure_though_a_send_fails_after_it_returned`                                                                                                                                                                                   |
| The states made for a call whose merged send's reply fails only after the call returned are kept and reported                                                                                                                                                             |                                                                                                                                                                                                                                                      | `a_state_whose_merged_sends_reply_failed_later_is_reported`                                                                                                                                                                                                            |
| So are those made for a later call admitted before that failure was recorded                                                                                                                                                                                              |                                                                                                                                                                                                                                                      | `a_state_made_for_a_call_admitted_before_an_earlier_failure_was_recorded_is_reported`                                                                                                                                                                                  |
| The drop panic of a state made for a call that is refused, or fails, reaches the caller (the first one's, once every such state is freed), and the adapter fails: one catch in `call` fails it for this panic as for a panic of the check, the state closure or the logic | `a_refused_call_frees_the_state_made_for_it_and_raises_its_drops_panic`, `a_failed_call_frees_the_state_made_for_it_and_raises_its_drops_panic`, `a_logic_that_panics_fails_the_adapter`, `a_check_or_a_state_closure_that_panics_fails_the_adapter` | `a_refused_call_frees_every_state_made_for_it_then_raises_the_first_drops_panic`, `a_failed_call_frees_every_state_made_for_it_then_raises_the_first_drops_panic`, `a_logic_that_panics_fails_the_adapter`, `a_check_or_a_state_closure_that_panics_fails_the_adapter` |
| Freed as the logic's panic unwinds, such a state leaves the panic of its drop to the panic hook                                                                                                                                                                           | `a_logics_panic_reaches_the_caller_though_its_new_states_drop_panics_too`                                                                                                                                                                            | `a_logics_panic_reaches_the_caller_though_its_new_states_drops_panic_too`                                                                                                                                                                                              |
| Each reply a call below gives absorbs its drop's panic, whether dropped pending or once ended                                                                                                                                                                             | `a_dropped_replys_pending_sends_are_freed_raising_no_panic_of_their_drops`, `a_failed_calls_pending_sends_are_freed_raising_no_panic_of_their_drops`, `a_reply_from_below_whose_drop_panics_once_ended_leaves_the_call_done`                         |                                                                                                                                                                                                                                                                        |
| The kept sends are freed as the adapter's executor is dropped while a panic unwinds, raising no panic of their drops                                                                                                                                                      |                                                                                                                                                                                                                                                      | `dropped_as_a_panic_unwinds_it_frees_the_kept_sends_raising_none`                                                                                                                                                                                                      |
| A send whose instruction's copy panics frees the copies made before it, then its panic reaches the caller                                                                                                                                                                 |                                                                                                                                                                                                                                                      | `a_send_whose_copy_panics_frees_the_copies_made_before_it`                                                                                                                                                                                                             |
| Dropped rather than closed, the adapter's executor absorbs the panic of each state's drop                                                                                                                                                                                 | `dropped_it_frees_its_state_raising_no_panic_of_its_drop`                                                                                                                                                                                            | `dropped_it_frees_every_state_raising_no_panic_of_their_drops`                                                                                                                                                                                                         |
| `AdapterFn::check` frees the check it replaces, raising no panic of its drop                                                                                                                                                                                              | `a_replaced_closure_is_freed_raising_no_panic_of_its_drop`                                                                                                                                                                                           |                                                                                                                                                                                                                                                                        |
| At close, the panic of a state's drop, of `state_report` or of the report's `Q::default` reaches the closer once the executor below is closed                                                                                                                             | `a_close_whose_state_drop_panics_closes_below_then_raises_it`, `a_close_whose_state_report_panics_closes_below_then_raises_it`, `a_close_whose_reports_default_panics_closes_below_then_raises_it`                                                   | `a_close_whose_states_drops_panic_closes_below_then_raises_the_first`, `a_close_whose_state_report_panics_closes_below_then_raises_it`, `a_close_whose_reports_default_panics_closes_below_then_raises_it`                                                             |
| When both `state_report` and a state's drop panic, `state_report`'s panic reaches the closer                                                                                                                                                                              | `a_state_reports_panic_reaches_the_closer_though_the_states_drop_panics_too`                                                                                                                                                                         | `a_state_reports_panic_reaches_the_closer_though_the_states_drops_panic_too`                                                                                                                                                                                           |
| When the close below panics too, the first panic reaches the closer, and the close below's is left to the panic hook                                                                                                                                                      | `a_state_reports_panic_wins_over_the_close_belows`, `a_state_drops_panic_wins_over_the_close_belows`                                                                                                                                                 | `a_state_reports_panic_wins_over_the_close_belows`, `the_first_state_drops_panic_wins_over_the_close_belows`                                                                                                                                                           |
| When only the close below panics, its panic reaches the closer                                                                                                                                                                                                            | `a_close_belows_panic_reaches_the_closer_when_nothing_panicked_before`                                                                                                                                                                               | `a_close_belows_panic_reaches_the_closer_when_nothing_panicked_before`                                                                                                                                                                                                 |
| The call's reply passes up each send's readouts in the order its reply gives them, a later send's before those an earlier send gives later                                                                                                                                | `a_sends_readouts_pass_up_in_the_order_its_reply_gives_them`, `a_later_sends_readouts_pass_up_before_those_an_earlier_send_gives_later`                                                                                                              |                                                                                                                                                                                                                                                                        |
| With one state that owns its way below, a run below gives the readouts of every shot, by shot, each shot's in its reply's order                                                                                                                                           | `an_owned_run_gives_its_readouts_by_shot_each_in_the_order_read` (`src/tests/adapter/adapter_fn/owning.rs`)                                                                                                                                          |                                                                                                                                                                                                                                                                        |

From the API docs of the adapter written by hand (`Adapter`, `AdapterBoundary`,
`AdapterSeeds`) and of the offer (`Capabilities`):

| Promise                                                                                                                                           | Tests                                                                                                                                                                                                                                         |
| ------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Adapter::accepts` is read before `build`, and the set it states is offered upstream whatever the adapter offers                                  | `a_code_stating_its_offer_whole_from_downstreams_offers_the_set_it_accepts` (`src/tests/compose/adapters.rs`)                                                                                                                                 |
| An adapter that states no set and passes its calls through as they are offers downstream's set, whatever it offers                                | `an_adapter_passing_its_calls_through_offers_the_set_downstream_whatever_its_offer` (`src/tests/compose/adapters.rs`)                                                                                                                         |
| An adapter written by hand that draws, composed over an executor given a seed, draws from its own seeds in either grouping                        | `a_noise_model_written_by_hand_draws_from_its_own_seeds_in_either_grouping` (`tests/simulator.rs`)                                                                                                                                            |
| `AdapterBoundary::drawing_seeds` is `None` for an adapter that draws nothing, and for one refused for no run's seed                               | `an_adapter_that_draws_nothing_is_given_no_seeds`, `an_adapter_refused_for_no_run_seed_is_given_no_seeds` (`src/tests/adapter/boundary.rs`)                                                                                                   |
| An adapter written by hand that draws where no run's seed reaches it is refused when built                                                        | `an_adapter_written_by_hand_drawing_with_no_run_seed_below_is_refused` (`src/tests/adapter/boundary.rs`)                                                                                                                                      |
| An adapter written by hand draws from its own seeds, not as the last executor does, and from those an `adapter_fn` drawing with its id draws from | `an_adapter_written_by_hand_draws_otherwise_than_the_last_executor` (`src/tests/compose/adapters.rs`), `an_adapter_written_by_hand_draws_from_the_seeds_an_adapter_fn_with_its_id_draws_from` (`src/tests/adapter/boundary.rs`)               |
| `probabilities` is never offered upstream of an adapter that draws, whatever it offers, not even by one above it that draws nothing               | `a_driver_needing_probabilities_is_refused_upstream_of_an_adapter_that_draws_and_states_its_offer_whole`, `a_driver_needing_probabilities_is_refused_through_an_adapter_offering_them_above_one_that_draws` (`src/tests/compose/adapters.rs`) |
| `rng_ids` offers the ids below an adapter and its own, whatever it offers                                                                         | `adapters_drawing_with_the_same_id_are_refused_through_one_that_states_its_offer_whole`, `an_adapter_writing_other_ids_into_its_offer_offers_those_below_and_its_own` (`src/tests/compose/adapters.rs`)                                       |
