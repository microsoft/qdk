// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use super::{Function, Instruction, Module, Operand, Param};
use crate::model::Type;
use crate::qir;

#[derive(Debug, Clone)]
pub struct BuilderError(pub String);

impl std::fmt::Display for BuilderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BuilderError: {}", self.0)
    }
}

/// Cursor-based builder for in-place IR mutation.
pub struct ModuleBuilder<'a> {
    module: &'a mut Module,
    func_idx: usize,
    block_idx: usize,
    instr_idx: usize,
}

impl<'a> ModuleBuilder<'a> {
    pub fn new(module: &'a mut Module) -> Self {
        Self {
            module,
            func_idx: 0,
            block_idx: 0,
            instr_idx: 0,
        }
    }

    /// Set cursor to the function with the given name.
    pub fn position_at_function(&mut self, name: &str) -> Result<(), BuilderError> {
        let idx = self
            .module
            .functions
            .iter()
            .position(|f| f.name == name)
            .ok_or_else(|| BuilderError(format!("function not found: {name}")))?;
        self.func_idx = idx;
        self.block_idx = 0;
        self.instr_idx = 0;
        Ok(())
    }

    /// Set cursor to the block with the given label within the current function.
    pub fn position_at_block(&mut self, label: &str) -> Result<(), BuilderError> {
        let func = &self.module.functions[self.func_idx];
        let idx = func
            .basic_blocks
            .iter()
            .position(|b| b.name == label)
            .ok_or_else(|| BuilderError(format!("block not found: {label}")))?;
        self.block_idx = idx;
        self.instr_idx = 0;
        Ok(())
    }

    /// Set cursor before a specific instruction index in the current block.
    pub fn position_before(&mut self, instr_idx: usize) {
        self.instr_idx = instr_idx;
    }

    /// Set cursor to end of current block.
    pub fn position_at_end(&mut self) {
        let len = self.module.functions[self.func_idx].basic_blocks[self.block_idx]
            .instructions
            .len();
        self.instr_idx = len;
    }

    /// Insert an instruction before the current cursor position.
    /// Equivalent to `PyQIR`'s `builder.insert_before(instr, new_instr)`.
    pub fn insert_before(&mut self, instruction: Instruction) {
        let block = &mut self.module.functions[self.func_idx].basic_blocks[self.block_idx];
        block.instructions.insert(self.instr_idx, instruction);
        self.instr_idx += 1;
    }

    /// Insert an instruction at the end of the current block.
    /// Equivalent to `PyQIR`'s `builder.insert_at_end(block, instr)`.
    pub fn insert_at_end(&mut self, instruction: Instruction) {
        let block = &mut self.module.functions[self.func_idx].basic_blocks[self.block_idx];
        block.instructions.push(instruction);
    }

    /// Remove the instruction at the current cursor position.
    /// Equivalent to `PyQIR`'s `call.erase()` / `instr.remove()`.
    /// Cursor stays at same index (now pointing to next instruction).
    pub fn erase_at_cursor(&mut self) -> Instruction {
        let block = &mut self.module.functions[self.func_idx].basic_blocks[self.block_idx];
        block.instructions.remove(self.instr_idx)
    }

    /// Returns the number of instructions in the current block.
    #[must_use]
    pub fn current_block_len(&self) -> usize {
        self.module.functions[self.func_idx].basic_blocks[self.block_idx]
            .instructions
            .len()
    }

    /// Construct a Call instruction and insert it at the cursor position.
    /// Equivalent to `PyQIR`'s `builder.call(func, args)`.
    pub fn call(
        &mut self,
        callee: &str,
        args: Vec<(Type, Operand)>,
        return_ty: Option<Type>,
        result: Option<String>,
    ) {
        let instr = Instruction::Call {
            callee: callee.to_string(),
            args,
            return_ty,
            result,
            attr_refs: Vec::new(),
        };
        self.insert_before(instr);
    }

    /// Construct an arbitrary instruction and insert it at the cursor position.
    /// Equivalent to `PyQIR`'s `builder.instr(opcode, operands)`.
    pub fn instr(&mut self, instruction: Instruction) {
        self.insert_before(instruction);
    }

    /// Add a function declaration if one with the same name doesn't already exist.
    /// Returns `true` if a new declaration was added.
    pub fn ensure_declaration(
        &mut self,
        name: &str,
        return_type: Type,
        param_types: Vec<Type>,
    ) -> bool {
        if self.module.functions.iter().any(|f| f.name == name) {
            return false;
        }
        self.module.functions.push(Function {
            name: name.to_string(),
            return_type,
            params: param_types
                .into_iter()
                .map(|ty| Param { ty, name: None })
                .collect(),
            is_declaration: true,
            attribute_group_refs: Vec::new(),
            basic_blocks: Vec::new(),
        });
        true
    }

    /// Remove functions not matching a predicate.
    /// Resets cursor indices to 0 after removal to avoid stale references.
    pub fn retain_functions<F: FnMut(&Function) -> bool>(&mut self, f: F) {
        self.module.functions.retain(f);
        self.func_idx = 0;
        self.block_idx = 0;
        self.instr_idx = 0;
    }

    /// Find the entry point function by scanning attribute groups for
    /// the `"entry_point"` attribute.
    #[must_use]
    pub fn entry_point_index(&self) -> Option<usize> {
        qir::inspect::find_entry_point(self.module)
    }

    /// Immutable access to the underlying `Module`.
    #[must_use]
    pub fn module(&self) -> &Module {
        self.module
    }

    /// Mutable access to the underlying `Module`.
    pub fn module_mut(&mut self) -> &mut Module {
        self.module
    }
}
