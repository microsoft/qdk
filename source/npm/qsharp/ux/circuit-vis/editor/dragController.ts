// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { ComponentGrid, Unitary } from "../data/circuit.js";
import {
  addControl,
  addOperation,
  moveOperation,
  removeControl,
  removeOperation,
} from "../actions/circuitActions.js";
import {
  createGateGhost,
  createWireDropzone,
  makeDropzoneBox,
  removeAllWireDropzones,
} from "./draggable.js";
import {
  beginToolboxDrag,
  resetTransient,
  trackTemporaryDropzone,
} from "../actions/interactionActions.js";
import { InteractionContext } from "./interactionContext.js";
import { Location } from "../data/location.js";
import { promptForArguments } from "./contextMenu.js";
import { QubitController } from "./qubitController.js";
import { enableAutoScroll } from "./scrollController.js";
import { toolboxGateDictionary } from "./toolboxGates.js";
import {
  deepEqual,
  findOperation,
  findParentArray,
  getGateElems,
  getGateLocationString,
  getMinMaxRegIdx,
  getToolboxElems,
} from "../utils.js";

/**
 * `DragController` — owns the gate drag-and-drop surface. Carved
 * out of `CircuitEvents` in R5. By far the largest controller:
 * gate-drag, toolbox-drag, dropzone commit, document-level
 * cleanup/cancel, ghost element creation, and the wire-pick
 * dropzones used by the add-control / remove-control flow that
 * the context menu invokes.
 *
 * Why one controller for so much. These flows all share the same
 * dropzone overlay, the same ghost element, the same
 * `interaction` flags (`dragging`, `mouseUpOnCircuit`,
 * `selectedOperation`, `selectedWire`, `movingControl`) and the
 * same document-level mouseup that decides whether a drag was a
 * commit, a cancel, or a drag-out-delete. Splitting them further
 * would multiply the cross-controller plumbing without separating
 * any real concerns.
 *
 * Cross-controller dependencies:
 *
 * - Holds a `QubitController` reference for the one document-mouseup
 *   path that detects a qubit-label drag-off and calls
 *   `removeQubitLineWithConfirmation`. Avoids the alternative of
 *   either putting that logic on the qubit controller (which would
 *   need its own document-mouseup handler racing with this one) or
 *   on `CircuitEvents` (which would need a non-trivial method
 *   again, defeating the point of R5).
 */
export class DragController {
  constructor(
    private readonly ctx: InteractionContext,
    private readonly qubitController: QubitController,
  ) {
    this.installLayerListeners();
    this.installGateListeners();
    this.installToolboxListeners();
    this.installDropzoneListeners();
    this.installDocumentListeners();
  }

  dispose(): void {
    this.uninstallToolboxListeners();
    this.uninstallDocumentListeners();
  }

  /**
   * Begin the wire-pick flow that lets the user click a wire to add
   * a control to `selectedOperation`. Called from the context menu.
   */
  startAddingControl(selectedOperation: Unitary, selectedLocation: string) {
    this.ctx.interaction.selectedOperation = selectedOperation;
    this.ctx.container.classList.add("adding-control");
    this.ctx.ghostQubitLayer.style.display = "block";

    for (let wireIndex = 0; wireIndex < this.ctx.wireData.length; wireIndex++) {
      const isTarget = this.ctx.interaction.selectedOperation?.targets.some(
        (target) => target.qubit === wireIndex,
      );
      const isControl = this.ctx.interaction.selectedOperation?.controls?.some(
        (control) => control.qubit === wireIndex,
      );
      if (isTarget || isControl) continue;

      const dropzone = createWireDropzone(
        this.ctx.circuitSvg,
        this.ctx.wireData,
        wireIndex,
      );
      dropzone.addEventListener("mousedown", (ev: MouseEvent) =>
        ev.stopPropagation(),
      );
      dropzone.addEventListener("click", () =>
        this.commitAddControl(selectedOperation, selectedLocation, wireIndex),
      );
      this.ctx.overlayLayer.appendChild(dropzone);
    }
  }

