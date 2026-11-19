// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Rule`]: the one list of the rules the kit checks, each with the
//! sentence a report states it by and where the protocol states it. A new
//! rule is a new variant here and a new check in its role's module.

use std::fmt;

/// A rule a part keeps, and which the kit checks: each check names its
/// own; every check of an executor or an adapter also reports the shape
/// of a batch's readouts and how it ends shots it does not hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Rule {
    /// An executor or adapter writes every readout value of every batch it
    /// answers: [`executor::writes_every_readout`](crate::executor::writes_every_readout),
    /// and the adapter's check of the same name.
    WritesEveryReadout,
    /// An executor or adapter answers each batch with readouts of the
    /// batch's shape, one row per shot of the batch's readout slots.
    ///
    /// The part above reads row `i` as the batch's `i`-th shot, and slot
    /// `j` as its `j`-th readout, so readouts of another shape give it
    /// another shot's values, or none. No check is named for this rule:
    /// every check that reads readouts reports it, once per place and
    /// shape however many runs show it. An executor that adds a row to each
    /// answer is reported as `execute 0, batch 0: readouts of 5 rows of 2
    /// slots, for a batch of 4 shots of 2 slots`.
    KeepsEachBatchsShape,
    /// An executor or adapter does nothing for shots it does not hold, or
    /// no longer holds, in `end_shots`.
    ///
    /// The runner ends every shot of a run once it ends: also those the
    /// driver ended earlier, and those the driver never sent. A part that
    /// expects every shot it is told to end to be held breaks such runs.
    /// No check is named for this rule: the kit's driver ends each shot
    /// after its last `execute`, and a run holds a shot the driver never
    /// sends ([`Script`](crate::Script) says when), so every check of an
    /// executor or an adapter reports each `end_shots` that panicked
    /// ending shots among which some the part did not hold, in each of its
    /// runs that ends or that the check fails on purpose, except in the
    /// run where a check drops the part and
    /// withholds its ends: run 4 of
    /// [`executor::starts_each_shot_afresh`](crate::executor::starts_each_shot_afresh),
    /// and of the adapter's check of the same name. The kit cannot tell
    /// which shot a panic came from, so a panic that a held shot of such an
    /// `end_shots` caused is reported under this rule too; the report
    /// quotes the panic. An `end_shots` whose every shot the part held is
    /// not reported: its
    /// panic fails the run, or is raised again from it, as the protocol
    /// says. An executor that expects each shot it ends to be held, under a
    /// script whose shots all end at execute 1, is reported as
    /// `` `end_shots` after execute 1: panicked ending shots 0, 1, 2, 3, 57,
    /// among which shots 0, 1, 2, 3, 57 it did not hold, or no longer held:
    /// shot 0 is not held ``, the runner ending again the shots the driver
    /// ended, and shot 57, which it never sent.
    IgnoresEndsOfShotsItDoesNotHold,
    /// For a replayable part, a shot's records depend only on the seed and
    /// the shot's id:
    /// [`executor::gives_each_shot_what_it_gives_alone`](crate::executor::gives_each_shot_what_it_gives_alone),
    /// and the adapter's check of the same name.
    GivesEachShotWhatItGivesAlone,
    /// A part keeps a shot's state from one `execute` to the next:
    /// [`executor::continues_each_shot_across_executes`](crate::executor::continues_each_shot_across_executes),
    /// and the adapter's check of the same name.
    ContinuesEachShotAcrossExecutes,
    /// An `execute` changes no shot outside its batches:
    /// [`executor::changes_no_shot_outside_its_batches`](crate::executor::changes_no_shot_outside_its_batches),
    /// and the adapter's check of the same name.
    ChangesNoShotOutsideItsBatches,
    /// A part makes each shot afresh at its first batch, whatever an
    /// earlier part left: [`executor::starts_each_shot_afresh`](crate::executor::starts_each_shot_afresh),
    /// and the adapter's check of the same name.
    StartsEachShotAfresh,
    /// A part does an `execute`'s work only once its future is polled:
    /// [`executor::does_nothing_until_polled`](crate::executor::does_nothing_until_polled),
    /// and the adapter's check of the same name.
    DoesNothingUntilPolled,
    /// An executor's `accepts` and `offers` do not change once it is
    /// composed: [`executor::keeps_its_accepts_and_offers`](crate::executor::keeps_its_accepts_and_offers).
    KeepsItsAcceptsAndOffers,
    /// After a failed run, a part runs the same shots again as a new part
    /// would:
    /// [`executor::runs_as_new_after_a_failed_run`](crate::executor::runs_as_new_after_a_failed_run),
    /// and the adapter's check of the same name.
    RunsAsNewAfterAFailedRun,
    /// An adapter sends below only shots of the batches it got:
    /// [`adapter::sends_only_shots_it_got`](crate::adapter::sends_only_shots_it_got).
    SendsOnlyShotsItGot,
    /// An adapter or a driver passes the seed below unchanged:
    /// [`adapter::passes_the_seed_on`](crate::adapter::passes_the_seed_on),
    /// [`driver::passes_the_seed_on`](crate::driver::passes_the_seed_on).
    PassesTheSeedOn,
    /// A sender states each batch's readout slots as its instructions count
    /// them: [`adapter::states_readout_slots`](crate::adapter::states_readout_slots),
    /// [`driver::states_readout_slots`](crate::driver::states_readout_slots).
    StatesReadoutSlots,
    /// An adapter merges into each row the conditions read below on its
    /// shot in the same `execute`, and no other shot's, and raises none
    /// because of one read below:
    /// [`adapter::passes_each_shots_conditions_up`](crate::adapter::passes_each_shots_conditions_up).
    PassesEachShotsConditionsUp,
    /// An adapter's row reports only its own `execute`'s conditions, and
    /// none raised because of an earlier one's:
    /// [`adapter::reports_only_each_executes_conditions`](crate::adapter::reports_only_each_executes_conditions).
    ReportsOnlyEachExecutesConditions,
    /// An adapter sends checked batches below for a checked batch:
    /// [`adapter::carries_checked_batches_below`](crate::adapter::carries_checked_batches_below).
    CarriesCheckedBatchesBelow,
    /// A caller puts a shot in at most one batch of a call to the part
    /// below's `execute`, which
    /// [`adapter::sends_only_shots_it_got`](crate::adapter::sends_only_shots_it_got)
    /// and [`driver::sends_only_its_shots`](crate::driver::sends_only_its_shots)
    /// check.
    ///
    /// The part below may run a call's batches in any order, or at once
    /// (`Execute::execute`), so a shot in two batches of one call gets its
    /// two sets of instructions in an order the caller cannot know.
    /// Each batch below holding a shot an earlier batch of its call
    /// holds is reported; a driver that puts shot 0 in both batches of
    /// its second `execute` is reported as `execute 1, call 0 below, batch
    /// 1: holds shot 0 that an earlier batch of its call holds`.
    PutsEachShotInOneBatchPerCall,
    /// A driver gives one output per shot, in the order of the run's shots,
    /// each the one its shot gives alone:
    /// [`driver::gives_one_output_per_shot_in_order`](crate::driver::gives_one_output_per_shot_in_order).
    GivesOneOutputPerShotInOrder,
    /// A driver sends only the run's shots:
    /// [`driver::sends_only_its_shots`](crate::driver::sends_only_its_shots).
    SendsOnlyItsShots,
    /// A driver merges each `execute`'s conditions into the shot's output,
    /// and no other shot's, and raises none because of one read below:
    /// [`driver::merges_each_executes_conditions`](crate::driver::merges_each_executes_conditions).
    MergesEachExecutesConditions,
}

