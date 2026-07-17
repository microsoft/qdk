// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import {
  moveQubit,
  removeQubitWithDependents,
} from "../../actions/circuitActions.js";
import { createQubitLabelGhost, createWireDropzone } from "../draggable.js";
import { InteractionContext } from "./interactionContext.js";
import { trackTemporaryDropzone } from "../../actions/interactionActions.js";
import { createConfirmPrompt } from "../prompts.js";
import { enableAutoScroll } from "./scrollController.js";
import { getQubitLabelElems } from "../domUtils.js";

/**
 * `QubitController` — owns qubit-line interactions.
 *
 * Two surfaces:
 *
 * 1. **Drag a qubit label.** Mousedown on a qubit-label spawns swap and insert-between dropzones
 *    along every other wire; mouseup on one dispatches `moveQubit` (Action layer) and re-renders.
 * 2. **Remove a qubit line.** `removeQubitLineWithConfirmation` is invoked from two callers: (a)
 *    the context menu (via `CircuitEvents`'s thin delegate, kept for backward compat), and (b) the
 *    drag controller's document-mouseup handler when a qubit label is dragged off the circuit.
 *
 * No `dispose()` — the qubit-label elements live inside the SVG, which is replaced wholesale on
 * each `enableEvents` re-run, so their listeners die with the element.
 */
export class QubitController {
  constructor(private readonly ctx: InteractionContext) {
    this.installLabelListeners();
  }

  /**
   * Remove a qubit line, prompting first if it has any operations attached. Public because the drag
   * controller's drag-out-delete path needs to invoke it from a different mouseup handler.
   */
  removeQubitLineWithConfirmation(qubitIdx: number): void {
    const numOperations = this.ctx.model.qubitUseCounts[qubitIdx];

    const doRemove = () => {
      removeQubitWithDependents(this.ctx.model, qubitIdx);
      this.ctx.wireData.splice(qubitIdx, 1);
      this.ctx.renderFn();
    };

    if (numOperations === 0) {
      doRemove();
      return;
    }

    const message =
      numOperations === 1
        ? `There is 1 operation associated with this qubit line. Do you want to remove it?`
        : `There are ${numOperations} operations associated with this qubit line. Do you want to remove them?`;
    createConfirmPrompt(message, (confirmed) => {
      if (!confirmed) return;
      doRemove();
    });
  }

  private installLabelListeners(): void {
    const elems = getQubitLabelElems(this.ctx.container);
    elems.forEach((elem) => {
      elem.addEventListener("mousedown", (ev: MouseEvent) =>
        this.onLabelMouseDown(ev, elem),
      );
      elem.style.pointerEvents = "all";
    });
  }

  private onLabelMouseDown(ev: MouseEvent, elem: SVGTextElement): void {
    ev.stopPropagation();
    this.ctx.interaction.selectedOperation = null;
    this.spawnGhost(ev, elem);

    const sourceIndexStr = elem.getAttribute("data-wire");
    const sourceWire = sourceIndexStr != null ? parseInt(sourceIndexStr) : null;
    if (sourceWire == null) return;
    this.ctx.interaction.selectedWire = sourceWire;

    // Dropzones ON each wire (skip self). Exclude the trailing ghost wire — it's a placeholder for
    // adding new qubits, not a real swap target.
    for (
      let targetWire = 0;
      targetWire < this.ctx.wireData.length - 1;
      targetWire++
    ) {
      if (targetWire === sourceWire) continue;
      const dropzone = createWireDropzone(
        this.ctx.circuitSvg,
        this.ctx.wireData,
        targetWire,
      );
      dropzone.addEventListener("mouseup", () =>
        this.commitMove(sourceWire, targetWire, false),
      );
      trackTemporaryDropzone(this.ctx.interaction, dropzone);
      this.ctx.overlayLayer.appendChild(dropzone);
    }

    // Dropzones BETWEEN wires (including before-first and after-last, but not after the ghost
    // wire). Skip the source's own bracket positions since "insert between" at them is a no-op.
    for (let i = 0; i <= this.ctx.wireData.length - 1; i++) {
      if (i === sourceWire || i === sourceWire + 1) continue;
      const dropzone = createWireDropzone(
        this.ctx.circuitSvg,
        this.ctx.wireData,
        i,
        true,
      );
      dropzone.addEventListener("mouseup", () =>
        this.commitMove(sourceWire, i, true),
      );
      trackTemporaryDropzone(this.ctx.interaction, dropzone);
      this.ctx.overlayLayer.appendChild(dropzone);
    }
  }

  private commitMove(
    sourceWire: number,
    targetWire: number,
    isBetween: boolean,
  ): void {
    moveQubit(this.ctx.model, sourceWire, targetWire, isBetween);
    this.ctx.renderFn();
  }

  private spawnGhost(ev: MouseEvent, elem: SVGTextElement): void {
    this.ctx.interaction.dragging = true;
    enableAutoScroll(this.ctx.circuitSvg, this.ctx.interaction);
    createQubitLabelGhost(ev, this.ctx.container, elem);
  }
}
