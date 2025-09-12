// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { appendChildren, createSvgElements, setAttributes } from "./utils";

// Icons are 256x256, with a 32 spacing inbetween
export function getIcon(name: string): SVGGElement {
  const g = createSvgElements("g")[0] as SVGGElement;
  g.setAttribute("class", "qs-atoms-icon");

  const circle = createSvgElements("circle")[0] as SVGCircleElement;
  setAttributes(circle, {
    cx: "128",
    cy: "128",
    r: "116",
    "data-control": name,
  });

  switch (name) {
    case "prev":
      {
        const [path1, path2] = createSvgElements("path", "path");
        setAttributes(path1, { d: "M60,128 l55,-32 l0,64 z" });
        setAttributes(path2, { d: "M122,128 l55,-32 l0,64 z" });
        appendChildren(g, [circle, path1, path2]);
      }
      break;
    case "pause":
      {
        const [path1, path2] = createSvgElements("path", "path");
        setAttributes(path1, { d: "M99 80 l16 0 l0 96 l-16 0 z" });
        setAttributes(path2, { d: "M141 80 l16 0 l0 96 l-16 0 z" });
        appendChildren(g, [circle, path1, path2]);
      }
      break;
    case "play":
      {
        const [path1] = createSvgElements("path");
        setAttributes(path1, { d: "M180,128 l-83,-47 l0,96 z" });
        appendChildren(g, [circle, path1]);
      }
      break;
    case "next":
      {
        const [path1, path2] = createSvgElements("path", "path");
        setAttributes(path1, { d: "M196,128 l-55,-32 l0,64 z" });
        setAttributes(path2, { d: "M134,128 l-55,-32 l0,64 z" });
        appendChildren(g, [circle, path1, path2]);
      }
      break;
    case "zoom-in":
      {
        const [path1] = createSvgElements("path");
        setAttributes(path1, {
          d: "M68 144 l60 -50 l60 50",
          class: "qs-atoms-icon-line",
        });
        appendChildren(g, [circle, path1]);
      }
      break;
    case "zoom-out":
      {
        const [path1] = createSvgElements("path");
        setAttributes(path1, {
          d: "M68 112 l60 50 l60 -50",
          class: "qs-atoms-icon-line",
        });
        appendChildren(g, [circle, path1]);
      }
      break;
    default:
      throw "Unknown icon";
  }

  return g;
}

export function createPlayerControls(): SVGSVGElement {
  const svg = createSvgElements("svg")[0] as SVGSVGElement;
  const width = 256 * 3 + 32 * 2;
  setAttributes(svg, {
    viewBox: `0 0 ${width} 256`,
    class: "qs-atoms-toolbar",
  });
  const prev = getIcon("prev");
  prev.setAttribute("transform", "translate(0)");

  const play = getIcon("play");
  play.setAttribute("transform", "translate(288)");

  const pause = getIcon("pause");
  pause.setAttribute("transform", "translate(288)");
  pause.style.display = "none"; // Hidden if not playing

  const next = getIcon("next");
  next.setAttribute("transform", "translate(576)");

  appendChildren(svg, [prev, play, pause, next]);
  return svg;
}

export function createZoomControls(): SVGSVGElement {
  const svg = createSvgElements("svg")[0] as SVGSVGElement;
  const width = 256 * 2 + 32;
  setAttributes(svg, {
    viewBox: `0 0 ${width} 256`,
    class: "qs-atoms-toolbar qs-atoms-toolbar-left",
  });
  const zoomIn = getIcon("zoom-in");
  zoomIn.setAttribute("transform", "translate(0)");

  const zoomOut = getIcon("zoom-out");
  zoomOut.setAttribute("transform", "translate(288)");

  appendChildren(svg, [zoomIn, zoomOut]);
  return svg;
}

export interface Scrubber {
  element: HTMLDivElement;
  setNavHandler(handler: (step: number) => void): void;
  setRange(len: number): void;
  reset(): void;
  next(): void;
  prev(): void;
  isAtEnd(): boolean;
}

export function createScrubberControls(): Scrubber {
  let navHandler: ((step: number) => void) | null = null;
  let val = 0;
  let max = 0;

  const div = document.createElement("div");
  div.style.margin = "0 auto";

  const span = document.createElement("span");
  span.classList.add("qs-atoms-step");

  const slider = document.createElement("input");
  slider.type = "range";
  slider.classList.add("qs-atoms-slider");
  slider.id = "slider";
  slider.min = "0";
  slider.max = "0";
  slider.value = "0";

  // oninput gives constant updates while scrubbing, whereas onchange is only once released
  slider.oninput = (ev) => {
    ev.preventDefault();
    val = parseInt((ev.target as HTMLInputElement).value);
    render();
    if (navHandler) navHandler(val);
  };

  div.appendChild(span);
  div.appendChild(slider);

  function setRange(len: number) {
    max = len;
    slider.max = `${len}`;
    render();
  }

  function render() {
    span.innerText = `${val} / ${max}`;
    slider.value = `${val}`;
  }

  function next() {
    if (val < max) {
      val += 1;
      render();
      if (navHandler) navHandler(val);
    }
  }

  function prev() {
    if (val > 0) {
      val -= 1;
      render();
      if (navHandler) navHandler(val);
    }
  }

  render();

  return {
    element: div,
    setRange,
    setNavHandler: (handler) => (navHandler = handler),
    next,
    prev,
    reset: () => (val = 0),
    isAtEnd: () => val === max,
  };
}
