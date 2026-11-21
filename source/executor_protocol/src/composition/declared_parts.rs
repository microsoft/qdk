// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The parts of a chain as they declare themselves, top to bottom
//! ([`DeclaredParts`]): what the [`on`](crate::DriverJoins::on) that makes
//! a runner checks the whole chain with, and what names each part.

use super::CompositionFault;
use crate::PartId;
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
    /// then each resource and feature that part does not serve, which the
    /// requirement reaches, against each part further below, down to the
    /// first that serves it.
    pub(crate) fn faults(&self) -> Vec<CompositionFault> {
        let mut faults = Vec::new();
        for (at, requirer) in self.parts.iter().enumerate() {
            let Some(mut reaching) = requirer.requires.cloned() else {
                continue;
            };
            for (below, provider) in self.parts.iter().enumerate().skip(at + 1) {
                let provided = (provider.provides).expect("each part below the driver provides");
                let fit = if below == at + 1 {
                    reaching.fits(provided).map(drop)
                } else {
                    reaching.kinds_fit(provided)
                };
                if let Err(mismatch) = fit {
                    faults.push(CompositionFault {
                        requirer: self.id(at),
                        provider: self.id(below),
                        mismatch,
                    });
                }
                reaching = reaching.not_served_by(provided);
            }
        }
        faults
    }

    /// Each part, by its position and name, top to bottom.
    pub(crate) fn ids(&self) -> Vec<PartId> {
        (0..self.parts.len()).map(|at| self.id(at)).collect()
    }

    /// The part at `at`, by its position and name.
    pub(crate) fn id(&self, at: usize) -> PartId {
        PartId::new(at, self.parts[at].name)
    }
}
