// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::unreadable_literal)]
#![allow(clippy::float_cmp)]

use crate::semantic::ast::{LiteralKind, TimeUnit};

use super::Duration;

#[test]
fn normalize_pair_same_units() {
    let d1 = Duration::new(5.0, TimeUnit::Ns);
    let d2 = Duration::new(10.0, TimeUnit::Ns);
    let (normalized_d1, normalized_d2) = d1.normalize_pair(d2);

    assert_eq!(normalized_d1.value, 5.0);
    assert_eq!(normalized_d1.unit, TimeUnit::Ns);
    assert_eq!(normalized_d2.value, 10.0);
    assert_eq!(normalized_d2.unit, TimeUnit::Ns);
}

#[test]
fn normalize_pair_different_units() {
    let d1 = Duration::new(1.0, TimeUnit::Us); // 1 microsecond = 1000 nanoseconds
    let d2 = Duration::new(500.0, TimeUnit::Ns); // 500 nanoseconds
    let (normalized_d1, normalized_d2) = d1.normalize_pair(d2);

    // Should choose Ns (smaller unit)
    assert_eq!(normalized_d1.value, 1000.0);
    assert_eq!(normalized_d1.unit, TimeUnit::Ns);
    assert_eq!(normalized_d2.value, 500.0);
    assert_eq!(normalized_d2.unit, TimeUnit::Ns);
}

#[test]
fn normalize_pair_ms_to_us() {
    let d1 = Duration::new(2.0, TimeUnit::Ms); // 2 milliseconds
    let d2 = Duration::new(1500.0, TimeUnit::Us); // 1500 microseconds
    let (normalized_d1, normalized_d2) = d1.normalize_pair(d2);

    // Should choose Us (smaller unit)
    assert_eq!(normalized_d1.value, 2000.0); // 2ms = 2000us
    assert_eq!(normalized_d1.unit, TimeUnit::Us);
    assert_eq!(normalized_d2.value, 1500.0);
    assert_eq!(normalized_d2.unit, TimeUnit::Us);
}

#[test]
fn normalize_pair_s_to_ms() {
    let d1 = Duration::new(1.0, TimeUnit::S); // 1 second
    let d2 = Duration::new(500.0, TimeUnit::Ms); // 500 milliseconds
    let (normalized_d1, normalized_d2) = d1.normalize_pair(d2);

    // Should choose Ms (smaller unit)
    assert_eq!(normalized_d1.value, 1000.0); // 1s = 1000ms
    assert_eq!(normalized_d1.unit, TimeUnit::Ms);
    assert_eq!(normalized_d2.value, 500.0);
    assert_eq!(normalized_d2.unit, TimeUnit::Ms);
}

#[test]
fn normalize_pair_with_dt() {
    let d1 = Duration::new(5.0, TimeUnit::Dt);
    let d2 = Duration::new(10.0, TimeUnit::Ns);
    let (normalized_d1, normalized_d2) = d1.normalize_pair(d2);

    assert_eq!(normalized_d1.value, 5.0);
    assert_eq!(normalized_d1.unit, TimeUnit::Dt);
    assert_eq!(normalized_d2.value, 10.0);
    assert_eq!(normalized_d2.unit, TimeUnit::Dt);
}

#[test]
fn addition_with_normalize_pair() {
    let d1 = Duration::new(1.0, TimeUnit::Ms); // 1 millisecond
    let d2 = Duration::new(500.0, TimeUnit::Us); // 500 microseconds
    let result = d1 + d2; // Should use Us (smaller unit): 1000us + 500us = 1500us

    assert_eq!(result.value, 1500.0);
    assert_eq!(result.unit, TimeUnit::Us);
}

#[test]
fn comparison_with_normalize_pair() {
    let d1 = Duration::new(1.0, TimeUnit::Ms); // 1 millisecond = 1,000,000 nanoseconds
    let d2 = Duration::new(1_000_000.0, TimeUnit::Ns); // 1,000,000 nanoseconds

    assert_eq!(d1, d2);
}

#[test]
fn display() {
    let d1 = Duration::new(5.5, TimeUnit::Ns);
    assert_eq!(format!("{d1}"), "5.5 ns");

    let d2 = Duration::new(1000.0, TimeUnit::Us);
    assert_eq!(format!("{d2}"), "1000.0 us");

    let d3 = Duration::new(2.5, TimeUnit::Dt);
    assert_eq!(format!("{d3}"), "2.5 dt");
}

#[test]
fn default() {
    let d = Duration::default();
    assert_eq!(d.value, 0.0);
    assert_eq!(d.unit, TimeUnit::default());
}

#[test]
fn new() {
    let d = Duration::new(42.5, TimeUnit::Ms);
    assert_eq!(d.value, 42.5);
    assert_eq!(d.unit, TimeUnit::Ms);
}

