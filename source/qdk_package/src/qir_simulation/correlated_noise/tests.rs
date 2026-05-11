// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::parse_noise_table;
use expect_test::expect;

#[allow(clippy::float_cmp)]
#[test]
fn simple_noise_table() {
    let contents = "
II,0.9
IX,0.033
XI,0.033
XX,0.033";

    let noise_table = parse_noise_table(contents).expect("parsing should succeed");

    assert_eq!(noise_table.qubits, 2);
    assert_eq!(noise_table.loss, 0.0);
    assert_eq!(noise_table.pauli_noise.len(), 3);
    // II (key=0) is the identity — should be excluded.
    assert!(!noise_table.pauli_noise.contains_key(&0));
    // IX=1, XI=4, XX=5
    assert_eq!(noise_table.pauli_noise[&1], 0.033);
    assert_eq!(noise_table.pauli_noise[&4], 0.033);
    assert_eq!(noise_table.pauli_noise[&5], 0.033);
}

#[allow(clippy::float_cmp, clippy::unreadable_literal)]
#[test]
fn noise_table_scientific_notation() {
    let contents = "
II,0.994654476
IX,3.071369852930967e-08
XI,1.2949870973525467e-06
XX,1.401857148503582e-08";

    let noise_table = parse_noise_table(contents).expect("parsing should succeed");

    assert_eq!(noise_table.qubits, 2);
    assert_eq!(noise_table.loss, 0.0);
    assert_eq!(noise_table.pauli_noise.len(), 3);
    // II (key=0) is the identity — should be excluded.
    assert!(!noise_table.pauli_noise.contains_key(&0));
    // IX=1, XI=4, XX=5
    assert_eq!(noise_table.pauli_noise[&1], 3.071369852930967e-08);
    assert_eq!(noise_table.pauli_noise[&4], 1.2949870973525467e-06);
    assert_eq!(noise_table.pauli_noise[&5], 1.401857148503582e-08);
}

#[allow(clippy::float_cmp)]
#[test]
fn noise_table_with_comments() {
    let contents = "
# This is a comment
II,0.9
IX,0.033
# This is another comment
XI,0.033
XX,0.033";

    let noise_table = parse_noise_table(contents).expect("parsing should succeed");

    assert_eq!(noise_table.qubits, 2);
    assert_eq!(noise_table.loss, 0.0);
    assert_eq!(noise_table.pauli_noise.len(), 3);
    // II (key=0) is the identity — should be excluded.
    assert!(!noise_table.pauli_noise.contains_key(&0));
    // IX=1, XI=4, XX=5
    assert_eq!(noise_table.pauli_noise[&1], 0.033);
    assert_eq!(noise_table.pauli_noise[&4], 0.033);
    assert_eq!(noise_table.pauli_noise[&5], 0.033);
}

#[allow(clippy::float_cmp)]
#[test]
fn simple_noise_with_header() {
    let contents = "
pauli,probability
II,0.9
IX,0.033
XI,0.033
XX,0.033";

    let noise_table = parse_noise_table(contents).expect("parsing should succeed");

    assert_eq!(noise_table.qubits, 2);
    assert_eq!(noise_table.loss, 0.0);
    assert_eq!(noise_table.pauli_noise.len(), 3);
    // II (key=0) is the identity — should be excluded.
    assert!(!noise_table.pauli_noise.contains_key(&0));
    // IX=1, XI=4, XX=5
    assert_eq!(noise_table.pauli_noise[&1], 0.033);
    assert_eq!(noise_table.pauli_noise[&4], 0.033);
    assert_eq!(noise_table.pauli_noise[&5], 0.033);
}

#[allow(clippy::float_cmp)]
#[test]
fn noise_table_with_whitespaces() {
    let contents = "
 II , 0.9 
 IX , 0.033 
 XI , 0.033 
 XX , 0.033 ";

    let noise_table = parse_noise_table(contents).expect("parsing should succeed");

    assert_eq!(noise_table.qubits, 2);
    assert_eq!(noise_table.loss, 0.0);
    assert_eq!(noise_table.pauli_noise.len(), 3);
    // II (key=0) is the identity — should be excluded.
    assert!(!noise_table.pauli_noise.contains_key(&0));
    // IX=1, XI=4, XX=5
    assert_eq!(noise_table.pauli_noise[&1], 0.033);
    assert_eq!(noise_table.pauli_noise[&4], 0.033);
    assert_eq!(noise_table.pauli_noise[&5], 0.033);
}

