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
  _deleteOperationWithConfirmation,
  _moveOperationWithConfirmation,
} from "../operationPrompts.js";
import {
  createGateGhost,
  createWireDropzone,
  makeDropzoneBox,
  makeShiftExtendGhost,
  removeAllWireDropzones,
} from "../draggable.js";
import {
  beginToolboxDrag,
  resetTransient,
  trackTemporaryDropzone,
} from "../../actions/interactionActions.js";
import { InteractionContext } from "./interactionContext.js";
import { LayoutScope } from "../../renderer/layoutMap.js";
import { Location } from "../../data/location.js";
import { promptForArguments } from "../contextMenu.js";
import { QubitController } from "./qubitController.js";
import { enableAutoScroll } from "./scrollController.js";
import { toolboxGateDictionary } from "../toolboxGates.js";
import {
  deepEqual,
  findOperation,
  getAncestorColumnSiblingWires,
  getGateElems,
  getGateLocationString,
  getQuantumWireRange,
  getToolboxElems,
} from "../../utils.js";

/**
 * `DragController` — owns the gate drag-and-drop surface: gate-drag,
 * toolbox-drag, dropzone commit, document-level cleanup/cancel,
 * ghost element creation, and the wire-pick dropzones for the
 * add-control / remove-control flow the context menu invokes.
 *
 * These flows share one dropzone overlay, one ghost element, the
 * same `interaction` flags, and the same document-level mouseup that
 * classifies a drag as commit, cancel, or drag-out-delete — so they
 * live in a single controller.
 *
 * Holds a `QubitController` reference for the one document-mouseup
 * path that detects a qubit-label drag-off and calls
 * `removeQubitLineWithConfirmation`.
 */
export class DragController {
  /**
   * Shift-extend context, populated by `onGateMouseDown` when the
   * dragged source is internal to an expanded group, cleared by
   * `tearDownShiftExtend` on container mouseup. Drives the extra
   * "extend vertically" dropzones and the ghost-border overlay.
   * `null` whenever the current drag can't extend a group.
   */
  private _shiftExtendCtx: {
    /** Hierarchical location of the immediate parent group G. */
    parentLoc: string;
    /** `[minWire, maxWire]` of G's current target span. */
    parentMinWire: number;
    parentMaxWire: number;
    /** Geometry of G's children scope, from `LayoutMap.scopes`. */
    parentScope: LayoutScope;
  } | null = null;

  /**
   * Dropzones spawned by `spawnShiftExtendDropzones`, tracked
   * separately so shift-release can clear them ahead of the
   * container-mouseup cleanup.
   */
  private _shiftExtendDropzones: SVGElement[] = [];

  /** Ghost-border rect currently painted in the overlay, if any. */
  private _ghostBorder: SVGElement | null = null;

