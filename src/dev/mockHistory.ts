/**
 * What the preview's history (`get_history`, src/dev/mockBackend.ts) holds
 * before the page runs anything: a few weeks of a pretend Mac's updates and
 * uninstalls, as earlier launches of Banager kept them in `history.json`
 * (crates/banager-core/src/history/mod.rs) -- two today, so that the
 * Updates page's 「最近的更新记录」 shows both of its date forms, one older than
 * the 30 days the page lists, an uninstall it never lists, and an update
 * that failed (「未能更新：网络连接失败」) and one that changed nothing
 * (「显示已更新，但版本没有变化」), which it lists among the rest while the last check still
 * offers each an update. jq's failure is not listed: no update is offered
 * for it any more, as if it had been updated in Terminal since. Times count
 * back from when the preview opened; today's two never reach back past
 * midnight, so they are today's at any hour.
 */
import type {
  AlreadyUpdated,
  FollowUpWarning,
  ArtifactKey,
  HistoryRecord,
  HistoryResult,
  HistoryView,
  OpRequest,
  Outcome,
} from "../lib/types";
import { causeKeepsItsLine, failureDetail, faultFailure, type FailureCause } from "../lib/failureCause";
import { IDS, key } from "./mockData";

const MINUTE = 60 * 1000;
const DAY = 24 * 60 * MINUTE;

/** The earlier launches' id: none of the page's operations are theirs. */
const EARLIER = "mock-earlier-launch";

/**
 * How long before `now` one of today's records finished: `ago`, or `share`
 * of the time since midnight when the day is younger than that, so that it
 * is still today's just after midnight, and still before now. With the
 * smaller `ago` taking the smaller `share`, the newer stays the newer.
 */
function earlierToday(ago: number, share: number): (now: number) => number {
  return (now) => {
    const midnight = new Date(now);
    midnight.setHours(0, 0, 0, 0);
    return Math.min(ago, (now - midnight.getTime()) * share);
  };
}

function kept(
  opId: number,
  ago: number | ((now: number) => number),
  artifact: ArtifactKey,
  adapterId: string,
  from: string | null,
  to: string | null,
  fields: Partial<HistoryRecord> = {},
): (now: number) => HistoryRecord {
  return (now) => ({
    run: EARLIER,
    op_id: opId,
    finished_at: now - (typeof ago === "number" ? ago : ago(now)),
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
  kept(14, earlierToday(25 * MINUTE, 1 / 3), key(IDS.brew, "Formula", "htop"), "brew", "3.4.0", "3.4.1"),
  kept(13, earlierToday(70 * MINUTE, 2 / 3), key(IDS.brew, "Formula", "ripgrep"), "brew", "15.0.0", "15.1.0"),
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
  // Failed, and the last check still offers httpie 3.3.0: listed.
  kept(10, 4 * DAY, key(IDS.pipx, "Tool", "httpie"), "pipx", "3.2.4", null, {
    result: { Failed: { cause: "network" } },
    verified: false,
  }),
  // r6 y3-batch. An Update all yesterday: pcre2 was upgraded as a
  // dependency by the update before it, so its own found it done
  // (「已由前面的更新一并完成」); the cask's app had been moved out of
  // Applications (「未能更新：App已不在原来的位置」); tokei failed in words no
  // cause names, kept as its first error line (「原因：…」 behind the ⓘ).
  // The two failures are listed while the last check offers them.
  kept(21, DAY + 6 * MINUTE, key(IDS.brew, "Formula", "pcre2"), "brew", "10.47", "10.47", {
    verified: false,
    already_updated: "ByEarlierUpdate",
  }),
  kept(22, DAY + 4 * MINUTE, key(IDS.brew, "Cask", "android-platform-tools"), "brew", "36.0.0", null, {
    display_name: "Android SDK Platform-Tools",
    result: { Failed: { cause: "appMissing" } },
    verified: false,
  }),
  kept(23, DAY + 2 * MINUTE, key(IDS.cargo, "Binary", "tokei"), "cargo", "12.1.2", null, {
    result: {
      Failed: {
        cause: null,
        detail: "failed to compile `tokei v13.0.1`, intermediate artifacts can be found at `~/Library/Caches/cargo-install`",
      },
    },
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

/**
 * A failure as `history::record_for` keeps it: its cause, and the tool's
 * first error line where there is none or the cause's words point at it
 * (`causeKeepsItsLine`).
 */
function keptFailure(cause: FailureCause | null, words: string): HistoryResult {
  const detail = cause === null || causeKeepsItsLine(cause) ? failureDetail(words) : null;
  return { Failed: detail === null ? { cause } : { cause, detail } };
}

/** `history::record_for`'s category for an outcome. */
function resultOf(outcome: Outcome): HistoryResult {
  if (outcome === "Succeeded" || outcome === "Unconfirmed" || outcome === "Cancelled") return outcome;
  if ("NeedsAttention" in outcome) return { NeedsAttention: outcome.NeedsAttention };
  if ("Failed" in outcome) return keptFailure(outcome.Failed.cause, outcome.Failed.summary);
  const { cause, detail } = faultFailure(outcome.BanagerFailed);
  return { Failed: detail === null ? { cause } : { cause, detail } };
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
  /** The operation's `already_updated`, kept for one that succeeded. */
  alreadyUpdated?: AlreadyUpdated | null;
  followUpWarnings?: FollowUpWarning[];
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
    ...(args.followUpWarnings?.length ? { follow_up_warnings: args.followUpWarnings } : {}),
    // An update already at its new version did not move it itself: the
    // real readings before and after are the same (the preview moves its
    // row only to show it done).
    verified: update
      ? result === "Succeeded" &&
        !args.alreadyUpdated &&
        args.before !== null &&
        args.after !== null &&
        args.before !== args.after
      : result === "Succeeded",
    ...(result === "Succeeded" && args.alreadyUpdated ? { already_updated: args.alreadyUpdated } : {}),
  };
}
