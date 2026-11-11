// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![deny(missing_docs)]

//! Recipes: ready-made ways to use the qodec runtime's pieces.
//!
//! | Recipe | What it is |
//! |---|---|
//! | [`FrameDecoding`] | The reference [`Design`]: in every layer, the frame decoder on top of the spec encoder |
//! | [`Compiler`] | Encoders without decoders, lowering a whole program ahead of time |
//! | [`runtime()`] | The usual [`Runtime`] over a plan's layers |
//!
//! It re-exports the builder crate, and through it the pieces, model and
//! interface crates.

pub mod compiler;
pub mod frame_decoding;

pub use qodec_runtime_builder::*;

pub use compiler::{Compiler, CompilerPlan};
pub use frame_decoding::FrameDecoding;

// A plan is shared by shots on many threads.
const _: () = {
    const fn shareable<T: Send + Sync>() {}
    shareable::<CompilerPlan>();
};

/// One shot of `plan`: its layers, fresh, on a [`Runtime`] over `backend`,
/// which runs the bottom ISA. Pass `&mut backend` to reuse one backend across
/// shots.
pub fn runtime<'a, B, E>(plan: &QodecPlan, backend: B) -> QodecRuntime<'a, E>
where
    B: BatchBackend<E> + 'a,
{
    Runtime::new(
        plan.instantiate(),
        Box::new(backend),
        plan.top(),
        plan.bottom(),
    )
}