  /** Currently-installed shift keydown/keyup listeners, if any. */
  private _onShiftDown: ((ev: KeyboardEvent) => void) | null = null;
  private _onShiftUp: ((ev: KeyboardEvent) => void) | null = null;

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
  startAddingControl(selectedOperation: Unitary) {
    this.ctx.interaction.selectedOperation = selectedOperation;
    this.ctx.container.classList.add("adding-control");
    this.ctx.ghostQubitLayer.style.display = "block";

    for (let wireIndex = 0; wireIndex < this.ctx.wireData.length; wireIndex++) {
      // Only pure-quantum target/control entries (`result === undefined`)
      // disqualify a wire. A classical-ref entry `{qubit, result}` on
      // the M-owning wire doesn't make it a quantum target or control,
      // so a quantum control can still be added there.
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
   * Begin the wire-pick flow that lets the user click a control dot
   * to remove it. Called from the context menu.
   */
  startRemovingControl(selectedOperation: Unitary) {
    this.ctx.interaction.selectedOperation = selectedOperation;
    this.ctx.container.classList.add("removing-control");

    this.ctx.interaction.selectedOperation.controls?.forEach((control) => {
      // Skip classical-ref controls: a `{qubit, result}` control is the
      // group's classical dependency on a producing M, with no quantum
      // control-dot to click.
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
      // Reset per-dropzone visibility marks left by
      // `hideInvalidDropzones`, so a drag that doesn't re-render
      // (canceled, or a no-op drop) doesn't leave the next drag with
      // stale `display: none` marks.
      this.showAllDropzones();
      // Clear shift-extend context, drop any leftover shift-extend
      // dropzones and the ghost border, and uninstall the shift
      // listeners. Pairs with `setupShiftExtend` in `onGateMouseDown`.
      this.tearDownShiftExtend();
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
    if (
      elem.getAttribute("data-expanded") !== "true" ||
      this.ctx.interaction.movingControl
    ) {
      // Looked up via `findOperation` against the model so subsequent
      // edits operate on the live op, not a stale snapshot.
      //
      // The `movingControl` carve-out covers grabbing a control dot on
      // an expanded group: those dots are direct children of the
      // group's `data-expanded="true"` node (child gate elems
      // stopPropagation first), so without this branch the early-return
      // below would leave `selectedOperation` null and the drag would
      // never start.
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
    // The scope that contains the selected op is the parent of its
    // location (an op at "0,0-1,2" lives in the "0,0" scope).
    //
    // Quantum-only span: a classically-controlled op's `.controls`
    // back-reference to the producing M isn't a draggable leg.
    const [minTarget, maxTarget] = getQuantumWireRange(
      this.ctx.interaction.selectedOperation,
    );
    const selectedAddr = Location.parse(selectedLocation);
    const last = selectedAddr.last();
    if (last == null) return;
    const [colIndex, opIndex] = last;
    const parentPrefix = selectedAddr.parent().toString();
    const parentScope = this.ctx.layoutMap.scopes.get(parentPrefix);
    if (parentScope == null) return;

    const dropzoneCtx = {
      scope: parentScope,
      wireData: this.ctx.wireData,
      pathPrefix: parentPrefix,
    };
    for (let wire = minTarget; wire <= maxTarget; wire++) {
      if (wire === this.ctx.interaction.selectedWire) continue;
      const dropzone = makeDropzoneBox(dropzoneCtx, {
        colIndex,
        opIndex,
        wireIndex: wire,
        interColumn: false,
      });
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

    // Hide dropzones whose drop would invert producer-before-consumer
    // ordering for any classical register the selected op consumes
    // from outside its own subtree. See `hideInvalidDropzones`.
    this.hideInvalidDropzones(selectedLocation);

    // Arm shift-extend if the source is internal to an expanded
    // ancestor group; no-op for top-level sources or untracked
    // scopes. See `setupShiftExtend`.
    this.setupShiftExtend(selectedAddr);

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
    // Set when a code path delegates rendering to a prompt-aware
    // wrapper (`_moveOperationWithConfirmation`), which owns its own
    // renderFn call; the trailing deepEqual block then skips its own
    // to avoid double-rendering.
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

    // Shift-extend dropzones offer drop targets on wires outside the
    // destination group's current span. The action layer treats the
    // target location string as authoritative (it re-derives ancestor
    // `.targets` from post-move children), so no special routing is
    // needed here.

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
          // Pass `selectedWire` as the source wire so a group /
          // multi-target clone shifts every register by the same
          // delta. Without it, `addOperation` collapses `targets` to a
          // single-wire stub and strands the children.
          addOperation(
            this.ctx.model,
            this.ctx.interaction.selectedOperation,
            targetLoc,
            targetWire,
            insertNewColumn,
            this.ctx.interaction.selectedWire,
          );
        }
      } else {
        // Regular move path. Routes through the prompt-aware wrapper
        // so moving a measurement with downstream classical consumers
        // surfaces a confirmation dialog. The wrapper owns the
        // renderFn call on both branches, so skip the trailing
        // deepEqual block via `mutationHandledByWrapper`.
        _moveOperationWithConfirmation(
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
          this.ctx.renderFn();
        } else {
          // Drag-out-delete. Routes through the prompt-aware wrapper
          // so deleting a measurement with downstream classical
          // consumers confirms first; the wrapper owns renderFn on
          // both branches.
          _deleteOperationWithConfirmation(
            this.ctx.model,
            selectedLocation,
            this.ctx.renderFn,
          );
        }
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
   * Hide every dropzone that would, if used as the drop target for
   * the currently-dragged op, invert the "producer measurement comes
   * before its classical consumer" ordering. Invalid dropzones get
   * `display: none` so they neither paint nor catch mouseup.
   *
   * A classically-conditional unitary carries `(qubit, result)`
   * references to a producing M; dropping it before that M points at
   * a classical register that doesn't exist yet at the consumer's
   * position, which crashes the renderer or yields a broken circuit.
   *
   * Producers internal to the dragged subtree don't constrain the
   * drop — they travel with the consumer. See
   * [`collectExternalProducerLocations`](../../actions/circuitActions.ts).
   *
   * Pairs with the `moveOperation` safety-net refusal: this filter is
   * the user-facing surface; the action-layer refusal catches drops
   * that slip through.
   */
  private hideInvalidDropzones(selectedLocation: string): void {
    // Reset every dropzone to visible first so stale marks from a
    // previous drag don't bleed into this one. (Belt-and-suspenders
    // with the layer-mouseup reset in `installLayerListeners`.)
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
      // Hide if any external producer is NOT in a strictly earlier
      // column than this drop target. Column-strict (not plain
      // document order) also catches a consumer promoted to a higher
      // level that lands in the same outer column as its producer.
      for (const pLoc of producerLocs) {
        if (!pLoc.inEarlierColumnThan(targetLoc)) {
          dz.style.display = "none";
          return;
        }
      }
    });
  }

  /**
   * Clear every per-dropzone `display` mark, restoring CSS-default
   * visibility. Shared by `hideInvalidDropzones` and the
   * layer-mouseup teardown so no drag inherits stale marks.
   */
  private showAllDropzones(): void {
    const dropzones =
      this.ctx.dropzoneLayer.querySelectorAll<SVGElement>(".dropzone");
    dropzones.forEach((dz) => {
      dz.style.display = "";
    });
  }

  /**
   * Final step of `startAddingControl`: add the control, tear down
   * the add-control UI, and re-render. The action layer
   * (`addControl` → `_resolveSpanChange`) owns the post-widening
   * cascade — column splits, ancestor `.targets` refresh, sibling
   * shifts — so this wrapper does not duplicate any of it.
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

  /******************************
   *        shift-extend         *
   ******************************/

  /**
   * Arm the shift-extend pathway for a new internal-source drag.
   * No-op if `selectedAddr` is top-level (no parent group to extend)
   * or if the immediate parent's children scope isn't tracked by the
   * LayoutMap (defensive).
   *
   * On the happy path: captures the parent group's wire span +
   * scope, installs document shift keydown/keyup listeners, and
   * spawns initial dropzones if shift is already held at drag start.
   */
  private setupShiftExtend(selectedAddr: Location): void {
    if (selectedAddr.depth < 2) return; // top-level source
    const parentAddr = selectedAddr.parent();
    const parentLoc = parentAddr.toString();
    const parentScope = this.ctx.layoutMap.scopes.get(parentLoc);
    if (parentScope == null) return;

    const parentOp = findOperation(this.ctx.model.componentGrid, parentLoc);
    if (parentOp == null) return;
    // Quantum-only span: shift-extend reach mirrors the group's
    // editable wire scope, not its visual span including any
    // classical-control back-references.
    const [parentMinWire, parentMaxWire] = getQuantumWireRange(parentOp);

    this._shiftExtendCtx = {
      parentLoc,
      parentMinWire,
      parentMaxWire,
      parentScope,
    };

    // Install live shift tracking. Document-level because the user
    // may shift+drag with the cursor outside the SVG (e.g. hovering
    // the editor chrome on the way to the target wire).
    this._onShiftDown = (ev) => {
      if (ev.key !== "Shift") return;
      this.spawnShiftExtendDropzones();
    };
    this._onShiftUp = (ev) => {
      if (ev.key !== "Shift") return;
      this.clearShiftExtendDropzones();
      this.clearGhostBorder();
    };
    document.addEventListener("keydown", this._onShiftDown);
    document.addEventListener("keyup", this._onShiftUp);
  }

  /**
   * Tear down shift-extend state for the current (or just-ended)
   * drag. Idempotent — safe to call when nothing was armed.
   */
  private tearDownShiftExtend(): void {
    this.clearShiftExtendDropzones();
    this.clearGhostBorder();
    if (this._onShiftDown != null) {
      document.removeEventListener("keydown", this._onShiftDown);
      this._onShiftDown = null;
    }
    if (this._onShiftUp != null) {
      document.removeEventListener("keyup", this._onShiftUp);
      this._onShiftUp = null;
    }
    this._shiftExtendCtx = null;
  }

  /**
   * Spawn the temporary "extend group vertically" dropzones for the
   * currently-armed shift-extend context. Re-spawn-safe (clears
   * existing first), idempotent for the same context.
   *
   * Emitted at every `(column, wire)` pair where:
   *   - `column` is one of the parent group's existing inner columns
   *     OR the trailing-append column past its rightmost child;
   *   - `wire` is in `[0, wireData.length)` but NOT in the parent
   *     group's `[minTarget, maxTarget]` span.
   *
   * Each dropzone is tagged `data-shift-extend="true"` so the
   * mouseup handler can recognize a shift-extend release for
   * visual cleanup (the ghost border). The action layer
   * (`moveOperation`) always re-derives ancestor `.targets` from
   * post-move children, so no special routing on the action call
   * is needed \u2014 the location string of the dropzone is enough.
   * Hover-enter paints the ghost border for that wire; hover-leave
   * clears it.
   */
  private spawnShiftExtendDropzones(): void {
    if (this._shiftExtendCtx == null) return;
    this.clearShiftExtendDropzones();

    const { parentScope, parentMinWire, parentMaxWire, parentLoc } =
      this._shiftExtendCtx;
    const realColCount = parentScope.columnXOffsets.length;
    // +1 for the trailing-append column past the rightmost.
    const totalCols = realColCount + 1;

    // Wires the group can't directly extend onto because a sibling
    // at some level of the ancestor chain already occupies them in
    // that level's outer column — dropping there would land the new
    // op directly on an existing one. We walk the full ancestor
    // chain since shift-extend widens every ancestor whose span
    // doesn't already enclose the drop wire.
    //
    // The cross-over case (extending past an in-between sibling to a
    // clear wire) is intentionally not filtered: `moveOperation`'s
    // dest-side cascade splits the outer column so the in-between
    // sibling slides one column right of the widened ancestor.
    const blockedWires = getAncestorColumnSiblingWires(
      this.ctx.model.componentGrid,
      parentLoc,
    );

    const dropzoneCtx = {
      scope: parentScope,
      wireData: this.ctx.wireData,
      pathPrefix: parentLoc,
    };
    for (let colIndex = 0; colIndex < totalCols; colIndex++) {
      for (let wire = 0; wire < this.ctx.wireData.length; wire++) {
        // Only emit for wires outside the group's current span; wires
        // inside already have regular inner dropzones.
        if (wire >= parentMinWire && wire <= parentMaxWire) continue;

        // Skip wires a sibling already occupies (see `blockedWires`).
        if (blockedWires.has(wire)) continue;

        // opIndex = 0: the wire is outside the group's span so no op
        // in this column shares it; the op slots in without splicing
        // a new column.
        const dropzone = makeDropzoneBox(dropzoneCtx, {
          colIndex,
          opIndex: 0,
          wireIndex: wire,
          interColumn: false,
        });
        dropzone.setAttribute("data-shift-extend", "true");
        // Force a normal drop (no new outer column), not an
        // insert-between gesture.
        dropzone.setAttribute("data-dropzone-inter-column", "false");
        dropzone.addEventListener("mouseup", this.onDropzoneMouseUp);
        dropzone.addEventListener("mouseenter", () => {
          this.paintGhostBorder(wire, colIndex);
        });
        dropzone.addEventListener("mouseleave", () => {
          this.clearGhostBorder();
        });
        this.ctx.dropzoneLayer.appendChild(dropzone);
        this._shiftExtendDropzones.push(dropzone);
      }
    }
  }

  /**
   * Remove every shift-extend dropzone from the layer. Fired on
   * shift-up (so the dropzones disappear immediately) and on
   * container mouseup (belt-and-suspenders). Idempotent.
   */
  private clearShiftExtendDropzones(): void {
    for (const dz of this._shiftExtendDropzones) {
      dz.parentNode?.removeChild(dz);
    }
    this._shiftExtendDropzones = [];
  }

  /**
   * Paint the ghost-border overlay for the given hover wire and
   * column. Replaces any existing ghost border (so moving between
   * shift-extend dropzones updates the preview).
   */
  private paintGhostBorder(hoverWire: number, hoverCol: number): void {
    if (this._shiftExtendCtx == null) return;
    this.clearGhostBorder();
    const { parentScope, parentMinWire, parentMaxWire } = this._shiftExtendCtx;
    this._ghostBorder = makeShiftExtendGhost(
      parentScope,
      this.ctx.wireData,
      parentMinWire,
      parentMaxWire,
      hoverWire,
      hoverCol,
    );
    this.ctx.overlayLayer.appendChild(this._ghostBorder);
  }

  /** Remove the ghost-border overlay if one is painted. Idempotent. */
  private clearGhostBorder(): void {
    if (this._ghostBorder != null) {
      this._ghostBorder.parentNode?.removeChild(this._ghostBorder);
      this._ghostBorder = null;
    }
  }
}
