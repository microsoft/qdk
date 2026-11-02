// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Checks of the rules that the type system cannot enforce, for
//! implementers to run in their own tests: the compiler checks that a type
//! has `call`, `close` and `fold`, and that a call is an
//! [`InstructionCall`], not that they keep the rules that callers rely on.
//! They check the rules of [`Executor`] (a refusal leaves the executor
//! usable, a failure is final, and ends the later calls still in flight
//! once it is recorded as the executor's: the check polls the failing
//! call's reply first), of [`Reply`] (a reply appends
//! its events, keeping what the caller held, checked wherever a check
//! reads readouts), of [`Close`], [`Fold`], described instruction sets
//! and readout tags, and the promises of
//! [`Capabilities`](crate::Capabilities) (`replayable`; `resumable_shots`, with
//! its rule that a call changes no shot it does not name), and of shot
//! conditions (a shot keeps from one call to the next those its outcomes
//! carried; the checks that compare a shot's readouts over the same calls
//! compare the conditions they carry too, since a shot's conditions are
//! its own, as its readouts are). They also
//! check that a new executor starts its shots as made, whatever an earlier
//! one left, closed or dropped, a rule every check that builds several
//! executors relies on.
//! Built with the `conformance` feature, so a crate that implements the
//! traits enables it as a dev-dependency:
//!
//! ```toml
//! [dev-dependencies.executor_protocol]
//! path = "../executor_protocol"
//! features = ["conformance"]
//! ```
//!
//! Each check panics, naming the rule broken, when the implementation does
//! not keep it, and names what it needs when its inputs do not let it check
//! the rule. A check of a rule or a need on each shot names every shot that
//! breaks it, so that one run reports every fault.

// Known issue, open (blocks neither this commit nor a later one): no check
// here of the rule that a wrapper never forgets a refusal (`Executor`'s
// contract): an executor that wraps another, or is made of one, keeps
// refusing once the other is closed or failed. It would need a function
// that wraps the executor under test, a new shape for a check; whether to
// add one is a design question held for the user. Only the crate's own
// wrappers are tested (`.run_on(..)` and `.shots(..)`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;
use std::iter;
use std::slice;
use std::task::Waker;

use crate::reply::wait_status;
use crate::{
    Close, ErrorKind, Executor, Fold, InstructionBatch, InstructionBatchRef, InstructionCall,
    InstructionSet, Message, Opcode, Operand, OperandKind, Rejected, Reply, ReplyStatus,
    RetryClass, ShotConditions, ShotId, ShotOutcome, ShotSet, Value, wait,
};

/// Closes `executor`, then calls it with `message`: the call must be
/// rejected with [`Rejected::Closed`]. `message` should be one the executor
/// accepts while open, so that the rejection can only be the close's.
///
/// It checks only the answer. A closed executor must also do no work (an
/// adapter runs no logic and makes no state, though the closed executor
/// below would refuse what it sends), which the implementer's own tests
/// must pin.
///
/// # Panics
///
/// When the call is accepted, or rejected for another reason.
pub fn rejects_calls_once_closed<M: Message, E: Executor<M> + Close>(
    executor: &mut E,
    message: M::Ref<'_>,
) {
    executor.close();
    expect_closed(executor.call(message), "a call after close");
}

/// The rule of [`Close`] that both [`rejects_calls_once_closed`] and
/// [`refuses_calls_after_a_failure`] check: `answer`, the answer to `call`
/// made once the executor is closed, must be a rejection with
/// [`Rejected::Closed`].
///
/// # Panics
///
/// When `answer` is an accepted call, or a rejection for another reason;
/// the panic message starts with `call`.
fn expect_closed<R>(answer: Result<R, Rejected>, call: &str) {
    match answer {
        Err(Rejected::Closed) => {}
        Err(other) => panic!("{call} must be rejected with `Closed`, not `{other:?}`"),
        Ok(_) => panic!("{call} must be rejected with `Closed`, but it was accepted"),
    }
}

