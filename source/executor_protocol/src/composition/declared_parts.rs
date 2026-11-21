// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The parts of a chain as they declare themselves, top to bottom
//! ([`DeclaredParts`]): what the [`on`](crate::DriverJoins::on) that makes
//! a runner checks the whole chain with, and what bounds each run.

use super::{CompositionFault, PartId};
use crate::capabilities::Forwarding;
use crate::{BoundaryCompatible, Capabilities};

/// The parts of a chain, top to bottom, each with its name and what it
/// requires and provides, as the roles' hidden `declare` adds them.
///
/// It is public, because the roles' hidden hook names it, but no public
/// path reaches it, so that no other crate can call or override the hook.
pub struct DeclaredParts<'a> {
    parts: Vec<DeclaredPart<'a>>,
}

/// Why a chain's provision is found: the `on` that asks for it joins a
/// driver with an executor at the bottom.
const CHAIN: &str = "a chain has a driver above an executor";

/// One part, as it declares itself: a driver requires, an executor
/// provides, and an adapter does both.
struct DeclaredPart<'a> {
    name: &'a str,
    requires: Option<&'a Capabilities>,
    provides: Option<&'a Capabilities>,
}

impl<'a> DeclaredParts<'a> {
    /// No part yet.
    pub(crate) fn new() -> Self {
        Self { parts: Vec::new() }
    }

    /// Adds the part below those added: its name, what it requires and
    /// what it provides.
    pub(crate) fn push(
        &mut self,
        name: &'a str,
        requires: Option<&'a Capabilities>,
        provides: Option<&'a Capabilities>,
    ) {
        self.parts.push(DeclaredPart {
            name,
            requires,
            provides,
        });
    }

    /// How many parts were added.
    pub(crate) fn len(&self) -> usize {
        self.parts.len()
    }

    /// Every fault of the chain, by its requirer top to bottom: what each
    /// part requires against what the part below it provides, all of it;
    /// then each resource and feature the part below does not require
    /// again, which it forwards, against each part further below, until a
    /// part requires it again.
    pub(crate) fn faults(&self) -> Vec<CompositionFault> {
        let mut faults = Vec::new();
        for (at, requirer) in self.parts.iter().enumerate() {
            let Some(mut forwarded) = requirer.requires.cloned() else {
                continue;
            };
            for (below, provider) in self.parts.iter().enumerate().skip(at + 1) {
                let fit = match provider.provides {
                    Some(provided) if below == at + 1 => forwarded.fits(provided).map(drop),
                    Some(provided) => forwarded.kinds_fit(provided),
                    None => Ok(()),
                };
                if let Err(mismatch) = fit {
                    faults.push(CompositionFault {
                        requirer: self.id(at),
                        provider: self.id(below),
                        mismatch,
                    });
                }
                let Some(answered) = provider.requires else {
                    break;
                };
                forwarded = forwarded.unanswered(answered);
                if forwarded.states_nothing() {
                    break;
                }
            }
        }
        faults
    }

    /// What the parts below the top one provide together, as each run is
    /// bounded by: the bottom part's provision, then each adapter's above
    /// it, the stricter where two state a resource or a feature, through
    /// each adapter that does not require it again.
    pub(crate) fn provision(&self) -> Capabilities {
        let (bottom, adapters) = (self.parts[1..].split_last()).expect(CHAIN);
        let below = bottom.provides.expect(CHAIN).clone();
        adapters.iter().rev().fold(below, |below, adapter| {
            match (adapter.provides, adapter.requires) {
                (Some(provided), Some(required)) => provided.stricter_through(required, &below),
                _ => below,
            }
        })
    }

    /// The part at `at`, by its position and name.
    pub(crate) fn id(&self, at: usize) -> PartId {
        PartId::new(at, self.parts[at].name)
    }
}
