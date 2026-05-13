// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import type { LayoutMap } from "../renderer/layoutMap.js";
import type { EditorHandlers, Sqore } from "../sqore.js";
import { createDropzones } from "./draggable.js";
import { enableEvents } from "./events.js";
import { mountEditorShell } from "./shell.js";

/**
 * Editor-mode bootstrap: takes a freshly-rendered circuit and
 * installs every editor concern around it in one call.
 *
 * After this returns:
 *
 * - Editor-only DOM (toolbox panel, state-viz panel, dropzones,
 *   ghost-qubit row) lives inside `container`.
 * - Pointer + keyboard interaction is wired up via the controllers
 *   in [events.ts](events.ts).
 * - The host has received a notification through `editor.editCallback`
 *   with the minimized circuit data.
 *
 * Sqore re-invokes this on every `renderCircuit` so the editor
 * chrome stays in sync with the underlying SVG; downstream code is
 * responsible for tolerating that. Most of it does so by being
 * idempotent on re-call (e.g. [mountEditorShell](shell.ts) reuses
 * pre-existing DOM rather than recreating it).
 *
 * Keeping the orchestration here means [sqore.ts](../sqore.ts) only
 * needs to know "there's an editor to install" — the editor's
 * internal sub-architecture (dropzones, shell, events, edit
 * notification) doesn't leak into the View entrypoint.
 *
 * @param container HTML element holding the rendered circuit.
 * @param sqore     Sqore instance — needed by `enableEvents` and to
 *                  emit minimized-circuit data via `editCallback`.
 * @param layoutMap Geometry from the layout pass — used by both the
 *                  dropzone layer and the editor's interaction
 *                  controllers. See [layoutMap.ts](../renderer/layoutMap.ts).
 * @param editor    Editor handlers from the host (edit/run/state-viz
 *                  callbacks).
 * @param refresh   Re-render closure — typically
 *                  `() => sqore.renderCircuit(container)`. Passed
 *                  through to `enableEvents` so editor mutations
 *                  trigger a re-render without coupling the editor
 *                  back to Sqore's private API.
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
