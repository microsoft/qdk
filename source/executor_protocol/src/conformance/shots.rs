// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Checks of the rules of shots: a shot is its own, alone or among others,
//! continued from one call to the next, changed only by a call that names
//! it, started as made whatever an earlier executor left, and keeping its
//! conditions from one call to the next.

use std::collections::BTreeSet;

use super::readouts::{Readouts, expect_on_each_shot, readouts, readouts_of_calls, tagged};
use crate::{Close, Executor, InstructionBatch, ShotConditions, ShotId, ShotSet};

/// Runs `batches` on `shots` of an executor that `make` builds, then each
/// shot of `shots` alone on another, then `shots` split into calls in
/// another order on two more, each batch in two calls: on one, the shots at
/// odd indices, then those at even indices; on the other, the shots in
/// reverse order, in two halves. Both splits run whatever the number of
/// batches, one batch included. Each shot's readouts must be the same,
/// value for value, with the same conditions, since an executor that
/// offers `replayable` draws each shot's random numbers from that shot's
/// own seeds, and a shot's conditions are its own, so which shots share
/// its calls, and in which order they come, changes nothing. `make` must
/// build the same executor each time, with the same seed; `shots` must
/// hold three shots or more, since both splits of two shots give the same
/// calls; `batches` must make a readout, and the shots' readouts differ
/// between some two shots, so that there is something to compare and a
/// generator shared by the shots would show: choose shots and batches that
/// draw differently (a coin, on enough shots that two of them toss
/// differently). For an executor that offers `replayable`.
///
/// # Panics
///
/// When a shot's readouts alone, or with the shots split into calls,
/// differ from its readouts among all the shots; when a call of `batches`
/// is refused, or its reply fails, or changes the events the caller held
/// (a reply appends its own); when `shots` holds fewer than three
/// shots, `batches` make no readout, or every shot reads the same.
pub fn gives_each_shot_alone_what_it_gives_among_others<I, E: Executor<InstructionBatch<I>>>(
    mut make: impl FnMut() -> E,
    shots: &ShotSet,
    batches: &[&[I]],
) {
    assert!(
        shots.len() > 2,
        "`shots` must hold three shots or more, so that each runs among others and the two \
         splits give different calls"
    );
    let among = readouts(&mut make(), shots, batches);
    assert!(
        among.values().any(|readouts| !readouts.is_empty()),
        "`batches` must make a readout, so that the shots' readouts can be compared"
    );
    let of = |readouts: &Readouts, shot| readouts.get(&shot).cloned().unwrap_or_default();
    expect_on_each_shot(
        shots,
        |shot| of(&readouts(&mut make(), &ShotSet::one(shot), batches), shot) == of(&among, shot),
        |on| {
            format!(
                "each shot alone must give what it gives among the shots {:?}, but it gives \
                 otherwise on {on}",
                shots.as_slice()
            )
        },
    );
    for (parts, how) in splits(shots) {
        let split = readouts_of_calls(&mut make(), &split_into_calls(&parts, batches));
        expect_on_each_shot(
            shots,
            |shot| of(&split, shot) == of(&among, shot),
            |on| {
                format!(
                    "each shot must give what it gives among the shots {:?} when they are \
                     split into calls {how}, but it gives otherwise on {on}",
                    shots.as_slice()
                )
            },
        );
    }
    // Checked last, so that shots that read alike because they share one
    // state are told by the rule they break, not by this need.
    let first = of(&among, shots.as_slice()[0]);
    assert!(
        shots.iter().any(|&shot| of(&among, shot) != first),
        "the shots' readouts must differ between some two shots, so that a generator they \
         share would show: choose shots and batches that draw differently"
    );
}

