// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::span::Span;

use qsc_data_structures::functors::FunctorApp;
use qsc_fir::fir::{ExprId, PackageId, StoreItemId};

#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub span: Span,
    pub id: StoreItemId,
    pub caller: PackageId,
    pub functor: FunctorApp,
    pub loop_iterations: Vec<LoopScope>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct LoopScope {
    pub loop_expr: ExprId,
    pub iteration_count: usize,
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
    pub fn to_frames(&self) -> Vec<Frame> {
        self.frames.clone()
    }

    pub fn push_frame(&mut self, frame: Frame) {
        self.frames.push(frame);
    }

    pub fn pop_frame(&mut self) -> Option<Frame> {
        self.frames.pop()
    }

    pub fn push_loop_iteration(&mut self, loop_expr: ExprId) {
        if let Some(frame) = self.frames.last_mut() {
            frame.loop_iterations.push(LoopScope {
                loop_expr,
                iteration_count: 0,
            });
        }
    }

    pub fn pop_loop_iteration(&mut self) {
        if let Some(frame) = self.frames.last_mut() {
            frame.loop_iterations.pop();
        }
    }

    pub fn increment_loop_iteration(&mut self) {
        if let Some(frame) = self.frames.last_mut()
            && let Some(loop_scope) = frame.loop_iterations.last_mut()
        {
            loop_scope.iteration_count += 1;
        }
    }
}
