// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

mod memory_compute;

use num_bigint::BigUint;
use num_complex::Complex;
use qsc::{Backend, interpret::Value};
use rand::{Rng, SeedableRng, rngs::StdRng};
use rustc_hash::FxHashMap;
use std::{array, cell::RefCell, f64::consts::PI, fmt::Debug, iter::Sum};

use crate::{counts::memory_compute::CachingStrategy, system::LogicalResourceCounts};
use memory_compute::MemoryComputeInfo;

/// Resource counter implementation
///
/// This counter tracks all resources while executing a QIR program.  It takes
/// care of qubit management, gate counting, and depth calculation.
pub struct LogicalCounter {
    /// Stack of free qubits
    free_list: Vec<usize>,
    /// Next free qubit id, in case `free_list` is empty
    next_free: usize,
    /// Depth counter
    max_layer: Vec<usize>,
    /// Layers
    layers: Vec<LayerInfo>,
    /// T-count (excluded in rotation count)
    t_count: usize,
    /// Number of Z rotation gates (excluding Cliffords and T gates)
    r_count: usize,
    /// CCZ count (does not contribute to T count)
    ccz_count: usize,
    /// Number of single-qubit and multiple-qubit measurements
    m_count: usize,
    /// Global allocation barrier (when calling global barrier this is advanced
    /// to allocate new qubits after the barrier)
    allocation_barrier: usize,
    /// Caching stack
    caching_stack: Vec<String>,
    /// Caching
    caching_layers: FxHashMap<String, LayerCache>,
    /// Repeating
    repeats: Vec<RepeatEntry>,
    /// Memory/Compute architecture
    memory_compute: Option<MemoryComputeInfo>,
    /// Random number generator
    rnd: RefCell<StdRng>,
}

impl Default for LogicalCounter {
    fn default() -> Self {
        Self {
            free_list: vec![],
            next_free: 0,
            max_layer: vec![],
            layers: vec![],
            t_count: 0,
            r_count: 0,
            ccz_count: 0,
            m_count: 0,
            allocation_barrier: 0,
            caching_stack: vec![],
            caching_layers: FxHashMap::default(),
            repeats: vec![],
            memory_compute: None,
            rnd: RefCell::new(StdRng::seed_from_u64(0)),
        }
    }
}

impl LogicalCounter {
    #[must_use]
    pub fn logical_resources(&self) -> LogicalResourceCounts {
        let (num_compute_qubits, read_from_memory_count, write_to_memory_count) =
            if let Some(memory_compute) = &self.memory_compute {
                (
                    Some(memory_compute.compute_size() as u64),
                    Some(memory_compute.read_from_memory_count() as u64),
                    Some(memory_compute.write_to_memory_count() as u64),
                )
            } else {
                (None, None, None)
            };

        LogicalResourceCounts {
            num_qubits: self.next_free as _,
            t_count: self.t_count as _,
            rotation_count: self.r_count as _,
            rotation_depth: self.layers.iter().filter(|layer| layer.r != 0).count() as _,
            ccz_count: self.ccz_count as _,
            ccix_count: 0,
            measurement_count: self.m_count as _,
            num_compute_qubits,
            read_from_memory_count,
            write_to_memory_count,
        }
    }

    fn schedule_r(&mut self, q: usize) {
        let level = self.level_at(q);

        if level == self.layers.len() {
            self.layers.push(LayerInfo::new_with_r());
        } else {
            self.layers[level].r += 1;
        }

        self.max_layer[q] += 1;
    }

    fn schedule_t(&mut self, q: usize) {
        let level = self.level_at(q);

        if level == self.layers.len() {
            self.layers.push(LayerInfo::new_with_t());
        } else {
            self.layers[level].t += 1;
        }

        self.max_layer[q] += 1;
    }

    fn schedule_ccz(&mut self, q1: usize, q2: usize, q3: usize) {
        let d1 = self.level_at(q1);
        let d2 = self.level_at(q2);
        let d3 = self.level_at(q3);
        let max_depth = d1.max(d2).max(d3);

        if max_depth == self.layers.len() {
            self.layers.push(LayerInfo::new_with_ccz());
        } else {
            self.layers[max_depth].ccz += 1;
        }

        self.max_layer[q1] = max_depth + 1;
        self.max_layer[q2] = max_depth + 1;
        self.max_layer[q3] = max_depth + 1;
    }

