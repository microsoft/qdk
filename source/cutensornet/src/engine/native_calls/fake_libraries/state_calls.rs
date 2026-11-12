// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! `StateCalls` of the fake libraries: states, their operators and
//! computes, network operators and expectations.

use super::{
    Computed, FakeLibraries, FakeState, Object, failed, id,
    state_vector::{StateVector, truncate},
};
use crate::engine::{
    EngineError,
    native_calls::{
        OpaqueHandle, Stream,
        state_calls::{
            MpsLayout, NativeCapacity, StateCalls, StateF64Configuration, StateI32Configuration,
            StateU32Configuration,
        },
    },
};
use num_complex::Complex64;

impl StateCalls for FakeLibraries {
    fn create_state(
        &self,
        handle: OpaqueHandle,
        mode_extents: &[i64],
    ) -> Result<OpaqueHandle, EngineError> {
        let mut inner = self.call("create_state")?;
        if mode_extents.iter().any(|&extent| extent != 2) {
            return Err(failed("create_state", "a mode is not a qubit"));
        }
        inner.create_on(
            handle,
            "create_state",
            Object::State(FakeState {
                sites: mode_extents.len(),
                start: None,
                operators: Vec::new(),
                bonds: None,
                configurations: 0,
                prepared_at: None,
                computed: None,
            }),
        )
    }

    fn destroy_state(&self, state: OpaqueHandle) -> Result<(), EngineError> {
        self.call("destroy_state")?
            .destroy(state, "destroy_state", |o| matches!(o, Object::State(_)))
    }

    fn apply_tensor_operator(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        modes: &[i32],
        tensor: OpaqueHandle,
        unitary: bool,
    ) -> Result<(), EngineError> {
        const OPERATION: &str = "apply_tensor_operator";
        let mut inner = self.call(OPERATION)?;
        inner.owned(handle, state, OPERATION)?;
        inner.buffer(id(tensor), OPERATION)?;
        let sites = modes
            .iter()
            .map(|&mode| usize::try_from(mode).map_err(|_| failed(OPERATION, "negative mode")))
            .collect::<Result<Vec<_>, _>>()?;
        inner.applied.push((modes.to_vec(), unitary));
        let target = inner.state(state, OPERATION)?;
        if sites.iter().any(|&site| site >= target.sites) {
            return Err(failed(OPERATION, "a mode outside the state"));
        }
        target.operators.push((sites, id(tensor)));
        Ok(())
    }

    fn finalize_mps(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        capacity: &NativeCapacity,
    ) -> Result<(), EngineError> {
        let mut inner = self.call("finalize_mps")?;
        inner.owned(handle, state, "finalize_mps")?;
        let state = inner.state(state, "finalize_mps")?;
        if state.bonds.is_some() {
            return Err(failed("finalize_mps", "the state is already finalized"));
        }
        let shape = capacity.native_shape();
        // As the library: MPS finalization needs at least two sites.
        if state.sites < 2 {
            return Err(failed("finalize_mps", "an MPS needs at least two sites"));
        }
        if shape.site_count() != state.sites {
            return Err(failed(
                "finalize_mps",
                "the capacity has another site count",
            ));
        }
        let bonds: Vec<usize> = (0..state.sites - 1)
            .map(|cut| shape.bond_dim(cut).unwrap_or(1))
            .collect();
        state.bonds = Some(bonds.clone());
        inner.finalized_bonds.push(bonds);
        Ok(())
    }

    fn configure_state_f64(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        configuration: StateF64Configuration,
    ) -> Result<(), EngineError> {
        let mut inner = self.call("configure_state_f64")?;
        inner.owned(handle, state, "configure_state_f64")?;
        let state = inner.state(state, "configure_state_f64")?;
        if state.bonds.is_none() {
            return Err(failed("configure_state_f64", "configured before finalize"));
        }
        state.configurations += 1;
        inner.f64_configurations.push(configuration);
        Ok(())
    }

    fn configure_state_u32(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        configuration: StateU32Configuration,
    ) -> Result<(), EngineError> {
        let mut inner = self.call("configure_state_u32")?;
        inner.owned(handle, state, "configure_state_u32")?;
        let fake_state = inner.state(state, "configure_state_u32")?;
        if fake_state.bonds.is_none() {
            return Err(failed("configure_state_u32", "configured before finalize"));
        }
        fake_state.configurations += 1;
        inner.u32_configurations.push((id(state), configuration));
        Ok(())
    }