  /**
   * Begin the wire-pick flow that lets the user click a control dot
   * to remove it. Called from the context menu.
   */
  startRemovingControl(selectedOperation: Unitary) {
    this.ctx.interaction.selectedOperation = selectedOperation;
    this.ctx.container.classList.add("removing-control");

    this.ctx.interaction.selectedOperation.controls?.forEach((control) => {
      const dropzone = createWireDropzone(
        this.ctx.circuitSvg,
        this.ctx.wireData,
        control.qubit,
      );
      dropzone.addEventListener("mousedown", (ev: MouseEvent) =>
        ev.stopPropagation(),
      );
      dropzone.addEventListener("click", () => {
        if (
          this.ctx.interaction.selectedOperation == null ||
          this.ctx.interaction.selectedOperation.kind !== "unitary"
        )
          return;
        const successful = removeControl(
          this.ctx.model,
          this.ctx.interaction.selectedOperation,
          control.qubit,
        );
        this.ctx.interaction.selectedOperation = null;
        this.ctx.container.classList.remove("removing-control");
        if (successful) this.ctx.renderFn();
      });
      this.ctx.overlayLayer.appendChild(dropzone);
    });
  }

  /******************************
   *   Listener installation    *
   ******************************/

  private installLayerListeners(): void {
    // Container mouseup hides editor overlay layers (dropzones,
    // ghost-qubit). Done at this level, not on circuitSvg, because
    // the user might release the mouse over the toolbox or chrome.
    this.ctx.container.addEventListener("mouseup", () => {
      if (this.ctx.model.qubits.length !== 0) {
        this.ctx.ghostQubitLayer.style.display = "none";
      }
      this.ctx.dropzoneLayer.style.display = "none";
    });

    // Track whether the most recent mouseup landed on the circuit
    // surface itself; consumed by the document mouseup to decide
    // drag-out-delete vs commit.
    this.ctx.circuitSvg.addEventListener("mouseup", () => {
      this.ctx.interaction.mouseUpOnCircuit = true;
    });

    // Suppress native context menu inside the editor.
    this.ctx.container.addEventListener("contextmenu", (ev: MouseEvent) => {
      ev.preventDefault();
    });
  }

  private installGateListeners(): void {
    const elems = getGateElems(this.ctx.container);
    elems.forEach((elem) => {
      elem?.addEventListener("mousedown", (ev: MouseEvent) =>
        this.onGateMouseDown(ev, elem),
      );

      // Arg-button: in-place argument editing for parameterized gates.
      const argButtons = elem.querySelectorAll<SVGElement>(".arg-button");
      argButtons.forEach((argButton) => {
        argButton.classList.add("edit-mode");
        argButton.addEventListener("click", () =>
          this.onArgButtonClick(argButton),
        );
      });
    });
  }

  private installToolboxListeners(): void {
    const elems = getToolboxElems(this.ctx.container);
    elems.forEach((elem) => {
      elem.addEventListener("mousedown", this.onToolboxMouseDown);
    });
  }

  private uninstallToolboxListeners(): void {
    const elems = getToolboxElems(this.ctx.container);
    elems.forEach((elem) => {
      elem.removeEventListener("mousedown", this.onToolboxMouseDown);
    });
  }

  private installDropzoneListeners(): void {
    const dropzoneElems =
      this.ctx.dropzoneLayer.querySelectorAll<SVGRectElement>(".dropzone");
    dropzoneElems.forEach((dropzoneElem) => {
      dropzoneElem.addEventListener("mouseup", this.onDropzoneMouseUp);
    });
  }

  private installDocumentListeners(): void {
    document.addEventListener("mouseup", this.onDocumentMouseUp);
    document.addEventListener("mousedown", this.onDocumentMouseDown);
  }