    fn schedule_two_qubit_clifford(&mut self, q1: usize, q2: usize) {
        let d1 = self.level_at(q1);
        let d2 = self.level_at(q2);
        let max_depth = d1.max(d2);
        self.max_layer[q1] = max_depth;
        self.max_layer[q2] = max_depth;
    }

    fn level_at(&mut self, q: usize) -> usize {
        while self.max_layer.len() <= q {
            self.qubit_allocate();
        }

        self.max_layer[q]
    }

    fn global_barrier(&mut self) -> usize {
        let depth = self.layers.len();

        for layer in &mut self.max_layer {
            *layer = depth;
        }

        self.allocation_barrier = depth;
        depth
    }

    fn begin_caching(&mut self, name: &str, variant: i64) -> bool {
        let label = format!("{name}-{variant}");

        if let Some(LayerCache::End {
            start_depth,
            end_depth,
            combined_layer,
            m_count,
            wtm_count,
            rfm_count,
        }) = self.caching_layers.get(&label)
        {
            self.layers.extend_from_within(*start_depth..*end_depth);

            self.t_count += combined_layer.t;
            self.r_count += combined_layer.r;
            self.ccz_count += combined_layer.ccz;
            self.m_count += *m_count;
            if let Some(memory_compute) = &mut self.memory_compute {
                memory_compute.increase_write_to_memory_count(*wtm_count);
                memory_compute.increase_read_from_memory_count(*rfm_count);
            }

            false
        } else {
            let depth = self.global_barrier();

            self.caching_layers.insert(
                label.clone(),
                LayerCache::Begin {
                    start_depth: depth,
                    m_count: self.m_count,
                    wtm_count: self.wtm_count(),
                    rfm_count: self.rfm_count(),
                },
            );
            self.caching_stack.push(label);

            true
        }
    }

    fn end_caching(&mut self) -> Result<(), String> {
        let Some(label) = self.caching_stack.pop() else {
            return Err("cannot end caching before beginning caching".to_string());
        };

        let entry = self
            .caching_layers
            .remove(&label)
            .expect("layer caching should always have matching begin and end");

        let LayerCache::Begin {
            start_depth,
            m_count,
            wtm_count,
            rfm_count,
        } = entry
        else {
            panic!("layer caching should always have matching begin and end");
        };

        let end_depth = self.layers.len();

        let range = &self.layers[start_depth..end_depth];
        let sum: LayerInfo = range.iter().sum();

        self.caching_layers.insert(
            label,
            LayerCache::End {
                start_depth,
                end_depth,
                combined_layer: sum,
                m_count: self.m_count - m_count,
                wtm_count: self.wtm_count() - wtm_count,
                rfm_count: self.rfm_count() - rfm_count,
            },
        );

        self.global_barrier();
        Ok(())
    }

    pub fn begin_repeat(&mut self, count: i64) -> Result<(), String> {
        let start_depth = self.global_barrier();

        self.repeats.push(RepeatEntry {
            count: count
                .try_into()
                .map_err(|_| format!("Estimate count {count} is too large to fit in a usize.",))?,
            start_depth,
            m_count: self.m_count,
            wtm_count: self.wtm_count(),
            rfm_count: self.rfm_count(),
        });

        Ok(())
    }

    #[allow(clippy::similar_names)]
    pub fn end_repeat(&mut self) {
        if let Some(RepeatEntry {
            count,
            start_depth,
            m_count,
            wtm_count,
            rfm_count,
        }) = self.repeats.pop()
        {
            if count == 0 {
                return;
            }

            let end_depth = self.global_barrier();

            let range = &self.layers[start_depth..end_depth];
            let sum: LayerInfo = range.iter().sum();

            // We skip one iteration, which was already done explicitly between
            // begin_repeat and end_repeat
            let r_depth = range.iter().filter(|l| l.r != 0).count();
            let combined_r_depth = r_depth * (count - 1);
            let combined_t_count = sum.t * (count - 1);
            let combined_r_count = sum.r * (count - 1);
            let combined_ccz_count = sum.ccz * (count - 1);
            let combined_m_count = (self.m_count - m_count) * (count - 1);

            if r_depth > 0 {
                let first_layer_r_count = combined_r_count - (combined_r_depth - 1);

                self.layers.push(LayerInfo {
                    ccz: combined_ccz_count,
                    r: first_layer_r_count,
                    t: combined_t_count,
                });
                for _ in 1..combined_r_depth {
                    self.layers.push(LayerInfo::new_with_r());
                }
            } else {
                self.layers.push(LayerInfo {
                    ccz: combined_ccz_count,
                    r: combined_r_count,
                    t: combined_t_count,
                });
            }

            self.t_count += combined_t_count;
            self.r_count += combined_r_count;
            self.ccz_count += combined_ccz_count;
            self.m_count += combined_m_count;

            if let Some(memory_compute) = &mut self.memory_compute {
                memory_compute.increase_write_to_memory_count(
                    (memory_compute.write_to_memory_count() - wtm_count) * (count - 1),
                );
                memory_compute.increase_read_from_memory_count(
                    (memory_compute.read_from_memory_count() - rfm_count) * (count - 1),
                );
            }

            self.global_barrier();
        }
    }