/// Calls `executor` with `refused`, a message it refuses for a fault of the
/// message's own (an instruction outside its set, a qubit it does not
/// have), then with `accepted`: the refusal must not be
/// [`Rejected::EarlierFailure`] or [`Rejected::Closed`], which would say that
/// the executor ended, and `accepted` must then be accepted and its reply
/// end well, since a refusal means that nothing ran, so the caller can
/// correct its call and send it again. `refused` must be refused with a
/// class that says so, not [`RetryClass::Resend`], after which the work may
/// have run. `accepted` should be a message the executor runs to its end
/// before the refusal, so that a refusal or a failure of it can only be the
/// earlier refusal's doing. The rule also asks that the refused message
/// changed no state, which this check cannot see; nor does it check that
/// `accepted`'s reply delivers its events, since it cannot know a message's
/// events, only that the reply ends well.
///
/// # Panics
///
/// When `refused` is accepted, or refused with a class of `Resend` (this
/// check needs a refusal after which nothing ran), or refused with
/// `EarlierFailure` or `Closed`; when `accepted` is then refused, or its
/// reply fails.
pub fn accepts_calls_after_a_refusal<M: Message, E: Executor<M>>(
    executor: &mut E,
    refused: M::Ref<'_>,
    accepted: M::Ref<'_>,
) {
    match executor.call(refused) {
        Err(rejected @ (Rejected::EarlierFailure(_) | Rejected::Closed)) => panic!(
            "a call refused for its own fault must not be refused with `{rejected:?}`, which \
             says that the executor ended"
        ),
        Err(rejected) if rejected.retry() == RetryClass::Resend => panic!(
            "the refused message must be refused with a class that says that nothing ran, not \
             `{rejected:?}`, after which its work may have run"
        ),
        Err(_) => {}
        Ok(_) => panic!("the refused message must be refused, so that this check sees a refusal"),
    }
    match executor.call(accepted) {
        Ok(reply) => {
            if let Err(reason) = wait(reply, &mut Vec::new()) {
                panic!("a call after a refusal must run, but its reply failed: `{reason}`");
            }
        }
        Err(rejected) => panic!(
            "a call after a refusal must be accepted, but it was refused with `{rejected:?}`"
        ),
    }
}

