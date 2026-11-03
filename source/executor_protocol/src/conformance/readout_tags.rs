// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Checks of the rules of readout tags: an executor refuses a batch that
//! repeats a tag, at the instruction that repeats it.

use std::collections::BTreeMap;
use std::slice;

use super::instruction_sets::expect_refused_at;
use super::readouts::{expect_on_each_shot, outcomes_of_call};
#[cfg(doc)]
use crate::Rejected;
use crate::{Executor, InstructionBatch, InstructionBatchRef, ShotId, ShotOutcome, ShotSet};

/// Calls `executor`, on `shots`, shots it runs, with batches of `a` and `b`,
/// two instructions it runs, and `a_again`, another instruction it runs, which
/// reads under `a`'s tag, and reads another qubit than `a` where the executor
/// has one, unless `b` reads `a`'s: with `a`'s qubit, and `b` on another, an
/// executor that refuses a repeated qubit, rather than a repeated tag,
/// conforms. This check knows nothing of qubits, so it cannot check that need.
/// `[a]`, `[a]` again in another batch, `[b]`, `[a_again]` and `[a, b]` must be
/// accepted: a later batch may use a tag again, and readouts under different
/// tags may share a batch. `[a, b, a]`, `[b, a, a]`, `[a, a, b]`,
/// `[a, b, b, a]`, `[b, a, a, b]`, `[a, a_again]` and `[a, b, a_again]` must
/// each be rejected with [`Rejected::Instruction`] at the first instruction, in
/// the batch's order, whose readout repeats a tag (2, 2, 1, 2, 2, 1 and 2),
/// since within a batch a tag names one readout of each shot
/// ([`ShotOutcome::tag`](crate::ShotOutcome::tag)), and that is the instruction
/// [`check_unique_tags`](crate::check_unique_tags) refuses: the repeat is of
/// the batch's first tag, of the tag just before it, and in the middle of the
/// batch; with two repeats, the first is of either tag; and, with `a_again`,
/// the repeat is by another instruction. So an executor that compares each tag
/// with the first only, or with the one before it only, that refuses at the
/// batch's last instruction or at the last repeat, that looks for repeats tag
/// by tag, or that refuses a repeated instruction, or a repeated qubit (with
/// `a_again` on another qubit, or `b` on `a`'s), rather than a repeated tag,
/// would show. Each refused batch is called after the refusal of the one
/// before, which leaves the executor usable. The rule also asks that nothing of
/// a refused batch runs, which this check cannot see. It waits for the replies
/// of `[a]`, `[b]` and `[a_again]`: `a` and `b` must each make one readout on
/// each shot, under two different tags, so that a batch that holds one twice
/// repeats a tag and `[a, b]` repeats none; and `a_again`, another instruction
/// than `a`, one under `a`'s tag.
///
/// # Panics
///
/// When a batch is refused that should be accepted, or the other way round,
/// or when `[a, b, a]`, `[b, a, a]`, `[a, a, b]`, `[a, b, b, a]`,
/// `[b, a, a, b]`, `[a, a_again]` or `[a, b, a_again]` is rejected for
/// another reason or at another index; when `[a]` or `[b]` makes other than
/// one readout on some shot, or both read under one tag on some shot, or
/// the reply of one fails; when `a_again` is `a`, or `[a_again]` makes
/// other than one readout under `a`'s tag on some shot.
pub fn refuses_a_tag_repeated_within_a_batch<
    I: Clone + PartialEq,
    E: Executor<InstructionBatch<I>>,
