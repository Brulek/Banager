/**
 * What Banager can do about one update candidate, and whether the user
 * has hidden it, decided in one place so the Updates page and the
 * Installed page cannot disagree. The Installed page's rows used to call
 * every entry in `snapshot.updates` "Update available" -- a pinned
 * package, a package Banager could not check, one the user had ignored --
 * and so promised updates the Updates page, which applied four more
 * conditions, did not offer.
 */
import type {
  InstalledArtifact,
  InstanceNote,
  ManagerInstance,
  Settings,
  SkippedVersion,
  SnoozedUpdate,
  Snapshot,
  SourceError,
  Unavailable,
  UpdateBlocked,
  UpdateCandidate,
} from "./types";
import { adapterIdOf, canWrite, isAvailable } from "./sources";
import { artifactKeyId } from "../store/ui";
import { updatesUnchecked } from "./uncheckedStandalone";
import { unusedCopies } from "./commands";

/**
 * Why a listed update is or is not offered. The order of the checks in
 * `updateStateOf` is the order the Updates page's status chip has always
 * gone by:
 * what is true of the whole source first ("View only" holds whatever the
 * next refresh finds), then "could not check" (without a check there is no
 * update to block), then the package's own refusal, then a source that is
 * not answering right now.
 */
export type UpdateState =
  | { kind: "actionable" }
  /** The source refuses every operation (`ManagerInstance.read_only_reason`). */
  | { kind: "readOnly" }
  /** `checkable: false`: Banager could not establish the remote version. */
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
 * the source, `blocked_upgrade` in crates/banager-core/src/session/plans.rs
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
 * `Settings.ignored_updates`, every version), "Remind Me in 30 Days" on it
 * (`snoozed`: `Settings.snoozed_updates`, every version until its date),
 * or "Skip this version" on the version it offers (`skipped`:
 * `Settings.skipped_versions`, that version only).
 */
export type HiddenBy = "ignored" | "snoozed" | "skipped";

/** The lists in `Settings` that hide an update. */
export type HidingSettings = Pick<Settings, "ignored_updates" | "skipped_versions" | "snoozed_updates">;

/** How long "Remind Me in 30 Days" hides an update. */
export const SNOOZE_DAYS = 30;
const DAY_MS = 24 * 60 * 60 * 1000;

/**
 * The snoozes in `settings` still running at `nowMs` -- `until` ahead of
 * the clock -- in the order they were made. One that has run out hides
 * nothing; Rust drops it as it loads the settings, and the page leaves it
 * out until then.
 */
export function activeSnoozes(settings: Pick<Settings, "snoozed_updates">, nowMs: number = Date.now()): SnoozedUpdate[] {
  return (settings.snoozed_updates ?? []).filter((snoozed) => snoozed.until * 1000 > nowMs);
}

/**
 * `snoozed` once "Remind Me in 30 Days" is pressed on `candidate`'s row
 * at `nowMs`: its package hidden until 30 days on (`SNOOZE_DAYS`), in
 * place of any earlier snooze of it, and with every snooze that has run
 * out dropped.
 */