/// Calls `executor`, an executor whose state carries over from one call to
/// the next, with `failing`, a message whose work fails once started, and
/// waits for its reply, which must end failed ([`ReplyStatus::Failed`],
/// not cancelled, which says that the work was stopped from outside),
/// saying why; then calls it
/// with each message of `later`, in order: each call must be refused with
/// [`Rejected::EarlierFailure`], whose reason contains the failure's, since
/// the failure left the executor's state part-way through `failing`, and
/// the caller learns which failure ended its run. Then closes `executor` and
/// calls it with `later[0]` again: the call must be refused with
/// [`Rejected::Closed`], which comes first, since a caller may not call a
/// closed executor at all. `later` should hold messages the executor accepts
/// before the failure, so that a refusal can only be the failure's; for an
/// executor of several shots, some on shots that `failing` did not name,
/// since the failure ends the whole executor. `failing`'s reply must end:
/// the check waits for it ([`wait`]), so it never returns if that reply
/// never ends.
///
/// # Panics
///
/// When `failing` is refused, or its reply does not end
/// [`ReplyStatus::Failed`] (it ends done, or cancelled), or fails with an
/// empty reason, which this check needs to name the failure; when `later`
/// is empty; when a message of `later` is accepted (the panic shows its
/// reply's first poll), or refused for another reason, or with a reason
/// that does not contain the failure's; when the call after close is not
/// refused with `Closed`.
pub fn refuses_calls_after_a_failure<M: Message, E: Executor<M> + Close>(
    executor: &mut E,
    failing: M::Ref<'_>,
    later: &[M::Ref<'_>],
) {
    assert!(!later.is_empty(), "`later` must hold a message to refuse");
    let reply = executor
        .call(failing)
        .expect("the failing message must be accepted, so that its work starts");
    let failure = failure_of(reply);
    for (k, &message) in later.iter().enumerate() {
        match executor.call(message) {
            Err(Rejected::EarlierFailure(reason)) => assert!(
                reason.contains(&failure),
                "a refusal after a failure must name the failure, `{failure}`, but the \
                 refusal of `later[{k}]` says `{reason}`"
            ),
            Err(other) => panic!(
                "a call after a failure must be rejected with `EarlierFailure`, not `{other:?}` \
                 (`later[{k}]`)"
            ),
            Ok(mut reply) => {
                // Polled once, not waited for: a reply that never ends must
                // not hang the check.
                let mut events = Vec::new();
                let status = reply.poll_events(&mut events, Waker::noop());
                panic!(
                    "a call after a failure must be rejected with `EarlierFailure`, but it was \
                     accepted (`later[{k}]`), and its reply, polled once, was `{status:?}` \
                     with {} events",
                    events.len()
                )
            }
        }
    }
    executor.close();
    expect_closed(executor.call(later[0]), "a call after a failure and close");
}

/// The reason why `reply`, the reply to the failing message of a check,
/// failed: it must end failed ([`ReplyStatus::Failed`], not cancelled,
/// which says that the work was stopped from outside), saying why, so that
/// a later call's refusal or failure can name it.
///
/// # Panics
///
/// When `reply` ends done or cancelled, or fails with an empty reason.
fn failure_of<R: Reply>(reply: R) -> String {
    let failure = match wait_status(reply, &mut Vec::new()) {
        ReplyStatus::Failed(reason) => reason,
        ReplyStatus::Cancelled => panic!(
            "the failing message's reply must end failed, not cancelled: a cancellation says \
             that its work was stopped from outside, not that it failed"
        ),
        end => panic!("the failing message's reply must end failed, not `{end:?}`"),
    };
    assert!(
        !failure.is_empty(),
        "the failing message's reply must say why it failed, so that a refusal can name it"
    );
    failure
}

/// Calls `executor`, an executor whose state carries over from one call to
/// the next, with `failing`, a message whose work fails once started, then
/// with `later`, before it waits for `failing`'s reply, which
/// must end failed ([`ReplyStatus::Failed`]), saying why: `later`, in
/// flight as the earlier call failed, must end at that failure, since it
/// would run on the state the failure left part-way through `failing`. It
/// is either refused with [`Rejected::EarlierFailure`], whose reason
/// contains the failure's, when the executor knew of the failure as it was
/// called (one that runs its calls inside `call`), or accepted, and then
/// its reply fails, with a reason that contains the failure's, and hands
/// over no event (one that runs its calls elsewhere, such as on a
/// placement, where `later` may even have run, on another shot). `later`
/// should be a message the executor accepts before the failure, so that a
/// refusal or a failure can only be the failure's; for an executor of
/// several shots, on shots that `failing` does not name, since the failure
/// ends the whole executor. It waits for both replies, so run it where a
/// reply that never ends shows (a test with a time limit).
///
/// # Panics
///
/// When `failing` is refused, or its reply does not end failed, or fails
/// with an empty reason; when `later` is refused for another reason, or
/// with a reason that does not contain the failure's; when `later` is
/// accepted and its reply ends done or cancelled, fails with a reason that
/// does not contain the failure's, or hands over an event.
pub fn ends_calls_in_flight_at_an_earlier_failure<M: Message, E: Executor<M>>(
    executor: &mut E,
    failing: M::Ref<'_>,
    later: M::Ref<'_>,
) {
    let failing = executor
        .call(failing)
        .expect("the failing message must be accepted, so that its work starts");
    let later = executor.call(later);
    let failure = failure_of(failing);
    match later {
        Err(Rejected::EarlierFailure(reason)) => assert!(
            reason.contains(&failure),
            "a refusal after a failure must name the failure, `{failure}`, but the refusal of \
             `later` says `{reason}`"
        ),
        Err(other) => panic!(
            "a call in flight at an earlier failure must be refused with `EarlierFailure`, or \
             end failed, not be refused with `{other:?}`"
        ),
        Ok(reply) => {
            let mut events = Vec::new();
            match wait_status(reply, &mut events) {
                ReplyStatus::Failed(reason) => assert!(
                    reason.contains(&failure),
                    "a call in flight at an earlier failure must end with that failure, \
                     `{failure}`, but its reply says `{reason}`"
                ),
                end => panic!(
                    "a call in flight at an earlier failure must end failed, with that \
                     failure, `{failure}`, not `{end:?}`"
                ),
            }
            assert!(
                events.is_empty(),
                "a call in flight at an earlier failure must hand over no event, since it would \
                 read the state the failure left, but it handed over {}",
                events.len()
            );
        }
    }
}

/// Closes `executor` twice: the second report must be empty
/// ([`Default`]), so that a caller that folds every report it receives counts
/// the executor's work once, and the first must hold that work, since a run
/// closes its executor once ([`Close`]). `executor` must have done some
/// work, so that its first report is not empty, and a report given twice
/// would show.
///
/// # Panics
///
/// When the second report is not empty; when the first is empty, which
/// names the rule when the second is not, and the need when it is too.
pub fn reports_once<E: Close>(executor: &mut E)
where
    E::Report: PartialEq + Debug,
{
    let first = executor.close();
    let second = executor.close();
    assert!(
        first != E::Report::default() || second == E::Report::default(),
        "the first close must report the executor's work, not a later one, but the first \
         report was empty and the second was not"
    );
    assert_ne!(
        first,
        E::Report::default(),
        "`executor` must have done some work, so that its first report is not empty and a \
         report given twice would show"
    );
    assert_eq!(
        second,
        E::Report::default(),
        "a second close must return an empty report"
    );
}

/// Folds `report` with the [`Default`] report, on either side: the result
/// must be `report`, so that a part that did nothing adds nothing.
///
/// # Panics
///
/// When folding in the default report changes `report`.
pub fn default_is_empty<Q: Fold + Clone + PartialEq + Debug>(report: &Q) {
    let mut right = report.clone();
    right.fold(Q::default());
    assert_eq!(
        &right, report,
        "folding in the default report must change nothing"
    );
    let mut left = Q::default();
    left.fold(report.clone());
    assert_eq!(
        &left, report,
        "folding into the default report must give the report folded in"
    );
}

/// Folds `a`, `b` and `c` in every order and grouping: each two in both
/// orders, and the three in each of their six orders, each in both
/// groupings (`(a b) c` folds `a b` first, `a (b c)` folds `b c` first).
/// The totals of each two must be equal, and the twelve of the three must
/// be equal, so that the reports of the parts of one run (shots run in
/// separate executors, say) fold into the report of the whole whichever
/// part ends first: a fold that is right on some orders only (one that
/// keeps an error of its own but drops one folded in, say) would show.
/// `a`, `b` and `c` must differ two by two, so that a total that depends
/// on the order or the grouping would show: of equal reports, any order
/// and any grouping fold the same reports in the same places.
///
/// For totals in floating point, choose values whose sums are exact (small
/// integers, say): a grouping then cannot change the total by rounding.
///
/// # Panics
///
/// When two orders or groupings give different totals, naming every pair
/// whose two orders differ, or else every total of the three that differs
/// from `(a b) c`; when two of `a`, `b` and `c` are equal, naming every
/// such pair.
pub fn folds_in_any_order<Q: Fold + Clone + PartialEq + Debug>(a: &Q, b: &Q, c: &Q) {
    let fold = |x: &Q, y: &Q| {
        let mut total = x.clone();
        total.fold(y.clone());
        total
    };
    let reports = [("a", a), ("b", b), ("c", c)];
    let orders: Vec<String> = [(0, 1), (1, 2), (0, 2)]
        .into_iter()
        .filter_map(|(one, other)| {
            let ((one, one_report), (other, other_report)) = (reports[one], reports[other]);
            let forth = fold(one_report, other_report);
            let back = fold(other_report, one_report);
            (forth != back)
                .then(|| format!("`{one} {other}` gives `{forth:?}`, `{other} {one}` `{back:?}`"))
        })
        .collect();
    assert!(
        orders.is_empty(),
        "the order of two reports must not change their total, but {}",
        orders.join("; ")
    );
    let mut totals = Vec::new();
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let [(one, one_report), (two, two_report), (three, three_report)] =
            order.map(|at| reports[at]);
        totals.push((
            format!("({one} {two}) {three}"),
            fold(&fold(one_report, two_report), three_report),
        ));
        totals.push((
            format!("{one} ({two} {three})"),
            fold(one_report, &fold(two_report, three_report)),
        ));
    }
    let (first, total) = &totals[0];
    let differ: Vec<String> = totals[1..]
        .iter()
        .filter(|(_, other)| other != total)
        .map(|(how, other)| format!("`{how}` gives `{other:?}`"))
        .collect();
    assert!(
        differ.is_empty(),
        "the order and grouping of three reports must not change their total, but `{first}` \
         gives `{total:?}`, and {}",
        differ.join(", ")
    );
    // Checked last, so that a fold that breaks the rule is told by the rule
    // it breaks.
    let equal: Vec<&str> = [
        ("`a` and `b`", a == b),
        ("`b` and `c`", b == c),
        ("`a` and `c`", a == c),
    ]
    .into_iter()
    .filter_map(|(pair, equal)| equal.then_some(pair))
    .collect();
    assert!(
        equal.is_empty(),
        "`a`, `b` and `c` must differ two by two, so that a total that depends on the order or \
         the grouping would show, but {} are equal",
        equal.join(", ")
    );
}

