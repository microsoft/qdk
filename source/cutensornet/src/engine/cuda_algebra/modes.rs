// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The mode labels of the algebra's tensor descriptors. Two tensors' modes
//! of one label are one index; a call's output keeps the labels it names.

/// A pair's left extent.
pub(super) const LEFT: i32 = 1;
/// The left site's qubit.
pub(super) const P: i32 = 2;
/// The bond the pair shares before the call.
pub(super) const BOND: i32 = 3;
/// The right site's qubit.
pub(super) const Q: i32 = 4;
/// A pair's right extent.
pub(super) const RIGHT: i32 = 5;
/// The left qubit once gated.
pub(super) const P_OUT: i32 = 6;
/// The right qubit once gated.
pub(super) const Q_OUT: i32 = 7;
/// The bond the call returns.
pub(super) const NEW_BOND: i32 = 8;
