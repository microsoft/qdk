// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { ComponentGrid, Operation, Qubit, Unitary } from "./circuit.js";
import { Sqore } from "./sqore.js";
import { toolboxGateDictionary } from "./panel.js";
import {
  getGateLocationString,
  findOperation,
  getToolboxElems,
  getGateElems,
  getHostElems,
  getWireData,
  locationStringToIndexes,
  findParentArray,
  deepEqual,
  getQubitLabelElems,
  getMinMaxRegIdx,
} from "./utils.js";
import { addContextMenuToHostElem, promptForArguments } from "./contextMenu.js";
import {
  removeTrailingUnusedQubits,
  addControl,
  addOperation,
  findAndRemoveOperations,
  moveOperation,
  removeControl,
  removeOperation,
  resolveOverlappingOperations,
} from "./circuitManipulation.js";
import {
  createGateGhost,
  createQubitLabelGhost,
  createWireDropzone,
  getColumnOffsetsAndWidths,
  makeDropzoneBox,
  removeAllWireDropzones,
} from "./draggable.js";
import { getOperationRegisters } from "../../src/utils.js";

let events: CircuitEvents | null = null;

/**
 * Creates and attaches the events that allow editing of the circuit.
 *
 * @param container     HTML element for rendering visualization into
 * @param sqore         Sqore object
 */
const enableEvents = (
  container: HTMLElement,
  sqore: Sqore,
  useRefresh: () => void,
): void => {
  if (events != null) {
    events.dispose();
  }
  events = new CircuitEvents(container, sqore, useRefresh);
};

class CircuitEvents {
  renderFn: () => void;
  componentGrid: ComponentGrid;
  qubits: Qubit[];
  qubitUseCounts: number[];
  private circuitSvg: SVGElement;
  private dropzoneLayer: SVGGElement;
  private ghostQubitLayer: SVGGElement;
  private temporaryDropzones: SVGElement[] = [];
  private wireData: number[];
  private columnXData: { xOffset: number; colWidth: number }[];
  private selectedOperation: Operation | null = null;
  private selectedWire: number | null = null;
  private movingControl: boolean = false;
  private mouseUpOnCircuit: boolean = false;
  private dragging: boolean = false;
  private disableLeftAutoScroll: boolean = false;

  constructor(
    private container: HTMLElement,
    sqore: Sqore,
    useRefresh: () => void,
  ) {
    this.renderFn = useRefresh;

    this.circuitSvg = container.querySelector("svg.qviz") as SVGElement;
    this.dropzoneLayer = container.querySelector(
      ".dropzone-layer",
    ) as SVGGElement;
    this.ghostQubitLayer = container.querySelector(
      ".ghost-qubit-layer",
    ) as SVGGElement;

    this.componentGrid = sqore.circuit.componentGrid;
    this.qubits = sqore.circuit.qubits;
    this.qubitUseCounts = new Array(this.qubits.length).fill(0);
    for (const column of this.componentGrid) {
      for (const op of column.components) {
        this.incrementQubitUseCountForOp(op);
      }
    }

    this.wireData = getWireData(this.container);
    this.columnXData = getColumnOffsetsAndWidths(this.container);
    removeTrailingUnusedQubits(this);

    if (this.qubits.length === 0) {
      this.ghostQubitLayer.style.display = "block";
    }

    this._addContextMenuEvent();
    this._addDropzoneLayerEvents();
    this._addHostElementsEvents();
    this._addGateElementsEvents();
    this._addToolboxElementsEvents();
    this._addDropzoneElementsEvents();
    this._addQubitLineEvents();
    this._addDocumentEvents();
  }

  /**
   * Dispose the CircuitEvents instance and remove event listeners
   */
  dispose() {
    this._removeToolboxElementsEvents();
    this._removeDocumentEvents();
  }

  /***************************
   * Events Adding Functions *
   ***************************/

  documentKeydownHandler = (ev: KeyboardEvent) => {
    const selectedLocation = this.selectedOperation
      ? getGateLocationString(this.selectedOperation)
      : null;
    if (ev.ctrlKey && selectedLocation) {
      this.container.classList.remove("moving");
      this.container.classList.add("copying");
    }
  };

