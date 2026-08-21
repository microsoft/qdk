// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{env, num::NonZeroUsize};

use num_complex::Complex64;
use tensor4all_core::{
    AnyScalar, Canonical, DynIndex, FactorizeOptions, IdxTensor, SvdTruncationPolicy, TensorIndex,
    contract, factorize,
};
use tensor4all_treetn::{CanonicalizationOptions, TreeTN};

use crate::{
    CapabilityStatus, EngineDescriptor, EngineInfo, ExecutionPolicy, Matrix2, Matrix4, Measurement,
    MpsCapabilities, MpsEngine, MpsEngineFactory, MpsError, Pauli, ResourceResolution,
    ResourceResolutionSource, SiteId, SitePauli,
};

const TENSOR4ALL_REVISION: &str = "b44786399664d92c5ba5e000c7a471c2a1509280";
const PROJECTION_TOLERANCE: f64 = 1.0e-15;

type Mps = TreeTN<IdxTensor, usize>;

#[derive(Clone, Copy, Debug, Default)]
pub struct Tensor4AllFactory;

#[must_use]
pub fn factory() -> Tensor4AllFactory {
    Tensor4AllFactory
}

pub struct Tensor4AllEngine {
    state: Mps,
    physical_indices: Vec<DynIndex>,
    factorize_options: FactorizeOptions,
    info: EngineInfo,
    reached_bond_dimension: usize,
}

impl MpsEngineFactory for Tensor4AllFactory {
    type Engine = Tensor4AllEngine;

    fn descriptor(&self) -> EngineDescriptor {
        descriptor()
    }

    fn capabilities(&self) -> MpsCapabilities {
        let planned = |reason: &str| CapabilityStatus::Planned {
            reason: reason.into(),
        };
        MpsCapabilities {
            complex64: CapabilityStatus::Available,
            maximum_gate_arity: 2,
            dynamic_allocation: CapabilityStatus::Available,
            measurement_reset: CapabilityStatus::Available,
            non_local_routing: planned("non-local routing is a later core iteration"),
            observables: CapabilityStatus::Available,
            noise: planned("noise support is a later trajectory or mixed-state iteration"),
            discarded_weight_diagnostics: planned(
                "the pinned tensor4all API does not expose discarded weight",
            ),
            constrained_cpu_resources: planned(
                "the pinned tensor4all API uses a process-global execution context",
            ),
            backend: "tensor4all/tenferro-faer".into(),
            device: "cpu".into(),
        }
    }

    fn create_engine(&self, policy: &ExecutionPolicy) -> Result<Self::Engine, MpsError> {
        policy.validate()?;
        if policy.resources.max_cpu_threads.is_some() {
            return Err(MpsError::NoEngineSatisfiesPolicy);
        }

        let resources = resolve_resources()?;
        let mut factorize_options = FactorizeOptions::svd().with_canonical(Canonical::Left);
        if let Some(threshold) = policy
            .truncation
            .max_relative_discarded_squared_weight_per_split
        {
            factorize_options = factorize_options.with_svd_policy(
                SvdTruncationPolicy::new(threshold)
                    .with_relative()
                    .with_squared_values()
                    .with_discarded_tail_sum(),
            );
        }
        if let Some(max_bond_dimension) = policy.truncation.max_bond_dimension {
            factorize_options = factorize_options.with_max_bond_dim(max_bond_dimension.get());
        }

        Ok(Tensor4AllEngine {
            state: Mps::new(),
            physical_indices: Vec::new(),
            factorize_options,
            info: EngineInfo {
                descriptor: descriptor(),
                resources,
            },
            reached_bond_dimension: 1,
        })
    }
}

impl MpsEngine for Tensor4AllEngine {
    fn info(&self) -> EngineInfo {
        self.info.clone()
    }

