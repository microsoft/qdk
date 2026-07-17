// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::Env;
use crate::backend::{Backend, TracingBackend};
use crate::output::GenericReceiver;
use crate::val::Value;
use qsc_fir::fir::{ExecGraphConfig, PackageStoreLookup};

/// A zero-sized backend used only to evaluate classical arithmetic functions.
struct ClassicalBackend;

impl Backend for ClassicalBackend {}

/// Helper to evaluate Q# functions.
pub(crate) struct FunctionEvaluator {
    /// Evaluation environment.
    env: Env,
    /// Function to evaluate.
    function_val: Value,
    /// Package containing the function.
    package: qsc_fir::fir::PackageId,
}

impl FunctionEvaluator {
    pub(crate) fn new(function_val: Value) -> Result<Self, String> {
        let package = match &function_val {
            Value::Closure(closure) => closure.id.package,
            Value::Global(id, _) => id.package,
            _ => return Err("classical arithmetic function must be callable".to_string()),
        };

        Ok(Self {
            env: Env::default(),
            function_val,
            package,
        })
    }

    /// Evaluates Q# function on the given input.
    pub(crate) fn evaluate(
        &mut self,
        globals: &impl PackageStoreLookup,
        input: Value,
    ) -> Result<Value, String> {
        let mut scratch = ClassicalBackend;
        let mut backend = TracingBackend::no_tracer(&mut scratch);
        let mut sink = std::io::sink();
        let mut receiver = GenericReceiver::new(&mut sink);
        crate::invoke(
            self.package,
            None,
            globals,
            ExecGraphConfig::NoDebug,
            &mut self.env,
            &mut backend,
            &mut receiver,
            self.function_val.clone(),
            input,
        )
        .map_err(|(err, _)| err.to_string())
    }
}
