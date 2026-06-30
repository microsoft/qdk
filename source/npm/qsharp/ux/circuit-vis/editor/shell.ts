// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import type { Circuit } from "../data/circuit.js";
import { ensureStateVisualization } from "../state-viz/stateVizController.js";
import type { StateColumn } from "../state-viz/stateViz.js";
import type { PrepareStateVizOptions } from "../state-viz/worker/stateVizPrep.js";
import { createToolboxElement } from "./toolbox.js";

/**
 * Build the editor-mode DOM shell around the rendered circuit:
 *
 * - Wraps the circuit SVG in `.circuit-wrapper`.
 * - Prepends the toolbox panel (with optional Run button).
 * - Shows an "empty circuit" hint when the wires group is empty.
 * - Tags the container/wrapper with editor layout classes.
 * - Mounts the state-visualization panel (when the host supports it).
 *
 * Idempotent — safe to re-call on every render; pre-existing elements
 * are reused so the SVG element identity stays stable. Called only in
 * editor mode, once per `renderCircuit`.
 *
 * @param container         HTML element holding the rendered circuit.
 * @param computeStateVizColumnsForCircuitModel  Optional state-viz
 *   compute callback. When provided, enables the state-viz panel.
 * @param runCallback       Optional Run-button click handler. When
 *   omitted, no Run button is rendered.
 */
const mountEditorShell = (
  container: HTMLElement,
  computeStateVizColumnsForCircuitModel?: (
    model: Circuit,
    opts?: PrepareStateVizOptions,
  ) => Promise<StateColumn[]>,
  runCallback?: () => void,
): void => {
  const { wrapper, circuit } = getOrCreateCircuitWrapper(container);
  removeEmptyCircuitMessage(wrapper);
  attachToolboxPanelIfMissing(container, () =>
    createToolboxElement(runCallback),
  );
  addEmptyCircuitMessageIfEmpty(wrapper, circuit);
  applyCircuitEditorLayoutClasses(container, wrapper);

  ensureStateVisualization(container, computeStateVizColumnsForCircuitModel);
};

/* ----- private helpers ----- */

const getOrCreateCircuitWrapper = (container: HTMLElement) => {
  let wrapper: HTMLElement | null = container.querySelector(".circuit-wrapper");
  const circuit = container.querySelector("svg.qviz") as SVGElement | null;
  if (circuit == null) {
    throw new Error("No circuit found in the container");
  }
  if (!wrapper) {
    wrapper = document.createElement("div");
    wrapper.className = "circuit-wrapper";
    wrapper.appendChild(circuit);
    container.appendChild(wrapper);
  } else if (circuit.parentElement !== wrapper) {
    wrapper.appendChild(circuit);
  }

  return { wrapper, circuit };
};

const attachToolboxPanelIfMissing = (
  container: HTMLElement,
  createToolboxFn: () => HTMLElement,
): void => {
  if (container.querySelector(".panel") != null) return;
  // The toolbox sits inside a `.panel` div, prepended so it lives to
  // the left of the circuit wrapper in the editor's flex layout.
  const panelElem = document.createElement("div");
  panelElem.className = "panel";
  panelElem.appendChild(createToolboxFn());
  container.prepend(panelElem);
};

const removeEmptyCircuitMessage = (wrapper: HTMLElement): void => {
  const prevMsg = wrapper.querySelector(".empty-circuit-message");
  if (prevMsg) prevMsg.remove();
};

const addEmptyCircuitMessageIfEmpty = (
  wrapper: HTMLElement,
  circuit: SVGElement,
): void => {
  const wiresGroup = circuit?.querySelector(".wires");
  if (!wiresGroup || wiresGroup.children.length === 0) {
    const emptyMsg = document.createElement("div");
    emptyMsg.className = "empty-circuit-message";
    emptyMsg.textContent =
      "Your circuit is empty. Drag gates from the toolbox to get started!";
    wrapper.appendChild(emptyMsg);
  }
};

const applyCircuitEditorLayoutClasses = (
  container: HTMLElement,
  wrapper: HTMLElement,
): void => {
  container.classList.add("circuit-editor-container");
  wrapper.classList.add("circuit-wrapper");
};

export { mountEditorShell };
