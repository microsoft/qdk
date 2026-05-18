// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/// <reference types="@types/vscode-webview"/>

const vscodeApi: WebviewApi<LearningState> = acquireVsCodeApi();

import { render } from "preact";
import { useEffect, useReducer, useRef } from "preact/hooks";
import { Markdown, setRenderer } from "qsharp-lang/ux";
import "./webview.css";
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore - there are no types for this
import mk from "@vscode/markdown-it-katex";
import markdownIt from "markdown-it";
import type {
  CurrentActivity,
  ActionGroup,
  OverallProgress,
  ActivityContent,
  ActivityLocation,
  SolutionCheckResult,
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

type AppState = {
  learning: LearningState | null;
  output: OutputState | null;
  busy: boolean;
};

type AppAction =
  /** Full state refresh (initial load, external progress change, etc.). */
  | { type: "stateUpdate"; state: LearningState }
  /** A next/back navigation completed. If !moved, the user hit an edge. */
  | {
      type: "navResult";
      direction: "next" | "back";
      moved: boolean;
      state: LearningState;
    }
  /** An exercise check completed with pass/fail and captured output. */
  | { type: "checkResult"; result: SolutionCheckResult; state: LearningState }
  /** A run completed (output shown in the editor, not here). */
  | { type: "runComplete"; state: LearningState }
  /** An unrecoverable error during action execution. */
  | { type: "error"; message: string }
  /** User triggered an action; sets busy flag and optionally shows a loading spinner. */
  | { type: "startAction"; slow: boolean }
  /** User dismissed the output panel. */
  | { type: "dismissOutput" };

/** Given the current state and an action, returns the next state. No side effects. */
function reducer(state: AppState, action: AppAction): AppState {
  switch (action.type) {
    case "stateUpdate":
      return applyState(state, action.state, state.output);
    case "navResult": {
      let output = state.output;
      if (!action.moved) {
        output =
          action.direction === "next"
            ? {
                type: "text",
                text: "🎉 You have completed all content!",
                variant: "pass",
              }
            : { type: "text", text: "Already at the beginning." };
      }
      return applyState(state, action.state, output);
    }
    case "checkResult":
      return applyState(state, action.state, {
        type: "check",
        result: action.result,
        variant: action.result.passed ? "pass" : "fail",
      });
    case "runComplete":
      return applyState(state, action.state, null);
    case "error":
      return {
        ...state,
        output: {
          type: "text",
          text: "Error: " + action.message,
          variant: "fail",
        },
        busy: false,
      };
    case "startAction":
      return {
        ...state,
        busy: true,
        output: action.slow ? { type: "loading" } : state.output,
      };
    case "dismissOutput":
      return { ...state, output: null };
  }
}

/** Apply a new LearningState, clearing output if the activity changed. */
function applyState(
  prev: AppState,
  learning: LearningState,
  output: OutputState | null,
): AppState {
  const p = prev.learning?.position;
  const n = learning.position;
  if (!p || locationKey(p.location) !== locationKey(n.location)) {
    output = null;
  }
  return { learning, output, busy: false };
}

// ─── Helpers ───

function openFile(uri: string) {
  vscodeApi.postMessage({ command: "openFile", uri });
}

function openChat(text: string) {
  vscodeApi.postMessage({ command: "openChat", text });
}

function messageToAction(msg: HostToWebviewMessage): AppAction {
  if (msg.command === "state") {
    return { type: "stateUpdate", state: msg.state };
  }
  if (msg.command === "error") {
    return { type: "error", message: msg.message };
  }
  switch (msg.action) {
    case "next":
    case "back":
      return {
        type: "navResult",
        direction: msg.action,
        moved: msg.result.moved,
        state: msg.state,
      };
    case "check":
      return {
        type: "checkResult",
        result: msg.result,
        state: msg.state,
      };
    case "run":
      return { type: "runComplete", state: msg.state };
  }
}

function locationKey(loc: ActivityLocation): string {
  return `${loc.courseId}__${loc.unitId}__${loc.activityId}`;
}

// ─── Components ───

function App() {
  const [appState, dispatch] = useReducer(reducer, null, () => ({
    learning: vscodeApi.getState() ?? null,
    output: null,
    busy: false,
  }));
  const { learning, output, busy } = appState;

  // Listen for messages from the extension host
  useEffect(() => {
    const handler = (event: MessageEvent) => {
      dispatch(messageToAction(event.data));
    };
    window.addEventListener("message", handler);
    return () => window.removeEventListener("message", handler);
  }, []);

  // Persist learning state for webview restoration
  useEffect(() => {
    if (learning) {
      vscodeApi.setState(learning);
    }
  }, [learning]);

  // Signal readiness to the extension host
  useEffect(() => {
    vscodeApi.postMessage({ command: "ready" });
  }, []);

  const onAction = (action: string) => {
    if (busy) return;

    if (action === "hint-chat") {
      openChat("Give me a hint");
      return;
    }
    if (action === "explain-chat") {
      openChat("Explain this concept in more detail");
      return;
    }

    const slow = ["run", "check"].includes(action);
    dispatch({ type: "startAction", slow });
    vscodeApi.postMessage({ command: "action", action });
  };

  const onDismiss = () => dispatch({ type: "dismissOutput" });

  if (!learning) {
    return <div class="loading">Loading...</div>;
  }

  return (
    <>
      <Branding />
      <Header current={learning.position} />
      <ContentBody
        content={learning.position.content}
        activityKey={locationKey(learning.position.location)}
      />
      {output ? <OutputPanel output={output} onDismiss={onDismiss} /> : null}
      <ActionBar groups={learning.actions} busy={busy} onAction={onAction} />
      <ProgressBar progress={learning.progress} />
    </>
  );
}

function Branding() {
  const mobiusUri = document.body.dataset.mobiusUri ?? "";
  return (
    <div class="branding">
      <img
        class="branding-icon"
        src={mobiusUri}
        width="18"
        height="18"
        alt="Microsoft Quantum logo"
      />
      <span class="branding-text">Microsoft Quantum Katas</span>
    </div>
  );
}

function Header({ current }: { current: CurrentActivity }) {
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

function OutputPanel({
  output: out,
  onDismiss,
}: {
  output: OutputState;
  onDismiss: () => void;
}) {
  const className = "output" + (out.variant ? " " + out.variant : "");
  const label = out.variant ? "Result" : "Output";

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
      <EventList messages={result.messages} />
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

function EventList({ messages }: { messages: string[] }) {
  return (
    <>
      {messages.map((msg, i) => (
        <div key={i} class="message">
          {msg}
        </div>
      ))}
    </>
  );
}

function ActionBar({
  groups,
  busy: isBusy,
  onAction,
}: {
  groups: ActionGroup[];
  busy: boolean;
  onAction: (action: string) => void;
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
          onAction(primary.action);
          return;
        }
      }

      // Single-letter shortcuts
      if (key.length === 1) {
        const match = allBindings.find((b) => b.key === key);
        if (match) {
          e.preventDefault();
          onAction(match.action);
        }
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [groups, isBusy, onAction]);

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
                  onClick={() => onAction(binding.action)}
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
    <footer
      class="progress-bar"
      title="View progress"
      role="button"
      tabIndex={0}
      onClick={onClick}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onClick();
        }
      }}
    >
      <span class="pb-overall">
        {stats.completedActivities}/{stats.totalActivities} ({pct}%)
      </span>
      {currentUnit ? (
        <>
          <span class="pb-kata-label pb-active">{currentUnit.title}</span>
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
        <span class="pb-kata-label pb-active">{currentPosition.unitId}</span>
      ) : null}
    </footer>
  );
}

// ─── Init ───

render(<App />, document.body);
