// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The roles' provided methods: the `_each` forms, which give rows and
//! outputs as they are final, and the `_stream` forms, which run a stream
//! of instructions in pieces; each by its default, and through the parts
//! that forward it.

mod adapt;
mod drive;
mod execute;
