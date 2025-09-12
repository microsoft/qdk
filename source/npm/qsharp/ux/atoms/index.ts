// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import {
  createPlayerControls,
  createScrubberControls,
  createZoomControls,
} from "./controls";
import "./index.css";
import { Layout, type ZoneLayout, type TraceData } from "./layout";
import { addChildWithClass } from "./utils";

export type { TraceData, ZoneLayout };

export function Atoms(
  container: HTMLElement,
  zoneLayout: ZoneLayout,
  trace: TraceData,
) {
  container.classList.add("qs-atoms-app");
  const toolstrip = addChildWithClass(container, "div", "qs-atoms-toolstrip");

  const zoomControls = createZoomControls();
  const playerControls = createPlayerControls();
  const scrubberControls = createScrubberControls();
  scrubberControls.setRange(trace.steps.length);

  toolstrip.appendChild(zoomControls);
  toolstrip.appendChild(scrubberControls.element);
  toolstrip.appendChild(playerControls);

  // Render the layout
  const zones = addChildWithClass(container, "div", "qs-atoms-zones");
  const layout = new Layout(zoneLayout, trace);
  zones.appendChild(layout.container);

  scrubberControls.setNavHandler((step: number) => layout.gotoStep(step));

  function setAppWidth() {
    const newWidth = layout.width * layout.scale + 32;
    // eslint-disable-next-line @typescript-eslint/no-unused-expressions
    newWidth > 600
      ? (container.style.width = `${newWidth}px`)
      : (container.style.width = "600px");
  }

  function onZoomIn() {
    layout.zoomIn();
    setAppWidth();
  }

  function onZoomOut() {
    layout.zoomOut();
    setAppWidth();
  }

  // Wire up the controls
  container.tabIndex = 0;
  container.addEventListener("keydown", (e) => {
    switch (e.key) {
      case "ArrowRight":
        e.preventDefault();
        e.stopPropagation();
        scrubberControls.next();
        break;
      case "ArrowLeft":
        e.preventDefault();
        e.stopPropagation();
        scrubberControls.prev();
        break;
      case "ArrowUp":
        e.preventDefault();
        e.stopPropagation();
        onZoomIn();
        break;
      case "ArrowDown":
        e.preventDefault();
        e.stopPropagation();
        onZoomOut();
        break;
      case "f":
        e.preventDefault();
        e.stopPropagation();
        layout.faster();
        break;
      case "s":
        e.preventDefault();
        e.stopPropagation();
        layout.slower();
        break;
      case "p":
        e.preventDefault();
        e.stopPropagation();
        onPlayPause();
        break;
    }
  });

  const next = container.querySelector(
    "[data-control='next']",
  ) as SVGCircleElement;
  const prev = container.querySelector(
    "[data-control='prev']",
  ) as SVGCircleElement;
  const play = container.querySelector(
    "[data-control='play']",
  ) as SVGCircleElement;
  const pause = container.querySelector(
    "[data-control='pause']",
  ) as SVGCircleElement;
  const zoomIn = container.querySelector(
    "[data-control='zoom-in']",
  ) as SVGCircleElement;
  const zoomOut = container.querySelector(
    "[data-control='zoom-out']",
  ) as SVGCircleElement;

  let playTimer: any; // Different platforms have different types for setTimeout
  function onPlayPause() {
    // If it was playing, pause it
    if (playTimer) {
      pause.parentElement!.style.display = "none";
      play.parentElement!.style.display = "inline";
      clearTimeout(playTimer);
      playTimer = undefined;
      return;
    }

    // Else start playing
    play.parentElement!.style.display = "none";
    pause.parentElement!.style.display = "inline";

    if (scrubberControls.isAtEnd()) scrubberControls.reset();

    function onTimeout() {
      if (scrubberControls.isAtEnd()) {
        pause.parentElement!.style.display = "none";
        play.parentElement!.style.display = "inline";
        playTimer = undefined;
      } else {
        scrubberControls.next();
        playTimer = setTimeout(onTimeout, layout.stepInterval);
      }
    }
    playTimer = setTimeout(onTimeout, 0);
  }

  next.addEventListener("click", () => scrubberControls.next());
  prev.addEventListener("click", () => scrubberControls.prev());
  zoomIn.addEventListener("click", onZoomIn);
  zoomOut.addEventListener("click", onZoomOut);

  play.addEventListener("click", onPlayPause);
  pause.addEventListener("click", onPlayPause);

  setTimeout(onZoomOut, 16);
}
