# qodec_demo

> **An example, not a library.** This branch shows the executor protocol
> (`source/executor_protocol`) running the Qodec runtime's code layers as
> adapters, end to end. Nothing here is meant to be used or maintained as is:
> read it as a worked example of the protocol on a real case.
> `source/qodec_runtime` and `source/qodec_lowerer` are unchanged copies of
> the Qodec runtime prototype and its lowerer at commit `564b78cf4` of the
> `oscarpuente/qodec-runtime` branch. The demo's own
> code is `source/qodec_demo`. The questions it leaves open for the protocol
> are in `docs/open-questions.md`.

## The problem

The Qodec runtime (`qodec_runtime`) runs a program of logical instructions
through a stack of code layers. Each layer lowers its calls into the
instructions of the layer below, decodes what comes back, and owns the
layer below it. The executor protocol (`executor_protocol`) composes a
driver, adapters and an executor into a chain with `>>`. A Qodec layer
should be one of its adapters, unchanged, and so should a profiler, above
the layer or below it. The chain should give the stack's own outcomes,
over a backend that answers within its call or later.

This crate checks that. It is a test bed: each protocol question about a
real code runtime gets answered here, with a test. Where the protocol falls
short, the crate records a finding: an ignored test that states what should
hold, with the reason in its `#[ignore]`.

## The model

```text
static_driver() >> qodec layer >> ... >> profiler() >> backend
```

- `static_driver()`, the protocol's driver, heads the chain: it runs the
  program given to the chain (`chain.run(program, shots)`), sending the
  program's batches as calls to what is below it.
- A backend ends it: `coin_device` (a coin per readout, `src/coin.rs`) or
  `simulator` (noisy classical bits, `src/noisy_bits.rs`). Each also has a
  form that serves as the runtime's own bottom backend, so the chain and
  the runtime's stack (`src/stack.rs`) run on the same bits.
- Between them, `QodecLayer::new(&qodec, k)?.adapter()` is layer `k` of a
  qodec as an adapter (`src/qodec_layer.rs`). It keeps one Qodec runtime
  per shot. Each shot's runtime owns its way below (`BelowHandle`) and calls
  it as often as it needs: in c4c6, 14 bottom batches for one top batch.
