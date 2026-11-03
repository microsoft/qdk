// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The operation vocabulary every producer maps onto.
//!
//! A vocabulary owned by one producer permits other producers to diverge,
//! leaving readers unable to interpret their graphs. Defining it below every
//! producer makes compatibility an explicit shared contract.
//!
//! The vocabulary is a fact about the graph, and
//! a producer's job is to map its own source language onto these names. It does
//! not get to extend them. [`accept_producer`] is how a producer states the
//! names it emits and finds out whether the vocabulary admits them, and a
//! producer that grew a name of its own fails there rather than silently
//! diverging until a reader refuses the result.
//!
//! Nothing here depends on any source language. The names are literals rather
//! than being derived from a gate enum, because the gate enum lives in a crate
//! above this one and importing it downward is what would make this a circuit
//! vocabulary again.

use std::collections::BTreeSet;

/// The operation name carrying a literal parameter value.
pub const CONSTANT: &str = "constant:";

/// The suffix marking a constant node whose angle still names a parameter.
pub const SYMBOLIC: &str = "symbolic";

/// The prefix every quantum operation name carries.
pub const QUANTUM: &str = "quantum.";

/// The operation name receiving a measurement result for a classical bit.
pub const STORE: &str = "classical.store:";

/// The operation name standing for one declared classical bit.
pub const CLBIT: &str = "classical.allocate";

/// The operation name carrying a program's global phase.
pub const PHASE: &str = "circuit.global_phase:";

/// The operation name standing for one declared qubit.
pub const ALLOCATE: &str = "quantum.allocate";

/// The operation name consuming a qubit's final capability.
pub const RELEASE: &str = "quantum.release";

/// The operation name for a reset.
pub const RESET: &str = "quantum.reset";

/// The operation name for a barrier.
pub const BARRIER: &str = "quantum.barrier";

/// The operation name for a conditional, which owns its branch regions.
pub const GAMMA: &str = "quantum.gamma";

/// The operation name for a loop, which owns its body region.
///
/// The body is a theta region, so what makes this a loop rather than a block
/// run once is the region's own declaration: a back-edge exit and a carried
/// binding for every wire that crosses an iteration. A node with this name and
/// neither is refused rather than read as a straight run.
pub const THETA: &str = "quantum.theta";

/// The operation name carrying a qubit through a branch that leaves it alone.
///
/// Branches of one conditional have to produce the same things, or nothing
/// downstream could say what the conditional produced. A branch that does not
/// touch a carried qubit still has to hand it on, and this is what does it.
pub const CARRY: &str = "quantum.carry";

/// The operation name carrying a classical bit into a branch that reads it.
///
/// A binding pairs a port with one in its region's immediate parent, so a
/// conditional below the region its predicate was measured in has nothing to
/// bind from. This is the value's answer to the same problem [`CARRY`] answers
/// for a capability. The `quantum.` prefix is what the raise path's vocabulary
/// admits, not a claim that the value is quantum.
pub const RELAY: &str = "quantum.relay";

/// The operation name for a classical instruction the graph carries without
/// interpreting.
///
/// A node under this name says an instruction is here and where it sits in the
/// order, and says nothing else. Nothing reads it, no rule matches it, and no
/// contract describes it: what the instruction means stays in the program it
/// was lowered from, and the return leg copies it back from there.
///
/// One name rather than one per instruction kind. A name per kind would be a
/// classification, and classifying classical instructions is the step this
/// stops short of. The name stays outside the prefixes that mark an operation
/// pure classical, because a rule matching on shape alone would otherwise be
/// free to merge two carriers that stand for different instructions.
pub const OPAQUE: &str = "classical.opaque";

/// Every gate name the vocabulary admits.
///
/// A closed list rather than a prefix rule. `quantum.` is what separates this
/// vocabulary from another producer's, but it cannot say whether a name inside
/// it means anything, and a rewrite that matched on the prefix alone would act
/// on a name nobody defined.
pub const GATES: [&str; 33] = [
    "quantum.ccx",
    "quantum.crx",
    "quantum.cry",
    "quantum.crz",
    "quantum.cswap",
    "quantum.cx",
    "quantum.cy",
    "quantum.cz",
    "quantum.h",
    "quantum.i",
    "quantum.measure_reset_z",
    "quantum.measure_x",
    "quantum.measure_y",
    "quantum.measure_z",
    "quantum.prepare_x",
    "quantum.prepare_y",
    "quantum.prepare_z",
    "quantum.rx",
    "quantum.rxx",
    "quantum.ry",
    "quantum.ryy",
    "quantum.rz",
    "quantum.rzz",
    "quantum.s",
    "quantum.s_adj",
    "quantum.swap",
    "quantum.sx",
    "quantum.sx_adj",
    "quantum.t",
    "quantum.t_adj",
    "quantum.x",
    "quantum.y",
    "quantum.z",
];

/// The structural operation names, which are everything the vocabulary admits
/// that is not a gate.
pub const STRUCTURAL: [&str; 10] = [
    ALLOCATE, RELEASE, RESET, BARRIER, GAMMA, THETA, CARRY, RELAY, CLBIT, OPAQUE,
];

/// The operation names that carry their meaning in a suffix rather than being
/// the whole name.
pub const PREFIXED: [&str; 3] = [CONSTANT, STORE, PHASE];

