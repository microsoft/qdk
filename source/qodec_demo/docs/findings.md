# Behaviours and findings

What the demo shows, each behaviour with the tests that pin it, and its
findings: where the protocol falls short, each pinned by an ignored test
that states what should hold and fails today
(`cargo test -p qodec_demo -- --ignored`), with the reason in its
`#[ignore]`. The [README](../README.md) gives the problem, the model and
the measurements.

Tests are named by function, and a section links the file that holds
them. Paths are the demo's (`src/…`, `tests/…`), unless marked as the
protocol's (`executor_protocol`). The ids QD1 to QD5 name the questions
of [open-questions.md](open-questions.md); QD6, Q65 and Q67 to Q69 are in
the README's [open questions](../README.md#open-questions).

## What it shows

### The chain gives the stack's outcomes

In [`tests/repetition3.rs`](../tests/repetition3.rs) and
[`tests/stacked_codes.rs`](../tests/stacked_codes.rs):

- repetition3 (`the_chain_gives_the_qodec_stacks_outcomes`);
- c422, one layer (`c422_gives_the_qodec_stacks_outcomes`);
- c4c6, two layers (`c4c6_gives_the_qodec_stacks_outcomes`).

The bottom instructions are the stack's too, batch by batch and in order
(`c4c6_runs_the_qodec_stacks_bottom_batches_in_order`).

### The chain gives the stack's statuses

Each layer marks each result's outcomes with its runtime's status, as
the shot's conditions (`ShotConditions`), and the adapter facility adds,
with no code in the layer, the conditions of the outcomes the shot read
below so far. Each outcome then carries the status the stack gives its
result: a shot's conditions are sticky, as a runtime's status is within a
shot, and the stack's top runtime takes the status from the runtime
below into its results. In c4c6 the middle raises `unresolved` during the
idles, and every later result carries it
(`c4c6_middle_raises_unresolved_in_the_idles_and_every_later_result_carries_it`).

- repetition3 (`the_chain_gives_the_qodec_stacks_statuses`, in
  `tests/repetition3.rs`), also when a later call reuses a readout tag:
  each call's reply carries its own result's status
  (`reused_readout_tags_keep_every_results_status`);
- c422, one layer (`c422_gives_the_qodec_stacks_statuses`);
- c4c6, two layers: the top results carry the `unresolved` the middle
  layer raises (`c4c6_top_statuses_equal_the_qodec_stacks`), and a lost
  bottom readout comes up as a `Value::Missing`, which the top runtime
  takes as an unresolved bit
  (`a_lost_readout_reaches_c4c6s_top_as_a_value`).

Below the top layer, an observer sees the middle layer mark each shot's
outcomes `unresolved` (`c4c6_middle_raises_unresolved_on_every_shot`),
and each of a shot's results keeps its status on its own reply, though
the top layer reuses its tags below in each batch
(`the_middle_layer_gives_a_status_per_result_of_each_shot`). These are
in [`tests/stacked_codes.rs`](../tests/stacked_codes.rs).

### Composition and determinism

In [`tests/determinism.rs`](../tests/determinism.rs). With the same seed,
c4c6's profiled chain gives the same outcomes, each with its conditions,
profiler totals and device report in each of these cases.

- `>>` is associative: the two layers composed first, or the layers and
  the profiler, or the profiler and the coins as one executor, give the
  same run (`c4c6_gives_the_same_run_with_its_layers_composed_first`,
  `c4c6_gives_the_same_run_with_its_layers_and_profiler_composed_first`,
  `c4c6_gives_the_same_run_with_its_executor_composed_first`).
- The run awaited gives the same run as waited
  (`c4c6_gives_the_same_run_awaited_as_waited`).
- Runs of c4c6, c422 and repetition3 (two seeds), awaited together on
  one thread, each give the outcomes, with their conditions, they give
  alone
  (`runs_awaited_together_on_one_thread_each_give_what_they_give_alone`).
