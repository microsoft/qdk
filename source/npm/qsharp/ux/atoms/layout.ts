// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { appendChildren, createSvgElements, setAttributes } from "./utils.js";

// Zones will be rendered from top to bottom in array order
type ZoneData = {
  title: string;
  rows: number;
  kind: "register" | "interaction" | "measurement";
};

export type ZoneLayout = {
  cols: number;
  zones: ZoneData[];
};

export type TraceData = {
  metadata: any;
  qubits: Array<[number, number]>;
  steps: Array<{
    id: string | number;
    ops: Array<string>;
  }>;
};

// const exampleLayout: ZoneLayout = {
//   "cols": 36,
//   "zones": [
//     { "title": "Register 1", "rows": 17, "kind": "register" },
//     { "title": "Interaction Zone", "rows": 4, "kind": "interaction" },
//     { "title": "Register 2", "rows": 17, "kind": "register" },
//     { "title": "Measurement Zone", "rows": 4, "kind": "measurement" },
//   ],
// };

type Location = [number, number, SVGElement?];

const qubitSize = 10;
const zoneSpacing = 8;
const colPadding = 20;
const initialScale = 1.5;
const scaleStep = 0.15;
const speedStep = 0.75;
const zoneBoxCornerRadius = 3;
const doublonCornerRadius = 5;

// Used when no trace data is provided to fill the qubit mappings, assuming all register
// zones are populated with sequentially numbered qubit ids from the top left to the bottom right.
export function fillQubitLocations(
  layout: ZoneLayout,
): Array<[number, number]> {
  const qubits: Array<[number, number]> = [];
  let currRow = 0;
  layout.zones.forEach((zone) => {
    for (let row = 0; row < zone.rows; ++row) {
      if (zone.kind === "register") {
        for (let col = 0; col < layout.cols; ++col) {
          qubits.push([currRow, col]);
        }
      }
      ++currRow;
    }
  });

  return qubits;
}

function parseMove(op: string): { qubit: number; to: Location } | undefined {
  const match = op.match(/move\((\d+), (\d+)\) (\d+)/);
  if (match) {
    const to: Location = [parseInt(match[1]), parseInt(match[2])];
    return { qubit: parseInt(match[3]), to };
  }
  return undefined;
}

function parseGate(
  op: string,
): { gate: string; qubit: number; arg?: string } | undefined {
  const match = op.match(/(\w+)\s*(\(.*\))? (\d+)/);
  if (match) {
    const gate = match[1];
    const qubit = parseInt(match[3]);
    const arg = match[2]
      ? match[2].substring(1, match[2].length - 2)
      : undefined;
    return { gate, qubit, arg };
  }
}

