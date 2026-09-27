/**
 * What Canager can do about one update candidate, and whether the user
 * has hidden it, decided in one place so the Updates page and the
 * Installed page cannot disagree. The Installed page's badge used to call
 * every entry in `snapshot.updates` "Update available" -- a pinned
 * package, a package Canager could not check, one the user had ignored --
 * and so promised updates the Updates page, which applied four more
 * conditions, did not offer.
 */
import type {
  ManagerInstance,
  Settings,
  SkippedVersion,
  UpdateBlocked,
  UpdateCandidate,
} from "./types";
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
 * Why the Updates page leaves out an update the snapshot has: the user
 * pressed "Never remind me" on its package (`ignored`:
 * `Settings.ignored_updates`, every version), or "Skip this version" on
 * the version it offers (`skipped`: `Settings.skipped_versions`, that
 * version only).
 */
export type HiddenBy = "ignored" | "skipped";

/** The two lists in `Settings` that hide an update. */
export type HidingSettings = Pick<Settings, "ignored_updates" | "skipped_versions">;

/** One string per skipped version of one package. */
function skippedVersionId(skipped: SkippedVersion): string {
  return `${artifactKeyId(skipped.key)}|${skipped.version}`;
}

/**
 * The rule, in one place: why `settings` hides a candidate, or null when
 * the Updates page lists it. Both pages read it -- the Updates page
 * through `notHidden` for everything it lists, counts or selects (its
 * rows, its headline and second line, Select all, Invert selection and
 * Update selected), the Installed page's badge directly -- so neither can
 * offer an update the other hides. It builds its two lookups once and
 * returns the check to run per candidate, so a long list is not rescanned
 * for each row.
 *
 * "Never remind me" comes first: it holds for every version, so a package
 * that is both reads as ignored.
 *
 * A skip hides a candidate only while its `target` is the version that
 * was skipped; once the source offers another, the row is listed again.
 * An Ollama model's `target` is a digest, and so is what its skip stored:
 * both are the registry manifest's config digest, so they compare like
 * with like. A candidate Canager could not check is never hidden by a
 * skip: its `target` is its installed version (`uncheckable_candidate` in
 * crates/canager-core/src/adapters/mod.rs), not a version any source
 * offered, and the Updates page offers no Skip this version on its row.
 */
export function hidingRule(
  settings: HidingSettings,
): (candidate: UpdateCandidate) => HiddenBy | null {
  const ignoredIds = new Set(settings.ignored_updates.map(artifactKeyId));
  const skippedIds = new Set(settings.skipped_versions.map(skippedVersionId));
  return (candidate) => {
    if (ignoredIds.has(artifactKeyId(candidate.key))) return "ignored";
    if (
      candidate.checkable &&
      skippedIds.has(skippedVersionId({ key: candidate.key, version: candidate.target }))
    ) {
      return "skipped";
    }
    return null;
  };
}

/** The candidates the Updates page lists: every one `hidingRule` does not hide. */
export function notHidden(
  updates: UpdateCandidate[],
  settings: HidingSettings,
): UpdateCandidate[] {
  const hiddenBy = hidingRule(settings);
  return updates.filter((u) => hiddenBy(u) === null);
}

/**
 * `skipped` once "Skip this version" is pressed on `candidate`'s row: the
 * version the row offers, recorded in place of any earlier skip of the
 * same package. That earlier one can only be for a version the source no
 * longer offers -- a skip that still matched would have hidden the row --
 * so it could never hide anything again. Nothing else is dropped: a skip
 * whose version the source has moved past stays until the user skips that
 * package's next version or removes it.
 */
export function withSkippedVersion(
  skipped: SkippedVersion[],
  candidate: UpdateCandidate,
): SkippedVersion[] {
  const id = artifactKeyId(candidate.key);
  return [
    ...skipped.filter((s) => artifactKeyId(s.key) !== id),
    { key: candidate.key, version: candidate.target },
  ];
}

/**
 * The version a skip may show the user, or null when it must not be
 * shown. An Ollama model's skipped version is a registry manifest's config
 * digest -- the `target` of the `UpdateChannel::Digest` candidate it was
 * skipped from (`check_one_model` in
 * crates/canager-core/src/adapters/ollama/mod.rs, the only producer of
 * `Model` rows) -- and no hash goes in front of this audience. A skip
 * carries no channel, so this goes by the key's kind, as the Installed
 * page does when it leaves a model's digest out of the row's name.
 */
export function shownSkippedVersion(skipped: SkippedVersion): string | null {
  return skipped.key.kind === "Model" ? null : skipped.version;
}
