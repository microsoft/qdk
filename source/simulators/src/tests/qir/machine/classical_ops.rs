// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the classical instructions compute: comparisons, integer and
//! floating-point arithmetic, conversions and `gep`, each operand read
//! from a constant or a register by its flag.

use super::super::{binary, compare, op, program, record_bool, ret, set};
use super::{both, computed, computed_by, records, trap};
use crate::OutputRecord;
use crate::bytecode::Instruction;
use crate::bytecode::encoding::{
    FCMP_FALSE, FCMP_OEQ, FCMP_OGE, FCMP_OGT, FCMP_OLE, FCMP_OLT, FCMP_ONE, FCMP_ORD, FCMP_TRUE,
    FCMP_UEQ, FCMP_UGE, FCMP_UGT, FCMP_ULE, FCMP_ULT, FCMP_UNE, FCMP_UNO, FLAG_AUX0_IMM,
    FLAG_SRC0_IMM, FLAG_SRC1_IMM, ICMP_EQ, ICMP_NE, ICMP_SGE, ICMP_SGT, ICMP_SLE, ICMP_SLT,
    ICMP_UGE, ICMP_UGT, ICMP_ULE, ICMP_ULT, OP_ADD, OP_AND, OP_ASHR, OP_CONST, OP_FADD, OP_FCMP,
    OP_FDIV, OP_FMUL, OP_FPEXT, OP_FPTOSI, OP_FPTOUI, OP_FPTRUNC, OP_FREM, OP_FSUB, OP_GEP,
    OP_ICMP, OP_INTTOPTR, OP_LSHR, OP_MOV, OP_MUL, OP_NOP, OP_OR, OP_SDIV, OP_SEXT, OP_SHL,
    OP_SITOFP, OP_SREM, OP_SUB, OP_TRUNC, OP_UDIV, OP_UITOFP, OP_UREM, OP_XOR, OP_ZEXT,
};
use crate::qir::{QirReadout, QirTrap};

/// Each integer division and remainder of 5 by a register holding 0.
#[test]
fn a_division_by_zero_is_a_trap() {
    for opcode in [OP_UDIV, OP_SDIV, OP_UREM, OP_SREM] {
        let bytecode = program(&[&[binary(opcode, FLAG_SRC0_IMM, 1, 5, 0), ret()]]);
        assert_eq!(
            trap(&bytecode),
            QirTrap::DivisionByZero { instruction: 0 },
            "{opcode:#x}"
        );
    }
}

/// The records of a shot of a program comparing the constants `a` and
/// `b` by the `icmp` predicate `predicate`, recorded as a `Bool`: by a
/// machine, and by `run_shot`.
fn icmp(predicate: u8, a: i64, b: i64) -> (Vec<OutputRecord>, Vec<OutputRecord>) {
    let comparison = Instruction {
        dst: 0,
        src0: a.cast_unsigned(),
        src1: b.cast_unsigned(),
        opcode: compare(OP_ICMP, predicate).opcode | FLAG_SRC0_IMM | FLAG_SRC1_IMM,
        ..Instruction::default()
    };
    both(&program(&[&[comparison, record_bool(0), ret()]]))
}

/// Each of the 10 predicates of `icmp`, on -1 and 1, whose order as
/// signed integers is the reverse of their order as unsigned ones (-1 is
/// 2^64 - 1), in both orders, and on two equal numbers: true as LLVM
/// defines it, and as `run_shot` computes it.
#[test]
fn icmp_compares_by_each_predicate() {
    // Per predicate, whether it holds of (-1, 1), (1, 1), (1, -1).
    let table = [
        (ICMP_EQ, "010"),
        (ICMP_NE, "101"),
        (ICMP_SLT, "100"),
        (ICMP_SLE, "110"),
        (ICMP_SGT, "001"),
        (ICMP_SGE, "011"),
        (ICMP_ULT, "001"),
        (ICMP_ULE, "011"),
        (ICMP_UGT, "100"),
        (ICMP_UGE, "110"),
    ];
    let pairs = [(-1, 1), (1, 1), (1, -1)];
    for (predicate, holds) in table {
        for ((a, b), holds) in pairs.iter().zip(holds.chars()) {
            let (machine, run_shot) = icmp(predicate, *a, *b);
            assert_eq!(
                machine,
                [OutputRecord::Bool(holds == '1')],
                "predicate {predicate} of {a} and {b}"
            );
            // COMPATIBILITY NOTE: LLVM's comparisons are also those
            // `run_shot` (bytecode/runtime.rs, `OP_ICMP`) computes, hence
            // the machine keeps them, a drop-in replacement for it.
            assert_eq!(machine, run_shot, "predicate {predicate} of {a} and {b}");
        }
    }
}