    fn append_zero_site(&mut self) -> Result<SiteId, MpsError> {
        let site = SiteId(self.physical_indices.len());
        let physical = DynIndex::new_dyn(2);
        if site.0 == 0 {
            let tensor = IdxTensor::from_dense(
                vec![physical.clone()],
                vec![Complex64::ONE, Complex64::ZERO],
            )
            .map_err(engine_error)?;
            self.state
                .add_tensor(site.0, tensor)
                .map_err(engine_error)?;
        } else {
            let previous_name = site.0 - 1;
            let previous_node = self.state.node_index(&previous_name).ok_or_else(|| {
                MpsError::InternalInvariant(format!("missing MPS site {previous_name}"))
            })?;
            let previous_tensor = self
                .state
                .tensor(previous_node)
                .ok_or_else(|| {
                    MpsError::InternalInvariant(format!("missing MPS tensor {previous_name}"))
                })?
                .clone();
            let bond = DynIndex::new_dyn(1);
            let mut extended_indices = previous_tensor.external_indices();
            extended_indices.push(bond.clone());
            let extended_previous = IdxTensor::from_dense(
                extended_indices,
                previous_tensor
                    .to_vec::<Complex64>()
                    .map_err(engine_error)?,
            )
            .map_err(engine_error)?;
            self.state
                .replace_tensor(previous_node, extended_previous)
                .map_err(engine_error)?
                .ok_or_else(|| {
                    MpsError::InternalInvariant(format!("missing MPS site {previous_name}"))
                })?;

            let tensor = IdxTensor::from_dense(
                vec![bond.clone(), physical.clone()],
                vec![Complex64::ONE, Complex64::ZERO],
            )
            .map_err(engine_error)?;
            let node = self
                .state
                .add_tensor(site.0, tensor)
                .map_err(engine_error)?;
            self.state
                .connect(previous_node, &bond, node, &bond)
                .map_err(engine_error)?;
        }
        self.physical_indices.push(physical);
        Ok(site)
    }

    fn apply_one(&mut self, site: SiteId, matrix: &Matrix2) -> Result<(), MpsError> {
        apply_one(&mut self.state, &self.physical_indices, site, matrix)
    }

    fn apply_adjacent_two(
        &mut self,
        first: SiteId,
        second: SiteId,
        matrix: &Matrix4,
    ) -> Result<(), MpsError> {
        if first.0.abs_diff(second.0) != 1 {
            return Err(MpsError::NonAdjacentOperands {
                first: first.0,
                second: second.0,
            });
        }
        apply_two(
            &mut self.state,
            &self.physical_indices,
            first,
            second,
            matrix,
            &self.factorize_options,
        )?;
        self.reached_bond_dimension = self
            .reached_bond_dimension
            .max(self.state.link_dims().into_iter().max().unwrap_or(1));
        Ok(())
    }

    fn probability_one(&mut self, site: SiteId) -> Result<f64, MpsError> {
        let norm_squared = self.state.norm_squared().map_err(engine_error)?;
        if norm_squared <= PROJECTION_TOLERANCE {
            return Err(MpsError::InternalInvariant(
                "cannot measure a zero-norm state".into(),
            ));
        }
        let mut projected = self.state.clone();
        apply_one(
            &mut projected,
            &self.physical_indices,
            site,
            &projector(Measurement::One),
        )?;
        projected.clear_canonical_region();
        Ok(projected.norm_squared().map_err(engine_error)? / norm_squared)
    }

    fn project_z(&mut self, site: SiteId, outcome: Measurement) -> Result<(), MpsError> {
        apply_one(
            &mut self.state,
            &self.physical_indices,
            site,
            &projector(outcome),
        )?;
        self.state.clear_canonical_region();
        let norm = self.state.norm().map_err(engine_error)?;
        if norm <= PROJECTION_TOLERANCE {
            return Err(MpsError::ZeroProbabilityProjection(outcome));
        }
        self.state
            .scale_mut(AnyScalar::new_real(norm.recip()))
            .map_err(engine_error)
    }