    fn add_estimate(
        &mut self,
        estimates: &[(i64, i64)],
        layout: i64,
        qubits: &[usize],
    ) -> Result<(), String> {
        if layout != 1 {
            return Err(
                "Parameter layout in AccountForEstimates must be 1 for PSSPCLayout.".to_string(),
            );
        }

        let mut aux_qubit_count = 0_usize;
        let mut t_count = 0_usize;
        let mut r_count = 0_usize;
        let mut r_depth = 0_usize;
        let mut ccz_count = 0_usize;
        let mut m_count = 0_usize;
        for (kind, count) in estimates {
            if *count < 0 {
                return Err(format!("Negative estimate count: {count}"));
            }
            let count: usize = (*count)
                .try_into()
                .map_err(|_| format!("Estimate count {count} is too large to fit in a usize.",))?;
            match *kind {
                0 => aux_qubit_count += count,
                1 => t_count += count,
                2 => r_count += count,
                3 => r_depth += count,
                4 => ccz_count += count,
                5 => m_count += count,
                _ => return Err(format!("Unknown estimate kind: {kind}")),
            }
        }

        // Allocate helper qubits
        let helper_qubits = (0..aux_qubit_count)
            .map(|_| self.qubit_allocate())
            .collect::<Vec<_>>();

        // Set barrier among all qubits
        let all_qubits = qubits.iter().chain(helper_qubits.iter());
        let max_depth = all_qubits
            .clone()
            .map(|q| self.max_layer[*q])
            .max()
            .unwrap_or(0);
        for qubit in all_qubits {
            self.max_layer[*qubit] = max_depth;
        }

        // Add up the estimates, dividing up between layers if appropriate.
        let num_layers = if r_depth == 0 {
            if r_count != 0 {
                return Err("Rotation depth of zero must use rotation count zero.".to_string());
            }

            self.layers.push(LayerInfo {
                t: t_count,
                r: r_count,
                ccz: ccz_count,
            });

            1
        } else {
            if r_depth < (r_count as f64 / qubits.len() as f64).ceil() as usize {
                return Err(format!(
                    "Rotation depth {r_depth} is too small for rotation count {r_count} and {} qubits.",
                    qubits.len()
                ));
            }

            let r_count_per_layer = r_count / r_depth;
            let extra_count = r_count - (r_count_per_layer * r_depth);

            self.layers.push(LayerInfo {
                t: t_count,
                r: r_count_per_layer + extra_count,
                ccz: ccz_count,
            });

            for _ in 1..r_depth {
                self.layers.push(LayerInfo {
                    t: 0,
                    r: r_count_per_layer,
                    ccz: 0,
                });
            }

            r_depth
        };

        self.t_count += t_count;
        self.r_count += r_count;
        self.ccz_count += ccz_count;
        self.m_count += m_count;

        for qubit in qubits {
            self.max_layer[*qubit] += num_layers;
        }

        // Release helper qubits
        for qubit in helper_qubits {
            self.qubit_release(qubit);
        }

        Ok(())
    }

    fn enable_memory_compute(&mut self, compute_capacity: i64, strategy: i64) {
        let compute_capacity: usize = compute_capacity
            .try_into()
            .expect("compute capacity is too large to fit in a usize");
        if self.memory_compute.is_none() {
            self.memory_compute = Some(MemoryComputeInfo::new(if strategy == 0 {
                CachingStrategy::least_recently_used(compute_capacity)
            } else {
                CachingStrategy::least_frequently_used(compute_capacity)
            }));
        }
    }