/// The comparisons `fcmp` makes of `a` and `b` by each predicate, each
/// recorded as a `Bool`, in the order of `predicates`.
fn fcmp(predicates: &[u8], a: f64, b: f64) -> Vec<OutputRecord> {
    let mut instructions = Vec::new();
    for predicate in predicates {
        instructions.push(Instruction {
            dst: 0,
            src0: a.to_bits(),
            src1: b.to_bits(),
            opcode: compare(OP_FCMP, *predicate).opcode | FLAG_SRC0_IMM | FLAG_SRC1_IMM,
            ..Instruction::default()
        });
        instructions.push(record_bool(0));
    }
    instructions.push(ret());
    records(&program(&[&instructions]), |_| QirReadout::One)
}

/// Each of the 16 predicates of `fcmp`, on two numbers less, equal and
/// greater, with NaN on either side, and with NaN on both (the same NaN,
/// and two of other signs and payloads): true as LLVM defines it, two
/// NaNs unordered as one is.
#[test]
fn fcmp_compares_by_each_of_llvm_s_predicates() {
    // Per predicate, whether it holds of (1, 2), (2, 2), (2, 1), (NaN, 1),
    // (1, NaN), (NaN, NaN), (NaN, another NaN); `-` where it is not
    // compared: `one` on NaN is the known deviation of the next test.
    let table = [
        (FCMP_FALSE, "0000000"),
        (FCMP_OEQ, "0100000"),
        (FCMP_OGT, "0010000"),
        (FCMP_OGE, "0110000"),
        (FCMP_OLT, "1000000"),
        (FCMP_OLE, "1100000"),
        (FCMP_ONE, "101----"),
        (FCMP_ORD, "1110000"),
        (FCMP_UNO, "0001111"),
        (FCMP_UEQ, "0101111"),
        (FCMP_UGT, "0011111"),
        (FCMP_UGE, "0111111"),
        (FCMP_ULT, "1001111"),
        (FCMP_ULE, "1101111"),
        (FCMP_UNE, "1011111"),
        (FCMP_TRUE, "1111111"),
    ];
    let other_nan = f64::from_bits(0xfff8_0000_0000_0456);
    let pairs = [
        (1.0, 2.0),
        (2.0, 2.0),
        (2.0, 1.0),
        (f64::NAN, 1.0),
        (1.0, f64::NAN),
        (f64::NAN, f64::NAN),
        (f64::NAN, other_nan),
    ];
    for (predicate, holds) in table {
        assert_eq!(holds.len(), pairs.len(), "predicate {predicate}");
        for ((a, b), holds) in pairs.iter().zip(holds.chars()) {
            if holds == '-' {
                continue;
            }
            assert_eq!(
                fcmp(&[predicate], *a, *b),
                [OutputRecord::Bool(holds == '1')],
                "predicate {predicate} of {a} and {b}"
            );
        }
    }
}

/// Zero and minus zero, in either order, compared by each of the 16
/// predicates of `fcmp`: equal, as LLVM and IEEE 754 define, though their
/// bits differ.
#[test]
fn fcmp_takes_zeros_of_either_sign_as_equal() {
    // Per predicate, whether it holds of two equal numbers.
    let table = [
        (FCMP_FALSE, false),
        (FCMP_OEQ, true),
        (FCMP_OGT, false),
        (FCMP_OGE, true),
        (FCMP_OLT, false),
        (FCMP_OLE, true),
        (FCMP_ONE, false),
        (FCMP_ORD, true),
        (FCMP_UNO, false),
        (FCMP_UEQ, true),
        (FCMP_UGT, false),
        (FCMP_UGE, true),
        (FCMP_ULT, false),
        (FCMP_ULE, true),
        (FCMP_UNE, false),
        (FCMP_TRUE, true),
    ];
    for (a, b) in [(0.0, -0.0), (-0.0, 0.0)] {
        for (predicate, holds) in table {
            assert_eq!(
                fcmp(&[predicate], a, b),
                [OutputRecord::Bool(holds)],
                "predicate {predicate} of {a} and {b}"
            );
        }
        // COMPATIBILITY NOTE: `run_shot` (bytecode/runtime.rs, `OP_FCMP`)
        // compares its six predicates as Rust's `==`, `!=`, `<`, `<=`, `>`
        // and `>=` do, zeros of either sign equal, hence the machine keeps
        // them, a drop-in replacement for it.
        for predicate in [FCMP_OEQ, FCMP_ONE, FCMP_OLT, FCMP_OLE, FCMP_OGT, FCMP_OGE] {
            let comparison = Instruction {
                dst: 0,
                src0: a.to_bits(),
                src1: b.to_bits(),
                opcode: compare(OP_FCMP, predicate).opcode | FLAG_SRC0_IMM | FLAG_SRC1_IMM,
                ..Instruction::default()
            };
            let (machine, run_shot) = both(&program(&[&[comparison, record_bool(0), ret()]]));
            assert_eq!(machine, run_shot, "predicate {predicate} of {a} and {b}");
        }
    }
}

