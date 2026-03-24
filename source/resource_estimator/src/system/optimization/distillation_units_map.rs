// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use std::cmp::max;

use std::rc::Rc;

use crate::estimates::LogicalPatch;
use crate::system::modeling::{
    PhysicalQubit, Protocol, TFactoryDistillationUnit, TFactoryDistillationUnitTemplate,
    TFactoryDistillationUnitType, TFactoryQubit,
};

pub struct DistillationUnitsMap<'a> {
    physical_distillation_units: Vec<TFactoryDistillationUnit<'a>>,
    is_valid_physical_distillation_units: Vec<bool>,
    logical_distillation_units: Vec<Vec<TFactoryDistillationUnit<'a>>>,
    num_combined_distillation_units: usize,
    num_physical_distillation_units: usize,
    num_logical_distillation_units: usize,
    min_valid_code_distance_indexes: Vec<usize>,
    num_code_distances: usize,
    distances: Vec<u64>,
}

impl<'a> DistillationUnitsMap<'a> {
    pub fn create(
        qubit: &PhysicalQubit,
        qubits: &[Option<Rc<LogicalPatch<Protocol>>>],
        distances: Vec<u64>,
        distillation_unit_templates: &'a [TFactoryDistillationUnitTemplate],
    ) -> Self {
        let num_code_distances = distances.len();
        let combined_distillation_unit_templates = Self::get_templates_for_unit_type(
            distillation_unit_templates,
            TFactoryDistillationUnitType::Combined,
        );
        let num_combined_distillation_units = combined_distillation_unit_templates.len();

        let mut purely_logical_distillation_unit_templates = Self::get_templates_for_unit_type(
            distillation_unit_templates,
            TFactoryDistillationUnitType::Logical,
        );
        let num_logical_distillation_units = purely_logical_distillation_unit_templates.len();

        let mut physical_distillation_units: Vec<TFactoryDistillationUnit> =
            combined_distillation_unit_templates
                .iter()
                .map(|x| TFactoryDistillationUnit::by_template(x, &TFactoryQubit::Physical(qubit)))
                .collect();

        let mut purely_physical_distillation_units: Vec<TFactoryDistillationUnit> =
            distillation_unit_templates
                .iter()
                .filter(|x| x.unit_type == TFactoryDistillationUnitType::Physical)
                .map(|x| TFactoryDistillationUnit::by_template(x, &TFactoryQubit::Physical(qubit)))
                // exclude purely invalid physical distillation units from scratch
                .filter(TFactoryDistillationUnit::is_valid)
                .collect();

        let num_physical_distillation_units = purely_physical_distillation_units.len();
        physical_distillation_units.append(&mut purely_physical_distillation_units);

        let mut is_valid_physical_distillation_units =
            vec![true; num_combined_distillation_units + num_physical_distillation_units];

        for idx in 0..num_combined_distillation_units {
            if !physical_distillation_units[idx].is_valid() {
                is_valid_physical_distillation_units[idx] = false;
            }
        }

        let mut logical_distillation_unit_templates = combined_distillation_unit_templates;
        logical_distillation_unit_templates.append(&mut purely_logical_distillation_unit_templates);

        let mut logical_distillation_units: Vec<Vec<TFactoryDistillationUnit>> = Vec::new();

        for qubit in qubits {
            if let Some(qubit) = qubit {
                logical_distillation_units.push(
                    logical_distillation_unit_templates
                        .iter()
                        .map(|x| {
                            TFactoryDistillationUnit::by_template(x, &TFactoryQubit::Logical(qubit))
                        })
                        .collect(),
                );
            } else {
                logical_distillation_units.push(Vec::new());
            }
        }

        let mut min_valid_code_distance_indexes =
            vec![qubits.len(); num_logical_distillation_units + num_combined_distillation_units];
        for idx in 0..num_logical_distillation_units + num_combined_distillation_units {
            if let Some(min_valid_code_distance_index) = logical_distillation_units
                .iter()
                .filter(|x| !x.is_empty())
                .enumerate()
                .filter_map(|(distance_index, x)| {
                    if x[idx].is_valid() {
                        Some(distance_index)
                    } else {
                        None
                    }
                })
                .min()
            {
                min_valid_code_distance_indexes[idx] = min_valid_code_distance_index;
            }
        }

        Self {
            physical_distillation_units,
            is_valid_physical_distillation_units,
            logical_distillation_units,
            num_combined_distillation_units,
            num_physical_distillation_units,
            num_logical_distillation_units,
            min_valid_code_distance_indexes,
            num_code_distances,
            distances,
        }
    }

