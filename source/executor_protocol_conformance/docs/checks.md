# The checks

What each check takes and the rule it checks, by role; the rules every
check also reports; how to read a report; and how to add a rule. The
[README](../README.md) says what the kit is for and defines the words
used here (part, `make`, `Script`, `Trials`, `Nonconformance`).

## Terms

A few more words, used by the checks' arguments and the reports.

- **Seedable:** a part whose offer lets a run fix its seed, so that it
  gives the same records each time
  ([`Offer::seedable`](../../executor_protocol/src/offer.rs)).
  Every run of a trial has the trial's seed, so a part under an
  executor's or an adapter's check must be seedable.
- **`needs`:** what the kit's driver above an executor or an adapter
  needs of it (its qubits, for instance), which the part gets ready for
  at each run.
- **`first`, `second`:** two lists of instructions, for the checks of a
  shot's state, which send them in several orders. Each check's
  documentation says what they must show: in each, `second` reads what
  `first` changes.
- **Groups:** the two sets some checks split the shots into, to send each
  set in `execute`s or batches of its own. Trial 0's are fixed (such as
  the shots at even positions and those at odd ones); each drawn trial
  draws its own.

The kit sends a [`Script`](../src/script.rs)'s batches checked, except
in one run of the adapter's `states_readout_slots`, which checks
unchecked batches too.

## The rules, by role

Each check's documentation, in its file, says what it runs and what it
needs to judge the part; a check given too little (too few shots, nothing
read, instructions that change nothing) returns `CannotJudge` rather than
pass. Each Rule cell is the statement, in a report, of the rule the
check checks, or of its two rules joined by a semicolon, without the
protocol's items they cite.

**An executor's rules,** in [`src/executor.rs`](../src/executor.rs):

| Check                                 | Rule                                                                              | Takes                                                                                                     |
| ------------------------------------- | --------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| `writes_every_readout`                | it writes every readout value of each batch it answers                            | a script that reads                                                                                       |
| `gives_each_shot_what_it_gives_alone` | for a replayable part, a shot's records depend only on the seed and the shot's id | a script of three shots or more, which read differently                                                   |
| `continues_each_shot_across_executes` | it keeps a shot's state from one `execute` to the next, until its end             | shots, and a `second` that reads what `first` changes                                                     |
| `changes_no_shot_outside_its_batches` | an `execute` changes no shot outside its batches                                  | four shots or more, a `first` that changes a shot each time it runs, and a `second` that reads the change |
| `starts_each_shot_afresh`             | it makes each shot afresh at its first batch, whatever an earlier part left       | shots, and a `second` that reads otherwise after `first` than alone                                       |
| `does_nothing_until_polled`           | it does an `execute`'s work only once its future is polled                        | a script whose `execute`s read otherwise when sent twice                                                  |
| `keeps_its_accepts_and_offers`        | its `accepts` and `offers` do not change once it is composed                      | a script of its instructions                                                                              |
| `runs_as_new_after_a_failed_run`      | after a failed run, it runs the same shots again as a new part would              | a script whose first `execute` reads                                                                      |

**An adapter's rules,** in [`src/adapter.rs`](../src/adapter.rs): an
adapter answers the part above it as an executor does, so it keeps the
executor's rules above, checked by functions of the same names, except
`keeps_its_accepts_and_offers`. As a caller of the part below, it keeps
these:

| Check                                   | Rule                                                                                                                                                |
| --------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `sends_only_shots_it_got`               | it sends below only shots of the batches it got; it puts a shot in at most one batch of a call to the part below's `execute`                        |
| `passes_the_seed_on`                    | it passes the seed below unchanged                                                                                                                  |
| `states_readout_slots`                  | it states each batch's readout slots as its instructions count them                                                                                 |
| `passes_each_shots_conditions_up`       | it merges into each row the conditions read below on its shot in the same `execute`, and no other shot's, and raises none because of one read below |
| `reports_only_each_executes_conditions` | each row reports only its own `execute`'s conditions, and none raised because of an earlier one's                                                   |
| `carries_checked_batches_below`         | it sends checked batches below for a checked batch                                                                                                  |

