// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! EXPERIMENTAL, development only (not for review): in-order fusion, the
//! `seq` fusion of the polyene swaps study, in the resumer.
//!
//! One block is open at a time. Each gate joins it while the union of
//! their sites holds at most `max_sites` sites; otherwise the block is
//! emitted and the gate opens the next. So the blocks keep the gates'
//! order, and no gate moves past another: unlike [`GateFusion::Pairs`],
//! which may hold several open blocks and emit one long after its gates,
//! letting the bonds between grow (on the polyene's step, Pairs needed
//! 1.8× the unfused run's bond inside a step; in-order fusion kept it).
//!
//! A block of one or two sites is an ordinary operator; a block of three
//! or more is a [`Step::DevWide`], for an engine that applies it (the CPU
//! engine with its dev MPO path).
//!
//! [`GateFusion::Pairs`]: super::fusion::GateFusion::Pairs

use std::rc::Rc;

use super::super::lowering::{DevWideOperator, Step};
use super::fusion::Block;

/// `steps` with each run of operators between two draws fused in order
/// into blocks of at most `max_sites` sites; each draw (and each wide
/// step) kept in its place.
///
/// # Panics
///
/// When `max_sites` is below 2.
pub(crate) fn fuse_runs(steps: Vec<Step>, max_sites: usize) -> Vec<Step> {
    assert!(
        max_sites >= 2,
        "in-order fusion holds blocks of 2 sites or more"
    );
    let mut fused = Vec::with_capacity(steps.len());
    let mut open: Option<Block> = None;
    for step in steps {
        match step {
            Step::Apply(operator) => {
                let on = operator.sites();
                if let Some(block) = open.as_mut() {
                    let mut union = block.sites.clone();
                    union.extend_from_slice(on);
                    union.sort_unstable();
                    union.dedup();
                    if union.len() <= max_sites {
                        if union.len() > block.sites.len() {
                            let mut wide = Block::identity(union);
                            wide.multiply(&block.sites, &block.matrix);
                            *block = wide;
                        }
                        block.multiply(on, operator.matrix.row_major());
                        continue;
                    }
                    fused.push(emit(open.take().expect("open")));
                }
                let mut sites = on.to_vec();
                sites.sort_unstable();
                let mut block = Block::identity(sites);
                block.multiply(on, operator.matrix.row_major());
                open = Some(block);
            }
            other => {
                fused.extend(open.take().map(emit));
                fused.push(other);
            }
        }
    }
    fused.extend(open.map(emit));
    fused
}

fn emit(block: Block) -> Step {
    if block.sites.len() <= 2 {
        Step::Apply(block.into_operator())
    } else {
        Step::DevWide(Rc::new(DevWideOperator {
            sites: block.sites,
            matrix: block.matrix,
        }))
    }
}
