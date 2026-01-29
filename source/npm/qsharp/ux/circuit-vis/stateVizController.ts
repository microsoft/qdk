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
//             │    └─ stateCompute.ts
//             │         ├─ getCurrentCircuitModel() from events.ts
//             │         ├─ if host API exists:
//             │         │     globalThis.qsharpStateComputeApi.*
//             │         │        (VS Code webview)
//             │         │         └─ editor.tsx → stateComputeWorker.ts
//             │         │              ├─ stateComputeCore.ts (compute)
//             │         │              └─ stateVizPrep.ts (prep)
//             │         └─ else fallback:
//             │               ├─ stateComputeCore.ts (compute ampMap)
//             │               └─ stateVizPrep.ts (prep columns)
//             └─ updateStatePanelFromColumns(...)  (render)
//                  stateViz.ts

import {
  createStatePanel,
  updateStatePanelFromColumns,
  renderDefaultStatePanel,
  renderUnsupportedStatePanel,
  setStatePanelLoading,
} from "./stateViz.js";
import { computeStateVizColumnsFromCurrentModelAsync } from "./stateCompute.js";
import { prepareStateVizColumnsFromAmpMap } from "./stateVizPrep.js";
import {
  attachStateDevToolbar,
  createDefaultDevToolbarState,
  type DevToolbarState,
  getStaticMockAmpMap,
} from "./devToolbar.js";

type StateVizController = {
  state: DevToolbarState;
  requestRenderState: () => void;
};

export function ensureStateVisualization(
  container: HTMLElement,
  showDevToolbar: boolean = false,
  statePanelInitiallyExpanded: boolean = false,
): void {
  // Ensure a right-side state panel exists.
  if (container.querySelector(".state-panel") == null) {
    const statePanel = createStatePanel(statePanelInitiallyExpanded === true);
    container.appendChild(statePanel);
  }

  const panelElem = container.querySelector(
    ".state-panel",
  ) as HTMLElement | null;
  if (!panelElem) return;

  // Sqore calls createPanel() on every edit (it re-renders the SVG). Keep a
  // single state-viz controller per panel element so in-flight renders from
  // previous calls can't toggle loading off underneath new renders.
  (panelElem as any)._stateVizContainer = container;

  const existingController = (panelElem as any)._stateVizController as
    | StateVizController
    | undefined;

  if (existingController) {
    if (showDevToolbar) {
      attachStateDevToolbar(panelElem, existingController.state, () => {
        existingController.requestRenderState();
      });
    }
    existingController.requestRenderState();
    return;
  }

  const state: DevToolbarState = createDefaultDevToolbarState();

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

  const renderState = async (panel: HTMLElement) => {
    const requestId = ++renderRequestId;

    if (state.dataMode === "mock") {
      activeLoadingRequestId = 0;
      clearLoadingTimer();
      clearHideLoadingTimer();
      setStatePanelLoading(panelElem, false);
      const ampMap = getStaticMockAmpMap(state.mockSet);
      const columns = prepareStateVizColumnsFromAmpMap(ampMap as any, {
        normalize: false,
        minProbThreshold: state.minProbThreshold,
      });
      updateStatePanelFromColumns(panel, columns, {
        normalize: false,
        minProbThreshold: state.minProbThreshold,
      });
      return true;
    }

    try {
      beginLoadingForRequest(requestId);
      const columns = await computeStateVizColumnsFromCurrentModelAsync(
        state.endianness,
        {
          normalize: true,
          minProbThreshold: state.minProbThreshold,
        },
      );

      // Ignore late results if a newer render request started.
      if (requestId !== renderRequestId) return true;

      if (columns && columns.length > 0) {
        updateStatePanelFromColumns(panel, columns, {
          normalize: true,
          minProbThreshold: state.minProbThreshold,
        });
        return true;
      }

      // Determine current wire count from the circuit DOM.
      const hostContainer =
        ((panelElem as any)._stateVizContainer as HTMLElement | undefined) ??
        container;
      const circuit = hostContainer.querySelector("svg.qviz");
      const wiresGroup = circuit?.querySelector(".wires");
      const wireCount = wiresGroup ? wiresGroup.children.length : 0;
      renderDefaultStatePanel(panel, wireCount);
      return false;
    } catch (e) {
      // Ignore cancellation from host worker termination.
      if (requestId !== renderRequestId) return true;

      const err = e as Error;
      if (err?.name === "AbortError") {
        return true;
      }
      if (err?.name === "UnsupportedStateComputeError") {
        renderUnsupportedStatePanel(panel, err.message);
        return true;
      }
      renderUnsupportedStatePanel(
        panel,
        "State visualization is unavailable for this circuit.",
      );
      return true;
    } finally {
      endLoadingForRequest(requestId);
    }
  };

  const requestRenderState = () => {
    void renderState(panelElem);
  };

  (panelElem as any)._stateVizController = {
    state,
    requestRenderState,
  } satisfies StateVizController;

  if (showDevToolbar) {
    attachStateDevToolbar(panelElem, state, () => {
      requestRenderState();
    });
  }

  // Initial render; if the circuit model isn't ready yet, retry briefly until available.
  void renderState(panelElem).then((gotReal) => {
    if (gotReal) return;
    let attempts = 20; // try for ~2 seconds total
    const retry = () => {
      void renderState(panelElem).then((ok) => {
        if (ok) return;
        if (--attempts > 0) setTimeout(retry, 100);
      });
    };
    setTimeout(retry, 100);
  });
}