#[test]
fn clone_copy() {
    let d1 = Duration::new(100.0, TimeUnit::Us);
    let d2 = d1; // Copy
    #[allow(clippy::clone_on_copy)]
    let d3 = d1.clone(); // Clone

    assert_eq!(d1, d2);
    assert_eq!(d1, d3);
    assert_eq!(d2, d3);
}

#[test]
fn partial_eq_same_unit() {
    let d1 = Duration::new(5.0, TimeUnit::Ns);
    let d2 = Duration::new(5.0, TimeUnit::Ns);
    let d3 = Duration::new(5.1, TimeUnit::Ns);

    assert_eq!(d1, d2);
    assert_ne!(d1, d3);
}

#[test]
fn partial_eq_with_epsilon() {
    let d1 = Duration::new(1.0, TimeUnit::Ns);
    let d2 = Duration::new(1.0 + f64::EPSILON / 2.0, TimeUnit::Ns);

    assert_eq!(d1, d2); // Should be equal within epsilon
}

#[test]
fn partial_eq_dt_units() {
    let d1 = Duration::new(5.0, TimeUnit::Dt);
    let d2 = Duration::new(5.0, TimeUnit::Dt);
    let d3 = Duration::new(6.0, TimeUnit::Dt);

    assert_eq!(d1, d2);
    assert_ne!(d1, d3);
}

#[test]
fn add_same_units() {
    let d1 = Duration::new(3.0, TimeUnit::Ms);
    let d2 = Duration::new(2.0, TimeUnit::Ms);
    let result = d1 + d2;

    assert_eq!(result.value, 5.0);
    assert_eq!(result.unit, TimeUnit::Ms);
}

#[test]
fn add_dt_units() {
    let d1 = Duration::new(3.0, TimeUnit::Dt);
    let d2 = Duration::new(2.0, TimeUnit::Dt);
    let result = d1 + d2;

    assert_eq!(result.value, 5.0);
    assert_eq!(result.unit, TimeUnit::Dt);
}

#[test]
fn add_mixed_with_dt() {
    let d1 = Duration::new(3.0, TimeUnit::Dt);
    let d2 = Duration::new(2.0, TimeUnit::Ms);
    let result = d1 + d2;

    assert_eq!(result.value, 5.0);
    assert_eq!(result.unit, TimeUnit::Dt);
}

#[test]
fn add_different_units_to_smaller() {
    let d1 = Duration::new(1.0, TimeUnit::S); // 1 second
    let d2 = Duration::new(500.0, TimeUnit::Ms); // 500 milliseconds
    let result = d1 + d2; // Should use Ms: 1000ms + 500ms = 1500ms

    assert_eq!(result.value, 1500.0);
    assert_eq!(result.unit, TimeUnit::Ms);
}

#[test]
fn sub_same_units() {
    let d1 = Duration::new(5.0, TimeUnit::Us);
    let d2 = Duration::new(2.0, TimeUnit::Us);
    let result = d1 - d2;

    assert_eq!(result.value, 3.0);
    assert_eq!(result.unit, TimeUnit::Us);
}

#[test]
fn sub_dt_units() {
    let d1 = Duration::new(10.0, TimeUnit::Dt);
    let d2 = Duration::new(3.0, TimeUnit::Dt);
    let result = d1 - d2;

    assert_eq!(result.value, 7.0);
    assert_eq!(result.unit, TimeUnit::Dt);
}

#[test]
fn sub_mixed_with_dt() {
    let d1 = Duration::new(10.0, TimeUnit::Ns);
    let d2 = Duration::new(3.0, TimeUnit::Dt);
    let result = d1 - d2;

    assert_eq!(result.value, 7.0);
    assert_eq!(result.unit, TimeUnit::Dt);
}

#[test]
fn sub_different_units_to_smaller() {
    let d1 = Duration::new(2.0, TimeUnit::Ms); // 2 milliseconds = 2000 microseconds
    let d2 = Duration::new(500.0, TimeUnit::Us); // 500 microseconds
    let result = d1 - d2; // Should use Us: 2000us - 500us = 1500us

    assert_eq!(result.value, 1500.0);
    assert_eq!(result.unit, TimeUnit::Us);
}

#[test]
fn mul_f64() {
    let d = Duration::new(5.0, TimeUnit::Ms);
    let result = d * 2.5;

    assert_eq!(result.value, 12.5);
    assert_eq!(result.unit, TimeUnit::Ms);
}

#[test]
fn mul_f64_zero() {
    let d = Duration::new(100.0, TimeUnit::Ns);
    let result = d * 0.0;

    assert_eq!(result.value, 0.0);
    assert_eq!(result.unit, TimeUnit::Ns);
}

