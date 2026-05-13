// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { InteractionState } from "../actions/interactionState.js";

/**
 * `enableAutoScroll` — install document-level mousemove listener that
 * scrolls the nearest scrollable ancestor of `circuitSvg` whenever
 * the cursor approaches an edge. Self-removes on the next `mouseup`.
 *
 * Invoked at drag start by both the gate-drag flow and the
 * qubit-label drag flow, hence the standalone shape.
 *
 * @param circuitSvg  The rendered `svg.qviz` root; used as the
 *                    starting point for the scrollable-ancestor
 *                    search.
 * @param interaction Session state. Reads/writes
 *                    `disableLeftAutoScroll` so that the drag flow
 *                    can opt out of left-edge auto-scroll once the
 *                    user has moved the cursor far enough right.
 */
export const enableAutoScroll = (
  circuitSvg: SVGElement,
  interaction: InteractionState,
): void => {
  const scrollSpeed = 10; // Pixels per frame
  const edgeThreshold = 50; // Distance from the edge to trigger scrolling

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
    return document.documentElement;
  };

  const scrollableAncestor = getScrollableAncestor(circuitSvg);

  const onMouseMove = (ev: MouseEvent) => {
    const rect = scrollableAncestor.getBoundingClientRect();

    const topBoundary = rect.top;
    const bottomBoundary = rect.bottom;
    const leftBoundary = rect.left;
    const rightBoundary = rect.right;

    // If the mouse has moved past the left boundary, re-enable left auto-scroll
    if (
      interaction.disableLeftAutoScroll &&
      ev.clientX > leftBoundary + 3 * edgeThreshold
    ) {
      interaction.disableLeftAutoScroll = false;
    }

    if (ev.clientY < topBoundary + edgeThreshold) {
      scrollableAncestor.scrollTop -= scrollSpeed;
    } else if (ev.clientY > bottomBoundary - edgeThreshold) {
      scrollableAncestor.scrollTop += scrollSpeed;
    }

    if (
      !interaction.disableLeftAutoScroll &&
      ev.clientX < leftBoundary + edgeThreshold
    ) {
      scrollableAncestor.scrollLeft -= scrollSpeed;
    } else if (ev.clientX > rightBoundary - edgeThreshold) {
      scrollableAncestor.scrollLeft += scrollSpeed;
    }
  };

  const onMouseUp = () => {
    document.removeEventListener("mousemove", onMouseMove);
    document.removeEventListener("mouseup", onMouseUp);
  };

  document.addEventListener("mousemove", onMouseMove);
  document.addEventListener("mouseup", onMouseUp);
};
