// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A context's memory: `alloca`s, loads and stores, what a function's
//! return frees, and the traps past the memory's end.

use super::super::{binary, program, record_int, ret, set};
use super::{both, records, trap};
use crate::OutputRecord;
use crate::bytecode::Instruction;
use crate::bytecode::encoding::{FLAG_SRC0_IMM, FLAG_SRC1_IMM, OP_ALLOCA, OP_LOAD, OP_STORE};
use crate::qir::{QirReadout, QirTrap};

/// Loads from, and stores to, the last word of three of constant data,
/// each loaded back, and past it: at address 3, and at 2^32 + 2, which is
/// the last word's address cut to 32 bits.
#[test]
fn a_load_or_store_past_the_context_s_memory_is_a_trap() {
    let on_data = |instructions: &[Instruction<u64>]| {
        let mut bytecode = program(&[&[instructions, &[record_int(1), ret()]].concat()]);
        bytecode.constant_data = vec![7, 8, 9];
        bytecode
    };
    let load = |address| on_data(&[set(0, address), binary(OP_LOAD, 0, 1, 0, 0)]);
    let store = |address| {
        on_data(&[
            set(0, address),
            binary(OP_STORE, FLAG_SRC0_IMM, 0, 42, 0),
            binary(OP_LOAD, FLAG_SRC0_IMM, 1, 2, 0),
        ])
    };
    assert_eq!(
        records(&load(2), |_| QirReadout::One),
        [OutputRecord::Int(9)]
    );
    assert_eq!(
        records(&store(2), |_| QirReadout::One),
        [OutputRecord::Int(42)]
    );
    for address in [3, (1 << 32) + 2] {
        assert_eq!(
            trap(&load(address)),
            QirTrap::LoadPastMemory {
                instruction: 1,
                address,
                words: 3
            },
            "the load at {address}"
        );
        // COMPATIBILITY NOTE: `run_shot` (bytecode/runtime.rs,
        // `write_memory`) grows its memory up to such an address; the
        // machine traps instead, since a store outside every allocation is
        // a producer's error, and a huge address would allocate without
        // bound.
        assert_eq!(
            trap(&store(address)),
            QirTrap::StorePastMemory {
                instruction: 1,
                address,
                words: 3
            },
            "the store at {address}"
        );
    }
}

/// Three words of constant data, then two `ALLOCA`s, of two words and of
/// one, each naming address 9 in its second operand: they give addresses
/// 3 and 5, each the first past the memory it finds.
#[test]
fn an_alloca_gives_the_address_past_the_memory() {
    let mut bytecode = program(&[&[
        binary(OP_ALLOCA, FLAG_SRC0_IMM | FLAG_SRC1_IMM, 1, 2, 9),
        binary(OP_ALLOCA, FLAG_SRC0_IMM | FLAG_SRC1_IMM, 2, 1, 9),
        record_int(1),
        record_int(2),
        ret(),
    ]]);
    bytecode.constant_data = vec![7, 8, 9];
    let (machine, run_shot) = both(&bytecode);
    assert_eq!(machine, [OutputRecord::Int(3), OutputRecord::Int(5)]);
    // COMPATIBILITY NOTE: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `OP_ALLOCA` allocates at `memory.len()`, and
    // does not read the address the adaptive pass still writes in `src1`),
    // hence we keep it so that the machine is a drop-in replacement; each
    // run of an `alloca` gets fresh words, as in LLVM.
    assert_eq!(machine, run_shot);
}

/// Three words of constant data, then two `ALLOCA`s of no words, as
/// LLVM's `alloca [0 x i64]` gives: both give address 3, since the first
/// takes no word.
#[test]
fn two_allocas_of_no_words_give_the_same_address() {
    let mut bytecode = program(&[&[
        binary(OP_ALLOCA, FLAG_SRC0_IMM | FLAG_SRC1_IMM, 1, 0, 0),
        binary(OP_ALLOCA, FLAG_SRC0_IMM | FLAG_SRC1_IMM, 2, 0, 0),
        record_int(1),
        record_int(2),
        ret(),
    ]]);
    bytecode.constant_data = vec![7, 8, 9];
    let (machine, run_shot) = both(&bytecode);
    assert_eq!(machine, [OutputRecord::Int(3), OutputRecord::Int(3)]);
    // COMPATIBILITY NOTE: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `OP_ALLOCA` grows the memory by its word
    // count, none here), hence we keep it so that the machine is a
    // drop-in replacement; LLVM lets two allocations of no bytes share an
    // address.
    assert_eq!(machine, run_shot);
}

/// Two words allocated past three of constant data, at address 3, the
/// second loaded before any store.
#[test]
fn an_alloca_s_words_read_as_zero() {
    let mut bytecode = program(&[&[
        binary(OP_ALLOCA, FLAG_SRC0_IMM | FLAG_SRC1_IMM, 1, 2, 3),
        binary(OP_LOAD, FLAG_SRC0_IMM, 2, 4, 0),
        record_int(2),
        ret(),
    ]]);
    bytecode.constant_data = vec![7, 8, 9];
    // COMPATIBILITY NOTE: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `OP_ALLOCA` resizes with zeros), hence we keep
    // it so that the machine is a drop-in replacement; LLVM leaves the
    // words undefined, which zero refines.
    assert_eq!(
        records(&bytecode, |_| QirReadout::One),
        [OutputRecord::Int(0)]
    );
}