impl Rule {
    /// The rule, as a sentence about the part, and where the protocol
    /// states it.
    fn statement(self) -> &'static str {
        match self {
            Rule::WritesEveryReadout => {
                "it writes every readout value of each batch it answers (`Execute::execute`)"
            }
            Rule::KeepsEachBatchsShape => {
                "it answers each batch with readouts of the batch's shape (`Execute::execute`)"
            }
            Rule::IgnoresEndsOfShotsItDoesNotHold => {
                "its `end_shots` does nothing for shots it does not hold, or no longer holds \
                 (`Execute::end_shots`)"
            }
            Rule::GivesEachShotWhatItGivesAlone => {
                "for a replayable part, a shot's records depend only on the seed and the shot's \
                 id (`Seed`, `Offer::replayable`)"
            }
            Rule::ContinuesEachShotAcrossExecutes => {
                "it keeps a shot's state from one `execute` to the next, until its end \
                 (`Execute::execute`)"
            }
            Rule::ChangesNoShotOutsideItsBatches => {
                "an `execute` changes no shot outside its batches (`Execute::execute`)"
            }
            Rule::StartsEachShotAfresh => {
                "it makes each shot afresh at its first batch, whatever an earlier part left \
                 (`Execute::execute`)"
            }
            Rule::DoesNothingUntilPolled => {
                "it does an `execute`'s work only once its future is polled \
                 (`Execute::execute`)"
            }
            Rule::KeepsItsAcceptsAndOffers => {
                "its `accepts` and `offers` do not change once it is composed \
                 (`ExecutorSetup::accepts`, `ExecutorSetup::offers`)"
            }
            Rule::RunsAsNewAfterAFailedRun => {
                "after a failed run, it runs the same shots again as a new part would \
                 (`Failure::Panicked`, `Seed`)"
            }
            Rule::SendsOnlyShotsItGot => {
                "it sends below only shots of the batches it got (`Adapt::adapt`)"
            }
            Rule::PassesTheSeedOn => "it passes the seed below unchanged (`Seed`)",
            Rule::StatesReadoutSlots => {
                "it states each batch's readout slots as its instructions count them \
                 (`Batch::new`)"
            }
            Rule::PassesEachShotsConditionsUp => {
                "it merges into each row the conditions read below on its shot in the same \
                 `execute`, and no other shot's, and raises none because of one read below \
                 (`ShotConditions`)"
            }
            Rule::ReportsOnlyEachExecutesConditions => {
                "each row reports only its own `execute`'s conditions, and none raised because \
                 of an earlier one's (`ShotConditions`)"
            }
            Rule::CarriesCheckedBatchesBelow => {
                "it sends checked batches below for a checked batch (`Adapt::adapt`)"
            }
            Rule::PutsEachShotInOneBatchPerCall => {
                "it puts a shot in at most one batch of a call to the part below's `execute` \
                 (`Execute::execute`)"
            }
            Rule::GivesOneOutputPerShotInOrder => {
                "it gives one output per shot, in the order of the run's shots, each the one its \
                 shot gives alone (`Drive::drive`, `Seed`)"
            }
            Rule::SendsOnlyItsShots => "it sends only the run's shots (`Drive::drive`)",
            Rule::MergesEachExecutesConditions => {
                "it merges each `execute`'s conditions into the shot's output, and no other \
                 shot's, and raises none because of one read below (`ShotConditions`)"
            }
        }
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.statement())
    }
}
