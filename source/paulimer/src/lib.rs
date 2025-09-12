pub mod bipartite_normal_form;
pub mod bits;
pub mod clifford;
pub mod operations;
pub mod outcome_complete_simulation;
pub mod outcome_specific_simulation;
pub mod pauli;
pub mod quantum_core;
pub mod setwise;

pub use operations::UnitaryOp;

// mod utils;

type Tuple2<T> = (T, T);
type Tuple4<T> = (T, T, T, T);
type Tuple8<T> = (T, T, T, T, T, T, T, T);
type Tuple2x2<T> = Tuple2<Tuple2<T>>;
type Tuple4x2<T> = Tuple4<Tuple2<T>>;

pub trait NeutralElement {
    type NeutralElementType: 'static;
    fn neutral_element(&self) -> Self::NeutralElementType;
    fn default_size_neutral_element() -> Self::NeutralElementType;
    fn neutral_element_of_size(size: usize) -> Self::NeutralElementType;
}

use clifford::CliffordUnitary;
use pauli::SparsePauli;
use quantum_core::PositionedPauliObservable;

pub trait Simulation {
    fn pauli_exp(&mut self, sparse_pauli: &[PositionedPauliObservable]);

    fn apply_unitary(&mut self, unitary_op: UnitaryOp, support: &[usize]);
    fn apply_clifford(&mut self, clifford: &CliffordUnitary, support: &[usize]);
    fn apply_permutation(&mut self, permutation: &[usize], support: &[usize]);

    fn controlled_pauli(
        &mut self,
        observable1: &[PositionedPauliObservable],
        observable2: &[PositionedPauliObservable],
    );
    fn pauli(&mut self, observable: &[PositionedPauliObservable]);
    fn random_bit(&mut self) -> usize;

    fn measure(&mut self, observable: &[PositionedPauliObservable]) -> usize;
    fn measure_with_hint(
        &mut self,
        observable: &[PositionedPauliObservable],
        hint: &[PositionedPauliObservable],
    ) -> usize;
    fn measure_sparse(&mut self, pauli: &SparsePauli) -> usize;

    fn conditional_pauli(
        &mut self,
        observable: &[PositionedPauliObservable],
        outcomes: &[usize],
        parity: bool,
    );

    fn assert_stabilizer(&self, observable: &[PositionedPauliObservable]);
    fn assert_stabilizer_up_to_sign(&self, observable: &[PositionedPauliObservable]);
    fn assert_anti_stabilizer(&self, observable: &[PositionedPauliObservable]);

    fn with_capacity(num_qubits: usize, num_outcomes: usize, num_random_outcomes: usize) -> Self;

    fn new() -> Self;

    fn num_random_outcomes(&self) -> usize;
    fn random_outcome_indicator(&self) -> &[bool];
}

#[must_use]
pub fn subscript_digits(number: usize) -> String {
    let mut res = String::new();
    for char in number.to_string().chars() {
        let digit = char.to_digit(10).unwrap_or_default() as usize;
        res.push(SUB_CHARS[digit]);
    }
    res
}

pub const SUB_CHARS: [char; 10] = ['₀', '₁', '₂', '₃', '₄', '₅', '₆', '₇', '₈', '₉'];