    fn get_templates_for_unit_type(
        distillation_unit_templates: &[TFactoryDistillationUnitTemplate],
        unit_type: TFactoryDistillationUnitType,
    ) -> Vec<&TFactoryDistillationUnitTemplate> {
        distillation_unit_templates
            .iter()
            .filter(|x| x.unit_type == unit_type)
            .collect()
    }

    fn get(&self, position: usize, distance: u64, idx: usize) -> &TFactoryDistillationUnit<'_> {
        // physical: combined, purely physical
        // logical: combined, purely logical
        // enumeration: combined, purely logical, purely physical
        if distance == 1 && position == 0 {
            let index = if idx < self.num_combined_distillation_units {
                idx
            } else {
                idx - self.num_logical_distillation_units
            };

            &self.physical_distillation_units[index]
        } else {
            &self.logical_distillation_units[distance as usize][idx]
        }
    }

    pub fn get_min_distance_indexes(&self, indexes: &[usize]) -> Vec<usize> {
        indexes
            .iter()
            .enumerate()
            .scan(0, |state, (position, &idx)| {
                *state = max(*state, self.get_min_distance_index(position, idx));
                Some(*state)
            })
            .collect()
    }

    pub fn get_max_distance_indexes(&self, indexes: &[usize]) -> Vec<usize> {
        indexes
            .iter()
            .enumerate()
            .map(|(position, &idx)| self.get_max_distance_index(position, idx))
            .collect()
    }

    fn get_min_distance_index(&self, position: usize, idx: usize) -> usize {
        if position == 0 {
            if idx >= self.num_logical_distillation_units + self.num_combined_distillation_units {
                0
            } else if idx < self.num_combined_distillation_units {
                if self.is_valid_physical_distillation_units[idx] {
                    0
                } else {
                    max(1, self.min_valid_code_distance_indexes[idx])
                }
            } else {
                // skip code distance = 1 for logical units at first round of distillation
                max(1, self.min_valid_code_distance_indexes[idx])
            }
        } else if idx >= self.num_logical_distillation_units + self.num_combined_distillation_units
        {
            panic!("Invalid position for physical unit: {position}")
        } else {
            self.min_valid_code_distance_indexes[idx]
        }
    }

    fn get_max_distance_index(&self, position: usize, idx: usize) -> usize {
        if idx >= self.num_logical_distillation_units + self.num_combined_distillation_units {
            if position == 0 {
                0
            } else {
                panic!("Invalid position for physical unit: {position}")
            }
        } else {
            self.num_code_distances - 1
        }
    }

    pub fn get_many(
        &self,
        distance_indexes: &[usize],
        indexes: &[usize],
    ) -> Vec<&TFactoryDistillationUnit<'_>> {
        indexes
            .iter()
            .zip(distance_indexes)
            .enumerate()
            .map(|(position, (&idx, &distance_index))| {
                self.get(position, self.distances[distance_index], idx)
            })
            .collect()
    }

    pub fn iterate_for_all_distillation_units<F>(&self, num_rounds: usize, action: &mut F)
    where
        F: FnMut(&[usize]),
    {
        let mut a: Vec<usize> = vec![0; num_rounds + 1];
        loop {
            action(&a[1..]);
            let mut j = num_rounds;
            while a[j]
                == if j == 0 {
                    1
                } else if j == 1 {
                    self.num_logical_distillation_units
                        + self.num_combined_distillation_units
                        + self.num_physical_distillation_units
                        - 1
                } else {
                    self.num_logical_distillation_units + self.num_combined_distillation_units - 1
                }
            {
                a[j] = 0;
                j -= 1;
            }

            if j == 0 {
                break;
            }

            a[j] += 1;
        }
    }
}
