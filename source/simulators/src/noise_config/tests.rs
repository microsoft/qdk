// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::noise_config::{self, CorrelatedNoiseSampler, uq1_63};

#[derive(Debug, Clone, PartialEq)]
enum Fault {
    None,
    Value(u64),
}

impl noise_config::Fault for Fault {
    fn none() -> Self {
        Self::None
    }

    fn loss() -> Self {
        unimplemented!()
    }
}

#[test]
fn sample_smallest_probability_element_at_start() {
    let choices = vec![Fault::Value(0), Fault::Value(1)];
    let probs = vec![1, 1];
    let sampler = CorrelatedNoiseSampler::new(choices, probs);
    assert_eq!(Fault::Value(0), sampler.sample_with_value(0));
    assert_eq!(Fault::Value(1), sampler.sample_with_value(1));
    assert_eq!(Fault::None, sampler.sample_with_value(2));
}

#[test]
fn sample_smallest_probability_element_at_end() {
    let choices = vec![Fault::Value(0), Fault::Value(1)];
    let probs = vec![uq1_63::ONE - 1, 1];
    let sampler = CorrelatedNoiseSampler::new(choices, probs);
    assert_eq!(Fault::Value(0), sampler.sample_with_value(uq1_63::ONE - 2));
    assert_eq!(Fault::Value(1), sampler.sample_with_value(uq1_63::ONE - 1));
}

#[test]
fn binary_search_works_as_expected() {
    let mut choices = Vec::new();
    let mut probs = Vec::new();

    for i in 0..100 {
        choices.push(Fault::Value(i));
        probs.push(1);
    }

    let sampler = CorrelatedNoiseSampler::new(choices, probs);
    for i in 0..100 {
        assert_eq!(Fault::Value(i), sampler.sample_with_value(i));
    }
}
