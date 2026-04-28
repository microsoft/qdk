// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// @ts-nocheck — vanilla JS webview client for the Katas Panel.
// Loaded by both the VS Code webview and the standalone dev harness.
// Expects `globalThis.__vscodeApi` to be set before this script runs.
"use strict";

(() => {
  const vscodeApi = globalThis.__vscodeApi;
  const R = globalThis.KatasRender;

  // ─── DOM slots ───
  const headerEl = document.getElementById("header");
  const crumbEl = headerEl.querySelector(".crumb");
  const badgeEl = headerEl.querySelector(".badge");
  const contentEl = document.getElementById("content");
  const outputEl = document.getElementById("output");
  const actionsEl = document.getElementById("actions");
  const progressEl = document.getElementById("progress-bar");

  let busy = false;
  let lastPositionKey = null;
  let currentActionBindings = [];

  // ─── Rendering ───

  function renderHeader(position) {
    const item = position.item;
    const itemTitle =
      item.type === "exercise" ? item.title : item.sectionTitle || "";
    crumbEl.textContent =
      position.kataTitle +
      " › " +
      position.sectionTitle +
      (itemTitle && itemTitle !== position.sectionTitle
        ? " › " + itemTitle
        : "");

    badgeEl.className = "badge";
    if (item.type === "exercise") {
      badgeEl.textContent = item.isComplete ? "✔ done" : "exercise";
      badgeEl.classList.add(item.isComplete ? "complete" : "exercise");
    } else if (item.type === "lesson-text") {
      badgeEl.textContent = "lesson";
    } else if (item.type === "lesson-example") {
      badgeEl.textContent = "example";
    } else if (item.type === "lesson-question") {
      badgeEl.textContent = "question";
    } else {
      badgeEl.textContent = item.type;
    }
  }

  function renderContent(position) {
    contentEl.innerHTML = R.renderContentBody(position.item);
    contentEl.scrollTop = 0;
  }

  function showOutput(html, variant) {
    outputEl.className = "output" + (variant ? " " + variant : "");
    outputEl.innerHTML =
      '<button class="out-dismiss" aria-label="Dismiss" title="Dismiss">×</button>' +
      '<div class="out-label">' +
      labelFor(variant) +
      "</div>" +
      '<div class="out-body">' +
      html +
      "</div>";
    outputEl.hidden = false;
    outputEl
      .querySelector(".out-dismiss")
      .addEventListener("click", clearOutput);
  }

  function labelFor(variant) {
    if (variant === "pass") {
      return "Result";
    }
    if (variant === "fail") {
      return "Result";
    }
    return "Output";
  }

  function clearOutput() {
    outputEl.hidden = true;
    outputEl.innerHTML = "";
  }

  function renderActions(groups) {
    actionsEl.innerHTML = "";
    currentActionBindings = [];
    for (const group of groups) {
      const div = document.createElement("div");
      div.className = "action-group";
      for (const binding of group) {
        if (binding.action === "quit" || binding.action === "menu") {
          continue;
        }
        const btn = document.createElement("button");
        if (binding.codicon) {
          const icon = document.createElement("span");
          icon.className = "codicon codicon-" + binding.codicon;
          btn.appendChild(icon);
          btn.appendChild(document.createTextNode(" " + binding.label));
        } else {
          btn.textContent = binding.label;
        }
        if (binding.primary) {
          btn.classList.add("primary");
        }
        // Tooltip: show label + shortcut key + chat note for AI actions
        var tip = binding.label;
        if (binding.key && binding.key !== "space") {
          tip += " (" + binding.key.toUpperCase() + ")";
        } else if (binding.key === "space") {
          tip += " (Space)";
        }
        if (binding.codicon === "sparkle") {
          tip += " — opens Copilot Chat";
        }
        btn.title = tip;
        btn.dataset.action = binding.action;
        btn.disabled = busy;
        btn.addEventListener("click", () => executeAction(binding.action));
        div.appendChild(btn);
        currentActionBindings.push(binding);
      }
      if (div.children.length > 0) {
        actionsEl.appendChild(div);
      }
    }
  }

  function setBusy(b) {
    busy = b;
    for (const btn of actionsEl.querySelectorAll("button")) {
      btn.disabled = b;
    }
  }

  function renderProgressBar(progress) {
    try {
      progressEl.innerHTML = R.renderProgressBar(progress);
    } catch {
      progressEl.innerHTML = "";
    }
  }

  function applyState(state) {
    if (!state) {
      return;
    }
    const pos = state.position;
    const key = pos.kataId + ":" + pos.sectionId + ":" + pos.itemIndex;
    if (key !== lastPositionKey) {
      renderHeader(pos);
      renderContent(pos);
      clearOutput();
      lastPositionKey = key;
    } else {
      renderHeader(pos);
    }
    renderActions(state.actions);
    renderProgressBar(state.progress);
    vscodeApi.setState(state);
  }

  function invalidateContent() {
    lastPositionKey = null;
  }

  // ─── Action dispatch ───

  function executeAction(action) {
    if (busy) {
      return;
    }

    if (action === "hint-chat") {
      vscodeApi.postMessage({ command: "openChat", text: "Give me a hint" });
      return;
    }

    if (action === "explain-chat") {
      vscodeApi.postMessage({
        command: "openChat",
        text: "Explain this concept in more detail",
      });
      return;
    }

    if (action === "discuss-chat") {
      vscodeApi.postMessage({
        command: "openChat",
        text: "Help me think through this question without revealing the answer",
      });
      return;
    }

    setBusy(true);

    var slow = ["run", "circuit", "check"].indexOf(action) >= 0;
    if (slow) {
      showOutput('<div class="loading">Working…</div>');
    }

    vscodeApi.postMessage({ command: "action", action: action });
  }

  // ─── Messages from extension host ───

  window.addEventListener("message", function (event) {
    var msg = event.data;
    switch (msg.command) {
      case "state":
        applyState(msg.state);
        setBusy(false);
        break;
      case "result": {
        var action = msg.action;
        var result = msg.result;
        switch (action) {
          case "next":
          case "back":
            if (result && !result.moved) {
              showOutput(
                action === "next"
                  ? '<div class="success">🎉 You have completed all content!</div>'
                  : '<div class="message">Already at the beginning.</div>',
              );
            }
            break;
          case "check":
            if (result.passed) {
              invalidateContent();
            }
            break;

          case "reveal-answer":
            showOutput(
              "<div>" +
                result +
                '</div><a class="chat-link" data-chat="Explain why this is the answer"><span class="codicon codicon-sparkle"></span> Explain this answer</a>',
            );
            break;
          case "solution":
            showOutput(
              '<div style="margin-bottom:0.3rem"><strong>Reference Solution</strong></div><pre>' +
                R.escapeHtml(result) +
                '</pre><a class="chat-link" data-chat="Explain this solution step by step"><span class="codicon codicon-sparkle"></span> Explain this solution</a>',
            );
            break;
          case "run":
          case "circuit":
            clearOutput();
            break;
        }
        if (msg.state) {
          applyState(msg.state);
        }
        // Show check result *after* applyState, so invalidateContent()+clearOutput() doesn't wipe it.
        if (action === "check") {
          showOutput(
            R.renderSolutionCheck(result),
            result.passed ? "pass" : "fail",
          );
        }
        setBusy(false);
        break;
      }
      case "error":
        showOutput(
          '<div class="fail">Error: ' + R.escapeHtml(msg.message) + "</div>",
          "fail",
        );
        setBusy(false);
        break;
    }
  });

  // Chat-link clicks (data-chat) → open chat via extension host.
  document.addEventListener("click", function (e) {
    var link = e.target.closest("[data-chat]");
    if (!link) {
      return;
    }
    e.preventDefault();
    vscodeApi.postMessage({ command: "openChat", text: link.dataset.chat });
  });

  // File-path link clicks → open via extension host.
  contentEl.addEventListener("click", function (e) {
    var a = e.target.closest("a.file-path-link");
    if (!a) {
      return;
    }
    e.preventDefault();
    var url = a.getAttribute("href");
    if (url) {
      vscodeApi.postMessage({ command: "openFile", uri: url });
    }
  });

  // Keyboard shortcuts — dispatch based on the key bindings in the action bar.
  document.addEventListener("keydown", function (e) {
    if (busy) {
      return;
    }
    // Ignore when focus is inside an input/textarea.
    var tag = (e.target.tagName || "").toLowerCase();
    if (tag === "input" || tag === "textarea" || tag === "select") {
      return;
    }

    var key = e.key.toLowerCase();
    if (key === " ") {
      key = "space";
    }
    var btn = actionsEl.querySelector("button[data-action]:not(:disabled)");
    // Find matching binding by scanning all rendered buttons.
    var buttons = actionsEl.querySelectorAll("button[data-action]");
    for (var i = 0; i < buttons.length; i++) {
      // Match by looking up the key from current action groups.
      // For Space, trigger the primary action.
      if (key === "space" && buttons[i].classList.contains("primary")) {
        e.preventDefault();
        executeAction(buttons[i].dataset.action);
        return;
      }
    }
    // Match single-letter shortcuts against rendered bindings.
    if (key.length === 1 && currentActionBindings) {
      for (var j = 0; j < currentActionBindings.length; j++) {
        var b = currentActionBindings[j];
        if (b.key === key) {
          e.preventDefault();
          executeAction(b.action);
          return;
        }
      }
    }
  });

  // Click the progress bar footer → focus sidebar tree view
  progressEl.addEventListener("click", () => {
    vscodeApi.postMessage({ command: "focusProgress" });
  });

  // Restore cached state immediately for instant render on restart
  const cachedState = vscodeApi.getState();
  if (cachedState) {
    applyState(cachedState);
  }

  // Signal ready
  vscodeApi.postMessage({ command: "ready" });
})();
