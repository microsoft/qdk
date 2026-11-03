// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Checks of the rules of reports: an executor reports its work once, at
//! its first close, and reports fold with an empty default, in any order.

use std::fmt::Debug;

use crate::{Close, Fold};

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