    fn configure_state_i32(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        _configuration: StateI32Configuration,
    ) -> Result<(), EngineError> {
        let mut inner = self.call("configure_state_i32")?;
        inner.owned(handle, state, "configure_state_i32")?;
        let fake_state = inner.state(state, "configure_state_i32")?;
        if fake_state.bonds.is_none() {
            return Err(failed("configure_state_i32", "configured before finalize"));
        }
        Ok(())
    }

    fn prepare_state(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        maximum_workspace_bytes: usize,
        workspace: OpaqueHandle,
        _stream: Stream,
    ) -> Result<(), EngineError> {
        let mut inner = self.call("prepare_state")?;
        inner.owned(handle, state, "prepare_state")?;
        inner.owned(handle, workspace, "prepare_state")?;
        inner.prepare_limits.push(maximum_workspace_bytes);
        inner.require(workspace, "prepare_state", |o| {
            matches!(o, Object::Workspace { .. })
        })?;
        let state = inner.state(state, "prepare_state")?;
        if state.bonds.is_none() {
            return Err(failed("prepare_state", "prepared before finalize"));
        }
        state.prepared_at = Some(state.operators.len());
        Ok(())
    }

    fn compute_state(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        workspace: OpaqueHandle,
        layout: &mut MpsLayout,
        outputs: &mut [OpaqueHandle],
        _stream: Stream,
    ) -> Result<(), EngineError> {
        const OPERATION: &str = "compute_state";
        let mut inner = self.call(OPERATION)?;
        inner.owned(handle, state, OPERATION)?;
        inner.owned(handle, workspace, OPERATION)?;
        let scratch_bytes = inner.lent_scratch(workspace, OPERATION)?;
        let live_scratch = inner.live_scratch();
        inner.computes.push((live_scratch, scratch_bytes));
        for output in outputs.iter() {
            inner.buffer(id(*output), OPERATION)?;
        }
        let (sites, start, operators, bonds) = {
            let state = inner.state(state, OPERATION)?;
            if state.prepared_at != Some(state.operators.len()) {
                return Err(failed(OPERATION, "not prepared since the last operator"));
            }
            let Some(bonds) = state.bonds.clone() else {
                return Err(failed(OPERATION, "not finalized"));
            };
            (
                state.sites,
                state.start.clone(),
                state.operators.clone(),
                bonds,
            )
        };
        if outputs.len() != sites || layout.extents.len() != sites {
            return Err(failed(OPERATION, "one output per site is needed"));
        }
        let mut computed = if let Some(start) = start {
            inner.outputs_live(&start, OPERATION)?;
            start.state
        } else {
            StateVector::zero(sites)
        };
        for (sites, tensor) in &operators {
            inner.apply(&mut computed, sites, *tensor, OPERATION)?;
        }
        for (cut, &bond) in bonds.iter().enumerate() {
            // Only where a value is dropped: the eigenvectors of a nearly
            // rank-one density matrix lose an amplitude of 1e-100.
            let kept = computed.kept_rank(cut).min(bond);
            if kept < computed.rank_above(cut, 1e-20) {
                truncate(&mut computed, cut, kept);
            }
        }
        let bond = |cut: usize| -> Result<i64, EngineError> {
            let realized = computed.kept_rank(cut).min(bonds[cut]);
            i64::try_from(realized).map_err(|_| failed(OPERATION, "bond overflow"))
        };
        for site in 0..sites {
            let extents = if let Some(misreported) = inner.misreported_extents.get(&site) {
                misreported.clone()
            } else if site == 0 {
                vec![2, bond(0)?]
            } else if site + 1 == sites {
                vec![bond(site - 1)?, 2]
            } else {
                vec![bond(site - 1)?, 2, bond(site)?]
            };
            if layout.extents[site].len() != extents.len()
                || layout.strides[site].len() != extents.len()
            {
                return Err(failed(OPERATION, "layout of the wrong rank"));
            }
            let mut stride = 1;
            for (axis, &extent) in extents.iter().enumerate() {
                layout.strides[site][axis] = stride;
                stride *= extent;
            }
            layout.extents[site].copy_from_slice(&extents);
        }
        let at = operators.len();
        let outputs: Vec<usize> = outputs.iter().map(|&output| id(output)).collect();
        let number = inner.computed_states.len();
        inner.computed_states.insert(number, computed.clone());
        for (site, &output) in outputs.iter().enumerate() {
            inner.mps_tensors.insert(output, (number, site));
        }
        inner.stream_busy = true;
        inner.state(state, OPERATION)?.computed = Some((
            Computed {
                state: computed,
                outputs,
            },
            at,
        ));
        Ok(())
    }

