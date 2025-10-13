// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// @ts-check

import assert from "node:assert/strict";
import { test } from "node:test";
import { Sqore } from "../../../dist/ux/circuit-vis/sqore.js";
import { CURRENT_VERSION } from "../../../dist/data-structures/circuit.js";

test("empty circuit", () => {
  /**
   * @type {import("../../../dist/data-structures/circuit.js").CircuitGroup}
   */
  const circuitGroup = {
    version: CURRENT_VERSION,
    circuits: [],
  };

  assert.throws(
    () => new Sqore(circuitGroup),
    (e) => e instanceof Error && e.message.includes("No circuit found"),
  );
});

test("one gate", () => {
  /**
   * @type {import("../../../dist/data-structures/circuit.js").CircuitGroup}
   */
  const circuitGroup = {
    version: CURRENT_VERSION,
    circuits: [
      {
        qubits: [
          {
            id: 0,
            numResults: 0,
          },
        ],
        componentGrid: [
          {
            components: [
              {
                gate: "H",
                kind: "unitary",
                targets: [
                  {
                    qubit: 0,
                  },
                ],
              },
            ],
          },
        ],
      },
    ],
  };

  const sqore = new Sqore(circuitGroup);
});
