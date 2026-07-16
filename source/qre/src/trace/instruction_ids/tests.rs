// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;

#[test]
fn test_instruction_name_primary() {
    assert_eq!(instruction_name(H), Some("H"));
    assert_eq!(instruction_name(CNOT), Some("CNOT"));
    assert_eq!(instruction_name(T), Some("T"));
    assert_eq!(instruction_name(MEAS_Z), Some("MEAS_Z"));
}

#[test]
fn test_instruction_name_aliases_return_primary() {
    // Aliases should return the primary name
    assert_eq!(instruction_name(H_XZ), Some("H"));
    assert_eq!(instruction_name(CX), Some("CNOT"));
    assert_eq!(instruction_name(SQRT_Z), Some("S"));
    assert_eq!(instruction_name(SQRT_SQRT_Z), Some("T"));
}

#[test]
fn test_instruction_name_unknown() {
    assert_eq!(instruction_name(0x9999), None);
}
