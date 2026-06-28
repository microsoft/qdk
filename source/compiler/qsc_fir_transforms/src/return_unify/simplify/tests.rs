// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Per-rule test suites for the [`super`] simplifier catalogue.
//!
//! Each rule file has a sibling test module so failures localize to the
//! rule that broke. See [`super`] for the rule signature contract.

mod bare_return;
mod both_branches;
mod dead_flag;
mod dead_local;
mod fixpoint;
mod guard_clause;
mod identical_branches;
mod let_folding;
mod single_branch;
