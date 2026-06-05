// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { useState } from "preact/hooks";
import { QscEventTarget } from "qsharp-lang";
import { Editor } from "./editor.js";
import { OutputTabs } from "./tabs.js";
import { BlochSphere, Markdown } from "qsharp-lang/ux";

import type {
  CompilerState,
  ICompilerWorker,
  ILanguageServiceWorker,
  VSDiagnostic,
} from "qsharp-lang";

import type {
  ExplainedSolution,
  Kata,
  Question,
  KataSection,
  BlochItem,
} from "qsharp-lang/katas";

type Props = {
  kata: Kata;
  compiler: ICompilerWorker;
  compiler_worker_factory: () => ICompilerWorker;
  compilerState: CompilerState;
  onRestartCompiler: () => void;
  languageService: ILanguageServiceWorker;
};

// Wrapper for the interactive Bloch sphere widget embedded inline in a
// kata via the `@[bloch](...)` directive. We give it a bit of vertical
// breathing room above and below so it visually separates from the
// surrounding prose, and render the optional caption as a small
// heading. The widget itself is the standard `BlochSphere` -- no
// special compact mode -- so students get the full transport bar,
// gate editor, and history pane to play with.
function BlochEmbed({ item }: { item: BlochItem }) {
  return (
    <div class="kata-bloch-embed">
      {item.title && <h4 class="kata-bloch-embed-title">{item.title}</h4>}
      <BlochSphere initialGates={item.gates} />
    </div>
  );
}

function ExplainedSolutionElem(props: { solution: ExplainedSolution }) {
  return (
    <details>
      <summary>{"💡 Solution"}</summary>
      {props.solution.items.map((item) => {
        switch (item.type) {
          case "example":
          case "solution":
            return (
              <pre>
                <code>{item.code}</code>
              </pre>
            );
          case "text-content":
            return <Markdown markdown={item.content}></Markdown>;
          case "bloch":
            return <BlochEmbed item={item} />;
        }
      })}
    </details>
  );
}

function QuestionElem(props: { question: Question }) {
  return (
    <>
      <h2>{"❓ Question:"}</h2>
      <Markdown markdown={props.question.description.content}></Markdown>
      <details>
        <summary>
          <strong>{"💡 Answer"}</strong>
        </summary>
        {props.question.answer.items.map((item) => {
          switch (item.type) {
            case "example":
              return (
                <pre>
                  <code>{item.code}</code>
                </pre>
              );
            case "text-content":
              return <Markdown markdown={item.content}></Markdown>;
            case "bloch":
              return <BlochEmbed item={item} />;
          }
        })}
      </details>
    </>
  );
}

function LessonElem(props: Props & { section: KataSection }) {
  if (props.section.type !== "lesson") throw "Invalid section type";
  const lesson = props.section;

  const [shotError, setShotError] = useState<VSDiagnostic>();
  const [evtHandler] = useState(() => new QscEventTarget(true));

  return (
    <div>
      <div class="section-title">
        <h1>
          {"📖 Lesson: "}
          <u>{lesson.title}</u>
        </h1>
      </div>
      <div class="kata-text-content">
        {lesson.items.map((item) => {
          switch (item.type) {
            case "example":
              return (
                <>
                  <Editor
                    code={item.code}
                    compiler={props.compiler}
                    compiler_worker_factory={props.compiler_worker_factory}
                    compilerState={props.compilerState}
                    onRestartCompiler={props.onRestartCompiler}
                    kataSection={lesson}
                    evtTarget={evtHandler}
                    defaultShots={1}
                    showShots={false}
                    showExpr={false}
                    shotError={shotError}
                    setAst={() => ({})}
                    setHir={() => ({})}
                    setRir={() => ({})}
                    setQir={() => ({})}
                    activeTab="results-tab"
                    languageService={props.languageService}
                  ></Editor>
                  <OutputTabs
                    evtTarget={evtHandler}
                    showPanel={false}
                    kataMode={true}
                    onShotError={(diag?: VSDiagnostic) => setShotError(diag)}
                    ast=""
                    hir=""
                    rir={["", ""]}
                    qir=""
                    activeTab="results-tab"
                    setActiveTab={() => undefined}
                  ></OutputTabs>
                </>
              );
            case "text-content":
              return <Markdown markdown={item.content}></Markdown>;
            case "question":
              return <QuestionElem question={item}></QuestionElem>;
            case "bloch":
              return <BlochEmbed item={item} />;
          }
        })}
      </div>
    </div>
  );
}

function ExerciseElem(props: Props & { section: KataSection }) {
  if (props.section.type !== "exercise") throw "Invalid section type";
  const exercise = props.section;

  const [shotError, setShotError] = useState<VSDiagnostic>();
  const [evtHandler] = useState(() => new QscEventTarget(true));

  return (
    <div>
      <div class="section-title">
        <h1>
          {"⌨ Exercise: "}
          <u>{exercise.title}</u>
        </h1>
      </div>
      <Markdown
        className="excercise-description"
        markdown={exercise.description.content}
      />
      <div>
        <Editor
          defaultShots={1}
          showExpr={false}
          showShots={false}
          shotError={shotError}
          evtTarget={evtHandler}
          compiler={props.compiler}
          compilerState={props.compilerState}
          compiler_worker_factory={props.compiler_worker_factory}
          onRestartCompiler={props.onRestartCompiler}
          code={exercise.placeholderCode}
          kataSection={exercise}
          setAst={() => ({})}
          setHir={() => ({})}
          setRir={() => ({})}
          setQir={() => ({})}
          activeTab="results-tab"
          languageService={props.languageService}
        ></Editor>
        <OutputTabs
          evtTarget={evtHandler}
          showPanel={false}
          kataMode={true}
          onShotError={(diag?: VSDiagnostic) => setShotError(diag)}
          ast=""
          hir=""
          rir={["", ""]}
          qir=""
          activeTab="results-tab"
          setActiveTab={() => undefined}
        ></OutputTabs>
      </div>
      <ExplainedSolutionElem
        solution={exercise.explainedSolution}
      ></ExplainedSolutionElem>
    </div>
  );
}

export function Kata(props: Props) {
  return (
    <div class="markdown-body kata-override">
      {props.kata.sections.map((section) =>
        section.type === "lesson" ? (
          <LessonElem {...props} section={section}></LessonElem>
        ) : (
          <ExerciseElem {...props} section={section}></ExerciseElem>
        ),
      )}
    </div>
  );
}
