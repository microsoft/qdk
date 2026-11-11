// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    BatchResult, QuantumBackend, QuantumBackendInstruction, RzTranspiler,
    interface::Driver,
    transpilers::{
        CliffordTranspiler, DecomposeToffoliTranspiler, MzResetTranspiler, SwapTransplier,
        Transpiler,
    },
};
use rustc_hash::FxHashMap;

/// Applies transpilers selected by source instruction name.
///
/// Unregistered instructions pass through unchanged. Generated instructions are
/// forwarded directly to the backend, not fed into other transpilers.
pub struct TranspilingAdapter<B> {
    backend: B,
    transpilers: FxHashMap<&'static str, Box<dyn Transpiler>>,
}

impl<B: QuantumBackend> TranspilingAdapter<B> {
    /// Returns mutable access to the wrapped backend.
    pub fn get_backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    /// Wraps a backend, rejecting duplicate source instruction names.
    pub fn new(backend: B, transpilers: Vec<Box<dyn Transpiler>>) -> Result<Self, String> {
        let mut by_name = FxHashMap::default();
        for transpiler in transpilers {
            let name = transpiler.source_instruction_name();
            if by_name.insert(name, transpiler).is_some() {
                return Err(format!("Duplicate transpiler for instruction `{name}`"));
            }
        }
        Ok(Self {
            backend,
            transpilers: by_name,
        })
    }

    /// Wraps a backend with Clifford, SWAP, Toffoli, measurement/reset, and rotation decompositions.
    pub fn with_defaults(backend: B) -> Self {
        Self::new(
            backend,
            vec![
                Box::new(SwapTransplier),
                Box::new(DecomposeToffoliTranspiler),
                Box::new(RzTranspiler::new(1e-6, true)),
                Box::new(MzResetTranspiler),
                Box::new(CliffordTranspiler::Cy),
                Box::new(CliffordTranspiler::Cz),
                Box::new(CliffordTranspiler::Sx),
            ],
        )
        .unwrap_or_else(|error| panic!("{error}"))
    }
}

impl<B: QuantumBackend> Driver for TranspilingAdapter<B> {
    type Backend = B;

    /// Returns the wrapped backend.
    fn get_backend(&self) -> &B {
        &self.backend
    }
}

impl<B: QuantumBackend> QuantumBackend for TranspilingAdapter<B> {
    /// Transpiles each source instruction once and forwards the resulting batch.
    fn execute_batch(
        &mut self,
        instructions: &[QuantumBackendInstruction],
    ) -> Result<BatchResult, String> {
        let mut transformed = Vec::with_capacity(instructions.len());
        for instruction in instructions {
            if let Some(transpiler) = self.transpilers.get_mut(instruction.instruction_name) {
                transpiler.transpile(instruction, &mut transformed)?;
            } else {
                transformed.push(instruction.clone());
            }
        }
        self.backend.execute_batch(&transformed)
    }
}
