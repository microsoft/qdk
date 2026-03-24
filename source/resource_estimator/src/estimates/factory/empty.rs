// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::marker::PhantomData;

use crate::estimates::{ErrorCorrection, Factory, FactoryBuilder};

pub struct NoFactories<E> {
    phantom: PhantomData<E>,
}

impl<E> Default for NoFactories<E> {
    fn default() -> Self {
        Self {
            phantom: PhantomData,
        }
    }
}

#[derive(Clone)]
pub struct NoFactory<P> {
    phantom: PhantomData<P>,
}

impl<E: ErrorCorrection<Parameter = impl Clone>> FactoryBuilder<E> for NoFactories<E> {
    type Factory = NoFactory<E::Parameter>;

    fn find_factories(
        &self,
        _ftp: &E,
        _qubit: &std::rc::Rc<<E as ErrorCorrection>::Qubit>,
        _magic_state_type: usize,
        _output_error_rate: f64,
        _max_code_parameter: &<E as ErrorCorrection>::Parameter,
    ) -> Result<Vec<std::borrow::Cow<'_, Self::Factory>>, String> {
        unreachable!()
    }

    fn num_magic_state_types(&self) -> usize {
        0
    }
}

impl<P: Clone> Factory for NoFactory<P> {
    type Parameter = P;

    fn physical_qubits(&self) -> u64 {
        unreachable!()
    }

    fn duration(&self) -> u64 {
        unreachable!()
    }

    fn num_output_states(&self) -> u64 {
        unreachable!()
    }

    fn max_code_parameter(&self) -> Option<std::borrow::Cow<'_, Self::Parameter>> {
        unreachable!()
    }
}
