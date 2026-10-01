/**
 * What the preview's history (`get_history`, src/dev/mockBackend.ts) holds
 * before the page runs anything: a few weeks of a pretend Mac's updates and
 * uninstalls, as earlier launches of Banager kept them in `history.json`
 * (crates/banager-core/src/history/mod.rs) -- two today, so that the
 * Updates page's 「最近更新」 shows both of its date forms, one older than
 * the 30 days the page lists, an uninstall it never lists, and an update
 * that failed (「未能更新：网络连接失败」) and one that changed nothing
 * (「结果不符」), which it lists among the rest. Times count back from when
 * the preview opened.
 */
import { failureCause } from "../lib/failureCause";
import type { ArtifactKey, HistoryRecord, HistoryResult, HistoryView, OpRequest, Outcome } from "../lib/types";
import { IDS, key } from "./mockData";

const MINUTE = 60 * 1000;
const DAY = 24 * 60 * MINUTE;

/** The earlier launches' id: none of the page's operations are theirs. */
const EARLIER = "mock-earlier-launch";

function kept(
  opId: number,
  ago: number,
  artifact: ArtifactKey,
  adapterId: string,
  from: string | null,
  to: string | null,
  fields: Partial<HistoryRecord> = {},
): (now: number) => HistoryRecord {
  return (now) => ({
    run: EARLIER,
    op_id: opId,
    finished_at: now - ago,
    key: artifact,
    display_name: artifact.name,
    adapter_id: adapterId,
    kind: "Update",
    from_version: from,
    to_version: to,
    result: "Succeeded",
    verified: true,
    ...fields,
  });
}

const SEEDED = [
  kept(14, 25 * MINUTE, key(IDS.brew, "Formula", "htop"), "brew", "3.4.0", "3.4.1"),
  kept(13, 70 * MINUTE, key(IDS.brew, "Formula", "ripgrep"), "brew", "15.0.0", "15.1.0"),
  kept(9, 2 * DAY, key(IDS.brew, "Formula", "jq"), "brew", "1.8.2", null, {
    result: { Failed: { cause: "network" } },
    verified: false,
  }),
  kept(8, 3 * DAY, key(IDS.npm, "Package", "prettier"), "npm", "3.8.0", "3.8.1"),
  kept(6, 9 * DAY, key(IDS.brew, "Formula", "wget"), "brew", "1.24.5", "1.25.0"),
  // Taken as done on presence alone: listed as 「已更新」, not 「已核实」.
  kept(4, 16 * DAY, key(IDS.brew, "Formula", "gh"), "brew", "2.100.2", "2.101.0", { verified: false }),
  kept(3, 20 * DAY, key(IDS.pipx, "Tool", "yt-dlp"), "pipx", "2026.8.20", null, { kind: "Uninstall" }),
  kept(1, 40 * DAY, key(IDS.brew, "Formula", "ffmpeg"), "brew", "9.0.0", "9.0.1_1"),
  // npm said it updated, and the version read back had not changed.
  kept(11, 5 * DAY, key(IDS.npm, "Package", "typescript"), "npm", "6.0.2", "6.0.2", {
    result: { NeedsAttention: "UnchangedAfterUpgrade" },
    verified: false,
  }),
];

/** The history the preview opens with, newest first, as `get_history` answers it. */
export function mockHistory(now: number): HistoryView {
  return {
    run: "mock-this-launch",
    cleared_before: null,
    records: SEEDED.map((make) => make(now)).sort((a, b) => b.finished_at - a.finished_at),
  };
}

/** `history::record_for`'s category for an outcome. */
function resultOf(outcome: Outcome): HistoryResult {
  if (outcome === "Succeeded" || outcome === "Unconfirmed" || outcome === "Cancelled") return outcome;
  if ("NeedsAttention" in outcome) return { NeedsAttention: outcome.NeedsAttention };
  if ("Failed" in outcome) return { Failed: { cause: failureCause(outcome.Failed.summary) } };
  const fault = outcome.BanagerFailed;
  return {
    Failed: { cause: typeof fault !== "string" && "HomebrewStillUpdating" in fault ? "homebrewUpdating" : null },
  };
}

/**
 * The record the preview keeps for an operation of its own that ended, as
 * `history::record_for` builds it, or null for one Rust does not keep: an
 * install, or one cancelled before it started.
 */
export function mockRecord(args: {
  run: string;
  opId: number;
  request: OpRequest;
  outcome: Outcome;
  started: boolean;
  displayName: string;
  adapterId: string;
  before: string | null;
  after: string | null;
  now: number;
}): HistoryRecord | null {
  const { request, outcome } = args;
  if (request.kind === "Install") return null;
  if (!args.started && outcome === "Cancelled") return null;
  const update = request.kind === "Upgrade";
  const result = resultOf(outcome);
  const model = request.artifact_kind === "Model";
  const exitedZero = result === "Succeeded" || (typeof result !== "string" && "NeedsAttention" in result);
  return {
    run: args.run,
    op_id: args.opId,
    finished_at: args.now,
    key: { instance_id: request.instance_id, kind: request.artifact_kind, name: request.name },
    display_name: args.displayName,
    adapter_id: args.adapterId,
    kind: update ? "Update" : "Uninstall",
    from_version: model ? null : args.before,
    to_version: model || !update || !exitedZero ? null : args.after,
    result,
    verified: update
      ? result === "Succeeded" && args.before !== null && args.after !== null && args.before !== args.after
      : result === "Succeeded",
  };
}
