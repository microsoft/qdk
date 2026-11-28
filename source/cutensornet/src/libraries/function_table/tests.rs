// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What every function table does when it is filled.

use crate::test_utils::stand_ins::StandInResolver;
use crate::{LibraryFault, libraries::function_table::FunctionTable};

/// The functions a test library declares, which `function_table!` checks
/// its rows against.
mod declarations {
    pub(super) unsafe extern "C" fn zeta() {}
    pub(super) unsafe extern "C" fn alpha() {}
    pub(super) unsafe extern "C" fn mu() {}
}

/// The function-pointer types of the test library's functions, each named
/// as its function.
#[allow(
    non_camel_case_types,
    reason = "each function-pointer type is named as its C function"
)]
mod family {
    pub(super) type zeta = unsafe extern "C" fn();
    pub(super) type alpha = unsafe extern "C" fn();
    pub(super) type mu = unsafe extern "C" fn();
}

crate::libraries::function_table::function_table! {
    /// A test library's table, its rows out of alphabetical order, an
    /// optional one between two required ones.
    #[expect(dead_code, reason = "the tests read only the faults of filling it")]
    struct TestFunctions, declared in declarations {
        required family::zeta,
        optional family::alpha,
        required family::mu,
    }
}

/// A library that lacks required functions is refused, naming each one in
/// its table's order, so that the user fixes them all at once. A missing
/// optional function is not a fault.
#[test]
fn every_missing_required_function_is_reported_in_table_order() {
    let resolver = TestFunctions::ROWS
        .iter()
        .fold(StandInResolver::audited(), |resolver, row| {
            resolver.without(row.0)
        });
    let faults = TestFunctions::resolve(&resolver)
        .err()
        .expect("the required functions are missing");
    assert_eq!(
        faults,
        [
            LibraryFault::MissingRequiredSymbol { symbol: "zeta" },
            LibraryFault::MissingRequiredSymbol { symbol: "mu" },
        ]
    );
}
