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
  InstanceNote,
  ManagerInstance,
  Settings,
  SkippedVersion,
  Snapshot,
  UpdateBlocked,
  UpdateCandidate,
} from "./types";
import { canWrite, isAvailable } from "./sources";
import { artifactKeyId } from "../store/ui";

/**
 * Why a listed update is or is not offered. The order of the checks in
 * `updateStateOf` is the order the Updates page's status chip has always
 * gone by:
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

/** One string per skipped version of one package: the Settings page's list
 *  keys and removes entries by it, and `hidingRule` looks skips up by it. */
export function skippedVersionId(skipped: SkippedVersion): string {
  return `${artifactKeyId(skipped.key)}|${skipped.version}`;
}

/**
 * Whether "Skip this version" can do on `candidate`'s row what its hint
 * says -- hide this update until the source offers another version -- and
 * so whether the Updates page offers it there and `hidingRule` lets a
 * skip hide the row. It can when `target` names one release. Two kinds of
 * row have a `target` that does not:
 *
 * - One Canager could not check: its `target` is its installed version
 *   (`uncheckable_candidate` in crates/canager-core/src/adapters/mod.rs),
 *   not a version any source offered.
 * - A Homebrew cask declared `version :latest`. The `brew outdated
 *   --json=v2` Canager runs lists one only when it is greedy about that
 *   cask -- given `--greedy`, which Canager passes while Settings' Include
 *   self-updating apps is on, or set to be by Homebrew's own
 *   HOMEBREW_UPGRADE_GREEDY or HOMEBREW_UPGRADE_GREEDY_CASKS -- and then
 *   whenever it takes the cask's download to have changed
 *   (`Cask#outdated_version`). The version it offers is the cask's own,
 *   "latest", for every release (`Cask#outdated_info`), so a skip of
 *   "latest" would hide each later release as well and never end: Never
 *   remind me, behind a hint that promises a reminder. Homebrew tells such
 *   a version by this same string (`Cask::DSL::Version#latest?`), and so
 *   does `reconcile` in crates/canager-core/src/adapters/brew/mod.rs. This
 *   goes by the version offered, not the one installed: a copy installed
 *   while its cask still had numbered versions is named by that version,
 *   and is offered "latest" all the same.
 *
 * Such a row gets only "Never remind me", which says that it lasts. Nothing
 * here compares `target` with `current`. Homebrew lists an unpinned formula
 * whose installed keg is its current version, but neither linked nor
 * opt-linked, with the two alike (`Formula#outdated_kegs`), and its next
 * release has another number, so a skip of it ends as promised. An Ollama
 * model's two are digests from different hash spaces, never to be compared
 * (`check_one_model` in crates/canager-core/src/adapters/ollama/mod.rs).
 */
export function canSkipVersion(candidate: UpdateCandidate): boolean {
  if (!candidate.checkable) return false;
  return !(candidate.key.kind === "Cask" && candidate.target === "latest");
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
 * with like. A skip never hides a row whose `target` does not name one
 * release -- one Canager could not check, or a Homebrew cask declared
 * `version :latest` (`canSkipVersion`) -- and the Updates page offers no
 * Skip this version on such a row.
 */
export function hidingRule(
  settings: HidingSettings,
): (candidate: UpdateCandidate) => HiddenBy | null {
  const ignoredIds = new Set(settings.ignored_updates.map(artifactKeyId));
  const skippedIds = new Set(settings.skipped_versions.map(skippedVersionId));
  return (candidate) => {
    if (ignoredIds.has(artifactKeyId(candidate.key))) return "ignored";
    if (
      canSkipVersion(candidate) &&
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
 * The updates Canager can install from the Updates page right now: every
 * one it lists (`notHidden`) whose row has an Update button and a checkbox
 * (`isUpdateActionable`, against the instance its key names). This is the
 * Updates page's "N updates", the rows its Select all and Update all tick,
 * and the count on the sidebar's Updates entry -- one function, so the
 * badge cannot promise a row the page does not offer.
 */
export function actionableUpdatesOf(
  snapshot: Pick<Snapshot, "instances" | "updates">,
  settings: HidingSettings,
): UpdateCandidate[] {
  const instancesById = new Map(snapshot.instances.map((instance) => [instance.id, instance]));
  return notHidden(snapshot.updates, settings).filter((candidate) =>
    isUpdateActionable(candidate, instancesById.get(candidate.key.instance_id)),
  );
}

/**
 * Whether each note means Canager could not fully check this source for
 * updates this time, so finding none there is not news that there are
 * none: Homebrew's list of software could not be downloaded
 * (`IndexMayBeStale`), so its updates were checked against a copy of that
 * list that may be out of date; it is still downloading (`IndexUpdating`),
 * so they were not checked this time at all; or the launcher is left
 * without its program (`LauncherOnly`), so there is no installed version
 * to check. The four PATH notes are about what runs when the tool's name
 * is typed in Terminal, not about the check. Read by
 * `everySourceChecked`. A `Record`, so a note added to `InstanceNote`
 * without an answer here fails `tsc`.
 */
const NOTE_LEAVES_UPDATES_UNCHECKED: Record<InstanceNote, boolean> = {
  IndexMayBeStale: true,
  IndexUpdating: true,
  NotOnPath: false,
  ShadowedByHomebrew: false,
  ShadowedByNpm: false,
  ShadowedByOther: false,
  LauncherOnly: true,
};

/**
 * Whether every source answered the last check and was checked for
 * updates in full, so that finding no update anywhere means there is
 * none. What the Updates page asks before it says "Everything is up to
 * date" rather than "No updates in the sources Canager could check", and
 * what `updatesSummary` asks before the Overview says it. A read-only
 * source is one Canager *can* check.
 */
export function everySourceChecked(instances: ManagerInstance[]): boolean {
  return instances.every(
    (instance) =>
      isAvailable(instance) &&
      !instance.status.notes.some((note) => NOTE_LEAVES_UPDATES_UNCHECKED[note]),
  );
}

/**
 * The Overview's headline, as the Updates page would put it:
 *
 * - `updates`: there are updates it offers to install
 *   (`actionableUpdatesOf`) -- the rows "Review updates" selects.
 * - `upToDate`: no update listed at all and every source checked in full
 *   (`everySourceChecked`) -- exactly when the Updates page says
 *   "Everything is up to date".
 * - `nothingToUpdate`: none to install, and not that either. Some are
 *   listed that Canager cannot install (pinned, read-only, not checkable,
 *   from a source not answering), the user hid the rest, or a source was
 *   not checked in full. Calling that up to date is the lie the Updates
 *   page stopped telling; the Overview does not start.
 */
export type UpdatesSummary =
  | { kind: "updates"; actionable: UpdateCandidate[] }
  | { kind: "upToDate" }
  | { kind: "nothingToUpdate" };

export function updatesSummary(
  snapshot: Pick<Snapshot, "instances" | "updates">,
  settings: HidingSettings,
): UpdatesSummary {
  const actionable = actionableUpdatesOf(snapshot, settings);
  if (actionable.length > 0) return { kind: "updates", actionable };
  if (snapshot.updates.length === 0 && everySourceChecked(snapshot.instances)) {
    return { kind: "upToDate" };
  }
  return { kind: "nothingToUpdate" };
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