/// `fcmp one` of NaN with itself, and with 1 on either side: true, as
/// `run_shot` computes it and its test `test_frem_by_zero_is_nan` expects,
/// though LLVM defines it false.
#[test]
fn fcmp_one_holds_when_either_side_is_nan() {
    for (a, b) in [(f64::NAN, f64::NAN), (f64::NAN, 1.0), (1.0, f64::NAN)] {
        let comparison = Instruction {
            dst: 0,
            src0: a.to_bits(),
            src1: b.to_bits(),
            opcode: compare(OP_FCMP, FCMP_ONE).opcode | FLAG_SRC0_IMM | FLAG_SRC1_IMM,
            ..Instruction::default()
        };
        let (machine, run_shot) = both(&program(&[&[comparison, record_bool(0), ret()]]));
        assert_eq!(machine, [OutputRecord::Bool(true)], "{a} and {b}");
        // DISCUSS COMPATIBILITY: this behavior was present in `run_shot`
        // (bytecode/runtime.rs, `FCMP_ONE => a != b`), but LLVM defines
        // `one` as ordered, false when either side is NaN, so we keep it
        // here meanwhile but we expect discussion/feedback as part of the
        // review. We think this is OK for now because Q# compiles its `!=`
        // of doubles to `one` (qsc_partial_eval), so `run_shot` gives what
        // Q# means (NaN != NaN, as Python's `test_frem_by_zero_is_nan`
        // expects), so we recommend that Q# emit `une` for `!=`, then
        // LLVM's `one` in both runtimes and that test, changed together.
        assert_eq!(machine, run_shot, "{a} and {b}");
    }
}

/// An instruction of `opcode` writing register 1 from the constant
/// `value`, with `aux0` (a width, for `SEXT`).
fn unary(opcode: u8, value: u64, aux0: u64) -> Instruction<u64> {
    Instruction {
        dst: 1,
        src0: value,
        aux0,
        ..op(opcode, FLAG_SRC0_IMM)
    }
}

/// An instruction of `opcode` writing register 1 from the constants `a`
/// and `b`.
fn constants(opcode: u8, a: u64, b: u64) -> Instruction<u64> {
    binary(opcode, FLAG_SRC0_IMM | FLAG_SRC1_IMM, 1, a, b)
}

/// An instruction of `opcode` writing register 1 from the constant reals
/// `x` and `y`.
fn reals(opcode: u8, x: f64, y: f64) -> Instruction<u64> {
    constants(opcode, x.to_bits(), y.to_bits())
}

/// The bits of `value`, as [`computed`] gives a register holding it.
fn real(value: f64) -> i64 {
    value.to_bits().cast_signed()
}

/// A `NOP` between register 1 set to 3 and its record.
#[test]
fn nop_changes_no_register() {
    assert_eq!(computed(op(OP_NOP, 0)), 3);
}

/// Register 1 set by `CONST` to 77.
#[test]
fn const_sets_its_register_to_its_value() {
    let constant = Instruction {
        dst: 1,
        src0: 77,
        ..op(OP_CONST, 0)
    };
    assert_eq!(computed(constant), 77);
}

