// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import assert from "node:assert/strict";
import { test } from "node:test";
import { VIRTUAL_GEOMETRY } from "../../dist/ux/majorana/index.js";
import {
  createFixture,
  createVirtualFixture,
  representativeDeviceSizes,
  representativeVirtualDeviceSizes,
} from "./fixtures.mjs";

test("uses shared aligned centers for tetrons and qubit boxes", () => {
  const { geometry } = createFixture(2, 2);
  for (const tetron of geometry.tetrons.values()) {
    assert.equal(
      tetron.qubitBox.x + tetron.qubitBox.width / 2,
      tetron.center.x,
    );
    assert.equal(
      tetron.qubitBox.y + tetron.qubitBox.height / 2,
      tetron.center.y,
    );
  }
});

test("generates stable 1x1 logical geometry", () => {
  const { geometry } = createFixture(1, 1);
  assert.deepEqual(geometry.viewBox, {
    x: 0,
    y: 0,
    width: 780,
    height: 332,
  });
  assert.deepEqual(geometry.tetrons.get(0).center, { x: 200, y: 60 });
  assert.deepEqual(geometry.tetrons.get(3).center, { x: 580, y: 272 });
  assert.deepEqual(geometry.tetrons.get(0).mzmAnchors, {
    1: { x: 100, y: 24 },
    2: { x: 300, y: 24 },
    3: { x: 100, y: 96 },
    4: { x: 300, y: 96 },
  });
  assert.deepEqual(geometry.tetrons.get(0).upperRail, {
    edgeId: "q0-upper-rail",
    start: { x: 100, y: 24 },
    end: { x: 300, y: 24 },
  });
  assert.deepEqual(geometry.tetrons.get(0).bridge, {
    edgeId: "q0-bridge",
    start: { x: 200, y: 24 },
    end: { x: 200, y: 96 },
  });
  assert.deepEqual(geometry.linearIslands.get("island-r0-c0").center, {
    x: 200,
    y: 166,
  });
  assert.deepEqual(geometry.linearIslands.get("island-r0-c0").start, {
    x: 100,
    y: 166,
  });
});

test("creates adjacency endpoints at aligned qubit-box boundaries", () => {
  const { geometry } = createFixture(1, 1);
  assert.deepEqual(geometry.adjacencyEdges.get("horizontal-q0-q1"), {
    edgeId: "horizontal-q0-q1",
    start: { x: 288, y: 60 },
    end: { x: 492, y: 60 },
  });
  assert.deepEqual(geometry.adjacencyEdges.get("vertical-q0-q2"), {
    edgeId: "vertical-q0-q2",
    start: { x: 200, y: 116 },
    end: { x: 200, y: 216 },
  });
});

test("generates deterministic routing obstacles for tetrons and islands", () => {
  const { topology, geometry } = createFixture(2, 1);
  assert.equal(
    geometry.routingObstacles.length,
    topology.tetrons.length * 10 +
      topology.linearIslands.length * 8 +
      topology.adjacencyEdges.filter(
        (edge) => edge.orientation === "horizontal",
      ).length *
        2,
  );
  assert.deepEqual(geometry.routingObstacles[0], {
    id: "q0-m1-connector",
    kind: "mzm-connector",
    bounds: { x: 76, y: 4, width: 14, height: 40 },
  });
  assert.deepEqual(geometry.routingObstacles[1], {
    id: "q0-m1-block",
    kind: "block",
    bounds: { x: 24, y: 3, width: 42, height: 42 },
  });
  assert.ok(
    geometry.routingObstacles.some(
      (obstacle) => obstacle.id === "island-r0-c0-block",
    ),
  );
  assert.deepEqual(
    geometry.routingObstacles.find(
      (obstacle) =>
        obstacle.id === "island-r0-c0-left-upper-vertical-connector",
    ),
    {
      id: "island-r0-c0-left-upper-vertical-connector",
      kind: "vertical-connector",
      bounds: { x: 24, y: 123, width: 42, height: 14 },
    },
  );
  assert.deepEqual(
    geometry.routingObstacles.find(
      (obstacle) =>
        obstacle.id === "island-r0-c0-right-lower-vertical-connector",
    ),
    {
      id: "island-r0-c0-right-lower-vertical-connector",
      kind: "vertical-connector",
      bounds: { x: 334, y: 195, width: 42, height: 14 },
    },
  );
  assert.deepEqual(
    geometry.routingObstacles.find(
      (obstacle) => obstacle.id === "horizontal-q0-q1-upper-connector",
    ),
    {
      id: "horizontal-q0-q1-upper-connector",
      kind: "mzm-connector",
      bounds: { x: 383, y: 4, width: 14, height: 40 },
    },
  );
});

