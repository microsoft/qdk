// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`NativeCalls`]: the calls to the CUDA Runtime and cuTensorNet that a
//! canonical MPS on the GPU needs.
//!
//! CI has no GPU, yet the code that makes these calls must run in its
//! tests. So it makes each native call through these traits, and is the
//! same whichever implementation answers them.
//!
//! ```text
//! NativeCalls            all three traits, which each implementation has
//!   ├─ session_calls.rs  SessionCalls: the device, stream and handle
//!   ├─ memory_calls.rs   MemoryCalls: device buffers, copies, workspaces
//!   └─ tensor_calls.rs   TensorCalls: tensor descriptors, gate splits,
//!                        QRs, pairwise contractions
//! the values passed      NativePointer: one type per kind of native
//!                        object, beside the trait that creates it (in
//!                        tensor_calls/, beside its operation);
//!                        complex.rs (Complex64Abi)
//! ```
//!
//! Each implementation has one file per trait. A new native call is a
//! method of the trait of its kind, implemented in that trait's file of
//! each implementation.
//!
//! The libraries name every object they create (a stream, a handle, a
//! descriptor, a device buffer) by an untyped pointer. Each kind has its
//! own [`NativePointer`] type, so that passing one kind where another is
//! expected fails to compile.

pub(crate) mod complex;
#[cfg(test)]
pub(crate) mod fake_libraries;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod loaded_libraries;
pub(crate) mod memory_calls;
pub(crate) mod session_calls;
pub(crate) mod tensor_calls;

use std::{ffi::c_void, ptr::NonNull};

/// The calls to the CUDA Runtime and cuTensorNet: the session's, the
/// memory's and the tensor API's, in one trait.
pub(crate) trait NativeCalls:
    session_calls::SessionCalls + tensor_calls::TensorCalls
{
}

impl<Calls> NativeCalls for Calls where
    Calls: session_calls::SessionCalls + tensor_calls::TensorCalls + ?Sized
{
}

/// An object a native library created, of one kind (a CUDA stream, a
/// cuTensorNet handle, a tensor descriptor, a device buffer…): the pointer
/// the library returned, which every call on the object passes back.
///
/// Each kind is its own type, declared by [`native_pointer!`], so a call
/// takes only the kind it names. The pointer is never null.
pub(crate) trait NativePointer: Copy {
    /// The object the library returned as `pointer`, which the caller knows
    /// to be of this kind.
    fn from_pointer(pointer: NonNull<c_void>) -> Self;
    /// The pointer the library returned.
    fn pointer(self) -> NonNull<c_void>;
    /// The pointer to pass back to the library.
    fn as_ptr(self) -> *mut c_void {
        self.pointer().as_ptr()
    }
}

/// Declares one kind of [`NativePointer`]: a `repr(transparent)` wrapper of
/// the pointer, which a native call unwraps (`as_ptr`) to pass it. Like the
/// pointer, it is `Copy` and neither `Send` nor `Sync`.
macro_rules! native_pointer {
    ($(#[$attribute:meta])* $name:ident) => {
        $(#[$attribute])*
        #[repr(transparent)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(crate) struct $name(std::ptr::NonNull<std::ffi::c_void>);

        impl $crate::engine::native_calls::NativePointer for $name {
            fn from_pointer(pointer: std::ptr::NonNull<std::ffi::c_void>) -> Self {
                Self(pointer)
            }

            fn pointer(self) -> std::ptr::NonNull<std::ffi::c_void> {
                self.0
            }
        }
    };
}
pub(crate) use native_pointer;

#[cfg(test)]
mod tests;
