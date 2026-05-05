// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/// <reference types="@types/vscode-webview"/>

const vscodeApi: WebviewApi<LearningState> = acquireVsCodeApi();

import { render } from "preact";
import { useEffect, useRef } from "preact/hooks";
import { Markdown, setRenderer } from "qsharp-lang/ux";
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore - there are no types for this
import mk from "@vscode/markdown-it-katex";
import markdownIt from "markdown-it";
import type {
  CurrentActivity,
  ActionGroup,
  OverallProgress,
  ActivityContent,
  SolutionCheckResult,
  OutputEvent,
  LearningState,
  HostToWebviewMessage,
} from "../types.js";
import { WebviewApi } from "vscode-webview";

const md = markdownIt("commonmark");
md.use(mk, { enableMathBlockInHtml: true, enableMathInlineInHtml: true });
setRenderer((input: string) => md.render(input));

// ─── State ───

type OutputState = {
  variant?: "pass" | "fail";
} & (
  | { type: "loading" }
  | { type: "text"; text: string }
  | { type: "check"; result: SolutionCheckResult }
);

let state: LearningState | null = null;
let output: OutputState | null = null;
let busy = false;
let currentActivityKey: string | null = null;

// ─── Messages from extension host ───

window.addEventListener("message", (event) => {
  const msg: HostToWebviewMessage = event.data;
  if (msg.command === "state" || msg.command === "result") {
    applyState(msg.state);
  }
  switch (msg.command) {
    case "state":
      break;
    case "result": {
      switch (msg.action) {
        case "next":
          if (!msg.result.moved) {
            output = {
              type: "text",
              text: "🎉 You have completed all content!",
              variant: "pass",
            };
          }
          break;
        case "back":
          if (!msg.result.moved) {
            output = {
              type: "text",
              text: "Already at the beginning.",
            };
          }
          break;
        case "check":
          output = {
            type: "check",
            result: msg.result,
            variant: msg.result.passed ? "pass" : "fail",
          };
          break;
        case "run":
        case "circuit":
          output = null;
          break;
      }
      break;
    }
    case "error":
      output = {
        type: "text",
        text: "Error: " + msg.message,
        variant: "fail",
      };
      break;
  }
  busy = false;
  rerender();
});

function applyState(newState: LearningState): void {
  const newActivity =
    newState.position.unitId + ":" + newState.position.activityId;
  if (newActivity !== currentActivityKey) {
    output = null;
    currentActivityKey = newActivity;
  }
  state = newState;
  vscodeApi.setState(state);
}

function rerender() {
  render(<App />, document.body);
}

// ─── Action dispatch ───

function executeAction(action: string): void {
  if (busy) {
    return;
  }

  if (action === "hint-chat") {
    openChat("Give me a hint");
    return;
  }

  if (action === "explain-chat") {
    openChat("Explain this concept in more detail");
    return;
  }

  busy = true;
  const slow = ["run", "circuit", "check"].indexOf(action) >= 0;
  if (slow) {
    output = { type: "loading" };
  }
  rerender();
  vscodeApi.postMessage({ command: "action", action });
}

// ─── Helpers ───

function openFile(filePath: string) {
  const fwd = filePath.replace(/\\/g, "/");
  const fileUrl = "file:///" + (fwd.startsWith("/") ? fwd.slice(1) : fwd);
  vscodeApi.postMessage({ command: "openFile", uri: fileUrl });
}

function openChat(text: string) {
  vscodeApi.postMessage({ command: "openChat", text });
}

// ─── Components ───

function App() {
  if (!state) {
    return <div class="loading">Loading...</div>;
  }
  return (
    <>
      <Branding />
      <Header current={state.position} />
      <ContentBody
        content={state.position.content}
        activityKey={currentActivityKey}
      />
      {output ? <OutputPanel output={output} /> : null}
      <ActionBar groups={state.actions} busy={busy} />
      <ProgressBar progress={state.progress} />
    </>
  );
}

function Branding() {
  const mobiusUri = document.body.dataset.mobiusUri ?? "";
  return (
    <div class="branding">
      <img class="branding-icon" src={mobiusUri} width="18" height="18" />
      <span class="branding-text">Microsoft Quantum Katas</span>
    </div>
  );
}