/// Calls `executor`, on `shots`, with batches of `accepted` and a call
/// outside `set`. First `[accepted, accepted, accepted]`, which must be
/// accepted and its reply end well, so that a refusal of a later batch of
/// three calls or fewer can only be the call outside's. Then, for each call
/// outside, `[accepted, outside]`, `[accepted, outside, accepted]` and
/// `[outside, accepted]`: each must be rejected with
/// [`Rejected::Instruction`] at the index of the call outside (1, 1 and
/// 0), whether it is last, in the middle or first, so that a caller learns
/// which call to change. Last, `[outside, accepted, outside]`, for every
/// pair of calls outside, in each order and one call twice included, must
/// be rejected at the first of the two, 0, as [`Rejected::Instruction`]
/// promises: an executor that looks for each reason in its own pass over
/// the batch, and so refuses at a later call when the earlier one is
/// outside for another reason, would show.
///
/// The calls outside are made from `accepted`, one for each way a call can
/// differ from every call of the set, by the definition of
/// [`InstructionSet::check`]: an opcode the set lacks, with `accepted`'s
/// operands; and, at `accepted`'s opcode, operands whose kinds are not
/// those it takes, as a sequence: one operand too many; one too few (the
/// last left out), when it takes one; and, at each of its operands in
/// turn, an operand of each other kind ([`OperandKind`]), the others
/// unchanged. The opcode lacked is the count of `set`'s mnemonics, since an
/// opcode is its instruction's position in the set, so opcodes have no
/// gaps. The rule also asks that nothing of a refused batch runs, which
/// this check cannot see. `set` is the set the executor describes (or its
/// maker does, in [`Capabilities::instruction_set`](crate::Capabilities)),
/// `accepted` a call of it that the executor runs, three times in one
/// batch (one that makes no readout, since a batch repeats no tag), and
/// `shots` shots it runs. It waits for the first batch's reply.
///
/// # Panics
///
/// When a batch with a call outside is accepted, or rejected for another
/// reason or at another index (`[outside, accepted, outside]`, for any
/// pair, at another than 0); when `accepted` is not a call of `set`, or
/// `[accepted, accepted, accepted]` is refused, or its reply fails.
pub fn refuses_calls_outside_its_set<E: Executor<InstructionBatch<InstructionCall>>>(
    executor: &mut E,
    shots: &ShotSet,
    set: &InstructionSet,
    accepted: &InstructionCall,
) {
    assert!(
        set.check(accepted).is_ok(),
        "`accepted` must be a call of the set"
    );
    if let Err(rejected) = outcomes_of_call(
        executor,
        shots,
        &[accepted.clone(), accepted.clone(), accepted.clone()],
    ) {
        panic!(
            "`accepted` must be a call that the executor runs, three times in one batch, so \
             that a refusal of a batch that holds it can only be the call outside's, but \
             `[accepted, accepted, accepted]` was rejected with `{rejected:?}`"
        );
    }
    let outside_calls = calls_outside(set, accepted);
    for (outside, how) in &outside_calls {
        for (instructions, at, batch) in [
            (
                vec![accepted.clone(), outside.clone()],
                1,
                "[accepted, outside]",
            ),
            (
                vec![accepted.clone(), outside.clone(), accepted.clone()],
                1,
                "[accepted, outside, accepted]",
            ),
            (
                vec![outside.clone(), accepted.clone()],
                0,
                "[outside, accepted]",
            ),
        ] {
            let answer = executor.call(InstructionBatchRef {
                shots,
                instructions: &instructions,
            });
            expect_refused_at(
                answer,
                at,
                "a call outside the set",
                &format!("`{batch}`, the call outside {how}"),
            );
        }
    }
    for (first, first_how) in &outside_calls {
        for (second, second_how) in &outside_calls {
            let answer = executor.call(InstructionBatchRef {
                shots,
                instructions: &[first.clone(), accepted.clone(), second.clone()],
            });
            expect_refused_at(
                answer,
                0,
                "the first of two calls outside the set",
                &format!(
                    "`[outside, accepted, outside]`, the first call outside {first_how}, the \
                     second {second_how}"
                ),
            );
        }
    }
}

