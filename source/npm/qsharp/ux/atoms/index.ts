// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import {
  createPlayerControls,
  createScrubberControls,
  createZoomControls,
} from "./controls.js";
import "./index.css";
import { Layout, type ZoneLayout, type TraceData } from "./layout.js";
import { addChildWithClass } from "./utils.js";

export type { TraceData, ZoneLayout };

export function Atoms(
  container: HTMLElement,
  zoneLayout: ZoneLayout,
  trace: TraceData,
) {
  // On first load, Python might still be syncing the underlying model, so just bail if required data is missing
  if (!zoneLayout.zones?.length || !trace.steps?.length) return;

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
      case "t":
        e.preventDefault();
        e.stopPropagation();
        layout.cycleAnimation();
        break;
    }
  });

  const info = container.querySelector(
    "[data-control='info']",
  ) as SVGCircleElement;
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

  const infoBox = document.createElement("div");
  infoBox.innerHTML = `
  <b>Keyboard shortcuts</b>
  <div class="qs-atoms-info-box-grid">
    <div>Left</div><div>Step back</div>
    <div>Right</div><div>Step forward</div>
    <div>Up</div><div>Zoom in</div>
    <div>Down</div><div>Zoom out</div>
    <div>P</div><div>Play / Pause </div>
    <div>F</div><div>Faster animation</div>
    <div>S</div><div>Slower animation</div>
    <div>T</div><div>Toggle animation type</div>
  </div>
  `;
  infoBox.classList.add("qs-atoms-info-box");
  container.appendChild(infoBox);

  info.addEventListener("mouseover", () => {
    infoBox.style.display = "block";
  });
  info.addEventListener("mouseout", () => {
    infoBox.style.display = "none";
  });

  next.addEventListener("click", () => scrubberControls.next());
  prev.addEventListener("click", () => scrubberControls.prev());
  zoomIn.addEventListener("click", onZoomIn);
  zoomOut.addEventListener("click", onZoomOut);

  play.addEventListener("click", onPlayPause);
  pause.addEventListener("click", onPlayPause);

  setTimeout(onZoomOut, 16);
}