test("keeps every representative device inside its view box", () => {
  for (const [rows, columns] of representativeDeviceSizes) {
    const { geometry } = createFixture(rows, columns);
    for (const tetron of geometry.tetrons.values()) {
      assert.ok(tetron.bounds.x >= 0);
      assert.ok(tetron.bounds.y >= 0);
      assert.ok(
        tetron.bounds.x + tetron.bounds.width <= geometry.viewBox.width,
      );
      assert.ok(
        tetron.bounds.y + tetron.bounds.height <= geometry.viewBox.height,
      );
    }
    for (const obstacle of geometry.routingObstacles) {
      assert.ok(obstacle.bounds.x >= 0);
      assert.ok(obstacle.bounds.y >= 0);
      assert.ok(
        obstacle.bounds.x + obstacle.bounds.width <= geometry.viewBox.width,
      );
      assert.ok(
        obstacle.bounds.y + obstacle.bounds.height <= geometry.viewBox.height,
      );
    }
  }
});

test("places virtual circles at associated physical-column midpoints", () => {
  const { geometry, virtualGeometry } = createVirtualFixture(1, 1);
  assert.deepEqual(virtualGeometry.viewBox, geometry.viewBox);
  assert.deepEqual(virtualGeometry.qubits.get(0), {
    virtualQubitId: 0,
    center: { x: 200, y: 166 },
    radius: VIRTUAL_GEOMETRY.qubitRadius,
    bounds: { x: 128, y: 94, width: 144, height: 144 },
  });
  assert.deepEqual(virtualGeometry.qubits.get(1), {
    virtualQubitId: 1,
    center: { x: 580, y: 166 },
    radius: VIRTUAL_GEOMETRY.qubitRadius,
    bounds: { x: 508, y: 94, width: 144, height: 144 },
  });
});

test("clips virtual connectors to circle boundaries", () => {
  const { virtualGeometry } = createVirtualFixture(2, 2);
  assert.deepEqual(virtualGeometry.adjacencyEdges.get("horizontal-v0-v1"), {
    edgeId: "horizontal-v0-v1",
    orientation: "horizontal",
    scope: "intra-cell",
    supportedOperation: "CX",
    start: { x: 272, y: 166 },
    end: { x: 508, y: 166 },
  });
  assert.deepEqual(virtualGeometry.adjacencyEdges.get("horizontal-v1-v4"), {
    edgeId: "horizontal-v1-v4",
    orientation: "horizontal",
    scope: "inter-cell",
    supportedOperation: "CX",
    start: { x: 652, y: 166 },
    end: { x: 888, y: 166 },
  });
  assert.deepEqual(virtualGeometry.adjacencyEdges.get("vertical-v0-v2"), {
    edgeId: "vertical-v0-v2",
    orientation: "vertical",
    scope: "inter-cell",
    supportedOperation: "CZ",
    start: { x: 200, y: 238 },
    end: { x: 200, y: 518 },
  });
});

test("keeps every virtual circle and connector inside the shared view box", () => {
  for (const [rows, columns] of representativeVirtualDeviceSizes) {
    const { virtualGeometry } = createVirtualFixture(rows, columns);
    const { viewBox } = virtualGeometry;
    for (const qubit of virtualGeometry.qubits.values()) {
      assert.ok(qubit.bounds.x >= viewBox.x);
      assert.ok(qubit.bounds.y >= viewBox.y);
      assert.ok(
        qubit.bounds.x + qubit.bounds.width <= viewBox.x + viewBox.width,
      );
      assert.ok(
        qubit.bounds.y + qubit.bounds.height <= viewBox.y + viewBox.height,
      );
    }
    for (const connector of virtualGeometry.adjacencyEdges.values()) {
      for (const point of [connector.start, connector.end]) {
        assert.ok(point.x >= viewBox.x);
        assert.ok(point.y >= viewBox.y);
        assert.ok(point.x <= viewBox.x + viewBox.width);
        assert.ok(point.y <= viewBox.y + viewBox.height);
      }
    }
  }
});