  documentKeyupHandler = (ev: KeyboardEvent) => {
    const selectedLocation = this.selectedOperation
      ? getGateLocationString(this.selectedOperation)
      : null;
    if (ev.ctrlKey && selectedLocation) {
      this.container.classList.remove("copying");
      this.container.classList.add("moving");
    }
  };

  documentMousedownHandler = () => {
    removeAllWireDropzones(this.circuitSvg);
  };

  documentMouseupHandler = (ev: MouseEvent) => {
    const copying = ev.ctrlKey;
    this.container.classList.remove("moving", "copying");
    // Handle deleting operations that have been dragged outside the circuit
    if (!this.mouseUpOnCircuit && this.dragging && !copying) {
      const selectedLocation = this.selectedOperation
        ? getGateLocationString(this.selectedOperation)
        : null;
      if (this.selectedOperation != null && selectedLocation != null) {
        // We are dragging a gate with a location (not from toolbox) outside the circuit
        // If we are moving a control, remove it from the selectedOperation
        if (
          this.movingControl &&
          this.selectedOperation.kind === "unitary" &&
          this.selectedOperation.controls != null &&
          this.selectedWire != null
        ) {
          const controlIndex = this.selectedOperation.controls.findIndex(
            (control) => control.qubit === this.selectedWire,
          );
          if (controlIndex !== -1)
            this.selectedOperation.controls.splice(controlIndex, 1);
        } else {
          // Otherwise, remove the selectedOperation
          removeOperation(this, selectedLocation);
        }
        this.renderFn();
      } else if (this.selectedWire != null) {
        // If we are dragging a qubit line, remove it
        this.removeQubitLineWithConfirmation(this.selectedWire);
      }
    }

    this._resetState();
  };

  /**
   * Resets the internal state of the CircuitEvents instance after a drag or interaction.
   * Clears selection, flags, and removes any temporary dropzones from the DOM.
   * Note that this does not clear the selectedOperation as that is needed to be persistent
   * for context-menu selections.
   */
  _resetState() {
    this.selectedWire = null;
    this.movingControl = false;
    this.mouseUpOnCircuit = false;
    this.dragging = false;
    this.disableLeftAutoScroll = false;

    for (const dropzone of this.temporaryDropzones) {
      if (dropzone.parentNode) {
        dropzone.parentNode.removeChild(dropzone);
      }
    }
    this.temporaryDropzones = [];
  }

  /**
   * Enable auto-scrolling when dragging near the edges of the container
   */
  _enableAutoScroll() {
    const scrollSpeed = 10; // Pixels per frame
    const edgeThreshold = 50; // Distance from the edge to trigger scrolling

    // Utility function to find the nearest scrollable ancestor
    const getScrollableAncestor = (element: Element): HTMLElement => {
      let currentElement: Element | null = element;
      while (currentElement) {
        const overflowY = window.getComputedStyle(currentElement).overflowY;
        const overflowX = window.getComputedStyle(currentElement).overflowX;
        if (
          overflowY === "auto" ||
          overflowY === "scroll" ||
          overflowX === "auto" ||
          overflowX === "scroll"
        ) {
          return currentElement as HTMLElement;
        }
        currentElement = currentElement.parentElement;
      }
      return document.documentElement; // Fallback to the root element
    };

    const scrollableAncestor = getScrollableAncestor(this.circuitSvg);

    const onMouseMove = (ev: MouseEvent) => {
      const rect = scrollableAncestor.getBoundingClientRect();

      const topBoundary = rect.top;
      const bottomBoundary = rect.bottom;
      const leftBoundary = rect.left;
      const rightBoundary = rect.right;

      // If the mouse has moved past the left boundary, we want to re-enable left auto-scrolling
      if (
        this.disableLeftAutoScroll &&
        ev.clientX > leftBoundary + 3 * edgeThreshold
      ) {
        this.disableLeftAutoScroll = false;
      }

      // Check if the cursor is near the edges
      if (ev.clientY < topBoundary + edgeThreshold) {
        // Scroll up
        scrollableAncestor.scrollTop -= scrollSpeed;
      } else if (ev.clientY > bottomBoundary - edgeThreshold) {
        // Scroll down
        scrollableAncestor.scrollTop += scrollSpeed;
      }

      if (
        !this.disableLeftAutoScroll &&
        ev.clientX < leftBoundary + edgeThreshold
      ) {
        // Scroll left
        scrollableAncestor.scrollLeft -= scrollSpeed;
      } else if (ev.clientX > rightBoundary - edgeThreshold) {
        // Scroll right
        scrollableAncestor.scrollLeft += scrollSpeed;
      }
    };

    const onMouseUp = () => {
      // Remove the mousemove listener when dragging stops
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
    };

    // Add the mousemove listener when dragging starts
    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  }

