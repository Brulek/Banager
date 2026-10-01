import type { Measured, OpRequest, Plan, Warning } from "../lib/types";
import { mockFamilyOf } from "./mockFamilies";

/**
 * The preview's stand-in for what `Session::issue_plan` adds to an
 * uninstall preview (crates/banager-core/src/session/kept.rs): the folders
 * a tool's family keeps its data in, that the pretend Mac has, with about
 * how much each takes -- and Ollama's models folder for Homebrew's
 * `ollama`. As in Rust, only for an uninstall, and never a path the plan
 * already names: the standalone Claude Code's own list keeps `~/.claude`
 * and `~/.claude.json` (./mockPlans.ts), so its dialog shows them there.
 */

const MB = 1_000_000;
const GB = 1_000_000_000;

function about(bytes: number): Measured {
  return { bytes: Math.round(bytes), partial: false, at_least: false };
}

/** What the pretend Mac has, by family, in the table's order. */
const KEPT: Record<string, { path: string; what: "ToolData" | "Models"; size: Measured | null }[]> = {
  "claude-code": [
    { path: "~/.claude", what: "ToolData", size: about(412.3 * MB) },
    { path: "~/.claude.json", what: "ToolData", size: about(0.05 * MB) },
  ],
  codex: [{ path: "~/.codex", what: "ToolData", size: about(38.4 * MB) }],
  // Its folder leads into ~/Documents, which is never measured: the path alone.
  "gemini-cli": [{ path: "~/.gemini", what: "ToolData", size: null }],
  // The models folder: the layers Ollama's own sizes count (./mockSizes.ts) and their manifests.
  ollama: [{ path: "~/.ollama/models", what: "Models", size: about(6.62 * GB + 0.2 * MB) }],
  // Its sessions, logins and logs, then its settings (opencode's own docs).
  opencode: [
    { path: "~/.local/share/opencode", what: "ToolData", size: about(21.7 * MB) },
    { path: "~/.config/opencode", what: "ToolData", size: about(0.01 * MB) },
  ],
};

/** `plan`, with a `KeepsData` for each path the tool `request` uninstalls leaves behind. */
export function withMockKeptData(
  plan: Plan,
  adapterId: string,
  request: OpRequest,
  ownCodexInstall = false,
): Plan {
  if (request.kind !== "Uninstall") return plan;
  const family = mockFamilyOf(adapterId, {
    instance_id: request.instance_id,
    kind: request.artifact_kind,
    name: request.name,
  });
  const kept = family === null ? [] : (KEPT[family] ?? []);
  if (kept.length === 0) return plan;
  const named = new Set(
    plan.warnings.flatMap((warning) => {
      if (typeof warning === "string") return [];
      if ("WillKeep" in warning) return [warning.WillKeep.path];
      if ("WillTrash" in warning) return [warning.WillTrash.path];
      if ("AlreadyGone" in warning) return [warning.AlreadyGone.path];
      return [];
    }),
  );
  const added: Warning[] = kept
    .filter((item) => !named.has(item.path))
    // ~/.codex with Codex's own install in it: Rust leaves that folder out
    // of the size (`kept_data::LEFT_OUT`), and says so.
    .map((item) => ({
      KeepsData: {
        ...item,
        left_out: item.path === "~/.codex" && ownCodexInstall ? ["~/.codex/packages/standalone"] : [],
      },
    }));
  return { ...plan, warnings: [...plan.warnings, ...added] };
}
