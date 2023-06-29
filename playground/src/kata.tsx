// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { useEffect, useRef } from "preact/hooks";
import {
  CompilerState,
  ICompilerWorker,
  ILanguageServiceWorker,
  Kata,
  QscEventTarget,
} from "qsharp";
import { Editor } from "./editor.js";
import { OutputTabs } from "./tabs.js";

export function Kata(props: {
  kata: Kata;
  compiler: ICompilerWorker;
  compilerState: CompilerState;
  onRestartCompiler: () => void;
  languageService: ILanguageServiceWorker;
}) {
  const kataContent = useRef<HTMLDivElement>(null);
  const itemContent = useRef<(HTMLDivElement | null)[]>([]);

  // Need to keep around QscEventTargets around on re-render unless the Kata changes.
  const lastKata = useRef<Kata>();
  const handlerMap = useRef<QscEventTarget[]>();
  if (lastKata.current !== props.kata) {
    lastKata.current = props.kata;

    // This gives an extra EventTarget we don't need for 'reading' types, but that's fine.
    handlerMap.current = props.kata.items.map(() => new QscEventTarget(true));
  }
  const itemEvtHandlers = handlerMap.current || [];

  useEffect(() => {
    // MathJax rendering inside of React components seems to mess them up a bit,
    // so we'll take control of it here and ensure the contents are replaced.
    if (!kataContent.current) return;
    kataContent.current.innerHTML = props.kata.contentAsHtml;

    props.kata.items.forEach((item, idx) => {
      const parentDiv = itemContent.current[idx];
      const div = parentDiv?.querySelector(".kata-item-content");
      if (!div) return;
      div.innerHTML = item.contentAsHtml;
    });
    // In case we're now rendering less items than before, be sure to truncate
    itemContent.current.length = props.kata.items.length;

    MathJax.typeset();
  }, [props.kata]);

  return (
    <div class="markdown-body kata-override">
      <div ref={kataContent}></div>
      <br></br>
      {props.kata.items.map((item, idx) => {
        if (item.type === "reading") {
          return (
            <div ref={(elem) => (itemContent.current[idx] = elem)}>
              <div class="kata-item-content"></div>
            </div>
          );
        }

        return (
          <div>
            <div ref={(elem) => (itemContent.current[idx] = elem)}>
              <div class="kata-item-content"></div>
            </div>
            <Editor
              defaultShots={1}
              showExpr={false}
              showShots={false}
              evtTarget={itemEvtHandlers[idx]}
              compiler={props.compiler}
              compilerState={props.compilerState}
              onRestartCompiler={props.onRestartCompiler}
              code={
                item.type === "exercise"
                  ? item.placeholderImplementation
                  : item.source
              }
              kataVerify={
                item.type === "exercise" ? item.verificationImplementation : ""
              }
              key={item.id}
              setHir={() => ({})}
              activeTab="results-tab"
              languageService={props.languageService}
            ></Editor>
            <OutputTabs
              key={item.id + "-results"}
              evtTarget={itemEvtHandlers[idx]}
              showPanel={false}
              kataMode={true}
              hir=""
              activeTab="results-tab"
              setActiveTab={() => undefined}
            ></OutputTabs>
          </div>
        );
      })}
    </div>
  );
}
