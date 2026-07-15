// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import type { LayoutMap } from "../renderer/layoutMap.js";
import type { EditorHandlers, Sqore } from "../sqore.js";
import { createDropzones } from "./draggable.js";
import { enableEvents } from "./events.js";
import { mountEditorShell } from "./shell.js";

/**
 * Editor-mode bootstrap: takes a freshly-rendered circuit and installs every editor concern around
 * it in one call — editor-only DOM (toolbox, state-viz panel, dropzones, ghost-qubit row), pointer
 * + keyboard interaction (see [events.ts](events.ts)), and the host's edit notification.
 *
 * Sqore re-invokes this on every `renderCircuit`, so the helpers it calls are idempotent on re-call
 * (e.g. [mountEditorShell](shell.ts) reuses pre-existing DOM).
 *
 * @param container HTML element holding the rendered circuit.
 * @param sqore     Sqore instance — needed by `enableEvents` and to emit minimized-circuit data via
 *   `editCallback`.
 * @param layoutMap Geometry from the layout pass. See [layoutMap.ts](../renderer/layoutMap.ts).
 * @param editor    Editor handlers from the host (edit/run/state-viz callbacks).
 * @param refresh   Re-render closure — typically `() => sqore.renderCircuit(container)`, passed
 *   through to `enableEvents`.
 */
const installEditor = (
  container: HTMLElement,
  sqore: Sqore,
  layoutMap: LayoutMap,
  editor: EditorHandlers,
  refresh: () => void,
): void => {
  createDropzones(container, sqore, layoutMap);
  mountEditorShell(
    container,
    editor.computeStateVizColumnsForCircuitModel,
    editor.runCallback,
  );
  enableEvents(container, sqore, layoutMap, refresh);
  editor.editCallback(sqore.minimizeCircuits(sqore.circuitGroup));
};

export { installEditor };
