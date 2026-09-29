// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::f64::consts::{FRAC_1_SQRT_2, FRAC_PI_2, PI};

use num_complex::Complex64;
use qdk_simulators::execution::{OperatorMatrix, UnitaryOperation, basis_operator, unitary_matrix};

const O: Complex64 = Complex64::new(0.0, 0.0);
const L: Complex64 = Complex64::new(1.0, 0.0);
const I: Complex64 = Complex64::new(0.0, 1.0);

fn table(operation: UnitaryOperation) -> OperatorMatrix {
    unitary_matrix(operation).expect("tabulated gate")
}

/// The matrix as rows `M[out][in]`, read through the public accessors.
fn rows(matrix: &OperatorMatrix) -> Vec<Vec<Complex64>> {
    let dimension = matrix.dimension();
    (0..dimension)
        .map(|output| {
            (0..dimension)
                .map(|input| matrix.entry(output, input))
                .collect()
        })
        .collect()
}

fn apply(matrix: &OperatorMatrix, state: &[Complex64]) -> Vec<Complex64> {
    rows(matrix)
        .iter()
        .map(|row| row.iter().zip(state).map(|(m, v)| m * v).sum())
        .collect()
}

fn product(left: &OperatorMatrix, right: &OperatorMatrix) -> Vec<Vec<Complex64>> {
    let (left, right) = (rows(left), rows(right));
    (0..left.len())
        .map(|i| {
            (0..left.len())
                .map(|j| (0..left.len()).map(|k| left[i][k] * right[k][j]).sum())
                .collect()
        })
        .collect()
}

fn assert_rows_close(actual: &[Vec<Complex64>], expected: &[Vec<Complex64>]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        for (a, e) in actual.iter().zip(expected) {
            assert!((a - e).norm() <= 1e-15, "{actual:?} != {expected:?}");
        }
    }
}

fn one_qubit_gates() -> [UnitaryOperation; 6] {
    [
        UnitaryOperation::X { target: 0 },
        UnitaryOperation::H { target: 0 },
        UnitaryOperation::S { target: 0 },
        UnitaryOperation::Sx { target: 0 },
        UnitaryOperation::Rx {
            angle: 0.7,
            target: 0,
        },
        UnitaryOperation::Rz {
            angle: 0.7,
            target: 0,
        },
    ]
}

fn two_qubit_gates() -> [UnitaryOperation; 3] {
    [
        UnitaryOperation::Cx {
            control: 0,
            target: 1,
        },
        UnitaryOperation::Cz {
            control: 0,
            target: 1,
        },
        UnitaryOperation::Rzz {
            angle: 0.7,
            q1: 0,
            q2: 1,
        },
    ]
}

#[test]
fn constant_gates_match_their_textbook_matrices() {
    let h = Complex64::new(FRAC_1_SQRT_2, 0.0);
    let cases = [
        (
            UnitaryOperation::X { target: 0 },
            vec![vec![O, L], vec![L, O]],
        ),
        (
            UnitaryOperation::H { target: 0 },
            vec![vec![h, h], vec![h, -h]],
        ),
        (
            UnitaryOperation::S { target: 0 },
            vec![vec![L, O], vec![O, I]],
        ),
        (
            UnitaryOperation::Sx { target: 0 },
            vec![
                vec![(L + I) / 2.0, (L - I) / 2.0],
                vec![(L - I) / 2.0, (L + I) / 2.0],
            ],
        ),
        (
            UnitaryOperation::Cx {
                control: 0,
                target: 1,
            },
            vec![
                vec![L, O, O, O],
                vec![O, L, O, O],
                vec![O, O, O, L],
                vec![O, O, L, O],
            ],
        ),
        (
            UnitaryOperation::Cz {
                control: 0,
                target: 1,
            },
            vec![
                vec![L, O, O, O],
                vec![O, L, O, O],
                vec![O, O, L, O],
                vec![O, O, O, -L],
            ],
        ),
    ];
    for (operation, expected) in cases {
        assert_eq!(rows(&table(operation)), expected, "{operation:?}");
    }
}

#[test]
fn rotations_match_their_textbook_matrices_at_known_angles() {
    let h = FRAC_1_SQRT_2;
    let c = Complex64::new;
    // Rx(π/2) = (I - iX)/√2.
    assert_rows_close(
        &rows(&table(UnitaryOperation::Rx {
            angle: FRAC_PI_2,
            target: 0,
        })),
        &[vec![c(h, 0.0), c(0.0, -h)], vec![c(0.0, -h), c(h, 0.0)]],
    );
    // Rz(π) = diag(-i, i).
    assert_rows_close(
        &rows(&table(UnitaryOperation::Rz {
            angle: PI,
            target: 0,
        })),
        &[vec![-I, O], vec![O, I]],
    );
    // Rzz(π) = -i Z⊗Z = diag(-i, i, i, -i).
    assert_rows_close(
        &rows(&table(UnitaryOperation::Rzz {
            angle: PI,
            q1: 0,
            q2: 1,
        })),
        &[
            vec![-I, O, O, O],
            vec![O, I, O, O],
            vec![O, O, I, O],
            vec![O, O, O, -I],
        ],
    );
}

