# Glossary

[Protocol overview](../README.md).

The terms a reader meets in the README and in these pages, each in a
sentence or two, with the section that details it. The terms are grouped by
what they belong to; the names in code are the crate's.

## A chain and its parts

- **Chain**: a driver, any number of adapters and an executor, joined by
  `compose` (written `>>`); each part knows only the message of the one
  downstream
  ([Levels, roles and laws](internals/composition.md#levels-roles-and-laws)).
  Its prefix composed with the executor, the built chain is a `Chain`, which
  runs the job given to it, `chain.run(program, shots)`
  ([A chain and its job](composition.md#a-chain-and-its-job)).
- **Driver** (`Driver<P>`): the top of a chain, which runs any program of
  form `P` given to the chain, `chain.run(program, shots)`. A runtime that
  executes a program's control logic, such as a QIR runtime, is one: in
  `drive` it makes its calls on the one executor it is given, sending
  batches and reading their outcomes, and the adapters and the executor
  below look to it like that single executor. The caller builds the chain,
  `runtime.into_chain() >> adapters >> executor`, then calls
  `chain.run(program, shots)`. The crate's driver (`BuiltinDriver`, in the
  prefix `driver()` gives) runs a static program, and lets an adaptive
  program drive ([Composing a run](composition.md#composing-a-run)). A
  device's driver, the vendor's software below an executor, is not a
  `Driver`; and a caller that takes a reply is called a caller, not a
  driver.
- **Chain prefix** (`ChainPrefix`): every element of a chain being built
  but its last, the executor: a driver, wrapped so that it composes with
  `>>`, with the adapters composed below it so far. `driver()` is the
  crate's driver as a prefix with no adapter; `driver.into_chain()`
  (`DriverExt`) makes one of any driver. A prefix with an adapter composed
  below it is a prefix still, and with the executor it gives a
  `Chain`. It is not itself a `Driver<P>`: that is planned
  ([Planned features](planned.md)).
- **Executor** (`Executor<M>`): anything that runs one message type `M`, a
  batch of its instruction type (`InstructionBatch<I>`), on each shot the
  batch names. Offering `resumable_shots` promises that a later call continues a
  shot from the state its earlier calls left; without it, a caller may not
  count on that, though an executor made by `executor_fn` keeps each shot's
  state until it closes, whatever it promises. An ensemble holds one state
  shared by its shots
  ([An ensemble executor](internals/executors.md#an-ensemble-executor)).
- **Last executor**: the executor at the end of a chain, which calls nothing;
  a position, not a type. Its capabilities and its report originate there
  ([Levels, roles and laws](internals/composition.md#levels-roles-and-laws)).
- **Adapter** (`Adapter`): not an executor but a map from an executor to an
  executor: it maps the executor downstream of it, once, to an executor
  upstream, such as a code, a noise model or a recorder. The smallest is a
  state and a logic (`adapter_fn`); a state that keeps its way below, such
  as a code's runtime that owns its backend, is made an adapter by
  `owning_adapter_fn` ([Adapters](adapters.md)). One that sends
  lower-level instructions than it accepts, such as
  a code, is a lowering; one that sends what it accepts, such as a recorder,
  is a probe ([Adapters compose](internals/composition.md#adapters-compose)).
- **Probe**: an adapter that keeps the instructions: it sends what it
  accepts, and observes, pauses, or only adds refusals. The debugger of
  the guide tests (`tests/debugger/`) is one, and so would be a validator;
  a resource probe, which counts what a run uses, is planned
  ([Adapters compose](internals/composition.md#adapters-compose)).
- **Component**: a part of a chain, in one of its three roles: the driver,
  an adapter, or the executor (at the end of the chain, the last
  executor). A refusal of a job's needs, when the job is given to the
  chain, names the component checked (`UnmetNeed`), by a type name that
  includes those it wraps, so that the caller finds the one that falls
  short; each component that draws does so under an id of its own
  (`RngId`), so that adding one changes no other's draws
  ([Levels, roles and laws](internals/composition.md#levels-roles-and-laws)).
- **Runtime**, three senses, each named by whose it is:
  - a **code's runtime**: what runs a code's gadgets and decodes their
    readouts over a backend it owns, as a Qodec runtime does; in a chain,
    a shot's state of an `owning_adapter_fn`
    (`tests/static_isa/code_runtime.rs`;
    [A state that owns its way below](adapters.md#a-state-that-owns-its-way-below-owning_adapter_fn));
  - a **program's runtime**: what walks a program and feeds measurements
    back, such as a QIR runtime. In a chain it is a driver (`Driver<P>`,
    for its form of program `P`; a QIR runtime's, `Driver<QirProgram>`, is
    planned), which runs the program's classical part itself and sends its
    quantum regions downstream. The gates it sends are its needs
    (`Needs::instruction_set`), checked when the job is given
    (`Chain::run`)
    ([Writing a driver](../README.md#writing-a-driver);
    [How applications use it](instruction-sets.md#how-applications-use-it));
  - an **async runtime**, such as Tokio: what polls tasks
    ([An async task leaves its thread to other tasks](internals/replies.md#an-async-task-leaves-its-thread-to-other-tasks)).
- **Adaptive program** (`AdaptiveProgram`): a program that drives the
  executor itself: it sends each batch and can choose the next from the
  outcomes so far. A static program, given to a driver, is one that decides
  nothing. An adaptive program is consumed by its run. The crate's
  driver lets it drive; a chain runs any job's program as one, the program
  as its driver drives it (`DrivenProgram`)
  ([A chain and its job](composition.md#a-chain-and-its-job)).
- **Job**: what a `Chain` is given to run, `chain.run(program, shots)`: a
  program and its shots. It is checked before any call, every fault at once
  (`JobFaults`): its shots, its needs against what the chain offers, and the
  program's own faults; of several shots named twice, only the least is reported
  ([A chain and its job](composition.md#a-chain-and-its-job)).
- **Static program** (`StaticProgram`): a program of fixed batches, data
  with no code of its own, which the crate's driver sends in order to every
  shot of its job; it decides nothing on its outcomes, and a batch refused
  or failed stops it
  ([A chain and its job](composition.md#a-chain-and-its-job)).
- **Run** (`Run`): a `Chain` given a job. It owns the chain and
  runs nothing until it is waited for; then it runs the program, passes its
  calls on and closes the executor, once, when its program ends; a run
  dropped or unwound before then, and a `Chain` dropped without a job or
  whose job is refused, do not close it
  ([An executor's whole life](executors.md#an-executors-whole-life),
  [Known limits](internals/composition.md#a-run-closes-its-executor-only-when-its-program-ends)).
- **Compose** (`compose`, `>>`): puts the parts together, builds the chain
  from the executor upstream and checks it before anything runs, every
  fault reported at once (`BuildFaults`), except a part composed with the
  executor first, built on its own
  ([Composing a run](composition.md#composing-a-run)).
- **Boundary**: where the calls of an adapter (`AdapterBoundary`), or of
  the chain's driver, meet the executor downstream. `compose` decides an
  adapter's when it builds the chain, and the chain its driver's when a job
  is given to it (`Chain::run`): it checks the set sent against the set
  downstream, and has calls by opcode renumbered on their way down where
  the two sets number them otherwise
  ([Composing a run](composition.md#composing-a-run)).
- **Level**: how abstract the instructions are, logical or physical. An
  adapter that lowers instructions goes down a level and sends the same
  message
  ([Levels, roles and laws](internals/composition.md#levels-roles-and-laws)).
- **Capabilities and needs** (`Capabilities`, `Needs`): what a component offers
  (`Describe`) and what a job's program needs; a chain given a job checks one
  against the other (`Chain::run`), and an unmet need names the component
  (`UnmetNeed`) ([Capabilities and needs](executors.md#capabilities-and-needs)).
- **Conformance kit** (the `conformance` feature and module): checks of the
  rules the compiler cannot check, such as those of `call`, `close` and
  `fold`, which an implementer runs in its own tests
  ([Conformance checks](composition.md#conformance-checks)).

## A part's axes

- **Axis**: a choice that makes a part, independent of the others, set
  by an argument or a method of the facility that made the part (How, by
  what the logic calls), named alike on `executor_fn` and `adapter_fn`
  where both have it. There are six:
  - **What**: what the part does with a call: its logic
    ([Making an executor](executors.md#making-an-executor-executor_fn),
    [Adapters](adapters.md#a-state-and-a-logic-adapter_fn));
  - **Promise**: what it offers and admits, checked when the chain is built and
    when a job is given to it (what it offers) and at each call (what it admits,
    `.check`): an executor's `promise` argument; an adapter's sets,
    `.accepts(set)` and `.sends(set)`, and the id it draws with, `.drawing(id)`;
    and, on both, `.check(f)`, which refuses a call before the state is made and
    the logic runs. Options that change what an adapter offers or needs
    (`.offers(..)`, `.needs(..)`) are planned, and so is a facility that checks
    each call against the instruction set its part promises, so that `.check`
    holds only the part's other rules ([Capabilities and
    needs](executors.md#capabilities-and-needs),
    [Seeds](adapters.md#seeds-drawingid), [Planned features](planned.md));
  - **Shots**: whose state the logic sees: one state by default, a state
    per shot given `.shots(OnePerShot)`, on both facilities; grouped and
    sharded strategies are planned
    ([How an executor holds its shots](executors.md#how-an-executor-holds-its-shots),
    [A state per shot](adapters.md#a-state-per-shot-shotsonepershot));
  - **Where**: which thread runs the logic: by default the caller's,
    inside `call`; an executor's `.run_on(OwnThread)`, or `.run_on` given
    a closure that runs each job where it chooses, such as a pool.
    `Remote(link)`, another machine, is planned, and so is `.run_on` on
    adapters
    ([Where an executor runs](executors.md#where-an-executor-runs-run_on));
  - **How**: how an adapter takes the reply of what is below, chosen by
    what its logic calls rather than by a method: `below.send(..)` passes
    the reply up as it comes, and `below.run(..)` waits for its readouts.
    An executor has nothing below
    ([Adapters](adapters.md#a-state-and-a-logic-adapter_fn));
  - **Reports**: what it says when it closes: an executor's
    `.log_probability(f)` and `.state_report(f)`, an adapter's
    `.state_report(f)`; `.log_probability` on adapters is planned
    ([Reports](adapters.md#reports-state_reportf)).

  The options above are methods of what the facility returns, so the compiler
  has them given before `.shots` and `.run_on`. Each of these two puts a wrapper
  on the part made so far, the later around the earlier, so their order says
  where the shots run: `.shots(OnePerShot)` then `.run_on(OwnThread)` runs every
  shot on one thread, the other order each shot on a thread of its own. For a
  run that does not fail, of a logic that keeps its state in the shot's state,
  whose `make_state` makes the same state whatever thread it runs on, and whose
  `state_report` and `log_probability` read only the shot's state and what they
  captured when built, both orders give the same outcomes and report, and a
  failed call reports no state made for it in either. Drivers have no axes
  spelled as methods yet: a job's needs are stated by an adaptive program
  (`AdaptiveProgram::needs`), or by its driver for a program of another form
  (`Driver::needs`). The seed is not an axis, since no part offers it: it is
  the run's ([Seed](#shots-and-states)).

- **Wrapper**: an executor that wraps another of the same message, or is made of
  one, and continues it, such as the one `.shots(..)` or `.run_on(..)` puts on
  an executor. A wrapper never forgets a refusal: once the executor it wraps is
  closed or failed, it refuses every call, in either order of the two ([An
  executor fails](internals/executors.md#an-executor-fails)). An adapter's
  executor wraps the one below too, but runs other calls on it; these pages call
  it an adapter.

## Shots and states

- **Shot** (`ShotId`): one independent run of the program. A call names a
  set of shots (`ShotSet`); one shot is a set of one
  ([What this crate does](internals/composition.md#what-this-crate-does)).
- **State**: what an executor keeps for a shot from one call to the next,
  such as a simulator's qubits. `executor_fn` makes it with `make_state`,
  from the shot's context (`ShotContext`)
  ([Making an executor](executors.md#making-an-executor-executor_fn)).
- **Shot strategy** (`ShotStrategy`): how an executor made by `executor_fn`
  holds its shots' states, given to `.shots(strategy)`. `OnePerShot`
  makes one state per shot, at the shot's first call; grouped and sharded
  strategies are planned
  ([How an executor holds its shots](executors.md#how-an-executor-holds-its-shots)).
- **Ensemble**: an executor that runs a batch's shots together, from one
  state shared by its shots, such as a simulator that samples every shot
  at once; it is written by hand, on `ExecutorLifecycle`
  ([An ensemble executor](internals/executors.md#an-ensemble-executor)).
- **Replayable**: a capability. Each shot's outcomes depend only on the seed
  and the shot's id (`ShotId`), so a shot run alone gives what it gives
  among others, and the shots can be split across runs
  ([How an executor holds its shots](executors.md#how-an-executor-holds-its-shots)).
- **Seed**: shot `j` draws, in each component of the chain that draws, from
  a seed fixed by the run's seed, `j` and the component's id
  (`RngId`). The seed is the run's, not a part's: the last executor
  is given it (`.seed(s)`, on an `executor_fn`, when it is built) and
  gives it to its states; an adapter that draws upstream is given the same
  `s` when it is built, bound to its own id
  (`AdapterBoundary::drawing_seeds`), so one seed replays the chain. The
  run is planned to take the seed with the job, so that one chain serves
  many seeds ([Planned features](planned.md)). The crate gives seeds,
  not generators
  ([Seeds, not generators](executors.md#seeds-not-generators)).

## Making and placing an executor

- **`executor_fn`**: the facility that makes an executor (`ExecutorFn`) from
  the implementer's logic: a promise (the `Capabilities` it offers), a
  state made from the shot's context (`make_state`) and a logic run on the
  state (`logic`)
  ([Making an executor](executors.md#making-an-executor-executor_fn)).
- **Report closure**: a closure given to `.log_probability`, which reads a
  state's log probability, or to `.state_report`, which reads what a state
  tracked. It runs once per state, when the executor closes, to build the
  report
  ([Making an executor](executors.md#making-an-executor-executor_fn)).
- **Executor placement** (`ExecutorPlacement`): where a placed executor's
  work runs: on a thread of its own (`OwnThread`), or in the jobs
  (`PlacementJob`) that a closure runs where it chooses, such as a thread pool
  ([Where an executor runs](executors.md#where-an-executor-runs-run_on)).
- **Placed executor** (`PlacedExecutor`): an executor wrapped by
  `.run_on(placement)`. Its calls are queued, and return at once when the
  placement runs the work elsewhere; its work runs on the placement, in
  order, and its replies are `IncrementalReply`s;
  with `.run_on` before `.shots`, each shot has a placed executor of its own
  (`OnePerShotPlacedExecutor`)
  ([Where an executor runs](executors.md#where-an-executor-runs-run_on)).

## Calls and replies

- **Message** (`Message`): what a call carries, and which events its reply
  carries. There is one kind, the batch
  ([What this crate does](internals/composition.md#what-this-crate-does)).
- **Batch** (`InstructionBatch`): a list of instructions, run in order on
  each shot of a set, after the batches before them
  ([What this crate does](internals/composition.md#what-this-crate-does)).
- **Call** (`Executor::call`): hands a batch to an executor, which returns a
  reply or refuses the batch; the executor may run the work before it
  returns
  ([What this crate does](internals/composition.md#what-this-crate-does)).
- **Reply** (`Reply`): a call's answer in progress, polled for its events
  until it ends, `Done`, `Failed` or `Cancelled` (`ReplyStatus`; a placed
  executor dropped cancels its calls not yet started). The crate's two
  basic replies are `ImmediateReply`, for work that ran inside `call`, and
  `IncrementalReply`, filled through its `ReplySender`, for work that runs
  elsewhere; composition wraps replies in its own, so a caller stays
  generic over `Reply`
  ([Replies](replies.md#replies-one-reply-four-ways-to-take-it)).
- **Outcome** (`ShotOutcome`): an event of a reply, a value under its shot
  and the tag the batch gave its readout, marked with the shot's conditions
  so far. An executor that reports by tag
  refuses a tag repeated within a batch, by calling `check_unique_tags`
  ([Readout tags](replies.md#readout-tags)). A shot's outcomes come in the
  order its executor gives them, not sorted by tag: the shot's measurement
  order for the executors `executor_fn` makes, not always for an adapter, so
  a consumer matches each readout by its tag; the order across shots
  carries no meaning, so two builds are compared shot by shot
  ([The order of a reply's events](internals/replies.md#the-order-of-a-replys-events)).
- **Conditions** (`ShotConditions`): what a component found on a shot
  without ending it, any of rejected, inconsistent and unresolved. They
  are the shot's, not a readout's: each outcome carries its shot's
  conditions so far, an `adapter_fn` merges those raised below with its
  own and marks its outcomes with them, and a caller takes a shot's as the
  union of those its outcomes carry
  ([Shot conditions](replies.md#shot-conditions)).
- **Wake**: a reply wakes its waiting caller when it can make progress; an
  `IncrementalReply` wakes it at most once per sleep, however many events
  arrive
  ([The waking model](replies.md#the-waking-model)).

## Instructions

- **Instruction type** (the `I` of `InstructionBatch<I>`): fixes the shape
  of a batch's instructions. A Rust type, such as the guides' `Op`, is
  checked by the compiler; `InstructionCall` is checked by the executor,
  against the set it describes
  ([Instruction sets: types and descriptions](instruction-sets.md#instruction-sets-types-and-descriptions)).
- **Instruction set** (`InstructionSet`): a description of the
  instructions an executor runs, each named by its mnemonic, with the kinds
  of its operands and, where declared, its action. Any executor may give
  one (`Capabilities::instruction_set`), whatever its instruction type, and
  a chain checks it: against an adapter's when the chain is built, and
  against a program's when its job is given to the chain
  ([A run-time set: a description](instruction-sets.md#a-run-time-set-a-description)).
- **Instruction call** (`InstructionCall`): an instruction of a set known
  only at run time: its opcode and its operands
  ([A run-time set: a description](instruction-sets.md#a-run-time-set-a-description)).
- **Mnemonic**: an instruction's name in its set, such as `measure`. A name
  is resolved to its opcode once, when a program or an adapter is built,
  and a log gets it back with `set.display(&call)`
  ([A run-time set: a description](instruction-sets.md#a-run-time-set-a-description)).
- **Opcode** (`Opcode`): an instruction's position in the order its set
  declares it. A call carries it, so an executor dispatches by index
  (`OpcodeTable`). It means something only in its set: where two sets
  number the same instructions otherwise, calls are renumbered at their
  boundary
  ([A run-time set: a description](instruction-sets.md#a-run-time-set-a-description)).
- **Action** (`Action`): what an instruction does, declared as Qodec declares
  it: the blocks it takes and gives, then its steps. Sets of different names are
  matched by action, whatever the mnemonics, and between them an instruction
  whose action either set leaves undeclared is refused; sets of the same name
  are matched by mnemonic, with no action needed ([What an instruction
  does](instruction-sets.md#what-an-instruction-does),
  [Compatibility](instruction-sets.md#compatibility)).

## Refusals and failures

- **Rejection** (`Rejected`): a call refused; the reason says why, and its
  retry class says whether the work may have run
  ([What a rejection says](rejections.md#what-a-rejection-says)).
- **Retry class** (`RetryClass`): `Never` (correct the message) and `Later`
  (send it again later) say that nothing ran; `Resend` says that no verdict
  came over a link, so the work may have run
  ([What a rejection says](rejections.md#what-a-rejection-says)).
- **Failure**: a call that started and could not finish ends its reply with
  `ReplyStatus::Failed`, and the events delivered before stand. An executor
  whose state carries over then refuses its later calls (`EarlierFailure`)
  ([An executor fails](internals/executors.md#an-executor-fails)).
- **Executor lifecycle** (`ExecutorLifecycle`): what keeps an executor's
  rules of failure and close: it admits calls, records the earliest
  failure, binds replies that may fail later, and refuses calls once failed
  or closed
  ([An executor written by hand](executors.md#an-executor-written-by-hand-executorlifecycle)).

## Closing and reports

- **Close** (`Close`): finishes an executor's work, a placed executor
  waiting for the calls queued, and returns its report, once; later calls
  are refused (`Closed`). It stops no work: cancelling at close is planned
  ([Replies](replies.md#one-call)).
- **Report** (`Close::Report`): what an executor gives at close, of any
  type that folds (`Fold`); an ensemble written by hand may report a
  count. `executor_fn`'s executors report an `ExecutorReport`: the calls
  they accepted (`calls`), each shot's log probability where it is known
  (`log_p`, a `LogP`), and their states' reports, folded (`states`)
  ([What this crate does](internals/composition.md#what-this-crate-does)). A
  `PlacedExecutor` counts only the calls that reached the executor it
  places, and none once its placement stopped; through an adapter given no
  `state_report`, the report is the last executor's, passed up as it is,
  and one given a `state_report` adds its own beside it, its own first
  ([Reports](adapters.md#reports-state_reportf)).
- **Fold** (`Fold`): adds one report into another. The empty report adds
  nothing, and the order and grouping do not change the total, up to
  rounding for sums in floating point (`LogP`'s may differ in their last
  bits when regrouped), so the
  reports of shots run apart fold, shot by shot, into what one executor
  that ran them all reports: its log probabilities, and its states'
  reports where they count per shot. `calls` adds up across the executors
  ([What this crate does](internals/composition.md#what-this-crate-does)).

## Over a link (planned)

The link and its layers are not built yet; they are planned, with no
milestone, for an executor across a network. Their ids and rejections
exist (`CallId`, `Rejected::IdReused`, `Rejected::NoReply`), so that a
refusal keeps its meaning over a link.

- **Remote executor** (planned): an executor reached over a link, such as a
  device behind a cloud service or a simulator on another machine that a
  runner here drives. Its caller sends the same message
  ([A lost verdict](rejections.md#a-lost-verdict-planned-layers-l1-and-l0)).
- **Layers L1 and L0** (planned): what a remote executor does inside its
  link. A chain includes them only when it reaches over a link
  ([Levels, roles and laws](internals/composition.md#levels-roles-and-laws)).
  Elsewhere these pages use layer only in this sense; a part of a chain
  is an adapter or a [wrapper](#a-parts-axes), and a Qodec layer, Qodec's
  term, is in a chain an adapter.
- **Link and frame** (planned, layer L0): a link carries frames, each with a
  sequence number, and may lose, repeat or reorder them; the far end runs a
  frame resent under the same number at most once
  ([A lost verdict](rejections.md#a-lost-verdict-planned-layers-l1-and-l0)).
- **Endpoint and request** (planned, layer L1): an endpoint is called with
  requests, each a batch under a `CallId`, and its far end runs each id at
  most once; a repeat gets how the first run ended, and L0 replays the
  run's events for a frame resent under its number. Who chooses the id is
  decided with the link layers
  ([A lost verdict](rejections.md#a-lost-verdict-planned-layers-l1-and-l0)).
- **Verdict**: a call's reply or refusal. Over a link it may be lost, and
  the caller's end then refuses with `NoReply`, class `Resend`
  ([A lost verdict](rejections.md#a-lost-verdict-planned-layers-l1-and-l0)).
