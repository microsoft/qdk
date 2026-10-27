// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

"use strict";

const assert = require("node:assert/strict");
const { test } = require("node:test");
const { axisTicks, formatAxisTick, validateProfile } = require("./viewer.js");

test("requires a string group_name on each time series", () => {
  const metric = {
    metric_name: "X rate",
    metric_units: "events/s",
    group_name: "Event rates",
    series: [[0, 1]],
  };
  const profile = {
    totals: {
      logical_qubits: 0,
      logical_compute_qubits: 0,
      physical_qubits: 0,
      total_runtime_sec: 1,
      total_operations_count: 0,
      calls_per_operation: {},
      runtime_sec_per_operation: {},
    },
    profiles: { aggregation_window_sec: 1, profiles: [metric] },
    trace: null,
    infos: [],
  };
  assert.equal(validateProfile(profile), profile);
  for (const groupName of [undefined, null, 123, {}]) {
    metric.group_name = groupName;
    assert.throws(() => validateProfile(profile), /group_name/);
  }
});

test("uses integer ticks for a zero-to-ten axis", () => {
  assert.deepEqual(axisTicks(0, 10).ticks, [0, 2, 4, 6, 8, 10]);
});

test("prefers integer ticks when at least three fit", () => {
  assert.deepEqual(axisTicks(0, 3).ticks, [0, 1, 2, 3]);
});

test("avoids floating-point artifacts in fractional ticks", () => {
  const axis = axisTicks(0, 1.08);
  assert.deepEqual(axis.ticks, [0, 0.2, 0.4, 0.6, 0.8, 1]);
  assert.deepEqual(
    axis.ticks.map((value) => formatAxisTick(value, axis.step)),
    ["0", "0.2", "0.4", "0.6", "0.8", "1"],
  );
});

test("keeps between three and ten distinct, bounded ticks across scales and offsets", () => {
  for (const scale of [1e-12, 1e-6, 0.01, 1, 100, 1e6, 1e12]) {
    for (const offset of [0, 0.17, 7.31, 1000.19]) {
      for (const span of [0.03, 0.37, 1, 2.9, 9.9, 10, 11, 99]) {
        const minimum = offset * scale;
        const maximum = minimum + span * scale;
        const { ticks, step } = axisTicks(minimum, maximum);
        assert.ok(ticks.length >= 3 && ticks.length <= 10);
        assert.equal(new Set(ticks).size, ticks.length);
        assert.ok(ticks.every((value) => value >= minimum && value <= maximum));
        assert.ok(
          ticks.every(
            (value, index) => index === 0 || value > ticks[index - 1],
          ),
        );
        assert.equal(
          new Set(ticks.map((value) => formatAxisTick(value, step))).size,
          ticks.length,
        );
      }
    }
  }
});

test("preserves distinct labels when zoomed into nanosecond intervals", () => {
  const { ticks, step } = axisTicks(1, 1.00000001);
  assert.equal(
    new Set(ticks.map((value) => formatAxisTick(value, step))).size,
    ticks.length,
  );
});

test("rejects invalid or degenerate ranges explicitly", () => {
  for (const [minimum, maximum] of [
    [0, 0],
    [1, 0],
    [0, Infinity],
    [NaN, 1],
  ]) {
    assert.throws(() => axisTicks(minimum, maximum), RangeError);
  }
});
