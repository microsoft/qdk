// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{
    fmt::Display,
    ops::{Add, Deref, Index},
    sync::Arc,
};

use num_traits::FromPrimitive;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

#[derive(Default, Clone)]
pub struct ISA {
    instructions: FxHashMap<u64, Instruction>,
}

impl ISA {
    #[must_use]
    pub fn new() -> Self {
        ISA {
            instructions: FxHashMap::default(),
        }
    }

    pub fn add_instruction(&mut self, instruction: Instruction) {
        self.instructions.insert(instruction.id, instruction);
    }

    #[must_use]
    pub fn get(&self, id: &u64) -> Option<&Instruction> {
        self.instructions.get(id)
    }

    #[must_use]
    pub fn contains(&self, id: &u64) -> bool {
        self.instructions.contains_key(id)
    }

    #[must_use]
    pub fn satisfies(&self, requirements: &ISARequirements) -> bool {
        for constraint in requirements.constraints.values() {
            let Some(instruction) = self.instructions.get(&constraint.id) else {
                return false;
            };

            if instruction.encoding != constraint.encoding {
                return false;
            }

            match &instruction.metrics {
                Metrics::FixedArity {
                    arity, error_rate, ..
                } => {
                    // Constraint requires variable arity for this instruction
                    let Some(constraint_arity) = constraint.arity else {
                        return false;
                    };

                    // Arity must match
                    if *arity != constraint_arity {
                        return false;
                    }

                    // Error rate constraint must be satisfied
                    if let Some(ref bound) = constraint.error_rate_fn
                        && !bound.evaluate(error_rate)
                    {
                        return false;
                    }
                }

                Metrics::VariableArity { error_rate_fn, .. } => {
                    // If an arity and error rate constraint is specified, it
                    // must be satisfied
                    if let (Some(constraint_arity), Some(ref bound)) =
                        (constraint.arity, constraint.error_rate_fn)
                        && !bound.evaluate(&error_rate_fn.evaluate(constraint_arity))
                    {
                        return false;
                    }
                }
            }

            // Check that all required properties are present in the instruction
            for prop in &constraint.properties {
                if !instruction.has_property(prop) {
                    return false;
                }
            }
        }
        true
    }
}

impl Deref for ISA {
    type Target = FxHashMap<u64, Instruction>;

    fn deref(&self) -> &Self::Target {
        &self.instructions
    }
}

impl FromIterator<Instruction> for ISA {
    fn from_iter<T: IntoIterator<Item = Instruction>>(iter: T) -> Self {
        let mut isa = ISA::new();
        for instruction in iter {
            isa.add_instruction(instruction);
        }
        isa
    }
}

impl Display for ISA {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for instruction in self.instructions.values() {
            writeln!(f, "{instruction}")?;
        }
        Ok(())
    }
}

impl Index<u64> for ISA {
    type Output = Instruction;

    fn index(&self, index: u64) -> &Self::Output {
        &self.instructions[&index]
    }
}

impl Add<ISA> for ISA {
    type Output = ISA;

    fn add(self, other: ISA) -> ISA {
        let mut combined = self;
        for instruction in other.instructions.into_values() {
            combined.add_instruction(instruction);
        }
        combined
    }
}

#[derive(Default)]
pub struct ISARequirements {
    constraints: FxHashMap<u64, InstructionConstraint>,
}

impl ISARequirements {
    #[must_use]
    pub fn new() -> Self {
        ISARequirements {
            constraints: FxHashMap::default(),
        }
    }

    pub fn add_constraint(&mut self, constraint: InstructionConstraint) {
        self.constraints.insert(constraint.id, constraint);
    }
}

