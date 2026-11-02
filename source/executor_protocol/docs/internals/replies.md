# Replies: the detailed rules

[Protocol overview](../../README.md).

This page is for maintainers of the crate, and for those who need a rule at its
edge: the order of a reply's events for each kind of executor; how the two
replies of one call differ, and how they end when a side is dropped or
cancelled; a worker relaying an inner reply; the waking cycle step by step, how
each way of taking a reply sleeps, why `wait` loses no wake, several calls in
flight and async tasks, with the tests' runtimes; the rules and tests of readout
tags, with what the check costs; and, last, the tests of the promises of
[Replies](../replies.md) that the sections do not cite. A user taking or making
replies needs none of it to start, and starts at [Replies](../replies.md).

## The order of a reply's events

`poll_events` moves the events available now to the end of `out`
([Replies](../replies.md#replies-one-reply-four-ways-to-take-it)). The
events of one shot come in the order its executor gives them, not sorted
by tag. The executors `executor_fn` makes, with one
state or a state per shot, placed or not, give them in the order the shot
gives them (its measurement order)
(`a_shots_outcomes_keep_the_order_of_its_reads_not_of_their_tags`,
`a_shots_late_outcomes_keep_the_order_of_its_reads_not_of_their_tags`,
`src/tests/shot_states.rs`;
`outcomes_keep_the_order_the_logic_pushes_them_in`,
`src/tests/executor_fn.rs`; placed,
`each_event_reaches_the_caller_as_the_executor_it_places_hands_it_over`,
`src/tests/placement.rs`). An adapter may give them in another order:
an `adapter_fn` whose logic sends, then runs, gives the run's readouts
before the send's
(`an_observer_that_runs_passes_up_each_shots_readouts_in_the_replys_order`,
`src/tests/adapter/adapter_fn.rs`;
[Adapters](adapters.md#the-order-of-a-runs-readouts)). So a consumer
matches each readout by its tag
([Readout tags](../replies.md#readout-tags)), not by its place in the
reply. The order across shots carries no meaning,
since the outcomes of shots run in parallel interleave
(`an_executor_hands_over_each_shots_events_as_they_come`,
`src/tests/shot_states.rs`). So a driver that compares the outcomes of two
builds compares them shot by shot, as the test of law 2 does
([Levels, roles and laws](composition.md#levels-roles-and-laws)).

## One call: the rules

The two replies of [One call](../replies.md#one-call), side by side:

| Reply              | Where the work runs           | When `call` returns                                 | Polls until final status                           | Wakes the caller                                             |
| ------------------ | ----------------------------- | --------------------------------------------------- | -------------------------------------------------- | ------------------------------------------------------------ |
| `ImmediateReply`   | inside `call`                 | every event and the final status are available      | one                                                | never: nothing to wait for                                   |
| `IncrementalReply` | elsewhere, by a `ReplySender` | events or the final status may already be available | depends on event arrival and when the caller polls | at most once per stored waker, on a nonempty push or the end |

The events, the end and the caller's waker share one lock, so a push and a
"nothing yet, wake me" cannot cross. A `ReplySender` dropped before it ends
(its thread panicked) ends the reply with `Failed`, so the caller is not
left waiting (test `a_dropped_reply_sender_fails_its_reply`). The other way
round, a caller that drops its reply does not stop the work: the
`ReplySender` still pushes and ends it, and nobody takes the events (test
`a_dropped_reply_leaves_its_reply_sender_free_to_push_and_end`). Dropping
a placed executor cancels its calls not yet started: their replies end
`ReplyStatus::Cancelled`, and the call running finishes (tests
`dropped_it_cancels_the_calls_not_yet_started` and
`dropped_it_lets_the_call_running_finish`, `src/tests/placement.rs`). A
call on several shots, each on an executor of its own, ends `Failed` when
a shot failed and another was cancelled, whatever the shots' order: a
failure is news the caller must report, where a cancellation only says
that a shot never ran (test `a_failed_shot_outranks_a_cancelled_one`,
`src/tests/executor_fn/one_per_shot/placed.rs`). The end is a variant, not
a reason of `Failed`, because any executor could fail with the same text
and the compiler cannot check a match on a string; tokio
(`JoinError::is_cancelled`), Java (`CancellationException`) and Python
(`CancelledError`) type it too. `wait`, `wait_async` and `async_events`
give it as an error whose reason says the call was cancelled; a driver
that tells it from a failure polls the reply (test
`wait_gives_a_cancelled_reply_as_an_error`, `src/tests/reply.rs`).
Cancellation at an executor's `close` is planned, with no milestone yet,
for a caller that stops a run before its batches end: a user who
interrupts a long run from Python, or a driver that stops sampling once
its estimate is precise enough.

## A worker relays an inner reply: the sequence

An executor whose work runs on a worker thread can pass the reply of an
inner executor up with `relay`: it polls the inner reply with the worker's
own waker, appends each poll's events into the `ReplySender` at once, and
parks between polls; it ends the `ReplySender` as the inner reply ends:
done, failed or cancelled (test `relay_passes_a_cancellation_up`). Such an
executor is a pattern, not a type of this
crate. Test `relay_and_wait_pass_events_across_threads`
(`src/tests/reply.rs`) runs the `relay` → `wait` half, with a feeder in
place of the inner executor; the debugger's step
([guides](../guides.md#a-paused-debugger-the-step-completes-the-reply))
runs the whole shape.

```mermaid
sequenceDiagram
  participant D as driver
  participant X as executor
  participant W as worker thread
  participant T as inner executor
  participant R as inner reply
  participant C as ReplySender (worker's handle)
  participant I as IncrementalReply (caller's end)
  D->>X: call(batch)
  X->>X: reply_channel() → (ReplySender, IncrementalReply)
  X->>W: spawn(batch, ReplySender)
  X-->>D: Ok(IncrementalReply)
  D->>I: poll_events(out, waker)
  I-->>D: Pending (keeps the waker)
  W->>T: call(batch)
  T-->>W: Ok(inner reply)
  loop relay until the inner reply ends
    W->>R: poll_events(events, the worker's waker)
    R-->>W: events, status
    W->>C: append(events)
    opt nonempty events and a stored caller waker
      C--)D: wake (takes the waker)
      D->>I: poll_events(out, waker)
      I-->>D: events, Pending (keeps the waker)
    end
    alt status is Pending
      Note over W: parks until the inner reply wakes the worker
    else status is Done, Failed or Cancelled
      W->>C: done(), fail(reason) or its cancel
      opt a stored caller waker
        C--)D: wake (takes the waker)
      end
    end
  end
  D->>I: poll_events(out, waker)
  I-->>D: remaining events, Done, Failed or Cancelled
```

## The waking cycle, step by step

The cycle of [The waking model](../replies.md#the-waking-model), as
each side sees it:

| Step          | The executor (`ReplySender`)                     | The caller (`IncrementalReply`, through `wait` or a task) |
| ------------- | ------------------------------------------------ | --------------------------------------------------------- |
| 1. poll       | –                                                | takes the events held; `Pending`: leaves its waker        |
| 2. sleep      | runs; its pushes go into the reply               | parked, or its task suspended                             |
| 3. wake       | the first push (or the end) wakes the waker kept | –                                                         |
| 4. poll again | later pushes wake no one, until the next poll    | takes every event pushed since its last poll              |

**Why the caller polls, rather than being called back.** With a callback,
the executor would call a function of the caller with each event: the
caller would no longer choose when it takes the events, and a function
to be called from another thread must be `Send + 'static`. A waker is a
callback that carries no data: it says only that a poll would make
progress, and the events stay in the reply until the caller takes them.
Like a Rust `Stream`, a reply is pulled, and `async_events` makes it one,
each item the events one poll found; unlike a future or a promise (C++'s
`std::future`, Java's `Future`, a JavaScript promise), which settle once
with one value, it hands over many events, then one end, `Done` or
`Failed`, and the events before a failure stand.

## Only the sleep differs between the ways of taking a reply

Every way of taking a reply runs the cycle of
[The waking model](../replies.md#the-waking-model). They differ only in
step 2, what the caller does on `Pending`, and so in what its waker does
at step 3:

| The caller                                         | On `Pending` (step 2)                                             | Its waker, when woken (step 3)                 | What blocks                                        |
| -------------------------------------------------- | ----------------------------------------------------------------- | ---------------------------------------------- | -------------------------------------------------- |
| `wait`                                             | parks its thread                                                  | sets a flag and unparks the thread             | the caller's thread, which chose to wait           |
| `poll_events` with `Waker::noop()`                 | does its own work, and polls again when it chooses                | nothing                                        | nothing                                            |
| `poll_events` on several replies, a waker for each | parks, if no reply was woken                                      | marks its reply ready, and unparks the driver  | the driver's thread, only while none is ready      |
| `wait_async`, `async_events`                       | its task returns `Pending` to the runtime, which runs other tasks | the runtime queues the task to be polled again | nothing: the thread runs the runtime's other tasks |

The executor's side is the same in every row: the `ReplySender` pushes and,
if a waker is kept, wakes it. It does not know which row its caller is in.

```mermaid
sequenceDiagram
  participant D as caller
  participant I as IncrementalReply
  participant C as ReplySender (worker's handle)
  participant W as worker
  D->>I: poll_events(out, waker)
  I-->>D: the events so far, Pending (the waker is kept)
  alt wait
    Note over D: parks its thread
  else poll_events with Waker::noop()
    Note over D: does its own work
  else an async task
    Note over D: returns Pending to its runtime, which runs other tasks
  end
  W->>C: push(event 1)
  C--)D: waker.wake(), once (the noop waker does nothing)
  W->>C: push(event 2), done()
  Note over C: no waker is kept, so no wake: the events wait in the reply
  D->>I: poll_events(out, waker)
  I-->>D: events 1 and 2, Done
```

The noop row is the one caller that polls in a loop: it gives up being
woken, so it polls when its own work allows. That is its choice, as
parking is `wait`'s: neither is imposed by the reply.

## `wait` parks, and a wake before the park is not lost

`wait` polls with its thread's waker, which holds a flag and the thread.
On `Pending`, it parks while the flag is clear; the waker sets the flag,
then unparks. The flag is needed because the park token alone is not
enough: a poll that itself parks (a channel receive, a lock) can use the
token up (tests `wait_parks_until_woken_instead_of_spinning`,
`a_wake_during_the_poll_is_not_lost` and
`a_wake_from_another_thread_before_the_park_is_not_lost`,
`src/tests/reply.rs`).

Parking saves the waiting thread's CPU, and may add latency to each call
on a virtual machine, whose sleeping vCPU takes time to wake. Measured on
an earlier prototype of the protocol, not on this crate (two cores, WSL2):
with a thread per shot, the waiting thread's CPU time fell from 70–150 ms
when `wait` spun to 10 ms, while one call waited on a worker thread took
6–63 µs, against 9–23 µs spinning. Open, to be measured on bare metal:
whether `wait` should spin briefly before it parks.

```mermaid
sequenceDiagram
  participant D as wait (the caller's thread)
  participant P as its waker (flag, thread)
  participant I as IncrementalReply
  participant C as ReplySender (worker's handle)
  participant W as worker
  D->>I: poll_events(out, the waker)
  I-->>D: Pending (the waker is kept)
  alt the worker ends after the caller parks
    D->>P: park: the flag is clear, so thread::park()
    W->>C: push(events), done()
    C->>P: wake: set the flag, unpark
    P-->>D: park returns, and the flag is cleared
  else the worker ends before the caller parks
    W->>C: push(events), done()
    C->>P: wake: set the flag, unpark
    D->>P: park: the flag is set, so it returns at once
  end
  D->>I: poll_events(out, the waker)
  I-->>D: events, Done
```

## Several calls in flight

A driver keeps several replies, gives each its own waker, and sleeps until
one of them wakes it. Each waker records its call as ready and unparks the
driver, which polls only the calls that were woken. This is what an async
runtime does with many futures (test
`a_driver_keeps_several_calls_in_flight_and_sleeps_until_one_wakes_it`,
`src/tests/reply.rs`).

```mermaid
sequenceDiagram
  participant A as driver (one thread)
  participant X as call 1 (IncrementalReply)
  participant Y as call 2 (IncrementalReply)
  participant C1 as worker 1's ReplySender
  participant C2 as worker 2's ReplySender
  A->>X: poll_events(out, waker 1)
  X-->>A: Pending (keeps waker 1)
  A->>Y: poll_events(out, waker 2)
  Y-->>A: Pending (keeps waker 2)
  Note over A: nothing is ready: the thread parks
  C2->>C2: push(outcomes)
  C2--)A: waker 2: call 2 is ready, so unpark
  A->>Y: poll_events(out, waker 2)
  Y-->>A: outcomes, Pending
  Note over A: call 1 was not woken, so it is not polled
  C1->>C1: push(outcomes), done()
  C1--)A: waker 1: call 1 is ready, so unpark
  A->>X: poll_events(out, waker 1)
  X-->>A: outcomes, Done
  C2->>C2: done()
  C2--)A: waker 2: call 2 is ready, so unpark
  A->>Y: poll_events(out, waker 2)
  Y-->>A: Done
```

## An async task leaves its thread to other tasks

Two tasks share one thread under a one-thread runtime (`run_tasks`, in the
tests): task A awaits a call with `wait_async`, task B has work of its own
and yields after each step. A's `Pending` returns the thread to the
runtime, which runs B. The worker's push wakes A's waker, which queues A,
and the runtime polls A again the next time it picks a task (in the run
drawn, between two of B's steps). The thread parks only when no task is
queued (test
`an_async_task_awaiting_a_call_leaves_its_thread_to_other_tasks`,
`src/tests/reply/asynchronous.rs`).

```mermaid
sequenceDiagram
  participant R as runtime (one thread)
  participant A as task A, in wait_async
  participant B as task B
  participant I as IncrementalReply
  participant C as ReplySender (worker's thread)
  R->>A: poll(waker A)
  A->>I: poll_events(out, waker A)
  I-->>A: Pending (keeps waker A)
  A-->>R: Pending
  loop B's steps, while A waits
    R->>B: poll(waker B)
    B-->>R: Pending (B yields, and its waker queues it again)
  end
  C->>I: push(outcome)
  C--)R: waker A: queue A, unpark the thread
  C->>I: done(), no waker is kept, so no wake
  R->>B: poll(waker B)
  B-->>R: Pending (B yields, and its waker queues it again)
  R->>A: poll(waker A)
  A->>I: poll_events(out, waker A)
  I-->>A: outcome, Done
  A-->>R: Ready
  R->>B: poll(waker B), until B is done
```

## The tests drive futures with `block_on`

A `Future` or a `Stream` does nothing until something polls it, and gives
it a waker: an async runtime (Tokio, for one). The crate depends on no
runtime, so its tests bring two small ones:

- `block_on` (`src/tests.rs`) runs one future. It polls the future with a
  waker that unparks the test's thread, and parks the thread on `Pending`.
  With one task there is nothing else to run, so it parks, as `wait` does.
  `one_reply_is_taken_four_ways` uses it to show that the `Future` and the
  `Stream` hand over the same events as `wait` and `poll_events`, not that
  they leave the thread free.
- `run_tasks` (`src/tests.rs`) runs several tasks
  on one thread, and parks only when no task is ready.
  `an_async_task_awaiting_a_call_leaves_its_thread_to_other_tasks` uses it
  to show that a task awaiting a call leaves its thread to the others
  ([above](#an-async-task-leaves-its-thread-to-other-tasks)).

## The tests of the waking cycle

The tests of the cycle are in `src/tests/reply.rs`:

- `a_reply_sender_wakes_its_caller_once_per_pending_poll`;
- `a_poll_with_a_new_waker_replaces_the_old_one`;
- `a_poll_hands_over_every_event_pushed_since_the_last_one`;
- `an_empty_extend_or_append_does_not_wake`;
- `ending_a_reply_wakes_its_caller`;
- `a_reply_ended_before_its_first_poll_hands_over_everything_then`.

`wait_parks_until_woken_instead_of_spinning` (`src/tests/reply.rs`) shows
step 2: a parked thread does not spin. For an executor whose shots'
replies end after `call` returns (on threads of their own, say), in
`src/tests/shot_states.rs`,
`an_executor_hands_over_each_shots_events_as_they_come` shows a shot's
events handed over before the batch ends, while another shot still runs;
and `wait_on_an_executor_polls_at_most_twice_per_shot`, whose shots run on
threads of their own, shows that such an executor wakes its caller only
when a shot has something for it.
[`wait` parks](#wait-parks-and-a-wake-before-the-park-is-not-lost) draws `wait`
parking and being woken.

## Readout tags: the rules

`check_unique_tags` refuses a batch that repeats a tag with
`Rejected::Instruction` at the readout that repeats it
([Readout tags](../replies.md#readout-tags)). With several repeats, it
refuses the first, in the batch's order, as `Rejected::Instruction`
promises for each reason: a check made at each call stops at its first
fault. An executor that checks for
several reasons in turn, such as its set and then its tags, refuses at the
first fault of the first check that finds one.

The rule is pinned by the tests of `src/tests/outcome.rs`
(`a_readout_that_repeats_a_tag_is_refused_at_its_index`,
`a_tag_repeated_after_other_readouts_is_refused`,
`the_first_repeat_in_the_batchs_order_is_refused`,
`distinct_tags_are_accepted`,
`a_batch_of_at_most_one_readout_is_accepted`,
`an_instruction_that_repeats_a_tag_among_its_own_readouts_is_refused`,
`the_refusal_names_the_contract_and_the_planned_readout_register`), and
by two conformance checks. Both take `a_again`, another instruction than
`a` that reads under `a`'s tag, so that the rule shows to be about a
repeated tag, not a repeated instruction; and, where the executor has
another qubit than `a`'s, `a_again` reads it, so that the rule shows not
to be about a repeated qubit either. The checks know nothing of qubits,
so they cannot check that last need.
`refuses_a_tag_repeated_within_a_batch`, with two instructions `a` and `b`
that each read under a tag of its own, asks that a tag used again in a
later batch is accepted, and that each of `[a, b, a]`, `[b, a, a]`,
`[a, a, b]`, `[a, b, b, a]`, `[b, a, a, b]`, `[a, a_again]` and
`[a, b, a_again]` is refused at the first instruction that repeats a tag:
a repeat of the first tag, of the one just before, in the middle, the
first of two repeats, whichever its tag, and a repeat by another
instruction. And `refuses_a_repeated_tag_at_the_index_of_its_instruction`,
for a set that also has an instruction `silent` that makes no readout,
asks that `[silent, a, silent, a]` and `[silent, a, silent, a_again]` are
refused at the instruction's index (3), not the readout's (1), nor its
index counted from the first readout (2). Each check first asks that its
instructions make the readouts it relies on, and that `a_again` is not
`a`. A case that
neither check sends: an instruction that repeats a tag among its own
readouts, which their instructions, of one readout each, cannot make.
Both run on the toy and the
static code (`the_toy_and_the_code_refuse_a_tag_repeated_within_a_batch`),
on the described toy and the run-time code
(`the_device_and_the_code_refuse_a_tag_repeated_within_a_batch`), and on
the profiler and an existing backend through the bridge
(`each_executor_refuses_a_tag_repeated_within_a_batch`).

**What it costs.** One sort of the batch's readouts by tag: about 2 ns for
a batch of one readout (no allocation), and 1.5 to 13 ns per readout for
ten thousand, by how scrambled the tags are
(`cargo bench -p executor_protocol --bench readout_tags`). A runtime that
sends a batch at each measurement pays the first.

## Shot conditions: the rules

Each promise of [Shot conditions](../replies.md#shot-conditions), and of
the API docs of `ShotConditions`, in the page's order, with the tests that
pin it. They are in `src/tests/conditions.rs` unless named elsewhere; those
of the conformance kit's checks, which show that each check fails an
executor that breaks its rule, are in `src/tests/conformance.rs`.

| Promise                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | Tests                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ShotConditions::REJECTED`, `INCONSISTENT` and `UNRESOLVED` are the three conditions, `NONE` holds none, and `\|` gives the union of two sets                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            | `conditions_print_as_their_flags`, `the_conditions_a_logic_raises_twice_in_a_call_merge_by_union`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| A caller tests a shot's conditions with `ShotConditions::contains`, which holds only when they include every condition of the set it is given                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            | `a_shots_conditions_contain_a_set_only_with_every_condition_of_it`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `ShotConditions`' `Debug` names its flags (API docs)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     | `conditions_print_as_their_flags`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| An outcome is no larger for carrying its shot's conditions                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               | `an_outcome_is_no_larger_for_carrying_its_shots_conditions` (`src/tests/outcome.rs`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| A condition raised in a call that gives the shot no outcome reaches no caller in that call                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               | `a_condition_raised_below_in_a_call_with_no_readouts_reaches_no_caller`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| The kit's check across calls fails an executor that drops a condition on an earlier outcome of a shot, forgets one at a call, or marks what the caller held; it accepts one raised between two readouts of a call, takes the calls in the order they ran whatever their tags, and refuses batches that give no outcome a condition before a later call reads that shot                                                                                                                                                                                                                                                                                                   | `keeps_each_shots_conditions_from_one_call_to_the_next_catches_a_condition_dropped_on_an_earlier_outcome`, `keeps_each_shots_conditions_from_one_call_to_the_next_catches_conditions_forgotten_at_a_call`, `keeps_each_shots_conditions_from_one_call_to_the_next_catches_a_reply_that_marks_what_was_held`, `keeps_each_shots_conditions_from_one_call_to_the_next_lets_a_call_raise_one_between_readouts`, `keeps_each_shots_conditions_from_one_call_to_the_next_takes_the_calls_in_their_order_whatever_their_tags`, `keeps_each_shots_conditions_from_one_call_to_the_next_needs_a_condition_to_keep` (`src/tests/conformance.rs`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| The kit's check across calls fails an executor when one outcome of a call drops a condition while another outcome of that call carries it, whatever their order                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | `keeps_each_shots_conditions_from_one_call_to_the_next_catches_a_condition_one_outcome_of_a_call_drops_while_another_carries_it` (`src/tests/conformance.rs`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| The kit's checks of a shot alone, of shots a call does not name and of a fresh executor compare conditions as they compare values                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        | `gives_each_shot_alone_what_it_gives_among_others_catches_a_condition_raised_on_other_shots`, `changes_no_shot_a_call_does_not_name_catches_a_condition_raised_on_held_shots`, `starts_its_shots_as_made_whatever_an_earlier_executor_left_catches_conditions_left_behind` (`src/tests/conformance.rs`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `with_conditions` gives an outcome those conditions in place of its own                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | `an_outcome_given_conditions_carries_them_in_place_of_its_own` (`src/tests/outcome.rs`), `the_conditions_a_logic_writes_on_an_outcome_for_its_shot_reach_its_caller`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `Outcomes::raise_conditions` adds its conditions to every outcome the logic writes for those shots in the call, before as well as after, and to no other shot's                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | `a_condition_a_logic_raises_marks_every_outcome_it_writes_in_the_call`, `a_condition_a_logic_raises_marks_the_outcomes_it_writes_for_their_shots`, `an_outcome_written_with_a_condition_keeps_it_when_the_logic_raises_another`, `the_conditions_a_logic_raises_twice_in_a_call_merge_by_union`, `a_condition_raised_on_one_shot_marks_no_other_shots_outcomes`, `every_outcome_a_logic_writes_for_its_shot_at_once_carries_the_conditions_it_raised`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| A call an `executor_fn`'s logic fails hands over the outcomes the logic wrote before its `Err` with the conditions written on them and raised for their shots, with one state and with a state per shot                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | `a_call_its_logic_fails_hands_over_what_it_wrote_with_the_conditions_written_and_raised`, `with_a_state_per_shot_a_call_a_shots_logic_fails_hands_over_what_each_shot_wrote_with_its_own_conditions`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| An `executor_fn` keeps no condition across calls                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         | `an_executor_fn_keeps_no_condition_from_call_to_call`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| The page's example: a runtime with a state per shot rejects its shot, then writes a readout for it, which its caller finds rejected                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      | `a_condition_a_logic_raises_marks_the_outcomes_it_writes_for_their_shots`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| An `adapter_fn` marks the shot's other outcomes in the call with a condition its logic writes on one                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     | `the_conditions_a_logic_writes_on_an_outcome_mark_the_shots_other_outcomes_in_the_call`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| An `adapter_fn` takes in the conditions its logic raises and those its logic writes, its runs read and its sends read in the call, while their reply is still pending too, merged by union, and marks with them the outcomes of the call's shots, in the call and in later calls on the shot, those made while a send's reply of the call is still pending too, however it then ends, whatever the order of the call's shots                                                                                                                                                                                                                                             | `a_condition_a_logic_raises_and_one_from_below_on_the_same_shot_merge_by_union`, `a_condition_a_logic_raises_and_one_from_below_on_the_same_shot_both_mark_the_shots_outcomes_in_later_calls`, `the_conditions_raised_below_reach_the_caller_through_an_adapter_that_sends`, `a_condition_a_send_reads_marks_the_outcomes_the_logic_writes_for_the_shot_in_the_call`, `a_condition_a_send_reads_as_its_call_ends_marks_the_shots_outcomes_in_later_calls`, `a_decoders_outcomes_carry_the_conditions_of_its_runs_with_no_code_in_its_logic`, `an_owning_decoders_outcomes_carry_the_conditions_of_its_runs`, `the_conditions_of_several_runs_merge_by_union_shot_by_shot`, `the_conditions_a_logic_writes_on_an_outcome_mark_the_shots_outcomes_in_later_calls`, `a_condition_raised_in_a_call_with_no_readouts_marks_the_shots_outcomes_in_later_calls`, `the_conditions_a_logic_raises_twice_in_a_call_with_no_readouts_both_mark_later_outcomes`, `a_decoders_outcomes_carry_their_own_shots_conditions_whatever_the_order_of_its_shots`, `with_several_shots_flagged_each_outcome_of_a_shot_carries_its_conditions`, `a_condition_raised_in_a_call_whose_send_ends_after_it_marks_the_shots_outcomes_in_later_calls`, `a_condition_a_send_reads_at_the_poll_where_its_reply_ends_marks_the_outcomes_the_logic_wrote_however_it_ends`, `a_condition_a_send_reads_while_its_reply_is_pending_in_the_call_marks_the_shots_outcomes_in_later_calls`, `with_a_state_per_shot_the_conditions_each_pending_send_reads_as_the_call_ends_mark_its_shots_outcomes_in_later_calls` |
| Every outcome its sends give the call's shots carries the shot's conditions as the call that sent them ended, whenever and however they end, several of a shot at one poll too, merged by union with those from below                                                                                                                                                                                                                                                                                                                                                                                                                                                    | `a_condition_a_logic_raises_marks_the_outcomes_of_its_sends`, `a_condition_a_logic_raises_marks_its_shots_outcomes_of_its_sends_ending_later`, `a_condition_a_logic_raises_and_one_from_below_merge_by_union_on_its_sends_ending_later`, `a_sends_outcomes_handed_over_while_its_reply_is_pending_carry_the_shots_conditions`, `every_outcome_a_send_hands_over_after_its_call_carries_the_shots_conditions_however_it_ends`, `with_a_state_per_shot_every_outcome_a_send_hands_over_after_its_call_carries_its_own_shots_conditions_however_it_ends`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| Its reply marks its own outcomes alone, leaving those its caller held as they were                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       | `an_adapters_reply_leaves_the_outcomes_its_caller_held_unmarked`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `raise_conditions` raises on the logic's own shot alone with a state per shot, and on every shot of the call with one state                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              | `with_a_state_per_shot_a_condition_a_logic_raises_marks_its_own_shots_outcomes_alone`, `a_condition_a_logic_with_one_state_raises_marks_every_shot_of_the_call`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| A refused call raises nothing; a call that fails or is cancelled, by its logic, a run, a send in the call or after it, or a call below refused after another went down, still marks every outcome its reply hands over with the conditions it took in                                                                                                                                                                                                                                                                                                                                                                                                                    | `a_refused_call_raises_nothing`, `a_failed_calls_outcomes_carry_the_conditions_of_its_runs`, `with_one_state_a_failed_calls_outcomes_carry_the_conditions_its_logic_raised`, `with_a_state_per_shot_a_failed_calls_outcomes_carry_the_conditions_its_logic_raised`, `a_reply_failed_by_a_send_after_its_call_marks_what_it_hands_over`, `a_condition_a_send_reads_at_the_poll_where_its_reply_ends_marks_the_outcomes_the_logic_wrote_however_it_ends`, `however_a_call_ends_unless_refused_it_marks_what_it_hands_over_with_the_conditions_its_logic_raised_and_its_sends_read`, `however_a_call_ends_unless_refused_it_marks_what_it_hands_over_with_the_conditions_its_runs_read_and_its_logic_wrote`, `with_a_state_per_shot_however_a_call_fails_or_is_cancelled_below_it_marks_what_it_hands_over_with_the_conditions_its_logic_raised`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| A run's conditions are taken in, also when it follows a send whose reply is still pending or was cancelled, and even when it fails or is cancelled, from what it read before, a run that reads a tag of a shot twice too, on the shots it was given alone, whatever their order: a failed call's reply marks the outcomes the logic wrote with them, and none with a condition read on a shot the run was not given; a send's, read during the call, are taken in on whatever shot each names, given to the send or not; a readout of a shot the call was not given, which only a faulty executor below gives, passes up with the conditions it carries from below alone | `a_condition_a_failed_run_read_marks_the_outcomes_the_logic_wrote_in_the_call`, `a_condition_a_cancelled_run_read_marks_the_outcomes_the_logic_wrote_in_the_call`, `a_failed_run_takes_in_no_condition_it_read_on_a_shot_it_was_not_given`, `a_failed_run_takes_in_the_conditions_it_read_on_each_of_its_shots_whatever_their_order`, `a_condition_a_send_reads_on_a_shot_it_was_not_given_marks_that_shots_outcomes_in_later_calls`, `a_sends_readout_of_a_shot_the_call_was_not_given_passes_up_with_its_own_conditions_alone`, `a_condition_a_run_reads_after_a_send_pending_or_cancelled_marks_the_outcomes_the_logic_wrote_however_the_run_ends`, `a_run_that_reads_a_tag_of_a_shot_twice_takes_in_the_conditions_it_read_on_each_of_its_shots`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| The adapter passes the kit's check across calls, with one state, decoding or sending, and with a state per shot, over an executor below whose replies end within their call; and over one whose replies end after it when that executor keeps its shots' conditions                                                                                                                                                                                                                                                                                                                                                                                                      | `a_decoder_keeps_each_shots_conditions_from_one_call_to_the_next`, `an_adapter_that_sends_keeps_each_shots_conditions_from_one_call_to_the_next`, `with_a_state_per_shot_an_adapter_keeps_each_shots_conditions_from_one_call_to_the_next`, `an_adapter_whose_sends_end_after_their_call_keeps_each_shots_conditions_over_an_executor_that_keeps_them`, `with_a_state_per_shot_an_adapter_whose_sends_end_after_their_call_keeps_each_shots_conditions_over_an_executor_that_keeps_them`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| An adapter written by hand carries conditions by itself: the kit checks that it keeps, from one call to the next, the conditions that an earlier outcome of each shot carried                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            | `keeps_each_shots_conditions_from_one_call_to_the_next_catches_conditions_forgotten_at_a_call` (`src/tests/conformance.rs`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| Requirement: an executor that offers `resumable_shots` keeps each shot's conditions from one call to the next, which the kit's check across calls checks at run time for the conditions that an earlier outcome of the shot carried, failing one that forgets them at a call; it sees outcomes alone, and refuses batches that give no shot an outcome carrying a condition before a later call reads that shot                                                                                                                                                                                                                                                          | `keeps_each_shots_conditions_from_one_call_to_the_next_catches_conditions_forgotten_at_a_call`, `keeps_each_shots_conditions_from_one_call_to_the_next_catches_a_condition_dropped_on_an_earlier_outcome`, `keeps_each_shots_conditions_from_one_call_to_the_next_needs_a_condition_to_keep` (`src/tests/conformance.rs`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| An `executor_fn` offers `resumable_shots` when the capabilities it is given do, though it keeps no condition across calls by itself                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      | `describes_the_promise_as_given` (`src/tests/executor_fn/one_per_shot.rs`), `an_executor_fn_keeps_no_condition_from_call_to_call`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| An `executor_fn` whose logic keeps its shots' conditions in its state and raises them again in each call keeps them from one call to the next, by the kit's check                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        | `an_executor_fn_whose_logic_keeps_its_shots_conditions_in_its_state_keeps_them_from_one_call_to_the_next`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| Over an executor that breaks the requirement, an adapter can lose a condition a send gave after its call returned: the send's outcomes pass up with it (the test's first readout), the adapter never sees it, and the shot's later outcomes lack it                                                                                                                                                                                                                                                                                                                                                                                                                      | `an_adapter_whose_sends_end_after_their_call_loses_a_condition_over_an_executor_that_forgets_it`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| Over an executor that meets it, the shot's outcomes that a later call sends for carry the condition again: a send's outcomes pass up with the conditions from below, whenever they end                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   | `an_adapter_whose_sends_end_after_their_call_keeps_each_shots_conditions_over_an_executor_that_keeps_them`, `with_a_state_per_shot_an_adapter_whose_sends_end_after_their_call_keeps_each_shots_conditions_over_an_executor_that_keeps_them`, `a_condition_a_logic_raises_and_one_from_below_merge_by_union_on_its_sends_ending_later`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| Over an executor that meets it, so do the outcomes the logic writes for the shot in a later call that reads an outcome of the shot from below before it ends, whether the logic writes them before or after the read, and in every call after it: the adapter takes the condition in from that outcome, keeps it, and marks with it, as each call ends, what the logic wrote in the call, even in a later call that reads nothing                                                                                                                                                                                                                                        | `a_logic_that_writes_a_shots_outcome_after_a_run_below_reads_it_keeps_a_condition_a_send_gave_after_its_call`, `a_logic_that_writes_a_shots_outcome_before_a_run_below_reads_it_in_the_call_keeps_a_condition_a_send_gave_after_its_call`, `a_condition_a_send_reads_marks_the_outcomes_the_logic_writes_for_the_shot_in_the_call`, `a_condition_an_earlier_calls_run_read_marks_what_the_logic_writes_in_a_later_call_with_no_call_below`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| Known issue: a condition an earlier send gave after its call returned reaches the adapter only through an outcome of the shot that one of its calls reads from below before that call ends, so in the calls before that one the outcomes the logic writes for the shot lack it, while it reaches the caller in that send's outcomes (each test's first readout), even over an executor that keeps its shots' conditions: in a later call with no call below, in one whose run reads nothing for the shot, and in one whose send for the shot gives its outcomes after it returned                                                                                        | `a_logic_that_writes_a_shots_outcome_with_no_call_below_loses_a_condition_a_send_gave_after_its_call`, `a_logic_that_writes_a_shots_outcome_after_a_run_below_that_reads_none_of_it_loses_a_condition_a_send_gave_after_its_call`, `a_logic_that_writes_a_shots_outcome_in_a_call_whose_send_ends_after_it_loses_a_condition_a_send_gave_after_its_call`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| Over a link, a shot's conditions travel as one fixed code of three bits, and a code with an unknown bit decodes to nothing                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               | `conditions_codes_are_fixed_and_decode_back`, `a_conditions_code_with_an_unknown_bit_decodes_to_nothing` (`src/tests/codes.rs`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |

## The tests of the user page's promises

Each promise of [Replies](../replies.md) that the sections above do not pin, in
the page's order, with the tests that pin it.

| Promise                                                                                                                     | Tests                                                                                                             |
| --------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| Polling: a driver with `Waker::noop()` does its own work between polls                                                      | `a_driver_with_a_noop_waker_does_its_own_work_between_polls` (`src/tests/reply.rs`)                               |
| Async: `wait_async` is a `Future` of the whole reply                                                                        | `wait_async_is_a_future_of_the_whole_reply` (`src/tests/reply/asynchronous.rs`)                                   |
| Stream: `async_events` gives each batch of events as it comes                                                               | `async_events_gives_each_batch_of_events_as_it_comes` (`src/tests/reply/asynchronous.rs`)                         |
| An adapter whose logic waits below hands over nothing before that logic returns, not even what its sends below already read | `a_call_returns_only_once_its_logic_returns_though_a_send_below_already_read` (`src/tests/adapter/adapter_fn.rs`) |
