// fn is_sorted<Items: Iterator<Item = usize>>(items: Items) -> bool {
//     let vec_items: Vec<usize> = items.collect();
//     equal(sorted(vec_items.iter()), vec_items.iter())
// }

// MulAssign for PauliUnitary and PauliUnitaryAbs with rhs that implements Pauli or PauliAbs

use std::ops::{Mul, MulAssign, Neg};

use crate::NeutralElement;

use super::{
    generic::{PhaseExponent, PhaseExponentMutable},
    Pauli, PauliBinaryOps, PauliBits, PauliMutable, PauliNeutralElement, PauliUnitary,
    PauliUnitaryProjective,
};

impl<BitsLeft, PhaseLeft, PauliRight: Pauli<PhaseExponentValue = u8>> MulAssign<&PauliRight>
    for PauliUnitary<BitsLeft, PhaseLeft>
where
    PauliUnitary<BitsLeft, PhaseLeft>: PauliBinaryOps<PauliRight>,
    BitsLeft: PauliBits,
    PhaseLeft: PhaseExponent,
{
    fn mul_assign(&mut self, other: &PauliRight) {
        self.mul_assign_right(other);
    }
}

impl<BitsLeft, PauliRight: Pauli<PhaseExponentValue = u8>> MulAssign<&PauliRight>
    for PauliUnitaryProjective<BitsLeft>
where
    PauliUnitaryProjective<BitsLeft>: PauliBinaryOps<PauliRight>,
    BitsLeft: PauliBits,
{
    fn mul_assign(&mut self, other: &PauliRight) {
        self.mul_assign_right(other);
    }
}

pub struct Phase<Exponent: PhaseExponent>(Exponent);

impl<Exponent: PhaseExponent> Phase<Exponent> {
    pub fn from_exponent(exponent: Exponent) -> Self {
        Self(exponent)
    }
}

// Multiply an object by a reference on the right, consume the object return multiplication result
impl<BitsLeft, PhaseLeft, PauliRight: Pauli<PhaseExponentValue = u8>> Mul<&PauliRight>
    for PauliUnitary<BitsLeft, PhaseLeft>
where
    PauliUnitary<BitsLeft, PhaseLeft>: for<'a> MulAssign<&'a PauliRight>,
    BitsLeft: PauliBits,
    PhaseLeft: PhaseExponent,
{
    type Output = PauliUnitary<BitsLeft, PhaseLeft>;

    fn mul(mut self, other: &PauliRight) -> Self::Output {
        self *= other;
        self
    }
}

impl<BitsLeft, PauliRight: Pauli<PhaseExponentValue = u8>> Mul<&PauliRight>
    for PauliUnitaryProjective<BitsLeft>
where
    PauliUnitaryProjective<BitsLeft>: for<'a> MulAssign<&'a PauliRight>,
    BitsLeft: PauliBits,
{
    type Output = PauliUnitaryProjective<BitsLeft>;

    fn mul(mut self, other: &PauliRight) -> Self::Output {
        self *= other;
        self
    }
}

// Multiply an object by a reference on the left, consume the object return multiplication result

impl<BitsLeft, PhaseLeft, PauliRight: Pauli<PhaseExponentValue = u8>> Mul<PauliRight>
    for &PauliUnitary<BitsLeft, PhaseLeft>
where
    PauliRight: PauliBinaryOps<PauliUnitary<BitsLeft, PhaseLeft>>,
    BitsLeft: PauliBits,
    PhaseLeft: PhaseExponent,
{
    type Output = PauliRight;

    fn mul(self, mut other: PauliRight) -> Self::Output {
        other.mul_assign_left(self);
        other
    }
}

impl<BitsLeft, PauliRight: Pauli<PhaseExponentValue = ()>> Mul<PauliRight>
    for &PauliUnitaryProjective<BitsLeft>
where
    PauliRight: PauliBinaryOps<PauliUnitaryProjective<BitsLeft>>,
    BitsLeft: PauliBits,
{
    type Output = PauliRight;

    fn mul(self, mut other: PauliRight) -> Self::Output {
        other.mul_assign_left(self);
        other
    }
}

// Multiplying by a phase

impl<Bits: PauliBits, _Phase: PhaseExponentMutable, Exponent: PhaseExponent>
    MulAssign<Phase<Exponent>> for PauliUnitary<Bits, _Phase>