/// Every kind of operand, for [`calls_outside`] to try each. A kind added
/// to [`OperandKind`] fails to compile in [`an_operand_of`]: add it here
/// too.
const OPERAND_KINDS: [OperandKind; 4] = [
    OperandKind::Block,
    OperandKind::Real,
    OperandKind::Integer,
    OperandKind::Tag,
];

/// An operand of `kind`.
fn an_operand_of(kind: OperandKind) -> Operand {
    match kind {
        OperandKind::Block => Operand::Block(0),
        OperandKind::Real => Operand::Real(0.0),
        OperandKind::Integer => Operand::Integer(0),
        OperandKind::Tag => Operand::Tag(0),
    }
}

/// The calls outside `set` that [`refuses_calls_outside_its_set`] sends,
/// made from `accepted`, a call of `set`, each with how it is outside: one
/// for each way [`InstructionSet::check`] refuses a call.
fn calls_outside(
    set: &InstructionSet,
    accepted: &InstructionCall,
) -> Vec<(InstructionCall, String)> {
    let beyond = Opcode::new(u32::try_from(set.mnemonics().count()).unwrap_or(u32::MAX));
    let operands = || accepted.operands.iter().copied();
    let at_its_opcode = |operands: Vec<Operand>| InstructionCall::new(accepted.opcode, operands);
    let mut outside = vec![
        (
            InstructionCall::new(beyond, operands()),
            "at an opcode the set lacks".to_owned(),
        ),
        (
            at_its_opcode(operands().chain([Operand::Integer(0)]).collect()),
            "with an operand too many".to_owned(),
        ),
    ];
    if let Some(fewer) = accepted.operands.len().checked_sub(1) {
        outside.push((
            at_its_opcode(operands().take(fewer).collect()),
            "with an operand too few".to_owned(),
        ));
    }
    for (at, operand) in accepted.operands.iter().enumerate() {
        for kind in OPERAND_KINDS
            .into_iter()
            .filter(|&kind| kind != operand.kind())
        {
            let mut operands: Vec<Operand> = operands().collect();
            operands[at] = an_operand_of(kind);
            outside.push((
                at_its_opcode(operands),
                format!(
                    "with its operand {at} of kind `{kind:?}`, not `{:?}`",
                    operand.kind()
                ),
            ));
        }
    }
    outside
}