>(
    executor: &mut E,
    shots: &ShotSet,
    a: &I,
    b: &I,
    a_again: &I,
) {
    let accepted = |executor: &mut E, instructions: &[I], rule: &str| {
        let outcomes = outcomes_of_call(executor, shots, instructions).unwrap_or_else(|rejected| {
            panic!("{rule} must be accepted, but it was rejected with `{rejected:?}`")
        });
        tags_by_shot(outcomes)
    };
    let tags_of_a = accepted(executor, slice::from_ref(a), "a batch of one readout");
    accepted(
        executor,
        slice::from_ref(a),
        "a tag used again in a later batch",
    );
    let tags_of_b = accepted(executor, slice::from_ref(b), "a batch of one readout");
    for (tags, batch) in [(&tags_of_a, "[a]"), (&tags_of_b, "[b]")] {
        expect_on_each_shot(
            shots,
            |shot| one_readout(tags, shot).is_some(),
            |on| {
                format!(
                    "`a` and `b` must each make one readout on each shot, so that a batch \
                     that holds one twice repeats a tag, but `{batch}` does not, on {on}"
                )
            },
        );
    }
    expect_on_each_shot(
        shots,
        |shot| one_readout(&tags_of_a, shot) != one_readout(&tags_of_b, shot),
        |on| {
            format!(
                "`a` and `b` must read under different tags, so that `[a, b]` repeats none, \
                 but they read under one tag on {on}"
            )
        },
    );
    expect_a_again_reads_under_the_tag_of_a(executor, shots, a, a_again, &tags_of_a);
    accepted(
        executor,
        &[a.clone(), b.clone()],
        "two readouts under different tags",
    );
    for (instructions, at, batch) in [
        (vec![a.clone(), b.clone(), a.clone()], 2, "`[a, b, a]`"),
        (vec![b.clone(), a.clone(), a.clone()], 2, "`[b, a, a]`"),
        (vec![a.clone(), a.clone(), b.clone()], 1, "`[a, a, b]`"),
        (
            vec![a.clone(), b.clone(), b.clone(), a.clone()],
            2,
            "`[a, b, b, a]`",
        ),
        (
            vec![b.clone(), a.clone(), a.clone(), b.clone()],
            2,
            "`[b, a, a, b]`",
        ),
        (vec![a.clone(), a_again.clone()], 1, "`[a, a_again]`"),
        (
            vec![a.clone(), b.clone(), a_again.clone()],
            2,
            "`[a, b, a_again]`",
        ),
    ] {
        let answer = executor.call(InstructionBatchRef {
            shots,
            instructions: &instructions,
        });
        expect_refused_at(answer, at, "a tag repeated within a batch", batch);
    }
}

/// Calls `executor`, on `shots`, shots it runs, with batches of `a`, an
/// instruction that makes one readout, `a_again`, another instruction that
/// makes one under `a`'s tag, of another qubit than `a` where the executor
/// has one (a need this check cannot check, as it knows nothing of
/// qubits), and `silent`, one that makes none.
/// `[silent, a, silent, a]` and `[silent, a, silent, a_again]` must be
/// rejected with [`Rejected::Instruction`] at the index of the instruction
/// whose readout repeats a tag, among the batch's instructions (3), not
/// among its readouts (1), since a caller changes an instruction
/// ([`check_unique_tags`](crate::check_unique_tags) takes each readout's
/// tag with its instruction's index): `silent` comes before the first
/// readout and between the two, so that an executor that counts readouts,
/// or the instructions from the first readout on, would show; and the
/// repeat is by `a` and by another instruction, so that an executor that
/// refuses a repeated instruction, or a repeated qubit (with `a_again` on
/// another qubit), rather than a repeated tag, would show.
/// [`refuses_a_tag_repeated_within_a_batch`] checks the rest of the rule,
/// with instructions that each make a readout; this check is for an
/// executor whose set has an instruction that makes none. It waits for the
/// replies of `[a]`, `[silent]`, `[a_again]` and `[silent, a, silent]`,
/// which must be accepted, so that a refusal of a batch that adds a second
/// readout under `a`'s tag can only be the repeat's: `[a]` must make one
/// readout on each shot, so that a batch that holds it twice repeats a
/// tag; `[silent]` none, so that the index of a readout differs from its
/// instruction's; and `a_again`, another instruction than `a`, one under
/// `a`'s tag.
///
/// # Panics
///
/// When `[silent, a, silent, a]` or `[silent, a, silent, a_again]` is
/// accepted, or rejected for another reason or at another index; when
/// `[a]`, `[silent]`, `[a_again]` or `[silent, a, silent]` is refused, or
/// its reply fails; when `[a]` makes other than one readout on some shot,
/// or `[silent]` makes one; when `a_again` is `a`, or `[a_again]` makes
/// other than one readout under `a`'s tag on some shot.
pub fn refuses_a_repeated_tag_at_the_index_of_its_instruction<
    I: Clone + PartialEq,
    E: Executor<InstructionBatch<I>>,
