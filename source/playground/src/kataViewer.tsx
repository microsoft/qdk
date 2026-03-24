// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// import "preact/debug"; // Include this line only when debugging rendering
import "./kataViewer.css";

import { render } from "preact";
import { useEffect } from "preact/hooks";

// This viewer uses the html version of the katas bundle and MathJax, as quantum.microsoft.com does
import {
  Exercise,
  ExplainedSolutionItem,
  Kata,
  Lesson,
  getAllKatas,
} from "qsharp-lang/katas";

import {
  ensureTheme,
  detectThemeChange,
  updateStyleSheetTheme,
} from "qsharp-lang/ux";

declare global {
  // The below are added by the MathJax and Highlight.js scripts
  interface Window {
    MathJax: any;
    hljs: any;
  }
}

window.MathJax = {
  loader: {
    load: ["[tex]/color", "[tex]/braket"],
  },
  tex: {
    packages: { "[+]": ["color", "braket"] },
    inlineMath: [
      ["$", "$"],
      ["\\(", "\\)"],
    ],
    formatError: (jax: any, err: any) => {
      console.log("LaTeX processing error occurred. ", err, jax);
      const errorNode = document.createElement("div");
      errorNode.innerText = `LaTeX processing error: ${err.message}.\nLaTeX: ${jax.latex}\n\n`;
      errorNode.style.fontSize = "20px";
      errorNode.style.color = "red";
      document.querySelector("#errors")?.appendChild(errorNode);
      window.scroll(0, 0);
      jax.formatError(err);
    },
  },
  startup: {
    pageReady: async () => {
      await onload();
      return window.MathJax.startup.defaultPageReady();
    },
  },
};

function Nav(props: {
  katas: Kata[];
  onnav: (index: number) => void;
  selected: number;
}) {
  return (
    <div class="nav">
      {props.katas.map((kata, idx) => (
        <>
          <div
            className={
              idx === props.selected ? "nav-item nav-selected" : "nav-item"
            }
            onClick={() => props.onnav(idx)}
          >
            {kata.title}
          </div>
        </>
      ))}
    </div>
  );
}

function KataEl(props: { kata: Kata }) {
  useEffect(() => {
    window.hljs.highlightAll();
    window.MathJax.texReset();
    window.MathJax.typesetClear();
    window.MathJax.typesetPromise([".content"]);
  }, [props.kata.id]);
  window.scrollTo(0, 0);
  return (
    <div class="content" key={props.kata.id}>
      <div id="errors"></div>
      <h1>{props.kata.title}</h1>
      {props.kata.sections.map((section) =>
        section.type === "lesson" ? (
          <LessonEl lesson={section} />
        ) : (
          <ExerciseEl exercise={section} />
        ),
      )}
    </div>
  );
}

function LessonEl(props: { lesson: Lesson }) {
  const item = props.lesson;
  return (
    <>
      <h2>{item.title}</h2>
      {item.items.map((item) => {
        switch (item.type) {
          case "text-content":
            return (
              <div dangerouslySetInnerHTML={{ __html: item.content }}></div>
            );
          case "question":
            return (
              <>
                <h3>Question</h3>
                <div
                  dangerouslySetInnerHTML={{ __html: item.description.content }}
                />
                <h3>Answer</h3>
                {item.answer.items.map((answer) => (
                  <ExplainedSolution item={answer} />
                ))}
              </>
            );
          case "example":
            return (
              <pre>
                <code>{item.code}</code>
              </pre>
            );
        }
      })}
    </>
  );
}

function ExerciseEl(props: { exercise: Exercise }) {
  const item = props.exercise;
  return (
    <>
      <h2>{"Exercise: " + item.title}</h2>
      <div dangerouslySetInnerHTML={{ __html: item.description.content }} />
      <pre>
        <code>{item.placeholderCode}</code>
      </pre>
      <h4>Solution</h4>
      {item.explainedSolution.items.map((item) => (
        <ExplainedSolution item={item} />
      ))}
    </>
  );
}

function ExplainedSolution(props: { item: ExplainedSolutionItem }) {
  const item = props.item;
  return (
    <div>
      {item.type === "text-content" ? (
        <div dangerouslySetInnerHTML={{ __html: item.content }}></div>
      ) : (
        <pre>
          <code>{item.code}</code>
        </pre>
      )}
    </div>
  );
}

async function onload() {
  // Helper to react to theme changes by updating the GitHub and Highlightjs stylesheets
  const onThemeChange = (isDark: boolean) => {
    updateStyleSheetTheme(
      isDark,
      "gh/highlightjs",
      /(default\.min\.css)|(dark\.min\.css)/,
      "default.min.css",
      "dark.min.css",
    );
    updateStyleSheetTheme(
      isDark,
      "github-markdown-css",
      /(light\.css)|(dark\.css)/,
      "light.css",
      "dark.css",
    );
  };
  // Ensure a theme is set on load, set the stylesheet accordingly, and react to future changes
  onThemeChange(ensureTheme() || false);
  detectThemeChange(document.body, onThemeChange);

  const katas = await getAllKatas({ includeUnpublished: true });
  const app = document.querySelector("#app") as HTMLDivElement;

  function onRender(index: number) {
    render(
      <>
        <Nav katas={katas} onnav={onNav} selected={index} />
        <KataEl kata={katas[index]} />
      </>,
      app,
    );
  }

  // Update the history and URL fragment if the user navigates katas
  function onNav(index: number) {
    history.pushState(null, "", "#" + katas[index].id);
    onRender(index);
  }

  // Handle back/forward navigation
  window.addEventListener("popstate", () => {
    loadFromUrl();
  });

  function loadFromUrl() {
    let kataIndex = 0;
    if (window.location.hash) {
      const kataId = window.location.hash.slice(1);
      kataIndex = katas.findIndex((kata) => kata.id === kataId);
    }
    if (kataIndex < 0) kataIndex = 0;

    onRender(kataIndex);
  }

  // Do initial load
  loadFromUrl();
}