function Header({ current: current }: { current: CurrentActivity }) {
  const content = current.content;
  const crumb = current.unitTitle + " › " + current.activityTitle;

  let badgeText: string;
  let badgeClass = "badge";
  if (content.type === "exercise") {
    badgeText = content.isComplete ? "✔ done" : "exercise";
    badgeClass += content.isComplete ? " complete" : " exercise";
  } else if (content.type === "lesson-text") {
    badgeText = "lesson";
  } else {
    badgeText = "example";
  }

  return (
    <header class="header">
      <span class="crumb">{crumb}</span>
      <span class={badgeClass}>{badgeText}</span>
    </header>
  );
}

function ContentBody({
  content,
  activityKey,
}: {
  content: ActivityContent;
  activityKey: string | null;
}) {
  const ref = useRef<HTMLElement>(null);

  useEffect(() => {
    if (ref.current) {
      ref.current.scrollTop = 0;
    }
  }, [activityKey]);

  return (
    <section id="content" class="content" ref={ref}>
      {content.type === "lesson-text" && (
        <Markdown markdown={content.content ?? ""} />
      )}
      {content.type === "lesson-example" && <LessonExample content={content} />}
      {content.type === "exercise" && <Exercise content={content} />}
    </section>
  );
}

function LessonExample({
  content,
}: {
  content: Extract<ActivityContent, { type: "lesson-example" }>;
}) {
  return (
    <>
      {content.contentBefore && <Markdown markdown={content.contentBefore} />}
      {content.filePath && (
        <FilePathNote
          message="This example should be open in the editor to the right. If it’s not visible,"
          linkText="open it here"
          filePath={content.filePath}
        />
      )}
      {content.contentAfter && <Markdown markdown={content.contentAfter} />}
    </>
  );
}

function Exercise({
  content,
}: {
  content: Extract<ActivityContent, { type: "exercise" }>;
}) {
  return (
    <>
      <h3>{content.title ?? ""}</h3>
      {content.description && <Markdown markdown={content.description} />}
      {content.filePath && (
        <FilePathNote
          message="Your code file should be open in the editor to the right. If it’s not visible,"
          linkText="open it here"
          filePath={content.filePath}
        />
      )}
      {content.isComplete && (
        <div class="completion-banner">
          <span class="completion-icon">✓</span> Correct!
        </div>
      )}
    </>
  );
}

function FilePathNote({
  message,
  linkText,
  filePath,
}: {
  message: string;
  linkText: string;
  filePath: string;
}) {
  return (
    <p class="file-path">
      {message}{" "}
      <a
        class="file-path-link"
        href="#"
        title="Open in editor"
        onClick={(e) => {
          e.preventDefault();
          openFile(filePath);
        }}
      >
        {linkText}
      </a>
      .
    </p>
  );
}

function OutputPanel({ output: out }: { output: OutputState }) {
  const className = "output" + (out.variant ? " " + out.variant : "");
  const label = out.variant ? "Result" : "Output";

  const onDismiss = () => {
    output = null;
    rerender();
  };

  return (
    <div class={className}>
      <button
        class="out-dismiss"
        aria-label="Dismiss"
        title="Dismiss"
        onClick={onDismiss}
      >
        ×
      </button>
      <div class="out-label">{label}</div>
      <div class="out-body">
        <OutputBody output={out} />
      </div>
    </div>
  );
}

function OutputBody({ output: out }: { output: OutputState }) {
  switch (out.type) {
    case "loading":
      return <div class="loading">Working…</div>;
    case "text": {
      const cls =
        out.variant === "pass"
          ? "success"
          : out.variant === "fail"
            ? "fail"
            : "message";
      return <div class={cls}>{out.text}</div>;
    }
    case "check":
      return <SolutionResult result={out.result} />;
  }
}

function SolutionResult({ result }: { result: SolutionCheckResult }) {
  return (
    <>
      {result.passed ? (
        <div class="success">✔ All tests passed!</div>
      ) : (
        <div class="fail">✘ Check failed</div>
      )}
      <EventList events={result.events} />
      {result.error && <div class="fail">{result.error}</div>}
      {!result.passed && (
        <span
          class="chat-link"
          onClick={() => openChat("Help me understand why my solution failed")}
        >
          <span class="codicon codicon-sparkle" /> What went wrong?
        </span>
      )}
    </>
  );
}

