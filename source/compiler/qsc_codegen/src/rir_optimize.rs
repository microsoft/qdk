// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The post-partial-evaluation optimization stage.
//!
//! This is where the two halves meet. The producer lowers a program into a
//! graph, the shared rule cancels what it can, and the return leg carries the
//! result back. Every step declines rather than approximating, so a program
//! outside the accepted class leaves this stage exactly as it arrived.
//!
//! Declining is the ordinary case, not the exceptional one. The producer
//! accepts straight-line blocks, result-conditional branches, supported loops,
//! and calls into IR-function bodies; an unsupported loop shape, a callable that reaches itself, a qubit
//! operand that resolves to no static index, or a callable with no vocabulary
//! term all refuse. Returning the program untouched is the only safe response,
//! because the alternative is emitting something the graph could not represent.
//!
//! This is off unless a caller asks for it, and that default is not timidity.
//! Cancelling a pair is sound about the unitary and wrong about the device: a
//! chain of `x` gates is the identity on paper and is a noise accumulator on
//! hardware, so a program written to characterize gate error would compile to
//! nothing. The compiler cannot see the noise model from here, so the caller
//! who knows what the QIR is for decides.

use qsc_rir::rir::Program;

use crate::pass_pipeline::{Cancellation, OptimizationLevel, cancel_inverse_pairs};
use crate::rir_qhdg::{IdentityCensus, graph_to_program, lower_with_census};

/// What the stage did with one program.
///
/// For measurement rather than control flow. The stage declines silently by
/// design, so without a report the difference between declining on everything
/// and helping on everything is invisible from outside, which is how the first
/// wiring of this stage passed every test while doing nothing.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StageReport {
    /// How the program's qubit operands resolved, whether or not it lowered.
    pub census: IdentityCensus,
    /// Why the stage left the program alone, when it did.
    pub declined: Option<String>,
    /// Whether the program that came back differs from the one that went in.
    pub changed: bool,
}

/// Cancels adjacent inverse pairs in a program, or leaves it alone.
pub fn optimize(program: &mut Program, level: OptimizationLevel) -> StageReport {
    if level == OptimizationLevel::None {
        return StageReport {
            declined: Some("the caller asked for no optimization".to_string()),
            ..StageReport::default()
        };
    }
    let (lowered, census) = lower_with_census(program);
    let declined = |reason: String| StageReport {
        census,
        declined: Some(reason),
        changed: false,
    };
    let mut lowered = match lowered {
        Ok(lowered) => lowered,
        Err(error) => return declined(error.to_string()),
    };
    match cancel_inverse_pairs(&mut lowered.graph) {
        Ok(Cancellation::Changed) => {}
        Ok(outcome) => return declined(format!("{outcome:?}")),
        Err(error) => return declined(error),
    }
    // The graph changed, so the return leg has something to carry. A refusal
    // here means the rewrite did something removal cannot express, and keeping
    // the original is the only answer that cannot be wrong.
    match graph_to_program(program, &lowered.graph, &lowered) {
        Ok(returned) => {
            *program = returned;
            StageReport {
                census,
                declined: None,
                changed: true,
            }
        }
        Err(error) => declined(error.to_string()),
    }
}

#[cfg(test)]
mod tests;
