// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { ComponentGrid, Unitary } from "../../data/circuit.js";
import {
  addControl,
  addOperation,
  collectExternalProducerLocations,
  moveOperation,
  removeControl,
} from "../../actions/circuitActions.js";
import {
  deleteOperationWithConfirmation,
  moveOperationWithConfirmation,
} from "../prompts.js";
import {
  createGateGhost,
  createWireDropzone,
  removeAllWireDropzones,
} from "../draggable.js";
import {
  beginToolboxDrag,
  resetTransient,
} from "../../actions/interactionActions.js";
import { InteractionContext } from "./interactionContext.js";
import { Location } from "../../data/location.js";
import { promptForArguments } from "../prompts.js";
import { QubitController } from "./qubitController.js";
import { enableAutoScroll } from "./scrollController.js";
import { toolboxGateDictionary } from "../toolboxGates.js";
import { getGateElems, getToolboxElems } from "../domUtils.js";
import {
  deepEqual,
  findOperation,
  getGateLocationString,
} from "../../utils.js";

/**
 * `DragController` — owns the gate drag-and-drop surface: gate-drag, toolbox-drag, dropzone commit,
 * document-level cleanup/cancel, ghost element creation, and the wire-pick dropzones for the
 * add-control / remove-control flow the context menu invokes.
 *
 * These flows share one dropzone overlay, one ghost element, the same `interaction` flags, and the
 * same document-level mouseup that classifies a drag as commit, cancel, or drag-out-delete — so
 * they live in a single controller.
 *
 * Holds a `QubitController` reference for the one document-mouseup path that detects a qubit-label
 * drag-off and calls `removeQubitLineWithConfirmation`.
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
   * Begin the wire-pick flow that lets the user click a wire to add a control to
   * `selectedOperation`. Called from the context menu.
   */
  startAddingControl(selectedOperation: Unitary) {
    this.ctx.interaction.selectedOperation = selectedOperation;
    this.ctx.container.classList.add("adding-control");
    this.ctx.ghostQubitLayer.style.display = "block";

    for (let wireIndex = 0; wireIndex < this.ctx.wireData.length; wireIndex++) {
      // Only pure-quantum target/control entries (`result === undefined`) disqualify a wire. A
      // classical-ref entry `{qubit, result}` on the M-owning wire doesn't make it a quantum target
      // or control, so a quantum control can still be added there.
      const isTarget = this.ctx.interaction.selectedOperation?.targets.some(
        (target) => target.qubit === wireIndex && target.result === undefined,
      );
      const isControl = this.ctx.interaction.selectedOperation?.controls?.some(
        (control) =>
          control.qubit === wireIndex && control.result === undefined,
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
        this.commitAddControl(wireIndex),
      );
      this.ctx.overlayLayer.appendChild(dropzone);
    }
  }

  /**
   * Begin the wire-pick flow that lets the user click a control dot to remove it. Called from the
   * context menu.
   */
  startRemovingControl(selectedOperation: Unitary) {
    this.ctx.interaction.selectedOperation = selectedOperation;
    this.ctx.container.classList.add("removing-control");

    this.ctx.interaction.selectedOperation.controls?.forEach((control) => {
      // Skip classical-ref controls: a `{qubit, result}` control is the group's classical
      // dependency on a producing M, with no quantum control-dot to click.
      if (control.result !== undefined) return;
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
   * *****************************/

  private installLayerListeners(): void {
    // Container mouseup hides editor overlay layers (dropzones, ghost-qubit). Done at this level,
    // not on circuitSvg, because the user might release the mouse over the toolbox or chrome.
    this.ctx.container.addEventListener("mouseup", () => {
      if (this.ctx.model.qubits.length !== 0) {
        this.ctx.ghostQubitLayer.style.display = "none";
      }
      this.ctx.dropzoneLayer.style.display = "none";
      // Reset per-dropzone visibility marks left by `hideInvalidDropzones`, so a drag that doesn't
      // re-render (canceled, or a no-op drop) doesn't leave the next drag with stale `display:
      // none` marks.
      this.showAllDropzones();
    });

    // Track whether the most recent mouseup landed on the circuit surface itself; consumed by the
    // document mouseup to decide drag-out-delete vs commit.
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
   * *****************************/

  private onGateMouseDown = (ev: MouseEvent, elem: SVGGraphicsElement) => {
    // Allow dragging even when initiated on the arg-button — capture the wire from the sibling host
    // element so the drag knows which qubit is the "from" wire.
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
    if (
      elem.getAttribute("data-expanded") !== "true" ||
      this.ctx.interaction.movingControl
    ) {
      // Looked up via `findOperation` against the model so subsequent edits operate on the live op,
      // not a stale snapshot.
      //
      // The `movingControl` carve-out covers grabbing a control dot on an expanded group: those
      // dots are direct children of the group's `data-expanded="true"` node (child gate elems
      // stopPropagation first), so without this branch the early-return below would leave
      // `selectedOperation` null and the drag would never start.
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

    this.spawnGhost(ev);

    // Make sure the selectedOperation has location data — downstream drop logic reads it via
    // getGateLocationString().
    if (this.ctx.interaction.selectedOperation.dataAttributes == null) {
      this.ctx.interaction.selectedOperation.dataAttributes = {
        location: selectedLocation,
      };
    } else {
      this.ctx.interaction.selectedOperation.dataAttributes["location"] =
        selectedLocation;
    }

    // Hide dropzones whose drop would invert producer-before-consumer ordering for any classical
    // register the selected op consumes from outside its own subtree. See `hideInvalidDropzones`.
    this.hideInvalidDropzones(selectedLocation);

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
    // Set when a code path delegates rendering to a prompt-aware wrapper
    // (`moveOperationWithConfirmation`), which owns its own renderFn call; the trailing deepEqual
    // block then skips its own to avoid double-rendering.
    let mutationHandledByWrapper = false;
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

    // Shift-extend dropzones offer drop targets on wires outside the destination group's current
    // span. The action layer treats the target location string as authoritative (it re-derives
    // ancestor `.targets` from post-move children), so no special routing is needed here.

    if (sourceLocation == null) {
      // Source has no location → it's a fresh drop from the toolbox. Prompt for any required args
      // before committing.
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
        // Regular move path. Routes through the prompt-aware wrapper so moving a measurement with
        // downstream classical consumers surfaces a confirmation dialog. The wrapper owns the
        // renderFn call on both branches, so skip the trailing deepEqual block via
        // `mutationHandledByWrapper`.
        moveOperationWithConfirmation(
          this.ctx.model,
          sourceLocation,
          targetLoc,
          this.ctx.interaction.selectedWire,
          targetWire,
          this.ctx.interaction.movingControl,
          insertNewColumn,
          this.ctx.renderFn,
        );
        mutationHandledByWrapper = true;
      }
    }

    this.ctx.interaction.selectedOperation = null;
    resetTransient(this.ctx.interaction);

    if (
      !mutationHandledByWrapper &&
      !deepEqual(originalGrid, this.ctx.model.componentGrid)
    ) {
      this.ctx.renderFn();
    }
  };

  private onDocumentMouseDown = () => {
    removeAllWireDropzones(this.ctx.circuitSvg);
  };

  private onDocumentMouseUp = (ev: MouseEvent) => {
    const copying = ev.ctrlKey;
    this.ctx.container.classList.remove("moving", "copying");
    // Drag-out-delete: a drag that ended outside the circuit (and wasn't a Ctrl-copy) deletes the
    // source.
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
          this.ctx.renderFn();
        } else {
          // Drag-out-delete. Routes through the prompt-aware wrapper so deleting a measurement with
          // downstream classical consumers confirms first; the wrapper owns renderFn on both
          // branches.
          deleteOperationWithConfirmation(
            this.ctx.model,
            selectedLocation,
            this.ctx.renderFn,
          );
        }
      } else if (this.ctx.interaction.selectedWire != null) {
        // A qubit label was dragged off-circuit → ask the qubit controller (which owns the prompt +
        // render flow).
        this.qubitController.removeQubitLineWithConfirmation(
          this.ctx.interaction.selectedWire,
        );
      }
    }

    resetTransient(this.ctx.interaction);
  };

  /**
   * Bind the ghost element + auto-scroll to a fresh drag. Shared by gate-mousedown and
   * toolbox-mousedown; the qubit controller has its own ghost path (`createQubitLabelGhost`).
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
   * Hide every dropzone that would, if used as the drop target for the currently-dragged op, invert
   * the "producer measurement comes before its classical consumer" ordering. Invalid dropzones get
   * `display: none` so they neither paint nor catch mouseup.
   *
   * A classically-conditional unitary carries `(qubit, result)` references to a producing M;
   * dropping it before that M points at a classical register that doesn't exist yet at the
   * consumer's position, which crashes the renderer or yields a broken circuit.
   *
   * Producers internal to the dragged subtree don't constrain the drop — they travel with the
   * consumer. See [`collectExternalProducerLocations`](../../actions/circuitActions.ts).
   *
   * Pairs with the `moveOperation` safety-net refusal: this filter is the user-facing surface; the
   * action-layer refusal catches drops that slip through.
   */
  private hideInvalidDropzones(selectedLocation: string): void {
    // Reset every dropzone to visible first so stale marks from a previous drag don't bleed into
    // this one. (Belt-and-suspenders with the layer-mouseup reset in `installLayerListeners`.)
    this.showAllDropzones();

    const externalProducerLocs = collectExternalProducerLocations(
      this.ctx.model.componentGrid,
      selectedLocation,
    );
    if (externalProducerLocs.length === 0) return;

    const producerLocs = externalProducerLocs.map((s) => Location.parse(s));

    const dropzones =
      this.ctx.dropzoneLayer.querySelectorAll<SVGElement>(".dropzone");
    dropzones.forEach((dz) => {
      const targetLocStr = dz.getAttribute("data-dropzone-location");
      if (targetLocStr == null) return;
      const targetLoc = Location.parse(targetLocStr);
      // Hide if any external producer is NOT in a strictly earlier column than this drop target.
      // Column-strict (not plain document order) also catches a consumer promoted to a higher level
      // that lands in the same outer column as its producer.
      for (const pLoc of producerLocs) {
        if (!pLoc.inEarlierColumnThan(targetLoc)) {
          dz.style.display = "none";
          return;
        }
      }
    });
  }

  /**
   * Clear every per-dropzone `display` mark, restoring CSS-default visibility. Shared by
   * `hideInvalidDropzones` and the layer-mouseup teardown so no drag inherits stale marks.
   */
  private showAllDropzones(): void {
    const dropzones =
      this.ctx.dropzoneLayer.querySelectorAll<SVGElement>(".dropzone");
    dropzones.forEach((dz) => {
      dz.style.display = "";
    });
  }

  /**
   * Final step of `startAddingControl`: add the control, tear down the add-control UI, and
   * re-render. The action layer (`addControl` → `_resolveSpanChange`) owns the post-widening
   * cascade — column splits, ancestor `.targets` refresh, sibling shifts — so this wrapper does not
   * duplicate any of it.
   */
  private commitAddControl(wireIndex: number): void {
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

    this.ctx.renderFn();
  }
}