    fn expectation_pauli_product(&self, factors: &[SitePauli]) -> Result<Complex64, MpsError> {
        if self.physical_indices.is_empty() {
            return Ok(Complex64::ONE);
        }
        let mut transformed = self.state.clone();
        for factor in factors {
            let matrix = pauli_matrix(factor.pauli);
            apply_one(
                &mut transformed,
                &self.physical_indices,
                factor.site,
                &matrix,
            )?;
        }
        let inner = self.state.inner(&transformed).map_err(engine_error)?;
        Ok(Complex64::new(inner.real(), inner.imag()))
    }

    fn state_norm(&mut self) -> Result<f64, MpsError> {
        if self.physical_indices.is_empty() {
            Ok(1.0)
        } else {
            self.state.norm().map_err(engine_error)
        }
    }

    fn reached_bond_dimension(&self) -> usize {
        self.reached_bond_dimension
    }
}

fn descriptor() -> EngineDescriptor {
    EngineDescriptor {
        name: "tensor4all".into(),
        version: format!("0.2.0+{TENSOR4ALL_REVISION}"),
        backend: "tenferro-faer".into(),
        device: "cpu".into(),
    }
}

fn resolve_resources() -> Result<ResourceResolution, MpsError> {
    let configured_threads = env::var("RAYON_NUM_THREADS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|threads| *threads > 0);
    let threads = tensor4all_tensorbackend::with_default_backend(|backend| backend.num_threads());
    let max_cpu_threads = NonZeroUsize::new(threads).ok_or_else(|| {
        MpsError::InternalInvariant("tensor4all resolved a zero CPU thread budget".into())
    })?;
    let source = match (env::var("RAYON_NUM_THREADS"), configured_threads) {
        (Err(env::VarError::NotPresent), _) => ResourceResolutionSource::ProcessVisible,
        (Ok(_), Some(configured)) if configured == threads => ResourceResolutionSource::Environment,
        _ => ResourceResolutionSource::InvalidConfigurationFallback,
    };
    Ok(ResourceResolution {
        max_cpu_threads,
        source,
        caller_limit_honored: true,
    })
}

fn apply_one(
    state: &mut Mps,
    physical_indices: &[DynIndex],
    site: SiteId,
    matrix: &Matrix2,
) -> Result<(), MpsError> {
    let physical = physical_indices
        .get(site.0)
        .ok_or_else(|| MpsError::InternalInvariant(format!("missing physical site {}", site.0)))?;
    let node = state
        .node_index(&site.0)
        .ok_or_else(|| MpsError::InternalInvariant(format!("missing MPS node {}", site.0)))?;
    let site_tensor = state
        .tensor(node)
        .ok_or_else(|| MpsError::InternalInvariant(format!("missing MPS tensor {}", site.0)))?
        .clone();
    let output = DynIndex::new_dyn(2);
    let gate = matrix_tensor(vec![output.clone(), physical.clone()], matrix)?;
    let updated = contract(&[&gate, &site_tensor])
        .map_err(engine_error)?
        .replace_indices(&[output], std::slice::from_ref(physical))
        .map_err(engine_error)?;
    state
        .replace_tensor(node, updated)
        .map_err(engine_error)?
        .ok_or_else(|| MpsError::InternalInvariant(format!("missing MPS site {}", site.0)))?;
    Ok(())
}

fn apply_two(
    state: &mut Mps,
    physical_indices: &[DynIndex],
    first: SiteId,
    second: SiteId,
    matrix: &Matrix4,
    options: &FactorizeOptions,
) -> Result<(), MpsError> {
    state
        .canonicalize_mut([first.0, second.0], CanonicalizationOptions::default())
        .map_err(engine_error)?;
    let first_physical = physical_indices
        .get(first.0)
        .ok_or_else(|| MpsError::InternalInvariant(format!("missing physical site {}", first.0)))?;
    let second_physical = physical_indices.get(second.0).ok_or_else(|| {
        MpsError::InternalInvariant(format!("missing physical site {}", second.0))
    })?;
    let first_node = state
        .node_index(&first.0)
        .ok_or_else(|| MpsError::InternalInvariant(format!("missing MPS node {}", first.0)))?;
    let second_node = state
        .node_index(&second.0)
        .ok_or_else(|| MpsError::InternalInvariant(format!("missing MPS node {}", second.0)))?;
    let (edge, _) = state
        .edges_for_node(first_node)
        .into_iter()
        .find(|(_, neighbor)| *neighbor == second_node)
        .ok_or_else(|| MpsError::InternalInvariant("adjacent MPS sites are disconnected".into()))?;
    let old_bond = state
        .bond_index(edge)
        .ok_or_else(|| MpsError::InternalInvariant("missing MPS bond".into()))?
        .clone();
    let first_tensor = state
        .tensor(first_node)
        .ok_or_else(|| MpsError::InternalInvariant("missing first MPS tensor".into()))?
        .clone();
    let second_tensor = state
        .tensor(second_node)
        .ok_or_else(|| MpsError::InternalInvariant("missing second MPS tensor".into()))?
        .clone();
    let left_indices = first_tensor
        .external_indices()
        .into_iter()
        .filter(|index| index != &old_bond)
        .collect::<Vec<_>>();

    let first_output = DynIndex::new_dyn(2);
    let second_output = DynIndex::new_dyn(2);
    let gate = matrix_tensor(
        vec![
            first_output.clone(),
            second_output.clone(),
            first_physical.clone(),
            second_physical.clone(),
        ],
        matrix,
    )?;
    let combined = contract(&[&first_tensor, &second_tensor, &gate])
        .map_err(engine_error)?
        .replace_indices(
            &[first_output, second_output],
            &[first_physical.clone(), second_physical.clone()],
        )
        .map_err(engine_error)?;
    let factors = factorize(&combined, &left_indices, options).map_err(engine_error)?;
    let new_bond = factors.bond_index;
    state
        .replace_edge_bond(edge, new_bond.clone())
        .map_err(engine_error)?;
    state
        .replace_tensor(first_node, factors.left)
        .map_err(engine_error)?;
    state
        .replace_tensor(second_node, factors.right)
        .map_err(engine_error)?;
    state.set_ortho_towards(&new_bond, Some(second.0));
    state
        .set_canonical_region([second.0])
        .map_err(engine_error)?;
    Ok(())
}

fn matrix_tensor<const DIMENSION: usize>(
    indices: Vec<DynIndex>,
    matrix: &[[Complex64; DIMENSION]; DIMENSION],
) -> Result<IdxTensor, MpsError> {
    let data = (0..DIMENSION)
        .flat_map(|column| (0..DIMENSION).map(move |row| matrix[row][column]))
        .collect();
    IdxTensor::from_dense(indices, data).map_err(engine_error)
}

fn projector(outcome: Measurement) -> Matrix2 {
    match outcome {
        Measurement::Zero => [
            [Complex64::ONE, Complex64::ZERO],
            [Complex64::ZERO, Complex64::ZERO],
        ],
        Measurement::One => [
            [Complex64::ZERO, Complex64::ZERO],
            [Complex64::ZERO, Complex64::ONE],
        ],
    }
}

fn pauli_matrix(pauli: Pauli) -> Matrix2 {
    match pauli {
        Pauli::I => [
            [Complex64::ONE, Complex64::ZERO],
            [Complex64::ZERO, Complex64::ONE],
        ],
        Pauli::X => [
            [Complex64::ZERO, Complex64::ONE],
            [Complex64::ONE, Complex64::ZERO],
        ],
        Pauli::Y => [
            [Complex64::ZERO, -Complex64::I],
            [Complex64::I, Complex64::ZERO],
        ],
        Pauli::Z => [
            [Complex64::ONE, Complex64::ZERO],
            [Complex64::ZERO, -Complex64::ONE],
        ],
    }
}

fn engine_error(error: impl std::fmt::Display) -> MpsError {
    MpsError::EngineFailure(error.to_string())
}

#[cfg(test)]
mod tests;
