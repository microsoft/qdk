// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::span::Span;

use qsc_data_structures::functors::FunctorApp;
use qsc_fir::fir::{PackageId, StoreItemId};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub span: Span,
    pub id: StoreItemId,
    pub caller: PackageId,
    pub functor: FunctorApp,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct CallStack {
    frames: Vec<Frame>,
}

impl CallStack {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    #[must_use]
    pub fn frames(&self) -> Vec<Frame> {
        self.frames.clone()
    }

    pub fn push_frame(&mut self, frame: Frame) {
        self.frames.push(frame);
    }

    pub fn pop_frame(&mut self) -> Option<Frame> {
        self.frames.pop()
    }
}
