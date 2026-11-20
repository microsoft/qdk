// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`LiveBlocks`]: a shot's live blocks under Qodec's labels, with their
//! types, so that the parts refuse a call whose new block would take the
//! label of a live block of another type.
//!
//! The protocol addresses a block by its type and its index among that
//! type's blocks, so a `surface` block 0 and a `merged` block 0 are two
//! blocks. Qodec addresses it by a label alone, which the parts take from
//! the index, so both are its label 0: its runtime and its translator end
//! the live block under a label a call prepares a new block at, so
//! preparing the `surface` block would end the `merged` one, and the
//! program would read the wrong block, or fault, later. How the two
//! address spaces should meet is open; meanwhile a call that would make
//! two blocks share a label fails, naming them, before anything of its
//! batch runs on the shot.
//!
//! A block's type follows Qodec's own check ([`TypeChecker`]): a call's
//! inputs end, and its outputs become live, each of the type its operand
//! declares, so a merge's output keeps its first input's label under the
//! merged type. Unlike that check, an input that is not live passes:
//! Qodec's runtime decides what such a call does.
//!
//! [`TypeChecker`]: qodec_runtime_recipes::TypeChecker

use qodec_runtime_recipes::{Call, Isa};
use rustc_hash::FxHashMap;

use super::TranslationFault;

/// A shot's live blocks, by Qodec label, each with its type's index in its
/// ISA. Tracked only for an ISA of several block types: with one, a label
/// names one block of the protocol.
#[derive(Clone, Debug, Default)]
pub(crate) struct LiveBlocks {
    types: FxHashMap<u64, usize>,
}

impl LiveBlocks {
    /// The live blocks after `calls`, calls of `isa`, run on these, once
    /// no call of them would prepare a block at the label of a live block
    /// of another type.
    ///
    /// A call whose arguments do not fit its instruction, or whose operands
    /// do not bind, ends the tracking there: the blocks returned are those
    /// after the calls before it, and the part fails the batch on that call.
    /// In the layers, Qodec's runtime rejects a call whose arguments do not
    /// fit before running anything of the batch, so the shot keeps the
    /// blocks it had before the batch; a call whose operands do not bind is
    /// a fault, which loses the shot. The executor's translator fails on
    /// either, before running anything of the batch, and loses the shot.
    /// The exception is a release without a gadget, which Qodec's layers
    /// run unbound: the batch runs, but the blocks returned omit what the
    /// rest of the batch ends and prepares, so a later clash may go
    /// unchecked, or a later valid call be refused. Only an ill-typed
    /// program on an ISA of several types reaches it, one that Qodec's
    /// drivers reject first with their `TypeChecker`, which is not ported.
    ///
    /// # Errors
    ///
    /// When a call prepares a block, an output that is not one of its
    /// inputs, at the label of a live block of another type: the fault
    /// names the call, both types and the label.
    pub(crate) fn after_checking_labels(
        &self,
        isa: &Isa,
        calls: &[Call],
    ) -> Result<Self, TranslationFault> {
        if isa.types.names.len() < 2 {
            return Ok(Self::default());
        }
        let mut live = self.clone();
        for call in calls {
            let Ok(parsed) = isa.parse_call(call.instruction_id, &call.parameters) else {
                break;
            };
            let signature = &isa.instructions[parsed.index];
            let Ok((inputs, outputs)) = signature.bind(&isa.types, &parsed.operands, &live.types)
            else {
                break;
            };
            for output in outputs.iter().skip(inputs.len()) {
                if let Some(&held) = live.types.get(&output.label)
                    && held != output.block_type
                {
                    let label = output.label;
                    return Err(TranslationFault {
                        mnemonic: signature.mnemonic.clone(),
                        detail: format!(
                            "its new `{}` block {label} and the live `{}` block {label} would \
                             share Qodec's label {label}, as Qodec has one label per block \
                             whatever its type",
                            isa.types.names[output.block_type], isa.types.names[held]
                        ),
                    });
                }
            }
            for input in &inputs {
                live.types.remove(&input.label);
            }
            for output in outputs {
                live.types.insert(output.label, output.block_type);
            }
        }
        Ok(live)
    }
}