/*
We want to build up a cache of the qubit location at each 'n' steps so that scrubbing is fast.
Storing for each step is too large for the number of steps we want to handle. Also, the building
of the cache can take a long time, so we want to chunk it up to avoid blocking the UI thread.
*/
function TraceToGetLayoutFn(trace: TraceData) {
  const STEP_SIZE = 100; // How many steps between each cache entry

  const cacheEntries = Math.ceil(trace.steps.length / STEP_SIZE);
  const entrySize = trace.qubits.length * 2; // row and col for each qubit
  const cache = new Uint16Array(cacheEntries * entrySize);

  // Fill initial locations
  trace.qubits.forEach((loc, idx) => {
    cache[idx * 2] = loc[0];
    cache[idx * 2 + 1] = loc[1];
  });

  let lastIndexProcessed = 0;

  // Update the layout (which should be the prior step layout)
  function getNextStepLayout(stepIndex: number, layout: Uint16Array) {
    if (stepIndex <= 0 || stepIndex >= trace.steps.length) {
      throw "Step out of range";
    }
    // Extract the move operations in the prior step, to apply to the prior layout
    const moves = trace.steps[stepIndex - 1].ops
      .map(parseMove)
      .filter((x) => x != undefined);

    // Then apply them to the layout
    moves.forEach((move) => {
      layout[move.qubit * 2] = move.to[0];
      layout[move.qubit * 2 + 1] = move.to[1];
    });
  }

  // syncRunToIndex is used to force processing up to a certain index synchronously, such as
  // when the user is scrubbing to a point not yet processed asynchronously.
  function processChunk(syncRunToIndex = 0) {
    if (lastIndexProcessed >= cacheEntries - 1) {
      return; // Done
    }

    const startPriorIndex = lastIndexProcessed * entrySize;
    const startIndex = (lastIndexProcessed + 1) * entrySize;

    // Copy to the next entry as the starting point
    cache.copyWithin(startIndex, startPriorIndex, startPriorIndex + entrySize);
    const targetSlice = cache.subarray(startIndex, startIndex + entrySize);

    // Run each step in the chunk and apply the moves to the cache
    for (let stepOffset = 1; stepOffset <= STEP_SIZE; ++stepOffset) {
      const stepIndex = lastIndexProcessed * STEP_SIZE + stepOffset;
      if (stepIndex >= trace.steps.length) break;
      getNextStepLayout(stepIndex, targetSlice);
    }

    // Queue up the next chunk
    ++lastIndexProcessed;
    if (syncRunToIndex > 0) {
      // Process synchronously up to the requested index if not done yet
      if (lastIndexProcessed < syncRunToIndex) processChunk(syncRunToIndex);
    } else {
      // Process the next chunk asynchronously
      setTimeout(processChunk, 0);
    }
  }
  // Kick off the async processing
  processChunk();

  // When a layout is requested for a step, cache the response. Most of the time the following
  // step will be requested next, so we can easily just apply one set of moves to the prior layout.
  let lastRequestedStep = -1;
  const lastLayout = new Uint16Array(entrySize);

  function getLayoutAtStep(step: number): Uint16Array {
    if (step < 0 || step >= trace.steps.length) {
      throw "Step out of range";
    }

    // If the cache hasn't processed up to the required step, do so now
    if (step >= (lastIndexProcessed + 1) * STEP_SIZE) {
      const entryIndex = Math.floor(step / STEP_SIZE);
      processChunk(entryIndex);
    }

    // If the step exactly matches a cache entry, return that
    if (step % STEP_SIZE === 0) {
      const entryIndex = step / STEP_SIZE;
      const startIndex = entryIndex * entrySize;
      lastLayout.set(cache.subarray(startIndex, startIndex + entrySize));
      lastRequestedStep = step;
      return lastLayout;
    }

    // If the same as the last requested step, just return the cached layout
    if (step === lastRequestedStep) {
      return lastLayout;
    }

    // Otherwise, if the last requested step was the prior step, just apply the moves
    if (step === lastRequestedStep + 1) {
      getNextStepLayout(step, lastLayout);
      ++lastRequestedStep;
      return lastLayout;
    }

    // Otherwise, find the nearest prior cache entry and apply moves from there
    const entryIndex = Math.floor(step / STEP_SIZE);
    const startIndex = entryIndex * entrySize;
    lastLayout.set(cache.subarray(startIndex, startIndex + entrySize));
    for (
      let stepIndex = entryIndex * STEP_SIZE + 1;
      stepIndex <= step;
      ++stepIndex
    ) {
      getNextStepLayout(stepIndex, lastLayout);
    }
    lastRequestedStep = step;
    return lastLayout;
  }

  return getLayoutAtStep;
}

export class Layout {
  container: SVGSVGElement;
  width: number;
  height: number;
  scale: number = initialScale;
  qubits: Location[];
  rowOffset: number[];
  currentStep = 0;
  trackParent: SVGGElement;
  activeGates: SVGElement[] = [];
  trace: TraceData;
  getStepLayout: (step: number) => Uint16Array;
  showTracks = true;
  showDottedPath = true;
  stepInterval = 500; // Used for playing and animations

