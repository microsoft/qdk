// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{Stream, StreamKey};
use expect_test::expect;
use rand::Rng as _;
use rustc_hash::FxHashSet;
use std::fmt::Write as _;

const STREAMS: [Stream; 3] = [Stream::Measurement, Stream::Noise, Stream::Classical];

#[test]
fn neighboring_keys_and_streams_share_no_values() {
    // A shared seed made streams identical (e.g. noise reused measurement values), and
    // additive per-shot seeding made neighboring seeds give shifted copies of each other.
    for seed in [0, u64::MAX] {
        let next = seed.wrapping_add(1);
        let keys = [
            StreamKey::session(seed),
            StreamKey::session(next),
            StreamKey::shot(seed, 0),
            StreamKey::shot(seed, 1),
            StreamKey::shot(next, 0),
            StreamKey::shot(next, 1),
        ];
        let mut seen = FxHashSet::default();
        for key in keys {
            for stream in STREAMS {
                let mut rng = key.rng(stream);
                for _ in 0..64 {
                    assert!(
                        seen.insert(rng.next_u64()),
                        "{key:?} {stream:?} repeats a value from another stream"
                    );
                }
            }
        }
    }
}

#[test]
fn derived_streams_are_stable() {
    // Every seeded result depends on these values. If they change on purpose (e.g. after a
    // `rand` upgrade), seeded outputs change everywhere; update with `UPDATE_EXPECT=1`.
    let mut actual = String::new();
    for key in [StreamKey::session(0), StreamKey::shot(0, 0)] {
        for stream in STREAMS {
            let value = key.rng(stream).next_u64();
            writeln!(actual, "{key:?} {stream:?}: {value:#018x}").expect("writing should succeed");
        }
    }
    expect![[r#"
        StreamKey { seed: 0, scope: Session } Measurement: 0xd26ebef1d9f5fca8
        StreamKey { seed: 0, scope: Session } Noise: 0x9ba37ea61d290754
        StreamKey { seed: 0, scope: Session } Classical: 0x3d1d4c8da64d81b2
        StreamKey { seed: 0, scope: Shot(0) } Measurement: 0x907df3b6a2b552ce
        StreamKey { seed: 0, scope: Shot(0) } Noise: 0x905ac62ae355aa11
        StreamKey { seed: 0, scope: Shot(0) } Classical: 0x1c68ec61e2c1de21
    "#]]
    .assert_eq(&actual);
}