  /**
   * Add events for document
   */
  _addDocumentEvents() {
    document.addEventListener("keydown", this.documentKeydownHandler);
    document.addEventListener("keyup", this.documentKeyupHandler);
    document.addEventListener("mouseup", this.documentMouseupHandler);
    document.addEventListener("mousedown", this.documentMousedownHandler);
  }

  /**
   * Remove events for document
   */
  _removeDocumentEvents() {
    document.removeEventListener("keydown", this.documentKeydownHandler);
    document.removeEventListener("keyup", this.documentKeyupHandler);
    document.removeEventListener("mouseup", this.documentMouseupHandler);
    document.removeEventListener("mousedown", this.documentMousedownHandler);
  }

  /**
   * Add events specifically for dropzoneLayer
   */
  _addDropzoneLayerEvents() {
    this.container.addEventListener("mouseup", () => {
      if (this.qubits.length !== 0) {
        this.ghostQubitLayer.style.display = "none";
      }
      this.dropzoneLayer.style.display = "none";
    });

    this.circuitSvg.addEventListener("mouseup", () => {
      this.mouseUpOnCircuit = true;
    });
  }

  /**
   * Disable contextmenu default behaviors
   */
  _addContextMenuEvent() {
    this.container.addEventListener("contextmenu", (ev: MouseEvent) => {
      ev.preventDefault();
    });
  }

  /**
   * Add events for circuit objects in the circuit
   */
  _addHostElementsEvents() {
    const elems = getHostElems(this.container);
    elems.forEach((elem) => {
      elem.addEventListener("mousedown", (ev: MouseEvent) => {
        if (ev.button !== 0) return;
        if (elem.classList.contains("control-dot")) {
          this.movingControl = true;
        }
        const selectedWireStr = elem.getAttribute("data-wire");
        this.selectedWire =
          selectedWireStr != null ? parseInt(selectedWireStr) : null;
      });

      addContextMenuToHostElem(this, elem);
    });
  }

