use crate::quantum_core::PositionedPauliObservable;

use super::{PauliUnitaryProjective, SparsePauli};
use crate::{
    bits::BitVec,
    pauli::{generic::PauliUnitary, Pauli, PauliBinaryOps},
    NeutralElement,
};
pub type DensePauli = PauliUnitary<BitVec, u8>;
pub type DensePauliProjective = PauliUnitaryProjective<BitVec>;

impl From<&[PositionedPauliObservable]> for DensePauli {
    fn from(value: &[PositionedPauliObservable]) -> Self {
        let r: SparsePauli = value.into();
        match super::Pauli::max_qubit_id(&r) {
            Some(max_id) => {
                let mut dense = <DensePauli as crate::NeutralElement>::neutral_element_of_size(max_id + 1);
                super::PauliBinaryOps::assign(&mut dense, &r);
                dense
            }
            None => <DensePauli as crate::NeutralElement>::default_size_neutral_element(),
        }
    }
}

impl<const LENGTH: usize> From<[PositionedPauliObservable; LENGTH]> for DensePauli {
    fn from(pauli_observable: [PositionedPauliObservable; LENGTH]) -> Self {
        pauli_observable.as_slice().into()
    }
}

impl<const LENGTH: usize> From<&[PositionedPauliObservable; LENGTH]> for DensePauli {
    fn from(pauli_observable: &[PositionedPauliObservable; LENGTH]) -> Self {
        pauli_observable.as_slice().into()
    }
}

impl From<Vec<PositionedPauliObservable>> for DensePauli {
    fn from(value: Vec<PositionedPauliObservable>) -> Self {
        value.as_slice().into()
    }
}

impl From<&Vec<PositionedPauliObservable>> for DensePauli {
    fn from(value: &Vec<PositionedPauliObservable>) -> Self {
        value.as_slice().into()
    }
}

pub fn dense_from<PauliLike: Pauli>(pauli: &PauliLike, qubit_count: usize) -> DensePauli
where
    DensePauli: PauliBinaryOps<PauliLike>,
{
    let mut result = DensePauli::neutral_element_of_size(qubit_count);
    result.assign(pauli);
    result
}