/// The rule of an executor that refuses a batch for a fault of one of its
/// instructions: `answer`, the answer to `batch`, must be a rejection with
/// [`Rejected::Instruction`] at `at`, the index of `what`, the instruction
/// at fault, so that a caller learns which instruction to change.
///
/// # Panics
///
/// When `answer` is an accepted call, or a rejection for another reason or
/// at another index; the panic message starts with `what` and ends with
/// `batch`.
fn expect_refused_at<R>(answer: Result<R, Rejected>, at: usize, what: &str, batch: &str) {
    match answer {
        Err(Rejected::Instruction { at: refused, .. }) if refused == at => {}
        Err(Rejected::Instruction { at: refused, .. }) => {
            panic!("{what} must be rejected at its index, {at}, not {refused} ({batch})")
        }
        Err(other) => {
            panic!("{what} must be rejected with `Instruction`, not `{other:?}` ({batch})")
        }
        Ok(_) => {
            panic!("{what} must be rejected with `Instruction`, but it was accepted ({batch})")
        }
    }
}

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

/// Checks `holds` on each shot of `shots`, and panics once when it fails
/// on some, naming them all, so that a check run once reports every fault:
/// `message` makes the panic message from the shots on which it fails, in
/// the order of `shots`, named as `shot 2` or `shots 2, 4`.
///
/// # Panics
///
/// When `holds` fails on some shot.
fn expect_on_each_shot<'s>(
    shots: impl IntoIterator<Item = &'s ShotId>,
    mut holds: impl FnMut(ShotId) -> bool,
    message: impl FnOnce(&str) -> String,
) {
    let failing: Vec<String> = (shots.into_iter())
        .filter(|&&shot| !holds(shot))
        .map(ToString::to_string)
        .collect();
    match failing.as_slice() {
        [] => {}
        [shot] => panic!("{}", message(&format!("shot {shot}"))),
        shots => panic!("{}", message(&format!("shots {}", shots.join(", ")))),
    }
}

