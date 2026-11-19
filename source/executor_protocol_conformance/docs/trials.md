# Trials

Why a check runs its rule in several trials, how to replay the trial a
report names, and how to run more. The
[README](../README.md#terms) defines `Trials` and a trial.

## Why drawn trials

A check's rule holds for any shots, batches and conditions, but a check
runs only some. A part built by iterating against fixed runs may pass
them all and break the rule elsewhere.
So each check runs trial 0 on fixed patterns, the script or shots as
given, then trials whose patterns are drawn from a seed and the trial's
index: the shots' ids, order, batches and groups, where a run fails, the
conditions raised below, and the seed of each run. The instructions and
the number of shots stay as given. [`src/trials.rs`](../src/trials.rs)
says why in full, and how the default number of trials was chosen.

## Replaying a report

A check stops at the first trial that finds the part breaking a rule, or
unable to be judged, and reports that trial alone. A trial's patterns
depend only on its seed and its index, so a report names both, and a
report of a drawn trial ends with how to replay it, for instance:

```text
Replay it with `Trials::replaying(Trial { seed: 0x7, index: 2 })`, or rerun the trials up to it with EXECUTOR_PROTOCOL_CONFORMANCE_SEED=0x7 and EXECUTOR_PROTOCOL_CONFORMANCE_TRIALS=2 for checks given `Trials::from_env()`.
```

The trials before it passed, so rerunning them ends with the same report.

## Running more trials

The default trials catch every broken fake part of the kit's tests, but a
fault keyed to a rare pattern may need more. A test that gives a check
`Trials::from_env()` reads two environment variables, with no change to
the test:

- `EXECUTOR_PROTOCOL_CONFORMANCE_SEED`: the seed, in decimal or in
  hexadecimal after `0x` or `0X`, with no sign, with spaces around the
  value and each `_` in it ignored;
- `EXECUTOR_PROTOCOL_CONFORMANCE_TRIALS`: the number of drawn trials,
  written as the seed may be.

So `EXECUTOR_PROTOCOL_CONFORMANCE_TRIALS=1000 cargo test` runs a thousand
drawn trials, and a loop over seeds searches further. Each drawn trial
costs about one trial 0 of the check. A test may instead give
`Trials::new(seed, count)`, or `Trials::replaying(trial)` to rerun the
trial a report names.
