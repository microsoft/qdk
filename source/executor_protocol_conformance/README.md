# The executor protocol's conformance kit

The conformance kit, the QDK crate `executor_protocol_conformance`, was
designed to solve the problem of checking that a part of the
[executor protocol](../executor_protocol/README.md) (an executor, an
adapter or a driver, such as a simulator, a device's runtime, a noise
model or a program's runner) keeps the rules the protocol states but does
not check as shots run. The protocol checks at its boundaries only what it
can check cheaply, such as the instructions of a checked batch and the
readout slots it states; the other
[rules a part keeps](../executor_protocol/docs/writing-a-part.md#the-rules-a-part-keeps)
are the part's to keep: an executor writes every readout value, a
replayable part's records for a shot depend only on the seed and the
shot's id, an adapter passes up the conditions read below. Without a
shared check, the problem includes:

- wrong results with no error: a part that breaks a rule gives the parts
  above it values they read as the shot's;
- faults that show only on some shots, some batches of them or some
  orders, so that tests on a few fixed runs pass;
- the same checks written by every implementer, each in its own way, and
  the rules that are hard to check by hand left unchecked, such as a
  part's runs after a failed one, or its work before its future is polled.

The kit solves these with a check per rule and role, run in the
implementer's tests. Each check runs the part in a real chain, between
the kit's own driver and executor, which log what passes: a driver that
sends a script of batches above an executor or an adapter, and an
executor that answers below an adapter or a driver. It runs the part as
often as the rule needs (alone and among other shots, in several orders,
after a failed run), compares what it sees, and returns `Ok(())` or a
report naming each place where the part broke the rule. Trial 0 runs
fixed patterns; each later trial draws its patterns (such as the shots'
ids, order, batches and groups, where a run fails, and the conditions
raised below) from a seed, which the report names.

Today the kit holds:

- **The checks:** a function per rule, or per two rules read in the same
  runs, in a module per role: [`executor`](src/executor.rs),
  [`adapter`](src/adapter.rs) and [`driver`](src/driver.rs). Every check
  of an executor or an adapter also reports
  [two rules](docs/checks.md#the-rules-every-check-of-an-executor-or-an-adapter-reports)
  any of its runs can show broken.
- **What a check runs:** a [`Script`](src/script.rs), the `execute`s the
  kit's driver sends an executor or an adapter; and the
  [`Trials`](src/trials.rs) it runs, each a
  [`Trial`](src/trials/trial.rs) named by its seed and index.
- **What a check reports:** a [`Nonconformance`](src/nonconformance.rs):
  either each [`Rule`](src/rule.rs) the part broke, as a
  [`BrokenRule`](src/nonconformance/broken_rule.rs) holding each
  [`Fault`](src/nonconformance/fault.rs), what the check found at a
  [`Place`](src/nonconformance/place.rs); or why the check could not
  judge the part, and what to change. Each names the trial that found it.

Users write a test per part and rule: they take the kit as a
dev-dependency and call the rule's check with a closure that makes their
part, what to run and the trials, as [An example](#an-example) shows.
[docs/checks.md](docs/checks.md) lists every check with what it takes.

**Why this approach.**

- **One check per rule, for every implementer.** The rules are stated
  once, in the protocol, and checked once, here: an implementer writes no
  check of its own, only a few lines per rule in its tests, and a new rule
  is a new function and a new `Rule`
  ([adding a rule](docs/checks.md#adding-a-rule)).
- **Any implementation, as it runs in a chain.** The kit sees the part
  through its role's traits alone, so it checks any implementation; and
  its driver and executor join the part with the protocol's own `>>`, so
  the part runs as it does in a chain.
- **Drawn patterns, replayed exactly.** A part built by iterating against
  fixed runs may pass them all and break the rule elsewhere; patterns
  drawn from a seed cannot be fitted so, and a report names its seed and
  trial, so the failure replays exactly.

## Terms

The checks share a few words, defined once here;
[docs/checks.md](docs/checks.md#terms) defines those of the checks'
arguments.

- **Part:** the executor, adapter or driver under test.
- **`make`:** a closure that builds the part afresh, the same each time.
  Each run of a check is on a new part, except in the
  [executor's](src/executor.rs) and the [adapter's](src/adapter.rs)
  `runs_as_new_after_a_failed_run`, whose rule is about a part's later
  runs.
- **[`Script`](src/script.rs):** the `execute`s the kit's driver sends an
  executor or an adapter, in order, each a list of batches: their shots,
  and their instructions, written in the instruction set the part accepts.
- **[`Trials`](src/trials.rs):** trial 0 runs the script or shots as
  given; each later trial runs the same instructions on patterns drawn
  from the seed and the trial's index. `Trials::default()` runs trial 0
  and a fixed number of trials drawn from a fixed seed, so a check gives
  the same result each time.
- **[`Nonconformance`](src/nonconformance.rs):** its `Broken` case names
  the part and lists each [`BrokenRule`](src/nonconformance/broken_rule.rs)
  with its [`Fault`](src/nonconformance/fault.rs)s; a fault's
  [`Place`](src/nonconformance/place.rs) is, for instance, a batch, a row,
  a readout slot, a batch the part sent below, an output or a run. Its
  `CannotJudge` case says why the check could not judge the part with what
  it was given (too few shots, nothing read) and what to change.

## An example

An implementer's crate takes the kit as a dev-dependency:

```toml
[dev-dependencies]
executor_protocol_conformance = { path = "../executor_protocol_conformance" }
```

An executor's test then checks that it writes every readout value, on a
script of two `execute`s that read differently on each shot. Adapted
from the kit's own tests ([`src/tests.rs`](src/tests.rs) and
[`src/tests/executor.rs`](src/tests/executor.rs)), with `shots` and
`needs` written out, the executor is `FakeExecutor`, a fake executor of
classical bits from the kit's tests; `gates` writes each pair of a
mnemonic and a qubit as an instruction of the set the executor accepts,
here QIR's `h`, `x` and `mz`:

```rust
use executor_protocol::{Needs, ShotSet};
use executor_protocol_conformance::{Script, Trials, executor};

let script = Script::new()
    .execute([(
        ShotSet::ids(0..4),
        gates(&[("h", 0), ("x", 1), ("mz", 0), ("mz", 1)]),
    )])
    .execute([
        (ShotSet::ids([0, 1]), gates(&[("x", 0), ("mz", 0)])),
        (ShotSet::ids([2, 3]), gates(&[("h", 1), ("mz", 1)])),
    ]);
let trials = Trials::from_env();
if let Err(report) =
    executor::writes_every_readout(FakeExecutor::new, Needs::qubits(4), &script, trials)
{
    panic!("{report}");
}
```

`Trials::from_env()` runs the default trials unless the environment asks
for more (see [Running more trials](docs/trials.md#running-more-trials)).

A part that breaks the rule fails the test with a report naming the part,
the rule, each place and the trial. For example, for `FakeExecutor` made
to leave one readout slot of shot 1 unwritten in each `execute`, the
report reads as follows (the kit's [test of reports](src/tests/nonconformance.rs)
checks that it names the part, the rule, each place and the trial; its
wording may change):

```text
`executor_protocol_conformance::tests::fake_executor::FakeExecutor` breaks 1 rule, in trial 0, on the fixed patterns:
- it writes every readout value of each batch it answers (`Execute::execute`):
  - execute 0, batch 0, row 1 (shot 1), slot 1: holds the value the kit wrote before the part ran, in each of 22 runs
  - execute 1, batch 0, row 1 (shot 1), slot 0: holds the value the kit wrote before the part ran, in each of 22 runs
```

## The checks

A check is a function per rule, or per two rules it reads in the same
runs, in its role's file, whose documentation
says why the rule matters to the parts around the part, how the check
runs it, an example of a fault with the report it gives, and what the
check does not see. By role, the checks cover:

- [`src/executor.rs`](src/executor.rs): an executor's rules: its
  readouts and their shape, each shot's records and state, its ends of
  shots it does not hold, its work only once polled, its `accepts` and
  `offers` unchanged once composed, and a run after a failed one.
- [`src/adapter.rs`](src/adapter.rs): an adapter's: an executor's rules
  except keeping its `accepts` and `offers`, and, as a caller of the part
  below, the shots, seed, readout slots and checked batches it sends and
  the conditions it passes up.
- [`src/driver.rs`](src/driver.rs): a driver's, checked over the kit's
  executor: one output per shot in the order of the run's shots, the
  shots, seed and readout slots it sends, and each `execute`'s conditions
  merged into its outputs.

The guides in [`docs/`](docs):

- [checks.md](docs/checks.md): each check, its rule and what it takes;
  the rules every check of an executor or an adapter also reports; how to
  read a report; and how to add a rule.
- [trials.md](docs/trials.md): why a check runs drawn trials, how to
  replay the trial a report names, and how to run more.
- [limits.md](docs/limits.md): what the kit does not check, such as the
  values themselves, how a part uses the seed, or a chain composed with
  `>>`, each with why.

## Tests

The tests under [`src/tests/`](src/tests) check each check: it passes a
fake part that keeps every rule, and reports a fake that breaks its rule
at the places where it breaks it. The fakes run classical bits written in
QIR's `h`, `x` and `mz`: an executor, an adapter and a driver, each able
to break one rule at a time. Other fakes are fitted to one fixed pattern
of trial 0 each (such as its ids, their order, its uncut batches, its
groups, the kinds it raises, its seed, its number of `execute`s); the tests
check that the drawn trials report them. Run them with
`cargo test -p executor_protocol_conformance`.

## Platforms

| Platform      | Status                                                                                                                          |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| Linux aarch64 | the tests run                                                                                                                   |
| Linux x86-64  | expected, not verified                                                                                                          |
| wasm32        | not built: the checks wait for each run on the calling thread, with `Runner::run`, which the protocol does not build for wasm32 |