  constructor(
    public layout: ZoneLayout,
    trace: TraceData,
  ) {
    if (!trace.qubits?.length) {
      trace.qubits = fillQubitLocations(layout);
    }
    this.trace = trace;
    this.getStepLayout = TraceToGetLayoutFn(trace);

    this.qubits = structuredClone(trace.qubits);

    this.container = document.createElementNS(
      "http://www.w3.org/2000/svg",
      "svg",
    );

    const totalRows = layout.zones.reduce((prev, curr) => prev + curr.rows, 0);

    this.height =
      totalRows * qubitSize + zoneSpacing * (layout.zones.length + 1);
    this.width = layout.cols * qubitSize + colPadding;

    setAttributes(this.container, {
      viewBox: `-5 0 ${this.width} ${this.height}`,
      width: `${this.width * this.scale}px`,
      height: `${this.height * this.scale}px`,
    });

    // Loop through the zones, calculating the row offsets, and rendering the zones
    this.rowOffset = [];
    let nextOffset = zoneSpacing;
    let nextRowNum = 0;
    layout.zones.forEach((zone, index) => {
      this.renderZone(index, nextOffset, nextRowNum);
      for (let i = 0; i < zone.rows; ++i) {
        this.rowOffset.push(nextOffset);
        nextOffset += qubitSize;
        ++nextRowNum;
      }
      nextOffset += zoneSpacing; // Add spacing after each zone
    });

    const colNumOffset = nextOffset - 8;
    this.renderColNums(layout.cols, colNumOffset);

    // Put the track parent before the qubits, so the qubits render on top
    this.trackParent = createSvgElements("g")[0] as SVGGElement;

    this.trackParent.addEventListener("mouseover", (e) => {
      const t = e.target as SVGElement;
      if (t && t.dataset.qubitid) {
        const qubitId = parseInt(t.dataset.qubitid);
        this.highlightQubit(qubitId);
      }
    });
    this.trackParent.addEventListener("mouseout", () => {
      this.highlightQubit(null);
    });

    appendChildren(this.container, [this.trackParent]);

    this.renderQubits();
  }

  private highlightQubit(qubit: number | null) {
    if (qubit === null || qubit < 0 || qubit >= this.qubits.length) {
      this.container
        .querySelectorAll(".qs-atoms-qubit-highlight")
        .forEach((elem) => {
          elem.classList.remove("qs-atoms-qubit-highlight");
        });
    } else {
      this.container
        .querySelectorAll(`[data-qubitId="${qubit}"]`)
        .forEach((elem) => {
          elem.classList.add("qs-atoms-qubit-highlight");
        });
    }
  }

  private getOpsAtStep(step: number) {
    if (step < 0 || step >= this.trace.steps.length) {
      throw "Step out of range";
    }
    return this.trace.steps[step].ops;
  }

