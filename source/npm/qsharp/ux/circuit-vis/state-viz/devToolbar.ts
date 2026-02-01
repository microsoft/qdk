// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import type { Endianness } from "./stateComputeCore.js";
import type { AmpMap } from "./stateViz.js";

export type DataMode = "live" | "mock";

export interface DevToolbarState {
  endianness: Endianness;
  dataMode: DataMode;
  mockSet: number;
  minProbThreshold: number; // 0..1
}

export const createDefaultDevToolbarState = (): DevToolbarState => ({
  endianness: "big",
  dataMode: "live",
  mockSet: 0,
  minProbThreshold: 0.0,
});

/**
 * Attach a developer toolbar to the provided state panel element.
 * The toolbar mutates `state` in-place and calls `onChange(state)` on changes.
 * If a toolbar already exists, this function is a no-op.
 */
export const attachStateDevToolbar = (
  panelElem: HTMLElement,
  state: DevToolbarState,
  onChange: (s: DevToolbarState) => void,
): void => {
  const svgElem = panelElem.querySelector("svg.state-svg");
  if (!svgElem) return;
  if (panelElem.querySelector(".dev-toolbar")) return;

  const toolbar = document.createElement("div");
  toolbar.className = "dev-toolbar";

  // Endianness control
  const labelEndian = document.createElement("span");
  labelEndian.textContent = "Endianness:";
  const selEndian = document.createElement("select");
  selEndian.className = "endianness-select";
  const optBig = document.createElement("option");
  optBig.value = "big";
  optBig.text = "Big";
  const optLittle = document.createElement("option");
  optLittle.value = "little";
  optLittle.text = "Little";
  selEndian.appendChild(optBig);
  selEndian.appendChild(optLittle);
  selEndian.value = state.endianness;
  selEndian.addEventListener("change", () => {
    state.endianness = (selEndian.value as Endianness) ?? "big";
    onChange(state);
  });
  toolbar.appendChild(labelEndian);
  toolbar.appendChild(selEndian);

  // Separator
  const sep = document.createElement("span");
  sep.className = "dev-toolbar-sep";
  sep.textContent = "|";
  toolbar.appendChild(sep);

  // Data mode control
  const labelMode = document.createElement("span");
  labelMode.textContent = "Data:";
  const selMode = document.createElement("select");
  selMode.className = "data-mode-select";
  const optLive = document.createElement("option");
  optLive.value = "live";
  optLive.text = "Live";
  const optMock = document.createElement("option");
  optMock.value = "mock";
  optMock.text = "Mock";
  selMode.appendChild(optLive);
  selMode.appendChild(optMock);
  selMode.value = state.dataMode;
  toolbar.appendChild(labelMode);
  toolbar.appendChild(selMode);

  // Mock set selector
  const labelMock = document.createElement("span");
  labelMock.textContent = "Mock set:";
  const selMock = document.createElement("select");
  selMock.className = "mock-set-select";
  for (let i = 0; i < 4; i++) {
    const opt = document.createElement("option");
    opt.value = String(i);
    opt.text = `#${i + 1}`;
    selMock.appendChild(opt);
  }
  selMock.value = String(state.mockSet);
  const applyMockVisibility = () => {
    const show = selMode.value === "mock";
    labelMock.style.display = show ? "" : "none";
    selMock.style.display = show ? "" : "none";
  };
  selMode.addEventListener("change", () => {
    state.dataMode = (selMode.value as DataMode) ?? "live";
    applyMockVisibility();
    onChange(state);
  });
  selMock.addEventListener("change", () => {
    state.mockSet = parseInt(selMock.value) || 0;
    onChange(state);
  });
  applyMockVisibility();
  toolbar.appendChild(labelMock);
  toolbar.appendChild(selMock);

  // Separator
  const sep2 = document.createElement("span");
  sep2.className = "dev-toolbar-sep";
  sep2.textContent = "|";
  toolbar.appendChild(sep2);

  // Minimum probability threshold control (percentage)
  const labelThresh = document.createElement("span");
  labelThresh.textContent = "Min %:";
  const inputThresh = document.createElement("input");
  inputThresh.type = "number";
  inputThresh.min = "0";
  inputThresh.max = "100";
  inputThresh.step = "0.1";
  inputThresh.value = String(state.minProbThreshold * 100);
  inputThresh.title = "States below this percentage are aggregated into Others";
  inputThresh.addEventListener("change", () => {
    const v = parseFloat(inputThresh.value);
    const pct = isFinite(v) && v > 0 ? Math.min(100, Math.max(0, v)) : 0;
    inputThresh.value = String(pct);
    state.minProbThreshold = pct / 100;
    onChange(state);
  });
  toolbar.appendChild(labelThresh);
  toolbar.appendChild(inputThresh);

  panelElem.insertBefore(toolbar, svgElem);
};