Each takes a script the adapter sends batches below for, and
`reports_only_each_executes_conditions` a script of two `execute`s or
more.

**A driver's rules,** in [`src/driver.rs`](../src/driver.rs), each on the
run's shots, over the kit's executor:

| Check                                | Rule                                                                                                                         |
| ------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------- |
| `gives_one_output_per_shot_in_order` | it gives one output per shot, in the order of the run's shots, each the one its shot gives alone                             |
| `states_readout_slots`               | it states each batch's readout slots as its instructions count them                                                          |
| `sends_only_its_shots`               | it sends only the run's shots; it puts a shot in at most one batch of a call to the part below's `execute`                   |
| `passes_the_seed_on`                 | it passes the seed below unchanged                                                                                           |
| `merges_each_executes_conditions`    | it merges each `execute`'s conditions into the shot's output, and no other shot's, and raises none because of one read below |

Each takes shots the driver sends batches for;
`gives_one_output_per_shot_in_order` needs two or more, whose outputs
differ and depend on their shot only, and `merges_each_executes_conditions`
takes how to read conditions in an output.

## The rules every check of an executor or an adapter reports

Every check of an executor or an adapter also reports each batch answered
with readouts of another shape than the batch's, and each `end_shots` that
panics on shots the part does not hold, or no longer holds, which it must
ignore: the kit's driver ends each shot after its last `execute`, and a
run holds one more shot the driver never sends, so that the runner's end
of the run ends both kinds ([`Script`](../src/script.rs) says when a run
goes without that shot). In trial 0, a check some of whose runs went
without that shot writes to the standard error, which `cargo test` shows
beside a failing test or with `--nocapture`, that those runs did not
exercise an end of a shot never held.

## Reading a report

A check reports every place where the part broke its rule, not only the
first, and the other rules it saw broken on the way, each fault at a
place counted as [`Place`](../src/nonconformance/place.rs)'s documentation says.
A part refused below on every checked run, for slots it misstates, makes
the checks that need runs that end return `CannotJudge`, naming the
refusal: fix the rule that the adapter's or the driver's
`states_readout_slots` reports first. A check also stops at the first of
its runs that fails for another reason than the rule it checks: it
reports the rules its earlier runs found broken, if any, and otherwise
says that it cannot judge the part. `starts_each_shot_afresh` drops a
part with its shots never ended, as a crash leaves it; a part that panics
then is not judged, and the report names the panic.

`writes_every_readout` writes a value of its own into each readout slot
before the part runs, and reports a slot that equals it in every run; a
part that writes every slot and repeats its values for a seed passes in
two runs, and [`src/readout_prefills.rs`](../src/readout_prefills.rs)
gives the bound on runs, `MAX_RUNS`, and the odds of a wrong report.

A report names the trial that found the fault; [trials.md](trials.md#replaying-a-report)
says how to replay it.

## Adding a rule

A new rule the protocol states, and a part can break without the protocol
seeing it, takes four changes, all in this crate:

1. **A check:** one public function in its role's file
   ([`src/executor.rs`](../src/executor.rs),
   [`src/adapter.rs`](../src/adapter.rs) or
   [`src/driver.rs`](../src/driver.rs)), taking `make`, what to run and
   the `Trials`, as the others there do. Its documentation says why the
   rule matters to the parts around the part, how the check runs it, an
   example of a fault with the report it gives, and what the check does
   not see.
2. **A rule:** one variant of [`Rule`](../src/rule.rs), with the
   statement a report gives it, citing the protocol's item it is about.
3. **Its tests,** in `src/tests/<role>.rs`: the role's fake part made to
   break the rule (a new variant of the fake's `Breaks`), reported at
   each place where it breaks it; the check added to the role's test that
   a fake keeping every rule passes every check; and, if the check takes
   arguments it can be given too little of, the part not judged then.
4. **Its row** in the role's table above, and, if it changes what the kit
   does not check, [limits.md](limits.md).
