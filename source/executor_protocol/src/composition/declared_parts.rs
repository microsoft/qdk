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
            let Some(required) = requirer.requires else {
                continue;
            };
            let fault = |provider: usize, mismatch| CompositionFault {
                requirer: self.id(at),
                provider: self.id(provider),
                mismatch,
            };
            let mut below = self.parts.iter().enumerate().skip(at + 1);
            let Some((next, provider)) = below.next() else {
                continue;
            };
            if let Some(provided) = provider.provides
                && let Err(mismatch) = required.fits(provided)
            {
                faults.push(fault(next, mismatch));
            }
            let Some(answered) = provider.requires else {
                continue;
            };
            let mut forwarded = required.unanswered(answered);
            for (further, provider) in below {
                if forwarded.states_nothing() {
                    break;
                }
                if let Some(provided) = provider.provides
                    && let Err(mismatch) = forwarded.kinds_fit(provided)
                {
                    faults.push(fault(further, mismatch));
                }
                let Some(answered) = provider.requires else {
                    break;
                };
                forwarded = forwarded.unanswered(answered);
            }
        }
        faults
    }

    /// What the parts below the top one provide together, as each run is
    /// bounded by: each part's provision, the stricter where two state a
    /// resource or a feature, through each part that does not require it
    /// again; `None` for a chain of one part.
    pub(crate) fn provision(&self) -> Option<Capabilities> {
        self.parts
            .iter()
            .skip(1)
            .rev()
            .fold(None, |below: Option<Capabilities>, part| {
                let Some(provided) = part.provides else {
                    return below;
                };
                Some(match (below, part.requires) {
                    (Some(below), Some(required)) => provided.stricter(&below.unanswered(required)),
                    (Some(below), None) => provided.stricter(&below),
                    (None, _) => provided.clone(),
                })
            })
    }

    /// The part at `at`, by its position and name.
    pub(crate) fn id(&self, at: usize) -> PartId {
        PartId::new(at, self.parts[at].name)
    }
}
