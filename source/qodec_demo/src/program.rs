//! A program of top calls, written in the Qodec REPL's syntax and parsed
//! with the lowerer's `syntax` module, as the protocol's `StaticProgram`:
//! what the chains run, and what the Qodec runtime's own stack runs
//! (`stack.rs`).

use std::sync::Arc;

use executor_protocol::{InstructionCall, Needs, StaticProgram};
use qodec::Qodec;

use crate::boundary::{Boundary, opcode};

/// Top batches of calls of a qodec's top set, each readout under a tag
/// unique in the program.
#[derive(Clone, Debug)]
pub struct Program {
    /// The top set, and its conversions.
    pub top: Arc<Boundary>,
    pub batches: Vec<Vec<InstructionCall>>,
}

impl Program {
    /// One batch per string, each `;`-separated statements of the REPL, of
    /// `qodec`'s top set; its readouts tagged from 0 in program order.
    ///
    /// # Panics
    ///
    /// When the protocol's operands cannot express the top set, or a
    /// statement is not one of it.
    #[must_use]
    pub fn parse(qodec: &Qodec, batches: &[&str]) -> Self {
        let isa = &qodec.layers()[0].instruction_set;
        let top = Arc::new(Boundary::new(isa).expect("the operands express the top set"));
        let mut tag = 0;
        let batches = batches
            .iter()
            .map(|text| {
                let statements = crate::syntax::statements(text).expect("a program");
                statements
                    .into_iter()
                    .map(|s| {
                        let ins = crate::syntax::parse_instruction(isa, s).expect("a statement");
                        top.call(&ins, opcode(ins.instruction_id), &mut tag)
                    })
                    .collect()
            })
            .collect();
        Self { top, batches }
    }

    /// The program as the protocol's static program, which states its set,
    /// and `needs` beside.
    #[must_use]
    pub fn to_static(&self, needs: Needs) -> StaticProgram<InstructionCall> {
        let needs = needs.with_instruction_set(Some(Arc::clone(&self.top.set)));
        StaticProgram::new(needs, self.batches.clone())
    }
}
