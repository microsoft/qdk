// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import type {
  IAIProvider,
  AIHintContext,
  AIErrorContext,
  AIReviewContext,
  AIQuestionContext,
} from "../server/types.js";

interface ChatMessage {
  role: "system" | "user" | "assistant";
  content: string;
}

/**
 * AI provider that uses MCP sampling (`sampling/createMessage`) to call back
 * into the host's model. No API key needed — the host (e.g. Claude) handles
 * inference. Returns `null` gracefully if the host doesn't support sampling.
 */
export class MCPSamplingAIProvider implements IAIProvider {
  constructor(private mcpServer: McpServer) {}

  async getHint(ctx: AIHintContext): Promise<string | null> {
    const levelDesc = [
      "Give a gentle conceptual nudge — point the student toward the right idea without being specific about code.",
      "Give more specific algorithmic guidance — describe the approach or gate sequence to consider, but don't write code.",
      "Give a targeted hint about which Q# operations or patterns to use — be specific but still don't write the full solution.",
    ];
    const system = `You are a quantum computing tutor helping a student learn Q# through interactive exercises.
The student is stuck on an exercise. Give a hint without revealing the full solution.
Hint level: ${ctx.hintLevel}/3
${levelDesc[Math.min(ctx.hintLevel - 1, 2)]}
Keep your response concise (2-4 sentences). Never show complete code solutions.`;

    let userMsg = `## Exercise\n${ctx.exerciseDescription}\n\n## Student's Current Code\n\`\`\`qsharp\n${ctx.userCode}\n\`\`\``;
    if (ctx.checkResult?.error) {
      userMsg += `\n\n## Last Check Result\nError: ${ctx.checkResult.error}`;
    }
    if (ctx.previousHints.length > 0) {
      userMsg += `\n\n## Previous Hints Given\n${ctx.previousHints.map((h, i) => `${i + 1}. ${h}`).join("\n")}`;
    }

    return this.sample([
      { role: "system", content: system },
      { role: "user", content: userMsg },
    ]);
  }

  async explainError(ctx: AIErrorContext): Promise<string | null> {
    const system = `You are a Q# compiler error explainer for quantum computing beginners.
Explain the error in plain English. Relate it to quantum computing concepts when relevant.
Keep your explanation concise (2-3 sentences). Be helpful and encouraging.`;

    let userMsg = `## Code\n\`\`\`qsharp\n${ctx.code}\n\`\`\`\n\n## Error\n${ctx.error}`;
    if (ctx.exerciseDescription) {
      userMsg += `\n\n## Exercise Context\n${ctx.exerciseDescription}`;
    }

    return this.sample([
      { role: "system", content: system },
      { role: "user", content: userMsg },
    ]);
  }

  async reviewSolution(ctx: AIReviewContext): Promise<string | null> {
    const system = `You are a quantum computing code reviewer. The student has successfully solved an exercise.
Both their solution and the reference solution are correct. In 3-5 sentences:
- Note what the student did well
- If their approach differs from the reference, explain the difference
- Mention any deeper quantum concepts illustrated by the solution
Be positive and educational.`;

    const userMsg = `## Exercise\n${ctx.exerciseDescription}\n\n## Student's Solution\n\`\`\`qsharp\n${ctx.userCode}\n\`\`\`\n\n## Reference Solution\n\`\`\`qsharp\n${ctx.referenceSolution}\n\`\`\``;

    return this.sample([
      { role: "system", content: system },
      { role: "user", content: userMsg },
    ]);
  }

  async askQuestion(ctx: AIQuestionContext): Promise<string | null> {
    const system = `You are a quantum computing educator helping a student who is working through Q# katas (interactive exercises).
They are currently studying: "${ctx.kataTitle}".
Answer their question using the provided lesson content as context.
If the question goes beyond the current lesson, give a brief answer and mention which topics might cover it in more depth.
Keep answers focused, educational, and concise.`;

    const messages: ChatMessage[] = [{ role: "system", content: system }];

    if (ctx.history) {
      const recent = ctx.history.slice(-8);
      for (const h of recent) {
        messages.push({ role: h.role, content: h.content });
      }
    }

    let lessonContent = ctx.lessonContent;
    if (lessonContent.length > 4000) {
      lessonContent = lessonContent.slice(0, 4000) + "\n...(truncated)";
    }

    messages.push({
      role: "user",
      content: `## Current Lesson Content\n${lessonContent}\n\n## My Question\n${ctx.question}`,
    });

    return this.sample(messages);
  }

  /**
   * Send messages to the host via sampling. Extracts the system message into
   * `systemPrompt`, converts the rest to user/assistant roles (sampling spec
   * does not allow `system` in the messages array).
   */
  private async sample(messages: ChatMessage[]): Promise<string | null> {
    const sys = messages.find((m) => m.role === "system")?.content;
    const convo = messages
      .filter((m) => m.role !== "system")
      .map((m) => ({
        role: m.role as "user" | "assistant",
        content: { type: "text" as const, text: m.content },
      }));

    process.stderr.write(
      `[katas-mcp] sampling request: ${convo.length} msgs, system=${sys ? sys.length : 0} chars\n`,
    );
    try {
      const result = await this.mcpServer.server.createMessage({
        messages: convo,
        maxTokens: 600,
        systemPrompt: sys,
        temperature: 0.7,
      });
      process.stderr.write(
        `[katas-mcp] sampling response: type=${result.content.type}\n`,
      );
      if (result.content.type === "text") {
        return result.content.text;
      }
      return null;
    } catch (err) {
      // Host doesn't support sampling, or the user rejected it.
      process.stderr.write(
        `[katas-mcp] sampling failed: ${err instanceof Error ? err.message : String(err)}\n`,
      );
      return null;
    }
  }
}