/// Operations that are exactly their own inverse, leaving no global phase.
///
/// Named rather than derived, because being self-inverse is a claim about what
/// an operation means and a gate added to the vocabulary should have to make
/// that claim deliberately.
///
/// What is absent matters as much as what is present. `quantum.s` and
/// `quantum.t` are not here, and neither is any adjoint: `t` twice is `s`, not
/// the identity, and a rule that cancelled adjacent identical gates by name
/// would miscompile the pairs that already sit in this repository's checked-in
/// generated code. [`no_self_inverse_gate_is_a_quarter_turn_or_an_adjoint`]
/// keeps that true as the list changes.
pub const SELF_INVERSE: [&str; 11] = [
    "quantum.ccx",
    "quantum.cswap",
    "quantum.cx",
    "quantum.cy",
    "quantum.cz",
    "quantum.h",
    "quantum.i",
    "quantum.swap",
    "quantum.x",
    "quantum.y",
    "quantum.z",
];

/// Why the vocabulary will not admit what a producer declared.
#[derive(Clone, Debug, thiserror::Error, Eq, PartialEq)]
pub enum VocabularyError {
    #[error(
        "operation '{operation}' is not in the shared vocabulary; a producer maps its source language onto these names rather than adding to them"
    )]
    Undeclared { operation: String },
    #[error("a producer that declares no operation at all has not declared a vocabulary")]
    Empty,
}

/// Whether the vocabulary admits an operation name.
///
/// A prefixed name is admitted by its prefix, because the suffix carries a
/// value rather than an identity.
#[must_use]
pub fn admits(operation: &str) -> bool {
    GATES.contains(&operation)
        || STRUCTURAL.contains(&operation)
        || PREFIXED
            .iter()
            .any(|prefix| operation.starts_with(prefix) && operation.len() > prefix.len())
}

/// Accepts the operation names a producer emits, or names the first one this
/// vocabulary does not have.
///
/// This is the check that makes the vocabulary shared rather than merely
/// copied. A producer calls it with everything it can emit, so a name it grew
/// on its own is reported here, against this list, instead of reaching a reader
/// that can only refuse the whole graph.
///
/// # Errors
///
/// Returns the first undeclared operation, or [`VocabularyError::Empty`] when
/// the producer declared nothing.
pub fn accept_producer<'a>(
    declared: impl IntoIterator<Item = &'a str>,
) -> Result<BTreeSet<String>, VocabularyError> {
    let mut accepted = BTreeSet::new();
    for operation in declared {
        if !admits(operation) {
            return Err(VocabularyError::Undeclared {
                operation: operation.to_string(),
            });
        }
        accepted.insert(operation.to_string());
    }
    if accepted.is_empty() {
        return Err(VocabularyError::Empty);
    }
    Ok(accepted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vocabulary_admits_what_it_declares_and_nothing_else() {
        for operation in GATES.iter().chain(&STRUCTURAL) {
            assert!(admits(operation), "'{operation}' should be admitted");
        }
        for prefix in PREFIXED {
            assert!(admits(&format!("{prefix}0.25")));
            assert!(
                !admits(prefix),
                "'{prefix}' alone carries no value and is not an operation"
            );
        }
        for absent in ["quantum.made_up", "call", "quantum.", "", "x"] {
            assert!(!admits(absent), "'{absent}' should not be admitted");
        }
    }

    #[test]
    fn producer_with_unknown_operation_is_refused() {
        // The whole point of the surface. A second declaration is not tolerated
        // and then discovered downstream; it fails against this list.
        assert_eq!(
            accept_producer(["quantum.x", "quantum.dagger"]),
            Err(VocabularyError::Undeclared {
                operation: "quantum.dagger".to_string()
            })
        );
    }

    #[test]
    fn empty_producer_declaration_is_refused() {
        assert_eq!(accept_producer([]), Err(VocabularyError::Empty));
    }

    #[test]
    fn declared_gate_vocabulary_is_accepted() {
        assert!(accept_producer(GATES).is_ok());
    }

    #[test]
    fn declared_structural_vocabulary_is_accepted() {
        assert!(accept_producer(STRUCTURAL).is_ok());
    }

    #[test]
    fn no_self_inverse_gate_is_a_quarter_turn_or_an_adjoint() {
        // The live form of a hazard that is already in this repository's
        // checked-in generated code: adjacent `t;t` and `s;s` pairs sit in the
        // goldens, and a rule that treated them as cancellable would silently
        // emit a different program. Asserting the goldens are unchanged catches
        // it after the fact; asserting the mapping catches it here.
        for gate in SELF_INVERSE {
            assert!(
                GATES.contains(&gate),
                "'{gate}' claims to be self-inverse and is not in the vocabulary"
            );
            assert!(
                !gate.ends_with("_adj"),
                "'{gate}' is an adjoint, so it is the inverse of something else"
            );
        }
        for quarter in [
            "quantum.s",
            "quantum.s_adj",
            "quantum.t",
            "quantum.t_adj",
            "quantum.sx",
            "quantum.sx_adj",
        ] {
            assert!(
                !SELF_INVERSE.contains(&quarter),
                "'{quarter}' repeated is another gate, not the identity"
            );
        }
    }

    #[test]
    fn the_opaque_term_stays_outside_the_prefixes_that_mark_an_operation_pure() {
        // `PureCne` matches on shape rather than on a name and asks this
        // predicate whether the shape is one it may merge. A carrier under one
        // of those prefixes would put congruent-node elimination on nodes that
        // stand for distinct instructions, and the only thing left refusing the
        // merge would be an incidental payload comparison.
        assert!(admits(OPAQUE));
        assert!(accept_producer([OPAQUE]).is_ok());
        assert!(!crate::graph_rewrites::is_pure_classical(OPAQUE));
    }

    #[test]
    fn the_gate_list_has_no_duplicate_and_no_structural_name() {
        let gates: BTreeSet<&str> = GATES.into_iter().collect();
        assert_eq!(gates.len(), GATES.len(), "a gate is named once");
        for structural in STRUCTURAL {
            assert!(
                !gates.contains(structural),
                "'{structural}' is structural and cannot also be a gate"
            );
        }
    }
}