>(
    executor: &mut E,
    shots: &ShotSet,
    a: &I,
    silent: &I,
    a_again: &I,
) {
    let readouts = |executor: &mut E, instructions: &[I], batch: &str| {
        let outcomes = outcomes_of_call(executor, shots, instructions).unwrap_or_else(|rejected| {
            panic!(
                "`{batch}` must be accepted, so that a refusal of a batch that adds a second \
                 readout under `a`'s tag can only be the repeat's, but it was rejected with \
                 `{rejected:?}`"
            )
        });
        tags_by_shot(outcomes)
    };
    let of_a = readouts(executor, slice::from_ref(a), "[a]");
    let of_silent = readouts(executor, slice::from_ref(silent), "[silent]");
    let count = |tags: &BTreeMap<ShotId, Vec<u32>>, shot| tags.get(&shot).map_or(0, Vec::len);
    expect_on_each_shot(
        shots,
        |shot| count(&of_a, shot) == 1,
        |on| {
            format!(
                "`a` must make one readout on each shot, so that a batch that holds it twice \
                 repeats a tag, but `[a]` does not, on {on}"
            )
        },
    );
    expect_on_each_shot(
        shots,
        |shot| count(&of_silent, shot) == 0,
        |on| {
            format!(
                "`silent` must make no readout, so that the index of a readout differs from \
                 its instruction's, but `[silent]` makes some on {on}"
            )
        },
    );
    expect_a_again_reads_under_the_tag_of_a(executor, shots, a, a_again, &of_a);
    readouts(
        executor,
        &[silent.clone(), a.clone(), silent.clone()],
        "[silent, a, silent]",
    );
    for (again, batch) in [
        (a, "`[silent, a, silent, a]`"),
        (a_again, "`[silent, a, silent, a_again]`"),
    ] {
        let answer = executor.call(InstructionBatchRef {
            shots,
            instructions: &[silent.clone(), a.clone(), silent.clone(), again.clone()],
        });
        expect_refused_at(
            answer,
            3,
            "a tag repeated within a batch, after an instruction that makes no readout,",
            batch,
        );
    }
}

/// The tags of `outcomes`, shot by shot, in the order they came.
fn tags_by_shot(outcomes: Vec<ShotOutcome>) -> BTreeMap<ShotId, Vec<u32>> {
    let mut tags = BTreeMap::<ShotId, Vec<u32>>::new();
    for outcome in outcomes {
        tags.entry(outcome.shot).or_default().push(outcome.tag);
    }
    tags
}

/// The tag of `shot`'s readout in `tags`, when it has one readout.
fn one_readout(tags: &BTreeMap<ShotId, Vec<u32>>, shot: ShotId) -> Option<u32> {
    let tags = tags.get(&shot)?;
    <[u32; 1]>::try_from(tags.as_slice()).ok().map(|[tag]| tag)
}

/// The need of both tag checks on `a_again`: another instruction than `a`,
/// which makes one readout on each shot, under `a`'s tag, so that a batch
/// that holds `a` and `a_again` repeats a tag, but no instruction, and an
/// executor that refuses a repeated instruction, or a repeated qubit when
/// `a_again` reads another qubit than `a` (which this cannot check),
/// rather than a repeated tag, would show. Calls `[a_again]` on `shots`
/// of `executor` and waits for its reply. The last argument holds the tags
/// of `[a]`'s readouts, which its check has found one on each shot.
///
/// # Panics
///
/// When `a_again` is `a`; when `[a_again]` is refused, or its reply fails,
/// or it makes other than one readout on some shot, or one under another
/// tag than `a`'s.
fn expect_a_again_reads_under_the_tag_of_a<
    I: Clone + PartialEq,
    E: Executor<InstructionBatch<I>>,
>(
    executor: &mut E,
    shots: &ShotSet,
    a: &I,
    a_again: &I,
    tags_of_a: &BTreeMap<ShotId, Vec<u32>>,
) {
    assert!(
        a_again != a,
        "`a_again` must be another instruction than `a`, so that a batch that holds both \
         repeats a tag, but no instruction"
    );
    let outcomes =
        outcomes_of_call(executor, shots, slice::from_ref(a_again)).unwrap_or_else(|rejected| {
            panic!("`[a_again]` must be accepted, but it was rejected with `{rejected:?}`")
        });
    let tags_of_a_again = tags_by_shot(outcomes);
    expect_on_each_shot(
        shots,
        |shot| one_readout(&tags_of_a_again, shot) == one_readout(tags_of_a, shot),
        |on| {
            format!(
                "`a_again` must make one readout on each shot, under `a`'s tag, so that \
                 `[a, a_again]` repeats a tag, but `[a_again]` does not, on {on}"
            )
        },
    );
}