    fn initialize_mps(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        layout: &MpsLayout,
        tensors: &[OpaqueHandle],
    ) -> Result<(), EngineError> {
        const OPERATION: &str = "initialize_mps";
        let mut inner = self.call(OPERATION)?;
        inner.owned(handle, state, OPERATION)?;
        let tensors: Vec<usize> = tensors.iter().map(|&tensor| id(tensor)).collect();
        let Some(&(number, _)) = inner.mps_tensors.get(tensors.first().unwrap_or(&0)) else {
            return Err(failed(OPERATION, "the tensors are not a computed MPS"));
        };
        if tensors
            .iter()
            .enumerate()
            .any(|(site, tensor)| inner.mps_tensors.get(tensor) != Some(&(number, site)))
        {
            return Err(failed(OPERATION, "the tensors are not one computed MPS"));
        }
        let start = Computed {
            state: inner.computed_states[&number].clone(),
            outputs: tensors.clone(),
        };
        inner.outputs_live(&start, OPERATION)?;
        let sites = tensors.len();
        let ranks_fit = |tables: &[Box<[i64]>]| {
            tables.len() == sites
                && tables.iter().enumerate().all(|(site, table)| {
                    let rank = if site == 0 || site + 1 == sites { 2 } else { 3 };
                    table.len() == rank && table.iter().all(|&value| value > 0)
                })
        };
        if !ranks_fit(&layout.extents) || !ranks_fit(&layout.strides) {
            return Err(failed(OPERATION, "an extent or stride table does not fit"));
        }
        let target = inner.state(state, OPERATION)?;
        if target.sites != sites {
            return Err(failed(OPERATION, "the MPS has another site count"));
        }
        if !target.operators.is_empty() || target.computed.is_some() {
            return Err(failed(OPERATION, "the state already has operators"));
        }
        target.start = Some(start);
        Ok(())
    }

    fn capture_mps(&self, handle: OpaqueHandle, state: OpaqueHandle) -> Result<(), EngineError> {
        const OPERATION: &str = "capture_mps";
        let mut inner = self.call(OPERATION)?;
        inner.owned(handle, state, OPERATION)?;
        if inner.stream_busy {
            return Err(failed(OPERATION, "the compute may still be running"));
        }
        let target = inner.state(state, OPERATION)?;
        let computed = match &target.computed {
            Some((computed, at)) if *at == target.operators.len() => computed.clone(),
            _ => return Err(failed(OPERATION, "the state is not computed")),
        };
        target.start = Some(computed.clone());
        target.operators.clear();
        target.prepared_at = None;
        target.computed = Some((computed, 0));
        Ok(())
    }

    fn create_network_operator(
        &self,
        handle: OpaqueHandle,
        _mode_extents: &[i64],
    ) -> Result<OpaqueHandle, EngineError> {
        self.call("create_network_operator")?.create_on(
            handle,
            "create_network_operator",
            Object::Operator(Vec::new()),
        )
    }

    fn destroy_network_operator(&self, operator: OpaqueHandle) -> Result<(), EngineError> {
        self.call("destroy_network_operator")?
            .destroy(operator, "destroy_network_operator", |o| {
                matches!(o, Object::Operator(_))
            })
    }

    fn append_product(
        &self,
        handle: OpaqueHandle,
        operator: OpaqueHandle,
        coefficient: Complex64,
        factor_modes: &[Box<[i32]>],
        factor_tensors: &[OpaqueHandle],
    ) -> Result<(), EngineError> {
        const OPERATION: &str = "append_product";
        let mut inner = self.call(OPERATION)?;
        inner.owned(handle, operator, OPERATION)?;
        if factor_modes.len() != factor_tensors.len() {
            return Err(failed(OPERATION, "modes and tensors differ in number"));
        }
        let mut factors = Vec::with_capacity(factor_modes.len());
        for (modes, tensor) in factor_modes.iter().zip(factor_tensors) {
            inner.buffer(id(*tensor), OPERATION)?;
            let [mode] = **modes else {
                return Err(failed(OPERATION, "the fake takes one-mode factors"));
            };
            let mode = usize::try_from(mode).map_err(|_| failed(OPERATION, "negative mode"))?;
            factors.push((mode, id(*tensor)));
        }
        match inner.objects.get_mut(&id(operator)) {
            Some(Object::Operator(products)) => {
                products.push((coefficient, factors));
                Ok(())
            }
            _ => Err(failed(OPERATION, "no such operator")),
        }
    }

