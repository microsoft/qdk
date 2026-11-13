// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Rewrites: a batch's operators, after lowering, changed into cheaper
//! ones that give the same state, so that the engine, whose time goes to
//! its operators, does less. A run applies one rewrite today, which is
//! `GroupedRun`'s field and `MpsResumer`'s setting; when a second comes,
//! that one rewrite becomes a list of the rewrites a run applies.
//!
//! **The contract**, stated here once, that every [`Rewrite`] keeps:
//!
//! 1. **Same state:** the rewritten operators, applied in order, give the
//!    same state as the run's, to rounding. An engine that truncates does
//!    so after each operator, so on it the two states may differ within
//!    the truncation error.
//! 2. **Engine-ready:** every operator acts on one or two distinct sites,
//!    the most every engine applies.
//! 3. **Measurements and resets are barriers:** no operator moves across
//!    one, since they read the state.
//!
//! Clause 3 holds for every rewrite by construction: a rewrite never sees
//! a measurement or a reset, since [`rewrite_runs`] hands it one run of
//! operators at a time and keeps each draw in its place.

pub(in crate::mps_simulator) mod dev_in_order;
pub(in crate::mps_simulator) mod fusion;

use super::lowering::{Operator, Step};

/// A rewrite of the operators of a batch, keeping the contract of this
/// module. It takes one run at a time: the operators ([`Step::Apply`])
/// between two measurements or resets, or the batch's ends.
pub(crate) trait Rewrite {
    /// The operators that replace `run`, keeping the contract.
    fn rewrite(&self, run: Vec<Operator>) -> Vec<Operator>;
}

/// `steps` with each run of operators between two measurements or resets
/// given to `rewrite`, and each measurement and reset kept in its place.
pub(crate) fn rewrite_runs(steps: Vec<Step>, rewrite: &(impl Rewrite + ?Sized)) -> Vec<Step> {
    let mut rewritten = Vec::with_capacity(steps.len());
    let mut run = Vec::new();
    for step in steps {
        match step {
            Step::Apply(operator) => run.push(operator),
            draw @ (Step::Measure { .. } | Step::Reset { .. } | Step::DevWide(_)) => {
                let operators = rewrite.rewrite(std::mem::take(&mut run));
                rewritten.extend(operators.into_iter().map(Step::Apply));
                rewritten.push(draw);
            }
        }
    }
    rewritten.extend(rewrite.rewrite(run).into_iter().map(Step::Apply));
    rewritten
}