/// The two ways to split `shots` into two calls in another order, each
/// with how it splits them: the shots at odd indices then those at even
/// indices, which catches an executor that takes a call's shots for
/// contiguous when `shots` is in increasing order; and the shots in
/// reverse order, in two halves, which catches one that takes them for
/// sorted. Both split `shots` in the order given, so that a caller's order
/// gives other calls. `shots` holds three shots or more, so no part is
/// empty, and the two splits differ.
fn splits(shots: &ShotSet) -> [([ShotSet; 2], &'static str); 2] {
    let part = |shots: Vec<ShotId>| ShotSet::new(shots).expect("a part of `shots`, not empty");
    let reversed: Vec<ShotId> = shots.iter().rev().copied().collect();
    let (back, front) = reversed.split_at(reversed.len() / 2);
    [
        (
            at_odd_and_even_indices(shots),
            "at odd indices, then at even ones",
        ),
        (
            [back.to_vec(), front.to_vec()].map(part),
            "in reverse order, in two halves",
        ),
    ]
}

/// The shots of `shots` at odd indices, then those at even indices: two
/// parts that are not contiguous once `shots` holds four shots or more in
/// increasing order, since each shot of a part then lies between two
/// shots of the other. `shots` holds two shots or more, so neither part is
/// empty.
fn at_odd_and_even_indices(shots: &ShotSet) -> [ShotSet; 2] {
    let (odd, even): (Vec<_>, Vec<_>) =
        (shots.iter().enumerate()).partition(|&(at, _)| at % 2 == 1);
    [odd, even].map(|part| {
        ShotSet::new(part.into_iter().map(|(_, &shot)| shot).collect())
            .expect("a part of `shots`, not empty")
    })
}

/// The calls of `batches`, each in two calls, on the two `parts` of the
/// shots, each given with the index of its batch, under which its readouts
/// are compared.
fn split_into_calls<'a, I>(
    parts: &[ShotSet; 2],
    batches: &[&'a [I]],
) -> Vec<(usize, ShotSet, &'a [I])> {
    (batches.iter().enumerate())
        .flat_map(|(call, &batch)| parts.iter().map(move |shots| (call, shots.clone(), batch)))
        .collect()
}

/// Runs `first`, then `second`, in two calls on `shots` of an executor that
/// `make` builds, and both in one batch on `shots` of another: each shot's
/// readouts must be the same, since an executor that offers `resumable_shots`
/// keeps each shot's state from one call to the next, so the second call
/// continues the shot where the first left it. With two shots or more, two
/// more executors run `first`, then `second` on other shots than `first`,
/// so that an executor that makes again shots a call names, when the call
/// names other shots than the one before it, would show:
///
/// - one runs `first` on all the shots, then `second` in two calls, on the
///   shots at odd indices, then on those at even indices: each shot must
///   read as over two calls on all of them;
/// - the other runs `first` on the shots at odd indices, then `second` on
///   all the shots, so that a call names shots held and shots new to the
///   executor together: each shot at an odd index must read as over two
///   calls on all of them, and each at an even index as after `second`
///   alone, since a new shot starts as made.
///
/// A call that changes a shot it does not name is checked by
/// [`changes_no_shot_a_call_does_not_name`]. `make` must build the same
/// executor each time, with the same seed; `second` must make a readout,
/// under other tags than `first`'s, so that one batch of both is accepted,
/// and read what `first` changed on each shot: `second` alone, on another
/// executor, must read otherwise on each shot than after `first` in one
/// batch of both (a flip, then a read), so that an executor that starts a
/// shot again at a call would show. For an executor that offers
/// `resumable_shots`; if it does not offer `replayable`, choose batches that
/// draw nothing, since its draws may depend on how the shots' instructions are
/// split into calls.
///
/// # Panics
///
/// When a shot's readouts over two calls differ from its readouts in one
/// batch, or from its readouts when `second` names part of the shots, or
/// names them with new ones; when a new shot reads otherwise than alone
/// after `second` names it with shots that `first` named; when a call is
/// refused, or its reply fails, or changes the events the caller held (a
/// reply appends its own); when `second` makes no readout, reads under a
/// tag of `first`, or reads the same alone as after `first` on some shot.
pub fn continues_a_shot_from_one_call_to_the_next<I: Clone, E: Executor<InstructionBatch<I>>>(
    mut make: impl FnMut() -> E,
    shots: &ShotSet,
    first: &[I],
    second: &[I],
) {
    let split = readouts(&mut make(), shots, &[first, second]);
    assert!(
        split
            .values()
            .any(|readouts| readouts.iter().any(|&(call, ..)| call == 1)),
        "`second` must make a readout, so that it shows the state the first call left"
    );
    let tags_of_call = |shot, of| tags(&split, shot, |call| call == of);
    expect_other_tags(
        shots,
        |shot| tags_of_call(shot, 0),
        |shot| tags_of_call(shot, 1),
    );
    let alone = tagged(readouts(&mut make(), shots, &[second]));
    let both: Vec<I> = first.iter().chain(second).cloned().collect();
    let (split, joined) = (
        tagged(split),
        tagged(readouts(&mut make(), shots, &[&both])),
    );
    expect_on_each_shot(
        shots,
        |shot| split.get(&shot) == joined.get(&shot),
        |on| {
            format!(
                "a second call must continue each shot where the first left it: its readouts \
                 over two calls must be those of one batch of both, but they differ on {on}"
            )
        },
    );
    if shots.len() > 1 {
        let rule = "a call must continue each shot it names where the calls before it left it, \
                    or as made if none of them named it, whatever shots they named";
        let [odd, even] = at_odd_and_even_indices(shots);
        let parted = tagged(readouts_of_calls(
            &mut make(),
            &[
                (0, shots.clone(), first),
                (1, odd.clone(), second),
                (1, even.clone(), second),
            ],
        ));
        expect_on_each_shot(
            shots,
            |shot| parted.get(&shot) == split.get(&shot),
            |on| {
                format!(
                    "{rule}, but a shot reads otherwise when `second` names part of the shots \
                     that `first` named, on {on}"
                )
            },
        );
        let joining = tagged(readouts_of_calls(
            &mut make(),
            &[(0, odd.clone(), first), (1, shots.clone(), second)],
        ));
        expect_on_each_shot(
            &odd,
            |shot| joining.get(&shot) == split.get(&shot),
            |on| {
                format!(
                    "{rule}, but a shot reads otherwise when `second` names it with shots new \
                     to the executor, on {on}"
                )
            },
        );
        expect_on_each_shot(
            &even,
            |shot| joining.get(&shot) == alone.get(&shot),
            |on| {
                format!(
                    "{rule}, but a shot new to the executor reads otherwise than alone when \
                     `second` names it with shots that `first` named, on {on}"
                )
            },
        );
    }
    // Checked last, so that an executor that breaks the rule is told by the
    // rule it breaks.
    expect_on_each_shot(
        shots,
        |shot| {
            let alone = alone.get(&shot).map_or(&[][..], Vec::as_slice);
            let after_first: Vec<_> = (joined.get(&shot).into_iter().flatten())
                .filter(|(tag, _)| alone.iter().any(|(read, _)| read == tag))
                .collect();
            after_first.into_iter().ne(alone)
        },
        |on| {
            format!(
                "`second` must read what `first` changed, so that a shot made again at the \
                 second call would show: alone, it must read otherwise than after `first` on \
                 each shot, but it reads the same on {on}"
            )
        },
    );
}

/// Checks that a call changes no shot it does not name, whether the
/// executor holds that shot yet or not: an executor that offers
/// `resumable_shots` keeps a state per shot, which only a call on that shot
/// changes, so a shot held is left as it was, and a new shot starts fresh (in a
/// new executor too, which
/// [`starts_its_shots_as_made_whatever_an_earlier_executor_left`] checks). The
/// shots of `shots`, taken in increasing order whatever order it gives them in,
/// are split into those at even indices and those at odd indices, which lie
/// between each other. Four runs, each on an executor that `make` builds:
///
/// 1. `first` on the shots at even indices, then on those at odd indices,
///    then `second` on those at even indices, then on those at odd ones;
/// 2. the same calls without the one of `first` on the shots at odd
///    indices;
/// 3. `first` twice on the shots at even indices, then `second` on them;
/// 4. `second` alone on the shots at odd indices.
///
/// Each shot at an even index must read the same in runs 1 and 2, with the
/// same conditions (a call raises none on a shot it does not name): the call
/// on the others changes it neither by running on it (an executor that
/// also runs a call on shots it holds, or on the range of the call's
/// shots) nor by making the others (one that makes every state again when
/// a shot joins). Each shot at an odd index must read the same in runs 2
/// and 4: the call on the others, which ran before its first call, left it
/// new (an executor that also runs a call on the shots in its range it
/// does not hold yet, or that makes a new shot from a template its calls
/// change). These show provided that `first` changes a shot each time it
/// runs (a flip, not a set) and `second` reads that change: on each shot at
/// an even index, `second` must read otherwise in run 3 than in run 2, and
/// on each shot at an odd index, otherwise in run 1 than in run 4. `make`
/// must build the same executor each time, with the same seed; `shots`
/// must hold four shots or more, so that the shots at odd indices are not
/// contiguous and an executor that runs a call on the range of its shots
/// would show. For an executor that offers `resumable_shots`; if it does not
/// offer `replayable`, choose batches that draw nothing, since its draws may
/// depend on how the shots' instructions are split into calls.
///
/// # Panics
///
/// When a shot at an even index reads otherwise in the run with the call on
/// the shots at odd indices than in the run without it, or a shot at an odd
/// index reads otherwise after the call on the others than alone; when a
/// call is refused, or its reply fails, or changes the events the caller
/// held (a reply appends its own); when `shots` holds fewer than four
/// shots, or `second` reads the same after `first` twice as after `first`
/// once on some shot at an even index, or the same after `first` as alone
/// on some shot at an odd index.
pub fn changes_no_shot_a_call_does_not_name<I, E: Executor<InstructionBatch<I>>>(
    mut make: impl FnMut() -> E,
    shots: &ShotSet,
    first: &[I],
    second: &[I],
) {
    assert!(
        shots.len() > 3,
        "`shots` must hold four shots or more, so that the shots at odd indices are not \
         contiguous"
    );
    let mut increasing = shots.to_vec();
    increasing.sort_unstable();
    let increasing = ShotSet::new(increasing).expect("the shots of a set, each named once");
    let [odd, even] = at_odd_and_even_indices(&increasing);
    // `second` runs on each part in a call of its own, so that the only call
    // that brings shots new to the executor between the calls on the shots
    // at even indices is the one the rule is checked against.
    let with = readouts_of_calls(
        &mut make(),
        &[
            (0, even.clone(), first),
            (1, odd.clone(), first),
            (2, even.clone(), second),
            (3, odd.clone(), second),
        ],
    );
    let without = readouts_of_calls(
        &mut make(),
        &[
            (0, even.clone(), first),
            (2, even.clone(), second),
            (3, odd.clone(), second),
        ],
    );
    let alone = readouts_of_calls(&mut make(), &[(3, odd.clone(), second)]);
    let of = |readouts: &Readouts, shot| readouts.get(&shot).cloned().unwrap_or_default();
    expect_on_each_shot(
        &even,
        |shot| of(&with, shot) == of(&without, shot),
        |on| {
            format!(
                "a call must change no shot it does not name, but a shot reads otherwise after \
                 a call on the shots {:?}, on {on}",
                odd.as_slice()
            )
        },
    );
    expect_on_each_shot(
        &odd,
        |shot| of(&without, shot) == of(&alone, shot),
        |on| {
            format!(
                "a call must change no shot it does not name, held or not, so that a new shot \
                 starts fresh, but a shot reads otherwise after a call on the shots {:?} than \
                 alone, on {on}",
                even.as_slice()
            )
        },
    );
    // Checked last, so that an executor that breaks the rule is told by the
    // rule it breaks. Each on the shots its rule is checked on, and on each
    // of them, since a change the rule forbids shows only where `second`
    // reads what `first` changed.
    let twice = readouts_of_calls(
        &mut make(),
        &[
            (0, even.clone(), first),
            (1, even.clone(), first),
            (2, even.clone(), second),
        ],
    );
    let of_call = |readouts: &Readouts, shot, call| {
        let mut read = of(readouts, shot);
        read.retain(|&(made_in, ..)| made_in == call);
        read
    };
    expect_on_each_shot(
        &even,
        |shot| of_call(&twice, shot, 2) != of_call(&without, shot, 2),
        |on| {
            format!(
                "`first` must change a shot each time it runs (a flip, not a set), and \
                 `second` read that change, so that a call run again on a shot it does not \
                 name would show: after `first` twice, `second` must read otherwise than after \
                 `first` once on each shot at an even index, but it reads the same on {on}"
            )
        },
    );
    expect_on_each_shot(
        &odd,
        |shot| of_call(&with, shot, 3) != of(&alone, shot),
        |on| {
            format!(
                "`first` must change a new shot, and `second` read that change, so that a call \
                 that changes a shot not held yet would show: after `first`, `second` must \
                 read otherwise than alone on each shot at an odd index, but it reads the same \
                 on {on}"
            )
        },
    );
}

/// Checks that an executor starts each shot as made, whatever an earlier
/// executor left: what an executor, closed or dropped unclosed, did to its
/// shots does not carry over into the next one that `make` builds, as it
/// would were they made on one device that is reset only when an executor
/// closes, or never. Every check of this kit that builds several executors
/// relies on it, and the others drop theirs unclosed. Each run is on an
/// executor that `make` builds, closed before the next is built, but for
/// run 4's:
///
/// 1. `second` on `shots`;
/// 2. `first` on `shots`;
/// 3. `second` on `shots` again;
/// 4. `first` on `shots`, on an executor dropped unclosed (as a caller
///    that panics or returns early leaves it);
/// 5. `second` on `shots` again;
/// 6. `first` and `second` in one batch on `shots`.
///
/// Each shot must read the same in runs 1, 3 and 5, with the same
/// conditions (conditions an earlier executor raised do not carry over).
/// This shows provided that `first` changes each shot and `second` reads
/// that change: on each shot, `second` must read otherwise in run 6 than
/// in run 1. `make` must build the same executor each time, with the same
/// seed; `second` must read under other tags than `first`'s, so that one
/// batch of both is accepted.
/// It asks neither `resumable_shots` nor `replayable`: each executor runs one
/// call, and runs 1, 3 and 5 the same one.
///
/// # Panics
///
/// When a shot reads otherwise in run 3 or 5 than in run 1; when a call is
/// refused, or its reply fails, or changes the events the caller held (a
/// reply appends its own); when `first` in run 2 and `second` in run 1
/// read under one tag on some shot, or `second` reads the same in run 6 as
/// in run 1 on some shot.
pub fn starts_its_shots_as_made_whatever_an_earlier_executor_left<I, E>(
    mut make: impl FnMut() -> E,
    shots: &ShotSet,
    first: &[I],
    second: &[I],
) where
    I: Clone,
    E: Executor<InstructionBatch<I>> + Close,
{
    /// What becomes of an executor after its run.
    enum Then {
        Close,
        DropUnclosed,
    }
    let mut run = |batch: &[I], then: Then| {
        let mut executor = make();
        let readouts = readouts(&mut executor, shots, &[batch]);
        match then {
            Then::Close => {
                executor.close();
            }
            Then::DropUnclosed => drop(executor),
        }
        readouts
    };
    let before = run(second, Then::Close);
    let changed = run(first, Then::Close);
    let after = run(second, Then::Close);
    let of = |readouts: &Readouts, shot| readouts.get(&shot).cloned().unwrap_or_default();
    expect_on_each_shot(
        shots,
        |shot| of(&after, shot) == of(&before, shot),
        |on| {
            format!(
                "a new executor's shots must start as made, whatever an earlier executor left, \
                 but a shot reads otherwise after an earlier executor ran `first` on it, on \
                 {on}"
            )
        },
    );
    run(first, Then::DropUnclosed);
    let after_dropped = run(second, Then::Close);
    expect_on_each_shot(
        shots,
        |shot| of(&after_dropped, shot) == of(&before, shot),
        |on| {
            format!(
                "a new executor's shots must start as made, whatever an earlier executor left, \
                 but a shot reads otherwise after an earlier executor ran `first` on it and was \
                 dropped unclosed, on {on}"
            )
        },
    );
    // The needs are checked last, so that an executor that breaks the rule
    // is told by the rule it breaks; the tags before the batch of both, which
    // repeats a tag they share: an executor that refuses it would fail the
    // run with another message, and one that runs it could pass the next
    // need for nothing, with `first`'s readout under that tag.
    expect_other_tags(
        shots,
        |shot| tags(&changed, shot, |_| true),
        |shot| tags(&before, shot, |_| true),
    );
    let both: Vec<I> = first.iter().chain(second).cloned().collect();
    let joined = run(&both, Then::Close);
    expect_on_each_shot(
        shots,
        |shot| {
            let alone = of(&before, shot);
            let mut after_first = of(&joined, shot);
            after_first.retain(|&(_, tag, ..)| alone.iter().any(|&(_, read, ..)| read == tag));
            after_first != alone
        },
        |on| {
            format!(
                "`second` must read what `first` changed, so that a shot left changed by an \
                 earlier executor would show: after `first`, in one batch of both, it must \
                 read otherwise than alone on each shot, but it reads the same on {on}"
            )
        },
    );
}

/// Runs `batches` on `shots` of `executor`, one call each, in order, and
/// checks that each shot keeps its conditions from one call to the next:
/// each outcome of a shot carries every condition that the shot's outcomes
/// in earlier calls carried, since a condition, once raised, stays raised
/// for the shot (`docs/replies.md`, §Shot conditions). Within a call, a
/// shot's outcomes may carry different conditions, as an executor written
/// by hand may give them, marking only the readouts after it found one; a
/// caller takes their union. `batches` must give some shot an outcome that
/// carries a condition in a call before a later call that reads that shot,
/// so that there is a condition to keep. The protocol requires it of an
/// executor that offers `resumable_shots`, on which a shot continues from
/// one call to the next; it checks any executor that promises to keep each
/// shot's conditions, such as an `adapter_fn` over an executor whose
/// replies end within their call, or over one that keeps its shots'
/// conditions as the protocol requires, apart from a known issue
/// (`docs/replies.md`, §Shot conditions). It sees outcomes alone, so it
/// checks only the conditions that an earlier outcome of the shot carried:
/// a condition raised in a call that gave the shot no outcome, which the
/// requirement covers too, is beyond it. That a shot's conditions are its
/// own, whichever shots share its calls, is checked with its readouts by
/// [`gives_each_shot_alone_what_it_gives_among_others`] and
/// [`changes_no_shot_a_call_does_not_name`]; that a new executor starts
/// its shots with none, by
/// [`starts_its_shots_as_made_whatever_an_earlier_executor_left`].
///
/// # Panics
///
/// When an outcome of a shot lacks a condition that an outcome of the shot
/// in an earlier call carried, naming every such shot; when a call is
/// refused, or its reply fails, or changes the events the caller held (a
/// reply appends its own); when no shot has an outcome with a condition
/// followed by an outcome in a later call: `batches` raise none, or the
/// executor gives none on its outcomes.
pub fn keeps_each_shots_conditions_from_one_call_to_the_next<I, E>(
    executor: &mut E,
    shots: &ShotSet,
    batches: &[&[I]],
) where
    E: Executor<InstructionBatch<I>>,
{
    let readouts = readouts(executor, shots, batches);
    let mut kept_any = false;
    expect_on_each_shot(
        shots,
        |shot| {
            let mut earlier = ShotConditions::NONE;
            let mut this_call = ShotConditions::NONE;
            let mut call_at = None;
            let mut keeps = true;
            for &(call, _, _, conditions) in readouts.get(&shot).into_iter().flatten() {
                if call_at != Some(call) {
                    earlier |= this_call;
                    this_call = ShotConditions::NONE;
                    call_at = Some(call);
                }
                kept_any |= !earlier.is_empty();
                keeps &= conditions.contains(earlier);
                this_call |= conditions;
            }
            keeps
        },
        |on| {
            format!(
                "each outcome of a shot must carry the conditions its outcomes in earlier calls \
                 carried, since a condition stays raised for its shot, but one drops a \
                 condition on {on}"
            )
        },
    );
    assert!(
        kept_any,
        "`batches` must give some shot an outcome that carries a condition before a later \
         call reads that shot, so that there is a condition to keep, but no outcome of a \
         shot carries one before its outcomes in a later call: `batches` raise none, or the \
         executor gives none on its outcomes"
    );
}

/// The need of the checks that run `first` and `second` in one batch: on
/// each shot of `shots`, the tags they read under, `first_tags` and
/// `second_tags` of the shot, are disjoint, so that one batch of both
/// repeats no tag and is accepted.
///
/// # Panics
///
/// When a tag is in both on some shot, naming every such shot.
fn expect_other_tags<'s>(
    shots: impl IntoIterator<Item = &'s ShotId>,
    first_tags: impl Fn(ShotId) -> BTreeSet<u32>,
    second_tags: impl Fn(ShotId) -> BTreeSet<u32>,
) {
    expect_on_each_shot(
        shots,
        |shot| first_tags(shot).is_disjoint(&second_tags(shot)),
        |on| {
            format!(
                "`first` and `second` must use different tags, so that one batch of both is \
                 accepted, but they read under a tag in both on {on}"
            )
        },
    );
}

/// The tags `shot` reads under in `readouts`, in the batches whose index
/// `of` picks.
fn tags(readouts: &Readouts, shot: ShotId, of: impl Fn(usize) -> bool) -> BTreeSet<u32> {
    (readouts.get(&shot).into_iter().flatten())
        .filter_map(|&(call, tag, ..)| of(call).then_some(tag))
        .collect()
}
