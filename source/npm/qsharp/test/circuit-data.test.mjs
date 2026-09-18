// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import assert from "node:assert/strict";
import { test } from "node:test";

import { isOperation } from "../dist/data-structures/circuit.js";
import { toCircuitGroup } from "../dist/data-structures/legacyCircuitUpdate.js";

test("unitary accepts structured gate and output error information", () => {
  assert.equal(
    isOperation({
      kind: "unitary",
      gate: "X",
      targets: [{ qubit: 0 }],
      error: {
        gateError: 0.01,
        outputErrors: [[{ qubit: 0 }, 0.0199]],
      },
    }),
    true,
  );
});

test("unitary rejects the legacy scalar error shape", () => {
  assert.equal(
    isOperation({
      kind: "unitary",
      gate: "X",
      targets: [{ qubit: 0 }],
      error: 0.01,
    }),
    false,
  );
});

test("widget schema conversion accepts a circuit with structured errors", () => {
  const result = toCircuitGroup({
    qubits: [{ id: 0 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "X",
            targets: [{ qubit: 0 }],
            error: {
              gateError: 0.01,
              outputErrors: [[{ qubit: 0 }, 0.01]],
            },
          },
        ],
      },
    ],
  });

  assert.equal(result.ok, true);
});
