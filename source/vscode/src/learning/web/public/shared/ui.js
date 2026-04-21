// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// @ts-nocheck — vanilla JS shared between the web app and the MCP widget.
// DOM-interacting UI helpers. Requires globalThis.KatasRender.
"use strict";

(function () {
  const R = globalThis.KatasRender;

  /**
   * Create a UI controller bound to three host elements.
   *
   * @param {object} opts
   * @param {HTMLElement} opts.streamEl  Scrolling content/output/action log.
   * @param {HTMLElement} opts.actionsEl Action-bar container.
   * @param {HTMLElement} opts.progressEl Progress-bar footer.
   * @param {(action: string) => void} opts.onAction Called when any action
   *   button or keyboard shortcut fires. Transport-specific.
   */
  function createUiController({ streamEl, actionsEl, progressEl, onAction }) {
    const keyMap = new Map();        // key → action
    const actionLabels = new Map();  // action → label
    let lastPositionKey = null;      // dedup content entries

    function scrollToBottom() {
      streamEl.scrollTop = streamEl.scrollHeight;
    }

    function appendEntry(cls, bodyHtml, labelText) {
      const entry = document.createElement("div");
      entry.className = `stream-entry entry-${cls}`;
      if (labelText) {
        const lbl = document.createElement("div");
        lbl.className = "entry-label";
        lbl.textContent = labelText;
        entry.appendChild(lbl);
      }
      const body = document.createElement("div");
      body.className = "entry-body";
      body.innerHTML = bodyHtml;
      entry.appendChild(body);
      streamEl.appendChild(entry);
      R.renderMath(entry);
      scrollToBottom();
      return entry;
    }

    function appendContent(position) {
      const body = R.renderContentBody(position.item);
      const label = R.renderContentLabel(position);
      appendEntry("content", body, label);
    }

    function appendOutput(html, variant) {
      return appendEntry("output" + (variant ? " " + variant : ""), html);
    }

    function appendAction(action) {
      const label = actionLabels.get(action) || action;
      appendEntry("action", `<span>${R.escapeHtml(label)}</span>`);
    }

    function removeLastEntry() {
      const last = streamEl.lastElementChild;
      if (last) last.remove();
    }

    function renderActions(groups) {
      keyMap.clear();
      actionLabels.clear();
      actionsEl.innerHTML = "";

      for (const group of groups) {
        const div = document.createElement("div");
        div.className = "action-group";

        for (const binding of group) {
          actionLabels.set(binding.action, binding.label);
          if (binding.action === "quit") continue;

          const btn = document.createElement("button");
          const kbd = document.createElement("kbd");
          kbd.textContent = binding.key === "space" ? "␣" : binding.key;
          btn.appendChild(kbd);
          btn.appendChild(document.createTextNode(binding.label));
          if (binding.primary) btn.classList.add("primary");
          btn.dataset.action = binding.action;
          btn.addEventListener("click", () => onAction(binding.action));
          div.appendChild(btn);

          const rawKey = binding.key === "space" ? " " : binding.key;
          keyMap.set(rawKey, binding.action);
        }

        if (div.children.length > 0) actionsEl.appendChild(div);
      }
    }

    function renderProgressBar(progress) {
      try {
        progressEl.innerHTML = R.renderProgressBar(progress);
      } catch {
        // ignore
      }
    }

    /**
     * Apply a bundled ServerState: append content on position change,
     * then refresh actions and progress bar.
     */
    function applyState(state) {
      if (!state) return;
      const pos = state.position;
      const key = `${pos.kataId}:${pos.sectionIndex}:${pos.itemIndex}`;
      if (key !== lastPositionKey) {
        appendContent(pos);
        lastPositionKey = key;
      }
      renderActions(state.actions);
      renderProgressBar(state.progress);
    }

    /** Force the next applyState call to re-append the content entry. */
    function invalidateContent() {
      lastPositionKey = null;
    }

    function setBusy(isBusy) {
      for (const btn of actionsEl.querySelectorAll("button")) {
        btn.disabled = isBusy;
      }
    }

    /** Wire keyboard shortcuts to onAction. */
    function bindKeyboard() {
      document.addEventListener("keydown", (e) => {
        if (e.target.tagName === "INPUT" || e.target.tagName === "TEXTAREA") return;
        if (e.ctrlKey || e.metaKey || e.altKey) return;
        const key = e.key === " " ? " " : e.key.toLowerCase();
        const action = keyMap.get(key);
        if (action) {
          e.preventDefault();
          onAction(action);
        }
      });
    }

    /** Wire copy-to-clipboard buttons inside the stream. */
    function bindCopyButtons() {
      streamEl.addEventListener("click", (e) => {
        const btn = e.target.closest(".copy-btn");
        if (btn && navigator.clipboard) {
          navigator.clipboard.writeText(btn.dataset.copy);
        }
      });
    }

    return {
      appendContent,
      appendOutput,
      appendAction,
      appendEntry,
      removeLastEntry,
      renderActions,
      renderProgressBar,
      applyState,
      invalidateContent,
      setBusy,
      bindKeyboard,
      bindCopyButtons,
    };
  }

  globalThis.KatasUi = { createUiController };
})();