    fn assert_compute_qubits(&mut self, qubits: impl IntoIterator<Item = usize>) {
        if let Some(memory_compute) = &mut self.memory_compute {
            memory_compute.assert_compute_qubits(qubits);
        }
    }

    fn wtm_count(&self) -> usize {
        self.memory_compute
            .as_ref()
            .map_or(0, memory_compute::MemoryComputeInfo::write_to_memory_count)
    }

    fn rfm_count(&self) -> usize {
        self.memory_compute
            .as_ref()
            .map_or(0, memory_compute::MemoryComputeInfo::read_from_memory_count)
    }
}

impl Backend for LogicalCounter {
    type ResultType = bool;

    fn ccx(&mut self, ctl0: usize, ctl1: usize, q: usize) {
        self.assert_compute_qubits([ctl0, ctl1, q]);

        self.ccz_count += 1;
        self.schedule_ccz(ctl0, ctl1, q);
    }

    fn cx(&mut self, ctl: usize, q: usize) {
        self.assert_compute_qubits([ctl, q]);

        self.schedule_two_qubit_clifford(ctl, q);
    }

    fn cy(&mut self, ctl: usize, q: usize) {
        self.assert_compute_qubits([ctl, q]);

        self.schedule_two_qubit_clifford(ctl, q);
    }

    fn cz(&mut self, ctl: usize, q: usize) {
        self.assert_compute_qubits([ctl, q]);

        self.schedule_two_qubit_clifford(ctl, q);
    }

    fn h(&mut self, q: usize) {
        self.assert_compute_qubits([q]);
    }

    fn m(&mut self, q: usize) -> Self::ResultType {
        self.assert_compute_qubits([q]);

        self.m_count += 1;

        self.rnd.borrow_mut().gen_bool(0.5)
    }

    fn mresetz(&mut self, q: usize) -> Self::ResultType {
        self.m(q)
    }

    fn reset(&mut self, _q: usize) {}

    fn rx(&mut self, theta: f64, q: usize) {
        self.rz(theta, q);
    }

    fn rxx(&mut self, theta: f64, q0: usize, q1: usize) {
        self.rzz(theta, q0, q1);
    }

    fn ry(&mut self, theta: f64, q: usize) {
        self.rz(theta, q);
    }

    fn ryy(&mut self, theta: f64, q0: usize, q1: usize) {
        self.rzz(theta, q0, q1);
    }

    fn rz(&mut self, theta: f64, q: usize) {
        self.assert_compute_qubits([q]);

        let multiple = (theta / (PI / 4.0)).round();
        if ((multiple * (PI / 4.0)) - theta).abs() <= f64::EPSILON {
            let multiple = (multiple as i64).rem_euclid(8) as u64;
            if multiple & 1 == 1 {
                self.t(q);
            }
        } else {
            self.r_count += 1;
            self.schedule_r(q);
        }
    }

    fn rzz(&mut self, theta: f64, q0: usize, q1: usize) {
        self.cx(q1, q0);
        self.rz(theta, q0);
        self.cx(q1, q0);
    }

    fn sadj(&mut self, q: usize) {
        self.assert_compute_qubits([q]);
    }

    fn s(&mut self, q: usize) {
        self.assert_compute_qubits([q]);
    }

    fn sx(&mut self, q: usize) {
        self.assert_compute_qubits([q]);
    }

    fn swap(&mut self, q0: usize, q1: usize) {
        self.assert_compute_qubits([q0, q1]);
        self.schedule_two_qubit_clifford(q0, q1);
    }

    fn tadj(&mut self, q: usize) {
        self.assert_compute_qubits([q]);

        self.t_count += 1;
        self.schedule_t(q);
    }

    fn t(&mut self, q: usize) {
        self.assert_compute_qubits([q]);

        self.t_count += 1;
        self.schedule_t(q);
    }

    fn x(&mut self, _q: usize) {}

    fn y(&mut self, _q: usize) {}

    fn z(&mut self, _q: usize) {}

    fn qubit_allocate(&mut self) -> usize {
        if let Some(index) = self.free_list.pop() {
            index
        } else {
            let index = self.next_free;
            self.next_free += 1;
            self.max_layer.push(self.allocation_barrier);
            index
        }
    }