- `profiler()` counts the batches and instructions that pass through it
  (`src/profiler.rs`). Below the layers, it counts the physical cost of a
  program. It keeps a state per shot, so it counts a call on four shots as
  four batches, and cannot see how the shots are grouped into calls. To
  count the calls a device gets, put `call_counter()` above the device: it
  keeps one state, which sees each call whole, and sends each call below
  as it is (`src/call_counter.rs`;
  [The profiler](docs/findings.md#the-profiler)).
- Two wrappers change when any executor answers:
  - `AnswersLater` makes the executor's replies ready only at a later poll
    (`src/answers_later.rs`). Its lateness is one poll, which an adapter
    that polls its sends at once uses up.
  - `AnswersOnYield` answers the executor's replies from another thread,
    as a remote device's are, once the run above yields or after a
    timeout, which it counts (`src/answers_on_yield.rs`): a reply answered
    by its timeout was waited for on the thread that polls the run, unless
    that thread was not scheduled for the whole timeout. The run is polled
    again only once woken, so a wake lost while the run is yielded fails
    the run's await; one lost while a Qodec layer waits inside the poll
    hangs that poll, so the tests over a layer bound their own time.

The users' chain, the body of one test in
[`tests/profiler.rs`](tests/profiler.rs):

```rust
let qodec = c4c6();
let top = QodecLayer::new(&qodec, 0).expect("c4c6 plans");
let middle = QodecLayer::new(&qodec, 1).expect("c4c6 plans");
let coins = coin_device(bottom(&qodec).set.clone(), SEED);
let chain = (static_driver() >> top.adapter() >> middle.adapter() >> profiler() >> coins)
    .expect("every boundary matches");
let program = c4c6_one_batch_program(&qodec).to_static(Needs::default());
let shots: Vec<_> = CODE_SHOTS.collect();
let (outcomes, (profile, _)): (_, (Profile, _)) = chain
    .run(program, shot_set(&shots))
    .expect("the job fits the chain")
    .wait();
outcomes.expect("the run succeeds");

assert_eq!(profile.batches, C4C6_BOTTOM_BATCHES * shots.len() as u64);
```

## How to run it

The `qodec` library comes from crates.io (0.2.1). The tests load
repetition3 from [`tests/data`](tests/data/repetition3.qodec.yaml), a
copy of the bundle the library's repository ships (`microsoft/qdk-ec` at
commit `50f3bff`, `qodec/examples/repetition3`), kept unchanged since its
gadgets declare the checks the decoder conditions outcomes on, which the
lowerer's copy lacks; and c422 and c4c6 from the lowerer's examples
(`source/qodec_lowerer/examples`).

```sh
cargo test -p qodec_demo                       # the behaviours
cargo test -p qodec_demo -- --ignored          # the findings: these fail
cargo run --release -p qodec_demo --example lockstep [shots...]
cargo run --release -p qodec_demo --example latency [ms...]
cargo run --release -p qodec_demo --example throughput [shots...]
```

Each example first checks its runs against a reference, then reports its
numbers ([Measurements](#measurements)):

- `lockstep`, that on c422 and c4c6 the chain sends each shot's bottom
  batches and gives the outcomes of the Qodec runtime's own stack, and,
  on every case, that its counts bound each other (a merge makes no more
  calls than today, and a call holds at least one batch);
- `latency`, that each way gives over the slow device the outcomes and
  calls it gives over coins that answer within their call, and that a
  chain a shot gives the outcomes of the one chain;
- `throughput`, that the chain and the profiled chain give the stack's
  outcomes and run as many bottom instructions.

## Measurements

Three examples measure the chain end to end. The numbers below are from
release builds on one WSL machine, two cores. Each range pools several
runs:

- latency: sixteen runs;
- throughput from 64 shots on: eighteen runs;
- throughput on fewer shots: the seven runs since each layer plans only
  itself (below);
- throughput on fewer shots before that change: thirteen runs.

Times vary by about 10% between runs, more on few shots, so a new run
can fall slightly outside these ranges. Call counts do not vary.

- **Lockstep merging** (`examples/lockstep.rs`, Q67, run with no
  arguments: 4, 16, 64 and 256 shots): the device calls today against an
  ideal lockstep merge, which would send each step's equal bottom batches
  of all the shots in one call.
  - On c422 and repetition3, every shot sends the same 3 batches: today
    takes `1 + 2N` calls on `N` shots, lockstep 3 (171× fewer on 256).
  - On c4c6, the batches differ in the bits that the measurement-based
    preparation feeds forward (`X_if`, `Z_if`): its ancillas' readouts,
    random even on a noiseless device, so the coins are the realistic case
    (the example's `p=0.01` and `p=0` rows only bound it). Over coins,
    today takes 69 calls a shot, lockstep 204 on 4 shots (1.35× fewer) and
    399 on 256 (44×). A device that took those bits per shot could merge
    every step: 69 calls on any number of shots.
  - Each shot of c4c6 takes 69 steps. A lockstep merge whose step's
    distinct batches went below together would make 69 round trips one
    after another on any number of shots, against 69 a shot today, the
    shots served one after another. On a device bound by its latency, its
    gain in wall time would be `N`-fold, beyond its ratio of calls: on 4
    shots of the latency example, way `bare` makes 69 calls one after
    another, and way `shots` 276, 69 one after another on each of the 4
    shots' threads, the threads running together; each way takes about
    one shot's time.
- **Latency** (`examples/latency.rs`, Q65, run as `latency 0.1 1 10`): a
  run awaited over a device that answers each call after a latency, from
  threads of its own. The figures below are at 10 ms. At 0.1 and 1 ms, on
  4 shots, `shots` and `bare` again take less time than `layer` and
  `thread`; ways whose times are close, such as `layer` and `thread`, swap
  order between runs.
  - Wall time is about the latency times the calls made one after
    another, on every way.
  - With a Qodec layer (way `layer`), the run holds the thread that polls
    it for 67–90% of the run on c422, and all of it, in one poll, on c4c6.
    Built and run on a thread of its own (way `thread`), it leaves the
    caller's thread free, at the same wall time: freeing the thread gains
    no wall time.
  - The time goes on the shots, served one after another: c4c6 on 4 shots
    takes 2.9–3.0 s, 4 times its 0.71–0.76 s on 1 shot, against
    0.71–0.76 s for shot 0's bottom batches, run on the 4 shots with no
    layer (way `bare`).
  - Overlapping the shots' reads recovers it: a chain a shot, each on a
    thread of its own (way `shots`), takes 0.71–0.76 s on c4c6, with as
    many device calls as the one chain (276), and on c422 12 calls against
    its 9. The 69 calls of way `bare` on c4c6 are a bound that no layer
    reaches today.
- **Throughput** (`examples/throughput.rs`, run as
  `throughput 1 4 16 64 256 1024`): the time per bottom instruction over
  coins that answer within their call, against the Qodec runtime's own
  stack.
  - From 64 to 1024 shots, the chain takes 0.98–1.18 times the stack's
    time, and with the profiler 1.03–1.19 times.
  - On 1 shot, the chain takes 1.00–1.09 times the stack's time on c422
    and 1.05–1.11 times on c4c6, with or without the profiler. Each run
    builds what it runs: the chain's layers (about 0.4 ms on c422, 0.6 ms
    on c4c6), each planning its own layer and its decoder, and the stack,
    which plans the same.
  - Planning only the layer is a gain on few shots: on 1 shot of c4c6,
    the chain now takes about 1.06 times the stack's time, against about
    1.6 before (1.54–1.70; on c422 1.06–1.18), and building c4c6's layers
    took 1.4–1.8 ms. Each layer then planned the whole qodec, every layer
    and decoder, and then its own decoder again, as the Qodec prototype's
    did: its findings note that it "plans twice", the runtime keeping a
    plan's decoder plans private. The runtime's `LayerPlan` plans one
    layer, as the layer now does.
  - On 4 and 16 shots, the chain takes 1.01–1.18 times the stack's time,
    with or without the profiler (1.05–1.30 before that change).
  - The chain converts each call's instructions to and from the runtime's
    and checks them against the sets (`[GLUE]`, `[PLUMBING:checks]`), as
    the Qodec prototype's did. There, passing the runtime's own
    instructions through the protocol as typed instructions removed half
    (c422) to four fifths (c4c6) of the cycle overhead over the runtime's
    stack, which profiling put mostly on those conversions, checks and
    their allocations, bringing the chain from 1.11–1.17 to 1.04–1.07
    times the stack's time. As a facility, the prototype proposed (not
    built) a typed boundary, for an adapter whose sets above and below
    are one type, with a plan identity checked once at build in place of
    the per-call checks. That experiment is not ported: the runtime's
    instruction carries an opcode in one layer's numbering, so by the
    protocol's rule it may not be a `TypedInstruction`.

## What it shows

Each behaviour below is written out, with the tests that pin it, in
[docs/findings.md](docs/findings.md#what-it-shows). The ids QD1 to QD6,
Q65 and Q67 to Q69 name the [open questions](#open-questions).

- [The chain gives the stack's
  outcomes](docs/findings.md#the-chain-gives-the-stacks-outcomes) on
  repetition3, c422 and c4c6, and c4c6's bottom batches in the stack's
  order.
- [The chain gives the stack's
  statuses](docs/findings.md#the-chain-gives-the-stacks-statuses): each
  outcome carries its result's status as the shot's conditions, the
  status from below included, with no code in the layer for it (QD5,
  built).
- [Composition and
  determinism](docs/findings.md#composition-and-determinism): `>>` is
  associative, a run awaited gives the same run as waited, and runs
  awaited together each give what they give alone.
- [The profiler](docs/findings.md#the-profiler) counts the instructions of
  its place (below a layer, the gadgets'; above it, the program's) and
  each shot's batches, and changes no outcome.
- [Calls below a layer](docs/findings.md#calls-below-a-layer): a batch
  that reads goes below on its shot alone, and an upper call whose
  lowering reads nothing goes below on its shots at once.
- [A backend that answers
  later](docs/findings.md#a-backend-that-answers-later) gives the same
  run; the layer blocks while it waits for a read (QD1, mode (c)), and a
  chain with a Qodec layer cannot move to another thread.
- [A device that fails](docs/findings.md#a-device-that-fails) fails the
  run, naming its reason, and a later call is refused, naming the failure.
- [Close and drop](docs/findings.md#close-and-drop): dropped, every reply
  ends with no panic and no hang; closed, every reply ends done and the
  backend is closed once.
- [Instruction sets](docs/findings.md#instruction-sets): the chain matches
  the layer's sets by mnemonic, each argument is sent as its parameter
  declares, and a set the protocol cannot express is refused, naming each
  fault.
- [Qubits](docs/findings.md#qubits): `QodecLayer::within` refuses a call
  on a block beyond the qubits below, before the runtime runs.
- [Refusals and failures](docs/findings.md#refusals-and-failures): what
  `QodecLayer::new` refuses, and when the layer refuses or fails a call.
- [An adapter that reads
  below](docs/findings.md#an-adapter-that-reads-below) has two ways today:
  `run`, which holds the thread that called it, or a reply written by hand
  (Q69).
- [The protocol's conformance
  checks](docs/findings.md#the-protocols-conformance-checks) pass on the
  coins and on a layer over them; they found that the coins took a
  repeated tag, which they now refuse.

To write a layer, start from `src/qodec_layer.rs` and its markers (below).
To write a profiler, start from `src/profiler.rs` (a state per shot, an
adapter), `src/call_counter.rs` (one state) or `src/coin.rs` (a terminal
that counts what it runs, QEP's shape). A profiler that only sends below
keeps the chain's non-synchronous path, and sends each call below as it
is ([The profiler](docs/findings.md#the-profiler)).

The layer's code is marked by what it is, with the names of the Qodec
prototype's facility candidates:

- `[QODEC]` marks the runtime's own logic;
- `[GLUE]` marks the conversions between the runtime's types and the
  protocol's (`src/boundary.rs`);
- `[PLUMBING:<name>]` marks what any adapter needs and the protocol does
  not yet give;
- `[FACILITY:<name>]` marks where the protocol's adapter facility now gives
  what the prototype wrote as `[PLUMBING:<name>]`; the code there is the
  layer's own use of the facility.

The names are the prototype's, but for three differences. Its
`[PLUMBING:errors]` marked the mapping of the runtime's errors to a refusal
or a failure, which an adapter's logic cannot make; here it marks
`LowerError`. Its `[PLUMBING:down]` is `[GLUE]` here, since the facility
renumbers. Its check of one shot a call is gone: a state per shot serves
one shot by construction.

What is left for a runtime's authors to write, in code lines (neither blank
nor comment) of `src/qodec_layer.rs`, 265 in all, and `src/boundary.rs`, 172:

| What                                                                        | Lines |
| --------------------------------------------------------------------------- | ----- |
| `[QODEC]`: the runtime's plan, decoder, state and call                      | 14    |
| `[GLUE]`: the layer's conversions, besides `boundary.rs`                    | 12    |
| The layer's policy on the blocks below (`within`, QD4)                      | 32    |
| `[PLUMBING:map up]`: outcomes by tag to results in order, both ways         | 34    |
| `[PLUMBING:checks]`: the set and unique tags, at each call                  | 15    |
| `[PLUMBING:errors]`: `LowerError`                                           | 15    |
| The status: each result's, on its outcomes (QD5)                            | 1     |
| `[FACILITY:per-shot state]`: making a shot's state over its way below       | 12    |
| `[FACILITY:call and wait]`: sending or running a batch below                | 7     |
| `[FACILITY:lifecycle]`: admitting, failing and closing                      | 0     |
| `[FACILITY:numbering]`: the numbering below                                 | 0     |
| Assembly: the types, the builder, two spelled adapter types (28), accessors | 105   |
| Imports, and the unit tests' module                                         | 18    |

The builder's refusal of the bottom layer (6 lines), and its report of
every fault the build finds in one refusal (9 lines), are in the assembly.

The prototype's own layer (`layer.rs`) was 241 code lines. The facility
took about 73 of them: the per-shot bridge (`Apply`, 21); the numbering
and the adapter's build, but for its need of resumable shots and its offers,
which the facility does not take (a finding below) (30); the lifecycle
(admit, fail on a poisoned runtime, close: 14); and the call and wait (8).
That leaves about 168; this layer's 265 are about 97 more. Using the
facility takes 19 (the two non-zero `[FACILITY]` rows above, 12 + 7). The
prototype has no policy on the blocks below (32) and does not spell the
facility's adapter types (28: one state and a state per shot); it stops at
the first of the plan, the decoder and the sets that fails, where this
layer reports every fault (9); and it indexes past the bottom layer, which
this layer refuses (6). With the 19, that comes to 94 of the 97. The
other 3 or so are what the subtraction leaves, not a count: the other
rows of the table, which the prototype has too, differ from its code by
about that much in all.

## Findings

Each finding is pinned by an ignored test that states what should hold.
[docs/findings.md](docs/findings.md#findings) gives each with its tests,
and [docs/open-questions.md](docs/open-questions.md) the questions they
raise.

- [The layer cannot leave a backend that answers later
  free](docs/findings.md#the-layer-cannot-leave-a-backend-that-answers-later-free)
  ([QD1](docs/open-questions.md#qd1-calling-below-more-than-once-in-a-call)):
  awaited, the run holds the thread that polls it through each read below.
- [A call's shots are not read at
  once](docs/findings.md#a-calls-shots-are-not-read-at-once)
  ([QD2](docs/open-questions.md#qd2-shots-that-need-different-calls-below),
  Q67): below the layer, each read is on one shot.
- [A dropped chain reports a cancellation as a
  failure](docs/findings.md#a-dropped-chain-reports-a-cancellation-as-a-failure)
  (Q68): the adapter that gets a cancelled send fails, so every later
  reply ends failed.
- [A layer cannot state what it offers or
  needs](docs/findings.md#a-layer-cannot-state-what-it-offers-or-needs)
  ([QD4](docs/open-questions.md#qd4-what-a-layer-states-when-the-chain-is-built)):
  the adapter facility offers what is below unchanged and needs nothing of
  it.

Other frictions are marked `FRICTION` in the code:

- an executor of another crate writes its own `>>`
  (`src/answers_later.rs`);
- an adapter's logic can fail a call but not refuse it
  (`src/qodec_layer.rs`);
- the protocol's operands lack the runtime's `u64` blocks and its
  "unresolved" value (`src/boundary.rs`).

## Open questions

The design questions the demo's docs cite, each open but for QD5's
carrier, which is built (the protocol's `ShotConditions`). QD1–QD5 are written
out, with the cases that raise them and the ways out, in
[docs/open-questions.md](docs/open-questions.md):

- **QD1**: how an adapter calls below more than once in one call: (a) a
  resumable runtime, (b) an `async` one, or (c) one that blocks until each
  reply below ends, as the Qodec runtime does today
  ([QD1](docs/open-questions.md#qd1-calling-below-more-than-once-in-a-call)).
- **QD2**: how a layer serves shots that need different calls below
  ([QD2](docs/open-questions.md#qd2-shots-that-need-different-calls-below)).
- **QD3**: what keeps a shot's calls in order once several are in flight:
  a per-shot queue
  ([QD3](docs/open-questions.md#qd3-a-shots-calls-in-order-with-several-in-flight)).
- **QD4**: what a layer states when the chain is built: what it offers
  above, and what it needs below
  ([QD4](docs/open-questions.md#qd4-what-a-layer-states-when-the-chain-is-built)).
- **QD5**: how a status from below, a recoverable condition, and a failure
  reach the layer above; the status and the condition ride on each
  outcome, as the shot's conditions (built), and the failure side is open
  ([QD5](docs/open-questions.md#qd5-a-status-a-condition-and-a-failure-from-below)).
- **QD6**: the facility's names, and whether `>>` stays associative.
- **Q65**: what the layer's blocking (QD1, mode (c)) costs an async
  caller, and what to build to recover the wall time. Freeing the poll
  (QD1 modes (a), (b)) frees the thread but, the shots served one after
  another, gains no wall time; overlapping the shots' reads, or merging
  them in lockstep (Q67), recovers it. The latency example measures the
  ways ([Measurements](#measurements)); which to build first is open.
- **Q67**: whether a layer merges its shots' batches that read into one
  call below.
- **Q68**: whether a send cancelled below fails the adapter. Today it does,
  so every later reply ends failed; the executors' rule (Q42, decided:
  the protocol's `docs/executors.md`) is that the calls a dropped
  executor had not started end cancelled and fail no later call.
- **Q69**: whether `adapter_fn` and `owning_adapter_fn` give a way to map
  or observe the reply below as it passes up, keeping state as they do
  (beyond `MappedReply`'s stateless map), without `run`'s held thread
  and, with a state per shot, its calls on one shot.
