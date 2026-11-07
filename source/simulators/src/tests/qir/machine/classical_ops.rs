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

/// A comparison of `opcode` (`ICMP` or `FCMP`) by `predicate` of `a` and
/// `b`, constants or registers by `flags`, into register 1.
fn compared(opcode: u8, predicate: u8, flags: u64, a: u64, b: u64) -> Instruction<u64> {
    Instruction {
        dst: 1,
        src0: a,
        src1: b,
        opcode: compare(opcode, predicate).opcode | flags,
        ..Instruction::default()
    }
}

/// `ICMP EQ`s of register 1, which holds 3, and the constant 3, and of the
/// constant 3 and register 1.
#[test]
fn icmp_reads_each_operand_as_a_constant_or_a_register_by_its_own_flag() {
    assert_eq!(computed(compared(OP_ICMP, ICMP_EQ, FLAG_SRC1_IMM, 1, 3)), 1);
    assert_eq!(computed(compared(OP_ICMP, ICMP_EQ, FLAG_SRC0_IMM, 3, 1)), 1);
}

/// `FCMP OEQ`s of register 1, which holds 3, and the constant 3 (the same
/// bits, read as a double), and of the constant 3 and register 1.
#[test]
fn fcmp_reads_each_operand_as_a_constant_or_a_register_by_its_own_flag() {
    assert_eq!(
        computed(compared(OP_FCMP, FCMP_OEQ, FLAG_SRC1_IMM, 1, 3)),
        1
    );
    assert_eq!(
        computed(compared(OP_FCMP, FCMP_OEQ, FLAG_SRC0_IMM, 3, 1)),
        1
    );
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

/// The values [`computed`] gives an instruction of `opcode`, with `aux0`,
/// writing register 1 from the constant `value`, then from register 2 set
/// to `value`: equal when the instruction reads its value by its flag.
fn by_flag(opcode: u8, value: u64, aux0: u64) -> [i64; 2] {
    let from_register = Instruction {
        dst: 1,
        src0: 2,
        aux0,
        ..op(opcode, 0)
    };
    [
        computed(unary(opcode, value, aux0)),
        computed_by(&[set(2, value), from_register]),
    ]
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

/// -1.5, which a `float` holds, extended by `FPEXT` to a `double`.
#[test]
fn fpext_keeps_its_double() {
    assert_eq!(
        computed(unary(OP_FPEXT, (-1.5f64).to_bits(), 0)),
        real(-1.5)
    );
}

/// Register 1 set by `INTTOPTR` from 12.
#[test]
fn inttoptr_copies_its_integer() {
    assert_eq!(computed(unary(OP_INTTOPTR, 12, 0)), 12);
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

/// 2^32 - 1, the greatest `u32`, extended by `ZEXT`.
#[test]
fn zext_copies_its_word() {
    assert_eq!(computed(unary(OP_ZEXT, 0xffff_ffff, 0)), 0xffff_ffff);
}

/// 3 copied by each copy (`ZEXT`, `TRUNC`, `INTTOPTR`, `MOV`, `FPEXT`,
/// `FPTRUNC`), from a constant and from a register.
#[test]
fn a_copy_reads_its_value_as_a_constant_or_a_register_by_its_flag() {
    for opcode in [OP_ZEXT, OP_TRUNC, OP_INTTOPTR, OP_MOV, OP_FPEXT, OP_FPTRUNC] {
        assert_eq!(by_flag(opcode, 3, 0), [3, 3], "{opcode:#x}");
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

/// 3 (0b11) extended from 2 bits, from a constant and from a register.
#[test]
fn sext_reads_its_value_as_a_constant_or_a_register_by_its_flag() {
    assert_eq!(by_flag(OP_SEXT, 3, 2), [-1, -1]);
}

/// 2^64 - 1 plus 2.
#[test]
fn add_wraps_past_2_64() {
    assert_eq!(computed(constants(OP_ADD, u64::MAX, 2)), 1);
}

/// 2 minus 5.
#[test]
fn sub_wraps_below_0() {
    assert_eq!(computed(constants(OP_SUB, 2, 5)), -3);
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

/// 2^62 times 4.
#[test]
fn mul_wraps_past_2_64() {
    assert_eq!(computed(constants(OP_MUL, 1 << 62, 4)), 0);
}

/// 0b1100 and 0b1010.
#[test]
fn and_keeps_the_bits_both_operands_set() {
    assert_eq!(computed(constants(OP_AND, 0b1100, 0b1010)), 0b1000);
}

/// 0b1100 or 0b1010.
#[test]
fn or_sets_the_bits_either_operand_sets() {
    assert_eq!(computed(constants(OP_OR, 0b1100, 0b1010)), 0b1110);
}

/// 0b1100 xor 0b1010.
#[test]
fn xor_sets_the_bits_exactly_one_operand_sets() {
    assert_eq!(computed(constants(OP_XOR, 0b1100, 0b1010)), 0b0110);
}

/// 2^64 - 1, -1 as a signed integer, divided by 2.
#[test]
fn udiv_divides_as_unsigned() {
    assert_eq!(computed(constants(OP_UDIV, u64::MAX, 2)), i64::MAX);
}

/// 2^64 - 1, -1 as a signed integer, divided by 10.
#[test]
fn urem_takes_the_unsigned_remainder() {
    assert_eq!(computed(constants(OP_UREM, u64::MAX, 10)), 5);
}

/// -7 divided by 2, and 7 by -2.
#[test]
fn sdiv_rounds_toward_zero() {
    let (minus_7, minus_2) = ((-7i64).cast_unsigned(), (-2i64).cast_unsigned());
    assert_eq!(computed(constants(OP_SDIV, minus_7, 2)), -3);
    assert_eq!(computed(constants(OP_SDIV, 7, minus_2)), -3);
}

/// The remainders of -7 by 2, and of 7 by -2.
#[test]
fn srem_takes_the_sign_of_the_dividend() {
    let (minus_7, minus_2) = ((-7i64).cast_unsigned(), (-2i64).cast_unsigned());
    assert_eq!(computed(constants(OP_SREM, minus_7, 2)), -1);
    assert_eq!(computed(constants(OP_SREM, 7, minus_2)), 1);
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

/// 1.5 plus 0.25.
#[test]
fn fadd_adds_its_operands() {
    assert_eq!(computed(reals(OP_FADD, 1.5, 0.25)), real(1.75));
}

/// 1.5 minus 0.25.
#[test]
fn fsub_subtracts_its_second_operand_from_its_first() {
    assert_eq!(computed(reals(OP_FSUB, 1.5, 0.25)), real(1.25));
}

/// 1.5 times 0.25.
#[test]
fn fmul_multiplies_its_operands() {
    assert_eq!(computed(reals(OP_FMUL, 1.5, 0.25)), real(0.375));
}

/// 1.5 divided by 0.25.
#[test]
fn fdiv_divides_its_first_operand_by_its_second() {
    assert_eq!(computed(reals(OP_FDIV, 1.5, 0.25)), real(6.0));
}

/// 1 and -1 divided by zero and by minus zero: an infinity, its sign the
/// product of their signs, as IEEE 754 defines.
#[test]
fn fdiv_by_a_zero_gives_an_infinity_signed_by_both_operands() {
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
    }
}

/// The remainder of -7.5 by 2.
#[test]
fn frem_takes_the_sign_of_the_dividend() {
    assert_eq!(computed(reals(OP_FREM, -7.5, 2.0)), real(-1.5));
}

/// Each arithmetic instruction of `a` and `b`, with `a` read instead from
/// register 2, set to `a`, then with `b` read from it: each gives what it
/// gives of the constants. (The operands are such that reading the
/// constant 2 for either gives another value.)
#[test]
fn an_arithmetic_instruction_reads_each_operand_as_a_constant_or_a_register_by_its_own_flag() {
    let integers = [
        OP_ADD, OP_SUB, OP_MUL, OP_UDIV, OP_SDIV, OP_UREM, OP_SREM, OP_AND, OP_OR, OP_XOR,
    ]
    .map(|opcode| (opcode, 9, 5));
    let shifts = [OP_SHL, OP_LSHR, OP_ASHR].map(|opcode| (opcode, 12, 1));
    let doubles = [OP_FADD, OP_FSUB, OP_FMUL, OP_FDIV]
        .map(|opcode| (opcode, 1.5f64.to_bits(), 0.25f64.to_bits()));
    let frem = (OP_FREM, (-7.5f64).to_bits(), 2f64.to_bits());
    for (opcode, a, b) in integers
        .into_iter()
        .chain(shifts)
        .chain(doubles)
        .chain([frem])
    {
        let expected = computed(constants(opcode, a, b));
        let from_a = computed_by(&[set(2, a), binary(opcode, FLAG_SRC1_IMM, 1, 2, b)]);
        let from_b = computed_by(&[set(2, b), binary(opcode, FLAG_SRC0_IMM, 1, a, 2)]);
        assert_eq!([from_a, from_b], [expected; 2], "{opcode:#x}");
    }
}

/// -2.5 converted to an integer.
#[test]
fn fptosi_rounds_toward_zero() {
    assert_eq!(computed(unary(OP_FPTOSI, (-2.5f64).to_bits(), 0)), -2);
}

/// 1e300 and +inf, past the greatest integer, and -1e300 and -inf, past
/// the least, converted to integers.
#[test]
fn fptosi_saturates_out_of_range() {
    // COMPATIBILITY NOTE: LLVM leaves an `fptosi` out of range poison;
    // `run_shot` (bytecode/runtime.rs, `OP_FPTOSI`: an `as i64` cast)
    // saturates, and the machine keeps that value.
    let fptosi = |value: f64| computed(unary(OP_FPTOSI, value.to_bits(), 0));
    assert_eq!(fptosi(1e300), i64::MAX);
    assert_eq!(fptosi(f64::INFINITY), i64::MAX);
    assert_eq!(fptosi(-1e300), i64::MIN);
    assert_eq!(fptosi(f64::NEG_INFINITY), i64::MIN);
}

/// NaN converted to an integer.
#[test]
fn fptosi_converts_nan_to_0() {
    // COMPATIBILITY NOTE: LLVM leaves an `fptosi` of NaN poison;
    // `run_shot` (bytecode/runtime.rs, `OP_FPTOSI`: an `as i64` cast)
    // gives 0, and the machine keeps that value.
    assert_eq!(computed(unary(OP_FPTOSI, f64::NAN.to_bits(), 0)), 0);
}

/// 2^63, past the greatest signed integer, converted to an integer.
#[test]
fn fptoui_converts_as_unsigned() {
    assert_eq!(
        computed(unary(OP_FPTOUI, 2f64.powi(63).to_bits(), 0)),
        i64::MIN
    );
}

/// -2.5 and -inf, below 0, and 1e300 and +inf, past the greatest unsigned
/// integer, converted to unsigned integers.
#[test]
fn fptoui_saturates_out_of_range() {
    // COMPATIBILITY NOTE: LLVM leaves an `fptoui` out of range poison;
    // `run_shot` (bytecode/runtime.rs, `OP_FPTOUI`: an `as u64` cast)
    // saturates, and the machine keeps that value.
    let fptoui = |value: f64| computed(unary(OP_FPTOUI, value.to_bits(), 0));
    assert_eq!(fptoui(-2.5), 0);
    assert_eq!(fptoui(f64::NEG_INFINITY), 0);
    assert_eq!(fptoui(1e300), u64::MAX.cast_signed());
    assert_eq!(fptoui(f64::INFINITY), u64::MAX.cast_signed());
}

/// NaN converted to an unsigned integer.
#[test]
fn fptoui_converts_nan_to_0() {
    // COMPATIBILITY NOTE: LLVM leaves an `fptoui` of NaN poison;
    // `run_shot` (bytecode/runtime.rs, `OP_FPTOUI`: an `as u64` cast)
    // gives 0, and the machine keeps that value.
    assert_eq!(computed(unary(OP_FPTOUI, f64::NAN.to_bits(), 0)), 0);
}

/// -2 converted to a real.
#[test]
fn sitofp_converts_as_signed() {
    assert_eq!(
        computed(unary(OP_SITOFP, (-2i64).cast_unsigned(), 0)),
        real(-2.0)
    );
}

/// 2^63 converted to a real.
#[test]
fn uitofp_converts_as_unsigned() {
    assert_eq!(computed(unary(OP_UITOFP, 1 << 63, 0)), real(2f64.powi(63)));
}

/// -2.5 converted by `FPTOSI`, 2.5 by `FPTOUI`, and 3 by `SITOFP` and by
/// `UITOFP`, from a constant and from a register.
#[test]
fn a_conversion_reads_its_value_as_a_constant_or_a_register_by_its_flag() {
    for (opcode, value, converted) in [
        (OP_FPTOSI, (-2.5f64).to_bits(), -2),
        (OP_FPTOUI, 2.5f64.to_bits(), 2),
        (OP_SITOFP, 3, real(3.0)),
        (OP_UITOFP, 3, real(3.0)),
    ] {
        assert_eq!(by_flag(opcode, value, 0), [converted; 2], "{opcode:#x}");
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

/// `GEP`s of base 2, index 2 and size 2, each operand in turn read
/// instead from register 1, which holds 3.
#[test]
fn gep_reads_each_operand_as_a_constant_or_a_register_by_its_own_flag() {
    let gep = |flags, base, index, size| Instruction {
        dst: 1,
        src0: base,
        src1: index,
        aux0: size,
        ..op(OP_GEP, flags)
    };
    assert_eq!(
        computed(gep(FLAG_SRC1_IMM | FLAG_AUX0_IMM, 1, 2, 2)),
        3 + 2 * 2
    );
    assert_eq!(
        computed(gep(FLAG_SRC0_IMM | FLAG_AUX0_IMM, 2, 1, 2)),
        2 + 3 * 2
    );
    assert_eq!(
        computed(gep(FLAG_SRC0_IMM | FLAG_SRC1_IMM, 2, 2, 1)),
        2 + 2 * 3
    );
}
