// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { Parameter } from "./circuit.js";
import { removeControl, removeOperation } from "./circuitManipulation.js";
import { CircuitEvents } from "./events.js";
import { findGateElem, findOperation } from "./utils.js";
import {
  isValidAngleExpression,
  normalizeAngleExpression,
} from "./angleExpression.js";

/**
 * Adds a context menu to a host element in the circuit visualization.
 *
 * @param circuitEvents The CircuitEvents instance to handle circuit-related events.
 * @param hostElem The SVG element representing a gate component to which the context menu will be added.
 */
const addContextMenuToHostElem = (
  circuitEvents: CircuitEvents,
  hostElem: SVGGraphicsElement,
) => {
  hostElem?.addEventListener("contextmenu", (ev: MouseEvent) => {
    ev.preventDefault();

    // Remove any existing context menu
    const existingContextMenu = document.querySelector(".context-menu");
    if (existingContextMenu) {
      document.body.removeChild(existingContextMenu);
    }

    const gateElem = findGateElem(hostElem);
    if (!gateElem) return;
    const selectedLocation = gateElem.getAttribute("data-location");
    const selectedOperation = findOperation(
      circuitEvents.componentGrid,
      selectedLocation,
    );
    if (!selectedOperation || !selectedLocation) return;

    const contextMenu = document.createElement("div");
    contextMenu.classList.add("context-menu");
    contextMenu.style.top = `${ev.clientY + window.scrollY}px`;
    contextMenu.style.left = `${ev.clientX + window.scrollX}px`;
    contextMenu.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      e.stopPropagation();
    });
    contextMenu.addEventListener("mouseup", (e) => {
      e.preventDefault();
      e.stopPropagation();
    });

    const dataWireStr = hostElem.getAttribute("data-wire");
    const dataWire = dataWireStr != null ? parseInt(dataWireStr) : null;
    const isControl =
      hostElem.classList.contains("control-dot") && dataWire != null;

    const deleteOption = _createContextMenuItem("Delete", () => {
      removeOperation(circuitEvents, selectedLocation);
      circuitEvents.renderFn();
    });

    if (
      selectedOperation.kind === "measurement" ||
      selectedOperation.kind === "ket"
    ) {
      contextMenu.appendChild(deleteOption);
    } else if (isControl) {
      const removeControlOption = _createContextMenuItem(
        "Remove control",
        () => {
          removeControl(circuitEvents, selectedOperation, dataWire);
          circuitEvents.renderFn();
        },
      );
      contextMenu.appendChild(removeControlOption!);
    } else {
      const adjointOption = _createContextMenuItem("Toggle Adjoint", () => {
        if (selectedOperation.kind !== "unitary") return;
        selectedOperation.isAdjoint = !selectedOperation.isAdjoint;
        circuitEvents.renderFn();
      });

      const addControlOption = _createContextMenuItem("Add Control", () => {
        if (selectedOperation.kind !== "unitary") return;
        circuitEvents._startAddingControl(selectedOperation, selectedLocation);
      });

      let removeControlOption: HTMLDivElement | undefined;
      if (selectedOperation.controls && selectedOperation.controls.length > 0) {
        removeControlOption = _createContextMenuItem("Remove Control", () => {
          circuitEvents._startRemovingControl(selectedOperation);
        });
        contextMenu.appendChild(removeControlOption);
      }

      const promptArgOption = _createContextMenuItem("Edit Argument", () => {
        promptForArguments(
          selectedOperation.params!,
          selectedOperation.args,
        ).then((args) => {
          if (args.length > 0) {
            selectedOperation.args = args;
          } else {
            selectedOperation.args = undefined;
          }
          circuitEvents.renderFn();
        });
      });

      if (selectedOperation.gate == "X") {
        contextMenu.appendChild(addControlOption);
        if (removeControlOption) {
          contextMenu.appendChild(removeControlOption);
        }
        contextMenu.appendChild(deleteOption);
      } else {
        contextMenu.appendChild(adjointOption);
        contextMenu.appendChild(addControlOption);
        if (removeControlOption) {
          contextMenu.appendChild(removeControlOption);
        }
        if (
          selectedOperation.params !== undefined &&
          selectedOperation.params.length > 0
        ) {
          contextMenu.appendChild(promptArgOption);
        }
        contextMenu.appendChild(deleteOption);
      }
    }

    document.body.appendChild(contextMenu);

    document.addEventListener(
      "click",
      () => {
        if (document.body.contains(contextMenu)) {
          document.body.removeChild(contextMenu);
        }
      },
      { once: true },
    );
  });
};

/**
 * Prompt the user for argument values.
 * @param params - The parameters for which the user needs to provide values.
 * @param defaultArgs - The default values for the parameters, if any.
 * @returns A Promise that resolves with the user-provided arguments as an array of strings.
 */
const promptForArguments = (
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
 * Create a context menu item
 * @param text - The text to display in the menu item
 * @param onClick - The function to call when the menu item is clicked
 * @returns The created menu item element
 */
const _createContextMenuItem = (
  text: string,
  onClick: () => void,
): HTMLDivElement => {
  const menuItem = document.createElement("div");
  menuItem.classList.add("context-menu-option");
  menuItem.textContent = text;
  menuItem.addEventListener("click", onClick);
  return menuItem;
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

export { addContextMenuToHostElem, promptForArguments };