/// 0.1, which a `float` does not hold exactly, truncated by `FPTRUNC`.
#[test]
fn fptrunc_keeps_its_double() {
    // DISCUSS COMPATIBILITY: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `OP_FPEXT | OP_FPTRUNC` copy), but LLVM rounds
    // a `double` truncated to `float` to single precision (0.1 becomes
    // 0.100000001490116...), so we keep it here meanwhile but we expect
    // discussion/feedback as part of the review. We think this is OK
    // because registers hold doubles only and Q# computes in doubles (no
    // corpus program truncates), so we recommend keeping it until a
    // producer emits `float`s.
    assert_eq!(
        computed(unary(OP_FPTRUNC, 0.1f64.to_bits(), 0)),
        0.1f64.to_bits().cast_signed()
    );
}

/// 2^32 + 5 truncated by `TRUNC`, as to an `i32`.
#[test]
fn trunc_copies_its_whole_word() {
    // DISCUSS COMPATIBILITY: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `OP_ZEXT | OP_TRUNC | OP_INTTOPTR` copy), but
    // LLVM keeps the low bits of the narrower type (5 here). It is the
    // case of an integer at 64 bits whatever its LLVM width, whose
    // discussion and recommendation apply here (see
    // `an_integer_instruction_computes_at_64_bits_whatever_its_llvm_width`).
    assert_eq!(computed(unary(OP_TRUNC, (1 << 32) + 5, 0)), (1 << 32) + 5);
}

/// 2^32 - 1, the greatest `u32`, extended by `ZEXT`, which a sign
/// extension would change; 12 cast by `INTTOPTR`; and -1.5, which a
/// `float` holds, extended by `FPEXT` to a `double`.
#[test]
fn a_widening_or_pointer_conversion_copies_its_word() {
    for (opcode, value) in [
        (OP_ZEXT, 0xffff_ffff),
        (OP_INTTOPTR, 12),
        (OP_FPEXT, (-1.5f64).to_bits()),
    ] {
        assert_eq!(
            computed(unary(opcode, value, 0)),
            value.cast_signed(),
            "{opcode:#x}"
        );
    }
}

/// 1 and 2 extended from 1 bit, 0x7f from 8, and 2^62 from 63.
#[test]
fn sext_extends_the_sign_bit_of_its_width() {
    assert_eq!(computed(unary(OP_SEXT, 1, 1)), -1);
    assert_eq!(computed(unary(OP_SEXT, 2, 1)), 0);
    assert_eq!(computed(unary(OP_SEXT, 0x7f, 8)), 0x7f);
    assert_eq!(computed(unary(OP_SEXT, 1 << 62, 63)), -(1 << 62));
}

/// 0xff extended from 0, 64 and 65 bits.
#[test]
fn sext_of_0_or_64_or_more_bits_copies() {
    // COMPATIBILITY NOTE: `run_shot` (bytecode/runtime.rs, `OP_SEXT`)
    // copies a width it cannot shift by; LLVM has no such `sext` (its
    // widths are 1 to 63 here), so the copy is kept as `run_shot` has it.
    for bits in [0, 64, 65] {
        assert_eq!(computed(unary(OP_SEXT, 0xff, bits)), 0xff, "{bits} bits");
    }
}

/// 1 extended from 2^32 + 1 bits, which is 1 bit cut to 32.
#[test]
fn sext_reads_its_width_cut_to_32_bits() {
    // DISCUSS COMPATIBILITY: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `OP_SEXT`: `instr.aux0 as u32`), but it is an
    // accident of the cast (no LLVM width exceeds 64), so we keep it here
    // meanwhile but we expect discussion/feedback as part of the review.
    // We think this is OK because no producer writes such a width (the
    // corpus's are all 1), so we recommend that building refuse a width
    // of 0 or past 64 later, in both runtimes.
    assert_eq!(computed(unary(OP_SEXT, 1, (1 << 32) + 1)), -1);
}

/// 2^64 - 1 plus 2, 2 minus 5, and 2^62 times 4.
#[test]
fn add_sub_and_mul_wrap_modulo_2_64() {
    for (opcode, a, b, wrapped) in [
        (OP_ADD, u64::MAX, 2, 1),
        (OP_SUB, 2, 5, -3),
        (OP_MUL, 1 << 62, 4, 0),
    ] {
        assert_eq!(computed(constants(opcode, a, b)), wrapped, "{opcode:#x}");
    }
}