/// Two words allocated past three of constant data, at addresses 3 and
/// 4, then a store to the last of them, loaded back, and a load and a
/// store of the word after it.
#[test]
fn an_alloca_grows_the_memory_to_its_last_word() {
    let allocating_then = |instruction| {
        let mut bytecode = program(&[&[
            binary(OP_ALLOCA, FLAG_SRC0_IMM | FLAG_SRC1_IMM, 1, 2, 3),
            instruction,
            binary(OP_LOAD, FLAG_SRC0_IMM, 2, 4, 0),
            record_int(2),
            ret(),
        ]]);
        bytecode.constant_data = vec![7, 8, 9];
        bytecode
    };
    let store = |address| binary(OP_STORE, FLAG_SRC0_IMM | FLAG_SRC1_IMM, 0, 42, address);
    assert_eq!(
        records(&allocating_then(store(4)), |_| QirReadout::One),
        [OutputRecord::Int(42)]
    );
    assert_eq!(
        trap(&allocating_then(binary(OP_LOAD, FLAG_SRC0_IMM, 2, 5, 0))),
        QirTrap::LoadPastMemory {
            instruction: 1,
            address: 5,
            words: 5
        }
    );
    assert_eq!(
        trap(&allocating_then(store(5))),
        QirTrap::StorePastMemory {
            instruction: 1,
            address: 5,
            words: 5
        }
    );
}

/// One word of constant data, then an `ALLOCA` of 2^64 - 1 words: the
/// memory must grow past every address, which no allocation can hold.
#[test]
#[should_panic]
#[allow(
    clippy::should_panic_without_expect,
    reason = "the panic is the allocator's, whose text this crate does not promise"
)]
fn an_alloca_past_the_last_address_panics() {
    let alloca = binary(OP_ALLOCA, FLAG_SRC0_IMM | FLAG_SRC1_IMM, 1, u64::MAX, 0);
    let mut bytecode = program(&[&[alloca, ret()]]);
    bytecode.constant_data = vec![7];
    records(&bytecode, |_| QirReadout::One);
}

/// One word allocated at address 0, where the word 2^32 + 2, which needs
/// more than 32 bits, is stored and loaded back, then recorded.
#[test]
fn a_load_reads_back_the_whole_word_a_store_wrote() {
    let bytecode = program(&[&[
        binary(OP_ALLOCA, FLAG_SRC0_IMM | FLAG_SRC1_IMM, 2, 1, 0),
        binary(OP_STORE, FLAG_SRC0_IMM | FLAG_SRC1_IMM, 0, 0x1_0000_0002, 0),
        binary(OP_LOAD, FLAG_SRC0_IMM, 1, 0, 0),
        record_int(1),
        ret(),
    ]]);
    let (machine, run_shot) = both(&bytecode);
    assert_eq!(machine, [OutputRecord::Int(0x1_0000_0002)]);
    // COMPATIBILITY NOTE: this behavior was present in `run_shot`
    // (bytecode/runtime.rs, `OP_STORE`: `write_memory` stores a `u64`),
    // hence we keep it so that the machine is a drop-in replacement.
    assert_eq!(machine, run_shot);
}

/// Each operand of a memory instruction, read once as a constant and once
/// from a register, which holds another value: an `ALLOCA`'s word count,
/// 2 or the 3 register 2 holds, shown by the address of a following
/// `ALLOCA` of none; and a store's value, register 2 holding 7, at the
/// constant address 1, and its address, the constant 9 at the address 1
/// register 2 holds, each loaded back from address 1.
#[test]
fn each_memory_operand_is_read_as_a_constant_or_a_register_by_its_own_flag() {
    let next_address = |flags| {
        program(&[&[
            set(2, 3),
            binary(OP_ALLOCA, flags | FLAG_SRC1_IMM, 0, 2, 0),
            binary(OP_ALLOCA, FLAG_SRC0_IMM | FLAG_SRC1_IMM, 1, 0, 0),
            record_int(1),
            ret(),
        ]])
    };
    let stored = |register_2, store| {
        program(&[&[
            binary(OP_ALLOCA, FLAG_SRC0_IMM | FLAG_SRC1_IMM, 3, 2, 0),
            set(2, register_2),
            store,
            binary(OP_LOAD, FLAG_SRC0_IMM, 1, 1, 0),
            record_int(1),
            ret(),
        ]])
    };
    let rows = [
        (
            "alloca's word count, a constant",
            next_address(FLAG_SRC0_IMM),
            2,
        ),
        ("alloca's word count, from a register", next_address(0), 3),
        (
            "store's value, from a register",
            stored(7, binary(OP_STORE, FLAG_SRC1_IMM, 0, 2, 1)),
            7,
        ),
        (
            "store's address, from a register",
            stored(1, binary(OP_STORE, FLAG_SRC0_IMM, 0, 9, 2)),
            9,
        ),
    ];
    for (operand, bytecode, recorded) in rows {
        let (machine, run_shot) = both(&bytecode);
        assert_eq!(machine, [OutputRecord::Int(recorded)], "{operand}");
        // COMPATIBILITY NOTE: this behavior was present in `run_shot`
        // (bytecode/runtime.rs, `OP_ALLOCA` and `OP_STORE`: `resolve_u64`
        // of each operand by its own flag), hence we keep it so that the
        // machine is a drop-in replacement.
        assert_eq!(machine, run_shot, "{operand}: the records of `run_shot`");
    }
}