#[test]
fn the_first_operand_is_the_most_significant_bit() {
    let cx = table(UnitaryOperation::Cx {
        control: 0,
        target: 1,
    });
    // |control, target⟩ = |10⟩ is index 2 and becomes |11⟩, index 3; |01⟩ stays.
    assert_eq!(apply(&cx, &[O, O, L, O]), [O, O, O, L]);
    assert_eq!(apply(&cx, &[O, L, O, O]), [O, L, O, O]);
}

#[test]
fn basis_operators_map_the_read_bit_to_the_continuing_bit() {
    let zero = [L, O];
    let one = [O, L];
    for (result, basis, expected) in [
        (false, false, vec![vec![L, O], vec![O, O]]),
        (false, true, vec![vec![O, L], vec![O, O]]),
        (true, false, vec![vec![O, O], vec![L, O]]),
        (true, true, vec![vec![O, O], vec![O, L]]),
    ] {
        assert_eq!(rows(&basis_operator(result, basis)), expected);
    }
    // |0⟩⟨1| (read 1, then reset) is not Hermitian: a transposed layout
    // would keep |0⟩ and annihilate |1⟩ instead.
    let reset_after_one = basis_operator(false, true);
    assert_eq!(apply(&reset_after_one, &one), zero);
    assert_eq!(apply(&reset_after_one, &zero), [O, O]);
    assert_eq!(reset_after_one.row_major(), [O, L, O, O]);
}

#[test]
fn square_roots_square_to_their_gates() {
    let sx = table(UnitaryOperation::Sx { target: 0 });
    let s = table(UnitaryOperation::S { target: 0 });
    assert_eq!(
        product(&sx, &sx),
        rows(&table(UnitaryOperation::X { target: 0 }))
    );
    assert_eq!(product(&s, &s), [vec![L, O], vec![O, -L]]);
}

#[test]
fn every_tabulated_gate_is_unitary() {
    for operation in one_qubit_gates().into_iter().chain(two_qubit_gates()) {
        let matrix = rows(&table(operation));
        let dimension = matrix.len();
        let gram: Vec<Vec<Complex64>> = (0..dimension)
            .map(|i| {
                (0..dimension)
                    .map(|j| {
                        (0..dimension)
                            .map(|k| matrix[k][i].conj() * matrix[k][j])
                            .sum()
                    })
                    .collect()
            })
            .collect();
        let identity: Vec<Vec<Complex64>> = (0..dimension)
            .map(|i| (0..dimension).map(|j| if i == j { L } else { O }).collect())
            .collect();
        assert_rows_close(&gram, &identity);
    }
}

#[test]
fn shape_and_diagonality_are_reported() {
    for operation in one_qubit_gates() {
        let matrix = table(operation);
        assert_eq!((matrix.qubit_count(), matrix.dimension()), (1, 2));
        assert_eq!(matrix.row_major().len(), 4);
    }
    for operation in two_qubit_gates() {
        let matrix = table(operation);
        assert_eq!((matrix.qubit_count(), matrix.dimension()), (2, 4));
        assert_eq!(matrix.row_major().len(), 16);
    }
    let diagonal = |operation| table(operation).is_diagonal();
    let [x, h, s, sx, rx, rz] = one_qubit_gates().map(diagonal);
    let [cx, cz, rzz] = two_qubit_gates().map(diagonal);
    assert_eq!(
        [x, h, s, sx, rx, rz, cx, cz, rzz],
        [false, false, true, false, false, true, false, true, true]
    );
    assert!(basis_operator(true, true).is_diagonal());
    assert!(!basis_operator(false, true).is_diagonal());
}

#[test]
fn matrices_do_not_depend_on_operands() {
    assert_eq!(
        table(UnitaryOperation::Cz {
            control: 0,
            target: 1
        }),
        table(UnitaryOperation::Cz {
            control: 7,
            target: 3
        })
    );
    assert_eq!(
        table(UnitaryOperation::Rx {
            angle: 0.2,
            target: 0
        }),
        table(UnitaryOperation::Rx {
            angle: 0.2,
            target: 5
        })
    );
}

#[test]
fn gates_outside_the_table_have_no_matrix() {
    for operation in [
        UnitaryOperation::I { target: 0 },
        UnitaryOperation::Y { target: 0 },
        UnitaryOperation::Z { target: 0 },
        UnitaryOperation::SAdj { target: 0 },
        UnitaryOperation::SxAdj { target: 0 },
        UnitaryOperation::T { target: 0 },
        UnitaryOperation::TAdj { target: 0 },
        UnitaryOperation::Ry {
            angle: 0.1,
            target: 0,
        },
        UnitaryOperation::Cy {
            control: 0,
            target: 1,
        },
        UnitaryOperation::Rxx {
            angle: 0.1,
            q1: 0,
            q2: 1,
        },
        UnitaryOperation::Ryy {
            angle: 0.1,
            q1: 0,
            q2: 1,
        },
        UnitaryOperation::Swap { q1: 0, q2: 1 },
    ] {
        assert_eq!(unitary_matrix(operation), None, "{operation:?}");
    }
}

#[test]
#[should_panic(expected = "operator index out of range")]
fn entries_outside_the_matrix_panic() {
    let _ = basis_operator(false, false).entry(2, 0);
}
