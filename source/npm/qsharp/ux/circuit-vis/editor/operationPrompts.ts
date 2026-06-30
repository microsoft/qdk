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
 * measurement is removed along with every dependent op. The
 * non-measurement / no-consumer paths pass straight through to
 * [`removeOperation`](../actions/circuitActions.ts).
 *
 * `renderFn` runs once on every path that mutates the model. On
 * cancel, nothing mutates and `renderFn` is NOT called.
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
 * classical consumers, prompt before committing: on confirm, the
 * move remaps the classical refs of consumers that stay after the
 * M's new column and cascade-deletes any that would end up
 * at-or-before it. Non-measurement / no-consumer paths pass straight
 * through to [`moveOperation`](../actions/circuitActions.ts).
 *
 * `movingControl` MUST be threaded through unchanged. The drag
 * controller routes every non-clone drag through here, including
 * control-dot drags on ordinary unitaries; hardcoding `false` would
 * make `_moveY`'s single-leg branch rewrite the op onto the
 * control's wire (turning CNOT(target=q1, ctrl=q0) into a
 * self-controlled X on q0). The M-consumer path passes `false` to
 * `moveMeasurementWithDependents` since Ms have no `controls`.
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
      // strictly before them. Runs in pre-move coordinates, which is
      // sound since splicing doesn't change relative column ordering.
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
 * Build the body text for the M-move confirmation prompt. Emits a
 * move-only, delete-only, or combined clause depending on which
 * consumer buckets are non-empty, pluralized per-clause.
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
