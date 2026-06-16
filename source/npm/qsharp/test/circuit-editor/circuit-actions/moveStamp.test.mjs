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
import { moveOperation } from "../../../dist/ux/circuit-vis/actions/circuitActions.js";
import { at, build, circuit, gate, group } from "./_helpers.mjs";

test("moveOperation: returned op carries sqore-prev-location stamp with the source location", () => {
  const model = build(circuit(2, [[gate("H", 0)], [gate("X", 1)]]));

  const moved = moveOperation(model, "0,0", "1,0", 0, 1, false, false);
  assert.ok(moved);
  assert.equal(
    /** @type {any} */ (moved).dataAttributes?.["sqore-prev-location"],
    "0,0",
    "stamp must hold the PRE-move source location so Sqore can recover the ViewState entry",
  );
});

test("moveOperation: stamp survives the deep-clone roundtrip even when source had no prior dataAttributes", () => {
  const model = build(circuit(2, [[gate("H", 0)], [gate("X", 1)]]));
  // Precondition: no dataAttributes on the source op going in.
  assert.equal(/** @type {any} */ (at(model, "0,0")).dataAttributes, undefined);

  const moved = moveOperation(model, "0,0", "1,0", 0, 1, false, false);
  assert.ok(moved);
  assert.equal(
    /** @type {any} */ (moved).dataAttributes?.["sqore-prev-location"],
    "0,0",
  );
});

test("moveOperation: stamp persists for a control-leg move on a group", () => {
  // Control-leg move on a group takes a distinct `_moveY` branch but
  // must still stamp prev-location for the ViewState transfer.
  const model = build(
    circuit(4, [
      [group("Foo", [[gate("H", 1), gate("X", 2)]], { ctrls: [0] })],
    ]),
  );
  const moved = moveOperation(model, "0,0", "0,0", 0, 3, true, false);
  assert.ok(moved);
  assert.equal(
    /** @type {any} */ (moved).dataAttributes?.["sqore-prev-location"],
    "0,0",
    "control-leg move on a group must still stamp the prev-location for ViewState transfer",
  );
});