/// The tags `shot` reads under in `readouts`, in the batches whose index
/// `of` picks.
fn tags(readouts: &Readouts, shot: ShotId, of: impl Fn(usize) -> bool) -> BTreeSet<u32> {
    (readouts.get(&shot).into_iter().flatten())
        .filter_map(|&(call, tag, ..)| of(call).then_some(tag))
        .collect()
}

/// Each shot's readouts in `readouts`, by tag alone, in the order of the
/// tags, without their conditions: those of a run over several calls
/// compare with those of one batch, whose earlier readouts carry the
/// conditions raised by its later instructions, where a run over several
/// calls marks only the readouts of later calls with them.
fn tagged(readouts: Readouts) -> BTreeMap<ShotId, Vec<(u32, Value)>> {
    (readouts.into_iter())
        .map(|(shot, readouts)| {
            let mut tagged: Vec<_> = (readouts.into_iter())
                .map(|(_, tag, value, _)| (tag, value))
                .collect();
            tagged.sort_by_key(|&(tag, _)| tag);
            (shot, tagged)
        })
        .collect()
}

/// Each shot's readouts in a run of several calls: the index of the batch
/// that made each (its call, when each batch is one call), its tag, its
/// value and the conditions it carries, in the order of the batches and the
/// tags. The checks that compare a shot's readouts over the same calls
/// compare their conditions too, since a shot's conditions are its own, as
/// its readouts are.
type Readouts = BTreeMap<ShotId, Vec<(usize, u32, Value, ShotConditions)>>;

