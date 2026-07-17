// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { CircuitEvents } from "../events.js";
import { addContextMenuToHostElem } from "../contextMenu.js";
import { InteractionContext } from "./interactionContext.js";
import { getHostElems, parseWireYs } from "../domUtils.js";
import { pickClosestWireIndex } from "../../utils.js";

/**
 * `SelectionController` — owns mousedown on **host elements** (the inner clickable bits of a gate:
 * control dots, target circles, targets/measure crosses), and attaches the context menu.
 *
 * Intentionally small: it only captures the wire under the cursor (so the drag controller knows
 * which qubit is being grabbed) and flags whether the grab was on a control dot (so a drag can
 * detach just that control instead of the whole gate).
 *
 * The actual gate-drag start lives in `DragController`, which listens for mousedown on the outer
 * `.gate` element. Both fire during the same physical click — the host listener runs first (deeper
 * in the DOM) so `selectedWire` is set by the time the drag controller's gate handler runs.
 *
 * The context menu still receives the full `CircuitEvents` because `addContextMenuToHostElem`
 * expects it.
 *
 * No `dispose()` — host elements live inside the SVG, replaced wholesale on each `enableEvents`
 * re-run.
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
   * Resolve "which wire did the user grab?" for a click on a host element. The grabbed wire is the
   * handle that [`_moveY`](../../actions/circuitActions.ts) slides by `targetWire - sourceWire`.
   *
   * Two paths:
   *
   *   1. **Single-wire host elem** (control dots, target circles, measurement crosses, ket boxes,
   *      single-target unitary bodies). The static `data-wire` set by
   *      [`_addDataWires`](../draggable.ts) is exactly right.
   *
   *   2. **Multi-wire host elem** (group body, SWAP, multi-qubit measurement). The static
   *      `data-wire` is always the topmost wire of the span, which would degrade unit-shift to "pin
   *      top wire to drop wire". Instead, project the click into SVG coords and pick the closest
   *      wire-Y via [`pickClosestWireIndex`](../../utils.ts).
   *
   * Fallback: if `getScreenCTM()` returns `null` or the closest-wire lookup fails, fall back to the
   * static `data-wire` attribute.
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

    // `circuitSvg` is typed as the looser `SVGElement` in `InteractionContext` but is always the
    // root `<svg>` at runtime. Cast locally to reach `getScreenCTM` without widening the type.
    const svg = this.ctx.circuitSvg as unknown as SVGSVGElement;
    const ctm = svg.getScreenCTM();
    if (ctm == null) return fallback();
    const pt = new DOMPoint(ev.clientX, ev.clientY);
    const svgPt = pt.matrixTransform(ctm.inverse());

    const wireIndex = pickClosestWireIndex(svgPt.y, wireYs, this.ctx.wireData);
    return wireIndex >= 0 ? wireIndex : fallback();
  }
}
