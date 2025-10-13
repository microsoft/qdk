// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// @ts-check

import assert from "node:assert/strict";
import { test } from "node:test";
import { Sqore } from "../../../dist/ux/circuit-vis/sqore.js";
import { CURRENT_VERSION } from "../../../dist/data-structures/circuit.js";
import { withDom } from "./withDom.js";
import path from "node:path";
import { fileURLToPath } from "node:url";

withDom();
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

function serializeNode(node) {
  // Use XMLSerializer for stable SVG output
  const ser = new XMLSerializer();
  // we wrap in <div> to keep a consistent root in snapshots
  return `${ser.serializeToString(node)}\n`;
}

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

test("one gate", (t0) => {
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

  const container = document.getElementById("app");
  const sqore = new Sqore(circuitGroup);
  sqore.draw(container);

  const html = serializeNode(document);

  const out = path.join(
    __dirname,
    "__html_snapshots__",
    "simple-two-circles.html",
  );

  t0.assert.snapshot(html);
  t0.assert.fileSnapshot(html, out, { serializers: [(s) => String(s)] });
});