  private renderZone(zoneIndex: number, offset: number, firstRowNum = 0) {
    const zoneData = this.layout.zones[zoneIndex];
    const g = createSvgElements("g")[0];
    setAttributes(g, {
      transform: `translate(0 ${offset})`,
      class: "qs-atoms-zonebox",
    });

    if (zoneData.kind !== "interaction") {
      // For non-interaction zones we draw one big rounded rectangle with lines between qubit rows & cols
      const rect = createSvgElements("rect")[0];
      setAttributes(rect, {
        x: "0",
        y: "0",
        width: `${this.layout.cols * qubitSize}`,
        height: `${zoneData.rows * qubitSize}`,
        rx: `${zoneBoxCornerRadius}`,
      });
      appendChildren(g, [rect]);

      // Draw the lines between the rows
      for (let i = 1; i < zoneData.rows; i++) {
        const path = createSvgElements("path")[0];
        setAttributes(path, {
          d: `M 0,${i * qubitSize} h${this.layout.cols * qubitSize}`,
        });
        appendChildren(g, [path]);
      }
      // Draw the lines between the columns
      for (let i = 1; i < this.layout.cols; i++) {
        const path = createSvgElements("path")[0];
        setAttributes(path, {
          d: `M ${i * qubitSize},0 v${zoneData.rows * qubitSize}`,
        });
        appendChildren(g, [path]);
      }
    } else {
      // For the interaction zone draw each doublon
      for (let row = 0; row < zoneData.rows; ++row) {
        for (let i = 0; i < this.layout.cols; i += 2) {
          const rect = createSvgElements("rect")[0];
          setAttributes(rect, {
            x: `${i * qubitSize}`,
            y: `${row * qubitSize}`,
            width: `${qubitSize * 2}`,
            height: `${qubitSize}`,
            rx: `${doublonCornerRadius}`,
          });
          const path = createSvgElements("path")[0];
          setAttributes(path, {
            d: `M ${(i + 1) * qubitSize},${row * qubitSize} v${qubitSize}`,
          });
          appendChildren(g, [rect, path]);
        }
      }
    }

    // Number the rows
    for (let i = 0; i < zoneData.rows; ++i) {
      const rowNum = firstRowNum + i;
      const label = createSvgElements("text")[0];
      setAttributes(label, {
        x: `${this.layout.cols * qubitSize + 5}`,
        y: `${i * qubitSize + 5}`,
        class: "qs-atoms-label",
      });
      label.textContent = `${rowNum}`;
      appendChildren(g, [label]);
    }

    // Draw the title
    const text = createSvgElements("text")[0];
    setAttributes(text, {
      x: "1",
      y: "-1",
      class: "qs-atoms-zone-text",
    });
    text.textContent = zoneData.title;

    appendChildren(g, [text]);
    appendChildren(this.container, [g]);
  }

  private renderQubits() {
    const elems = this.qubits.map((location, index) => {
      const [x, y] = this.getQubitCenter(index);

      // Safari has an issue animating multiple attributes concurrently, which we need
      // to do to move the qubit (animate 'cx' and 'cy'), so instead set cx and cy to 0
      // and position the qubit with a transform (see https://stackoverflow.com/a/72022385/1674945)

      const circle = createSvgElements("circle")[0];
      setAttributes(circle, {
        cx: `0`,
        cy: `0`,
        r: `2`,
        class: "qs-atoms-qubit",
        "data-qubitid": `${index}`,
      });
      circle.addEventListener("mouseover", () => this.highlightQubit(index));
      circle.addEventListener("mouseout", () => this.highlightQubit(null));
      // Animation sets the transform as a style attribute, not an element attribute.
      // Also note, when animating the CSS it requires the 'px' length type (unlike the attribute).
      circle.style.transform = `translate(${x}px, ${y}px)`;
      location[2] = circle;
      return circle;
    });

    appendChildren(this.container, elems);
  }

  private renderColNums(cols: number, offset: number) {
    const g = createSvgElements("g")[0];
    setAttributes(g, {
      transform: `translate(0 ${offset})`,
    });
    // Number the columns
    for (let i = 0; i < cols; ++i) {
      const label = createSvgElements("text")[0];
      setAttributes(label, {
        x: `${i * qubitSize + 5}`,
        y: `5`,
        class: "qs-atoms-label",
      });
      label.textContent = `${i}`;
      appendChildren(g, [label]);
    }
    appendChildren(this.container, [g]);
  }