  private uninstallDocumentListeners(): void {
    document.removeEventListener("mouseup", this.onDocumentMouseUp);
    document.removeEventListener("mousedown", this.onDocumentMouseDown);
  }

  /******************************
   *        Handlers            *
   ******************************/

  private onGateMouseDown = (ev: MouseEvent, elem: SVGGraphicsElement) => {
    // Allow dragging even when initiated on the arg-button — capture
    // the wire from the sibling host element so the drag knows which
    // qubit is the "from" wire.
    const argButtonElem = (ev.target as HTMLElement).closest(".arg-button");
    if (argButtonElem) {
      const siblingWithWire =
        argButtonElem.parentElement?.querySelector("[data-wire]");
      if (siblingWithWire) {
        const selectedWireStr = siblingWithWire.getAttribute("data-wire");
        this.ctx.interaction.selectedWire =
          selectedWireStr != null ? parseInt(selectedWireStr) : null;
      }
    }

    let selectedLocation = null;
    if (elem.getAttribute("data-expanded") !== "true") {
      // Looked up via `findOperation` against the model so subsequent
      // edits operate on the live op, not a stale snapshot.
      selectedLocation = elem.getAttribute("data-location");
      this.ctx.interaction.selectedOperation = findOperation(
        this.ctx.model.componentGrid,
        selectedLocation,
      );
    }
    if (ev.button !== 0) return;
    ev.stopPropagation();
    removeAllWireDropzones(this.ctx.circuitSvg);
    if (
      this.ctx.interaction.selectedOperation === null ||
      this.ctx.interaction.selectedWire === null ||
      !selectedLocation
    )
      return;

    // Add temporary per-op dropzones for the multi-target drag flow.
    // The scope that *contains* the selected op is the parent of its
    // location: e.g. an op at "0,0-1,2" lives in the "0,0" scope, an
    // op at "1,0" lives in the top-level "" scope.
    const [minTarget, maxTarget] = getMinMaxRegIdx(
      this.ctx.interaction.selectedOperation,
    );
    const selectedAddr = Location.parse(selectedLocation);
    const last = selectedAddr.last();
    if (last == null) return;
    const [colIndex, opIndex] = last;
    const parentPrefix = selectedAddr.parent().toString();
    const parentScope = this.ctx.layoutMap.scopes.get(parentPrefix);
    if (parentScope == null) return;

    for (let wire = minTarget; wire <= maxTarget; wire++) {
      if (wire === this.ctx.interaction.selectedWire) continue;
      const dropzone = makeDropzoneBox(
        colIndex,
        opIndex,
        parentScope,
        this.ctx.wireData,
        wire,
        false,
        parentPrefix,
      );
      dropzone.addEventListener("mouseup", this.onDropzoneMouseUp);
      trackTemporaryDropzone(this.ctx.interaction, dropzone);
      this.ctx.dropzoneLayer.appendChild(dropzone);
    }

    this.spawnGhost(ev);

    // Make sure the selectedOperation has location data — downstream
    // drop logic reads it via getGateLocationString().
    if (this.ctx.interaction.selectedOperation.dataAttributes == null) {
      this.ctx.interaction.selectedOperation.dataAttributes = {
        location: selectedLocation,
      };
    } else {
      this.ctx.interaction.selectedOperation.dataAttributes["location"] =
        selectedLocation;
    }

    this.ctx.container.classList.add("moving");
    this.ctx.ghostQubitLayer.style.display = "block";
    this.ctx.dropzoneLayer.style.display = "block";
  };

  private onArgButtonClick = async (argButton: SVGElement) => {
    if (this.ctx.interaction.selectedOperation == null) return;
    const params = this.ctx.interaction.selectedOperation.params;
    const displayArgs = argButton.textContent || "";
    if (params) {
      const args = await promptForArguments(params, [displayArgs]);
      if (args.length > 0) {
        this.ctx.interaction.selectedOperation.args = args;
        this.ctx.renderFn();
      }
    }
  };