#[test]
fn mul_f64_negative() {
    let d = Duration::new(10.0, TimeUnit::Us);
    let result = d * -2.0;

    assert_eq!(result.value, -20.0);
    assert_eq!(result.unit, TimeUnit::Us);
}

#[test]
fn mul_i64() {
    let d = Duration::new(3.0, TimeUnit::S);
    let result = d * 4i64;

    assert_eq!(result.value, 12.0);
    assert_eq!(result.unit, TimeUnit::S);
}

#[test]
fn mul_i64_zero() {
    let d = Duration::new(100.0, TimeUnit::Ms);
    #[allow(clippy::erasing_op)]
    let result = d * 0i64;

    assert_eq!(result.value, 0.0);
    assert_eq!(result.unit, TimeUnit::Ms);
}

#[test]
fn mul_i64_negative() {
    let d = Duration::new(5.0, TimeUnit::Dt);
    let result = d * -3i64;

    assert_eq!(result.value, -15.0);
    assert_eq!(result.unit, TimeUnit::Dt);
}

#[test]
fn div_duration_same_units() {
    let d1 = Duration::new(10.0, TimeUnit::Ms);
    let d2 = Duration::new(2.0, TimeUnit::Ms);
    let result = d1 / d2;

    assert_eq!(result, 5.0);
}

#[test]
fn div_duration_dt_units() {
    let d1 = Duration::new(15.0, TimeUnit::Dt);
    let d2 = Duration::new(3.0, TimeUnit::Dt);
    let result = d1 / d2;

    assert_eq!(result, 5.0);
}

#[test]
fn div_duration_mixed_with_dt() {
    let d1 = Duration::new(20.0, TimeUnit::Dt);
    let d2 = Duration::new(4.0, TimeUnit::Ns);
    let result = d1 / d2;

    assert_eq!(result, 5.0);
}

#[test]
fn div_duration_different_units() {
    let d1 = Duration::new(2.0, TimeUnit::Ms); // 2 milliseconds = 2000 microseconds
    let d2 = Duration::new(500.0, TimeUnit::Us); // 500 microseconds
    let result = d1 / d2; // 2000us / 500us = 4.0

    assert_eq!(result, 4.0);
}

#[test]
fn div_f64() {
    let d = Duration::new(10.0, TimeUnit::Us);
    let result = d / 2.5;

    assert_eq!(result.value, 4.0);
    assert_eq!(result.unit, TimeUnit::Us);
}

#[test]
fn div_f64_negative() {
    let d = Duration::new(15.0, TimeUnit::Ns);
    let result = d / -3.0;

    assert_eq!(result.value, -5.0);
    assert_eq!(result.unit, TimeUnit::Ns);
}

#[test]
fn div_i64() {
    let d = Duration::new(20.0, TimeUnit::S);
    let result = d / 4i64;

    assert_eq!(result.value, 5.0);
    assert_eq!(result.unit, TimeUnit::S);
}

#[test]
fn div_i64_negative() {
    let d = Duration::new(12.0, TimeUnit::Dt);
    let result = d / -3i64;

    assert_eq!(result.value, -4.0);
    assert_eq!(result.unit, TimeUnit::Dt);
}

#[test]
fn from_duration_to_literal_kind() {
    let d = Duration::new(42.0, TimeUnit::Ms);
    let literal: LiteralKind = d.into();

    match literal {
        LiteralKind::Duration(dur) => {
            assert_eq!(dur.value, 42.0);
            assert_eq!(dur.unit, TimeUnit::Ms);
        }
        _ => panic!("Expected LiteralKind::Duration"),
    }
}

#[test]
fn from_f64_to_literal_kind() {
    let literal: LiteralKind = 123.45f64.into();

    match literal {
        LiteralKind::Duration(dur) => {
            assert_eq!(dur.value, 123.45);
            assert_eq!(dur.unit, TimeUnit::default());
        }
        _ => panic!("Expected LiteralKind::Duration"),
    }
}

#[test]
fn from_i64_to_literal_kind() {
    let literal: LiteralKind = 456i64.into();

    match literal {
        LiteralKind::Duration(dur) => {
            assert_eq!(dur.value, 456.0);
            assert_eq!(dur.unit, TimeUnit::default());
        }
        _ => panic!("Expected LiteralKind::Duration"),
    }
}

#[test]
fn smaller_unit_dt_priority() {
    assert_eq!(
        Duration::smaller_unit(TimeUnit::Dt, TimeUnit::Ns),
        TimeUnit::Dt
    );
    assert_eq!(
        Duration::smaller_unit(TimeUnit::Dt, TimeUnit::Ms),
        TimeUnit::Dt
    );
    assert_eq!(
        Duration::smaller_unit(TimeUnit::Dt, TimeUnit::Us),
        TimeUnit::Dt
    );
    assert_eq!(
        Duration::smaller_unit(TimeUnit::Ms, TimeUnit::Dt),
        TimeUnit::Dt
    );
    assert_eq!(
        Duration::smaller_unit(TimeUnit::Dt, TimeUnit::Dt),
        TimeUnit::Dt
    );
}