  renderGateOnQubit(qubit: number, gate: string, arg?: string) {
    if (gate == "RESET") gate = "R";
    const [x, y] = this.getQubitCenter(qubit);

    const gateClass =
      gate === "MZ"
        ? "qs-atoms-gate qs-atoms-gate-mz"
        : gate === "R"
          ? "qs-atoms-gate qs-atoms-gate-reset"
          : "qs-atoms-gate";

    const g = createSvgElements("g")[0];
    setAttributes(g, {
      transform: `translate(${x - qubitSize / 2} ${y - qubitSize / 2})`,
      class: "qs-atoms-gate",
    });

    if (gate === "CZ") {
      // Render the rounded doublon box in a bright color and x--x inside
      const [rect, path, leftDot, rightDot] = createSvgElements(
        "rect",
        "path",
        "circle",
        "circle",
      );
      setAttributes(rect, {
        x: `0`,
        y: "0",
        width: `${qubitSize * 2}`,
        height: `${qubitSize}`,
        rx: `${doublonCornerRadius}`,
        class: "qs-atoms-zonebox qs-atoms-gate-cz",
      });
      // <path d= "M45,5 h10" stroke-width="1.5" stroke="black"/>
      // <circle cx="45" cy="5" r="2" stroke-width="0" fill="#123" />
      // <circle cx="55" cy="5" r="2" stroke-width="0" fill="#123" />
      setAttributes(path, {
        fill: "none",
        stroke: "black",
        "stroke-width": "1.5",
        d: "M5,5 h10",
      });
      setAttributes(leftDot, {
        cx: "5",
        cy: "5",
        r: "2",
        "stroke-width": "0",
        fill: "#123",
      });
      setAttributes(rightDot, {
        cx: "15",
        cy: "5",
        r: "2",
        "stroke-width": "0",
        fill: "#123",
      });
      appendChildren(g, [rect, path, leftDot, rightDot]);
    } else {
      const [rect, text] = createSvgElements("rect", "text");
      setAttributes(rect, {
        x: "0.5",
        y: "0.5",
        width: `${qubitSize - 1}`,
        height: `${qubitSize - 1}`,
        class: gateClass,
      });
      setAttributes(text, {
        x: "5",
        y: arg ? "2.75" : "5",
        class: "qs-atoms-gate-text",
      });
      text.textContent = gate;

      appendChildren(g, [rect, text]);

      if (arg) {
        const argText = createSvgElements("text")[0];
        setAttributes(argText, {
          x: "5",
          y: "7",
          class: "qs-atoms-gate-text qs-atoms-gate-text-small",
          textLength: "8",
        });
        text.classList.add("qs-atoms-gate-text-small");
        argText.textContent = arg;
        appendChildren(g, [argText]);
      }
    }

    appendChildren(this.container, [g]);
    this.activeGates.push(g);
  }

  clearGates() {
    this.activeGates.forEach((gate) => {
      gate.parentElement?.removeChild(gate);
    });
    // TODO: Clear doublons too
    this.activeGates = [];
  }

  zoomIn() {
    this.scale += scaleStep * this.scale;
    setAttributes(this.container, {
      width: `${this.width * this.scale}px`,
      height: `${this.height * this.scale}px`,
    });
  }

  zoomOut() {
    this.scale -= scaleStep * this.scale;
    setAttributes(this.container, {
      width: `${this.width * this.scale}px`,
      height: `${this.height * this.scale}px`,
    });
  }

  faster() {
    this.stepInterval = this.stepInterval * speedStep;
  }

  slower() {
    this.stepInterval = this.stepInterval / speedStep;
  }

  cycleAnimation() {
    if (!this.showTracks) {
      this.showTracks = true;
      this.showDottedPath = true;
    } else if (this.showTracks && this.showDottedPath) {
      this.showDottedPath = false;
    } else {
      this.showTracks = false;
      this.showDottedPath = false;
    }
    this.gotoStep(this.currentStep);
  }

  getQubitRowOffset(row: number) {
    return this.rowOffset[row];
  }

  getLocationCenter(row: number, col: number): [number, number] {
    const x = col * qubitSize + qubitSize / 2;
    const y = this.getQubitRowOffset(row) + qubitSize / 2;
    return [x, y];
  }

  getQubitCenter(qubit: number): [number, number] {
    if (this.qubits[qubit] == undefined) {
      throw "Qubit not found";
    }

    const [row, col] = this.qubits[qubit];
    return this.getLocationCenter(row, col);
  }