  private onToolboxMouseDown = (ev: MouseEvent) => {
    if (ev.button !== 0) return;
    this.ctx.container.classList.add("moving");
    this.ctx.ghostQubitLayer.style.display = "block";
    this.ctx.dropzoneLayer.style.display = "block";
    const elem = ev.currentTarget as HTMLElement;
    const type = elem.getAttribute("data-type");
    if (type == null) return;
    beginToolboxDrag(this.ctx.interaction, toolboxGateDictionary[type]);
    this.spawnGhost(ev);
  };

  private onDropzoneMouseUp = async (ev: MouseEvent) => {
    const dropzoneElem = ev.currentTarget as SVGRectElement;
    const copying = ev.ctrlKey;
    // Snapshot for the no-op deepEqual short-circuit at the end.
    const originalGrid = JSON.parse(
      JSON.stringify(this.ctx.model.componentGrid),
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
      this.ctx.interaction.selectedOperation == null
    )
      return;
    const sourceLocation = getGateLocationString(
      this.ctx.interaction.selectedOperation,
    );

    if (sourceLocation == null) {
      // Source has no location → it's a fresh drop from the toolbox.
      // Prompt for any required args before committing.
      if (
        this.ctx.interaction.selectedOperation.params != undefined &&
        (this.ctx.interaction.selectedOperation.args === undefined ||
          this.ctx.interaction.selectedOperation.args.length === 0)
      ) {
        const args = await promptForArguments(
          this.ctx.interaction.selectedOperation.params,
        );
        if (!args || args.length === 0) {
          return;
        }
        // Deep-copy the toolbox prototype before mutating it.
        this.ctx.interaction.selectedOperation = JSON.parse(
          JSON.stringify(this.ctx.interaction.selectedOperation),
        );
        if (this.ctx.interaction.selectedOperation == null) return;
        this.ctx.interaction.selectedOperation.args = args;
      }

      addOperation(
        this.ctx.model,
        this.ctx.interaction.selectedOperation,
        targetLoc,
        targetWire,
        insertNewColumn,
      );
    } else if (sourceLocation && this.ctx.interaction.selectedWire != null) {
      if (copying) {
        if (
          this.ctx.interaction.movingControl &&
          this.ctx.interaction.selectedOperation.kind === "unitary"
        ) {
          addControl(
            this.ctx.model,
            this.ctx.interaction.selectedOperation,
            targetWire,
          );
          moveOperation(
            this.ctx.model,
            sourceLocation,
            targetLoc,
            this.ctx.interaction.selectedWire,
            targetWire,
            this.ctx.interaction.movingControl,
            insertNewColumn,
          );
        } else {
          addOperation(
            this.ctx.model,
            this.ctx.interaction.selectedOperation,
            targetLoc,
            targetWire,
            insertNewColumn,
          );
        }
      } else {
        moveOperation(
          this.ctx.model,
          sourceLocation,
          targetLoc,
          this.ctx.interaction.selectedWire,
          targetWire,
          this.ctx.interaction.movingControl,
          insertNewColumn,
        );
      }
    }

    this.ctx.interaction.selectedOperation = null;
    resetTransient(this.ctx.interaction);

    if (!deepEqual(originalGrid, this.ctx.model.componentGrid)) {
      this.ctx.renderFn();
    }
  };

  private onDocumentMouseDown = () => {
    removeAllWireDropzones(this.ctx.circuitSvg);
  };