impl FromIterator<InstructionConstraint> for ISARequirements {
    fn from_iter<T: IntoIterator<Item = InstructionConstraint>>(iter: T) -> Self {
        let mut reqs = ISARequirements::new();
        for constraint in iter {
            reqs.add_constraint(constraint);
        }
        reqs
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Instruction {
    id: u64,
    encoding: Encoding,
    metrics: Metrics,
    properties: Option<FxHashMap<u64, u64>>,
}

impl Instruction {
    #[must_use]
    pub fn fixed_arity(
        id: u64,
        encoding: Encoding,
        arity: u64,
        time: u64,
        space: Option<u64>,
        length: Option<u64>,
        error_rate: f64,
    ) -> Self {
        let length = length.unwrap_or(arity);
        let space = space.unwrap_or(length);

        Instruction {
            id,
            encoding,
            metrics: Metrics::FixedArity {
                arity,
                length,
                space,
                time,
                error_rate,
            },
            properties: None,
        }
    }

    #[must_use]
    pub fn variable_arity(
        id: u64,
        encoding: Encoding,
        time_fn: VariableArityFunction<u64>,
        space_fn: VariableArityFunction<u64>,
        length_fn: Option<VariableArityFunction<u64>>,
        error_rate_fn: VariableArityFunction<f64>,
    ) -> Self {
        let length_fn = length_fn.unwrap_or_else(|| space_fn.clone());

        Instruction {
            id,
            encoding,
            metrics: Metrics::VariableArity {
                length_fn,
                space_fn,
                time_fn,
                error_rate_fn,
            },
            properties: None,
        }
    }

    #[must_use]
    pub fn with_id(&self, id: u64) -> Self {
        let mut new_instruction = self.clone();
        new_instruction.id = id;
        new_instruction
    }

    #[must_use]
    pub fn id(&self) -> u64 {
        self.id
    }

    #[must_use]
    pub fn encoding(&self) -> Encoding {
        self.encoding
    }

    #[must_use]
    pub fn arity(&self) -> Option<u64> {
        match &self.metrics {
            Metrics::FixedArity { arity, .. } => Some(*arity),
            Metrics::VariableArity { .. } => None,
        }
    }

    #[must_use]
    pub fn space(&self, arity: Option<u64>) -> Option<u64> {
        match &self.metrics {
            Metrics::FixedArity { space, .. } => Some(*space),
            Metrics::VariableArity { space_fn, .. } => arity.map(|a| space_fn.evaluate(a)),
        }
    }

    #[must_use]
    pub fn length(&self, arity: Option<u64>) -> Option<u64> {
        match &self.metrics {
            Metrics::FixedArity { length, .. } => Some(*length),
            Metrics::VariableArity { length_fn, .. } => arity.map(|a| length_fn.evaluate(a)),
        }
    }

    #[must_use]
    pub fn time(&self, arity: Option<u64>) -> Option<u64> {
        match &self.metrics {
            Metrics::FixedArity { time, .. } => Some(*time),
            Metrics::VariableArity { time_fn, .. } => arity.map(|a| time_fn.evaluate(a)),
        }
    }

    #[must_use]
    pub fn error_rate(&self, arity: Option<u64>) -> Option<f64> {
        match &self.metrics {
            Metrics::FixedArity { error_rate, .. } => Some(*error_rate),
            Metrics::VariableArity { error_rate_fn, .. } => {
                arity.map(|a| error_rate_fn.evaluate(a))
            }
        }
    }

    #[must_use]
    pub fn expect_space(&self, arity: Option<u64>) -> u64 {
        self.space(arity)
            .expect("Instruction does not support variable arity")
    }

    #[must_use]
    pub fn expect_length(&self, arity: Option<u64>) -> u64 {
        self.length(arity)
            .expect("Instruction does not support variable arity")
    }

    #[must_use]
    pub fn expect_time(&self, arity: Option<u64>) -> u64 {
        self.time(arity)
            .expect("Instruction does not support variable arity")
    }

    #[must_use]
    pub fn expect_error_rate(&self, arity: Option<u64>) -> f64 {
        self.error_rate(arity)
            .expect("Instruction does not support variable arity")
    }

    pub fn set_property(&mut self, key: u64, value: u64) {
        if let Some(ref mut properties) = self.properties {
            properties.insert(key, value);
        } else {
            let mut properties = FxHashMap::default();
            properties.insert(key, value);
            self.properties = Some(properties);
        }
    }

    #[must_use]
    pub fn get_property(&self, key: &u64) -> Option<u64> {
        self.properties.as_ref()?.get(key).copied()
    }

    #[must_use]
    pub fn has_property(&self, key: &u64) -> bool {
        self.properties
            .as_ref()
            .is_some_and(|props| props.contains_key(key))
    }

    #[must_use]
    pub fn get_property_or(&self, key: &u64, default: u64) -> u64 {
        self.get_property(key).unwrap_or(default)
    }
}

impl Display for Instruction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.metrics {
            Metrics::FixedArity { arity, .. } => {
                write!(f, "{} |{:?}| arity: {arity}", self.id, self.encoding)
            }
            Metrics::VariableArity { .. } => write!(f, "{} |{:?}|", self.id, self.encoding),
        }
    }
}

