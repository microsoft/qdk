// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import {
  collectMeasurementConsumers,
  moveMeasurementWithDependents,
  moveOperation,
  removeMeasurementWithDependents,
  removeOperation,
} from "../actions/circuitActions.js";
import { CircuitModel } from "../data/circuitModel.js";
import { Location } from "../data/location.js";
import { Operation } from "../data/circuit.js";
import { _createConfirmPrompt } from "./prompts.js";
import { findOperation } from "../utils.js";

/**
 * Delete an operation. If the op is a measurement with downstream
 * classical consumers, prompt the user first; on confirm, the
 * measurement is removed along with every dependent op.
 *
 * The non-measurement / no-consumer paths are direct passthroughs
 * to [`removeOperation`](../actions/circuitActions.ts) so callers
 * don't need to special-case the wrapper themselves — every Delete
 * path in the editor can route through this entry point safely.
 *
 * Wraps the existing
 * [`_createConfirmPrompt`](prompts.ts) primitive, mirroring the
 * pattern from
 * [`QubitController.removeQubitLineWithConfirmation`](controllers/qubitController.ts)
 * where the controller owns the prompt + render orchestration and
 * the action layer stays UI-free.
 *
 * `renderFn` is invoked once at the end of every code path that
 * mutates the model (including the no-consumer fast path). When the
 * user cancels the prompt, no mutation happens and `renderFn` is
 * NOT called — matching the "no model change → no re-render"
 * convention the dropzone-commit path already uses.
 */
const _deleteOperationWithConfirmation = (
  model: CircuitModel,
  location: string,
  renderFn: () => void,
): void => {
  const op = findOperation(model.componentGrid, location);
  if (op != null && op.kind === "measurement") {
    const consumers = collectMeasurementConsumers(
      model.componentGrid,
      location,
    );
    if (consumers.length > 0) {
      const n = consumers.length;
      const message =
        n === 1
          ? `Deleting this measurement will also delete 1 dependent operation that references its classical result. Continue?`
          : `Deleting this measurement will also delete ${n} dependent operations that reference its classical result. Continue?`;
      _createConfirmPrompt(message, (confirmed) => {
        if (!confirmed) return;
        removeMeasurementWithDependents(
          model,
          location,
          consumers.map((c) => c.op),
        );
        renderFn();
      });
      return;
    }
  }
  removeOperation(model, location);
  renderFn();
};

/**
 * Move an operation. If the op is a measurement with downstream
 * classical consumers, prompt the user before committing the move.
 * On confirm, the move runs with consumer-classical-ref remapping
 * for every consumer that stays after the M's new column, and
 * cascade-deletion for every consumer that would end up
 * at-or-before it (a "mix of move and delete" — the user's
 * directive for the column-order edge case).
 *
 * The non-measurement / no-consumer paths are direct passthroughs
 * to [`moveOperation`](../actions/circuitActions.ts). `renderFn` is
 * invoked once when the model changes; the dropzone-commit path's
 * existing `deepEqual` short-circuit handles the "no change" case
 * upstream so we don't second-guess it here.
 *
 * `movingControl` MUST be threaded through unchanged. The
 * dragController routes every non-clone drag through this wrapper,
 * INCLUDING control-dot drags on ordinary unitaries (a CNOT's
 * control dot, a group's control dot, etc.). Hardcoding
 * `movingControl: false` here corrupts those gates: `_moveY`'s
 * single-leg branch treats the control's wire as the target's
 * wire and rewrites `op.targets` to a single-wire stub on the
 * control's wire — turning CNOT(target=q1, ctrl=q0) into a
 * self-controlled X on q0 after a horizontal-column drag of the
 * control dot. The M-consumer cascade path below still passes
 * `false` to `moveMeasurementWithDependents` because Ms have no
 * `controls` array, so control-drag of an M is unreachable.
 */
const _moveOperationWithConfirmation = (
  model: CircuitModel,
  sourceLocation: string,
  targetLocation: string,
  sourceWire: number,
  targetWire: number,
  movingControl: boolean,
  insertNewColumn: boolean,
  renderFn: () => void,
): void => {
  const sourceOp = findOperation(model.componentGrid, sourceLocation);
  if (sourceOp != null && sourceOp.kind === "measurement") {
    const consumers = collectMeasurementConsumers(
      model.componentGrid,
      sourceLocation,
    );
    if (consumers.length > 0) {
      // Partition consumers by whether the M's new column comes
      // strictly before them in document order. The partition runs
      // in PRE-MOVE coordinates: `targetLocation` describes the
      // user's intended slot relative to the current grid, and
      // every consumer's location is also pre-move. Post-move
      // splicing doesn't change relative column ordering.
      const targetLocParsed = Location.parse(targetLocation);
      const survivors: { op: Operation; location: string }[] = [];
      const invalidated: { op: Operation; location: string }[] = [];
      for (const c of consumers) {
        const cLoc = Location.parse(c.location);
        if (targetLocParsed.inEarlierColumnThan(cLoc)) {
          survivors.push(c);
        } else {
          invalidated.push(c);
        }
      }

      const message = _buildMoveMConsumerMessage(
        survivors.length,
        invalidated.length,
      );
      _createConfirmPrompt(message, (confirmed) => {
        if (!confirmed) return;
        moveMeasurementWithDependents(
          model,
          sourceLocation,
          targetLocation,
          sourceWire,
          targetWire,
          insertNewColumn,
          invalidated.map((c) => c.op),
        );
        renderFn();
      });
      return;
    }
  }
  moveOperation(
    model,
    sourceLocation,
    targetLocation,
    sourceWire,
    targetWire,
    movingControl,
    insertNewColumn,
  );
  renderFn();
};

/**
 * Build the body text for the M-move confirmation prompt.
 *
 * Three shapes:
 *
 *   - Pure survivors (move-only): "N dependent operation(s)
 *     will be updated to reference the measurement's new wire".
 *   - Pure invalidated (delete-only): "would end up before this
 *     measurement and will be deleted".
 *   - Mixed: both clauses, separated, so the user knows exactly
 *     what each consumer gets.
 *
 * Singular vs. plural is handled per-clause; pluralization
 * heuristics use the standard English `-s` rule (every consumer
 * label here is "operation", which is regular).
 */
const _buildMoveMConsumerMessage = (
  survivors: number,
  invalidated: number,
): string => {
  const opWord = (n: number): string =>
    n === 1 ? "1 dependent operation" : `${n} dependent operations`;
  const willBeUpdated =
    survivors === 1
      ? `${opWord(survivors)} will be updated to reference this measurement's new wire`
      : `${opWord(survivors)} will be updated to reference this measurement's new wire`;
  const willBeDeleted =
    invalidated === 1
      ? `${opWord(invalidated)} would end up before this measurement in document order and will be deleted`
      : `${opWord(invalidated)} would end up before this measurement in document order and will be deleted`;

  if (survivors > 0 && invalidated > 0) {
    return `Moving this measurement: ${willBeUpdated}; ${willBeDeleted}. Continue?`;
  }
  if (survivors > 0) {
    return `Moving this measurement: ${willBeUpdated}. Continue?`;
  }
  // invalidated > 0 (the caller only enters this branch when
  // consumers.length > 0, so at least one bucket is non-empty).
  return `Moving this measurement: ${willBeDeleted}. Continue?`;
};

export { _deleteOperationWithConfirmation, _moveOperationWithConfirmation };
