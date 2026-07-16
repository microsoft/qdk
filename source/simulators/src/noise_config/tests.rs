// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::noise_config::{Sampler, decode_pauli, encode_pauli, uq1_63};

#[test]
fn sample_smallest_probability_element_at_start() {
    let choices = vec![0, 1];
    let probs = vec![1, 1];
    let sampler = Sampler::new(choices, probs);
    assert_eq!(Some(&0), sampler.sample_with_value(0));
    assert_eq!(Some(&1), sampler.sample_with_value(1));
    assert_eq!(None, sampler.sample_with_value(2));
}

#[test]
fn sample_smallest_probability_element_at_end() {
    let choices = vec![0, 1];
    let probs = vec![uq1_63::ONE - 1, 1];
    let sampler = Sampler::new(choices, probs);
    assert_eq!(Some(&0), sampler.sample_with_value(uq1_63::ONE - 2));
    assert_eq!(Some(&1), sampler.sample_with_value(uq1_63::ONE - 1));
}

#[test]
fn binary_search_works_as_expected() {
    let mut choices = Vec::new();
    let mut probs = Vec::new();

    for i in 0..100 {
        choices.push(i);
        probs.push(1);
    }

    let sampler = Sampler::new(choices, probs);
    for i in 0..100 {
        assert_eq!(Some(&i), sampler.sample_with_value(i));
    }
}

#[test]
fn test_encode_pauli() {
    assert_eq!(0b_000_000, encode_pauli("II"));
    assert_eq!(0b_000_001, encode_pauli("IX"));
    assert_eq!(0b_000_011, encode_pauli("IY"));
    assert_eq!(0b_000_010, encode_pauli("IZ"));
    assert_eq!(0b_000_100, encode_pauli("IL"));

    assert_eq!(0b_001_000, encode_pauli("XI"));
    assert_eq!(0b_001_001, encode_pauli("XX"));
    assert_eq!(0b_001_011, encode_pauli("XY"));
    assert_eq!(0b_001_010, encode_pauli("XZ"));
    assert_eq!(0b_001_100, encode_pauli("XL"));

    assert_eq!(0b_011_000, encode_pauli("YI"));
    assert_eq!(0b_011_001, encode_pauli("YX"));
    assert_eq!(0b_011_011, encode_pauli("YY"));
    assert_eq!(0b_011_010, encode_pauli("YZ"));
    assert_eq!(0b_011_100, encode_pauli("YL"));

    assert_eq!(0b_010_000, encode_pauli("ZI"));
    assert_eq!(0b_010_001, encode_pauli("ZX"));
    assert_eq!(0b_010_011, encode_pauli("ZY"));
    assert_eq!(0b_010_010, encode_pauli("ZZ"));
    assert_eq!(0b_010_100, encode_pauli("ZL"));

    assert_eq!(0b_100_000, encode_pauli("LI"));
    assert_eq!(0b_100_001, encode_pauli("LX"));
    assert_eq!(0b_100_011, encode_pauli("LY"));
    assert_eq!(0b_100_010, encode_pauli("LZ"));
    assert_eq!(0b_100_100, encode_pauli("LL"));
}

#[test]
fn test_decode_pauli() {
    const MAP: [char; 5] = ['I', 'X', 'Z', 'Y', 'L'];
    assert_eq!(vec!['I', 'I'], decode_pauli(0b_000_000, 2, &MAP));
    assert_eq!(vec!['I', 'X'], decode_pauli(0b_000_001, 2, &MAP));
    assert_eq!(vec!['I', 'Y'], decode_pauli(0b_000_011, 2, &MAP));
    assert_eq!(vec!['I', 'Z'], decode_pauli(0b_000_010, 2, &MAP));
    assert_eq!(vec!['I', 'L'], decode_pauli(0b_000_100, 2, &MAP));

    assert_eq!(vec!['X', 'I'], decode_pauli(0b_001_000, 2, &MAP));
    assert_eq!(vec!['X', 'X'], decode_pauli(0b_001_001, 2, &MAP));
    assert_eq!(vec!['X', 'Y'], decode_pauli(0b_001_011, 2, &MAP));
    assert_eq!(vec!['X', 'Z'], decode_pauli(0b_001_010, 2, &MAP));
    assert_eq!(vec!['X', 'L'], decode_pauli(0b_001_100, 2, &MAP));

    assert_eq!(vec!['Y', 'I'], decode_pauli(0b_011_000, 2, &MAP));
    assert_eq!(vec!['Y', 'X'], decode_pauli(0b_011_001, 2, &MAP));
    assert_eq!(vec!['Y', 'Y'], decode_pauli(0b_011_011, 2, &MAP));
    assert_eq!(vec!['Y', 'Z'], decode_pauli(0b_011_010, 2, &MAP));
    assert_eq!(vec!['Y', 'L'], decode_pauli(0b_011_100, 2, &MAP));

    assert_eq!(vec!['Z', 'I'], decode_pauli(0b_010_000, 2, &MAP));
    assert_eq!(vec!['Z', 'X'], decode_pauli(0b_010_001, 2, &MAP));
    assert_eq!(vec!['Z', 'Y'], decode_pauli(0b_010_011, 2, &MAP));
    assert_eq!(vec!['Z', 'Z'], decode_pauli(0b_010_010, 2, &MAP));
    assert_eq!(vec!['Z', 'L'], decode_pauli(0b_010_100, 2, &MAP));

    assert_eq!(vec!['L', 'I'], decode_pauli(0b_100_000, 2, &MAP));
    assert_eq!(vec!['L', 'X'], decode_pauli(0b_100_001, 2, &MAP));
    assert_eq!(vec!['L', 'Y'], decode_pauli(0b_100_011, 2, &MAP));
    assert_eq!(vec!['L', 'Z'], decode_pauli(0b_100_010, 2, &MAP));
    assert_eq!(vec!['L', 'L'], decode_pauli(0b_100_100, 2, &MAP));
}
