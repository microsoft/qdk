// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a part's `ready` answers when it cannot take a run now:
//! [`NotReady`].

use std::error::Error;
use std::fmt;

use crate::PartId;

/// Why a part cannot take a run now, such as a device that is busy or
/// calibrating, or a remote executor whose link is down: the run is then
/// refused before anything runs, naming the part
/// ([`RunRefusal::PartNotReady`](crate::RunRefusal::PartNotReady)), and may
/// be sent again later.
#[derive(Debug)]
pub struct NotReady {
    why: Box<dyn Error + Send + Sync>,
    /// The part that answered it, once the joined part holding that part
    /// has named it; the runner names the top parts.
    part: Option<PartId>,
}

impl NotReady {
    /// Not ready, for `why`.
    pub fn new(why: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self {
            why: why.into(),
            part: None,
        }
    }

    /// Why the part is not ready.
    #[must_use]
    pub fn why(&self) -> &(dyn Error + Send + Sync + 'static) {
        &*self.why
    }

    /// Names `part`, the top part of the half that answered it, as the
    /// part that did, unless a joined part inside that half already named
    /// a part below its top; `None`, where the chain's check did not reach
    /// the joined part, leaves it unnamed, for the part above to name.
    pub(crate) fn answered_by(mut self, part: Option<&PartId>) -> Self {
        if self.part.is_none() {
            self.part = part.cloned();
        }
        self
    }

    /// The part named as the one that answered it, taken out, so that a
    /// part relaying it from another chain does not carry that chain's
    /// name into its own.
    pub(crate) fn take_part(&mut self) -> Option<PartId> {
        self.part.take()
    }
}

impl fmt::Display for NotReady {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not ready: {}", self.why)
    }
}

impl Error for NotReady {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&*self.why)
    }
}
