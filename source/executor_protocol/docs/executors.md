# Executors, capabilities and seeds

[Protocol overview](../README.md).

An implementer writes its own logic, the physics or a device's calls, and
the crate makes it an executor that keeps the protocol's rules. This page
gives what an executor offers and a program needs, the facility that makes
an executor (`executor_fn`), how it holds its shots' states (`.shots`),
where it runs (`.run_on`), an executor's life, an executor written by hand
(`ExecutorLifecycle`), and how shots are seeded.

## Capabilities and needs

A component says what it offers (`Describe::capabilities`), and a job's needs (a
`Needs`) are stated by an adaptive program (`AdaptiveProgram::needs`), or by its
driver for a program of another form (`Driver::needs`), as the crate's driver
states a `StaticProgram`'s. `compose` builds the chain from the executor
upstream; when a job is given to the chain, each need of its program is checked
against what the chain offers, before anything runs:

```mermaid
sequenceDiagram
  participant C as caller
  participant P as compose
  participant E as executor
  participant A as each adapter, from the executor upstream
  participant H as chain
  C->>P: driver() >> adapters >> executor
  P->>E: capabilities()
  E-->>P: Capabilities
  P->>A: build(boundary)
  A-->>P: what it offers upstream, and its own faults
  alt no fault
    P-->>C: Ok(chain): nothing has run
  else
    P-->>C: Err(BuildFaults): every fault, upstream first
  end
  C->>H: run(program, shots)
  H->>H: check each need of the program against what the chain offers
  alt every need met, no fault
    H-->>C: Ok(Run): nothing has run
  else
    H-->>C: Err(JobFaults): every fault
  end
```

Each need is a number, compared with what is offered: a need is met when
$\text{needed} \le \text{offered}$, with $\text{true} = 1$ and
$\text{false} = 0$. So a `false` need is always met, and a `true` need is
met only by a `true` capability. `UnmetNeed` names
the component, so the caller knows which one to change, and for
instructions lists each needed instruction that does not fit
(`mismatches`).