export function withSnoozed(
  snoozed: SnoozedUpdate[] | undefined,
  candidate: UpdateCandidate,
  nowMs: number = Date.now(),
): SnoozedUpdate[] {
  const id = artifactKeyId(candidate.key);
  return [
    ...activeSnoozes({ snoozed_updates: snoozed }, nowMs).filter((s) => artifactKeyId(s.key) !== id),
    { key: candidate.key, until: Math.floor((nowMs + SNOOZE_DAYS * DAY_MS) / 1000) },
  ];
}

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
 * - One Banager could not check: its `target` is its installed version
 *   (`uncheckable_candidate` in crates/banager-core/src/adapters/mod.rs),
 *   not a version any source offered.
 * - A Homebrew cask declared `version :latest`. The `brew outdated
 *   --json=v2` Banager runs lists one only when it is greedy about that
 *   cask -- given `--greedy`, which Banager passes while Settings' Include
 *   self-updating apps is on, or set to be by Homebrew's own
 *   HOMEBREW_UPGRADE_GREEDY or HOMEBREW_UPGRADE_GREEDY_CASKS -- and then
 *   whenever it takes the cask's download to have changed
 *   (`Cask#outdated_version`). The version it offers is the cask's own,
 *   "latest", for every release (`Cask#outdated_info`), so a skip of
 *   "latest" would hide each later release as well and never end: Never
 *   remind me, behind a hint that promises a reminder. Homebrew tells such
 *   a version by this same string (`Cask::DSL::Version#latest?`), and so
 *   does `reconcile` in crates/banager-core/src/adapters/brew/mod.rs. This
 *   goes by the version offered, not the one installed: a copy installed
 *   while its cask still had numbered versions is named by that version,
 *   and is offered "latest" all the same.
 *
 * Such a row gets only "Never remind me", which says that it lasts. Nothing
 * here compares `target` with `current`. Homebrew lists an unpinned formula
 * whose installed keg is its current version, but neither linked nor
 * opt-linked, with the two alike (`Formula#outdated_kegs`), and its next
 * release has another number, so a skip of it ends as promised. An Ollama
 * model's two are the digests of two manifests, never to be compared
 * (`check_one_model` in crates/banager-core/src/adapters/ollama/mod.rs).
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
 * Update selected), the Installed page's chips directly -- so neither can
 * offer an update the other hides. It builds its two lookups once and
 * returns the check to run per candidate, so a long list is not rescanned
 * for each row.
 *
 * "Never remind me" comes first: it holds for every version, so a package
 * that is both reads as ignored. A snooze comes next, while its date is
 * ahead of `nowMs` (`activeSnoozes`): it too holds for every version. A
 * snooze that runs out while the page is open lists its row again at the
 * page's next look -- the next snapshot, or the next change of settings.
 *
 * A skip hides a candidate only while its `target` is the version that
 * was skipped; once the source offers another, the row is listed again.
 * An Ollama model's `target` is a digest, and so is what its skip stored:
 * both are the registry manifest's own (`manifest_digest` in
 * crates/banager-core/src/adapters/ollama/mod.rs), one per republish, so
 * they compare like with like. A skip saved before that -- of the
 * manifest's config digest, which a republish of new weights alone kept --
 * matches no `target` now: its row is listed again, and the old skip
 * stays until the model's next version is skipped (`withSkippedVersion`)
 * or it is removed in Settings. A skip never hides a row whose `target`
 * does not name one release -- one Banager could not check, or a Homebrew
 * cask declared `version :latest` (`canSkipVersion`) -- and the Updates
 * page offers no Skip this version on such a row.
 */
