// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// Home for the editor's prompt dialogs: the confirm-dialog and text-input primitives, plus the
// delete/move confirmation flows and the argument-collection flow that use them.

import {
  collectSubtreeConsumers,
  moveOperationWithDependents,
  countStrandedConsumers,
  removeOperation,
  removeOperationWithDependents,
} from "../actions/circuitActions.js";
import { CircuitModel } from "../data/circuitModel.js";
import { Parameter } from "../data/circuit.js";
import {
  isValidAngleExpression,
  normalizeAngleExpression,
} from "../angleExpression.js";

/**
 * Confirm-dialog primitive used by destructive editor flows (currently only "remove a qubit line
 * that has operations attached").
 *
 * Standalone so individual controllers can use it without depending on the full `CircuitEvents`
 * class.
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

  // Handle Enter (commit) and Escape (cancel) globally while the prompt is open. Capture-phase so
  // we don't fight any descendant handlers in the editor surface.
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

  // Drop focus from whatever was focused so Enter/Escape go through our document-level handler
  // instead of any input that had it.
  if (document.activeElement) {
    (document.activeElement as HTMLElement).blur();
  }
};

/**
 * Delete an operation. If it would strand downstream classical consumers (ops referencing a
 * measurement result produced inside the deleted subtree), prompt first and cascade-delete them on
 * confirm; otherwise pass straight through to `removeOperation`. Kind-agnostic — one prompt covers a
 * group carrying any number of producers.
 *
 * `renderFn` runs once on every mutating path; on cancel nothing mutates and it is NOT called.
 */
export const deleteOperationWithConfirmation = (
  model: CircuitModel,
  location: string,
  renderFn: () => void,
): void => {
  const consumers = collectSubtreeConsumers(model.componentGrid, location);
  if (consumers.length > 0) {
    const message = _buildConsumerCascadeMessage("Deleting", consumers.length);
    createConfirmPrompt(message, (confirmed) => {
      if (!confirmed) return;
      removeOperationWithDependents(
        model,
        location,
        consumers.map((c) => c.op),
      );
      renderFn();
    });
    return;
  }
  removeOperation(model, location);
  renderFn();
};

/**
 * Move an operation. Previews the move on a clone to find any downstream classical consumers it
 * would strand: strands none — commit directly, no prompt; strands some — prompt, then commit and
 * cascade-delete them on confirm. One prompt covers the whole move; surviving consumers are
 * repointed by the move's token pass.
 *
 * `movingControl` MUST be threaded through unchanged — the drag controller routes control-dot drags
 * through here, and hardcoding `false` would make `_moveY` rewrite the op onto the control's wire
 * (turning CNOT(target=q1, ctrl=q0) into a self-controlled X on q0).
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
  const strandedCount = countStrandedConsumers(
    model,
    sourceLocation,
    targetLocation,
    sourceWire,
    targetWire,
    movingControl,
    insertNewColumn,
  );

  const commit = () => {
    moveOperationWithDependents(
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

  if (strandedCount === 0) {
    commit();
    return;
  }

  const message = _buildConsumerCascadeMessage("Moving", strandedCount);
  createConfirmPrompt(message, (confirmed) => {
    if (!confirmed) return;
    commit();
  });
};

/**
 * Build the confirmation body for an action that will cascade-delete `count` stranded classical-ref
 * consumers. `verb` names the action ("Moving" / "Deleting").
 */
const _buildConsumerCascadeMessage = (verb: string, count: number): string => {
  const clause =
    count === 1
      ? "1 dependent operation that references a measurement result"
      : `${count} dependent operations that reference a measurement result`;
  return `${verb} this operation will also delete ${clause}. Continue?`;
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