where
    PauliUnitary<Bits, _Phase>: PauliMutable + Pauli<PhaseExponentValue = u8>,
{
    fn mul_assign(&mut self, phase: Phase<Exponent>) {
        self.add_assign_phase_exp(phase.0.raw_value());
    }
}

impl<Bits: PauliBits, _Phase: PhaseExponentMutable, Exponent: PhaseExponent> Mul<Phase<Exponent>>
    for PauliUnitary<Bits, _Phase>
where
    PauliUnitary<Bits, _Phase>: PauliMutable + Pauli<PhaseExponentValue = u8>,
{
    type Output = PauliUnitary<Bits, _Phase>;

    fn mul(mut self, phase: Phase<Exponent>) -> Self::Output {
        self.add_assign_phase_exp(phase.0.raw_value());
        self
    }
}

impl<BitsLeft, PhaseLeft, BitsRight, PhaseRight> Mul<&PauliUnitary<BitsRight, PhaseRight>>
    for &PauliUnitary<BitsLeft, PhaseLeft>
where
    PauliUnitary<BitsLeft, PhaseLeft>: PauliNeutralElement,
    <PauliUnitary<BitsLeft, PhaseLeft> as NeutralElement>::NeutralElementType:
        PauliBinaryOps<PauliUnitary<BitsRight, PhaseRight>>,
    BitsRight: PauliBits,
    PhaseRight: PhaseExponent,
    BitsLeft: PauliBits,
    PhaseLeft: PhaseExponent,
{
    type Output = <PauliUnitary<BitsLeft, PhaseLeft> as NeutralElement>::NeutralElementType;

    fn mul(self, other: &PauliUnitary<BitsRight, PhaseRight>) -> Self::Output {
        let mut res = <PauliUnitary<BitsLeft, PhaseLeft> as NeutralElement>::neutral_element(self);
        res.assign(self);
        res.mul_assign_right(other);
        res
    }
}

impl<BitsLeft, BitsRight> Mul<&PauliUnitaryProjective<BitsRight>>
    for &PauliUnitaryProjective<BitsLeft>
where
    PauliUnitaryProjective<BitsLeft>: PauliNeutralElement,
    <PauliUnitaryProjective<BitsLeft> as NeutralElement>::NeutralElementType:
        PauliBinaryOps<PauliUnitaryProjective<BitsRight>>,
    BitsRight: PauliBits,
    BitsLeft: PauliBits,
{
    type Output = <PauliUnitaryProjective<BitsLeft> as NeutralElement>::NeutralElementType;

    fn mul(self, other: &PauliUnitaryProjective<BitsRight>) -> Self::Output {
        let mut res = <PauliUnitaryProjective<BitsLeft> as NeutralElement>::neutral_element(self);
        res.assign(self);
        res.mul_assign_right(other);
        res
    }
}

// negating object consumes it and returns the negation
impl<Bits: PauliBits, Phase: PhaseExponentMutable> Neg for PauliUnitary<Bits, Phase>
where
    PauliUnitary<Bits, Phase>: PauliMutable + Pauli<PhaseExponentValue = u8>,
{
    type Output = Self;
    fn neg(mut self) -> Self::Output {
        self.add_assign_phase_exp(2u8);
        self
    }
}

impl<Bits: PauliBits> Neg for PauliUnitaryProjective<Bits>
where
    PauliUnitaryProjective<Bits>: PauliMutable,
{
    type Output = Self;
    fn neg(self) -> Self::Output {
        self
    }
}

// negating object consumes it and returns the negation
impl<Bits: PauliBits + Clone, Phase: PhaseExponentMutable + Clone> Neg
    for &PauliUnitary<Bits, Phase>
where
    PauliUnitary<Bits, Phase>: PauliMutable + Pauli<PhaseExponentValue = u8>,
{
    type Output = PauliUnitary<Bits, Phase>;
    fn neg(self) -> Self::Output {
        let mut res = self.clone();
        res.add_assign_phase_exp(2u8);
        res
    }
}

impl<Bits: PauliBits + Clone> Neg for &PauliUnitaryProjective<Bits>
where
    PauliUnitaryProjective<Bits>: PauliMutable,
{
    type Output = PauliUnitaryProjective<Bits>;
    fn neg(self) -> Self::Output {
        self.clone()
    }
}