  /**
   * Add events for circuit objects in the circuit
   */
  _addGateElementsEvents() {
    const elems = getGateElems(this.container);
    elems.forEach((elem) => {
      elem?.addEventListener("mousedown", (ev: MouseEvent) => {
        // Allow dragging even when initiated on the arg-button
        const argButtonElem = (ev.target as HTMLElement).closest(".arg-button");
        if (argButtonElem) {
          // Find the sibling element with the data-wire attribute
          const siblingWithWire =
            argButtonElem.parentElement?.querySelector("[data-wire]");
          if (siblingWithWire) {
            const selectedWireStr = siblingWithWire.getAttribute("data-wire");
            this.selectedWire =
              selectedWireStr != null ? parseInt(selectedWireStr) : null;
          }
        }

        let selectedLocation = null;
        if (elem.getAttribute("data-expanded") !== "true") {
          selectedLocation = elem.getAttribute("data-location");
          this.selectedOperation = findOperation(
            this.componentGrid,
            selectedLocation,
          );
        }
        if (ev.button !== 0) return;
        ev.stopPropagation();
        removeAllWireDropzones(this.circuitSvg);
        if (
          this.selectedOperation === null ||
          this.selectedWire === null ||
          !selectedLocation
        )
          return;

        // Add temporary dropzones specific to this operation
        const [minTarget, maxTarget] = getMinMaxRegIdx(
          this.selectedOperation,
          this.wireData.length,
        );
        for (let wire = minTarget; wire <= maxTarget; wire++) {
          if (wire === this.selectedWire) continue;
          const indexes = locationStringToIndexes(selectedLocation);
          const [colIndex, opIndex] = indexes[indexes.length - 1];
          const dropzone = makeDropzoneBox(
            colIndex,
            opIndex,
            this.columnXData,
            this.wireData,
            wire,
            false,
          );
          dropzone.addEventListener("mouseup", this.dropzoneMouseupHandler);
          this.temporaryDropzones.push(dropzone);
          this.dropzoneLayer.appendChild(dropzone);
        }

        this._createGhostElement(ev);

        // Make sure the selectedOperation has location data
        if (this.selectedOperation.dataAttributes == null) {
          this.selectedOperation.dataAttributes = {
            location: selectedLocation,
          };
        } else {
          this.selectedOperation.dataAttributes["location"] = selectedLocation;
        }

        this.container.classList.add("moving");
        this.ghostQubitLayer.style.display = "block";
        this.dropzoneLayer.style.display = "block";
      });

      // Enable arg-button behavior
      const argButtons = elem.querySelectorAll<SVGElement>(".arg-button");
      argButtons.forEach((argButton) => {
        argButton.classList.add("edit-mode");

        // Add click event to trigger promptForArguments
        argButton.addEventListener("click", async () => {
          if (this.selectedOperation == null) return;
          const params = this.selectedOperation.params;
          const displayArgs = argButton.textContent || "";
          if (params) {
            const args = await promptForArguments(params, [displayArgs]);
            if (args.length > 0) {
              this.selectedOperation.args = args;
              this.renderFn();
            }
          }
        });
      });
    });
  }

  toolboxMousedownHandler = (ev: MouseEvent) => {
    if (ev.button !== 0) return;
    this.container.classList.add("moving");
    this.ghostQubitLayer.style.display = "block";
    this.dropzoneLayer.style.display = "block";
    const elem = ev.currentTarget as HTMLElement;
    const type = elem.getAttribute("data-type");
    if (type == null) return;
    this.selectedOperation = toolboxGateDictionary[type];
    this.disableLeftAutoScroll = true;
    this._createGhostElement(ev);
  };

  /**
   * Add events for gates in the toolbox
   */
  _addToolboxElementsEvents() {
    const elems = getToolboxElems(this.container);
    elems.forEach((elem) => {
      elem.addEventListener("mousedown", this.toolboxMousedownHandler);
    });
  }

  /**
   * Remove events for gates in the toolbox
   */
  _removeToolboxElementsEvents() {
    const elems = getToolboxElems(this.container);
    elems.forEach((elem) => {
      elem.removeEventListener("mousedown", this.toolboxMousedownHandler);
    });
  }