#[derive(Clone)]
pub struct InstructionConstraint {
    id: u64,
    encoding: Encoding,
    arity: Option<u64>,
    error_rate_fn: Option<ConstraintBound<f64>>,
    properties: FxHashSet<u64>,
}

impl InstructionConstraint {
    #[must_use]
    pub fn new(
        id: u64,
        encoding: Encoding,
        arity: Option<u64>,
        error_rate_fn: Option<ConstraintBound<f64>>,
    ) -> Self {
        InstructionConstraint {
            id,
            encoding,
            arity,
            error_rate_fn,
            properties: FxHashSet::default(),
        }
    }

    /// Adds a property requirement to the constraint.
    pub fn add_property(&mut self, property: u64) {
        self.properties.insert(property);
    }

    /// Checks if the constraint requires a specific property.
    #[must_use]
    pub fn has_property(&self, property: &u64) -> bool {
        self.properties.contains(property)
    }

    /// Returns the set of required properties.
    #[must_use]
    pub fn properties(&self) -> &FxHashSet<u64> {
        &self.properties
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Encoding {
    #[default]
    Physical,
    Logical,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum Metrics {
    FixedArity {
        arity: u64,
        length: u64,
        space: u64,
        time: u64,
        error_rate: f64,
    },
    VariableArity {
        length_fn: VariableArityFunction<u64>,
        space_fn: VariableArityFunction<u64>,
        time_fn: VariableArityFunction<u64>,
        error_rate_fn: VariableArityFunction<f64>,
    },
}

#[derive(Clone, Serialize, Deserialize)]
pub enum VariableArityFunction<T> {
    Constant {
        value: T,
    },
    Linear {
        slope: T,
    },
    BlockLinear {
        block_size: u64,
        slope: T,
    },
    #[serde(skip)]
    Generic {
        func: Arc<dyn Fn(u64) -> T + Send + Sync>,
    },
}

impl<T: Add<Output = T> + std::ops::Mul<Output = T> + Copy + FromPrimitive>
    VariableArityFunction<T>
{
    pub fn constant(value: T) -> Self {
        VariableArityFunction::Constant { value }
    }

    pub fn linear(slope: T) -> Self {
        VariableArityFunction::Linear { slope }
    }

    pub fn block_linear(block_size: u64, slope: T) -> Self {
        VariableArityFunction::BlockLinear { block_size, slope }
    }

    pub fn generic(func: impl Fn(u64) -> T + Send + Sync + 'static) -> Self {
        VariableArityFunction::Generic {
            func: Arc::new(func),
        }
    }

    pub fn generic_from_arc(func: Arc<dyn Fn(u64) -> T + Send + Sync>) -> Self {
        VariableArityFunction::Generic { func }
    }

    pub fn evaluate(&self, arity: u64) -> T {
        match self {
            VariableArityFunction::Constant { value } => *value,
            VariableArityFunction::Linear { slope } => {
                *slope * T::from_u64(arity).expect("Failed to convert u64 to target type")
            }
            VariableArityFunction::BlockLinear { block_size, slope } => {
                let blocks = arity.div_ceil(*block_size);
                *slope * T::from_u64(blocks).expect("Failed to convert u64 to target type")
            }
            VariableArityFunction::Generic { func } => func(arity),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConstraintBound<T> {
    LessThan(T),
    LessEqual(T),
    Equal(T),
    GreaterThan(T),
    GreaterEqual(T),
}

impl<T: PartialOrd + PartialEq> ConstraintBound<T> {
    pub fn less_than(value: T) -> Self {
        ConstraintBound::LessThan(value)
    }

    pub fn less_equal(value: T) -> Self {
        ConstraintBound::LessEqual(value)
    }

    pub fn equal(value: T) -> Self {
        ConstraintBound::Equal(value)
    }

    pub fn greater_than(value: T) -> Self {
        ConstraintBound::GreaterThan(value)
    }

    pub fn greater_equal(value: T) -> Self {
        ConstraintBound::GreaterEqual(value)
    }

    pub fn evaluate(&self, other: &T) -> bool {
        match self {
            ConstraintBound::LessThan(v) => other < v,
            ConstraintBound::LessEqual(v) => other <= v,
            ConstraintBound::Equal(v) => other == v,
            ConstraintBound::GreaterThan(v) => other > v,
            ConstraintBound::GreaterEqual(v) => other >= v,
        }
    }
}
