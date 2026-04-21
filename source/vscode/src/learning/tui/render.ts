// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { Marked } from "marked";
import { markedTerminal } from "marked-terminal";
import type {
  NavigationItem,
  RunResult,
  SolutionCheckResult,
  CircuitResult,
  EstimateResult,
  OverallProgress,
  KataSummary,
  DumpInfo,
  MatrixInfo,
  HintResult,
} from "../server/index.js";

// ─── Simple ANSI helpers ───

const ESC = "\x1b[";
const bold = (s: string) => `${ESC}1m${s}${ESC}0m`;
const dim = (s: string) => `${ESC}2m${s}${ESC}0m`;
const underline = (s: string) => `${ESC}4m${s}${ESC}0m`;

// ─── Markdown rendering ───

const marked = new Marked(markedTerminal());

/** Render markdown content to styled terminal text */
export function renderMarkdown(md: string): string {
  // Strip any inline HTML (SVGs, style blocks) that leaked from the source
  const cleaned = md
    .replace(/<style[\s\S]*?<\/style>/gi, "")
    .replace(/<svg[\s\S]*?<\/svg>/gi, "")
    .replace(/<table[\s\S]*?<\/table>/gi, "")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
  try {
    return marked.parse(cleaned) as string;
  } catch {
    return cleaned;
  }
}

/** Render Q# code with a simple code block */
export function renderCode(code: string): string {
  const border = dim("─".repeat(60));
  return `${border}\n${code}\n${border}`;
}

// ─── Navigation item rendering ───

export function renderNavigationItem(item: NavigationItem): string {
  const lines: string[] = [];

  switch (item.type) {
    case "lesson-text": {
      lines.push(bold(`📖 ${item.sectionTitle}`));
      lines.push("");
      lines.push(renderMarkdown(item.content));
      break;
    }
    case "lesson-example": {
      lines.push(bold(`💡 Example — ${item.sectionTitle}`));
      lines.push("");
      lines.push(renderCode(item.code));
      break;
    }
    case "lesson-question": {
      lines.push(bold(`❓ Question — ${item.sectionTitle}`));
      lines.push("");
      lines.push(renderMarkdown(item.description));
      break;
    }
    case "exercise": {
      const status = item.isComplete ? "✅ Complete" : "⬚ Not solved";
      lines.push(bold(`🏋️ Exercise: ${item.title}`));
      lines.push(status);
      lines.push("");
      lines.push(renderMarkdown(item.description));
      lines.push("");
      lines.push(dim(`📄 Solution file: ${underline(item.filePath)}`));
      break;
    }
  }

  return lines.join("\n");
}

// ─── Results rendering ───

export function renderRunResult(result: RunResult): string {
  const lines: string[] = [];

  if (result.success) {
    lines.push("✅ Execution succeeded");
  } else {
    lines.push("❌ Execution failed");
  }

  for (const event of result.events) {
    switch (event.type) {
      case "message":
        lines.push(`  ${event.message}`);
        break;
      case "dump":
        lines.push(renderQuantumState(event.dump));
        break;
      case "matrix":
        lines.push(renderMatrix(event.matrix));
        break;
    }
  }

  if (result.result) {
    lines.push(`Result: ${bold(result.result)}`);
  }

  if (result.error) {
    lines.push(`Error: ${result.error}`);
  }

  return lines.join("\n");
}

export function renderSolutionCheck(result: SolutionCheckResult): string {
  const lines: string[] = [];

  if (result.passed) {
    lines.push(bold("✅ Solution correct! Well done!"));
  } else {
    lines.push(bold("❌ Solution incorrect."));
  }

  for (const event of result.events) {
    switch (event.type) {
      case "message":
        lines.push(`  ${event.message}`);
        break;
      case "dump":
        lines.push(renderQuantumState(event.dump));
        break;
      case "matrix":
        lines.push(renderMatrix(event.matrix));
        break;
    }
  }

  if (result.error) {
    lines.push(`  ${result.error}`);
  }

  return lines.join("\n");
}

export function renderCircuit(result: CircuitResult): string {
  const lines: string[] = [];
  lines.push(bold("⚡ Quantum Circuit"));
  lines.push("");
  lines.push(result.ascii);
  return lines.join("\n");
}

export function renderEstimate(result: EstimateResult): string {
  const lines: string[] = [];
  lines.push(bold("📊 Resource Estimate"));
  lines.push("");
  lines.push(`  Physical qubits: ${bold(String(result.physicalQubits))}`);
  lines.push(`  Runtime:         ${bold(result.runtime)}`);
  return lines.join("\n");
}