#[test]
fn smaller_unit_hierarchy() {
    assert_eq!(
        Duration::smaller_unit(TimeUnit::S, TimeUnit::Ms),
        TimeUnit::Ms
    );
    assert_eq!(
        Duration::smaller_unit(TimeUnit::Ms, TimeUnit::Us),
        TimeUnit::Us
    );
    assert_eq!(
        Duration::smaller_unit(TimeUnit::Us, TimeUnit::Ns),
        TimeUnit::Ns
    );
    assert_eq!(
        Duration::smaller_unit(TimeUnit::S, TimeUnit::Ns),
        TimeUnit::Ns
    );
}

#[test]
fn smaller_unit_same_units() {
    assert_eq!(
        Duration::smaller_unit(TimeUnit::Ns, TimeUnit::Ns),
        TimeUnit::Ns
    );
    assert_eq!(
        Duration::smaller_unit(TimeUnit::Ms, TimeUnit::Ms),
        TimeUnit::Ms
    );
    assert_eq!(
        Duration::smaller_unit(TimeUnit::Us, TimeUnit::Us),
        TimeUnit::Us
    );
    assert_eq!(
        Duration::smaller_unit(TimeUnit::S, TimeUnit::S),
        TimeUnit::S
    );
}

#[test]
fn convert_to_unit_same() {
    let d = Duration::new(100.0, TimeUnit::Ms);
    let converted = d.convert_to_unit(TimeUnit::Ms);

    assert_eq!(converted.value, 100.0);
    assert_eq!(converted.unit, TimeUnit::Ms);
}

#[test]
fn convert_to_unit_ns_to_us() {
    let d = Duration::new(5000.0, TimeUnit::Ns);
    let converted = d.convert_to_unit(TimeUnit::Us);

    assert_eq!(converted.value, 5.0);
    assert_eq!(converted.unit, TimeUnit::Us);
}

#[test]
fn convert_to_unit_s_to_ms() {
    let d = Duration::new(2.5, TimeUnit::S);
    let converted = d.convert_to_unit(TimeUnit::Ms);

    assert_eq!(converted.value, 2500.0);
    assert_eq!(converted.unit, TimeUnit::Ms);
}

#[test]
fn convert_to_unit_with_dt() {
    let d = Duration::new(42.0, TimeUnit::Dt);
    let converted = d.convert_to_unit(TimeUnit::Ns);

    assert_eq!(converted.value, 42.0);
    assert_eq!(converted.unit, TimeUnit::Ns);
}

#[test]
fn convert_to_unit_to_dt() {
    let d = Duration::new(100.0, TimeUnit::Ms);
    let converted = d.convert_to_unit(TimeUnit::Dt);

    assert_eq!(converted.value, 100.0);
    assert_eq!(converted.unit, TimeUnit::Dt);
}

#[test]
fn zero_duration_equality_mixed_units() {
    let d1 = Duration::new(0.0, TimeUnit::Ns);
    let d2 = Duration::new(0.0, TimeUnit::S);

    assert_eq!(d1, d2); // Both are zero, should be equal regardless of unit
}

#[test]
fn very_small_values() {
    let d1 = Duration::new(1e-9, TimeUnit::S);
    let d2 = Duration::new(1.0, TimeUnit::Ns);

    assert_eq!(d1, d2); // 1e-9 seconds = 1 nanosecond
}

#[test]
fn very_large_values() {
    let d1 = Duration::new(1e6, TimeUnit::S);
    let d2 = Duration::new(1e9, TimeUnit::Ms);

    assert_eq!(d1, d2); // 1e6 seconds = 1e9 milliseconds
}

#[test]
fn complex_arithmetic_chain() {
    let d1 = Duration::new(1.0, TimeUnit::S);
    let d2 = Duration::new(500.0, TimeUnit::Ms);
    let d3 = Duration::new(250000.0, TimeUnit::Us);

    // 1s + 500ms + 250000us = 1000ms + 500ms + 250ms = 1750ms
    let result = d1 + d2 + d3;
    assert_eq!(result.value, 1750000.0);
    assert_eq!(result.unit, TimeUnit::Us);
}

#[test]
fn mixed_operations() {
    let d1 = Duration::new(2.0, TimeUnit::S);
    let d2 = Duration::new(500.0, TimeUnit::Ms);

    let added = d1 + d2; // 2500ms
    let multiplied = added * 2.0; // 5000ms
    let divided = multiplied / Duration::new(1.0, TimeUnit::S); // 5000ms / 1000ms = 5.0

    assert_eq!(divided, 5.0);
}
