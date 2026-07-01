// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Home for the editor's prompt dialogs: the confirm-dialog and
// text-input primitives, plus the delete/move confirmation flows
// and the argument-collection flow that use them.

import {
  collectMeasurementConsumers,
  moveMeasurementWithDependents,
  moveOperation,
  removeMeasurementWithDependents,
  removeOperation,
} from "../actions/circuitActions.js";
import { CircuitModel } from "../data/circuitModel.js";
import { Location } from "../data/location.js";
import { Operation, Parameter } from "../data/circuit.js";
import { findOperation } from "../utils.js";
import {
  isValidAngleExpression,
  normalizeAngleExpression,
} from "../angleExpression.js";

/**
 * Confirm-dialog primitive used by destructive editor flows
 * (currently only "remove a qubit line that has operations attached").
 *
 * Standalone so individual controllers can use it without depending
 * on the full `CircuitEvents` class.
 *
 * @param message - Text shown in the prompt body.
 * @param callback - Invoked with `true` on OK, `false` on Cancel.
 */
export const createConfirmPrompt = (
  message: string,
  callback: (confirmed: boolean) => void,
) => {
  const overlay = document.createElement("div");
  overlay.classList.add("prompt-overlay");
  overlay.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    e.stopPropagation();
  });

  const confirmContainer = document.createElement("div");
  confirmContainer.classList.add("prompt-container");

  const messageElem = document.createElement("div");
  messageElem.classList.add("prompt-message");
  messageElem.textContent = message;

  const buttonsContainer = document.createElement("div");
  buttonsContainer.classList.add("prompt-buttons");

  const okButton = document.createElement("button");
  okButton.classList.add("prompt-button");
  okButton.textContent = "OK";
  okButton.addEventListener("click", () => {
    callback(true);
    document.body.removeChild(overlay);
    document.removeEventListener("keydown", handleGlobalKeyDown, true);
  });

  const cancelButton = document.createElement("button");
  cancelButton.classList.add("prompt-button");
  cancelButton.textContent = "Cancel";
  cancelButton.addEventListener("click", () => {
    callback(false);
    document.body.removeChild(overlay);
    document.removeEventListener("keydown", handleGlobalKeyDown, true);
  });

  // Handle Enter (commit) and Escape (cancel) globally while the
  // prompt is open. Capture-phase so we don't fight any descendant
  // handlers in the editor surface.
  const handleGlobalKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Enter") {
      event.preventDefault();
      okButton.click();
    } else if (event.key === "Escape") {
      event.preventDefault();
      cancelButton.click();
    }
  };
  document.addEventListener("keydown", handleGlobalKeyDown, true);

  buttonsContainer.appendChild(okButton);
  buttonsContainer.appendChild(cancelButton);

  confirmContainer.appendChild(messageElem);
  confirmContainer.appendChild(buttonsContainer);

  overlay.appendChild(confirmContainer);
  document.body.appendChild(overlay);

  // Drop focus from whatever was focused so Enter/Escape go through
  // our document-level handler instead of any input that had it.
  if (document.activeElement) {
    (document.activeElement as HTMLElement).blur();
  }
};

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
export const deleteOperationWithConfirmation = (
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
      createConfirmPrompt(message, (confirmed) => {
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
export const moveOperationWithConfirmation = (
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
      createConfirmPrompt(message, (confirmed) => {
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

/**
 * Prompt the user for argument values.
 * @param params - The parameters for which the user needs to provide values.
 * @param defaultArgs - The default values for the parameters, if any.
 * @returns A Promise that resolves with the user-provided arguments as an array of strings.
 */
export const promptForArguments = (
  params: Parameter[],
  defaultArgs: string[] = [],
): Promise<string[]> => {
  return new Promise((resolve) => {
    const collectedArgs: string[] = [];
    let currentIndex = 0;

    const promptNext = () => {
      if (currentIndex >= params.length) {
        resolve(collectedArgs);
        return;
      }

      const param = params[currentIndex];
      const defaultValue = defaultArgs[currentIndex] || "";

      _createInputPrompt(
        `Enter value for parameter "${param.name}":`,
        (userInput) => {
          if (userInput !== null) {
            collectedArgs.push(userInput);
            currentIndex++;
            promptNext();
          } else {
            resolve(defaultArgs); // User canceled the prompt
          }
        },
        defaultValue,
        isValidAngleExpression,
        'Examples: "2.0 * π" or "π / 2.0"',
      );
    };

    promptNext();
  });
};

/**
 * Create a user input prompt element
 * @param message - The message to display in the prompt
 * @param callback - The callback function to handle the user input
 * @param defaultValue - The default value to display in the input element
 * @param validateInput - A function to validate the user input
 * @param placeholder - The placeholder text for the input element
 */
const _createInputPrompt = (
  message: string,
  callback: (input: string | null) => void,
  defaultValue: string = "",
  validateInput: (input: string) => boolean = () => true,
  placeholder: string = "",
) => {
  // Create the prompt overlay
  const overlay = document.createElement("div");
  overlay.classList.add("prompt-overlay");
  overlay.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    e.stopPropagation();
  });

  // Create the prompt container
  const promptContainer = document.createElement("div");
  promptContainer.classList.add("prompt-container");

  // Create the message element
  const messageElem = document.createElement("div");
  messageElem.classList.add("prompt-message");
  messageElem.textContent = message;

  // Create the input element
  const inputElem = document.createElement("input");
  inputElem.classList.add("prompt-input");
  inputElem.type = "text";
  inputElem.value = defaultValue;
  inputElem.placeholder = placeholder;

  // Create the buttons container
  const buttonsContainer = document.createElement("div");
  buttonsContainer.classList.add("prompt-buttons");

  // Create the OK button
  const okButton = document.createElement("button");
  okButton.classList.add("prompt-button");
  okButton.textContent = "OK";

  // Function to validate input and toggle the OK button
  const validateAndToggleOkButton = () => {
    const processedInput = normalizeAngleExpression(inputElem.value);
    const isValid = validateInput(processedInput);
    okButton.disabled = !isValid;
  };

  // Add input event listener for validation
  inputElem.addEventListener("input", validateAndToggleOkButton);

  // Handle Enter key when input is focused
  inputElem.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && !okButton.disabled) {
      event.preventDefault();
      okButton.click();
    }
  });

  okButton.disabled = !validateInput(normalizeAngleExpression(defaultValue));
  okButton.addEventListener("click", () => {
    callback(normalizeAngleExpression(inputElem.value));
    document.body.removeChild(overlay);
    document.removeEventListener("keydown", handleGlobalKeyDown, true);
  });

  // Create the π button
  const piButton = document.createElement("button");
  piButton.textContent = "π";
  piButton.classList.add("pi-button", "prompt-button");
  piButton.addEventListener("click", () => {
    const cursorPosition = inputElem.selectionStart || 0;
    const textBefore = inputElem.value.substring(0, cursorPosition);
    const textAfter = inputElem.value.substring(cursorPosition);
    inputElem.value = `${textBefore}π${textAfter}`;
    inputElem.focus();
    inputElem.setSelectionRange(cursorPosition + 1, cursorPosition + 1); // Move cursor after "π"
    validateAndToggleOkButton();
  });

  // Create the Cancel button
  const cancelButton = document.createElement("button");
  cancelButton.classList.add("prompt-button");
  cancelButton.textContent = "Cancel";
  cancelButton.addEventListener("click", () => {
    callback(null);
    document.body.removeChild(overlay);
    document.removeEventListener("keydown", handleGlobalKeyDown, true);
  });

  // Handle Escape key globally while prompt is open
  const handleGlobalKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      cancelButton.click();
    }
  };
  document.addEventListener("keydown", handleGlobalKeyDown, true);

  // Append buttons to the container
  buttonsContainer.appendChild(piButton);
  buttonsContainer.appendChild(okButton);
  buttonsContainer.appendChild(cancelButton);

  // Append elements to the prompt container
  promptContainer.appendChild(messageElem);
  promptContainer.appendChild(inputElem);
  promptContainer.appendChild(buttonsContainer);

  // Append the prompt container to the overlay
  overlay.appendChild(promptContainer);

  // Append the overlay to the document body
  document.body.appendChild(overlay);

  // Focus the input element
  inputElem.focus();
};