export function hidingRule(
  settings: HidingSettings,
  nowMs: number = Date.now(),
): (candidate: UpdateCandidate) => HiddenBy | null {
  const ignoredIds = new Set(settings.ignored_updates.map(artifactKeyId));
  const snoozedIds = new Set(activeSnoozes(settings, nowMs).map((snoozed) => artifactKeyId(snoozed.key)));
  const skippedIds = new Set(settings.skipped_versions.map(skippedVersionId));
  return (candidate) => {
    if (ignoredIds.has(artifactKeyId(candidate.key))) return "ignored";
    if (snoozedIds.has(artifactKeyId(candidate.key))) return "snoozed";
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
  nowMs: number = Date.now(),
): UpdateCandidate[] {
  const hiddenBy = hidingRule(settings, nowMs);
  return updates.filter((u) => hiddenBy(u) === null);
}

/**
 * The updates Banager can install from the Updates page right now: every
 * one it lists (`notHidden`) whose row has an Update button and a checkbox
 * (`isUpdateActionable`, against the instance its key names). Less the
 * rows an update under way or just finished takes (`holdsRow`), these are
 * the rows that show a checkbox, and what Select all and Invert selection
 * tick. What Update all ticks, and every number of updates, is this less
 * the copies Terminal does not run (`countedUpdatesOf`).
 */
export function actionableUpdatesOf(
  snapshot: Pick<Snapshot, "instances" | "updates">,
  settings: HidingSettings,
  nowMs: number = Date.now(),
): UpdateCandidate[] {
  const instancesById = new Map(snapshot.instances.map((instance) => [instance.id, instance]));
  return notHidden(snapshot.updates, settings, nowMs).filter((candidate) =>
    isUpdateActionable(candidate, instancesById.get(candidate.key.instance_id)),
  );
}

/**
 * The updates every number counts and Update all takes: those Banager can
 * install (`actionableUpdatesOf`) but the update of a copy Terminal does
 * not run (`unusedCopies`: the 「终端用另一份」 row of a tool installed
 * twice). Updating that copy changes nothing the user types, and counting
 * it read as though the copy they use were behind (decision U4, walk 2
 * W2-3), so its row keeps its checkbox, unticked until the user ticks it.
 * A major-version update stays in: it has its own word and Skip This
 * Version. Less the rows an update takes (`holdsRow`), this is the
 * Updates page's "N updates", what its Update all ticks, the count on the
 * sidebar's Updates entry and on the Dock's badge (`useUpdateCount`), what
 * the update notification's report counts and what the Overview's Review
 * Updates selects -- one function, so no two of them can disagree.
 */
export function countedUpdatesOf(
  snapshot: Pick<Snapshot, "instances" | "updates" | "artifacts">,
  settings: HidingSettings,
  nowMs: number = Date.now(),
): UpdateCandidate[] {
  const unused = unusedCopies(snapshot.artifacts);
  return actionableUpdatesOf(snapshot, settings, nowMs).filter(
    (candidate) => !unused.has(artifactKeyId(candidate.key)),
  );
}

/**
 * Whether each note means Banager could not fully check this source for
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
 * Whether `instance` answered the last check and was checked for updates
 * in full: it answered (`isAvailable`), no note says its updates went
 * unchecked (`NOTE_LEAVES_UPDATES_UNCHECKED`), and it is not a source
 * whose updates Banager never checks (`updatesUnchecked`: Codex's own
 * install, whose check lists nothing, so no update there is no news). A
 * read-only source is one Banager *can* check.
 */
export function checkedInFull(instance: ManagerInstance): boolean {
  return (
    isAvailable(instance) &&
    !updatesUnchecked(instance) &&
    !instance.status.notes.some((note) => NOTE_LEAVES_UPDATES_UNCHECKED[note])
  );
}

/**
 * Whether every source answered the last check and was checked for
 * updates in full, so that finding no update anywhere means there is
 * none: no call of this round failed, and every instance answered and was
 * checked in full (`checkedInFull`). What the Updates page asks before it
 * says "Everything is up to date" rather than "No updates in the sources
 * Banager could check", and what `updatesSummary` asks before the
 * Overview says it.
 *
 * Any `SourceError` of this round is enough to fail it, whatever it
 * names. A source whose inventory or update check failed keeps last
 * round's rows and candidates (crates/banager-core/src/session/refresh.rs)
 * while its instance still reads as answering -- so `checkedInFull` alone
 * would call it checked -- and an error that names a bare adapter id
 * (its `detect` failed) or an instance dropped as a duplicate is about a
 * source whose tools may not be on screen at all. None of that was
 * checked this time, so no update listed is no news.
 */
export function everySourceChecked(instances: ManagerInstance[], errors: SourceError[]): boolean {
  return errors.length === 0 && instances.every(checkedInFull);
}

/**
 * Whether a row of `instance` with no update listed may say it is up to
 * date (the Installed page's 「已是最新」): this round's check reached its
 * source in full (`checkedInFull`) and none of its calls failed this
 * round -- `errors` names the instance when its inventory or its update
 * check failed, and `refresh` then carries the last round's rows and
 * candidates forward (crates/banager-core/src/session/refresh.rs), so no
 * update listed is no news. Where this is false the row says nothing
 * about updates, and the source's own notice, or the page's "some checks
 * didn't finish", says why.
 */
export function upToDateIsKnown(instance: ManagerInstance, errors: SourceError[]): boolean {
  return checkedInFull(instance) && !errors.some((error) => error.instance_id === instance.id);
}

/**
 * Of the notes that leave a source's updates unchecked
 * (`NOTE_LEAVES_UPDATES_UNCHECKED`), those after which it was still
 * checked in part: against a copy of Homebrew's list of software that may
 * be out of date (`IndexMayBeStale`). A list still downloading
 * (`IndexUpdating`) and a launcher without its program (`LauncherOnly`)
 * leave nothing checked. A `Record`, so a note added to `InstanceNote`
 * without an answer here fails `tsc`.
 */
const NOTE_LEAVES_UPDATES_CHECKED_IN_PART: Record<InstanceNote, boolean> = {
  IndexMayBeStale: true,
  IndexUpdating: false,
  NotOnPath: false,
  ShadowedByHomebrew: false,
  ShadowedByNpm: false,
  ShadowedByOther: false,
  LauncherOnly: false,
};

/**
 * Which ways of not answering the next check finds the same way, so not
 * news of this one: a Python with no pip (`NoPip`: nothing is broken),
 * and an Ollama at an `https://` address Banager never asks
 * (`HttpsHostRefused`) -- both until the user changes something outside
 * Banager. Not `RefusesAsRoot`: Banager was started with `sudo`, and
 * opening it again normally is what changes it, so 「这次」 holds for this
 * launch. A `Record`, so a variant added to `Unavailable` without an
 * answer here fails `tsc`.
 */
const UNAVAILABLE_EVERY_TIME: Record<Unavailable, boolean> = {
  NotRunning: false,
  NotResponding: false,
  RefusesAsRoot: false,
  HttpsHostRefused: true,
  NoPip: true,
};

/**
 * Of the notes that leave a source's updates unchecked, those the next
 * check finds the same way: a launcher left without its program
 * (`LauncherOnly`), until the user reinstalls or uninstalls it. A list
 * still downloading or out of date is news of this check.
 */
const NOTE_UNCHECKED_EVERY_TIME: Record<InstanceNote, boolean> = {
  IndexMayBeStale: false,
  IndexUpdating: false,
  NotOnPath: false,
  ShadowedByHomebrew: false,
  ShadowedByNpm: false,
  ShadowedByOther: false,
  LauncherOnly: true,
};

/**
 * Whether no check of `instance` would check its updates, so that their
 * going unchecked is so every time, not news of this check: Codex's own
 * install (`updatesUnchecked`), or a state the next check finds the same
 * way (`UNAVAILABLE_EVERY_TIME`, `NOTE_UNCHECKED_EVERY_TIME`). The
 * Overview neither names such a source as not checked this time nor
 * counts it as the rest that was; it is never `checkedInFull`, so it
 * still keeps the plain 「所有工具都是最新的」 away, as the Updates page's
 * "Everything is up to date".
 */
function uncheckedEveryTime(instance: ManagerInstance): boolean {
  const { unavailable, notes } = instance.status;
  return (
    updatesUnchecked(instance) ||
    (unavailable !== null && UNAVAILABLE_EVERY_TIME[unavailable]) ||
    notes.some((note) => NOTE_UNCHECKED_EVERY_TIME[note])
  );
}

/**
 * The sources this check did not check in full this time, for the
 * Overview to name them (decision I22: 「uv这次没检查，其余都是最新的」).
 *
 * - `ids`: what each goes by, each once -- the instances, in the
 *   snapshot's order, that did not answer, whose note says their updates
 *   went unchecked (`NOTE_LEAVES_UPDATES_UNCHECKED`), or that an error of
 *   this round names; then the ids errors name that the snapshot does not
 *   list (a bare adapter id, when its `detect` failed, or an instance
 *   dropped as a duplicate: `failedSourceAdapters` in src/lib/sources.ts).
 * - `partly`: one of them answered and was checked in part -- a step of
 *   its check failed, or it was checked against a list that may be out of
 *   date (`NOTE_LEAVES_UPDATES_CHECKED_IN_PART`) -- so 「未检查完」, not
 *   「没检查」.
 * - `rest`: some other source was checked in full (`checkedInFull`), so
 *   「其余都是最新的」 is about something. A source no check would check
 *   (`uncheckedEveryTime`: Codex's own install, a Python with no pip, an
 *   Ollama at an `https://` address, a launcher without its program) is
 *   neither named nor rest: that is so every time, not news of this
 *   check -- unless an error of this round names it.
 *
 * Null when every source was checked in full this time, but for those
 * Banager never checks.
 */
export interface NotChecked {
  ids: string[];
  partly: boolean;
  rest: boolean;
}

export function notCheckedThisTime(instances: ManagerInstance[], errors: SourceError[]): NotChecked | null {
  const failed = new Set(errors.map((error) => error.instance_id));
  const ids: string[] = [];
  let partly = false;
  let rest = false;
  for (const instance of instances) {
    const notes = instance.status.notes;
    if (uncheckedEveryTime(instance) && !failed.has(instance.id)) continue;
    const unchecked = !isAvailable(instance) || notes.some((note) => NOTE_LEAVES_UPDATES_UNCHECKED[note]);
    if (!unchecked && !failed.has(instance.id)) {
      rest = true;
      continue;
    }
    ids.push(instance.id);
    if (isAvailable(instance) && (failed.has(instance.id) || notes.some((note) => NOTE_LEAVES_UPDATES_CHECKED_IN_PART[note]))) {
      partly = true;
    }
  }
  for (const id of failed) if (!ids.includes(id) && !instances.some((instance) => instance.id === id)) ids.push(id);
  return ids.length === 0 ? null : { ids, partly, rest };
}

/**
 * Whether the source `instanceId` is one `notChecked` names: by its own
 * id, or by the bare adapter id of its kind, which names every source of
 * that kind (one only an error names, `notCheckedThisTime`).
 */
export function namedAsNotChecked(notChecked: NotChecked, instanceId: string): boolean {
  return notChecked.ids.includes(instanceId) || notChecked.ids.includes(adapterIdOf(instanceId));
}

/**
 * Whether Homebrew's update check leaves `artifact` out while Settings'
 * "Show Homebrew apps that have their own updater" (`include_self_updating`) is off: a
 * cask that updates itself (`auto_updates`, from `brew info`'s
 * `auto_updates: true`) or one declared `version :latest`, which Homebrew
 * installs as "latest". Banager passes `--greedy` to `brew outdated
 * --json=v2` only while that switch is on
 * (crates/banager-core/src/adapters/brew/mod.rs), and without it Homebrew
 * lists neither kind whatever version it has, so no update listed is no
 * news: the Installed page's row says nothing about updates rather than
 * 「已是最新」. With the switch on, `--greedy` checks both.
 */
export function leftOutOfUpdateCheck(artifact: InstalledArtifact, includeSelfUpdating: boolean): boolean {
  return (
    !includeSelfUpdating &&
    artifact.key.kind === "Cask" &&
    (artifact.auto_updates || artifact.version === "latest")
  );
}

/**
 * The Overview's headline, as the Updates page would put it:
 *
 * - `updates`: there are updates it counts (`countedUpdatesOf`: not one
 *   of a copy Terminal does not run) whose rows `holdsRow` leaves free --
 *   an update already under way, or one that has just worked, takes its
 *   row (`holdsRow` in src/components/UpdateProgress.tsx) -- the rows
 *   "Review updates" selects and the Updates page's "N updates" counts.
 *   `artifacts` says which copies those are; without it, none is.
 * - `updating`: none left to start, and some are being installed right
 *   now (`underway`: queued, running, being cancelled or read back) --
 *   `count` of them, in the words the Updates page's header uses for them.
 *   Not "Nothing to update" while that page shows them updating. An update
 *   that has finished and waits for the refresh that drops its row is not
 *   counted.
 * - `needsPassword`: none left to start and none under way, and some
 *   stopped where sudo wanted the Mac's password with no way to ask
 *   (`waitsForPassword`) -- `count` of them: rows that still offer their
 *   update, with no checkbox, as Terminal has to finish them. Never
 *   "Nothing to update" over them (walk-4 W4-1), as the Updates page's
 *   headline and the operation bar say 「13个需要输入密码」 of the same
 *   rows.
 * - `upToDate`: none of those, and every source answered this check and
 *   was checked in full this time (`notCheckedThisTime` is null) -- the
 *   Overview's 「都好了」, its green check (decision I22). What the Updates
 *   page lists besides is said under it, not held against it: the user hid
 *   it, Banager cannot update it here, or it is a copy Terminal does not
 *   run. `everything` is whether nothing is listed at all and every
 *   source was checked in full (`everySourceChecked`) -- exactly when the
 *   Updates page says "Everything is up to date". It is false where a
 *   source Banager never checks is there too (Codex's own install), and
 *   where anything is listed: then the headline says that what can be
 *   updated here is up to date, not that everything is.
 * - `nothingToUpdate`: none to install, and a source was not checked in
 *   full this time: `notChecked` names it, for the headline to say
 *   (「uv这次没检查，其余都是最新的」). Calling that up to date is the lie
 *   the Updates page stopped telling; the Overview does not start. Its
 *   group of problems says why, a row each. `everythingElse` is
 *   `everything` of the sources it does not name: none of them has a row
 *   listed -- hidden, can't be updated here, a copy Terminal does not run
 *   -- and each is one Banager checks (not Codex's own install). Where it
 *   is false the headline claims of the rest no more than the all good
 *   would of the same rows: 「其余能在这里更新的都已是最新」. The rows of
 *   a source it names (uv's, kept from its last answer) are its own, said
 *   under the headline and not held against the rest -- a lookup of one
 *   of them that did not succeed included: the Overview weighs only the
 *   rest's (`namedAsNotChecked`).
 *
 * The last three carry what the Overview says under the headline, in the
 * Updates page's own numbers: `cantUpdateHere`, the updates under its
 * "Can't update here (N)" -- every one it lists but those an update is
 * installing or has just installed; `hidden`, the updates it leaves out
 * because the user hid them (`hidingRule`; a skip or a never-remind that
 * hides no update this check found is not counted); and `notUsed`, the
 * rows of a copy Terminal does not run that have a checkbox and that no
 * number counts (decision U4). A tool whose lookup did not succeed is the
 * Overview's to weigh: such a row is among `cantUpdateHere`, and `upToDate`
 * says only that nothing is left to install and every source answered in
 * full -- not that every tool was looked up. Any such row
 * (`unsuccessfulLookupsOf` in src/lib/failedLookups.ts: no answer, or one
 * checking again will not mend, as a certificate Banager does not trust)
 * keeps the Overview's all good and its green check away, though the
 * others checked fine (independent review r6, F5) -- under
 * `nothingToUpdate`, one of a source it does not name: the rows of one it
 * names are that source's own; of them, only those checking again can
 * mend (`failedLookupsOf`) offer Check Again.
 *
 * `updates` and `updating` carry how many wait for the password too
 * (`password`), which the Overview says under its headline.
 */
export type UpdatesSummary =
  | { kind: "updates"; actionable: UpdateCandidate[]; password: number }
  | { kind: "updating"; count: number; password: number }
  | { kind: "needsPassword"; count: number; cantUpdateHere: number; hidden: number; notUsed: number }
  | { kind: "upToDate"; everything: boolean; cantUpdateHere: number; hidden: number; notUsed: number }
  | {
      kind: "nothingToUpdate";
      notChecked: NotChecked;
      everythingElse: boolean;
      cantUpdateHere: number;
      hidden: number;
      notUsed: number;
    };

export function updatesSummary(
  snapshot: Pick<Snapshot, "instances" | "updates" | "errors"> & Partial<Pick<Snapshot, "artifacts">>,
  settings: HidingSettings,
  holdsRow: (candidate: UpdateCandidate) => boolean = () => false,
  underway: (candidate: UpdateCandidate) => boolean = () => false,
  waitsForPassword: (candidate: UpdateCandidate) => boolean = () => false,
): UpdatesSummary {
  const actionable = actionableUpdatesOf(snapshot, settings);
  const password = actionable.filter(waitsForPassword).length;
  const unused = unusedCopies(snapshot.artifacts ?? []);
  const free = actionable.filter((candidate) => !holdsRow(candidate));
  const startable = free.filter((candidate) => !unused.has(artifactKeyId(candidate.key)));
  if (startable.length > 0) return { kind: "updates", actionable: startable, password };
  const updating = actionable.filter(underway).length;
  if (updating > 0) return { kind: "updating", count: updating, password };
  const listed = notHidden(snapshot.updates, settings).length;
  const besides = {
    cantUpdateHere: listed - actionable.length,
    hidden: snapshot.updates.length - listed,
    // None of the free rows is counted by now: each is a copy Terminal does not run.
    notUsed: free.length,
  };
  if (password > 0) return { kind: "needsPassword", count: password, ...besides };
  const notChecked = notCheckedThisTime(snapshot.instances, snapshot.errors);
  if (notChecked !== null) {
    const ofNamed = (instanceId: string) => namedAsNotChecked(notChecked, instanceId);
    const everythingElse =
      snapshot.updates.every((candidate) => ofNamed(candidate.key.instance_id)) &&
      snapshot.instances.every((instance) => ofNamed(instance.id) || checkedInFull(instance));
    return { kind: "nothingToUpdate", notChecked, everythingElse, ...besides };
  }
  // Not yet the all good: a row whose lookup did not succeed is among
  // `cantUpdateHere`, and the Overview weighs it (`unsuccessfulLookupsOf`).
  const everything = snapshot.updates.length === 0 && everySourceChecked(snapshot.instances, snapshot.errors);
  return { kind: "upToDate", everything, ...besides };
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
 * `settings` with what hides `candidate` (`by`, as `hidingRule` answered)
 * taken back: its never-remind, its snooze, or the skip of the version it
 * offers -- the Settings page's 「恢复提醒」 and 「取消跳过」, which the
 * Installed page's inspector offers too.
 */
export function withoutHiding(settings: Settings, by: HiddenBy, candidate: UpdateCandidate): Settings {
  const id = artifactKeyId(candidate.key);
  switch (by) {
    case "ignored":
      return { ...settings, ignored_updates: settings.ignored_updates.filter((key) => artifactKeyId(key) !== id) };
    case "snoozed":
      return {
        ...settings,
        snoozed_updates: (settings.snoozed_updates ?? []).filter((snoozed) => artifactKeyId(snoozed.key) !== id),
      };
    case "skipped": {
      const skip = skippedVersionId({ key: candidate.key, version: candidate.target });
      return { ...settings, skipped_versions: settings.skipped_versions.filter((s) => skippedVersionId(s) !== skip) };
    }
  }
}

/**
 * The version a skip may show the user, or null when it must not be
 * shown. An Ollama model's skipped version is a registry manifest's
 * digest -- the `target` of the `UpdateChannel::Digest` candidate it was
 * skipped from (`check_one_model` in
 * crates/banager-core/src/adapters/ollama/mod.rs, the only producer of
 * `Model` rows) -- and no hash goes in front of this audience. A skip
 * carries no channel, so this goes by the key's kind, as the Installed
 * page does when it leaves a model's digest out of the row's name.
 */
export function shownSkippedVersion(skipped: SkippedVersion): string | null {
  return skipped.key.kind === "Model" ? null : skipped.version;
}
