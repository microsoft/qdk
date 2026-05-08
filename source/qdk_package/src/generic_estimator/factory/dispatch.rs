// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{borrow::Cow, ops::Deref, rc::Rc};

use pyo3::{Bound, types::PyDict};
use resource_estimator::estimates::FactoryBuilder;

use super::super::{code::PythonQEC, utils::SerializableBound};

use super::{PythonFactory, PythonFactoryBuilder};

/// A generic factory dispatcher to support declaring multiple factory builders
/// as input to generic resource estimation.
pub struct PythonFactoryBuilderDispatch<'py>(pub Vec<PythonFactoryBuilder<'py>>);

impl<'py> Deref for PythonFactoryBuilderDispatch<'py> {
    type Target = Vec<PythonFactoryBuilder<'py>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'py> FactoryBuilder<PythonQEC<'py>> for PythonFactoryBuilderDispatch<'py> {
    type Factory = PythonFactory<'py>;

    fn find_factories(
        &self,
        ftp: &PythonQEC<'py>,
        qubit: &Rc<Bound<'py, PyDict>>,
        magic_state_type: usize,
        output_error_rate: f64,
        max_code_parameter: &SerializableBound<'py>,
    ) -> Result<Vec<Cow<'_, Self::Factory>>, String> {
        self[magic_state_type].find_factories(
            ftp,
            qubit,
            magic_state_type,
            output_error_rate,
            max_code_parameter,
        )
    }

    fn num_magic_state_types(&self) -> usize {
        self.len()
    }
}