  dropzoneMouseupHandler = async (ev: MouseEvent) => {
    const dropzoneElem = ev.currentTarget as SVGRectElement;
    const copying = ev.ctrlKey;
    // Create a deep copy of the component grid
    const originalGrid = JSON.parse(
      JSON.stringify(this.componentGrid),
    ) as ComponentGrid;
    const targetLoc = dropzoneElem.getAttribute("data-dropzone-location");
    const insertNewColumn =
      dropzoneElem.getAttribute("data-dropzone-inter-column") == "true" ||
      false;
    const targetWireStr = dropzoneElem.getAttribute("data-dropzone-wire");
    const targetWire = targetWireStr != null ? parseInt(targetWireStr) : null;

    if (
      targetLoc == null ||
      targetWire == null ||
      this.selectedOperation == null
    )
      return;
    const sourceLocation = getGateLocationString(this.selectedOperation);

    if (sourceLocation == null) {
      if (
        this.selectedOperation.params != undefined &&
        (this.selectedOperation.args === undefined ||
          this.selectedOperation.args.length === 0)
      ) {
        // Prompt for arguments and wait for user input
        const args = await promptForArguments(this.selectedOperation.params);
        if (!args || args.length === 0) {
          // User canceled the prompt, exit early
          return;
        }

        // Create a deep copy of the source operation
        this.selectedOperation = JSON.parse(
          JSON.stringify(this.selectedOperation),
        );
        if (this.selectedOperation == null) return;

        // Assign the arguments to the selected operation
        this.selectedOperation.args = args;
      }

      // Add a new operation from the toolbox
      addOperation(
        this,
        this.selectedOperation,
        targetLoc,
        targetWire,
        insertNewColumn,
      );
    } else if (sourceLocation && this.selectedWire != null) {
      if (copying) {
        if (this.movingControl && this.selectedOperation.kind === "unitary") {
          addControl(this, this.selectedOperation, targetWire);
          moveOperation(
            this,
            sourceLocation,
            targetLoc,
            this.selectedWire,
            targetWire,
            this.movingControl,
            insertNewColumn,
          );
        } else {
          addOperation(
            this,
            this.selectedOperation,
            targetLoc,
            targetWire,
            insertNewColumn,
          );
        }
      } else {
        moveOperation(
          this,
          sourceLocation,
          targetLoc,
          this.selectedWire,
          targetWire,
          this.movingControl,
          insertNewColumn,
        );
      }
    }

    this.selectedOperation = null;
    this._resetState();

    if (!deepEqual(originalGrid, this.componentGrid)) this.renderFn();
  };

  /**
   * Add events for dropzone elements
   */
  _addDropzoneElementsEvents() {
    const dropzoneElems =
      this.dropzoneLayer.querySelectorAll<SVGRectElement>(".dropzone");
    dropzoneElems.forEach((dropzoneElem) => {
      dropzoneElem.addEventListener("mouseup", this.dropzoneMouseupHandler);
    });
  }

  /**
   * Handler for mouseup on qubit line dropzones.
   * @param sourceWire The index of the source wire (being dragged).
   * @param targetWire The index of the target wire (drop location).
   * @param isBetween If true, creates a dropzone between wires.
   */
  qubitDropzoneMouseupHandler = (
    sourceWire: number,
    targetWire: number,
    isBetween: boolean,
  ) => {
    if (sourceWire === targetWire || sourceWire == null || targetWire == null)
      return;

    // Helper to move an array element
    function moveArrayElement<T>(arr: T[], from: number, to: number) {
      const el = arr.splice(from, 1)[0];
      arr.splice(to, 0, el);
    }

    // Update qubits array
    if (isBetween) {
      // Moving sourceWire to just before targetWire
      let insertAt = targetWire;
      // If moving down and passing over itself, adjust index
      if (sourceWire < insertAt) insertAt--;
      moveArrayElement(this.qubits, sourceWire, insertAt);
      moveArrayElement(this.qubitUseCounts, sourceWire, insertAt);
    } else {
      // Swap sourceWire and targetWire
      [this.qubits[sourceWire], this.qubits[targetWire]] = [
        this.qubits[targetWire],
        this.qubits[sourceWire],
      ];
      [this.qubitUseCounts[sourceWire], this.qubitUseCounts[targetWire]] = [
        this.qubitUseCounts[targetWire],
        this.qubitUseCounts[sourceWire],
      ];
    }

    // Update qubit ids to match their new positions
    this.qubits.forEach((q, idx) => {
      q.id = idx;
    });

    // Update all operations in componentGrid to reflect new qubit order
    for (const column of this.componentGrid) {
      for (const op of column.components) {
        getOperationRegisters(op).forEach((reg) => {
          if (isBetween) {
            // Move: update qubit indices
            if (reg.qubit === sourceWire) {
              reg.qubit = sourceWire < targetWire ? targetWire - 1 : targetWire;
            } else if (
              sourceWire < targetWire &&
              reg.qubit > sourceWire &&
              reg.qubit < targetWire
            ) {
              reg.qubit -= 1;
            } else if (
              sourceWire > targetWire &&
              reg.qubit >= targetWire &&
              reg.qubit < sourceWire
            ) {
              reg.qubit += 1;
            }
          } else {
            // Swap: swap indices
            if (reg.qubit === sourceWire) reg.qubit = targetWire;
            else if (reg.qubit === targetWire) reg.qubit = sourceWire;
          }
        });
      }
      // Sort operations in this column by their lowest-numbered register
      column.components.sort((a, b) => {
        const aRegs = getOperationRegisters(a);
        const bRegs = getOperationRegisters(b);
        const aMin = Math.min(...aRegs.map((r) => r.qubit));
        const bMin = Math.min(...bRegs.map((r) => r.qubit));
        return aMin - bMin;
      });
    }

    // Resolve overlapping operations into their own columns
    resolveOverlappingOperations(this.componentGrid);
    removeTrailingUnusedQubits(this);
    this.renderFn();
  };

