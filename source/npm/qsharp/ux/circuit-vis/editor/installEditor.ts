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
 * **Why this exists.** Before this entrypoint, [sqore.ts](../sqore.ts)
 * carried the full `if (isEditable)` orchestration: dropzones, then
 * panel, then run button, then events, then editCallback \u2014 five
 * imports and five lines of orchestration leaking the editor's
 * sub-architecture into the View entrypoint. With `installEditor`,
 * Sqore only needs to know "there's an editor to install"; the
 * editor's internal seams stay on the editor's side of the fence.
 *
 * @param container HTML element holding the rendered circuit.
 * @param sqore     Sqore instance \u2014 needed by `enableEvents` and to
 *                  emit minimized-circuit data via `editCallback`.
 * @param layoutMap Geometry from the layout pass \u2014 used by both the
 *                  dropzone layer and the editor's interaction
 *                  controllers.
 * @param editor    Editor handlers from the host (edit/run/state-viz
 *                  callbacks).
 * @param refresh   Re-render closure \u2014 typically
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
