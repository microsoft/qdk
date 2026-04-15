// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { QscEventTarget, VSDiagnostic } from "qsharp-lang";
import { ResultsTab } from "./results.js";
import { ActiveTab } from "./main.js";

const tabArray: Array<[ActiveTab, string]> = [
  ["results-tab", "RESULTS"],
  ["ast-tab", "AST"],
  ["hir-tab", "HIR"],
  ["rir-tab", "RIR"],
  ["qir-tab", "QIR"],
];

function AstTab(props: { ast: string; activeTab: ActiveTab }) {
  return props.activeTab === "ast-tab" ? (
    <textarea readOnly class="ast-output">
      {props.ast}
    </textarea>
  ) : null;
}

function HirTab(props: { hir: string; activeTab: ActiveTab }) {
  return props.activeTab === "hir-tab" ? (
    <textarea readOnly class="hir-output">
      {props.hir}
    </textarea>
  ) : null;
}

function RirTab(props: { rir: string[]; activeTab: ActiveTab }) {
  const raw = props.rir[0];
  const ssa = props.rir[1];
  return props.activeTab === "rir-tab" ? (
    <div>
      <textarea readOnly class="rir-output">
        {raw}
      </textarea>
      <textarea readOnly class="rir-output">
        {ssa}
      </textarea>
    </div>
  ) : null;
}

function QirTab(props: { qir: string; activeTab: ActiveTab }) {
  return props.activeTab === "qir-tab" ? (
    <textarea readOnly class="qir-output">
      {props.qir}
    </textarea>
  ) : null;
}

export function OutputTabs(props: {
  evtTarget: QscEventTarget;
  showPanel: boolean;
  onShotError?: (err?: VSDiagnostic) => void;
  kataMode?: boolean;
  ast: string;
  hir: string;
  rir: string[];
  qir: string;
  activeTab: ActiveTab;
  setActiveTab: (tab: ActiveTab) => void;
}) {
  return (
    <div class="results-column">
      {props.showPanel ? (
        <div class="results-labels">
          {tabArray.map((elem) => (
            <div
              onClick={() => props.setActiveTab(elem[0])}
              class={props.activeTab === elem[0] ? "active-tab" : ""}
            >
              {elem[1]}
            </div>
          ))}
        </div>
      ) : null}
      <ResultsTab {...props} />
      <AstTab {...props} />
      <HirTab {...props} />
      <RirTab {...props} />
      <QirTab {...props} />
    </div>
  );
}
