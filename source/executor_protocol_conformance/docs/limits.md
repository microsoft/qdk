# What the kit does not check

The rules and faults no check of the kit sees, each with why. The
[README](../README.md) says what the kit checks, and
[checks.md](checks.md) lists each check.

- **The values themselves:** whether a simulator's distribution is right
  is its own tests' job; the kit checks how values are written, kept and
  passed, whatever they are.
- **Some ways to leave a slot unwritten:** a part that reads the kit's
  value before writing it, or that answers with new readouts of the right
  shape, passes `writes_every_readout`.
- **Some ways to mishandle an end of shots not held:** a part whose
  `end_shots` panics on such shots is reported; one that instead changes
  a shot it holds shows only where a later run on the same part reads it
  (`runs_as_new_after_a_failed_run`). And the kit cannot tell which shot
  of an `end_shots` a panic came from: a panic that a held shot caused is
  reported under the rule on shots not held when the same `end_shots` also
  ends shots not held, and fails the run otherwise.
- **An executor's `accepts` and `offers` between runs:**
  `keeps_its_accepts_and_offers` reads them before `>>`, once the executor
  got ready for a run, and at each `execute` of a run, so a change at a
  time it does not read them, such as between two runs, passes.
- **An adapter's `accepts`, `sends`, `needs_below` and `offers_above` once
  composed,** which `keeps_its_accepts_and_offers` checks on an executor
  only; nor whether its `needs_below` is enough and states every block it
  takes below, which shows only in which parts an admission's refusal
  names.
- **A driver's polling, its runs after a failed one, and its `sends` and
  `needs` once composed:** the driver's checks cover only the rules in
  its [table](checks.md#the-rules-by-role).
- **How a part uses the seed:** that an executor takes each shot's stream
  from `seed.shot(id)`, and that a part that draws uses a `derive` key no
  other part uses. A part that draws nothing may rightly ignore the seed,
  so no check asks that records change with it; and whether a key is
  shared shows only across parts, where a check sees one. That an adapter
  or a driver passes the seed below unchanged is checked
  (`passes_the_seed_on`). Nor how an executor makes a shot from its
  needs: every run of a check has the same needs.
- **A part that is not replayable, on runs that send its shots
  otherwise:** a part whose offer is not replayable
  ([`Offer::not_replayable`](../../executor_protocol/src/offer.rs))
  may draw a run's shots from one stream, so its records may depend on
  the shots run beside it and on how the run puts them in batches and
  `execute`s. No check compares the records of such runs for it:
  `gives_each_shot_what_it_gives_alone`,
  `continues_each_shot_across_executes` and
  `changes_no_shot_outside_its_batches` pass it on their rule, and
  `starts_each_shot_afresh` and `does_nothing_until_polled` skip their
  check that the instructions given can show a broken rule. Each says so
  on the standard error in trial 0.
- **A part that is not seedable:** every run of a check fixes its seed,
  so that the check can compare runs and replay a trial, and the chain
  refuses a fixed seed when a part's offer is not seedable
  ([`Offer::not_seedable`](../../executor_protocol/src/offer.rs)). So
  every check of an executor or an adapter that is not seedable, as a
  device's runtime whose randomness is physical often is not, reports
  that it cannot judge the part, naming it. Check a seedable build of the
  part, if it has one, such as a simulator of the device.
- **An executor's conditions across `execute`s:** an executor raises its
  own conditions on a shot from its state, so the kit cannot tell one
  raised anew from one carried from an earlier `execute`; that a row
  reports only its own `execute`'s conditions is checked on an adapter
  (`reports_only_each_executes_conditions`).
- **An adapter's call below made before its own future is polled, and
  never polled:** the kit's executor below does nothing until polled, so
  such a call changes no record, and the adapter's
  `does_nothing_until_polled` passes it.
- **What a part does after a failure below, or an `execute` dropped while
  it runs:** the kit's executor fails no `execute` it runs, and a check
  drops a part's future only before its first poll, so a caller that
  reads its readouts after an error passes, and so does a part whose
  `end_shots`, after an `execute` dropped while it ran, neither cancels
  that work nor ignores its readouts.
- **A caller that sends a shot again after ending it, or after it
  failed:** the kit's executor answers every shot it is sent, ended or
  not, and fails no `execute`, so a driver or an adapter that ends a shot
  below and sends it again passes.
- **Faults keyed to rare patterns:** a part that breaks a rule only on
  patterns the drawn trials rarely reach (a given set of ids, a rare cut
  or grouping) passes the default trials; more seeds or trials find it,
  as [Running more trials](trials.md#running-more-trials) says.
- **Faults on inputs the test does not give:** the drawn trials redraw
  the shots' ids, order, batches and groups, where a run fails, the
  conditions raised below and the seeds, never the instructions or the
  number of shots; the ids of the shots sent are below 2¹⁶ (or 64 per
  shot, if more), or at an edge: pairs that agree on their low 16 bits
  (`k` and `k + 2¹⁶`), and the ids just below 2¹⁶, from 2¹⁶ up, and up to
  `u32::MAX` (`NEAR_AN_EDGE` of each), so that a part fitted to small ids
  (a `u64` bit mask), or to ids a `u16` holds, shows
  ([`src/trial_patterns.rs`](../src/trial_patterns.rs)
  says how they are drawn); and a run sends at most four times the
  script's `execute`s, or the two lists' (a drawn trial splits some in
  two, and a check may split each in two again or send each twice). A
  part that breaks a rule only on other instructions, more shots, other
  ids or a longer run passes at any number of trials. Give a check a
  script that holds them.
- **A part's runs after its own error or panic:**
  `runs_as_new_after_a_failed_run` fails a run above the part, as the kit
  cannot make a part fail; a part that recovers from a failure above it
  but not from its own passes.
- **A chain composed with `>>`:** a composed chain's parts get ready for
  the needs that cross the chain's own boundaries, which `>>` keeps
  through hooks a check's wrapper cannot forward; check each part of a
  chain on its own.
- **Planned: the rules on refusals that may be sent again** (a refusal
  means nothing ran), which wait on a planned networking layer of the
  protocol, which will define a refusal's retry class. A remote executor
  would need them.
