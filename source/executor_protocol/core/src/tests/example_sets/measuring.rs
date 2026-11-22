// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Measuring`]: gates named by string, of which `mz` writes a readout
//! slot.

use std::convert::Infallible;
use std::error::Error;
use std::fmt;

use super::{BitReadout, ShotFlags};
use crate::{BoundaryCompatible, Capabilities, Features, InstructionSet, ReadoutSlots, Resources};

/// Gates named by string, each on one qubit, of which `mz` measures its
/// qubit into one readout slot. An instruction names its gate by its
/// position in the set it is encoded in: a sender fits a receiver that
/// accepts every name it sends, and its binding is, for each name it
/// sends, that name's position in the receiver's set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Measuring {
    names: &'static [&'static str],
}

/// A gate of a [`Measuring`] set on a qubit: the position of its name in
/// the set it is encoded in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Gate {
    pub(crate) at: u32,
    pub(crate) qubit: u32,
}

impl Measuring {
    /// The gates `names`, in their order.
    pub(crate) const fn new(names: &'static [&'static str]) -> Self {
        Self { names }
    }

    /// The gate `name` of this set on `qubit`.
    pub(crate) fn gate(&self, name: &str, qubit: u32) -> Gate {
        let at = self
            .names
            .iter()
            .position(|known| *known == name)
            .unwrap_or_else(|| panic!("the set {self:?} has a gate `{name}`"));
        Gate {
            at: u32::try_from(at).expect("a set of fewer than 2^32 gates"),
            qubit,
        }
    }

    /// The name of `gate`, encoded in this set; `None` beyond it.
    pub(crate) fn name(&self, gate: Gate) -> Option<&'static str> {
        self.names.get(gate.at as usize).copied()
    }

    /// `gate`, encoded in this set, re-encoded with `binding`, the
    /// binding of this set's fit into a receiver's; a gate beyond this set
    /// is passed on as it is, as a part forwarding a faulty batch would.
    pub(crate) fn bound(binding: &[u32], gate: Gate) -> Gate {
        Gate {
            at: binding.get(gate.at as usize).copied().unwrap_or(gate.at),
            ..gate
        }
    }
}

/// No capabilities of its own: a [`Measuring`] set takes any qubit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NoCapabilities;

impl BoundaryCompatible for NoCapabilities {
    type Fit = ();
    type Mismatch = Infallible;

    fn fits(&self, _provided: &Self) -> Result<(), Infallible> {
        Ok(())
    }
}

impl BoundaryCompatible for Measuring {
    type Fit = Vec<u32>;
    type Mismatch = UnacceptedNames;

    fn fits(&self, accepted: &Self) -> Result<Vec<u32>, UnacceptedNames> {
        let mut unaccepted = Vec::new();
        let binding = (self.names.iter())
            .map(|name| {
                let at = accepted.names.iter().position(|known| known == name);
                at.map_or_else(
                    || {
                        unaccepted.push(*name);
                        0
                    },
                    |at| u32::try_from(at).expect("a set of fewer than 2^32 gates"),
                )
            })
            .collect();
        if unaccepted.is_empty() {
            Ok(binding)
        } else {
            Err(UnacceptedNames(unaccepted))
        }
    }
}

impl InstructionSet for Measuring {
    type Instruction = Gate;
    type Capabilities = NoCapabilities;
    type Requirement = ();
    type Fault = UnknownGate;
    type Readout = BitReadout;
    type ShotConditions = ShotFlags;
    const UNWRITTEN_READOUT: BitReadout = BitReadout::Unwritten;

    fn unconstrained() -> &'static Capabilities<Self> {
        static UNCONSTRAINED: Capabilities<Measuring> = Capabilities::new(
            Resources::unconstrained(),
            Features::unconstrained(),
            NoCapabilities,
        );
        &UNCONSTRAINED
    }

    fn readout_slots(&self, gate: &Gate) -> ReadoutSlots {
        u32::from(self.name(*gate) == Some("mz"))
    }

    fn requirement(&self, _required: &NoCapabilities) {}

    fn check(&self, gate: &Gate, _required: &()) -> Result<ReadoutSlots, UnknownGate> {
        match self.name(*gate) {
            Some(_) => Ok(self.readout_slots(gate)),
            None => Err(UnknownGate { at: gate.at }),
        }
    }
}

/// The names a sender sends that its receiver does not accept, in the
/// sender's order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UnacceptedNames(pub(crate) Vec<&'static str>);

impl fmt::Display for UnacceptedNames {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "gates not accepted: {}", self.0.join(", "))
    }
}

impl Error for UnacceptedNames {}

/// A gate beyond the receiver's set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UnknownGate {
    pub(crate) at: u32,
}

impl fmt::Display for UnknownGate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "no gate at position {} of the set", self.at)
    }
}

impl Error for UnknownGate {}