- A shot gives the same outcomes alone as among others
  (`a_shot_of_c4c6_gives_the_same_outcomes_alone`, in
  `tests/stacked_codes.rs`).

### The profiler

In [`tests/profiler.rs`](../tests/profiler.rs), unless named otherwise.

- Below the layer, it counts the gadgets' physical instructions
  (`below_the_layer_the_profiler_counts_the_gadgets_instructions`).
- Above the layer, it counts the program's logical instructions
  (`above_the_layer_the_profiler_counts_the_programs_instructions`).
- Below c422's layer and c4c6's two, it counts what the Qodec runtime's
  own stack sends its bottom backends
  (`below_c422s_layer_the_profiler_counts_the_stacks_bottom_instructions`,
  `below_two_layers_the_profiler_counts_the_stacks_bottom_instructions`);
  above them, the program's instructions
  (`above_c422s_layer_the_profiler_counts_the_programs_instructions`,
  `above_two_layers_the_profiler_counts_the_programs_instructions`).
- Below two layers, it counts 14 bottom batches per top batch
  (`below_two_layers_the_profiler_counts_the_runtimes_bottom_batches`,
  whose body is the README's example).
- With a profiler above c4c6's layers and one below, each counts its own
  place's instructions
  (`two_profilers_in_one_chain_each_count_their_own_places_instructions`).
- It changes no outcome (`the_profiler_changes_no_outcome`).
- It sends each call below as it is
  (`the_profiler_sends_each_call_below_as_it_is`), as does the recorder
  (`the_recorder_sends_each_call_below_as_it_is`, in
  `tests/calls_below.rs`).
- It keeps the chain's non-synchronous path: awaited over the profiler
  alone, no reply below waits on the thread that polls the run
  (`awaited_over_the_profiler_no_reply_below_waits_on_the_thread_that_polls_the_run`).
- It counts what is sent, in the shot's state: once the shot has a
  state, a call the executor below refuses is counted too
  (`once_a_shot_has_a_state_the_profiler_counts_a_call_refused_below`),
  but a shot's first call refused below is not, since the adapter
  facility keeps no state made for a refused call
  (`the_profiler_counts_no_shots_first_call_refused_below`). The
  recorder keeps the same calls
  (`once_a_shot_has_a_state_the_recorder_keeps_a_call_refused_below`,
  `the_recorder_keeps_no_shots_first_call_refused_below`, in
  `tests/calls_below.rs`). So the counts equal what ran below in a run
  that succeeds, not always in one that does not.
- It counts each shot's calls, so its totals do not depend on how the
  calls below the layers are grouped: one shot a call, or equal batches
  of several shots in one call; as c4c6's layers send them is the first
  again
  (`the_profilers_totals_do_not_depend_on_how_the_calls_below_qodec_are_grouped`).
  So it cannot count the device's calls: a counter of calls has one
  state, which sees each call whole (`src/call_counter.rs`, used in
  `tests/calls_below.rs`), and sends each call below as it is
  (`the_call_counter_sends_each_call_below_as_it_is`, in
  `tests/calls_below.rs`).

### Calls below a layer

In [`tests/calls_below.rs`](../tests/calls_below.rs). A call's shots go
below by the adapter facility's rule for a state per shot: a batch that
reads goes below on its shot alone, after the batches the shot sent
before it in the call, each alone too; the batches a shot sends after
its last read in the call merge with the other shots' equal sends into
one call. The protocol's tests pin that rule
(`a_shots_run_leaves_the_other_shots_sends_merged`,
`a_shots_sends_after_its_run_merge_with_the_others`).

- An upper call whose lowering reads nothing goes below on its shots at
  once; each batch that reads goes below on its shot alone
  (`below_a_layer_only_an_upper_call_whose_lowering_reads_nothing_goes_below_on_its_shots_at_once`).
- Every bottom batch of c4c6 reads, so each goes below on its shot
  alone: 276 device calls on 4 shots, as many as the stack's batches
  (`below_c4c6_every_bottom_batch_goes_below_on_its_shot_alone`).
- No program of the demo sends a batch before or after a read in one
  upper call, so the demo does not exercise those branches of the rule:
  c4c6 by the test above, c422 and repetition3 by their 9 calls on 4
  shots
  (`below_a_layer_only_an_upper_call_whose_lowering_reads_nothing_goes_below_on_its_shots_at_once`,
  `below_repetition3_no_upper_call_sends_a_batch_beside_a_read`).
- Merging a shot's batches that read with the other shots' needs a
  lockstep per-shot facility whose shots' logic can stop at a read:
  async, resumable, or a thread per shot (open, Q67; the finding
  [A call's shots are not read at once](#a-calls-shots-are-not-read-at-once)).

### A backend that answers later

In [`tests/answers_later.rs`](../tests/answers_later.rs), unless named
otherwise.

- The outcomes and the profiler's counts equal those over a backend that
  answers within its call
  (`the_profiled_chain_gives_the_same_run_over_a_backend_that_answers_later`,
  `repetition3_gives_the_same_run_over_a_backend_that_answers_later`).
- The same holds over a thread per shot, and over one thread for every
  shot
  (`the_profiled_chain_gives_the_same_run_over_a_thread_per_shot`,
  `the_profiled_chain_gives_the_same_run_over_coins_on_a_thread_of_their_own`).
- Each shot's bottom batches still come in the stack's order
  (`over_a_backend_that_answers_later_each_shot_runs_the_stacks_batches_in_order`),
  even when the program holds all its calls in flight and waits for the
  last first
  (`a_program_holding_all_its_calls_runs_the_stacks_batches_in_order`).
  This holds in mode (c) alone (QD1, QD3): each layer serves a call
  within it, and `AnswersLater` runs the wrapped call at `call`, so the
  order below is fixed when each call is made. Once a resumable or async
  runtime lets a shot's calls interleave, a per-shot queue of calls in
  flight (QD3) is needed for it to hold.
- `AnswersLater` counts a call's shots in flight from its reply until the
  reply ends or is dropped, and a refused call's never
  (`a_refused_call_puts_no_shot_in_flight`,
  `an_ended_reply_takes_its_shots_out_of_flight`,
  `a_dropped_reply_takes_its_shots_out_of_flight`, in
  `src/tests/answers_later.rs`). Below the layer, an upper call whose
  lowering reads nothing is in flight on all its shots at once
  (`below_a_qodec_layer_a_calls_shots_are_in_flight_at_once`).
- The layer blocks while it waits for a read: the runtime calls its
  backend and waits inside its own call (mode (c) of QD1).
  - Each read below is on one shot
    (`below_a_qodec_layer_each_read_is_on_one_shot`).
  - Awaited by a program that waits for each call, the run yields while
    that call's sends below are pending, but holds the thread that polls
    it through each read below: over `AnswersOnYield`, repetition3's
    prepare on 4 shots is answered by a yield
    (`awaited_over_a_qodec_layer_the_run_yields_while_a_send_below_is_pending`),
    and each of its 8 reads on 4 shots waits for its timeout
    (`awaited_over_a_qodec_layer_the_run_holds_its_thread_through_each_read_below`).
  - A send still pending when a later read parks is held too: awaited by
    a program holding all its calls in flight, repetition3's prepare
    waits for its timeout as the reads do
    (`awaited_over_a_qodec_layer_a_send_pending_when_a_later_read_parks_is_held_too`).
  - Over the profiler alone, no reply below waits on that thread: the
    profiler keeps the chain's non-synchronous path
    (`awaited_over_the_profiler_no_reply_below_waits_on_the_thread_that_polls_the_run`,
    in `tests/profiler.rs`).
- A chain with a Qodec layer cannot move to another thread, so it cannot
  be held by a binding that needs `Send`, such as QEP's Python profiler
  slot (`Box<dyn Profiler + Send + Sync>`; pyo3 classes must be `Send`);
  build it where it runs. The layer's adapter is not `Send` (the
  compile-fail example in `src/qodec_layer.rs`). The layer's plans are
  `Rc`, and each shot's runtime holds state that is not `Send` either
  (its decoder's runtime and its own `Rc` plans), so making the layer's
  plans `Arc` would not be enough. A caller that must not block, such as
  an async one, builds and runs the chain on a thread of its own, with
  the same run as on its own thread
  (`a_qodec_chain_built_and_run_on_a_thread_of_its_own_gives_the_same_run`).

### A device that fails

In [`tests/device_failures.rs`](../tests/device_failures.rs), below
c4c6's layers and the profiler, answering within its call or on a thread
per shot.

- The run fails, naming the device's reason, not a layer's
  (`a_device_failing_mid_run_fails_the_run_naming_its_reason`,
  `a_device_on_threads_failing_mid_run_fails_the_run_naming_its_reason`).
- A later call is refused for the failure, which it names
  (`after_a_device_failure_a_later_call_is_refused_naming_it`,
  `after_a_device_on_threads_failed_a_later_call_is_refused_naming_it`).
- Below repetition3's layer, a later call in flight ends done while the
  failing reply is unpolled, and fails with no events once it is polled
  (`a_later_call_in_flight_ends_done_while_the_failing_reply_is_unpolled`,
  `once_the_failing_reply_is_polled_a_later_call_in_flight_fails_with_no_events`);
  a new call refused for the failure, on the failed shot or a fresh
  one, leaves that later call to end done
  (`refusing_a_call_on_the_failed_shot_leaves_a_later_call_in_flight_to_end_done`,
  `refusing_a_call_on_a_fresh_shot_leaves_a_later_call_in_flight_to_end_done`).

### Close and drop

In [`tests/close_and_drop.rs`](../tests/close_and_drop.rs), repetition3's
chain with two calls in flight on each of 64 shots, held at the device.

- Dropped, every reply ends, cancelled, done or failed for a
  cancellation, with no panic and no hang
  (`dropped_with_calls_in_flight_every_reply_ends`), and the backend is
  dropped once, never closed
  (`dropped_with_calls_in_flight_the_backend_is_dropped_once_unclosed`;
  a run dropped mid-flight too,
  `a_run_dropped_mid_flight_drops_the_backend_once_unclosed`).
- Closed, `close` waits for every call queued, so each reply ends done
  (`closed_with_calls_in_flight_every_reply_ends_done`); closed twice,
  the chain closes the backend once
  (`closed_twice_with_calls_in_flight_the_backend_is_closed_once`), and
  once by a run that ends (`a_run_of_many_shots_closes_the_backend_once`).

### Instruction sets

In [`tests/instruction_sets.rs`](../tests/instruction_sets.rs), unless
named otherwise.

- The layer states only its own sets. The chain matches them, by
  mnemonic, in a set that numbers its instructions otherwise
  (`the_layer_runs_over_a_set_that_numbers_its_instructions_otherwise`).
- A set lacking an instruction the layer sends is refused at build
  (`the_layer_over_a_set_lacking_an_instruction_it_sends_is_refused_at_build`).
- The layers file readouts by tag, so their order below does not matter
  (`the_layers_read_the_executors_readouts_in_any_order`).
- A `number` argument written as an integer (`theta=1`) is sent as the
  nearest `Real`, as the set declares, while an `integer` argument stays
  an `Integer` (`an_integer_given_for_a_number_is_sent_as_the_nearest_real`);
  so repetition3 runs alike with `theta=1` and `theta=1.0`
  (`a_number_written_as_an_integer_runs_as_its_real_spelling`, in
  `tests/repetition3.rs`). Each argument is sent as its own parameter
  declares, whatever parameters come before it
  (`each_argument_is_sent_as_its_own_parameter_declares`).
- Each operand of a call is read as the parameter at its place declares
  (a `Bool` at a `bit` or a `boolean`, an `Int` at an `integer`, a
  `Float` at a `number`), so an instruction whose arguments have these
  forms comes back from its call as it was
  (`a_call_comes_back_as_the_instruction_it_was_made_from`).
- `Boundary::new`, which `QodecLayer::new` calls on the layer's sets,
  refuses a Qodec set the protocol's operands cannot express, naming
  each instruction at fault: one with a variadic block operand, among
  its inputs or its outputs
  (`a_qodec_set_with_a_variadic_block_operand_is_refused`), and one
  with a text argument (`a_qodec_set_with_a_text_argument_is_refused`).
  `QodecLayer::new` refuses the layer with every fault of its own set,
  each prefixed `layer k, its set:`, and of the set below, each prefixed
  `layer k, the set below:`, both sets' together (in
  `src/tests/qodec_layer.rs`,
  `a_layer_whose_set_the_protocol_cannot_express_is_refused_with_each_fault`,
  `a_layer_over_a_set_the_protocol_cannot_express_is_refused_with_each_fault`,
  `a_layer_with_faults_in_its_set_and_the_set_below_is_refused_with_each_naming_its_set`).

### Qubits

In [`tests/qubits.rs`](../tests/qubits.rs).

- The qubits a program needs below are not knowable when the chain is
  built: the runtime takes them as its blocks need them, so a lone block
  1 runs on the lowest qubits below
  (`unbounded_a_lone_block_runs_on_the_lowest_qubits_below`).
- `QodecLayer::within` refuses, at each call and before the runtime
  runs, a block beyond what fits, by the runtime's own count of the
  qubits blocks need
  (`within_counts_the_qubits_blocks_need_by_the_runtimes_count`): the
  program's batch is refused, not failed
  (`within_the_qubits_below_a_call_on_a_block_beyond_them_is_refused`,
  and at a later call,
  `within_the_qubits_below_a_later_call_on_a_block_beyond_them_is_refused`).
  A block that fits runs as it does unbounded
  (`within_the_qubits_below_a_held_block_runs`); unbounded, blocks
  beyond the qubits below fail below
  (`unbounded_blocks_beyond_the_qubits_below_fail_below`).

### Refusals and failures

In [`src/tests/qodec_layer.rs`](../src/tests/qodec_layer.rs).

- `QodecLayer::new` refuses the bottom layer, which has nothing below it,
  and a layer beyond it, up to the last index
  (`the_bottom_layer_is_refused_since_nothing_is_below_it`,
  `a_layer_beyond_the_bottom_is_refused`,
  `the_last_index_is_refused_as_beyond_the_bottom`).
- It refuses a layer the Qodec runtime cannot plan, with the runtime's
  error, prefixed `layer k:`
  (`a_layer_the_runtime_cannot_plan_is_refused_naming_it`), and one
  whose decoder it cannot plan, with the decoder's error, prefixed so
  too (`a_layer_whose_decoder_cannot_be_planned_is_refused_naming_it`).
  Either error comes first in one refusal, then the faults of the
  layer's set, then those of the set below
  (`a_layer_the_runtime_cannot_plan_is_refused_with_its_error_then_each_sets_faults`,
  `a_layer_whose_decoder_cannot_be_planned_is_refused_with_its_error_then_each_sets_faults`).
- The layer refuses a call its set does not take, such as one on
  operands its instruction does not take, and one repeating a tag
  (`a_call_on_operands_its_instruction_does_not_take_is_refused`,
  `a_call_repeating_a_tag_is_refused`).
- A readout missing below fails the call
  (`a_readout_missing_below_fails_the_call`), and so does an outcome
  under a tag the call did not read
  (`an_outcome_not_asked_for_below_fails_the_call`).
- A runtime error that ran nothing below, such as a measurement of a
  block never prepared, fails the call and the layer, which refuses
  every later call
  (`a_runtime_error_that_ran_nothing_fails_the_call_and_every_later_call`):
  the layer's logic can fail a call but not refuse it.
- Once a call failed (its runtime part-way through a batch), the layer
  refuses every later call, on any shot, naming the failure
  (`a_layer_whose_runtime_failed_part_way_refuses_every_later_call`).
  This is coarser than the stack, where only the failed shot's runtime
  is lost: by the protocol's rule, a failure ends the executor, so every
  shot ends.
- One state serving two shots fails loudly
  (`a_layer_of_one_state_fails_a_call_on_two_shots`).

### An adapter that reads below

An adapter that reads or maps the outcomes below has two ways today:

- Written with `adapter_fn` or `owning_adapter_fn`, it sees them only by
  `run`, which the protocol's `Below::run` documents as "parking the
  thread that called the adapter until the reply below ends", and which
  with a state per shot goes below on its shot alone. Under an awaited
  run, the thread that called the adapter is the one that polls the run
  (`awaited_over_a_qodec_layer_the_run_holds_its_thread_through_each_read_below`,
  in [`tests/answers_later.rs`](../tests/answers_later.rs)). The demo's
  layer reads this way; its profiler, an `adapter_fn` too, only sends.
- Written by hand (`Adapter` and `Apply`, the protocol's
  `docs/adapters.md`), it can instead wrap the reply below and map each
  event as it passes up, holding no thread, as the protocol's tests'
  `Decode` (`tests/static_isa/repetition3.rs`) and `Recorded`
  (`tests/debugger/log.rs`) do. The demo has no such adapter; its
  `AnswersLater` and `AnswersOnYield` wrap replies so, as executors
  written by hand.

For the second way, the protocol gives `MappedReply`, which maps each
event of a reply by a plain function and keeps no state, and `relay`,
with which a worker thread passes an inner reply's events up as they
come. An adapter that keeps state as it maps, as `Decode` keeps its
votes, still writes its own reply, and `adapter_fn` and
`owning_adapter_fn` offer no way to map the reply below at all (Q69).

### The protocol's conformance checks

In [`tests/conformance.rs`](../tests/conformance.rs), the checks of the
protocol's `conformance` feature run on the coins (`coin_device` of
`stim+rz`), and on repetition3's layer 0 composed with them, one check
per test. Both refuse calls once closed, take calls after a refusal,
refuse calls outside their set, report once, and keep the promises they
make: `replayable`, and
`resumable_shots` (a shot continues from one call to the next, a call
changes no shot it does not name, new parts start their shots as made).
The coins also refuse a repeated tag at the read that repeats it, which
they did not until these checks found it
(`the_coins_refuse_a_tag_repeated_within_a_batch`,
`the_coins_refuse_a_repeated_tag_at_the_index_of_its_instruction`); the
layer, whose own refusal is pinned above, ends a call in flight at an
earlier failure
(`the_layer_over_coins_ends_calls_in_flight_at_an_earlier_failure`), and
keeps each shot's conditions from one call to the next
(`the_layer_over_coins_keeps_each_shots_conditions_from_one_call_to_the_next`).

The layer's checks run on batches that each prepare the block before
measuring it, since a measurement of a block never prepared fails in
the runtime; so the protocol's tag checks, whose calls are each meant to
run alone, run on the coins only.

## Findings

Each finding states what it shows, the tests that pin how the demo
behaves today, and the ignored test that states what should hold.

### The layer cannot leave a backend that answers later free

[QD1](open-questions.md#qd1-calling-below-more-than-once-in-a-call), in
[`tests/answers_later.rs`](../tests/answers_later.rs). Awaited, the run
holds the thread that polls it through each read below: each of
repetition3's 8 reads on 4 shots waits for its timeout
(`awaited_over_a_qodec_layer_the_run_holds_its_thread_through_each_read_below`;
the other tests of
[A backend that answers later](#a-backend-that-answers-later)).

Ignored:
`awaited_over_a_qodec_layer_no_reply_below_waits_on_the_thread_that_polls_the_run`,
no read below waiting for its timeout, as over the profiler alone.

To fix it, the runtime's flush must be made resumable or `async` (QD1
modes (a), (b)), and the layer either written by hand (`Adapter` and
`Apply`: its own reply polls the suspended runtime over the executor
below, as the Qodec prototype sketched for one instruction, at the cost
of sharing the executor below as `Rc<RefCell<E>>`, of `E: 'static`, and
of a copied batch) or given by the protocol as an owning adapter whose
logic can wait without blocking (Q65). That frees the thread that polls
the run, as running the chain on a thread of its own does today, which
gains no wall time ([Measurements](../README.md#measurements), way
`thread`): the time goes on the shots, served one after another. It
comes back only if the shots' logic runs concurrently, each shot's read
pending while the others go on (a facility that polls every shot's
suspended logic, not one shot to its end, or a thread per shot), as a
chain a shot does with the layer's own calls on c4c6 (way `shots`).

### A call's shots are not read at once

[QD2](open-questions.md#qd2-shots-that-need-different-calls-below) and
Q67, in [`tests/answers_later.rs`](../tests/answers_later.rs). Each read
below is on one shot (`below_a_qodec_layer_each_read_is_on_one_shot`).

Ignored: `below_a_qodec_layer_a_calls_shots_are_read_at_once`, a call's
reads on repetition3 going below on its 4 shots at once.

Freeing the thread does not fix it: a shot's read would still go below
on that shot alone. Merging a shot's batches that read with the other
shots' needs a lockstep per-shot facility whose shots' logic can stop at
a read: async, resumable, or a thread per shot (open, Q67;
`tests/calls_below.rs` pins how the calls go below today). On a device
bound by its latency, such a merge would also bound the round trips by
the steps ([Measurements](../README.md#measurements), lockstep merging).
Which to build first is open (Q65, Q67).

### A dropped chain reports a cancellation as a failure

Q68, in [`tests/close_and_drop.rs`](../tests/close_and_drop.rs).

- Dropped with each shot's prepare running and its rotation queued, the
  placed device below ends each prepare done and each rotation
  cancelled, and there a cancelled call fails no later call (the
  executors' rule, Q42, decided: the protocol's `docs/executors.md`,
  `a_cancelled_call_fails_no_later_call`;
  `dropped_alone_the_device_ends_prepares_done_and_rotations_cancelled`).
- Above it, the adapter that gets a cancelled send fails, so every later
  reply ends failed with "an earlier call failed: the call was cancelled
  before it ran to its end", those of prepares that ran included: on 64
  shots, each reply ends cancelled, done, or failed for a cancellation
  (`dropped_with_calls_in_flight_every_reply_ends`); the ignored test
  below fails with those failures.
- The protocol documents this adapter rule (its `docs/adapters.md`).
  Whether an adapter whose send below is cancelled ends its own reply
  cancelled, and records nothing, is a protocol design question, open
  (Q68, which contradicts the executors' rule of Q42).

Ignored: `dropped_with_calls_in_flight_every_reply_ends_cancelled_or_done`,
each shot's prepare ending done and its rotation cancelled.

### A layer cannot state what it offers or needs

[QD4](open-questions.md#qd4-what-a-layer-states-when-the-chain-is-built),
in [`tests/qubits.rs`](../tests/qubits.rs).

The adapter facility offers what is below unchanged and needs nothing of
it: today a job on 2 blocks that states 2 qubits is accepted over 5
qubits that hold one block
(`a_job_needing_more_blocks_than_fit_below_is_accepted`), and a layer
over an executor without resumable shots builds
(`a_layer_over_an_executor_without_resumable_shots_builds`). Its `.offers` and
`.needs` are planned.

Ignored:

- `a_job_needing_more_blocks_than_fit_below_is_refused_when_given`, that
  job refused when it is given, 2 qubits needed against 1 offered;
- `a_layer_over_an_executor_without_resumable_shots_does_not_build`, that chain
  refused at build.
