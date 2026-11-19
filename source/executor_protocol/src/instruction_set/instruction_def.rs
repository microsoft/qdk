// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An instruction's declaration in its set ([`InstructionDef`]), built
//! field by field or step by step.

use super::{BlockOperand, DeclarationFault, Meaning, Parameter, ParameterKind};

/// An instruction's declaration in its set, as Qodec writes it.
///
/// Built field by field, or step by step from [`InstructionDef::new`]:
/// Qodec's `{mnemonic: rz, in: [qubit], out: [qubit], parameters: {theta:
/// number}}` is `InstructionDef::new("rz").acts_on([BlockType::qubit()])
/// .parameter("theta", ParameterKind::Number)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstructionDef {
    /// `mnemonic`: its name, unique in its set.
    pub mnemonic: String,
    /// `in`: the blocks it acts on, in order.
    pub inputs: Vec<BlockOperand>,
    /// `out`: those that survive, at the same positions, or that it makes,
    /// past `in`'s.
    pub outputs: Vec<BlockOperand>,
    /// Its classical inputs, in order.
    pub parameters: Vec<Parameter>,
    /// Its `observe` outcomes.
    pub outcomes: u32,
    /// Then its flags: its readouts are the outcomes, then the flags.
    pub flags: Vec<String>,
    /// Its Qodec `action`, in canonical form, or undeclared.
    pub meaning: Meaning,
}

impl InstructionDef {
    /// The instruction `mnemonic`, on no block, with no parameter, readout
    /// or meaning yet.
    #[must_use]
    pub fn new(mnemonic: impl Into<String>) -> Self {
        Self {
            mnemonic: mnemonic.into(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            parameters: Vec::new(),
            outcomes: 0,
            flags: Vec::new(),
            meaning: Meaning::undeclared(),
        }
    }

    /// Takes `operands` (`in`), consuming them.
    #[must_use]
    pub fn takes(mut self, operands: impl IntoIterator<Item = impl Into<BlockOperand>>) -> Self {
        self.inputs = operands.into_iter().map(Into::into).collect();
        self
    }

    /// Gives `operands` (`out`): those at `in`'s positions survive, the
    /// others are made.
    #[must_use]
    pub fn gives(mut self, operands: impl IntoIterator<Item = impl Into<BlockOperand>>) -> Self {
        self.outputs = operands.into_iter().map(Into::into).collect();
        self
    }

    /// Acts on `operands` in place: they are both its `in` and its `out`,
    /// as a gate's qubits.
    #[must_use]
    pub fn acts_on(self, operands: impl IntoIterator<Item = impl Into<BlockOperand>>) -> Self {
        let operands: Vec<BlockOperand> = operands.into_iter().map(Into::into).collect();
        self.takes(operands.clone()).gives(operands)
    }

    /// Then the parameter `name` of kind `kind`.
    #[must_use]
    pub fn parameter(mut self, name: impl Into<String>, kind: ParameterKind) -> Self {
        self.parameters.push(Parameter::new(name, kind));
        self
    }

    /// Gives `outcomes` outcomes.
    #[must_use]
    pub fn outcomes(mut self, outcomes: u32) -> Self {
        self.outcomes = outcomes;
        self
    }

    /// Reports `flags`, after its outcomes.
    #[must_use]
    pub fn flags(mut self, flags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.flags = flags.into_iter().map(Into::into).collect();
        self
    }

    /// Does what `meaning` says.
    #[must_use]
    pub fn meaning(mut self, meaning: Meaning) -> Self {
        self.meaning = meaning;
        self
    }

    /// A parameter or a flag it names twice, each repeat.
    pub(super) fn naming_faults(&self) -> Vec<DeclarationFault> {
        let mut faults = Vec::new();
        for (k, parameter) in self.parameters.iter().enumerate() {
            if self.parameters[..k]
                .iter()
                .any(|earlier| earlier.name == parameter.name)
            {
                faults.push(DeclarationFault::RepeatedParameter {
                    mnemonic: self.mnemonic.clone(),
                    parameter: parameter.name.clone(),
                });
            }
        }
        for (k, flag) in self.flags.iter().enumerate() {
            if self.flags[..k].contains(flag) {
                faults.push(DeclarationFault::RepeatedFlag {
                    mnemonic: self.mnemonic.clone(),
                    flag: flag.clone(),
                });
            }
        }
        faults
    }
}
