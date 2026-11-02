// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::AdapterError;

/// A code's layer that wraps the error of its adapter's logic in its own
/// error formats it: it reads the message the logic made it from.
#[test]
fn an_adapter_error_displays_the_message_it_was_made_from() {
    let error = AdapterError::from(format!("block {} is out of range", 3));
    assert_eq!(
        format!("the layer failed: {error}"),
        "the layer failed: block 3 is out of range"
    );
}