#[allow(clippy::float_cmp)]
#[test]
fn noise_table_with_newlines() {
    let contents = "

II, 0.9

IX, 0.033

XI, 0.033

XX, 0.033

";

    let noise_table = parse_noise_table(contents).expect("parsing should succeed");

    assert_eq!(noise_table.qubits, 2);
    assert_eq!(noise_table.loss, 0.0);
    assert_eq!(noise_table.pauli_noise.len(), 3);
    // II (key=0) is the identity — should be excluded.
    assert!(!noise_table.pauli_noise.contains_key(&0));
    // IX=1, XI=4, XX=5
    assert_eq!(noise_table.pauli_noise[&1], 0.033);
    assert_eq!(noise_table.pauli_noise[&4], 0.033);
    assert_eq!(noise_table.pauli_noise[&5], 0.033);
}

#[test]
fn csv_row_with_single_col_errors() {
    let contents = "
pauli,probability
II,0.9
IX,0.033
XI
XX,0.033";
    let err = parse_noise_table(contents).expect_err("parsing should fail");
    expect![[r#"
        InvalidRow {
            line: 4,
            content: "XI",
        }
    "#]]
    .assert_debug_eq(&err);
}

#[test]
fn csv_row_with_single_more_than_two_cols_errors() {
    let contents = "
pauli,probability
II,0.9
IX,0.033
XI,0.033,third_col
XX,0.033";
    let err = parse_noise_table(contents).expect_err("parsing should fail");
    expect![[r#"
        InvalidRow {
            line: 4,
            content: "XI,0.033,third_col",
        }
    "#]]
    .assert_debug_eq(&err);
}

#[test]
fn csv_row_with_invalid_float_errors() {
    let contents = "
pauli,probability
II,0.9
IX,0.033
XI,0.033ABC
XX,0.033";
    let err = parse_noise_table(contents).expect_err("parsing should fail");
    expect![[r#"
        InvalidFloat {
            line: 4,
            content: "XI,0.033ABC",
        }
    "#]]
    .assert_debug_eq(&err);
}

#[test]
fn csv_row_with_invalid_probability_errors() {
    let contents = "
pauli,probability
II,0.9
IX,0.033
XI,1.5
XX,0.033";
    let err = parse_noise_table(contents).expect_err("parsing should fail");
    expect![[r#"
        InvalidProbability(
            1.5,
        )
    "#]]
    .assert_debug_eq(&err);
}

#[test]
fn csv_row_with_inconsistent_pauli_length_errors() {
    let contents = "
pauli,probability
II,0.9
IX,0.033
XIZ,0.033
XX,0.033";
    let err = parse_noise_table(contents).expect_err("parsing should fail");
    expect![[r#"
        InconsistentLength {
            expected: 2,
            found: 3,
        }
    "#]]
    .assert_debug_eq(&err);
}

#[test]
fn csv_row_with_lowercase_pauli_errors() {
    let contents = "
pauli,probability
II,0.9
IX,0.033
xi,0.033
XX,0.033";
    let err = parse_noise_table(contents).expect_err("parsing should fail");
    expect![[r#"
        InvalidPauliChar {
            line: 4,
            content: "xi,0.033",
        }
    "#]]
    .assert_debug_eq(&err);
}

#[test]
fn csv_row_with_invalid_pauli_char_errors() {
    let contents = "
pauli,probability
II,0.9
IX,0.033
XA,0.033
XX,0.033";
    let err = parse_noise_table(contents).expect_err("parsing should fail");
    expect![[r#"
        InvalidPauliChar {
            line: 4,
            content: "XA,0.033",
        }
    "#]]
    .assert_debug_eq(&err);
}