  /**
   * Add events for qubit line labels
   */
  _addQubitLineEvents() {
    const elems = getQubitLabelElems(this.container);
    elems.forEach((elem) => {
      elem.addEventListener("mousedown", (ev: MouseEvent) => {
        ev.stopPropagation();
        this.selectedOperation = null;
        this._createQubitLabelGhost(ev, elem);

        const sourceIndexStr = elem.getAttribute("data-wire");
        const sourceWire =
          sourceIndexStr != null ? parseInt(sourceIndexStr) : null;
        if (sourceWire == null) return;
        this.selectedWire = sourceWire;

        // Dropzones ON each wire (skip self)
        // Exclude ghost qubit line (last wire)
        for (
          let targetWire = 0;
          targetWire < this.wireData.length - 1; // Exclude ghost
          targetWire++
        ) {
          if (targetWire === sourceWire) continue;
          const dropzone = createWireDropzone(
            this.circuitSvg,
            this.wireData,
            targetWire,
          );
          dropzone.addEventListener("mouseup", () =>
            this.qubitDropzoneMouseupHandler(sourceWire, targetWire, false),
          );
          this.temporaryDropzones.push(dropzone);
          this.circuitSvg.appendChild(dropzone);
        }

        // Dropzones BETWEEN wires (including before first and after last)
        // Exclude after ghost qubit line
        for (let i = 0; i <= this.wireData.length - 1; i++) {
          // Optionally, skip if dropping "between" at the source wire's own position
          if (i === sourceWire || i === sourceWire + 1) continue;
          const dropzone = createWireDropzone(
            this.circuitSvg,
            this.wireData,
            i,
            true,
          );
          dropzone.addEventListener("mouseup", () =>
            this.qubitDropzoneMouseupHandler(sourceWire, i, true),
          );
          this.temporaryDropzones.push(dropzone);
          this.circuitSvg.appendChild(dropzone);
        }
      });
      elem.style.pointerEvents = "all";
    });
  }

  /**
   * Removes a qubit line by index, with confirmation if it has associated operations.
   * @param qubitIdx The index of the qubit to remove.
   */
  removeQubitLineWithConfirmation(qubitIdx: number) {
    // Count number of operations associated with the qubit line
    const numOperations = this.qubitUseCounts[qubitIdx];

    const doRemove = () => {
      // Remove the qubit
      this.qubits.splice(qubitIdx, 1);
      this.qubitUseCounts.splice(qubitIdx, 1);
      this.wireData.splice(qubitIdx, 1);
      removeTrailingUnusedQubits(this);

      // Update all remaining operation references
      for (const column of this.componentGrid) {
        for (const op of column.components) {
          getOperationRegisters(op).forEach((reg) => {
            if (reg.qubit > qubitIdx) reg.qubit -= 1;
          });
        }
      }

      // Update qubit ids to match their new positions
      this.qubits.forEach((q, idx) => {
        q.id = idx;
      });

      this.renderFn();
    };

    if (numOperations === 0) {
      doRemove();
    } else {
      const message =
        numOperations === 1
          ? `There is 1 operation associated with this qubit line. Do you want to remove it?`
          : `There are ${numOperations} operations associated with this qubit line. Do you want to remove them?`;
      _createConfirmPrompt(message, (confirmed) => {
        if (confirmed) {
          findAndRemoveOperations(this, (op: Operation) =>
            getOperationRegisters(op).some((reg) => reg.qubit == qubitIdx),
          );
          doRemove();
        }
      });
    }
  }

