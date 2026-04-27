// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// @ts-nocheck — vanilla JS, no TypeScript
"use strict";

// ─── API helpers ───

async function api(method, path, body) {
  const opts = { method, headers: {} };
  if (body !== undefined) {
    opts.headers["Content-Type"] = "application/json";
    opts.body = JSON.stringify(body);
  }
  const res = await fetch(path, opts);
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(err.error || res.statusText);
  }
  return res.json();
}

const apiGet = (path) => api("GET", path);
const apiPost = (path, body) => api("POST", path, body);

// ─── Shared UI controller ───

const R = globalThis.KatasRender;
const ui = globalThis.KatasUi.createUiController({
  streamEl: document.getElementById("stream"),
  actionsEl: document.getElementById("actions"),
  progressEl: document.getElementById("progress-bar"),
  onAction: (action) => executeAction(action),
});

let busy = false; // prevent double-clicks during async ops

// ─── Action execution ───

async function executeAction(action) {
  if (busy) return;
  busy = true;
  ui.setBusy(true);

  ui.appendAction(action);

  try {
    switch (action) {
      case "next":
      case "back": {
        const endpoint = action === "next" ? "/api/next" : "/api/previous";
        const { moved, state } = await apiPost(endpoint);
        if (!moved) {
          if (action === "next") {
            ui.appendOutput(
              `<div class="success">🎉 You've completed all content!</div>`,
            );
            ui.appendOutput(R.renderProgress(state.progress));
          } else {
            ui.appendOutput(
              `<div class="message">Already at the beginning.</div>`,
            );
          }
        }
        ui.applyState(state);
        break;
      }

      case "run": {
        ui.appendOutput(`<div class="loading">Running...</div>`);
        const { result, state } = await apiPost("/api/run");
        ui.removeLastEntry();
        ui.appendOutput(R.renderRunResult(result));
        ui.applyState(state);
        break;
      }

      case "run-noise": {
        const shots = prompt("Number of shots:", "100");
        if (shots === null) break;
        const n = parseInt(shots, 10) || 100;
        ui.appendOutput(
          `<div class="loading">Running with noise (${n} shots)...</div>`,
        );
        const { result, state } = await apiPost("/api/run-noise", { shots: n });
        ui.removeLastEntry();
        ui.appendOutput(R.renderRunResult(result));
        ui.applyState(state);
        break;
      }

      case "circuit": {
        ui.appendOutput(`<div class="loading">Generating circuit...</div>`);
        const { result, state } = await apiPost("/api/circuit");
        ui.removeLastEntry();
        ui.appendOutput(R.renderCircuit(result));
        ui.applyState(state);
        break;
      }

      case "estimate": {
        ui.appendOutput(`<div class="loading">Estimating resources...</div>`);
        const { result, state } = await apiPost("/api/estimate");
        ui.removeLastEntry();
        ui.appendOutput(R.renderEstimate(result));
        ui.applyState(state);
        break;
      }

      case "check": {
        ui.appendOutput(`<div class="loading">Checking solution...</div>`);
        const { result, state } = await apiPost("/api/check");
        ui.removeLastEntry();
        ui.appendOutput(
          R.renderSolutionCheck(result),
          result.passed ? "result-pass" : "result-fail",
        );
        if (result.passed) ui.invalidateContent();
        ui.applyState(state);
        break;
      }

      case "hint": {
        const { result: hint, state } = await apiPost("/api/hint");
        ui.appendOutput(R.renderHint(hint));
        ui.applyState(state);
        break;
      }

      case "reveal-answer": {
        const { result: answer, state } = await apiPost("/api/reveal-answer");
        ui.appendOutput(`<div>${answer}</div>`);
        ui.applyState(state);
        break;
      }

      case "solution": {
        const { code } = await apiGet("/api/solution");
        ui.appendOutput(
          `<div style="margin-bottom:0.5rem"><strong>Reference Solution</strong></div><pre>${R.escapeHtml(code)}</pre>`,
        );
        break;
      }

      case "ask-ai": {
        const question = prompt("Ask a question:");
        if (!question || !question.trim()) break;
        ui.appendOutput(`<div class="loading">Asking AI...</div>`);
        const { result: answer, state } = await apiPost("/api/ai/ask", {
          question,
        });
        ui.removeLastEntry();
        ui.appendOutput(
          answer
            ? `<div>🤖 ${R.escapeHtml(answer)}</div>`
            : `<div class="message">AI not available.</div>`,
        );
        ui.applyState(state);
        break;
      }

      case "ai-hint": {
        ui.appendOutput(`<div class="loading">Getting AI hint...</div>`);
        const { result: hint, state } = await apiPost("/api/ai/hint");
        ui.removeLastEntry();
        ui.appendOutput(
          hint
            ? `<div>🤖 ${R.escapeHtml(hint)}</div>`
            : `<div class="message">AI hints not available.</div>`,
        );
        ui.applyState(state);
        break;
      }

      case "progress": {
        const state = await apiGet("/api/state");
        ui.appendOutput(R.renderProgress(state.progress));
        ui.applyState(state);
        break;
      }

      case "menu": {
        const katas = await apiGet("/api/katas");
        const names = katas
          .map(
            (k, i) =>
              `${i + 1}. ${k.title} (${k.completedCount}/${k.sectionCount})`,
          )
          .join("\n");
        const choice = prompt(`Jump to kata:\n${names}\n\nEnter number:`);
        if (choice === null) break;
        const idx = parseInt(choice, 10) - 1;
        if (idx >= 0 && idx < katas.length) {
          const state = await apiPost("/api/goto", {
            kataId: katas[idx].id,
          });
          ui.applyState(state);
        }
        break;
      }
    }
  } catch (err) {
    ui.appendOutput(
      `<div class="fail">Error: ${R.escapeHtml(err.message)}</div>`,
    );
  } finally {
    busy = false;
    ui.setBusy(false);
  }
}

// ─── Init ───

ui.bindCopyButtons();
ui.bindKeyboard();

(async function init() {
  const state = await apiGet("/api/state");
  ui.applyState(state);
})();
