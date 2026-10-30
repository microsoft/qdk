use super::{MAX_STEPS, OP_JUMP, OP_RET, run_shot};
use crate::{
    Simulator,
    bytecode::{AdaptiveProgram, Block, Instruction},
    noise_config::CumulativeNoiseConfig,
    stabilizer_simulator::StabilizerSimulator,
};
use std::sync::Arc;

fn single_block_program(instructions: Vec<Instruction<u64>>) -> AdaptiveProgram<u64> {
    AdaptiveProgram {
        num_qubits: 1,
        num_results: 1,
        num_registers: 1,
        entry_block: 0,
        block_table: vec![Block {
            instr_offset: 0,
            instr_count: instructions.len() as u64,
        }],
        instructions,
        function_table: Vec::new(),
        phi_entries: Vec::new(),
        switch_cases: Vec::new(),
        call_args: Vec::new(),
        constant_data: Vec::new(),
        quantum_ops: Vec::new(),
    }
}

fn new_simulator() -> StabilizerSimulator {
    StabilizerSimulator::new(1, 1, 0, Arc::new(CumulativeNoiseConfig::default()))
}

#[test]
fn terminating_program_returns_records() {
    let program = single_block_program(vec![Instruction {
        opcode: u64::from(OP_RET) | super::FLAG_DST_IMM,
        ..Default::default()
    }]);
    let records = run_shot(&program, &mut new_simulator()).expect("program should terminate");
    assert!(records.is_empty());
}

#[test]
fn non_terminating_program_errors_after_max_steps() {
    let program = single_block_program(vec![Instruction {
        opcode: u64::from(OP_JUMP),
        dst: 0,
        ..Default::default()
    }]);
    let err = run_shot(&program, &mut new_simulator()).expect_err("program should not terminate");
    assert_eq!(
        err,
        format!("program did not terminate within {MAX_STEPS} bytecode steps")
    );
}
