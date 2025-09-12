// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{CliffordUnitary, ControlledPauli, Hadamard, PauliExponent, Swap};
use crate::pauli::{commutes_with, generic::PhaseExponent, Pauli, PauliBits, PauliUnitary};
use std::ops::Mul;

impl<Bits: PauliBits, Phase: PhaseExponent> ControlledPauli<Bits, Phase> {
    /// # Panics
    ///
    /// Will panic
    pub fn new(
        control: PauliUnitary<Bits, Phase>,
        target: PauliUnitary<Bits, Phase>,
    ) -> ControlledPauli<Bits, Phase> {
        assert!(commutes_with(&control, &target));
        assert!(control.is_order_two());
        assert!(target.is_order_two());
        ControlledPauli(control, target)
    }
}

impl<Bits: PauliBits, Phase: PhaseExponent> PauliExponent<Bits, Phase> {
    /// # Panics
    ///
    /// Will panic
    pub fn new(pauli: PauliUnitary<Bits, Phase>) -> PauliExponent<Bits, Phase> {
        assert!(pauli.is_order_two());
        PauliExponent(pauli)
    }
}

macro_rules! delegate_left_multiplication_template_variants {
    ($left:ident) => {
        impl<Bits: PauliBits, _Phase: PhaseExponent> Mul<CliffordUnitary> for &$left<Bits, _Phase> {
            type Output = CliffordUnitary;

            fn mul(self, mut clifford: CliffordUnitary) -> Self::Output {
                self * &mut clifford;
                clifford
            }
        }

        impl<Bits: PauliBits, _Phase: PhaseExponent> Mul<&mut CliffordUnitary>
            for $left<Bits, _Phase>
        {
            type Output = ();

            fn mul(self, clifford: &mut CliffordUnitary) -> Self::Output {
                &self * clifford;
            }
        }

        impl<Bits: PauliBits, _Phase: PhaseExponent> Mul<CliffordUnitary> for $left<Bits, _Phase> {
            type Output = CliffordUnitary;

            fn mul(self, mut clifford: CliffordUnitary) -> Self::Output {
                &self * &mut clifford;
                clifford
            }
        }
    };
}

macro_rules! delegate_left_multiplication_variants {
    ($left:ty) => {
        impl Mul<CliffordUnitary> for &$left {
            type Output = CliffordUnitary;

            fn mul(self, mut clifford: CliffordUnitary) -> Self::Output {
                self * &mut clifford;
                clifford
            }
        }

        impl Mul<&mut CliffordUnitary> for $left {
            type Output = ();

            fn mul(self, clifford: &mut CliffordUnitary) -> Self::Output {
                &self * clifford;
            }
        }

        impl Mul<CliffordUnitary> for $left {
            type Output = CliffordUnitary;

            fn mul(self, mut clifford: CliffordUnitary) -> Self::Output {
                &self * &mut clifford;
                clifford
            }
        }
    };
}

delegate_left_multiplication_template_variants!(PauliUnitary);
delegate_left_multiplication_template_variants!(ControlledPauli);
delegate_left_multiplication_template_variants!(PauliExponent);
delegate_left_multiplication_variants!(Swap);
delegate_left_multiplication_variants!(Hadamard);