    fn create_expectation(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        operator: OpaqueHandle,
    ) -> Result<OpaqueHandle, EngineError> {
        let mut inner = self.call("create_expectation")?;
        inner.owned(handle, state, "create_expectation")?;
        inner.owned(handle, operator, "create_expectation")?;
        inner.require(state, "create_expectation", |o| {
            matches!(o, Object::State(_))
        })?;
        inner.require(operator, "create_expectation", |o| {
            matches!(o, Object::Operator(_))
        })?;
        inner.create_on(
            handle,
            "create_expectation",
            Object::Expectation {
                state: id(state),
                operator: id(operator),
                prepared: false,
            },
        )
    }

    fn destroy_expectation(&self, expectation: OpaqueHandle) -> Result<(), EngineError> {
        self.call("destroy_expectation")?
            .destroy(expectation, "destroy_expectation", |o| {
                matches!(o, Object::Expectation { .. })
            })
    }

    fn configure_expectation_hyper_samples(
        &self,
        handle: OpaqueHandle,
        expectation: OpaqueHandle,
        hyper_samples: i32,
    ) -> Result<(), EngineError> {
        let mut inner = self.call("configure_expectation_hyper_samples")?;
        inner.owned(handle, expectation, "configure_expectation_hyper_samples")?;
        inner.require(expectation, "configure_expectation_hyper_samples", |o| {
            matches!(o, Object::Expectation { .. })
        })?;
        inner.hyper_samples.push(hyper_samples);
        if hyper_samples > 0 {
            Ok(())
        } else {
            Err(failed(
                "configure_expectation_hyper_samples",
                "not positive",
            ))
        }
    }

    fn prepare_expectation(
        &self,
        handle: OpaqueHandle,
        expectation: OpaqueHandle,
        _maximum_workspace_bytes: usize,
        workspace: OpaqueHandle,
        _stream: Stream,
    ) -> Result<(), EngineError> {
        let mut inner = self.call("prepare_expectation")?;
        inner.owned(handle, expectation, "prepare_expectation")?;
        inner.owned(handle, workspace, "prepare_expectation")?;
        inner.require(workspace, "prepare_expectation", |o| {
            matches!(o, Object::Workspace { .. })
        })?;
        match inner.objects.get_mut(&id(expectation)) {
            Some(Object::Expectation { prepared, .. }) => {
                *prepared = true;
                Ok(())
            }
            _ => Err(failed("prepare_expectation", "no such expectation")),
        }
    }

    fn compute_expectation(
        &self,
        handle: OpaqueHandle,
        expectation: OpaqueHandle,
        workspace: OpaqueHandle,
        _stream: Stream,
    ) -> Result<(Complex64, Complex64), EngineError> {
        const OPERATION: &str = "compute_expectation";
        let mut inner = self.call(OPERATION)?;
        inner.owned(handle, expectation, OPERATION)?;
        inner.owned(handle, workspace, OPERATION)?;
        inner.lent_scratch(workspace, OPERATION)?;
        let queued = inner.expectation_values.pop_front();
        let Some(Object::Expectation {
            state,
            operator,
            prepared: true,
        }) = inner.objects.get(&id(expectation))
        else {
            return Err(failed(OPERATION, "no prepared expectation"));
        };
        let Some(Object::Operator(products)) = inner.objects.get(operator) else {
            return Err(failed(OPERATION, "no such operator"));
        };
        let computed = inner.computed(*state, OPERATION)?;
        let mut value = Complex64::new(0.0, 0.0);
        for (coefficient, factors) in products {
            let mut ket = computed.clone();
            for &(mode, tensor) in factors {
                inner.apply(&mut ket, &[mode], tensor, OPERATION)?;
            }
            value += coefficient * computed.inner(&ket);
        }
        let squared_norm = inner
            .squared_norm
            .unwrap_or_else(|| computed.inner(computed));
        Ok((queued.unwrap_or(value), squared_norm))
    }
}
