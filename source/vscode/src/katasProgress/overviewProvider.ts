// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import * as vscode from "vscode";
import { log } from "qsharp-lang";
import type { OverallProgress } from "./types.js";

/**
 * Minimal state sent to the overview webview.
 */
interface OverviewState {
  detected: boolean;
  completed: number;
  total: number;
  percent: number;
  currentKataTitle?: string;
  currentSectionTitle?: string;
  canContinue: boolean;
}

function computeState(
  snapshot: OverallProgress | undefined,
  detected: boolean,
): OverviewState {
  if (!snapshot) {
    return {
      detected,
      completed: 0,
      total: 0,
      percent: 0,
      canContinue: false,
    };
  }
  const { completedSections, totalSections } = snapshot.stats;
  const percent =
    totalSections > 0
      ? Math.round((completedSections / totalSections) * 100)
      : 0;

  let currentKataTitle: string | undefined;
  let currentSectionTitle: string | undefined;
  const pos = snapshot.currentPosition;
  if (pos && pos.kataId) {
    const kata = snapshot.katas.find((k) => k.id === pos.kataId);
    if (kata) {
      currentKataTitle = kata.title;
      const section = kata.sections[pos.sectionIndex];
      if (section) currentSectionTitle = section.title;
    }
  }

  return {
    detected,
    completed: completedSections,
    total: totalSections,
    percent,
    currentKataTitle,
    currentSectionTitle,
    canContinue: detected && Boolean(pos?.kataId),
  };
}

export class KatasOverviewProvider implements vscode.WebviewViewProvider {
  public static readonly viewType = "qsharp-vscode.katasOverview";

  private view: vscode.WebviewView | undefined;
  private pending: OverviewState | undefined;

  constructor(private readonly extensionUri: vscode.Uri) {}

  resolveWebviewView(webviewView: vscode.WebviewView): void {
    this.view = webviewView;
    webviewView.webview.options = {
      enableScripts: true,
      enableCommandUris: true,
      localResourceRoots: [this.extensionUri],
    };
    webviewView.webview.html = this.getHtml(webviewView.webview);

    webviewView.webview.onDidReceiveMessage((msg) => {
      if (!msg || typeof msg !== "object") return;
      switch (msg.command) {
        case "ready":
          if (this.pending) this.post(this.pending);
          break;
        case "continue":
          log.debug("[katasOverview] continue button clicked");
          void vscode.commands.executeCommand("qsharp-vscode.katasContinue");
          break;
        case "setup":
          log.debug("[katasOverview] setup button clicked");
          void vscode.commands.executeCommand("qsharp-vscode.katasSetup");
          break;
      }
    });

    if (this.pending) this.post(this.pending);
  }

  update(snapshot: OverallProgress | undefined, detected: boolean): void {
    const state = computeState(snapshot, detected);
    this.pending = state;
    this.post(state);
  }

  private post(state: OverviewState): void {
    if (!this.view) return;
    void this.view.webview.postMessage({ command: "update", state });
  }

