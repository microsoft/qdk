// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { CircuitEvents } from "../events.js";
import { addContextMenuToHostElem } from "../contextMenu.js";
import { InteractionContext } from "./interactionContext.js";
import { getHostElems } from "../../utils.js";

/**
 * `SelectionController` — owns mousedown on **host elements** (the
 * inner clickable bits of a gate: control dots, target circles,
 * targets/measure crosses), and attaches the context menu.
 *
 * Intentionally small: it only captures the wire under the cursor
 * (so the drag controller knows which qubit is being grabbed) and
 * flags whether the grab was on a control dot (so a drag can detach
 * just that control instead of the whole gate).
 *
 * The actual gate-drag start lives in `DragController`, which
 * listens for mousedown on the outer `.gate` element. Both fire
 * during the same physical click — the host listener runs first
 * (deeper in the DOM) so `selectedWire` is set by the time the
 * drag controller's gate handler runs.
 *
 * The context menu still receives the full `CircuitEvents` because
 * `addContextMenuToHostElem` expects it. Slimming that dependency
 * is a follow-up.
 *
 * No `dispose()` — host elements live inside the SVG, replaced
 * wholesale on each `enableEvents` re-run.
 */
export class SelectionController {
  constructor(
    private readonly ctx: InteractionContext,
    private readonly events: CircuitEvents,
  ) {
    this.installHostListeners();
  }

  private installHostListeners(): void {
    const elems = getHostElems(this.ctx.container);
    elems.forEach((elem) => {
      elem.addEventListener("mousedown", (ev: MouseEvent) =>
        this.onHostMouseDown(ev, elem),
      );
      addContextMenuToHostElem(this.events, elem);
    });
  }

  private onHostMouseDown(ev: MouseEvent, elem: SVGGraphicsElement): void {
    if (ev.button !== 0) return;
    if (elem.classList.contains("control-dot")) {
      this.ctx.interaction.movingControl = true;
    }
    const selectedWireStr = elem.getAttribute("data-wire");
    this.ctx.interaction.selectedWire =
      selectedWireStr != null ? parseInt(selectedWireStr) : null;
  }
}
