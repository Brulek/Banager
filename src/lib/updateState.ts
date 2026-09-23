/**
 * What Canager can do about one update candidate, decided in one place so
 * the Updates page and the Installed page cannot disagree. The Installed
 * page's badge used to call every entry in `snapshot.updates` "Update
 * available" -- a pinned package, a package Canager could not check, one
 * the user had ignored -- and so promised updates the Updates page, which
 * applied four more conditions, did not offer.
 */
import type { ArtifactKey, ManagerInstance, UpdateBlocked, UpdateCandidate } from "./types";
import { canWrite, isAvailable } from "./sources";
import { artifactKeyId } from "../store/ui";

/**
 * Why a listed update is or is not offered. The order of the checks in
 * `updateStateOf` is the order the Updates page's badge has always used:
 * what is true of the whole source first ("Read-only" holds whatever the
 * next refresh finds), then "could not check" (without a check there is no
 * update to block), then the package's own refusal, then a source that is
 * not answering right now.
 */
export type UpdateState =
  | { kind: "actionable" }
  /** The source refuses every operation (`ManagerInstance.read_only_reason`). */
  | { kind: "readOnly" }
  /** `checkable: false`: Canager could not establish the remote version. */
  | { kind: "cannotCheck" }
  /** The tool will refuse to update this package (`UpdateCandidate.blocked`). */
  | { kind: "blocked"; reason: UpdateBlocked }
  /** The source did not answer the last refresh, or is not in the snapshot
   *  at all; `Session::issue_plan` refuses both (`NotActionable`,
   *  `SourceGone`). */
  | { kind: "sourceUnavailable" };

/**
 * `candidate`'s state, given the instance its key's `instance_id` names
 * (`undefined` when the snapshot lacks it).
 *
 * `Session::issue_plan` applies the same conditions in Rust (spec §2.5 for
 * the source, `blocked_upgrade` in crates/canager-core/src/session/plans.rs
 * for the package), so a stale snapshot costs an error message, not a
 * wrong command.
 */
export function updateStateOf(
  candidate: UpdateCandidate,
  instance: ManagerInstance | undefined,
): UpdateState {
  if (instance !== undefined && !canWrite(instance)) return { kind: "readOnly" };
  if (!candidate.checkable) return { kind: "cannotCheck" };
  if (candidate.blocked !== null) return { kind: "blocked", reason: candidate.blocked };
  if (instance === undefined || !isAvailable(instance)) return { kind: "sourceUnavailable" };
  return { kind: "actionable" };
}

/** Whether the Updates page offers an Update button and a checkbox for
 *  `candidate`, and whether the Installed page calls it "Update available". */
export function isUpdateActionable(
  candidate: UpdateCandidate,
  instance: ManagerInstance | undefined,
): boolean {
  return updateStateOf(candidate, instance).kind === "actionable";
}

/**
 * The candidates the Updates page lists: every one the user has not
 * ignored (`Settings.ignored_updates`). The Installed page reads the same
 * list, so an ignored update is not "available" there either.
 */
export function notIgnored(
  updates: UpdateCandidate[],
  ignored: ArtifactKey[],
): UpdateCandidate[] {
  const ignoredIds = new Set(ignored.map((k) => artifactKeyId(k)));
  return updates.filter((u) => !ignoredIds.has(artifactKeyId(u.key)));
}
