// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// State visualization controller for the circuit side panel.
// Responsible for: ensuring the state panel exists, coordinating async state
// computation + rendering, suppressing stale renders, and managing the loading
// spinner/dev toolbar wiring.

// Here is a general overview of the flow for the state visualization:
// panel.ts
//   └─ ensureStateVisualization(...)
//        └─ stateVizController.ts  (loading spinner, request-id cancellation, retries)
//             ├─ computeStateVizColumnsFromCurrentModelAsync(...)
//             │    ├─ getCurrentCircuitModel(...) from events.ts
//             │    ├─ if compute callback is provided (via DrawOptions.editor):
//             │    │     computeStateVizColumnsForCircuitModel(...)
//             │    │        (e.g., VS Code webview uses a Web Worker)
//             │    │         └─ editor.tsx → stateComputeWorker.ts
//             │    │              ├─ uses state-viz/worker/stateCompute.ts (compute ampMap)
//             │    │              └─ uses state-viz/worker/stateVizPrep.ts (prep columns)
//             │    └─ else fallback (main thread):
//             │          ├─ state-viz/worker/stateCompute.ts (compute ampMap)
//             │          └─ state-viz/worker/stateVizPrep.ts (prep columns)
//             └─ updateStatePanelFromColumns(...)  (render)
//                  stateViz.ts

import {
  createStatePanel,
  updateStatePanelFromColumns,
  renderBlankStatePanel,
  renderMessageStatePanel,
  setStatePanelLoading,
} from "./stateViz.js";

import { getCurrentCircuitModel } from "../events.js";
import type { Circuit } from "../circuit.js";
import {
  computeAmpMapForCircuit,
  UnsupportedStateComputeError,
} from "./worker/stateCompute.js";
import {
  prepareStateVizColumnsFromAmpMap,
  type PrepareStateVizOptions,
} from "./worker/stateVizPrep.js";
import type { StateColumn } from "./stateViz.js";

const DEFAULT_MIN_PROB_THRESHOLD = 0.0;
const MAX_QUBITS_FOR_STATE_VIZ = 20;

type StateVizController = {
  requestRenderState: () => void;
  setComputeCallback: (cb?: ComputeStateVizColumnsForCircuitModel) => void;
  setHostContainer: (container: HTMLElement) => void;
};

type ComputeStateVizColumnsForCircuitModel = (
  model: Circuit,
  opts?: PrepareStateVizOptions,
) => Promise<StateColumn[]>;

