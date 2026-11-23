// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Stand-ins for a backend's algebra, wrapping the CPU algebra for the
//! engine's tests that need more than its answers: a spy that counts the
//! engine's calls, a fake that fails the call a test chooses, and a fake
//! whose reads come back NaN.

pub(super) mod failing_fake_algebra;
pub(super) mod nan_reading_fake_algebra;
pub(super) mod spy_algebra;