/// 2^31 - 1, the greatest `i32`, plus 1.
#[test]
fn an_integer_instruction_computes_at_64_bits_whatever_its_llvm_width() {
    // DISCUSS COMPATIBILITY: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `OP_ADD`: `wrapping_add` on `i64`), but the
    // bytecode carries no width (the pass emits every integer instruction
    // alike), so an `add i32` gives 2^31 here, where LLVM wraps to -2^31,
    // so we keep it here meanwhile but we expect discussion/feedback as
    // part of the review. We think this is OK for now because Q# emits
    // integer arithmetic on `i64` only, and a value other than 0 and 1
    // read as an `i1` traps (see `a_condition_other_than_0_or_1_is_a_trap`),
    // so we recommend keeping it until a producer emits narrower integers,
    // then having the pass emit widths and both runtimes wrap at them.
    assert_eq!(computed(constants(OP_ADD, 0x7fff_ffff, 1)), 0x8000_0000);
}

/// 0b1100 and 0b1010, combined by `AND`, `OR` and `XOR`.
#[test]
fn a_bitwise_instruction_combines_its_operands_bit_by_bit() {
    for (opcode, combined) in [(OP_AND, 0b1000), (OP_OR, 0b1110), (OP_XOR, 0b0110)] {
        assert_eq!(
            computed(constants(opcode, 0b1100, 0b1010)),
            combined,
            "{opcode:#x}"
        );
    }
}

/// 2^64 - 1, -1 as a signed integer, divided by 2, and its remainder by
/// 10.
#[test]
fn udiv_and_urem_read_their_operands_as_unsigned() {
    for (opcode, b, computed_unsigned) in [(OP_UDIV, 2, i64::MAX), (OP_UREM, 10, 5)] {
        assert_eq!(
            computed(constants(opcode, u64::MAX, b)),
            computed_unsigned,
            "{opcode:#x}"
        );
    }
}

/// -7 divided by 2, and 7 by -2, and their remainders: the quotient is
/// rounded toward zero, so the remainder takes the sign of the dividend.
#[test]
fn sdiv_and_srem_round_toward_zero() {
    let (minus_7, minus_2) = ((-7i64).cast_unsigned(), (-2i64).cast_unsigned());
    for (opcode, a, b, rounded) in [
        (OP_SDIV, minus_7, 2, -3),
        (OP_SDIV, 7, minus_2, -3),
        (OP_SREM, minus_7, 2, -1),
        (OP_SREM, 7, minus_2, 1),
    ] {
        assert_eq!(
            computed(constants(opcode, a, b)),
            rounded,
            "{opcode:#x} of {a} and {b}"
        );
    }
}

/// The least integer, divided by -1: the quotient would be 2^63.
#[test]
fn signed_division_of_the_least_integer_by_minus_1_wraps() {
    // COMPATIBILITY NOTE: LLVM leaves this division undefined; `run_shot`
    // (bytecode/runtime.rs, `wrapping_div`, `wrapping_rem`) wraps to the
    // least integer, remainder 0, and the machine keeps that value.
    let least = i64::MIN.cast_unsigned();
    assert_eq!(computed(constants(OP_SDIV, least, u64::MAX)), i64::MIN);
    assert_eq!(computed(constants(OP_SREM, least, u64::MAX)), 0);
}

/// 2^63 + 1 shifted right by 1.
#[test]
fn lshr_shifts_in_zeros() {
    assert_eq!(
        computed(constants(OP_LSHR, 0x8000_0000_0000_0001, 1)),
        1 << 62
    );
}

/// 2^63 + 1 shifted right by 63.
#[test]
fn ashr_shifts_in_the_sign_bit() {
    assert_eq!(computed(constants(OP_ASHR, 0x8000_0000_0000_0001, 63)), -1);
}

/// 2^63 + 1 shifted left by 65, right by 64, and right arithmetically by
/// 2^32 + 1.
#[test]
fn a_shift_shifts_by_its_count_mod_64() {
    // COMPATIBILITY NOTE: LLVM leaves a shift by 64 or more poison;
    // `run_shot` (bytecode/runtime.rs, `wrapping_shl`, `wrapping_shr` of
    // the count cut to 32 bits) shifts by the count mod 64, and the
    // machine keeps that value.
    let word = 0x8000_0000_0000_0001;
    assert_eq!(computed(constants(OP_SHL, word, 65)), 2);
    assert_eq!(computed(constants(OP_LSHR, word, 64)), i64::MIN + 1);
    assert_eq!(
        computed(constants(OP_ASHR, word, (1 << 32) + 1)),
        i64::MIN / 2
    );
}

