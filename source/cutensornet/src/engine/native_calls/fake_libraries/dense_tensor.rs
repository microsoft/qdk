// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`DenseTensor`] and the contraction the fake's tensor API computes with:
//! one contraction of labelled tensors, which also permutes a tensor's
//! modes. The SVD and the QR come from `dense_linalg`, whose SVD reports a
//! failure instead of returning wrong factors, so the fake cannot pass a
//! test its own maths got wrong.

use num_complex::Complex64;
use std::collections::BTreeMap;

/// A dense complex tensor: the extent and label of each mode, and its
/// elements, first mode fastest, as cuTensorNet lays out a tensor with null
/// strides.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct DenseTensor {
    pub(super) extents: Vec<usize>,
    pub(super) modes: Vec<i32>,
    pub(super) data: Vec<Complex64>,
}

/// Contracts `inputs` into a tensor of the modes `output`, in that order:
/// each element is the sum, over every label absent from the output, of
/// the product of the inputs' elements whose labels agree. With one input
/// whose labels are the output's, it permutes the input's modes. It fails
/// if a label has two extents, or an output label is in no input or
/// repeated.
pub(super) fn contract(
    inputs: &[&DenseTensor],
    output: &[i32],
) -> Result<DenseTensor, &'static str> {
    let mut extents: BTreeMap<i32, usize> = BTreeMap::new();
    for tensor in inputs {
        for (&mode, &extent) in tensor.modes.iter().zip(&tensor.extents) {
            if *extents.entry(mode).or_insert(extent) != extent {
                return Err("a label has two extents");
            }
        }
    }
    for (at, mode) in output.iter().enumerate() {
        if output[..at].contains(mode) {
            return Err("an output label is repeated");
        }
        if !extents.contains_key(mode) {
            return Err("an output label is in no input");
        }
    }
    // The labels, output first, then the summed ones; and each tensor's
    // stride along each label, 0 where it lacks the label.
    let mut labels: Vec<i32> = output.to_vec();
    labels.extend(extents.keys().filter(|mode| !output.contains(mode)));
    let label_extents: Vec<usize> = labels.iter().map(|mode| extents[mode]).collect();
    let strides = |modes: &[i32], extents: &[usize]| -> Vec<usize> {
        labels
            .iter()
            .map(|label| {
                modes
                    .iter()
                    .position(|mode| mode == label)
                    .map_or(0, |at| extents[..at].iter().product())
            })
            .collect()
    };
    let input_strides: Vec<Vec<usize>> = inputs
        .iter()
        .map(|tensor| strides(&tensor.modes, &tensor.extents))
        .collect();
    let output_extents = label_extents[..output.len()].to_vec();
    let output_strides = strides(output, &output_extents);
    let mut data = vec![Complex64::new(0.0, 0.0); output_extents.iter().product()];
    let mut index = vec![0; labels.len()];
    for _ in 0..label_extents.iter().product::<usize>() {
        let offset = |strides: &[usize]| -> usize {
            index
                .iter()
                .zip(strides)
                .map(|(i, stride)| i * stride)
                .sum()
        };
        let product: Complex64 = inputs
            .iter()
            .zip(&input_strides)
            .map(|(tensor, strides)| tensor.data[offset(strides)])
            .product();
        data[offset(&output_strides)] += product;
        for (i, &extent) in index.iter_mut().zip(&label_extents) {
            *i += 1;
            if *i < extent {
                break;
            }
            *i = 0;
        }
    }
    Ok(DenseTensor {
        extents: output_extents,
        modes: output.to_vec(),
        data,
    })
}