  gotoStep(step: number) {
    // Remove prior rendering's gates, trails, or remaining animations
    this.clearGates();
    this.trackParent.replaceChildren();
    this.container.getAnimations({ subtree: true }).forEach((anim) => {
      anim.cancel();
    });

    const forwards = step > this.currentStep;
    this.currentStep = step;

    // When on step 0, just layout the qubits per index 0
    // When on step 1, layout per index 0 then apply the gates/moves per index 0
    // When on step 2, layout per index 1 then apply the gates/moves per index 1
    // etc. until when on step n + 1, layout per index n and apply per index n
    const qubitLocationIndex = step === 0 ? 0 : step - 1;

    // Update all qubit locations
    const qubitLayout = this.getStepLayout(qubitLocationIndex);
    const qubitCount = qubitLayout.length / 2;
    for (let idx = 0; idx < qubitCount; ++idx) {
      const elem = this.qubits[idx][2];
      if (elem === undefined) {
        throw "Invalid qubit index in step";
      }
      const loc: Location = [qubitLayout[idx * 2], qubitLayout[idx * 2 + 1]];
      this.qubits[idx] = [loc[0], loc[1], elem]; // Update the location

      // Get the offset for the location and move it there
      const [x, y] = this.getQubitCenter(idx);
      elem.style.transform = `translate(${x}px, ${y}px)`;
    }

    // Now apply the ops
    if (step > 0) {
      const duration = forwards ? this.stepInterval / 2 : 0;
      const ops = this.getOpsAtStep(qubitLocationIndex);
      let trailId = 0;
      ops.forEach((op) => {
        const move = parseMove(op);
        if (move) {
          // Apply the move animation
          const [oldX, oldY] = this.getQubitCenter(move.qubit);
          const [newX, newY] = this.getLocationCenter(move.to[0], move.to[1]);
          const qubit = this.qubits[move.qubit][2];
          if (!qubit) throw "Invalid qubit index";
          if (this.showTracks) {
            if (this.showDottedPath) {
              // Render a hollow circle at the start position
              const [startCircle, trail] = createSvgElements("circle", "line");
              setAttributes(startCircle, {
                cx: `${oldX}`,
                cy: `${oldY}`,
                class: `qs-atoms-qubit-trail-start`,
                "data-qubitid": `${move.qubit}`,
              });

              const id = `dotted-trail-${trailId++}`;
              setAttributes(trail, {
                id,
                x1: `${oldX}`,
                y1: `${oldY}`,
                x2: `${newX}`,
                y2: `${newY}`,
                class: "qs-atoms-qubit-trail",
                "data-qubitid": `${move.qubit}`,
              });
              appendChildren(this.trackParent, [trail, startCircle]);
            } else {
              const id = `gradient-${trailId++}`;
              // Draw the path of any qubit movements
              const [gradient, trail] = createSvgElements(
                "linearGradient",
                "line",
              );
              setAttributes(gradient, {
                id,
                gradientUnits: "userSpaceOnUse",
                x1: `${oldX}`,
                y1: `${oldY}`,
                x2: `${newX}`,
                y2: `${newY}`,
              });
              gradient.innerHTML = `<stop offset="0%" stop-color="gray" stop-opacity="0.2"/><stop offset="100%" stop-color="gray" stop-opacity="0.8"/>`;
              setAttributes(trail, {
                x1: `${oldX}`,
                y1: `${oldY}`,
                x2: `${newX}`,
                y2: `${newY}`,
                style: `stroke-width: 2px; stroke: url(#${id})`,
              });
              appendChildren(this.trackParent, [gradient, trail]);
            }
          }
          qubit
            .animate(
              [
                { transform: `translate(${oldX}px, ${oldY}px)` },
                { transform: `translate(${newX}px, ${newY}px)` },
              ],
              {
                duration: this.showDottedPath ? 50 : duration,
                fill: "forwards",
                easing: "ease",
              },
            )
            .finished.then((anim) => {
              anim.commitStyles();
              anim.cancel();
            });
          // TODO: Check if you can/should cancel when scrubbing
        } else {
          // Wasn't a move, so render the gate
          const gate = parseGate(op);
          if (!gate) throw `Invalid gate: ${op}`;
          const arg = gate.arg ? gate.arg.substring(0, 4) : undefined;
          this.renderGateOnQubit(gate.qubit, gate.gate.toUpperCase(), arg);
        }
      });
    }
  }
}