/// 1.5 and 0.25, added, the second subtracted from the first, multiplied,
/// and the first divided by the second.
#[test]
fn a_float_arithmetic_instruction_computes_on_its_operands_as_doubles() {
    for (opcode, computed_double) in [
        (OP_FADD, 1.75),
        (OP_FSUB, 1.25),
        (OP_FMUL, 0.375),
        (OP_FDIV, 6.0),
    ] {
        assert_eq!(
            computed(reals(opcode, 1.5, 0.25)),
            real(computed_double),
            "{opcode:#x}"
        );
    }
}

/// 1 and -1 divided by zero and by minus zero: an infinity, its sign the
/// product of their signs; and their remainders by both zeros: NaN; as
/// IEEE 754 defines, with no trap, unlike an integer division.
#[test]
fn a_float_division_or_remainder_by_a_zero_gives_its_ieee_754_value() {
    let quotients = [
        (1.0, 0.0, f64::INFINITY),
        (1.0, -0.0, f64::NEG_INFINITY),
        (-1.0, 0.0, f64::NEG_INFINITY),
        (-1.0, -0.0, f64::INFINITY),
    ];
    for (x, zero, infinity) in quotients {
        assert_eq!(
            computed(reals(OP_FDIV, x, zero)),
            real(infinity),
            "{x} / {zero}"
        );
        let remainder = f64::from_bits(computed(reals(OP_FREM, x, zero)).cast_unsigned());
        assert!(remainder.is_nan(), "{x} % {zero} is {remainder}");
    }
}

/// The remainder of -7.5 by 2.
#[test]
fn frem_takes_the_sign_of_the_dividend() {
    assert_eq!(computed(reals(OP_FREM, -7.5, 2.0)), real(-1.5));
}

/// -2.5 converted to an integer.
#[test]
fn fptosi_rounds_toward_zero() {
    assert_eq!(computed(unary(OP_FPTOSI, (-2.5f64).to_bits(), 0)), -2);
}

/// 1e300 and +inf, past the greatest integer, -1e300 and -inf, past the
/// least, and NaN, converted by `FPTOSI`; and -2.5 and -inf, below 0,
/// 1e300 and +inf, past the greatest unsigned integer, and NaN, converted
/// by `FPTOUI`.
#[test]
fn a_float_to_integer_conversion_saturates_out_of_range_and_takes_nan_to_0() {
    // COMPATIBILITY NOTE: LLVM leaves an `fptosi` or an `fptoui` out of
    // range or of NaN poison; `run_shot` (bytecode/runtime.rs, `OP_FPTOSI`
    // and `OP_FPTOUI`: an `as i64` and an `as u64` cast) saturates, and
    // gives 0 for NaN, and the machine keeps those values.
    for (opcode, value, converted) in [
        (OP_FPTOSI, 1e300, i64::MAX),
        (OP_FPTOSI, f64::INFINITY, i64::MAX),
        (OP_FPTOSI, -1e300, i64::MIN),
        (OP_FPTOSI, f64::NEG_INFINITY, i64::MIN),
        (OP_FPTOSI, f64::NAN, 0),
        (OP_FPTOUI, -2.5, 0),
        (OP_FPTOUI, f64::NEG_INFINITY, 0),
        (OP_FPTOUI, 1e300, u64::MAX.cast_signed()),
        (OP_FPTOUI, f64::INFINITY, u64::MAX.cast_signed()),
        (OP_FPTOUI, f64::NAN, 0),
    ] {
        assert_eq!(
            computed(unary(opcode, value.to_bits(), 0)),
            converted,
            "{opcode:#x} of {value}"
        );
    }
}

/// 2^63, past the greatest signed integer, converted by `FPTOUI`; -2
/// converted by `SITOFP`; and 2^63 converted by `UITOFP`.
#[test]
fn a_conversion_takes_its_integer_as_signed_or_unsigned_by_its_opcode() {
    for (opcode, value, converted) in [
        (OP_FPTOUI, 2f64.powi(63).to_bits(), i64::MIN),
        (OP_SITOFP, (-2i64).cast_unsigned(), real(-2.0)),
        (OP_UITOFP, 1 << 63, real(2f64.powi(63))),
    ] {
        assert_eq!(computed(unary(opcode, value, 0)), converted, "{opcode:#x}");
    }
}

