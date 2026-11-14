// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The list of each part's needs and offer that the admission reports:
//! [`PartList`].
//!
//! The traits' hidden `needs_by_part` and `offers_by_part` take one, and
//! code outside the crate cannot name it, because this module is private:
//! so those methods cannot be overridden outside the crate, and every part
//! is listed as the crate lists it. A part's needs and offer are listed
//! only when the admission fails, so a chain that fits builds no list.

use crate::{Capabilities, Needs};

/// Each part's needs, top to bottom, and offer, bottom to top, each named by
/// its type; or nothing, when the caller wants only the chain's own needs
/// and offer.
pub struct PartList {
    keep: bool,
    /// The needs listed, in the order they were.
    pub(crate) needs: Vec<(&'static str, Needs)>,
    /// The offers listed, in the order they were.
    pub(crate) offers: Vec<(&'static str, Capabilities)>,
}

impl PartList {
    /// A list that keeps what it is told.
    pub(crate) fn new() -> Self {
        Self {
            keep: true,
            needs: Vec::new(),
            offers: Vec::new(),
        }
    }

    /// A list that keeps nothing, for a part that only asks its parts'
    /// needs or offer.
    pub(crate) fn ignored() -> Self {
        Self {
            keep: false,
            ..Self::new()
        }
    }

    /// Lists `needs` as those of part `T`.
    pub(crate) fn note_needs<T: ?Sized>(&mut self, needs: &Needs) {
        if self.keep {
            self.needs.push((std::any::type_name::<T>(), needs.clone()));
        }
    }

    /// Lists `offer` as that of part `T`.
    pub(crate) fn note_offer<T: ?Sized>(&mut self, offer: &Capabilities) {
        if self.keep {
            self.offers
                .push((std::any::type_name::<T>(), offer.clone()));
        }
    }
}
