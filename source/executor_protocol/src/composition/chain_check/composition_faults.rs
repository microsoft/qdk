// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Why the [`on`](crate::DriverJoins::on) that makes a runner refused its
//! chain: [`CompositionFaults`], every fault of the chain at once.

use std::error::Error;
use std::fmt;

use crate::{AdmissionFaults, FitFaults};

/// Every fault of a chain, reported at once by the
/// [`on`](crate::DriverJoins::on) that makes its
/// runner, before any shot runs: each boundary whose instructions do not
/// fit, top to bottom, and the admission's faults, if a part's offer does
/// not meet the driver's needs as they reach it.
#[derive(Clone, Debug)]
pub struct CompositionFaults {
    fit: Vec<FitFaults>,
    admission: Option<AdmissionFaults>,
}

impl CompositionFaults {
    /// The faults `fit` and `admission`, at least one of them.
    pub(super) fn new(fit: Vec<FitFaults>, admission: Option<AdmissionFaults>) -> Self {
        Self { fit, admission }
    }

    /// Each boundary of the chain that does not fit, top to bottom.
    #[must_use]
    pub fn fit(&self) -> &[FitFaults] {
        &self.fit
    }

    /// The admission's faults, if a part's offer does not meet the driver's
    /// needs as they reach it.
    #[must_use]
    pub fn admission(&self) -> Option<&AdmissionFaults> {
        self.admission.as_ref()
    }
}

impl fmt::Display for CompositionFaults {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the chain does not compose:")?;
        for fit in &self.fit {
            write!(f, " {fit}")?;
        }
        if let Some(admission) = &self.admission {
            write!(f, " {admission}")?;
        }
        Ok(())
    }
}

impl Error for CompositionFaults {}