/// Base 100 offset by index 3 of size 8, and by index -1.
#[test]
fn gep_offsets_its_base_by_index_times_size_wrapping() {
    let gep = |index| Instruction {
        dst: 1,
        src0: 100,
        src1: index,
        aux0: 8,
        ..op(OP_GEP, FLAG_SRC0_IMM | FLAG_SRC1_IMM | FLAG_AUX0_IMM)
    };
    assert_eq!(computed(gep(3)), 124);
    assert_eq!(computed(gep(u64::MAX)), 92);
}

/// `instruction` with its operand `name` (`src0`, `src1` or `aux0`) read
/// instead from register 2: its flag cleared and the register in its
/// place; and the value that operand held.
fn reading_register_2(mut instruction: Instruction<u64>, name: &str) -> (Instruction<u64>, u64) {
    let (flag, operand) = match name {
        "src0" => (FLAG_SRC0_IMM, &mut instruction.src0),
        "src1" => (FLAG_SRC1_IMM, &mut instruction.src1),
        "aux0" => (FLAG_AUX0_IMM, &mut instruction.aux0),
        _ => unreachable!("no instruction here reads {name} by a flag"),
    };
    let value = std::mem::replace(operand, 2);
    instruction.opcode &= !flag;
    (instruction, value)
}

/// Each classical instruction of constants into register 1, then with
/// each operand it reads by a flag read in turn from register 2, set to
/// that operand's value: each gives what it gives of the constants. Every
/// operand differs from 2, and gives another value read as the constant
/// 2, so a flag misread, or another operand's, shows. (A `SEXT`'s width is
/// a constant whatever its flags.)
#[test]
fn each_operand_is_read_as_a_constant_or_a_register_by_its_own_flag() {
    const BOTH: &[&str] = &["src0", "src1"];
    let of = |instruction: Instruction<u64>, a, b, aux0| Instruction {
        dst: 1,
        src0: a,
        src1: b,
        aux0,
        opcode: instruction.opcode | FLAG_SRC0_IMM | FLAG_SRC1_IMM | FLAG_AUX0_IMM,
        ..instruction
    };
    let comparisons = [compare(OP_ICMP, ICMP_EQ), compare(OP_FCMP, FCMP_OEQ)]
        .map(|comparison| (of(comparison, 3, 3, 0), BOTH));
    let integers = [
        OP_ADD, OP_SUB, OP_MUL, OP_UDIV, OP_SDIV, OP_UREM, OP_SREM, OP_AND, OP_OR, OP_XOR,
    ]
    .map(|opcode| (of(op(opcode, 0), 9, 5, 0), BOTH));
    let shifts = [OP_SHL, OP_LSHR, OP_ASHR].map(|opcode| (of(op(opcode, 0), 12, 1, 0), BOTH));
    let doubles = [OP_FADD, OP_FSUB, OP_FMUL, OP_FDIV].map(|opcode| {
        let (a, b) = (1.5f64.to_bits(), 0.25f64.to_bits());
        (of(op(opcode, 0), a, b, 0), BOTH)
    });
    let frem = of(op(OP_FREM, 0), (-7.5f64).to_bits(), 2f64.to_bits(), 0);
    let copies = [OP_ZEXT, OP_TRUNC, OP_INTTOPTR, OP_MOV, OP_FPEXT, OP_FPTRUNC]
        .map(|opcode| (of(op(opcode, 0), 3, 0, 0), &["src0"][..]));
    let conversions = [
        (OP_FPTOSI, (-2.5f64).to_bits()),
        (OP_FPTOUI, 2.5f64.to_bits()),
        (OP_SITOFP, 3),
        (OP_UITOFP, 3),
    ]
    .map(|(opcode, value)| (of(op(opcode, 0), value, 0, 0), &["src0"][..]));
    let gep = of(op(OP_GEP, 0), 5, 3, 4);
    let rows = comparisons
        .into_iter()
        .chain(integers)
        .chain(shifts)
        .chain(doubles)
        .chain([(frem, BOTH)])
        .chain(copies)
        .chain([(of(op(OP_SEXT, 0), 3, 0, 2), &["src0"][..])])
        .chain(conversions)
        .chain([(gep, &["src0", "src1", "aux0"][..])]);
    for (constants, operands) in rows {
        let expected = computed(constants);
        for name in operands {
            let (from_register, value) = reading_register_2(constants, name);
            assert_eq!(
                computed_by(&[set(2, value), from_register]),
                expected,
                "{:#x}, {name} from a register",
                constants.opcode
            );
        }
    }
}
