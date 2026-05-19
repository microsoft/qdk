// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { CircuitEvents } from "../events.js";
import { addContextMenuToHostElem } from "../contextMenu.js";
import { InteractionContext } from "./interactionContext.js";
import {
  getHostElems,
  parseWireYs,
  pickClosestWireIndex,
} from "../../utils.js";

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
    this.ctx.interaction.selectedWire = this.pickSelectedWire(ev, elem);
  }

  /**
   * Resolve "which wire did the user grab?" for a single click on a
   * host element. Backs the D3 design contract that
   * [`_moveY`](../../actions/circuitActions.ts) relies on: the
   * grabbed wire is the handle, and the whole op slides by
   * `targetWire - sourceWire`.
   *
   * Two paths:
   *
   *   1. **Single-wire host elem** (control dots, target circles,
   *      measurement crosses, ket boxes, and single-target unitary
   *      bodies). The static `data-wire` attribute set by
   *      [`_addDataWires`](../draggable.ts) is exactly right;
   *      `data-wire-ys` parses to a one-element array, and we just
   *      use the attribute the renderer / draggable handshake
   *      already wrote.
   *
   *   2. **Multi-wire host elem** (the body of a group, SWAP,
   *      multi-qubit measurement). The static `data-wire` is
   *      always the topmost wire of the span (an artifact of
   *      `_addDataWires`'s `findIndex`-on-`includes` shortcut),
   *      which would silently degrade unit-shift to "pin top wire
   *      to drop wire". Instead, project the click's
   *      `(clientX, clientY)` into SVG coords and pick the wire-Y
   *      closest to it via [`pickClosestWireIndex`](../../utils.ts).
   *
   * Fallback: if `getScreenCTM()` returns `null` (SVG is in a
   * detached subtree or some other browser edge case) or the
   * closest-wire lookup fails, fall back to the static `data-wire`
   * attribute so the click still resolves *some* wire.
   */
  private pickSelectedWire(
    ev: MouseEvent,
    elem: SVGGraphicsElement,
  ): number | null {
    const fallback = (): number | null => {
      const attr = elem.getAttribute("data-wire");
      return attr != null ? parseInt(attr) : null;
    };

    const wireYs = parseWireYs(elem);
    // Single-wire / unknown spans go straight to the static attr.
    if (wireYs.length <= 1) return fallback();

    // `circuitSvg` is typed as the looser `SVGElement` in
    // `InteractionContext` (it's only ever the root `<svg>` at
    // runtime). Cast locally to `SVGSVGElement` to reach
    // `getScreenCTM` without widening the context type.
    const svg = this.ctx.circuitSvg as unknown as SVGSVGElement;
    const ctm = svg.getScreenCTM();
    if (ctm == null) return fallback();
    const pt = new DOMPoint(ev.clientX, ev.clientY);
    const svgPt = pt.matrixTransform(ctm.inverse());

    const wireIndex = pickClosestWireIndex(svgPt.y, wireYs, this.ctx.wireData);
    return wireIndex >= 0 ? wireIndex : fallback();
  }
}
