// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { getGateLocationString } from "../utils.js";
import { InteractionContext } from "./interactionContext.js";

/**
 * `KeyboardController` — translates document-level keyboard events
 * into editor-state changes. Today the only behavior is the
 * Ctrl-toggle that swaps the `moving` / `copying` CSS classes on
 * the container while a gate is selected, so the cursor and ghost
 * preview reflect the current drop semantics.
 *
 * Owns its document `keydown` / `keyup` listeners; `dispose()`
 * removes them. No state of its own — the only mutable signal it
 * consults is whether `interaction.selectedOperation` has a
 * location string.
 */
export class KeyboardController {
  constructor(private readonly ctx: InteractionContext) {
    document.addEventListener("keydown", this.onKeyDown);
    document.addEventListener("keyup", this.onKeyUp);
  }

  dispose(): void {
    document.removeEventListener("keydown", this.onKeyDown);
    document.removeEventListener("keyup", this.onKeyUp);
  }

  /**
   * Ctrl-down while a placed (non-toolbox) gate is selected switches
   * the cursor/ghost into "copy" mode. Picks up the location off the
   * selected op rather than tracking selection separately.
   */
  readonly onKeyDown = (ev: KeyboardEvent) => {
    if (!ev.ctrlKey) return;
    if (!this.hasSelectedLocation()) return;
    this.ctx.container.classList.remove("moving");
    this.ctx.container.classList.add("copying");
  };

  /** Ctrl-up flips back to "move" mode. */
  readonly onKeyUp = (ev: KeyboardEvent) => {
    if (!ev.ctrlKey) return;
    if (!this.hasSelectedLocation()) return;
    this.ctx.container.classList.remove("copying");
    this.ctx.container.classList.add("moving");
  };

  private hasSelectedLocation(): boolean {
    const op = this.ctx.interaction.selectedOperation;
    if (op == null) return false;
    return getGateLocationString(op) != null;
  }
}
