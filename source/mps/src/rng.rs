// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use rand::{SeedableRng, TryRng, rngs::ChaCha12Rng};

const GOLDEN_GAMMA: u64 = 0x9e37_79b9_7f4a_7c15;

#[must_use]
pub fn derive_shot_seed(base_seed: u64, shot_index: u64) -> u64 {
    mix64(base_seed.wrapping_add(GOLDEN_GAMMA.wrapping_mul(shot_index.wrapping_add(1))))
}

pub(crate) struct MeasurementRng {
    inner: ChaCha12Rng,
}

impl MeasurementRng {
    pub(crate) fn new(seed: u64) -> Self {
        Self {
            inner: ChaCha12Rng::from_seed(expand_seed(seed)),
        }
    }

    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn next_f64(&mut self) -> f64 {
        let value = match self.inner.try_next_u64() {
            Ok(value) => value >> 11,
            Err(error) => match error {},
        };
        (value as f64) * (1.0 / 9_007_199_254_740_992.0)
    }
}

fn expand_seed(seed: u64) -> [u8; 32] {
    let mut expanded = [0_u8; 32];
    for (chunk, offset) in expanded.chunks_exact_mut(8).zip(1_u64..) {
        let word = mix64(seed.wrapping_add(GOLDEN_GAMMA.wrapping_mul(offset)));
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    expanded
}

fn mix64(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_is_stable() {
        let mut rng = MeasurementRng::new(0);
        assert_eq!(rng.next_f64().to_bits(), 0x3fea_3193_af70_56cf);
        assert_eq!(derive_shot_seed(0, 0), 0xe220_a839_7b1d_cdaf);
    }
}
