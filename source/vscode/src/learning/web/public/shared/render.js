// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// @ts-nocheck — vanilla JS shared between the web app and the MCP widget.
// Pure HTML builders — no DOM mutation, no fetch, no globals beyond this file.
"use strict";

(function () {
  function escapeHtml(str) {
    if (!str) return "";
    return String(str)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&#039;");
  }

  function formatComplex(re, im) {
    const r = +re.toFixed(4);
    const i = +im.toFixed(4);
    if (i === 0) return `${r}`;
    if (r === 0) return `${i}i`;
    return `${r}${i >= 0 ? "+" : ""}${i}i`;
  }

  function renderQuantumState(dump) {
    const entries = Object.entries(dump.state);
    if (entries.length === 0) return "";
    let html = `<table><tr><th>Basis</th><th>Amplitude</th><th>Probability</th></tr>`;
    for (const [basis, [re, im]] of entries) {
      const prob = (re * re + im * im) * 100;
      const amp = formatComplex(re, im);
      html += `<tr><td>${escapeHtml(basis)}</td><td>${amp}</td><td>${prob.toFixed(2)}%</td></tr>`;
    }
    html += `</table>`;
    return html;
  }

  function renderMatrix(matrixInfo) {
    const m = matrixInfo.matrix;
    if (!m || m.length === 0) return "";
    let html = `<table>`;
    for (const row of m) {
      html += `<tr>`;
      for (const [re, im] of row) {
        html += `<td>${formatComplex(re, im)}</td>`;
      }
      html += `</tr>`;
    }
    html += `</table>`;
    return html;
  }

  function renderEvents(events) {
    let html = "";
    for (const event of events) {
      switch (event.type) {
        case "message":
          html += `<div class="message">${escapeHtml(event.message)}</div>`;
          break;
        case "dump":
          html += renderQuantumState(event.dump);
          break;
        case "matrix":
          html += renderMatrix(event.matrix);
          break;
      }
    }
    return html;
  }

  function renderRunResult(result) {
    let html = renderEvents(result.events);
    if (result.error) {
      html += `<div class="fail">Error: ${escapeHtml(result.error)}</div>`;
    } else if (result.result) {
      html += `<div class="success">Result: ${escapeHtml(result.result)}</div>`;
    }
    return html;
  }

  function renderSolutionCheck(result) {
    let html = result.passed
      ? `<div class="success">✔ All tests passed!</div>`
      : `<div class="fail">✘ Tests failed</div>`;
    html += renderEvents(result.events);
    if (result.error) {
      html += `<div class="fail">${escapeHtml(result.error)}</div>`;
    }
    return html;
  }

  function renderCircuit(result) {
    return `<pre>${escapeHtml(result.ascii)}</pre>`;
  }

  function renderEstimate(result) {
    return (
      `<div>Physical qubits: <strong>${result.physicalQubits.toLocaleString()}</strong></div>` +
      `<div>Runtime: <strong>${escapeHtml(result.runtime)}</strong></div>`
    );
  }

  function renderProgress(progress) {
    const { stats, katas } = progress;
    const pct =
      stats.totalSections > 0
        ? Math.round((stats.completedSections / stats.totalSections) * 100)
        : 0;
    let html = `<div style="margin-bottom:0.5rem"><strong>Overall: ${stats.completedSections}/${stats.totalSections} sections (${pct}%)</strong></div>`;
    for (const [id, kata] of Object.entries(katas)) {
      const kPct =
        kata.total > 0 ? Math.round((kata.completed / kata.total) * 100) : 0;
      html += `<div>${escapeHtml(id)}: ${kata.completed}/${kata.total} (${kPct}%)</div>`;
    }
    return html;
  }

  function renderHint(hint) {
    if (!hint) return `<div class="message">No more hints available.</div>`;
    return (
      `<div class="hint-badge">Hint ${hint.current}/${hint.total}</div>` +
      `<div>${hint.hint}</div>`
    );
  }

  /** Content body HTML for a position item (lesson-text, lesson-example, lesson-question, exercise). */
  function renderContentBody(item) {
    switch (item.type) {
      case "lesson-text":
        return item.content;
      case "lesson-example":
        return `<pre><code>${escapeHtml(item.code)}</code></pre>`;
      case "lesson-question":
        return item.description;
      case "exercise": {
        let body = `<h3>${escapeHtml(item.title)}</h3>` + item.description;
        const fwd = item.filePath.replace(/\\/g, "/");
        const fileUrl = "file:///" + (fwd.startsWith("/") ? fwd.slice(1) : fwd);
        body += `<p class="file-path"><a class="file-path-link" href="${escapeHtml(fileUrl)}" title="Open in VS Code"><code>${escapeHtml(item.filePath)}</code></a><button class="copy-btn" data-copy="${escapeHtml(item.filePath)}" title="Copy path">📋</button></p>`;
        if (item.isComplete) {
          body += `<p class="success" style="margin-top:0.5rem">✔ Completed</p>`;
        }
        return body;
      }
      default:
        return "";
    }
  }

  function renderContentLabel(position) {
    return `${position.kataId} › Section ${position.sectionIndex + 1} › Item ${position.itemIndex + 1}`;
  }

  function renderProgressBar(progress) {
    const { stats, katas, currentPosition } = progress;
    const pct =
      stats.totalSections > 0
        ? Math.round((stats.completedSections / stats.totalSections) * 100)
        : 0;
    let html = `<span class="pb-overall">${stats.completedSections}/${stats.totalSections} (${pct}%)</span>`;
    const currentKata =
      katas && currentPosition ? katas[currentPosition.kataId] : null;
    if (currentKata) {
      html += `<span class="pb-kata-label pb-active">${escapeHtml(currentPosition.kataId)}</span>`;
      html += `<span class="pb-segments">`;
      for (const sec of currentKata.sections) {
        const isCurrent = sec.index === currentPosition.sectionIndex;
        const cls = sec.isComplete ? "done" : isCurrent ? "current" : "";
        html += `<span class="pb-seg ${cls}" title="${escapeHtml(sec.title)}"></span>`;
      }
      html += `</span>`;
    } else if (currentPosition && currentPosition.kataId) {
      html += `<span class="pb-kata-label pb-active">${escapeHtml(currentPosition.kataId)}</span>`;
    }
    return html;
  }

  /** KaTeX math rendering, if the KaTeX auto-render script is loaded. */
  function renderMath(el) {
    if (typeof renderMathInElement === "function") {
      const overrides = globalThis.__KATAS_KATEX_CONFIG ?? {};
      renderMathInElement(el, {
        delimiters: [
          { left: "$$", right: "$$", display: true },
          { left: "$", right: "$", display: false },
        ],
        throwOnError: false,
        trust: true,
        macros: {
          "\\ket": "\\left|{#1}\\right\\rangle",
          "\\bra": "\\left\\langle{#1}\\right|",
          "\\braket": "\\left\\langle{#1}\\middle|{#2}\\right\\rangle",
        },
        ...overrides,
      });
    }
  }

  const api = {
    escapeHtml,
    formatComplex,
    renderRunResult,
    renderSolutionCheck,
    renderQuantumState,
    renderMatrix,
    renderCircuit,
    renderEstimate,
    renderProgress,
    renderHint,
    renderContentBody,
    renderContentLabel,
    renderProgressBar,
    renderMath,
  };

  // Expose as a global for browsers/widgets that don't use modules.
  if (typeof globalThis !== "undefined") {
    globalThis.KatasRender = api;
  }
})();