| Property          | Offered (`Capabilities`), default                    | Needed (`Needs`), default             | Checked                                                                                                                         |
| ----------------- | ---------------------------------------------------- | ------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `qubits`          | `Option<u32>`, `None`: no limit                      | `Option<u32>`, `None`                 | `admit`, 1st: when both are stated                                                                                              |
| `resumable_shots` | `bool`, `false`                                      | `bool`, `false`                       | `admit`, 2nd                                                                                                                    |
| `resets`          | `bool`, `false`                                      | `bool`, `false`                       | `admit`, 3rd                                                                                                                    |
| `replayable`      | `bool`, `false`                                      | `bool`, `false`                       | `admit`, 4th                                                                                                                    |
| `log_probability` | `bool`, `false`                                      | `bool`, `false`                       | `admit`, 5th                                                                                                                    |
| `instruction_set` | `Option<Arc<InstructionSet>>`, `None`: not described | `Option<Arc<InstructionSet>>`, `None` | `admit`, 6th: when needed; each needed instruction must [fit](instruction-sets.md#compatibility), so `None` offered refuses any |
| `max_shots`       | `Option<u64>`, `None`: no limit                      | the job's shots, implied              | `admit`, 7th: when a limit is stated                                                                                            |
| `rng_ids`         | `RngIds`, the last executor's id alone               | –                                     | when a chain is built: `Capabilities::drawing` (`RngInUse`)                                                                     |

The default offers nothing an executor does not state, so an executor
states what it offers, and one that forgets to state a capability is
refused a job that needs it, naming the need, rather than running it
wrong:

- `resumable_shots` defaults to `false`: an executor that keeps each
  shot's state between calls, so that a later call continues the shot
  where an earlier one left it, says so. A shot's conditions are part of
  that state: an executor that offers it keeps each shot's conditions from
  one call to the next ([Shot conditions](replies.md#shot-conditions)),
  which is checked only at run time, by the conformance check
  `keeps_each_shots_conditions_from_one_call_to_the_next`, for the
  conditions an earlier outcome of the shot carried
  ([Conformance checks](composition.md#conformance-checks)).
- `replayable` defaults to `false`: an executor whose shots each replay,
  giving what they gave whichever shots share their calls, says so.
- `resets` defaults to `false`: an executor that can prepare a qubit again
  during a shot says so.
- `log_probability` defaults to `false`: only an executor that computes each
  shot's log probability says so (a simulator), not hardware, nor a
  sampler that returns values only.
- `rng_ids` lists the ids of the components that draw random numbers at or
  downstream of a component. A component that draws is refused when it is built
  against one that already draws with its id, so each draws from a seed of
  its own (see [Seeds](#seeds-not-generators)).

A chain given a job reports every need of the job's program that the chain
lacks, each a `Rejected::Unmet` among the job's `JobFaults`. A
caller that holds an executor checks its needs itself with
`Capabilities::admit`, which refuses the first need unmet, in the order of the
table (`Rejected::Unmet`).

`Needs::default()` states nothing but the run's shots, held at once, that every
run implies. A driver may add needs it derives from the program: the crate's
driver adds `resumable_shots` for a `StaticProgram` with calls before its last
batch, whose later batches continue its shots. A program of `InstructionCall`s
must state its set, which numbers its calls ([Instruction
sets](internals/instruction-sets.md#compatibility-the-rules)): a chain refuses
the job of one that does not, as its need of an instruction set unmet;
`Capabilities::admit` does not yet, a known gap, since it does not know the
caller's instruction type. A program states a need only when it wants it
checked. From the [tests of `compose`](../src/tests/compose.rs):

```rust
let mut program = Program::flip_and_toss(bits_set());
program.needs = program.needs.with_qubits(3).with_log_probability(true);
let chain = (driver() >> TestDoubleExecutor::with_model(Bits::default(), 7))
    .expect("nothing between to refuse");
let Err(JobFaults(faults)) = chain.run(program, [0]) else {
    panic!("three qubits and the log probability on two bits must be refused");
};
let needs: Vec<Option<Need>> = faults
    .iter()
    .map(|fault| match fault {
        Rejected::Unmet(unmet) => Some(unmet.need),
        _ => None,
    })
    .collect();
assert_eq!(needs, [Some(Need::Qubits), Some(Need::LogProbability)]);
```

A property the table lacks is added to the protocol in five steps
([Adding a capability](internals/executors.md#adding-a-capability)). Two
are planned, with no milestone yet:

- `recovers`: a call that fails does not fail the executor, and the same
  call sent again runs;
- `event_probabilities`: each outcome carries its probability as it comes,
  where today each shot's log probability comes in the report, at `close`.

## Making an executor: `executor_fn`

`executor_fn(promise, make_state, logic)` makes an executor (`ExecutorFn`)
from the implementer's logic alone:

- `promise` is what the executor offers (`Capabilities`), described as given.
- `make_state` makes the state a shot's calls run on (a simulator's qubits) from
  the shot's context (`ShotContext`), at the first call that needs it. Its `Err`
  refuses the call, and a later call tries again (placed whole, the executor
  refuses every later call: [Where an executor
  runs](#where-an-executor-runs-run_on)).
- `logic` runs a call's instructions on the state, and pushes each readout to
  `Outcomes`. Its `Err` fails the call: the outcomes pushed before it are handed
  over, the reply ends with its reason, and the executor refuses every later
  call. It raises a shot's conditions on one readout
  (`Outcome::with_conditions`) or on every readout it writes in the call
  (`Outcomes::raise_conditions`); the executor keeps none across calls, so
  the logic of one that offers `resumable_shots` keeps them in its state and
  raises them again in each call, as the protocol requires
  ([Shot conditions](replies.md#shot-conditions)).

The closures need no type annotations, and a plain function is passed by name.
Methods shape the executor further:

- `.check(check)` refuses the calls `check` refuses, before the state is made
  and the logic runs, such as a call on a qubit beyond the device's.
- `.log_probability(read)` reads a state's log probability when the executor
  closes, into its report. It is the only way to promise `log_probability`.
- `.state_report(report)` reads what a state tracked across its calls (a peak, a
  count) when the executor closes, folded into the executor's report.
- `.seed(seed)` gives the executor the run's seed: shot `j`'s state is made from
  `ShotContext::new(seed, j)`. Without it, a state that draws panics, which
  serves a logic that draws nothing.
- `.shots(strategy)` and `.run_on(placement)`, in the next two sections.

Without a shot strategy, the executor holds one state, for one shot, so a job
of more shots is refused when given to the chain. A panic inside `call` reaches
the caller, and the executor refuses every later call. Its `drop` never panics,
and frees the state before the closures, as `close` does, so that a state whose
drop reaches its device through what a closure holds still finds it there; so
does `.shots(OnePerShot)`. Placed per shot (`.run_on(..).shots(..)`, below),
each shot's state is freed on its placement before that shot's copies of the
closures, but `drop` does not wait for the shots, as no placed executor's drop
waits for its placement (below), so the executor's own closures may be freed
first: call `close`, which waits, when a state's drop needs them.
What changes the promise, and the rules of panics,
drops, reports and the seed at their edges, are in [Making an executor: the
rules](internals/executors.md#making-an-executor-the-rules).

An executor of a simulator with a bounded state per shot; its implementer writes
only its state, its logic and how to read its log probability. From
`bond_limited_executor`
([`tests/per_shot_executor/bond_limited.rs`](../tests/per_shot_executor/bond_limited.rs)):

```rust
let promise = Capabilities::default()
    .with_qubits(QUBITS)
    .with_resumable_shots(true)
    .with_replayable(true)
    .with_max_shots(MAX_SHOTS);
executor_fn(
    promise,
    move |context: ShotContext| {
        if busy.take_if(|&mut j| j == context.shot()).is_some() {
            return Err(Rejected::Unavailable("the device is busy".into()));
        }
        Ok(BondLimitedState::new(context.seed()))
    },
    run_bond_limited,
)
.check(check)
.log_probability(|state| state.log_p)
```

Its logic, `run_bond_limited`, is given the shot's state, the call's
instructions and the reply's outcomes:

```rust
pub fn run_bond_limited(
    state: &mut BondLimitedState,
    input: &CallInput<'_, Op>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), String> {
```

It runs the instructions on the toy's bits, whose `apply` pushes each
readout under the tag its instruction names (`tests/toy/simulator.rs`):

```rust
Op::Measure { qubit, tag } => {
    outcomes.push(Outcome::new(tag, self.bits[qubit as usize].into()));
}
```

## How an executor holds its shots

How the shots of a run share work is a choice with different costs and
guarantees, so each choice is an executor of its own, and its callers do
not change. A choice that holds the states of an `executor_fn`'s logic is
a shot strategy (`ShotStrategy`), given to `.shots(strategy)`:

| How the shots run                                            | Made by                                             | Replayable        | Status                                                                                              |
| ------------------------------------------------------------ | --------------------------------------------------- | ----------------- | --------------------------------------------------------------------------------------------------- |
| each in its own state, made at the shot's first call         | `.seed(seed).shots(OnePerShot)`                     | as promised       | here                                                                                                |
| together, from one state shared by its shots (an ensemble)   | an executor written by hand, on `ExecutorLifecycle` | as promised       | in other crates; here as a test double (`SharedState`, `src/tests.rs`)                              |
| on shared states, split where their values differ (grouped)  | a shot strategy                                     | yes               | planned, with no milestone yet: many shots of one program on a simulator whose shots rarely diverge |
| split over several executors, their replies merged (sharded) | a shot strategy                                     | as the inner ones | planned, with no milestone yet: a run spread over the cores, or over devices                        |

_Replayable_ means each shot's outcomes depend only on the seed and the shot's
id (`ShotId`), not on the other shots of the run; a program that needs it says
so in its `Needs`, a chain refuses its job when the chain's executor does not
offer it, and the conformance check
`gives_each_shot_alone_what_it_gives_among_others` checks an executor that does,
such as the executor above given `.shots(OnePerShot)`.

`.shots(OnePerShot)` makes a `OnePerShotExecutor`. Each shot's state is made at
the shot's first call, from the run's seed and the shot, and kept for the shot's
later calls until the executor closes. A call runs the logic on each of its
shots, in the batch's order. Its refusals and failures, and what it continues of
the executor it is given, are in [A state per shot: the
rules](internals/executors.md#a-state-per-shot-the-rules).

The seed is the run's, given with `.seed(seed)` before the strategy, and each
shot's state draws from it through its `ShotContext`; an adapter that draws
upstream draws from the same seed under an id of its own ([Seeds, not
generators](#seeds-not-generators)). A run of eight shots on the executor above,
from the [per-shot executor guide](../tests/per_shot_executor.rs):

```rust
let shots = ShotSet::range(0..=7).expect("not empty");
let executor = bond_limited_executor(None).seed(3).shots(OnePerShot);
let (outcomes, _) = (driver() >> executor)
    .expect("nothing between to refuse")
    .run(TossAgainOnOne, shots)
    .expect("the executor has resumable shots, log probability, 8 shots")
    .wait();
```

## Where an executor runs: `run_on`

By default an executor that `executor_fn` makes runs inside `call`, on the
caller's thread. `.run_on(placement)` places it (`PlacedExecutor`): its calls
are queued and run, in order, in the jobs it hands its placement
(`PlacementJob`). A call returns at once when the placement schedules the job to
run elsewhere (`OwnThread`, a thread pool); one that runs the job inline
(`|job: PlacementJob| job()`) runs the call's work before it returns. The
placements:

- `OwnThread`, a thread of the executor's own, for an executor that must run on
  one thread (a device's driver). The executor and its state must be `Send`,
  since the executor is moved to that thread.
- A closure that runs each job where it chooses, such as a thread pool.
- Planned, with no milestone yet: `Remote(link)`, which serves the far end of a
  link; placing any other executor that is `Send` (one written by hand, an
  adapter's executor or a chain); and placing a state that is not `Send` (an
  engine's GPU context). Until then, a chain runs on a thread of its own.

A placed executor's reply is an `IncrementalReply`, which an asynchronous task
can await. A failure on the executor's thread fails the call's reply, and the
executor refuses every later call. So does a refusal there: placed whole
(`.run_on(..)` alone, or after `.shots(..)`), an `executor_fn` refuses on its
thread, after `call` accepted the call, a new shot past its promise, a call its
`.check` refuses, and a state `make_state` cannot make; under
`.run_on(..).shots(..)`, `call` refuses them, and the executor stays usable. Its
`close` waits for the calls queued, then gives the report. Dropped without
`close`, it cancels the calls not yet started, and does not wait for its states
to be freed: call `close` to free what they hold (a device, GPU memory) before
building another executor on it. Panics, jobs dropped unrun, and where the
executor it places is closed and dropped, are in [A placed executor: the
rules](internals/executors.md#a-placed-executor-the-rules).

The order of `.shots` and `.run_on` says where the shots run:

- `.shots(OnePerShot).run_on(OwnThread)` runs every shot on the one thread.
- `.run_on(OwnThread).shots(OnePerShot)` runs each shot on a thread of its own,
  so the shots of a batch run in parallel.

The two orders, from the [tests of a placed state per
shot](../src/tests/executor_fn/one_per_shot/placed.rs):

```rust
let mut on_one_thread = recording(&threads).shots(OnePerShot).run_on(OwnThread);
let mut per_shot = recording(&threads).run_on(OwnThread).shots(OnePerShot);
```

```mermaid
flowchart LR
  subgraph one["shots, then run_on: one thread"]
    c1["calls"] --> t1["thread: the states of shots 0 and 1"]
  end
  subgraph each["run_on, then shots: a thread per shot"]
    c2["calls"] --> t20["thread: shot 0's state"]
    c2 --> t21["thread: shot 1's state"]
  end
```

For a run that does not fail, of a logic that keeps its state in the shot's
state, whose `make_state` makes the same state whatever thread it runs on, and
whose `state_report` and `log_probability` read only the shot's state and what
they captured when built, both orders give the same outcomes and report. A
thread per shot pays only for shots that share nothing and whose calls cost much
more than a thread hop, about 9 µs on one core (measured on an earlier prototype
of the protocol, not yet on this crate). Where the orders differ (a failure, a
drop, a logic's own state, the report), and what splitting a run costs, is in
[The order of `.shots` and
`.run_on`](internals/executors.md#the-order-of-shots-and-run_on).

## An executor's whole life

A run owns its executor: it passes the program's calls on, as its driver runs
it, and closes the executor once, after the program's last call. The arrows read
as in [Replies](replies.md#reading-the-sequence-diagrams):

```mermaid
sequenceDiagram
  participant C as caller
  participant R as run
  participant D as program, as its driver runs it
  participant E as executor
  C->>R: chain.run(program, shots), checked (nothing runs)
  C->>R: wait()
  R->>D: drive
  loop each call
    D->>R: call(InstructionBatchRef { shots, instructions })
    alt a shot the job did not name
      R-->>D: Err(ShotNotHeld), the executor never sees it
    else
      R->>E: call
      E-->>R: Ok(reply), or Err(rejected): nothing ran, unless its class is Resend
      R-->>D: the same, unchanged
    end
    Note over D,E: the program takes the reply its own way (see Replies)
  end
  D-->>R: the program's output
  R->>E: close(), once
  E-->>R: report
  R-->>C: (output, report)
```

The run closes the executor once, after the program's last call, also when a
call failed. A call after close is refused with `Rejected::Closed` (the
conformance check `rejects_calls_once_closed`). A run dropped before its program
ends does not close its executor (`Run`'s doc, which no test pins), an open
question that goes with the planned cancellation ([An executor's
close](internals/executors.md#an-executors-close)).

An executor whose state carries over from one call to the next refuses
every call after a failed one, with `Rejected::EarlierFailure`, which
names the failure (the conformance check `refuses_calls_after_a_failure`).
How the failure is kept, across wrappers and calls in flight, is in
[An executor fails](internals/executors.md#an-executor-fails).

## An executor written by hand: `ExecutorLifecycle`

An executor that holds one state shared by its shots (an ensemble, such as
a simulator in another crate that samples every shot at once) implements
`Executor` itself, with `Describe` and `Close`, and keeps the rules of
failure and close with `ExecutorLifecycle`:

- `admit`, at each call, refuses once closed or failed;
- `fail` records a failure learnt inside `call`;
- `bind` binds a reply that may fail after `call` returned, so that its failure
  is recorded when it is polled;
- `close` refuses every later call.

Each of its events is made with `ShotOutcome::new`, which gives it no
condition, and `.with_conditions(..)` gives it its shot's conditions so
far, which the executor must keep for each shot from one call to the next
when it offers `resumable_shots`; for such an executor, the conformance kit
checks that it does, for the conditions that an earlier outcome of the
shot carried (the requirement in
[Shot conditions](replies.md#shot-conditions)).

The crate's ensemble double, `SharedState` (`src/tests.rs`), is built on
`ExecutorLifecycle`, and so are the executors that `executor_fn` makes,
which keep no condition across calls by themselves: one that offers
`resumable_shots` meets the requirement only when its logic keeps its shots'
conditions in its state and raises them again in each call. An ensemble's
reply, and the double's conformance checks, are in
[An executor written by hand: the rules](internals/executors.md#an-executor-written-by-hand-the-rules)
and [An ensemble executor](internals/executors.md#an-ensemble-executor).

Such an executor is composed with `compose`. So that a chain ending at it
is written with `>>` all along, grouped either way, its crate implements
`>>` for it on each of the four public types that may be on its left,
`ChainHead`, `ComposedAdapters`, `AdapterFn` and `OnePerShotAdapter`
([example](../tests/executor_by_hand.rs);
[Which parts the operator joins](internals/composition.md#which-parts-the-operator-joins)).

## Seeds, not generators

Each shot draws from a seed of its own, fixed by the run's seed `s`, the shot
`j` and the id of the component that draws (`ComponentId`; the last executor's
is 0). No two shots of a run share a seed, nor two components of a shot. A seed
depends only on `(s, j, c)`, not on the other shots of the run or on which other
components draw.

The run's seed reaches each component that draws:

- an executor is built with it (`.seed(s)`), and gives shot `j`'s state
  `ShotContext::new(s, j)`;
- an adapter that draws is given it when it is built against what is
  downstream of it, bound to the id it draws with
  (`AdapterBoundary::drawing_seeds`), and draws for shot `j` from
  `AdapterSeeds::shot(j)`.

So one seed fixes the whole chain's draws. An adapter that draws with no run's
seed below is refused when composed.

`ShotContext` gives seeds, not generators: a component that wants a
generator writes `StdRng::seed_from_u64(ctx.rng_seed(c))` itself. These
seeds are not those that qdk's `run` or `run_qir` give each shot for the
same user seed. The formula, what it does not separate, and why the crate
gives no generator, are in
[Seeds: the derivation](internals/executors.md#seeds-the-derivation).

The detailed rules, for maintainers of the crate, are in
[Executors: the detailed rules](internals/executors.md).
