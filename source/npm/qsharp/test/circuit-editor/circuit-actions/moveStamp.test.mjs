// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// View-state stamp contract (`sqore-prev-location`).
//
// `moveOperation` deep-clones the source op, so the returned op
// has a different identity than the one in `Sqore.lastLocationMap`.
// A naive identity-keyed rebase in `Sqore.rebaseViewState` would
// drop the ViewState entry for the moved op, causing user-set
// expand/collapse choices to be lost. The most visible symptom
// is on classically-controlled groups: when no ViewState entry
// exists, the renderer's `hasClassicalControls && hasChildren`
// default re-expands groups the user had explicitly collapsed.
//
// `moveOperation` stamps `dataAttributes["sqore-prev-location"]`
// on the new op with the pre-move location. Sqore consumes the
// stamp as a fallback during rebase. These tests pin the stamp
// contract at the action layer.

// @ts-check

import { test } from "node:test";
import assert from "node:assert/strict";
import { CircuitModel } from "../../../dist/ux/circuit-vis/data/circuitModel.js";
import { moveOperation } from "../../../dist/ux/circuit-vis/actions/circuitActions.js";

test("moveOperation: returned op carries sqore-prev-location stamp with the source location", () => {
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      {
        components: [{ kind: "unitary", gate: "H", targets: [{ qubit: 0 }] }],
      },
      {
        components: [{ kind: "unitary", gate: "X", targets: [{ qubit: 1 }] }],
      },
    ],
  };
  const model = new CircuitModel(circuit);

  const moved = moveOperation(model, "0,0", "1,0", 0, 1, false, false);
  assert.ok(moved);
  const movedAny = /** @type {any} */ (moved);
  assert.equal(
    movedAny.dataAttributes?.["sqore-prev-location"],
    "0,0",
    "stamp must hold the PRE-move source location so Sqore can recover the ViewState entry",
  );
});

test("moveOperation: stamp survives the deep-clone roundtrip even when source had no prior dataAttributes", () => {
  // The source op has NO dataAttributes object before the move
  // (common for freshly-edited ops between renders). The stamp
  // contract has to lazily create the object — it can't depend on
  // a pre-existing dataAttributes.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }],
    componentGrid: [
      {
        components: [{ kind: "unitary", gate: "H", targets: [{ qubit: 0 }] }],
      },
      {
        components: [{ kind: "unitary", gate: "X", targets: [{ qubit: 1 }] }],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  // Verify the precondition the test is built around: no dataAttributes
  // on the source op going in.
  assert.equal(
    /** @type {any} */ (model.componentGrid[0].components[0]).dataAttributes,
    undefined,
  );

  const moved = moveOperation(model, "0,0", "1,0", 0, 1, false, false);
  assert.ok(moved);
  const movedAny = /** @type {any} */ (moved);
  assert.equal(movedAny.dataAttributes?.["sqore-prev-location"], "0,0");
});

test("moveOperation: stamp persists for a control-leg move on a group", () => {
  // Verifies the stamp is set regardless of which branch of `_moveY`
  // ran. The control-on-group leg-move path creates a new op
  // identity too, so the ViewState transfer must still work.
  /** @type {any} */
  const circuit = {
    qubits: [{ id: 0 }, { id: 1 }, { id: 2 }, { id: 3 }],
    componentGrid: [
      {
        components: [
          {
            kind: "unitary",
            gate: "Foo",
            targets: [{ qubit: 1 }, { qubit: 2 }],
            controls: [{ qubit: 0 }],
            children: [
              {
                components: [
                  { kind: "unitary", gate: "H", targets: [{ qubit: 1 }] },
                  { kind: "unitary", gate: "X", targets: [{ qubit: 2 }] },
                ],
              },
            ],
          },
        ],
      },
    ],
  };
  const model = new CircuitModel(circuit);
  const moved = moveOperation(model, "0,0", "0,0", 0, 3, true, false);
  assert.ok(moved);
  const movedAny = /** @type {any} */ (moved);
  assert.equal(
    movedAny.dataAttributes?.["sqore-prev-location"],
    "0,0",
    "control-leg move on a group must still stamp the prev-location for ViewState transfer",
  );
});