  private getHtml(webview: vscode.Webview): string {
    const nonce = getNonce();
    const csp = [
      `default-src 'none'`,
      `img-src ${webview.cspSource} https: data:`,
      `style-src ${webview.cspSource} 'unsafe-inline'`,
      `script-src 'nonce-${nonce}'`,
    ].join("; ");

    return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta http-equiv="Content-Security-Policy" content="${csp}" />
  <style>
    body {
      padding: 10px 12px;
      font-family: var(--vscode-font-family);
      font-size: var(--vscode-font-size);
      color: var(--vscode-foreground);
    }
    .summary { font-weight: 600; margin-bottom: 4px; }
    .subtitle {
      color: var(--vscode-descriptionForeground);
      font-size: 0.95em;
      margin-bottom: 8px;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    /* Landing-page styles (shown when no katas workspace is detected). */
    .landing { display: none; }
    .landing.active { display: block; }
    .landing h2 {
      margin: 0 0 6px 0;
      font-size: 1.05em;
      font-weight: 600;
    }
    .landing p {
      margin: 0 0 10px 0;
      color: var(--vscode-descriptionForeground);
      line-height: 1.45;
    }
    .landing ul {
      margin: 0 0 12px 0;
      padding-left: 18px;
      color: var(--vscode-descriptionForeground);
      line-height: 1.5;
    }
    .landing ul li { margin-bottom: 2px; }
    .landing ul li strong {
      color: var(--vscode-foreground);
      font-weight: 600;
    }
    .landing .cta-hint {
      color: var(--vscode-descriptionForeground);
      font-size: 0.9em;
      margin-top: 8px;
      text-align: center;
    }
    /* Tracker UI (shown when a katas workspace exists). */
    .tracker { display: none; }
    .tracker.active { display: block; }
    .hero {
      display: flex;
      align-items: center;
      gap: 12px;
      margin-bottom: 14px;
    }
    .ring {
      position: relative;
      flex: 0 0 auto;
      width: 56px;
      height: 56px;
    }
    .ring svg {
      width: 100%;
      height: 100%;
      transform: rotate(-90deg);
    }
    .ring .ring-bg {
      fill: none;
      stroke: var(--vscode-progressBar-background, rgba(128,128,128,0.25));
      stroke-width: 6;
      opacity: 0.5;
    }
    .ring .ring-fg {
      fill: none;
      stroke: var(--vscode-charts-blue, var(--vscode-textLink-foreground));
      stroke-width: 6;
      stroke-linecap: round;
      transition: stroke-dashoffset 240ms ease-out;
    }
    .ring .ring-label {
      position: absolute;
      inset: 0;
      display: flex;
      align-items: center;
      justify-content: center;
      font-size: 0.85em;
      font-weight: 600;
      color: var(--vscode-foreground);
    }
    .hero-text { min-width: 0; }
    .hero-headline {
      font-weight: 600;
      font-size: 1.05em;
      margin-bottom: 2px;
    }
    .hero-sub {
      color: var(--vscode-descriptionForeground);
      font-size: 0.9em;
    }
    .next {
      border-left: 2px solid var(--vscode-charts-blue, var(--vscode-textLink-foreground));
      padding: 6px 0 6px 10px;
      margin: 0 0 12px 0;
    }
    .next-label {
      font-size: 0.8em;
      text-transform: uppercase;
      letter-spacing: 0.04em;
      color: var(--vscode-descriptionForeground);
      margin-bottom: 2px;
    }
    .next-kata {
      font-weight: 600;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .next-section {
      color: var(--vscode-descriptionForeground);
      font-size: 0.95em;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .encourage {
      color: var(--vscode-descriptionForeground);
      font-size: 0.9em;
      line-height: 1.45;
      margin: 0 0 12px 0;
    }
    .stats {
      color: var(--vscode-descriptionForeground);
      font-size: 0.85em;
      text-align: center;
      margin-top: 8px;
    }
    button {
      width: 100%;
      padding: 6px 10px;
      border: 1px solid var(--vscode-button-border, transparent);
      background: var(--vscode-button-background);
      color: var(--vscode-button-foreground);
      cursor: pointer;
      font-family: inherit;
      font-size: inherit;
      border-radius: 2px;
    }
    button:hover { background: var(--vscode-button-hoverBackground); }
    button:disabled { opacity: 0.5; cursor: default; }
    button.secondary {
      background: var(--vscode-button-secondaryBackground, transparent);
      color: var(--vscode-button-secondaryForeground, var(--vscode-foreground));
    }
    button.secondary:hover {
      background: var(--vscode-button-secondaryHoverBackground, var(--vscode-list-hoverBackground));
    }
    .empty { color: var(--vscode-descriptionForeground); margin-bottom: 10px; }
  </style>
</head>
<body>
  <div id="root">
    <!-- Landing page: shown until a Quantum Katas workspace is detected. -->
    <div class="landing" id="landing">
      <h2>Welcome to the Quantum Katas</h2>
      <p>
        The Quantum Katas are a free, hands-on introduction to quantum computing
        with Q#. Each kata is a self-contained tutorial that teaches a concept
        through short readings followed by exercises you complete in your editor.
      </p>
      <p>What to expect:</p>
      <ul>
        <li><strong>Bite-sized lessons</strong> covering qubits, gates, measurement, entanglement, and famous algorithms like Grover's and Deutsch&ndash;Jozsa.</li>
        <li><strong>Code exercises</strong> with automated tests so you know the moment your solution is correct.</li>
        <li><strong>AI-guided learning</strong> &mdash; the Quantum Katas chat agent walks you through each section, answers questions, and gives hints on demand.</li>
        <li><strong>Progress tracking</strong> right here in this view once your workspace is set up.</li>
      </ul>
      <p>No prior quantum experience required &mdash; just curiosity.</p>
      <button id="setup">Get started</button>
      <div class="cta-hint">Sets up a workspace folder for your kata exercises.</div>
    </div>

    <!-- Tracker: shown once a katas workspace exists. -->
    <div class="tracker" id="tracker">
      <div class="hero">
        <div class="ring" aria-hidden="true">
          <svg viewBox="0 0 56 56">
            <circle class="ring-bg" cx="28" cy="28" r="24" />
            <circle class="ring-fg" id="ringFg" cx="28" cy="28" r="24"
              stroke-dasharray="150.8" stroke-dashoffset="150.8" />
          </svg>
          <div class="ring-label" id="ringLabel">0%</div>
        </div>
        <div class="hero-text">
          <div class="hero-headline" id="headline">Ready when you are</div>
          <div class="hero-sub" id="subline">Let's get started.</div>
        </div>
      </div>

      <div class="next" id="nextBlock" style="display:none;">
        <div class="next-label">Up next</div>
        <div class="next-kata" id="nextKata"></div>
        <div class="next-section" id="nextSection"></div>
      </div>

      <p class="encourage" id="encourage"></p>

      <button id="primary">Continue</button>
      <div class="stats" id="stats"></div>
    </div>
  </div>
  <script nonce="${nonce}">
    const vscode = acquireVsCodeApi();
    const landing = document.getElementById('landing');
    const tracker = document.getElementById('tracker');
    const setupBtn = document.getElementById('setup');
    const primary = document.getElementById('primary');
    const ringFg = document.getElementById('ringFg');
    const ringLabel = document.getElementById('ringLabel');
    const headline = document.getElementById('headline');
    const subline = document.getElementById('subline');
    const nextBlock = document.getElementById('nextBlock');
    const nextKata = document.getElementById('nextKata');
    const nextSection = document.getElementById('nextSection');
    const encourage = document.getElementById('encourage');
    const stats = document.getElementById('stats');

    // Circumference of r=24 circle.
    const RING_C = 2 * Math.PI * 24;
    ringFg.setAttribute('stroke-dasharray', String(RING_C));

    setupBtn.addEventListener('click', () => {
      vscode.postMessage({ command: 'setup' });
    });
    primary.addEventListener('click', () => {
      vscode.postMessage({ command: 'continue' });
    });

    function pickEncouragement(state) {
      if (state.completed === 0) {
        return "Your first kata walks you through qubits and basic gates \u2014 only takes a few minutes.";
      }
      if (state.percent >= 100) {
        return "You've finished every kata. Try a sample project, or revisit any kata to sharpen your intuition.";
      }
      if (state.percent >= 75) {
        return "You're in the home stretch \u2014 just a few katas left to go.";
      }
      if (state.percent >= 25) {
        return "Nice momentum. The chat agent can give hints anytime you get stuck.";
      }
      return "You're off to a great start. Keep going one section at a time.";
    }

    function render(state) {
      if (!state.detected) {
        landing.classList.add('active');
        tracker.classList.remove('active');
        return;
      }

      tracker.classList.add('active');
      landing.classList.remove('active');

      // Animate the ring + percent label.
      const pct = Math.max(0, Math.min(100, state.percent));
      ringFg.setAttribute('stroke-dashoffset', String(RING_C * (1 - pct / 100)));
      ringLabel.textContent = pct + '%';

      const isFresh = state.completed === 0 && !state.canContinue;
      const isDone = state.total > 0 && state.completed >= state.total;

      if (isDone) {
        headline.textContent = "All katas complete";
        subline.textContent = "Nicely done.";
        nextBlock.style.display = 'none';
        primary.textContent = 'Revisit a kata';
      } else if (isFresh) {
        headline.textContent = "Ready when you are";
        subline.textContent = "Your workspace is set up. Time to dive in.";
        nextBlock.style.display = 'none';
        primary.textContent = 'Start your first kata';
      } else if (state.canContinue && state.currentKataTitle) {
        headline.textContent = "Welcome back";
        subline.textContent = "Pick up where you left off.";
        nextBlock.style.display = 'block';
        nextKata.textContent = state.currentKataTitle;
        nextSection.textContent = state.currentSectionTitle ?? '';
        primary.textContent = 'Continue learning';
      } else {
        headline.textContent = "Keep going";
        subline.textContent = "Pick a kata to start.";
        nextBlock.style.display = 'none';
        primary.textContent = 'Open Quantum Katas';
      }

      encourage.textContent = pickEncouragement(state);
      stats.textContent = state.completed + ' of ' + state.total + ' sections complete';
      primary.disabled = false;
    }

    window.addEventListener('message', (ev) => {
      const msg = ev.data;
      if (msg && msg.command === 'update') render(msg.state);
    });

    vscode.postMessage({ command: 'ready' });
  </script>
</body>
</html>`;
  }
}

function getNonce(): string {
  const chars =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
  let out = "";
  for (let i = 0; i < 32; i++) {
    out += chars.charAt(Math.floor(Math.random() * chars.length));
  }
  return out;
}
