// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import {
  createMajoranaGeometry,
  createMajoranaTopology,
  createVirtualGeometry,
  createVirtualTopology,
  validateMajoranaInput,
} from "../../dist/ux/majorana/index.js";

export function createFixture(cellRows, cellColumns) {
  const input = validateMajoranaInput([
    [cellRows, cellColumns],
    [[["Mx", [0], null]]],
  ]);
  const topology = createMajoranaTopology(input.device);
  const geometry = createMajoranaGeometry(topology);
  return { input, topology, geometry };
}

export function createVirtualFixture(cellRows, cellColumns) {
  const fixture = createFixture(cellRows, cellColumns);
  const virtualTopology = createVirtualTopology(fixture.input.device);
  const virtualGeometry = createVirtualGeometry(
    virtualTopology,
    fixture.geometry,
  );
  return { ...fixture, virtualTopology, virtualGeometry };
}

export const representativeDeviceSizes = [
  [1, 1],
  [2, 1],
  [2, 2],
  [4, 4],
];

export const representativeVirtualDeviceSizes = [
  [1, 1],
  [2, 1],
  [1, 2],
  [2, 2],
  [4, 4],
];
