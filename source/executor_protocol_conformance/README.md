# The executor protocol's conformance kit

Checks, run in an implementer's tests, of the rules a part of the
[executor protocol](../executor_protocol/README.md) keeps that the protocol
states but does not check as it runs. They serve whoever writes an
executor, an adapter or a driver: a simulator, a device's runtime, a noise
model, a program's runner.

**The problem.** The protocol checks at its boundaries only what it can
check cheaply as shots run: the instructions of a checked batch, the
readout slots it states. The other rules a part keeps
([the rules a part keeps](../executor_protocol/docs/writing-a-part.md#the-rules-a-part-keeps))
are the part's to keep: an executor writes every readout value, a
replayable part's records for a shot depend only on the seed and the
shot's id, an adapter passes up the conditions read below. A part that
breaks one gives wrong results to the parts above it, with no error,
often only on some shots or some batches of them.

**How the kit solves it.** A function per rule and role (and two rules
every check of an executor or an adapter reports,
[docs/checks.md](docs/checks.md#the-rules-every-check-of-an-executor-or-an-adapter-reports))
runs the part in a real chain, between the kit's own driver and
executor: a driver that sends a script of batches above an executor or an
adapter, an executor that answers below an adapter or a driver, each
logging what passes. It runs the part as often as the rule needs (alone
and among other shots, in several orders, after a failed run), compares
what it sees, and returns `Ok(())` or a
[`Nonconformance`](src/nonconformance.rs) naming each place where the
part broke the rule. It does so in several trials: trial 0 on fixed
patterns, then trials whose patterns (such as the shots' ids, order,
batches and groups, where a run fails, and the conditions raised below)
are drawn from a seed.

**Why this approach.** The rules are stated once, in the protocol, and
checked once, here, for every implementer: an implementer writes no check
of its own, only a few lines per rule in its tests. The kit sees the part
through its role's trait alone, so it checks any implementation, and a
new rule is a new function and a new [`Rule`](src/rule.rs)
([adding a rule](docs/checks.md#adding-a-rule)). The kit's driver and
executor run under the real `>>`, so the part is checked as it runs in a
chain. A part built by iterating against fixed runs may pass them all
and break the rule elsewhere; patterns drawn from a seed cannot be
fitted so, and a report names its seed and trial, so the failure replays
exactly.

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
- **[`Trials`](src/trials.rs):** the trials a check runs. Trial 0 runs
  the script or shots as given, on fixed patterns; each later trial runs
  the same instructions on patterns drawn from the seed and the trial's
  index. `Trials::default()` runs trial 0 and a fixed number of trials
  drawn from a fixed seed, so a check gives the same result each time; a
  [`Trial`](src/trials/trial.rs) is one of them, as a report names it.
- **[`Nonconformance`](src/nonconformance.rs):** why a check does not pass.
  `Broken` names the part, each [`Rule`](src/rule.rs) it broke,
  and each [`Place`](src/nonconformance/place.rs) where it broke it, such as a
  batch, a row, a readout slot, a batch it sent below, an output or a run.
  `CannotJudge` says why the check could not judge the part with what it was
  given, and what to change. Each names the trial that found it.

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
the rule, each place and the trial. This one is `FakeExecutor` made to leave
one readout slot of shot 1 unwritten in each `execute`, as the kit's
[test of reports](src/tests/nonconformance.rs) pins it:

```text
`executor_protocol_conformance::tests::fake_executor::FakeExecutor` breaks 1 rule, in trial 0, on the fixed patterns:
- it writes every readout value of each batch it answers (`Execute::execute`):
  - execute 0, batch 0, row 1 (shot 1), slot 1: holds the value the kit wrote before the part ran, in each of 22 runs
  - execute 1, batch 0, row 1 (shot 1), slot 0: holds the value the kit wrote before the part ran, in each of 22 runs
```

## The checks

A check is a function per rule, or two rules it reads in the same runs,
in its role's file, whose documentation
says why the rule matters to the parts around the part, how the check
runs it, an example of a fault with the report it gives, and what the
check does not see:

- [`src/executor.rs`](src/executor.rs): an executor's rules, such as
  writing every readout value and keeping each shot's state from one
  `execute` to the next.
- [`src/adapter.rs`](src/adapter.rs): an adapter's, an executor's rules
  except keeping its `accepts` and `offers`, and, as a caller of the part
  below, the shots, seed, readout slots and checked batches it sends and
  the conditions it passes up.
- [`src/driver.rs`](src/driver.rs): a driver's, such as one output per
  shot in the order of the run's shots, checked over the kit's executor.

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