export function ensureStateVisualization(
  container: HTMLElement,
  computeStateVizColumnsForCircuitModel?: ComputeStateVizColumnsForCircuitModel,
): void {
  // Ensure a right-side state panel exists.
  if (container.querySelector(".state-panel") == null) {
    const statePanel = createStatePanel();
    container.appendChild(statePanel);
  }

  const panelElem = container.querySelector(
    ".state-panel",
  ) as HTMLElement | null;
  if (!panelElem) return;

  const existingController = (panelElem as any)._stateVizController as
    | StateVizController
    | undefined;

  if (existingController) {
    // Sqore calls createPanel() on every edit (it re-renders the SVG). Keep a
    // single state-viz controller per panel element so in-flight renders from
    // previous calls can't toggle loading off underneath new renders.
    existingController.setHostContainer(container);
    existingController.setComputeCallback(
      computeStateVizColumnsForCircuitModel,
    );
    existingController.requestRenderState();
    return;
  }

  // Captured, mutable inputs that can be updated by future calls to
  // ensureStateVisualization(...).
  let hostContainer: HTMLElement = container;
  let computeCallback: ComputeStateVizColumnsForCircuitModel | undefined =
    computeStateVizColumnsForCircuitModel;

  let renderRequestId = 0;
  let loadingTimer: number | null = null;
  let hideLoadingTimer: number | null = null;
  let activeLoadingRequestId = 0;
  let loadingShownAtMs = 0;

  const clearLoadingTimer = () => {
    if (loadingTimer != null) {
      clearTimeout(loadingTimer);
      loadingTimer = null;
    }
  };

  const clearHideLoadingTimer = () => {
    if (hideLoadingTimer != null) {
      clearTimeout(hideLoadingTimer);
      hideLoadingTimer = null;
    }
  };

  const beginLoadingForRequest = (requestId: number) => {
    // Always point loading at the newest request.
    activeLoadingRequestId = requestId;
    clearLoadingTimer();
    clearHideLoadingTimer();

    // If we're already showing loading (e.g., rapid edits), keep it on.
    if (panelElem.classList.contains("loading")) return;

    // Avoid flicker for fast computations by delaying the spinner.
    loadingTimer = setTimeout(() => {
      if (activeLoadingRequestId !== requestId) return;
      loadingShownAtMs = performance.now();
      setStatePanelLoading(panelElem, true);
    }, 200) as unknown as number;
  };

  const endLoadingForRequest = (requestId: number) => {
    if (activeLoadingRequestId !== requestId) return;
    clearLoadingTimer();

    // If loading was never shown (fast compute), there's nothing to hide.
    if (!panelElem.classList.contains("loading")) {
      activeLoadingRequestId = 0;
      return;
    }

    // Avoid flicker: once visible, keep loading on briefly, and debounce the
    // hide so rapid successive edits don't flash the spinner.
    const minVisibleMs = 250;
    const hideDebounceMs = 150;
    const elapsed = performance.now() - (loadingShownAtMs || 0);
    const remainingVisible = Math.max(0, minVisibleMs - elapsed);

    clearHideLoadingTimer();
    hideLoadingTimer = setTimeout(() => {
      if (activeLoadingRequestId !== requestId) return;
      activeLoadingRequestId = 0;
      setStatePanelLoading(panelElem, false);
    }, remainingVisible + hideDebounceMs) as unknown as number;
  };

  const renderState = async (panel: HTMLElement): Promise<void> => {
    const requestId = ++renderRequestId;

    try {
      beginLoadingForRequest(requestId);

      // If we were previously showing a message (e.g., unsupported/too many
      // qubits), clear it immediately so the loading overlay can be shown while
      // the new request is computing.
      if (panel.classList.contains("message")) {
        renderBlankStatePanel(panel);
      }

      // Determine current wire count and SVG for this render from the DOM.
      const circuitSvg = hostContainer.querySelector(
        "svg.qviz",
      ) as SVGElement | null;

      const columns = await computeStateVizColumnsFromCurrentModelAsync(
        {
          minProbThreshold: DEFAULT_MIN_PROB_THRESHOLD,
        },
        circuitSvg,
        computeCallback,
      );

      // Ignore late results if a newer render request started.
      if (requestId !== renderRequestId) return;

      if (columns == null) {
        // Model isn't ready for this render yet (events not enabled), or the
        // model corresponds to a different SVG (during a re-render). Keep the
        // panel blank for non-empty circuits so the loading overlay can show;
        // show a message state only when the circuit is truly empty.
        const wiresGroup = circuitSvg?.querySelector(".wires");
        const wireCount = wiresGroup ? wiresGroup.children.length : 0;
        if (wireCount <= 0) {
          renderMessageStatePanel(panel);
        } else {
          renderBlankStatePanel(panel);
        }
        return;
      }

      if (columns.length > 0) {
        updateStatePanelFromColumns(panel, columns);
      } else {
        // Empty model: show a message state.
        renderMessageStatePanel(panel);
      }

      return;
    } catch (e) {
      // Ignore cancellation from host worker termination.
      if (requestId !== renderRequestId) return;

      const err = e as Error;
      if (err?.name === "AbortError") {
        return;
      }
      if (err?.name === "UnsupportedStateComputeError") {
        renderMessageStatePanel(panel, err.message);
        return;
      }
      renderMessageStatePanel(
        panel,
        "State visualization is unavailable for this circuit.",
      );
      return;
    } finally {
      endLoadingForRequest(requestId);
    }
  };

  const requestRenderState = () => {
    void renderState(panelElem);
  };

  (panelElem as any)._stateVizController = {
    requestRenderState,
    setComputeCallback: (cb?: ComputeStateVizColumnsForCircuitModel) => {
      computeCallback = cb;
    },
    setHostContainer: (c: HTMLElement) => {
      hostContainer = c;
    },
  } satisfies StateVizController;

  // Re-render when the circuit model becomes available. The circuit SVG is
  // replaced before `enableEvents(...)` runs, so computing state immediately in
  // `createPanel(...)` would otherwise risk using a stale model.
  try {
    container.addEventListener("qsharp:circuit:modelReady", () => {
      requestRenderState();
    });
  } catch {
    // ignore
  }

  // Initial render.
  void renderState(panelElem);
}

async function computeStateVizColumnsFromCurrentModelAsync(
  opts: PrepareStateVizOptions = {},
  expectedCircuitSvg?: SVGElement | null,
  computeStateVizColumnsForCircuitModel?: ComputeStateVizColumnsForCircuitModel,
): Promise<StateColumn[] | null> {
  const model = getCurrentCircuitModel(expectedCircuitSvg);
  if (!model) return null;
  if (model.qubits.length === 0) return [];

  if (model.qubits.length > MAX_QUBITS_FOR_STATE_VIZ) {
    throw new UnsupportedStateComputeError(
      `Too many qubits for state visualization (limit: ${MAX_QUBITS_FOR_STATE_VIZ}). This circuit has ${model.qubits.length} qubits.`,
    );
  }

  if (computeStateVizColumnsForCircuitModel) {
    return await computeStateVizColumnsForCircuitModel(model, opts);
  }

  // Fallback: compute and prepare on the main thread.
  const ampMap = computeAmpMapForCircuit(model.qubits, model.componentGrid);
  return prepareStateVizColumnsFromAmpMap(ampMap as any, opts);
}
