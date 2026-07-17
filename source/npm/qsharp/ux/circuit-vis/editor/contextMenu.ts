// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import {
  _isMultiTargetOrGroup,
  removeControl,
} from "../actions/circuitActions.js";
import {
  deleteOperationWithConfirmation,
  promptForArguments,
} from "./prompts.js";
import { CircuitEvents } from "./events.js";
import { findGateElem } from "./domUtils.js";
import { findOperation } from "../utils.js";

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
      // Route through the prompt-aware wrapper so deleting a measurement with downstream consumers
      // confirms first.
      deleteOperationWithConfirmation(
        circuitEvents.model,
        selectedLocation,
        circuitEvents.renderFn,
      );
    });

    if (
      selectedOperation.kind === "measurement" ||
      selectedOperation.kind === "ket"
    ) {
      contextMenu.appendChild(deleteOption);
    } else if (isControl) {
      // Hide "Remove control" when the parent op is multi-target / a group, mirroring the
      // action-layer gating in `_isMultiTargetOrGroup`. Existing controls can still be moved via
      // control-drag.
      if (!_isMultiTargetOrGroup(selectedOperation)) {
        const removeControlOption = _createContextMenuItem(
          "Remove control",
          () => {
            removeControl(circuitEvents.model, selectedOperation, dataWire);
            circuitEvents.renderFn();
          },
        );
        contextMenu.appendChild(removeControlOption!);
      }
    } else {
      const adjointOption = _createContextMenuItem("Toggle Adjoint", () => {
        if (selectedOperation.kind !== "unitary") return;
        selectedOperation.isAdjoint = !selectedOperation.isAdjoint;
        circuitEvents.renderFn();
      });

      // Multi-target unitaries and groups don't get Add / Remove Control: groups carry no quantum
      // controls and multi-target bodies have no canonical control attachment point. Mirrors
      // `_isMultiTargetOrGroup` at the action layer.
      const allowControlAuthoring = !_isMultiTargetOrGroup(selectedOperation);

      // Groups (any op with `children`) don't get "Toggle Adjoint": adjointing a group would have
      // to propagate the marker through the subtree, and groups with a measurement or Reset aren't
      // adjointable at all.
      const allowAdjoint = selectedOperation.children == null;

      const addControlOption = _createContextMenuItem("Add Control", () => {
        if (selectedOperation.kind !== "unitary") return;
        circuitEvents._startAddingControl(selectedOperation);
      });

      let removeControlOption: HTMLDivElement | undefined;
      if (
        allowControlAuthoring &&
        selectedOperation.controls &&
        selectedOperation.controls.length > 0
      ) {
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
        if (allowControlAuthoring) {
          contextMenu.appendChild(addControlOption);
        }
        if (removeControlOption) {
          contextMenu.appendChild(removeControlOption);
        }
        contextMenu.appendChild(deleteOption);
      } else {
        if (allowAdjoint) {
          contextMenu.appendChild(adjointOption);
        }
        if (allowControlAuthoring) {
          contextMenu.appendChild(addControlOption);
        }
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

export { addContextMenuToHostElem };