  /*****************
   *     Misc.     *
   *****************/

  /**
   * Increment the use count for qubits used by the operation.
   * @param op The operation to increment use counts for.
   */
  incrementQubitUseCountForOp(op: Operation) {
    getOperationRegisters(op).forEach((reg) => {
      if (
        reg.result === undefined &&
        reg.qubit >= 0 &&
        reg.qubit < this.qubitUseCounts.length
      ) {
        this.qubitUseCounts[reg.qubit]++;
      }
    });
  }

  /**
   * Decrement the use count for qubits used by the operation.
   * @param op The operation to decrement the use count for.
   */
  decrementQubitUseCountForOp(op: Operation) {
    getOperationRegisters(op).forEach((reg) => {
      if (
        reg.result === undefined &&
        reg.qubit >= 0 &&
        reg.qubit < this.qubitUseCounts.length
      ) {
        this.qubitUseCounts[reg.qubit]--;
      }
    });
  }

  /**
   * Creates a ghost element for visual feedback during dragging.
   * This function initializes the dragging state, enables auto-scrolling,
   * and creates a visual representation of the selected operation.
   *
   * @param ev - The mouse event that triggered the creation of the ghost element.
   */
  _createGhostElement(ev: MouseEvent) {
    if (this.selectedOperation == null) return;
    this.dragging = true;
    this._enableAutoScroll();
    createGateGhost(
      ev,
      this.container,
      this.selectedOperation,
      this.movingControl,
    );
  }

  _createQubitLabelGhost(ev: MouseEvent, elem: SVGTextElement) {
    this.dragging = true;
    this._enableAutoScroll();
    createQubitLabelGhost(ev, this.container, elem);
  }

  /**
   * Start the process of adding a control to the selected operation.
   * This function creates dropzones for each wire that isn't already a target or control.
   *
   * @param selectedOperation - The unitary operation to which the control will be added.
   * @param selectedLocation - The location string of the selected operation.
   */
  _startAddingControl(selectedOperation: Unitary, selectedLocation: string) {
    this.selectedOperation = selectedOperation;
    this.container.classList.add("adding-control");
    this.ghostQubitLayer.style.display = "block";

    // Create dropzones for each wire that isn't already a target or control
    for (let wireIndex = 0; wireIndex < this.wireData.length; wireIndex++) {
      const isTarget = this.selectedOperation?.targets.some(
        (target) => target.qubit === wireIndex,
      );
      const isControl = this.selectedOperation?.controls?.some(
        (control) => control.qubit === wireIndex,
      );

      if (!isTarget && !isControl) {
        const dropzone = createWireDropzone(
          this.circuitSvg,
          this.wireData,
          wireIndex,
        );
        dropzone.addEventListener("mousedown", (ev: MouseEvent) =>
          ev.stopPropagation(),
        );
        dropzone.addEventListener("click", () => {
          if (
            this.selectedOperation != null &&
            this.selectedOperation.kind === "unitary"
          ) {
            const successful = addControl(
              this,
              this.selectedOperation,
              wireIndex,
            );
            this.selectedOperation = null;
            this.container.classList.remove("adding-control");
            this.ghostQubitLayer.style.display = "none";
            if (successful) {
              const indexes = locationStringToIndexes(selectedLocation);
              const [columnIndex, position] = indexes[indexes.length - 1];
              const selectedOperationParent = findParentArray(
                this.componentGrid,
                selectedLocation,
              );
              if (!selectedOperationParent) return;

              const [minTarget, maxTarget] = getMinMaxRegIdx(
                selectedOperation,
                this.wireData.length,
              );
              selectedOperationParent[columnIndex].components.forEach(
                (op, opIndex) => {
                  if (opIndex === position) return; // Don't check the selected operation against itself
                  const [minOp, maxOp] = getMinMaxRegIdx(
                    op,
                    this.wireData.length,
                  );
                  // Check if selectedOperation's range overlaps with op's range
                  if (
                    (minOp >= minTarget && minOp <= maxTarget) ||
                    (maxOp >= minTarget && maxOp <= maxTarget) ||
                    (minTarget >= minOp && minTarget <= minOp) ||
                    (maxTarget >= maxOp && maxTarget <= maxOp)
                  ) {
                    // If they overlap, move the operation
                    selectedOperationParent[columnIndex].components.splice(
                      position,
                      1,
                    );
                    // Not sure if this check is needed as we already know there
                    // should be other operations in this column.
                    if (
                      selectedOperationParent[columnIndex].components.length ===
                      0
                    ) {
                      selectedOperationParent.splice(columnIndex, 1);
                    }

                    selectedOperationParent.splice(columnIndex, 0, {
                      components: [selectedOperation],
                    });
                  }
                },
              );

              this.renderFn();
            }
          }
        });
        this.circuitSvg.appendChild(dropzone);
      }
    }
  }