// Mock data sets for testing

// Static mock map with a few non-zero amplitudes; other states are implicitly zero.
export const getStaticMockAmpMap = (setNum: number): AmpMap => {
  switch (setNum % 4) {
    case 0:
      return staticMockAmp1();
    case 1:
      return staticMockAmp2();
    case 2:
      return staticMockAmp3();
    case 3:
      return staticMockAmp4();
    default:
      return {};
  }
};

const staticMockAmp1 = (): AmpMap => {
  // 3-qubit example with evenly varied phases across states
  const states: Array<{ bit: string; p: number }> = [
    { bit: "000", p: 0.35 },
    { bit: "001", p: 0.2 },
    { bit: "010", p: 0.1 },
    { bit: "011", p: 0.0825 },
    { bit: "100", p: 0.07 },
    { bit: "101", p: 0.06 },
    { bit: "110", p: 0.03 },
    { bit: "111", p: 0.01 },
  ];
  const N = states.length;
  const ampMap: AmpMap = {};
  states.forEach((s, i) => {
    const phi = N > 1 ? -Math.PI + (2 * Math.PI * i) / (N - 1) : 0;
    const mag = Math.sqrt(s.p);
    ampMap[s.bit] = { re: mag * Math.cos(phi), im: mag * Math.sin(phi) };
  });

  return ampMap;
};

const staticMockAmp2 = (): AmpMap => {
  const ampMap = staticMockAmp1();

  delete ampMap["000"];
  delete ampMap["001"];
  delete ampMap["010"];
  delete ampMap["100"];
  delete ampMap["101"];
  delete ampMap["110"];
  delete ampMap["111"];

  return ampMap;
};

const staticMockAmp3 = (): AmpMap => {
  const ampMap = staticMockAmp1();

  delete ampMap["000"];
  delete ampMap["001"];
  delete ampMap["010"];
  delete ampMap["100"];
  delete ampMap["110"];
  delete ampMap["111"];

  return ampMap;
};

const staticMockAmp4 = (): AmpMap => {
  return {
    A: { prob: 0.6, phase: 0 },
    B: { prob: 0.6, phase: 0 },
    C: { prob: 0.6, phase: 0 },
    D: { prob: 0.6, phase: 0 },
    E: { prob: 0.6, phase: 0 },
    F: { prob: 0.6, phase: 0 },
    G: { prob: 0.6, phase: 0 },
    H: { prob: 0.6, phase: 0 },
    I: { prob: 0.6, phase: 0 },
    J: { prob: 0.6, phase: 0 },
    K: { prob: 0.6, phase: 0 },
    L: { prob: 0.6, phase: 0 },
    M: { prob: 0.6, phase: 0 },
    N: { prob: 0.6, phase: 0 },
    O: { prob: 0.6, phase: 0 },
    P: { prob: 0.6, phase: 0 },
    Q: { prob: 0.6, phase: 0 },
    R: { prob: 0.6, phase: 0 },
    S: { prob: 0.65, phase: 0 },
    T: { prob: 0.6, phase: 0 },
  };
};