  private onDocumentMouseUp = (ev: MouseEvent) => {
    const copying = ev.ctrlKey;
    this.ctx.container.classList.remove("moving", "copying");
    // Drag-out-delete: a drag that ended outside the circuit (and
    // wasn't a Ctrl-copy) deletes the source.
    if (
      !this.ctx.interaction.mouseUpOnCircuit &&
      this.ctx.interaction.dragging &&
      !copying
    ) {
      const selectedLocation = this.ctx.interaction.selectedOperation
        ? getGateLocationString(this.ctx.interaction.selectedOperation)
        : null;
      if (
        this.ctx.interaction.selectedOperation != null &&
        selectedLocation != null
      ) {
        // A placed gate (not from the toolbox) was dragged off-circuit.
        if (
          this.ctx.interaction.movingControl &&
          this.ctx.interaction.selectedOperation.kind === "unitary" &&
          this.ctx.interaction.selectedOperation.controls != null &&
          this.ctx.interaction.selectedWire != null
        ) {
          // Detached just the control we were dragging.
          removeControl(
            this.ctx.model,
            this.ctx.interaction.selectedOperation,
            this.ctx.interaction.selectedWire,
          );
        } else {
          removeOperation(this.ctx.model, selectedLocation);
        }
        this.ctx.renderFn();
      } else if (this.ctx.interaction.selectedWire != null) {
        // A qubit label was dragged off-circuit → ask the qubit
        // controller (which owns the prompt + render flow).
        this.qubitController.removeQubitLineWithConfirmation(
          this.ctx.interaction.selectedWire,
        );
      }
    }

    resetTransient(this.ctx.interaction);
  };

  /**
   * Bind the ghost element + auto-scroll to a fresh drag. Shared by
   * gate-mousedown and toolbox-mousedown; the qubit controller has
   * its own ghost path (`createQubitLabelGhost`).
   */
  private spawnGhost(ev: MouseEvent): void {
    if (this.ctx.interaction.selectedOperation == null) return;
    this.ctx.interaction.dragging = true;
    enableAutoScroll(this.ctx.circuitSvg, this.ctx.interaction);
    createGateGhost(
      ev,
      this.ctx.container,
      this.ctx.interaction.selectedOperation,
      this.ctx.interaction.movingControl,
    );
  }

  /**
   * Final step of `startAddingControl`: actually add the control,
   * then reshuffle the column if the new control range overlaps a
   * neighboring op.
   */
  private commitAddControl(
    selectedOperation: Unitary,
    selectedLocation: string,
    wireIndex: number,
  ): void {
    if (
      this.ctx.interaction.selectedOperation == null ||
      this.ctx.interaction.selectedOperation.kind !== "unitary"
    )
      return;
    const successful = addControl(
      this.ctx.model,
      this.ctx.interaction.selectedOperation,
      wireIndex,
    );
    this.ctx.interaction.selectedOperation = null;
    this.ctx.container.classList.remove("adding-control");
    this.ctx.ghostQubitLayer.style.display = "none";
    if (!successful) return;

    const last = Location.parse(selectedLocation).last();
    if (last == null) return;
    const [columnIndex, position] = last;
    const selectedOperationParent = findParentArray(
      this.ctx.model.componentGrid,
      selectedLocation,
    );
    if (!selectedOperationParent) return;

    // If the new control range intersects another op in the same
    // column, push the selected op into a fresh column ahead of it.
    const [minTarget, maxTarget] = getMinMaxRegIdx(selectedOperation);
    selectedOperationParent[columnIndex].components.forEach((op, opIndex) => {
      if (opIndex === position) return;
      const [minOp, maxOp] = getMinMaxRegIdx(op);
      if (
        (minOp >= minTarget && minOp <= maxTarget) ||
        (maxOp >= minTarget && maxOp <= maxTarget) ||
        (minTarget >= minOp && minTarget <= minOp) ||
        (maxTarget >= maxOp && maxTarget <= maxOp)
      ) {
        selectedOperationParent[columnIndex].components.splice(position, 1);
        if (selectedOperationParent[columnIndex].components.length === 0) {
          selectedOperationParent.splice(columnIndex, 1);
        }
        selectedOperationParent.splice(columnIndex, 0, {
          components: [selectedOperation],
        });
      }
    });

    this.ctx.renderFn();
  }
}
