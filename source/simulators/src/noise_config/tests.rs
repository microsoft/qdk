// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::noise_config::{self, CorrelatedNoiseSampler, decode_pauli, encode_pauli, uq1_63};

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

#[test]
fn test_encode_pauli() {
    assert_eq!(0b_00_00, encode_pauli("II"));
    assert_eq!(0b_00_01, encode_pauli("IX"));
    assert_eq!(0b_00_11, encode_pauli("IY"));
    assert_eq!(0b_00_10, encode_pauli("IZ"));
    assert_eq!(0b_01_00, encode_pauli("XI"));
    assert_eq!(0b_01_01, encode_pauli("XX"));
    assert_eq!(0b_01_11, encode_pauli("XY"));
    assert_eq!(0b_01_10, encode_pauli("XZ"));
    assert_eq!(0b_11_00, encode_pauli("YI"));
    assert_eq!(0b_11_01, encode_pauli("YX"));
    assert_eq!(0b_11_11, encode_pauli("YY"));
    assert_eq!(0b_11_10, encode_pauli("YZ"));
    assert_eq!(0b_10_00, encode_pauli("ZI"));
    assert_eq!(0b_10_01, encode_pauli("ZX"));
    assert_eq!(0b_10_11, encode_pauli("ZY"));
    assert_eq!(0b_10_10, encode_pauli("ZZ"));
}

#[test]
fn test_decode_pauli() {
    const MAP: [char; 4] = ['I', 'X', 'Z', 'Y'];
    assert_eq!(vec!['I', 'I'], decode_pauli(0b_00_00, 2, &MAP));
    assert_eq!(vec!['I', 'X'], decode_pauli(0b_00_01, 2, &MAP));
    assert_eq!(vec!['I', 'Y'], decode_pauli(0b_00_11, 2, &MAP));
    assert_eq!(vec!['I', 'Z'], decode_pauli(0b_00_10, 2, &MAP));
    assert_eq!(vec!['X', 'I'], decode_pauli(0b_01_00, 2, &MAP));
    assert_eq!(vec!['X', 'X'], decode_pauli(0b_01_01, 2, &MAP));
    assert_eq!(vec!['X', 'Y'], decode_pauli(0b_01_11, 2, &MAP));
    assert_eq!(vec!['X', 'Z'], decode_pauli(0b_01_10, 2, &MAP));
    assert_eq!(vec!['Y', 'I'], decode_pauli(0b_11_00, 2, &MAP));
    assert_eq!(vec!['Y', 'X'], decode_pauli(0b_11_01, 2, &MAP));
    assert_eq!(vec!['Y', 'Y'], decode_pauli(0b_11_11, 2, &MAP));
    assert_eq!(vec!['Y', 'Z'], decode_pauli(0b_11_10, 2, &MAP));
    assert_eq!(vec!['Z', 'I'], decode_pauli(0b_10_00, 2, &MAP));
    assert_eq!(vec!['Z', 'X'], decode_pauli(0b_10_01, 2, &MAP));
    assert_eq!(vec!['Z', 'Y'], decode_pauli(0b_10_11, 2, &MAP));
    assert_eq!(vec!['Z', 'Z'], decode_pauli(0b_10_10, 2, &MAP));
}
