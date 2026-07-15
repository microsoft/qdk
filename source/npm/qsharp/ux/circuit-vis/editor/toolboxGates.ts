// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { Ket, Measurement, Operation, Unitary } from "../data/circuit.js";

/**
 * Maps a toolbox key (e.g. `"RX"`, `"Reset"`) to the prototype `Operation` that gets dropped into
 * the circuit when the user drags that toolbox item. Keys are also used as `data-type` attributes
 * on the rendered toolbox SVG nodes.
 */
interface GateDictionary {
  [index: string]: Operation;
}

const _makeUnitary = (gate: string): Unitary => ({
  kind: "unitary",
  gate,
  targets: [],
});

const _makeMeasurement = (gate: string): Measurement => ({
  kind: "measurement",
  gate,
  qubits: [],
  results: [],
});

const _makeKet = (gate: string): Ket => ({
  kind: "ket",
  gate,
  targets: [],
});

/**
 * The default toolbox gate set. Order here is the order the gates appear in the toolbox grid
 * (left-to-right, top-to-bottom, 2 columns).
 *
 * To add a new toolbox gate: pick a stable key (used as `data-type` on the SVG node and as the
 * lookup key in `dragController.ts`), map it to a prototype `Operation`, and both the toolbox view
 * and the drag handlers will pick it up automatically.
 */
const toolboxGateDictionary: GateDictionary = {
  RX: _makeUnitary("Rx"),
  X: _makeUnitary("X"),
  RY: _makeUnitary("Ry"),
  Y: _makeUnitary("Y"),
  RZ: _makeUnitary("Rz"),
  Z: _makeUnitary("Z"),
  S: _makeUnitary("S"),
  T: _makeUnitary("T"),
  H: _makeUnitary("H"),
  SX: _makeUnitary("SX"),
  Reset: _makeKet("0"),
  Measure: _makeMeasurement("Measure"),
};

toolboxGateDictionary["RX"].params = [{ name: "theta", type: "Double" }];
toolboxGateDictionary["RY"].params = [{ name: "theta", type: "Double" }];
toolboxGateDictionary["RZ"].params = [{ name: "theta", type: "Double" }];

export { toolboxGateDictionary, type GateDictionary };