  /**
   * Start the process of removing a control from the selected operation.
   * This function creates dropzones only for wires that the selected operation has a control.
   *
   * @param selectedOperation - The unitary operation from which the control will be removed.
   */
  _startRemovingControl(selectedOperation: Unitary) {
    this.selectedOperation = selectedOperation;
    this.container.classList.add("removing-control");

    // Create dropzones only for wires that the selectedOperation has a control
    this.selectedOperation.controls?.forEach((control) => {
      const dropzone = createWireDropzone(
        this.circuitSvg,
        this.wireData,
        control.qubit,
      );
      dropzone.addEventListener("mousedown", (ev: MouseEvent) =>
        ev.stopPropagation(),
      );
      dropzone.addEventListener("click", () => {
        if (
          this.selectedOperation != null &&
          this.selectedOperation.kind === "unitary"
        ) {
          const successful = removeControl(
            this,
            this.selectedOperation,
            control.qubit,
          );
          this.selectedOperation = null;
          this.container.classList.remove("removing-control");
          if (successful) {
            this.renderFn();
          }
        }
      });
      this.circuitSvg.appendChild(dropzone);
    });
  }
}

/**
 * Create a confirm dialog box
 * @param message - The message to display in the confirm dialog
 * @param callback - The callback function to handle the user's response (true for OK, false for Cancel)
 */
const _createConfirmPrompt = (
  message: string,
  callback: (confirmed: boolean) => void,
) => {
  // Create the confirm overlay
  const overlay = document.createElement("div");
  overlay.classList.add("prompt-overlay");
  overlay.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    e.stopPropagation();
  });

  // Create the confirm container
  const confirmContainer = document.createElement("div");
  confirmContainer.classList.add("prompt-container");

  // Create the message element
  const messageElem = document.createElement("div");
  messageElem.classList.add("prompt-message");
  messageElem.textContent = message;

  // Create the buttons container
  const buttonsContainer = document.createElement("div");
  buttonsContainer.classList.add("prompt-buttons");

  // Create the OK button
  const okButton = document.createElement("button");
  okButton.classList.add("prompt-button");
  okButton.textContent = "OK";
  okButton.addEventListener("click", () => {
    callback(true); // User confirmed
    document.body.removeChild(overlay);
    document.removeEventListener("keydown", handleGlobalKeyDown, true);
  });

  // Create the Cancel button
  const cancelButton = document.createElement("button");
  cancelButton.classList.add("prompt-button");
  cancelButton.textContent = "Cancel";
  cancelButton.addEventListener("click", () => {
    callback(false); // User canceled
    document.body.removeChild(overlay);
    document.removeEventListener("keydown", handleGlobalKeyDown, true);
  });

  // Handle Enter and Escape keys globally while prompt is open
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

  // Remove focus from any currently focused element
  if (document.activeElement) {
    (document.activeElement as HTMLElement).blur();
  }
};

export { enableEvents, CircuitEvents };