/// Calls `executor` on `shots` with each of `batches`, in order, waiting
/// for each reply, and gives each shot's readouts, under the index of the
/// batch that made them.
///
/// # Panics
///
/// As [`readouts_of_calls`].
fn readouts<I, E: Executor<InstructionBatch<I>>>(
    executor: &mut E,
    shots: &ShotSet,
    batches: &[&[I]],
) -> Readouts {
    let calls: Vec<_> = (batches.iter().enumerate())
        .map(|(call, &batch)| (call, shots.clone(), batch))
        .collect();
    readouts_of_calls(executor, &calls)
}

/// Calls `executor` with each of `calls`, a batch on shots under the index
/// of its batch, in order, waiting for each reply ([`outcomes_of_call`]),
/// and gives each shot's readouts, under the index of the batch that made
/// them.
///
/// # Panics
///
/// When a call is refused, or its reply fails: the checks that run batches
/// need them run; when a reply changes what the buffer held.
fn readouts_of_calls<I, E: Executor<InstructionBatch<I>>>(
    executor: &mut E,
    calls: &[(usize, ShotSet, &[I])],
) -> Readouts {
    let mut readouts = Readouts::new();
    for (call, shots, instructions) in calls {
        let outcomes = outcomes_of_call(executor, shots, instructions).unwrap_or_else(|rejected| {
            panic!("the batches must be accepted, but one was refused with `{rejected:?}`")
        });
        for outcome in outcomes {
            (readouts.entry(outcome.shot).or_default()).push((
                *call,
                outcome.tag,
                outcome.value,
                outcome.conditions,
            ));
        }
    }
    for readouts in readouts.values_mut() {
        readouts.sort_by_key(|&(call, tag, ..)| (call, tag));
    }
    readouts
}

/// Calls `executor` on `shots` with `instructions`, and gives the outcomes
/// of its reply, or its refusal. The reply is waited for into a buffer that
/// already holds [`HELD`] and one outcome of each shot of the call
/// ([`held_of`]): a reply appends its events, keeping what the caller held
/// ([`Reply::poll_events`]), so that a caller may gather the events of
/// several replies in one buffer. The held outcomes of the call's shots
/// show a reply that marks the caller's outcomes of a shot it flags,
/// whichever shot of the call that is.
///
/// # Panics
///
/// When the reply fails: the checks that run batches need them run; when
/// the reply changes what the buffer held.
fn outcomes_of_call<I, E: Executor<InstructionBatch<I>>>(
    executor: &mut E,
    shots: &ShotSet,
    instructions: &[I],
) -> Result<Vec<ShotOutcome>, Rejected> {
    let held: Vec<ShotOutcome> = iter::once(HELD)
        .chain(shots.iter().map(|&shot| held_of(shot)))
        .collect();
    let reply = executor.call(InstructionBatchRef {
        shots,
        instructions,
    })?;
    let mut outcomes = held.clone();
    if let Err(reason) = wait(reply, &mut outcomes) {
        panic!("the batches must run, but the reply to one failed: `{reason}`");
    }
    assert!(
        outcomes.starts_with(&held),
        "a reply must append its events to the caller's buffer, keeping what it held \
         (`Reply::poll_events`), but the reply to one of the batches changed it"
    );
    Ok(outcomes.split_off(held.len()))
}

/// The outcome that the readout checks' buffer holds before each reply's
/// events: one that no executor gives for a caller's shots, of the last
/// shot and the last tag, with no value.
const HELD: ShotOutcome = ShotOutcome::new(ShotId::MAX, u32::MAX, Value::Error(ErrorKind::Other));

/// An outcome of `shot` that the readout checks' buffer holds before each
/// reply's events, as a caller holds those of the shot's earlier calls:
/// of the last tag, with no value and no condition.
const fn held_of(shot: ShotId) -> ShotOutcome {
    ShotOutcome::new(shot, u32::MAX, Value::Error(ErrorKind::Other))
}
