// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the classical instructions compute, as the walk of a
//! [`QirMachine`](super::QirMachine) runs them: integer and real
//! arithmetic, conversions between integers and reals, comparisons by
//! LLVM's predicates, and a word read as a bit.

use super::super::QirTrap;
use crate::bytecode::encoding::{
    FCMP_FALSE, FCMP_OEQ, FCMP_OGE, FCMP_OGT, FCMP_OLE, FCMP_OLT, FCMP_ONE, FCMP_ORD, FCMP_TRUE,
    FCMP_UEQ, FCMP_UGE, FCMP_UGT, FCMP_ULE, FCMP_ULT, FCMP_UNE, FCMP_UNO, ICMP_EQ, ICMP_NE,
    ICMP_SGE, ICMP_SGT, ICMP_SLE, ICMP_SLT, ICMP_UGE, ICMP_UGT, ICMP_ULE, ICMP_ULT, OP_ADD, OP_AND,
    OP_ASHR, OP_FADD, OP_FDIV, OP_FMUL, OP_FPEXT, OP_FPTOSI, OP_FPTOUI, OP_FPTRUNC, OP_FREM,
    OP_FSUB, OP_INTTOPTR, OP_LSHR, OP_MOV, OP_MUL, OP_OR, OP_SDIV, OP_SHL, OP_SITOFP, OP_SREM,
    OP_SUB, OP_TRUNC, OP_UDIV, OP_UITOFP, OP_UREM, OP_XOR, OP_ZEXT,
};

/// `word` as a signed integer: the same bits.
#[allow(clippy::cast_possible_wrap)]
pub(super) fn signed(word: u64) -> i64 {
    word as i64
}

/// `value` as a word: the same bits.
#[allow(clippy::cast_sign_loss)]
pub(super) fn unsigned(value: i64) -> u64 {
    value as u64
}

/// The binary `opcode` on `a` and `b`, as a run computes it; `None` for a
/// division or a remainder by zero.
#[inline]
pub(super) fn binary(opcode: u8, a: u64, b: u64) -> Option<u64> {
    let real = |value: f64| Some(value.to_bits());
    let (x, y) = (f64::from_bits(a), f64::from_bits(b));
    // A run shifts by the low 32 bits of `b`, wrapping.
    #[allow(clippy::cast_possible_truncation)]
    let shift = b as u32;
    match opcode {
        // The same bits as on signed integers.
        OP_ADD => Some(a.wrapping_add(b)),
        OP_SUB => Some(a.wrapping_sub(b)),
        OP_MUL => Some(a.wrapping_mul(b)),
        OP_UDIV => a.checked_div(b),
        OP_UREM => a.checked_rem(b),
        OP_SDIV => (b != 0).then(|| unsigned(signed(a).wrapping_div(signed(b)))),
        OP_SREM => (b != 0).then(|| unsigned(signed(a).wrapping_rem(signed(b)))),
        OP_AND => Some(a & b),
        OP_OR => Some(a | b),
        OP_XOR => Some(a ^ b),
        OP_SHL => Some(a.wrapping_shl(shift)),
        OP_LSHR => Some(a.wrapping_shr(shift)),
        OP_ASHR => Some(unsigned(signed(a).wrapping_shr(shift))),
        OP_FADD => real(x + y),
        OP_FSUB => real(x - y),
        OP_FMUL => real(x * y),
        OP_FDIV => real(x / y),
        // `%` is C's `fmod`, as LLVM's `frem`.
        OP_FREM => real(x % y),
        _ => unreachable!("the walk calls `binary` only on a binary opcode"),
    }
}

/// The unary `opcode` on `a`, as a run computes it: a copy, or a
/// conversion between integers and reals.
#[inline]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
pub(super) fn unary(opcode: u8, a: u64) -> u64 {
    match opcode {
        // A word holds every width: these keep its bits.
        OP_ZEXT | OP_TRUNC | OP_INTTOPTR | OP_MOV | OP_FPEXT | OP_FPTRUNC => a,
        OP_FPTOSI => unsigned(f64::from_bits(a) as i64),
        OP_FPTOUI => f64::from_bits(a) as u64,
        OP_SITOFP => (signed(a) as f64).to_bits(),
        OP_UITOFP => (a as f64).to_bits(),
        _ => unreachable!("the walk calls `unary` only on a unary opcode"),
    }
}

/// `a` and `b` compared by the predicate `predicate`, as signed or
/// unsigned integers.
pub(super) fn int_compare(predicate: u8, a: u64, b: u64) -> bool {
    let (x, y) = (signed(a), signed(b));
    match predicate {
        ICMP_EQ => a == b,
        ICMP_NE => a != b,
        ICMP_SLT => x < y,
        ICMP_SLE => x <= y,
        ICMP_SGT => x > y,
        ICMP_SGE => x >= y,
        ICMP_ULT => a < b,
        ICMP_ULE => a <= b,
        ICMP_UGT => a > b,
        ICMP_UGE => a >= b,
        _ => unreachable!("the check refused an unknown integer predicate"),
    }
}

/// `a` and `b` compared by the predicate `predicate`, as LLVM's `fcmp`
/// does: an ordered predicate is false, an unordered one true, when
/// either is NaN; but for `one`, which is true then (below).
#[allow(clippy::float_cmp)]
pub(super) fn float_compare(predicate: u8, a: f64, b: f64) -> bool {
    let unordered = a.is_nan() || b.is_nan();
    match predicate {
        FCMP_FALSE => false,
        FCMP_OEQ => a == b,
        FCMP_OGT => a > b,
        FCMP_OGE => a >= b,
        FCMP_OLT => a < b,
        FCMP_OLE => a <= b,
        FCMP_ORD => !unordered,
        FCMP_UNO => unordered,
        FCMP_UEQ => unordered || a == b,
        FCMP_UGT => unordered || a > b,
        FCMP_UGE => unordered || a >= b,
        FCMP_ULT => unordered || a < b,
        FCMP_ULE => unordered || a <= b,
        // True when either is NaN: right for `une`, but a known deviation
        // from LLVM for `one`, which is false then. It is kept to match the
        // bytecode's reference interpreter and its tests.
        FCMP_ONE | FCMP_UNE => a != b,
        FCMP_TRUE => true,
        _ => unreachable!("the check refused an unknown real predicate"),
    }
}

/// `value`, read as a bit by the instruction at `pc`: 0 is false and 1
/// true; any other value traps.
#[inline]
pub(super) fn bit(pc: usize, value: u64) -> Result<bool, QirTrap> {
    // A non-bit can only come from the pass, which does not wrap narrow
    // integer results (`add i1 true, true` is 2); it traps so that the bug
    // shows.
    match value {
        0 => Ok(false),
        1 => Ok(true),
        value => Err(QirTrap::NotABit {
            instruction: pc,
            value,
        }),
    }
}
