// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a producer knows about the wire an operand names.
//!
//! This is not an alias analysis and is not on the way to becoming one. It is a
//! decision procedure with three answers, and the third one is the point: a
//! caller that cannot be told two operands are the same wire declines, and
//! declining is the safe side of every question built on this.
//!
//! Three producers ask the question in three different situations and only one
//! of them is easy. A lowered circuit resolves every operand to a durable
//! resource identity, so identity is decidable. Below the Adaptive profile the
//! runtime IR names qubits by literal index, which is the same case. At the
//! Adaptive profile, and at FIR, an operand is a binding rather than an index:
//! a qubit is never a static index at FIR because the index is minted later,
//! and no alias analysis exists at that level anywhere in this tree. Two
//! bindings that are not the same binding may still hold the same wire at run
//! time, and nothing here pretends otherwise.
//!
//! So the abstraction admits exactly what can be proved and declines the rest.
//! [`Operand::Binding`] carries an obligation rather than a fact, and it is
//! stated on the variant because a producer that constructs one without meeting
//! it is where an unsound answer would come from.

use std::collections::BTreeSet;

use crate::identity::ResourceId;

/// What a producer knows about the wire one operand names.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Operand {
    /// A wire the producer resolved to a durable resource identity.
    Resource(ResourceId),
    /// A wire named by a binding rather than resolved to an identity.
    ///
    /// Constructing this asserts that the binding holds one wire over the whole
    /// span any caller will compare across. A producer that cannot show that,
    /// because the binding is assigned more than once or because the span
    /// crosses a boundary it did not analyze, owes [`Operand::Opaque`] instead.
    /// Nothing downstream can recover the difference.
    Binding(u32),
    /// A wire the producer could put in neither form.
    Opaque,
}

/// What can be said about two operands naming the same wire.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WireIdentity {
    /// Provably the same wire.
    Same,
    /// Provably not the same wire.
    Distinct,
    /// Neither could be proved. Callers decline.
    Unknown,
}

impl Operand {
    /// What can be said about these two operands naming the same wire.
    ///
    /// `Distinct` is claimed only between two resolved identities, because that
    /// is the only case where being different is a fact rather than the absence
    /// of a proof. Two bindings that differ are `Unknown`, not `Distinct`: a
    /// producer that treated them as different wires would be reasoning from
    /// how the program was written rather than from what it does.
    #[must_use]
    pub fn identity(self, other: Self) -> WireIdentity {
        match (self, other) {
            (Self::Resource(left), Self::Resource(right)) => {
                if left == right {
                    WireIdentity::Same
                } else {
                    WireIdentity::Distinct
                }
            }
            (Self::Binding(left), Self::Binding(right)) if left == right => WireIdentity::Same,
            _ => WireIdentity::Unknown,
        }
    }

    /// The resource this operand resolves to, when it resolves to one.
    #[must_use]
    pub const fn resource(self) -> Option<ResourceId> {
        match self {
            Self::Resource(id) => Some(id),
            _ => None,
        }
    }
}

/// The wires one operation touches, in operand order.
///
/// Order is kept rather than reduced to a set, because two operations over the
/// same wires in a different order are not the same operation and a set cannot
/// say so.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OperandWires(Vec<Operand>);

impl OperandWires {
    #[must_use]
    pub fn new(operands: impl IntoIterator<Item = Operand>) -> Self {
        Self(operands.into_iter().collect())
    }

    /// Every operand resolved to a durable identity, or nothing.
    ///
    /// All or nothing rather than the resolved subset. A partial answer would
    /// read as a complete one to any caller comparing sets, and an operation
    /// whose second operand is unknown is not an operation whose operands are
    /// known.
    #[must_use]
    pub fn resolved(&self) -> Option<BTreeSet<ResourceId>> {
        self.0.iter().map(|operand| operand.resource()).collect()
    }

    #[must_use]
    pub fn operands(&self) -> &[Operand] {
        &self.0
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether these two operations touch the same wires in the same positions.
    ///
    /// A length mismatch is `Distinct` because the two cannot correspond
    /// operand for operand. Anything else is the weakest answer any position
    /// gave, so one unknown operand makes the whole comparison unknown.
    #[must_use]
    pub fn identity(&self, other: &Self) -> WireIdentity {
        if self.0.len() != other.0.len() || self.0.is_empty() {
            return WireIdentity::Distinct;
        }
        let mut answer = WireIdentity::Same;
        for (left, right) in self.0.iter().zip(&other.0) {
            match left.identity(*right) {
                WireIdentity::Same => {}
                WireIdentity::Distinct => return WireIdentity::Distinct,
                WireIdentity::Unknown => answer = WireIdentity::Unknown,
            }
        }
        answer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resource(id: u64) -> Operand {
        Operand::Resource(ResourceId(id))
    }

    #[test]
    fn a_resolved_pair_is_decided_in_both_directions() {
        assert_eq!(resource(3).identity(resource(3)), WireIdentity::Same);
        assert_eq!(resource(3).identity(resource(4)), WireIdentity::Distinct);
    }

    #[test]
    fn two_different_bindings_are_unknown_rather_than_distinct() {
        // The case the abstraction exists for. Two names are not two wires: at
        // the Adaptive profile and at FIR a binding may hold whatever a
        // preceding assignment put in it, so treating a difference in name as a
        // difference in wire would be reasoning about the text.
        assert_eq!(
            Operand::Binding(0).identity(Operand::Binding(1)),
            WireIdentity::Unknown
        );
        assert_eq!(
            Operand::Binding(0).identity(Operand::Binding(0)),
            WireIdentity::Same
        );
    }

    #[test]
    fn nothing_can_be_said_across_the_two_forms_or_about_an_opaque_operand() {
        assert_eq!(
            resource(0).identity(Operand::Binding(0)),
            WireIdentity::Unknown
        );
        assert_eq!(
            Operand::Opaque.identity(Operand::Opaque),
            WireIdentity::Unknown,
            "an operand the producer could not name is not the same wire as itself"
        );
        assert_eq!(Operand::Opaque.identity(resource(0)), WireIdentity::Unknown);
    }

    #[test]
    fn one_unresolved_operand_withdraws_the_whole_answer() {
        let known = OperandWires::new([resource(0), resource(1)]);
        let partial = OperandWires::new([resource(0), Operand::Opaque]);

        assert_eq!(known.resolved().map(|set| set.len()), Some(2));
        assert_eq!(
            partial.resolved(),
            None,
            "a partial answer would read as a complete one"
        );
        assert_eq!(known.identity(&partial), WireIdentity::Unknown);
    }

    #[test]
    fn position_is_part_of_the_question() {
        // cx(a, b) then cx(b, a) is not the identity, and a set comparison
        // cannot see the difference.
        let forward = OperandWires::new([resource(0), resource(1)]);
        let reversed = OperandWires::new([resource(1), resource(0)]);
        assert_eq!(forward.identity(&reversed), WireIdentity::Distinct);
        assert_eq!(forward.identity(&forward.clone()), WireIdentity::Same);
    }

    #[test]
    fn an_operation_over_no_wire_matches_nothing_including_itself() {
        let none = OperandWires::default();
        assert!(none.is_empty());
        assert_eq!(none.identity(&none), WireIdentity::Distinct);
        assert_eq!(
            none.identity(&OperandWires::new([resource(0)])),
            WireIdentity::Distinct
        );
    }
}