function EventList({ events }: { events: OutputEvent[] }) {
  return (
    <>
      {events.map((event, i) => {
        if (event.type === "message") {
          return (
            <div key={i} class="message">
              {event.message}
            </div>
          );
        }
        return null;
      })}
    </>
  );
}

function ActionBar({
  groups,
  busy: isBusy,
}: {
  groups: ActionGroup[];
  busy: boolean;
}) {
  // Keyboard shortcut handling
  useEffect(() => {
    const allBindings = groups.flat();
    const handler = (e: KeyboardEvent) => {
      if (isBusy) {
        return;
      }
      const tag = ((e.target as HTMLElement).tagName || "").toLowerCase();
      if (tag === "input" || tag === "textarea" || tag === "select") {
        return;
      }

      let key = e.key.toLowerCase();
      if (key === " ") {
        key = "space";
      }

      // Space triggers the primary action
      if (key === "space") {
        const primary = allBindings.find((b) => b.primary);
        if (primary) {
          e.preventDefault();
          executeAction(primary.action);
          return;
        }
      }

      // Single-letter shortcuts
      if (key.length === 1) {
        const match = allBindings.find((b) => b.key === key);
        if (match) {
          e.preventDefault();
          executeAction(match.action);
        }
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [groups, isBusy]);

  return (
    <nav class="action-bar">
      {groups.map((group, gi) => {
        const buttons = group.filter(
          (b) => b.action !== "quit" && b.action !== "menu",
        );
        if (buttons.length === 0) {
          return null;
        }
        return (
          <div class="action-group" key={gi}>
            {buttons.map((binding) => {
              let tip = binding.label;
              if (binding.key && binding.key !== "space") {
                tip += " (" + binding.key.toUpperCase() + ")";
              } else if (binding.key === "space") {
                tip += " (Space)";
              }
              if (binding.codicon === "sparkle") {
                tip += " — opens Copilot Chat";
              }
              return (
                <button
                  key={binding.action}
                  class={binding.primary ? "primary" : undefined}
                  title={tip}
                  disabled={isBusy}
                  onClick={() => executeAction(binding.action)}
                >
                  {binding.codicon && (
                    <span class={`codicon codicon-${binding.codicon}`} />
                  )}
                  {binding.codicon ? " " + binding.label : binding.label}
                </button>
              );
            })}
          </div>
        );
      })}
    </nav>
  );
}

function ProgressBar({ progress }: { progress: OverallProgress }) {
  const { stats, units, currentPosition } = progress;
  const pct =
    stats.totalActivities > 0
      ? Math.round((stats.completedActivities / stats.totalActivities) * 100)
      : 0;

  const currentUnit =
    units && currentPosition
      ? units.find((k) => k.id === currentPosition.unitId)
      : null;

  const onClick = () => {
    vscodeApi.postMessage({ command: "focusProgress" });
  };

  return (
    <footer class="progress-bar" title="View progress" onClick={onClick}>
      <span class="pb-overall">
        {stats.completedActivities}/{stats.totalActivities} ({pct}%)
      </span>
      {currentUnit ? (
        <>
          <span class="pb-kata-label pb-active">
            {currentPosition!.unitTitle || currentPosition!.unitId}
          </span>
          <span class="pb-segments">
            {currentUnit.activities.map((act) => {
              const isCurrent = act.id === currentPosition!.activityId;
              const cls =
                "pb-seg" +
                (act.isComplete ? " done" : isCurrent ? " current" : "");
              return <span key={act.id} class={cls} title={act.title} />;
            })}
          </span>
        </>
      ) : currentPosition && currentPosition.unitId ? (
        <span class="pb-kata-label pb-active">
          {currentPosition.unitTitle || currentPosition.unitId}
        </span>
      ) : null}
    </footer>
  );
}

// ─── Init ───

state = vscodeApi.getState() ?? null;
rerender();
vscodeApi.postMessage({ command: "ready" });
