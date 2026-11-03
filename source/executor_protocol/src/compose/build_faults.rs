// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What building a chain reports: every fault of the chain
//! ([`BuildFaults`]), each a component and why it cannot be built
//! ([`BuildFault`], with its [`BuildError`]).

use std::fmt;
use std::sync::Arc;

use crate::instruction_set::write_mismatches;
#[cfg(doc)]
use crate::{Adapter, Rejected};
use crate::{InstructionMismatch, RngId, UnmetNeed};

/// A component of a chain that cannot be built against what is downstream
/// of it, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildFault {
    /// The component's type.
    pub component: &'static str,
    /// Why it cannot be built.
    pub error: BuildError,
}

/// Every fault of a chain built against an executor, each adapter's,
/// upstream first, so that whoever builds the chain can fix
/// them all at once. It reads as one message: each fault as its component
/// and its error (`component: error`), in their order, joined by `"; "`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildFaults(pub Vec<BuildFault>);

impl fmt::Display for BuildFaults {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_faults(f, &self.0, |f, BuildFault { component, error }| {
            write!(f, "{component}: {error}")
        })
    }
}

/// Writes `faults` as one message, in their order, each by `write`,
/// separated by `"; "`: the one spelling of a list of faults, for
/// [`BuildFaults`] and [`JobFaults`](crate::JobFaults).
pub(crate) fn write_faults<F>(
    f: &mut fmt::Formatter<'_>,
    faults: &[F],
    write: impl Fn(&mut fmt::Formatter<'_>, &F) -> fmt::Result,
) -> fmt::Result {
    for (i, fault) in faults.iter().enumerate() {
        if i > 0 {
            f.write_str("; ")?;
        }
        write(f, fault)?;
    }
    Ok(())
}

impl std::error::Error for BuildFaults {}

/// Why a component of a chain cannot be built against the components
/// downstream of it.
///
/// Two errors are equal when they state the same fault; an adapter's own
/// fault is equal only to itself and its clones, as its type is the
/// adapter's.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum BuildError {
    /// A component downstream already draws with this id, so the two would draw
    /// the same numbers in every shot: one of them must be given another id
    /// ([`AdapterFn::drawing`](crate::AdapterFn::drawing)).
    RngInUse(RngId),
    /// An adapter draws with this id ([`Adapter::draws`]), and no run's seed
    /// reaches it: the last executor below is given none
    /// ([`ExecutorFn::seed`](crate::ExecutorFn::seed)). Refused when built
    /// rather than at its first draw, since a seed made up for it would draw
    /// the same values in every run, and one drawn from the system could not be
    /// replayed; an [`adapter_fn`](fn@crate::adapter_fn) and an adapter written
    /// by hand alike.
    ///
    /// Only an [`executor_fn`](fn@crate::executor_fn) given a seed passes
    /// one up today. An executor written by hand cannot state one yet
    /// (planned), so an adapter that draws is refused over it: for example
    /// a noise model over the user's own simulator.
    NoRunSeed(RngId),
    /// An adapter sends instructions that the downstream executor does not
    /// accept: each of them, in the order the adapter declares them.
    Instructions(Vec<InstructionMismatch>),
    /// An adapter is built against an executor that describes no
    /// instruction set, so what it sends cannot be checked: an adapter that
    /// states the set it sends, or only the set it accepts and sends
    /// instructions of the kind it is called with (by opcode or typed), or
    /// one that sends calls by opcode and states no set: one that passes
    /// them through as they are, or one that makes them from instructions
    /// of another type, then also refused as [`Self::UnstatedSends`].
    NoInstructionSetDownstream,
    /// An adapter makes calls by opcode from instructions of another type,
    /// and states no set they are numbered by
    /// ([`Adapter::sends`]); the set it accepts, if
    /// any, describes the instructions it is called with, not its calls. So
    /// its opcodes cannot be checked downstream.
    UnstatedSends,
    /// A fault of the adapter's own, of a type it defines
    /// ([`AdapterBoundary::fault`](crate::AdapterBoundary::fault)): for example
    /// a code whose block needs more qubits than the downstream executor holds.
    /// Equal only to itself: two faults compare equal when they share one
    /// error.
    Adapter(Arc<dyn std::error::Error + Send + Sync>),
    /// A need of a component that the chain downstream of it does not meet
    /// ([`UnmetNeed`], as [`Rejected::Unmet`] states it). No component
    /// reports it yet: a job's unmet needs are reported when the job is
    /// given to the chain, among its [`JobFaults`](crate::JobFaults).
    /// Planned: an adapter that states what it needs downstream, such as a
    /// decoder that needs resumable shots, reports its unmet needs as this
    /// when the chain is built (`docs/planned.md`).
    Unmet(UnmetNeed),
}

impl PartialEq for BuildError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (BuildError::RngInUse(a), BuildError::RngInUse(b))
            | (BuildError::NoRunSeed(a), BuildError::NoRunSeed(b)) => a == b,
            (BuildError::Instructions(a), BuildError::Instructions(b)) => a == b,
            (BuildError::NoInstructionSetDownstream, BuildError::NoInstructionSetDownstream)
            | (BuildError::UnstatedSends, BuildError::UnstatedSends) => true,
            (BuildError::Adapter(a), BuildError::Adapter(b)) => Arc::ptr_eq(a, b),
            (BuildError::Unmet(a), BuildError::Unmet(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for BuildError {}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildError::RngInUse(id) => write!(
                f,
                "{id} are already drawn from downstream: two components would share them; \
                 give one of them another id, for example with \
                 adapter_fn(..).drawing(RngId::named(name))"
            ),
            BuildError::NoRunSeed(id) => write!(
                f,
                "{id} have no run's seed to be drawn from: give the last executor one, \
                 for example with executor_fn(..).seed(seed); for an executor written by \
                 hand, see BuildError::NoRunSeed"
            ),
            BuildError::Instructions(mismatches) => {
                f.write_str("the instructions sent do not fit downstream: ")?;
                write_mismatches(f, mismatches)
            }
            BuildError::NoInstructionSetDownstream => {
                f.write_str("the downstream executor describes no instruction set")
            }
            BuildError::UnstatedSends => f.write_str(
                "the adapter makes calls by opcode and states no set they are numbered by",
            ),
            BuildError::Adapter(fault) => fault.fmt(f),
            BuildError::Unmet(unmet) => unmet.fmt(f),
        }
    }
}

impl std::error::Error for BuildError {
    /// An adapter's own fault's source, as its text is the fault's.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BuildError::Adapter(fault) => fault.source(),
            _ => None,
        }
    }
}