export function renderQuantumState(dump: DumpInfo): string {
  const lines: string[] = [];
  lines.push(bold("Basis State\t\tAmplitude\t\tProbability"));
  lines.push("─".repeat(60));

  for (const [label, [real, imag]] of Object.entries(dump.state)) {
    const probability = real * real + imag * imag;
    const ampStr =
      imag === 0
        ? real.toFixed(4)
        : `${real.toFixed(4)} ${imag >= 0 ? "+" : "-"} ${Math.abs(imag).toFixed(4)}i`;
    lines.push(
      `|${label}⟩\t\t\t${ampStr}\t\t${(probability * 100).toFixed(2)}%`,
    );
  }

  return lines.join("\n");
}

export function renderMatrix(info: MatrixInfo): string {
  const lines: string[] = [];
  const rows = info.matrix;
  const n = rows.length;

  // Format each complex number
  const format = (real: number, imag: number): string => {
    if (imag === 0) return real.toFixed(4).padStart(10);
    const sign = imag >= 0 ? "+" : "-";
    return `${real.toFixed(4)} ${sign} ${Math.abs(imag).toFixed(4)}i`.padStart(
      16,
    );
  };

  lines.push(bold("Matrix"));
  for (let r = 0; r < n; r++) {
    const cells = rows[r].map(([re, im]) => format(re, im));
    lines.push(`  │ ${cells.join("  ")} │`);
  }

  return lines.join("\n");
}

// ─── Progress rendering ───

export function renderProgress(progress: OverallProgress): string {
  const lines: string[] = [];
  const { totalSections, completedSections } = progress.stats;
  const pct =
    totalSections > 0
      ? Math.round((completedSections / totalSections) * 100)
      : 0;

  lines.push(
    bold(
      `Overall Progress: ${completedSections}/${totalSections} sections (${pct}%)`,
    ),
  );
  lines.push(renderProgressBar(completedSections, totalSections, 40));
  lines.push("");

  for (const [kataId, kp] of progress.katas) {
    const kataPct =
      kp.total > 0 ? Math.round((kp.completed / kp.total) * 100) : 0;
    const status =
      kp.completed === kp.total ? "✅" : kp.completed > 0 ? "🔶" : "⬜";
    lines.push(
      `  ${status} ${kataId}: ${kp.completed}/${kp.total} (${kataPct}%)  ${renderProgressBar(kp.completed, kp.total, 20)}`,
    );
  }

  return lines.join("\n");
}

export function renderKataList(katas: KataSummary[]): string {
  const lines: string[] = [];
  for (let i = 0; i < katas.length; i++) {
    const k = katas[i];
    const marker = k.recommended
      ? "→"
      : k.completedCount === k.sectionCount
        ? "✅"
        : " ";
    const progress = `${k.completedCount}/${k.sectionCount}`;
    lines.push(
      `  ${marker} ${bold((i + 1).toString().padStart(2))}. ${k.title.padEnd(30)} ${dim(progress)}`,
    );
  }
  return lines.join("\n");
}

function renderProgressBar(
  completed: number,
  total: number,
  width: number,
): string {
  if (total === 0) return "░".repeat(width);
  const filled = Math.round((completed / total) * width);
  return "█".repeat(filled) + "░".repeat(width - filled);
}

// ─── Hint rendering ───

export function renderHint(hint: HintResult): string {
  const lines: string[] = [];
  lines.push(bold(`💡 Hint ${hint.current}/${hint.total}`));
  lines.push("");
  lines.push(renderMarkdown(hint.hint));
  return lines.join("\n");
}

// ─── Answer reveal ───

export function renderAnswer(answer: string): string {
  const lines: string[] = [];
  lines.push(bold("📝 Answer"));
  lines.push("");
  lines.push(renderMarkdown(answer));
  return lines.join("\n");
}

// ─── Welcome banner ───

export function renderWelcome(): string {
  const lines = [
    "",
    bold("  ╔═══════════════════════════════════════════╗"),
    bold("  ║         Q# Quantum Katas                  ║"),
    bold("  ║  Interactive quantum computing exercises   ║"),
    bold("  ╚═══════════════════════════════════════════╝"),
    "",
    dim("  Navigate through lessons, examples, and exercises"),
    dim("  to learn quantum computing with Q#."),
    "",
  ];
  return lines.join("\n");
}
