// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What crosses a boundary: the shot sets batches run on, and the readouts
//! that come back for them. A batch's checks are those of the boundary that
//! receives it, in `composition/boundary_crossing.rs`.

mod batch_readouts;
mod shot_set;