    fn qubit_release(&mut self, q: usize) -> bool {
        self.free_list.push(q);
        true
    }

    fn qubit_swap_id(&mut self, _q0: usize, _q1: usize) {
        // This can safely be treated as a no-op, because counts don't care which qubit is operated on,
        // just how many operations are performed, and relabeling is non-physical.
    }

    fn capture_quantum_state(&mut self) -> (Vec<(BigUint, Complex<f64>)>, usize) {
        (Vec::new(), 0)
    }

    fn qubit_is_zero(&mut self, _q: usize) -> bool {
        true
    }

    fn custom_intrinsic(&mut self, name: &str, arg: Value) -> Option<Result<Value, String>> {
        match name {
            "BeginEstimateCaching" => {
                let values = arg.unwrap_tuple();
                let [cache_name, cache_variant] = array::from_fn(|i| values[i].clone());
                Some(Ok(Value::Bool(self.begin_caching(
                    &cache_name.unwrap_string(),
                    cache_variant.unwrap_int(),
                ))))
            }
            "EndEstimateCaching" => Some(self.end_caching().map(|()| Value::unit())),
            "BeginRepeatEstimatesInternal" => {
                let count = arg.unwrap_int();
                Some(self.begin_repeat(count).map(|()| Value::unit()))
            }
            "EndRepeatEstimatesInternal" => {
                self.end_repeat();
                Some(Ok(Value::unit()))
            }
            "AccountForEstimatesInternal" => {
                let values = arg.unwrap_tuple();
                let [estimates, layout, qubits] = array::from_fn(|i| values[i].clone());
                let estimates = estimates
                    .unwrap_array()
                    .iter()
                    .map(|v| {
                        let entry = v.clone().unwrap_tuple();
                        let [variant, count] = array::from_fn(|i| entry[i].clone());
                        let variant = variant.unwrap_int();
                        let count = count.unwrap_int();
                        (variant, count)
                    })
                    .collect::<Vec<_>>();
                let layout = layout.unwrap_int();
                let qubits = qubits
                    .unwrap_array()
                    .iter()
                    .map(|v| v.clone().unwrap_qubit().deref().0)
                    .collect::<Vec<_>>();
                Some(
                    self.add_estimate(&estimates, layout, &qubits)
                        .map(|()| Value::unit()),
                )
            }
            "EnableMemoryComputeArchitecture" => {
                let values = arg.unwrap_tuple();
                let [compute_capacity, strategy] = array::from_fn(|i| values[i].clone());
                let compute_capacity = compute_capacity.unwrap_int();
                let strategy = strategy.unwrap_int();
                self.enable_memory_compute(compute_capacity, strategy);
                Some(Ok(Value::unit()))
            }
            "GlobalPhase" | "ConfigurePauliNoise" | "ConfigureQubitLoss" | "ApplyIdleNoise" => {
                Some(Ok(Value::unit()))
            }
            _ => None,
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct LayerInfo {
    t: usize,
    r: usize,
    ccz: usize,
}

impl LayerInfo {
    #[must_use]
    pub fn new_with_t() -> Self {
        Self { t: 1, r: 0, ccz: 0 }
    }

    #[must_use]
    pub fn new_with_r() -> Self {
        Self { t: 0, r: 1, ccz: 0 }
    }

    #[must_use]
    pub fn new_with_ccz() -> Self {
        Self { t: 0, r: 0, ccz: 1 }
    }
}

impl<'a> Sum<&'a LayerInfo> for LayerInfo {
    fn sum<I: Iterator<Item = &'a LayerInfo>>(iter: I) -> Self {
        let mut layer = LayerInfo::default();

        for current in iter {
            layer.t += current.t;
            layer.r += current.r;
            layer.ccz += current.ccz;
        }

        layer
    }
}

enum LayerCache {
    Begin {
        start_depth: usize,
        m_count: usize,
        wtm_count: usize,
        rfm_count: usize,
    },
    End {
        start_depth: usize,
        end_depth: usize,
        combined_layer: LayerInfo,
        m_count: usize,
        wtm_count: usize,
        rfm_count: usize,
    },
}

struct RepeatEntry {
    count: usize,
    start_depth: usize,
    m_count: usize,
    wtm_count: usize,
    rfm_count: usize,
}
