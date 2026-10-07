import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useCheckAgain, useOperations, useSnapshot, useSettings, useSaveSettings } from "../lib/queries";
import { elapsedSince } from "../lib/format";
import { useUiStore, artifactKeyId } from "../store/ui";
import {
  adapterIdOf,
  adapterLabel,
  instanceLabels,
  settingsSaveSentence,
  installedCountByInstance,
  sourceNoticesFor,
  toolDescription,
  unfinishedChecksNotice,
  UPDATE_BLOCKED_KEYS,
} from "../lib/sources";
import { copyStatusText, useCopyCommand } from "../lib/clipboard";
import { useOperationName } from "../lib/operations";
import { useTranslatedDescription } from "../lib/toolDescriptions";
import { listedName, modelPath, nameKey, namesUnderSeveralSources } from "../lib/names";
import { rankedComparator } from "../lib/sortRank";
import {
  JustUpdated,
  endingOfOutcome,
  endingOfRecord,
  justUpdatedOps,
  type JustUpdatedEntry,
} from "../components/JustUpdated";
import { clearedHere, recentUpdates, useClearHistory, useHistory, verifiedHere } from "../lib/history";
import { NO_HISTORY } from "../lib/types";
import { RowAction, ToolRow } from "../components/ToolRow";
import { StatusChip, type StatusChipProps } from "../components/StatusChip";
import { majorVersionWord } from "../components/majorVersionWord";
import { Menu, type MenuItem } from "../components/ui/Menu";
import { SourceNotices, useNoticeFold } from "../components/SourceNotices";
import { NOTICE_GRID } from "../components/SourceNotice";
import { UpdateConfirmDialog, useUpdateConfirm } from "../components/UpdateConfirm";
import { Refusal } from "../components/SheetParts";
import { VirtualList, type VirtualListHandle } from "../components/VirtualList";
import { ToolbarItems } from "../components/Toolbar";
import { ToolShowButton } from "../components/ToolShowButton";
import { shownBy } from "../lib/families";
import { useRovingRow } from "../components/rovingRows";
import { FirstCheck } from "../components/StatusRing";
import { EmptyState } from "../components/EmptyState";
import { CHECKED_KEYS, elapsedText, useMinuteClock } from "../components/PageHeader";
import {
  holdsRow,
  isRetryable,
  passwordStepsOpId,
  isUnderway,
  progressOf,
  progressWord,
  UpdateProgress,
  useCountedUpdates,
  useStartableUpdates,
  useUpdateOperationFor,
} from "../components/UpdateProgress";
import {
  blockedDetail,
  cannotCheckDetail,
  readOnlyDetail,
  unavailableDetail,
  updateVersionColumn,
} from "../components/updateDetails";
import { FAILURE_CAUSE_KEYS, type FailureCause } from "../lib/failureCause";
import { failedLookupsNotice, isFailedLookup, saysWhyInToolWords, sharedCannotCheckCause } from "../lib/failedLookups";
import { useTwins } from "../components/CommandFacts";
import { notUsedWord } from "../components/TwinAdvice";
import { DisclosureIcon } from "../components/icons";
import { BUTTON } from "../components/ui/controls";
import { focusOrFallback } from "../components/ui/focus";
import type {
  InstanceNote,
  InstalledArtifact,
  ManagerInstance,
  OpSummary,
  Settings,
  UpdateCandidate,
} from "../lib/types";
import {
  actionableUpdatesOf,
  withSnoozed,
  canSkipVersion,
  everySourceChecked,
  notHidden,
  updateStateOf,
  withSkippedVersion,
} from "../lib/updateState";
import type { UpdateState } from "../lib/updateState";

// The virtualizer's first guesses: a row, the "N more can't be updated
// here" line and the line under it, a notice's line, and "Update History"
// -- its heading, then a line a tool. Each slot then measures itself
// through `measureElement`.
const ROW_ESTIMATE = 52;
const SECTION_ESTIMATE = 32;
const SUMMARY_ESTIMATE = 32;
const NOTICE_ESTIMATE = 32;
const JUST_UPDATED_ESTIMATE = 64;
const JUST_UPDATED_LINE_ESTIMATE = 28;

/** No update: what the page starts from until the snapshot and the settings are in. */
const NO_UPDATES: UpdateCandidate[] = [];

/** Whatever `useTranslation()`'s `t` needs here; the same convention as `Translate` in src/lib/sources.ts. */
type Translate = (key: string, options?: Record<string, string | number>) => string;

/**
 * How many rows Update all would take, as the page says it over its list
 * and the window's toolbar under its title (`useUpdatesHeadline`):
 * 「10个可更新」. With none, not "0 updates": the rows under "Can't update
 * here" are real, and simply not Banager's to update. While some are
 * updating, only 「正在更新…」: the operation bar says how many, and both
 * are live regions -- one count heard at a time, not two read over each
 * other (decision I21d). Otherwise, then how many rows of a copy Terminal
 * does not run have a checkbox, which Update all leaves unticked (decision
 * U4): 「1个终端用不到」 -- never "nothing to update" over their Update
 * buttons. After them, how many stopped where
 * sudo wanted the Mac's password (`passwordStepsOpId`) -- rows that still
 * offer their update, with no checkbox, as Terminal has to finish them --
 * 「13个需要输入密码」, never "nothing to update" over them (walk-4 W4-1).
 */
export function updatesHeadline(
  t: Translate,
  updatingCount: number,
  startableCount: number,
  passwordCount = 0,
  notUsedCount = 0,
): string {
  if (updatingCount > 0) return t("updatesMore.updating");
  const parts: string[] = [];
  if (startableCount > 0) parts.push(t("updates.count", { count: startableCount }));
  if (notUsedCount > 0) parts.push(t("notUsedCopy.count", { count: notUsedCount }));
  if (passwordCount > 0) parts.push(t("updates.needPasswordCount", { count: passwordCount }));
  return parts.length === 0 ? t("updates.noneActionable") : parts.join(t("overview.listSeparator"));
}

/**
 * Whether a row's update stopped where sudo wanted the Mac's password
 * with no way to ask (`passwordStepsOpId`): the row keeps its update and
 * the steps for Terminal, and no checkbox.
 */
function waitsForPassword(op: OpSummary | null): boolean {
  return op !== null && passwordStepsOpId(progressOf(op)) !== null;
}

/**
 * The page's headline (`updatesHeadline`) from outside it, for the
 * toolbar's subtitle: the same rows counted the same way -- those Update
 * all would take (`useCountedUpdates`), those of a copy Terminal does not
 * run that have a checkbox all the same, and those an update is
 * installing now (`isUnderway`) -- so the two can never say different
 * numbers, and the first is the sidebar's. Null
 * until the snapshot and the settings are in, and while the page lists
 * nothing at all and says so in a sentence of its own. While the
 * 「显示」 popup shows only the AI coding tools, of those alone, as the
 * page's list and its Update button count them.
 */
export function useUpdatesHeadline(): string | null {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const { data: settings } = useSettings();
  const show = useUiStore((s) => s.updatesShow);
  const operationFor = useUpdateOperationFor();
  const startable = useStartableUpdates();
  const counted = useCountedUpdates();
  const inView = useMemo(() => {
    if (show === "all" || !snapshot) return () => true;
    const byId = new Map(snapshot.artifacts.map((artifact) => [artifactKeyId(artifact.key), artifact]));
    return (candidate: UpdateCandidate) => shownBy(show, byId.get(artifactKeyId(candidate.key)));
  }, [show, snapshot]);
  const listed = useMemo(
    () =>
      snapshot && settings
        ? { any: notHidden(snapshot.updates, settings).length > 0, actionable: actionableUpdatesOf(snapshot, settings) }
        : undefined,
    [snapshot, settings],
  );
  if (listed === undefined || startable === undefined || counted === undefined || !listed.any) return null;
  const updating = listed.actionable.filter(
    (candidate) => inView(candidate) && isUnderway(operationFor(candidate)),
  ).length;
  const shown = counted.filter(inView).length;
  // Rows with a checkbox that Update all leaves unticked: a copy Terminal does not run.
  const notUsed = startable.filter(inView).length - shown;
  const password = listed.actionable.filter(
    (candidate) => inView(candidate) && waitsForPassword(operationFor(candidate)),
  ).length;
  // Of only some: how many of how many, 「5个可更新，共13个」, so the
  // sidebar's 13 beside it does not read as wrong.
  if (show !== "all" && updating === 0 && shown > 0 && shown < counted.length) {
    const parts = [t("clarity.updatesOfAll", { count: shown, total: counted.length })];
    if (notUsed > 0) parts.push(t("notUsedCopy.count", { count: notUsed }));
    if (password > 0) parts.push(t("updates.needPasswordCount", { count: password }));
    return parts.join(t("overview.listSeparator"));
  }
  return updatesHeadline(t, updating, shown, password, notUsed);
}

/**
 * One slot in the virtualized list. The page is one flat list, sorted by
 * name, the way 360's update list is: every row it can update, then the
 * line that discloses the rows it cannot ("5 more can't be updated
 * here"), folded until pressed, as Cork's list folds its unmanaged
 * packages. Each row carries its source in its avatar's mark, and in words
 * where two sources list the same name, in place of the per-source
 * headings the list used to be grouped under. First, while there is
 * anything to say, what the sources had to say about this check -- the
 * list's first row, which scrolls away with it (spec §3.8) -- then the
 * rows, and after them, while there is anything in it, "Update History"
 * (`JustUpdated`): under the updates still to install, as the App Store's
 * is under Pending, or right under the notices when there are none.
 */
type ListItem =
  | { type: "notices"; count: number }
  | { type: "showEmpty" }
  | { type: "justUpdated"; count: number }
  | { type: "update"; candidate: UpdateCandidate; updatable: boolean }
  | { type: "section"; count: number; expanded: boolean }
  | { type: "summary"; count: number; cause: FailureCause | null };

/**
 * A slot's identity: its React key, and the key the virtualizer files the
 * slot's measured height under -- the same string, so a height stays with
 * the row or the toggle it was measured from. Left to its default, the
 * virtualizer keys by position: when an update finished and its row went,
 * every slot below moved up while its React key kept its DOM node, and
 * the heights stayed where they were -- the toggle, now where a row had
 * been, was placed as if it were that row. A moved node is not measured
 * again (its ref does not change, and a ResizeObserver sees no resize), so
 * nothing corrected it. An artifact key id has a `|` in it and the other
 * three do not, so they cannot collide.
 */
function listItemKey(item: ListItem): string {
  switch (item.type) {
    case "notices":
      return "section:notices";
    case "justUpdated":
      return "section:just-updated";
    case "update":
      return artifactKeyId(item.candidate.key);
    case "section":
      return "section:cant-update-here";
    case "summary":
      return "summary:cannot-check";
    case "showEmpty":
      return "section:show-empty";
  }
}

/** A slot's first guess at its height. */
function estimateSize(item: ListItem): number {
  if (item.type === "justUpdated") return JUST_UPDATED_ESTIMATE + item.count * JUST_UPDATED_LINE_ESTIMATE;
  if (item.type === "section") return SECTION_ESTIMATE;
  if (item.type === "summary") return SUMMARY_ESTIMATE;
  if (item.type === "notices") return NOTICE_ESTIMATE;
  return ROW_ESTIMATE;
}

/**
 * The slots ↑ and ↓ move between (`VirtualList`'s `keyboardRows`): the
 * rows, and the line that discloses the rows that can't be updated here.
 */
function keyboardRow(item: ListItem): boolean {
  return item.type === "update" || item.type === "section";
}

/** Where a line's hairline shows (`VirtualList`'s `hairlineBefore`): over a row, and over nothing else. */
function hairlineBefore(next: ListItem): boolean {
  return next.type === "update";
}

/**
 * The line that discloses the rows that can't be updated here, 32 high:
 * a 10pt triangle and the words, muted (spec §3.3; cork-outdated-zh.png),
 * on the rows' grid, as the notices over them are: the triangle centred in
 * the avatars' column, the words where the names start. One of the rows
 * ↑ and ↓ move between, Space or Enter opening it.
 */
function CantUpdateHere({ count, expanded, onToggle }: { count: number; expanded: boolean; onToggle: () => void }) {
  const { t } = useTranslation();
  const roving = useRovingRow();
  return (
    <button
      type="button"
      aria-expanded={expanded}
      onClick={onToggle}
      data-row-focus=""
      tabIndex={roving?.tabIndex}
      onFocus={roving?.onFocus}
      className="relative flex h-8 w-full items-center px-5 text-left text-body text-muted"
    >
      <span data-disclosure-symbol="" className={`flex shrink-0 justify-center ${NOTICE_GRID.checkbox.symbol}`}>
        <DisclosureIcon
          size={10}
          className={`shrink-0 transition-transform motion-reduce:transition-none ${expanded ? "rotate-90" : ""}`}
        />
      </span>
      <span className={`min-w-0 truncate ${NOTICE_GRID.checkbox.gap}`}>
        {t("updates.cantUpdateHere", { number: count })}
      </span>
    </button>
  );
}

/**
 * Whether a slot, once drawn, can be shown again as it was while the page
 * has nothing new to show (`VirtualList`'s `reusable`): all but "Just
 * updated", which reads the clock as it is drawn -- a time today, a date
 * on any other day (`finishedText`).
 */
function reusable(item: ListItem): boolean {
  return item.type !== "justUpdated";
}

/**
 * Whether each note says that typing the tool's name in Terminal may not
 * run this instance's copy: this copy is not on the PATH Banager sees, so
 * the name finds nothing there or another program with that name
 * (`NotOnPath`); another program with that name is found there before this
 * copy (`ShadowedBy*`); or the launcher's program files are gone
 * (`LauncherOnly`). Read by `saysItUpdatesItself`: a tool that updates
 * itself does so when it runs, and "it usually updates itself" is true of
 * this copy only where typing its name runs it. A `Record`, so a note
 * added to `InstanceNote` without an answer here fails `tsc`.
 */
const NAME_MAY_NOT_RUN_THIS_COPY: Record<InstanceNote, boolean> = {
  // Homebrew's: about its list of software, not about which copy runs.
  IndexMayBeStale: false,
  IndexUpdating: false,
  NotOnPath: true,
  ShadowedByHomebrew: true,
  ShadowedByNpm: true,
  ShadowedByOther: true,
  LauncherOnly: true,
};

export function UpdatesPage() {
  const { t, i18n } = useTranslation();
  const { data: snapshot, isLoading } = useSnapshot();
  const { data: settings } = useSettings();
  const saveSettings = useSaveSettings();
  const selectedUpdates = useUiStore((s) => s.selectedUpdates);
  const toggleUpdate = useUiStore((s) => s.toggleUpdate);
  const selectUpdates = useUiStore((s) => s.selectUpdates);
  const deselectUpdates = useUiStore((s) => s.deselectUpdates);
  const show = useUiStore((s) => s.updatesShow);
  const setShow = useUiStore((s) => s.setUpdatesShow);
  const setFocusedOpId = useUiStore((s) => s.setFocusedOpId);
  const setDrawerOpen = useUiStore((s) => s.setDrawerOpen);
  const operationFor = useUpdateOperationFor();
  const { data: operations } = useOperations();
  const opName = useOperationName(operations);
  const updateTargets = useUiStore((s) => s.updateTargets);
  const opFinishedAt = useUiStore((s) => s.opFinishedAt);
  const clearedJustUpdated = useUiStore((s) => s.clearedJustUpdated);
  const clearJustUpdated = useUiStore((s) => s.clearJustUpdated);
  // What the history kept, for 「最近的更新记录」 after a restart (src/lib/history.ts).
  const { data: history = NO_HISTORY } = useHistory();
  const clearHistory = useClearHistory();
  const showHiddenUpdates = useUiStore((s) => s.showHiddenUpdates);
  // The empty page's Check Again, the toolbar's own check, and when the
  // last one was, to the minute.
  const { checkAgain, checking } = useCheckAgain();
  const now = useMinuteClock(snapshot?.refreshed_at ?? null);

  // "N more can't be updated here": folded until pressed.
  const [showCantUpdate, setShowCantUpdate] = useState(false);
  // What the last "Copy command" did, said for a moment over the list.
  const { status: copyStatus, copy: copyCommand } = useCopyCommand();
  // A tool's line in the window's language: Chinese in Chinese, and
  // English in English for an npm, PyPI or crates.io package.
  const translatedDescription = useTranslatedDescription();

  // Every update the user has not hidden, with "Never remind me" or "Skip
  // this version": `notHidden`, the rule in src/lib/updateState.ts that
  // the Installed page's chips read too. Everything below that lists,
  // counts or selects a row starts from this list.
  const visibleUpdates = useMemo(() => {
    if (!snapshot || !settings) return [];
    return notHidden(snapshot.updates, settings);
  }, [snapshot, settings]);

  // A candidate carries no adapter of its own; the only route from an
  // UpdateCandidate to the source that produced it is its key's
  // `instance_id`, joined back to the snapshot's instances.
  const instancesById = useMemo(() => {
    const byId = new Map<string, ManagerInstance>();
    for (const instance of snapshot?.instances ?? []) byId.set(instance.id, instance);
    return byId;
  }, [snapshot]);

  // One lookup table instead of a `snapshot.artifacts.find` per row: that
  // scan made every render O(updates × artifacts).
  const artifactsById = useMemo(() => {
    const byId = new Map<string, InstalledArtifact>();
    for (const artifact of snapshot?.artifacts ?? []) {
      byId.set(artifactKeyId(artifact.key), artifact);
    }
    return byId;
  }, [snapshot]);

  // Each tool's other copies, for the word on a copy Terminal does not run (`notUsedWord`).
  const twins = useTwins(snapshot?.artifacts);

  // The source's name in the user's language, as the sidebar lists it --
  // with which one it is after it where this Mac has two of its kind,
  // 「Homebrew（Intel）」 (`instanceLabels`): the chip beside a row's name,
  // the `{{source}}` in its detail, and the one refusal that can reach a
  // real person verbatim otherwise (`planErrorMessage`'s NotActionable
  // case). A stale snapshot's own read-only/unavailable state cannot be
  // trusted for *which* reason applies -- that is exactly what went stale
  // -- but the instance's adapter and place, and therefore its label, do
  // not change underneath it, so this is safe to read from the same
  // snapshot.
  const labels = useMemo(() => instanceLabels(t, snapshot?.instances ?? []), [t, snapshot]);
  const sourceLabelFor = useCallback((instanceId: string): string => labels.get(instanceId) ?? instanceId, [labels]);

  // The name a row shows: the one the Installed page shows for the same
  // software ("Microsoft Visual Studio Code", "Claude Code"), or the
  // package's own name where the snapshot has no entry for it.
  const nameOf = useCallback(
    (candidate: UpdateCandidate): string =>
      artifactsById.get(artifactKeyId(candidate.key))?.display_name || candidate.key.name,
    [artifactsById],
  );

  // By name, as the user reads it -- a model's as its row shows it
  // (`listedName`) -- case and accents aside, and "node@22" after
  // "node@9". The key breaks a tie between two sources' same-named
  // packages, so the order never depends on the snapshot's.
  // Worked out once for every update of the check (`rankedComparator`), so
  // that each sort of hundreds of rows compares numbers, not names.
  const compareRows = useMemo(() => {
    const collator = new Intl.Collator(i18n.language, { numeric: true, sensitivity: "base" });
    return rankedComparator(
      snapshot?.updates ?? [],
      (a: UpdateCandidate, b: UpdateCandidate) =>
        collator.compare(listedName(a.key, nameOf(a)), listedName(b.key, nameOf(b))) ||
        collator.compare(artifactKeyId(a.key), artifactKeyId(b.key)),
    );
  }, [i18n.language, nameOf, snapshot]);

  // The confirmation every Update on this page opens -- a row's own,
  // Update selected and Update all: one batch flow, shared with the
  // Installed page's detail (`useUpdateConfirm`).
  const confirm = useUpdateConfirm({ nameOf, compare: compareRows, sourceLabelFor });
  const { openConfirm, dialogOpen, pageErrors } = confirm;
  // Where the focus goes once what was confirmed has started (`openConfirm`'s
  // `onStarted`): a row's own Update to its row, whose button gives way to
  // its progress; Update all and Update selected to the list's first row,
  // as the button turns off or into Update all -- never the window's body.
  const listHandle = useRef<VirtualListHandle | null>(null);
  // Clear was pressed, and the focus is to be found a place once its section is gone.
  const refocusAfterClear = useRef(false);
  // A row's ⋯ hid its update: the row the focus goes to once that row is
  // gone (the next row, or the one before it at the list's end), and the
  // settings the save was made from.
  const refocusAfterHide = useRef<{ gone: string; next: string | null; base: Settings } | null>(null);
  const focusRow = (candidate: UpdateCandidate) => () => listHandle.current?.focusKey(artifactKeyId(candidate.key));
  const focusList = () => listHandle.current?.focusFirst();

  const stateOf = (candidate: UpdateCandidate): UpdateState =>
    updateStateOf(candidate, instancesById.get(candidate.key.instance_id));

  // The rows Banager can update from here: every listed update whose row
  // has an Update button and a checkbox while no update takes it
  // (`startableUpdates`, below). `actionableUpdatesOf` is
  // `visibleUpdates` filtered by `isUpdateActionable` -- read-only source,
  // could not be checked, blocked, source not answering: `updateStateOf`
  // in src/lib/updateState.ts, which the Installed page's chips read too,
  // so the two pages cannot disagree about whether a package can be
  // updated -- and it is kept there because the sidebar's count on this
  // page's entry and the Overview's are it less the rows an update takes,
  // as below; neither may disagree with the page.
  // `Session::issue_plan` applies the same conditions in Rust (spec §2.5
  // for the source, `blocked_upgrade` in
  // crates/banager-core/src/session/plans.rs for the package), so a stale
  // snapshot costs an error message, not a wrong command.
  const actionableUpdates = useMemo(
    () => (snapshot && settings ? actionableUpdatesOf(snapshot, settings) : []),
    [snapshot, settings],
  );

  // The rows of it an update does not take (`holdsRow`): one under way,
  // or one that worked and still says so, stands in its row with no
  // checkbox -- Rust queues a second update of the same tool behind the
  // first, so a second click could only repeat it. These are every row
  // that shows a checkbox, and what Select all and Invert selection hand
  // to the store, so neither can tick a row the user could not tick by
  // hand (`useStartableUpdates`). Of them, Update all takes all but the
  // rows of a copy Terminal does not run, which keep their checkbox,
  // unticked (decision U4): `countedUpdates`, the header's "N updates",
  // which the update notification's report, the sidebar's count and the
  // Dock's badge read too (`useCountedUpdates`, `useUpdateCount`).
  //
  // While the 「显示」 popup shows only the AI coding tools (`shownBy`),
  // the rows it hides are out of all of these too: the list, Select all,
  // the count in Update all and what it updates are the rows in sight.
  const inView = useCallback(
    (candidate: UpdateCandidate) => shownBy(show, artifactsById.get(artifactKeyId(candidate.key))),
    [show, artifactsById],
  );
  const allStartable = useStartableUpdates() ?? NO_UPDATES;
  const startableUpdates = useMemo(() => allStartable.filter(inView), [allStartable, inView]);
  const allCounted = useCountedUpdates() ?? NO_UPDATES;
  const countedUpdates = useMemo(() => allCounted.filter(inView), [allCounted, inView]);

  // The list's two parts, each by name: the rows with an Update button,
  // and everything else listed -- pinned, read-only, could not be checked,
  // updating itself, its source not answering -- under "Can't update
  // here". Two numbers, never one: folding the second into the first
  // would promise buttons that are not there, and leaving it out would
  // call six listed pip packages "0 updates".
  const { actionableRows, otherRows } = useMemo(() => {
    const actionableIds = new Set(actionableUpdates.map((u) => artifactKeyId(u.key)));
    return {
      actionableRows: actionableUpdates.filter(inView).sort(compareRows),
      otherRows: visibleUpdates
        .filter((u) => !actionableIds.has(artifactKeyId(u.key)) && inView(u))
        .sort(compareRows),
    };
  }, [actionableUpdates, visibleUpdates, compareRows, inView]);

  // Only rows that are selected, still visible *and* still actionable
  // count. The store keeps a selection for a row that has since been
  // hidden; without this intersection "Update selected" would be enabled
  // for nothing and open an empty dialog. Actionability is in the same
  // intersection because a selection outlives the row that made it: a
  // candidate selected while it was actionable stays selected after a
  // refresh takes that away (a pin, a failed lookup, a source that stopped
  // answering), and the batch would then plan the very row whose Update
  // button has just gone.
  // An update taking a row since it was ticked leaves it out the same way.
  const selectedVisible = useMemo(() => {
    const startableIds = new Set(startableUpdates.map((u) => artifactKeyId(u.key)));
    // A set: with every one of hundreds of rows ticked, `includes` would go
    // through all of them for each row.
    const selected = new Set(selectedUpdates);
    return actionableRows.filter((u) => {
      const id = artifactKeyId(u.key);
      return startableIds.has(id) && selected.has(id);
    });
  }, [actionableRows, startableUpdates, selectedUpdates]);

  // "Update History": this session's updates that ended -- worked, or
  // did not, or ask to be checked -- once their rows have gone
  // (`justUpdatedOps`). Out of every count, and of Select all:
  // nothing in it has a checkbox or a button.
  //
  // The version is the one the snapshot now lists for the tool -- what is
  // installed, read back after the update -- or, where it lists none, the
  // one the update was for. A model's is a digest, and is not shown; nor
  // is one beside 「未能更新」 or what did not add up, where it would read
  // as the version the tool was updated to.
  //
  // Then what the history kept from before this window, of the last 30
  // days (`recentUpdates`): never a tool this window has an operation of,
  // and one that did not work or asks to be checked only while the last
  // check still offers that tool an update. Its row then lists it too, on
  // purpose: the row is a plain update that does not know the last try.
  // The two together newest first; this window's own that it did not see
  // finish last, as before.
  const justUpdated = useMemo((): JustUpdatedEntry[] => {
    const shownInRows = new Set<number>();
    for (const candidate of visibleUpdates) {
      const op = operationFor(candidate);
      if (op !== null) shownInRows.add(op.id);
    }
    const ops = justUpdatedOps(operations ?? [], {
      shownInRows,
      cleared: clearedJustUpdated,
      finishedAt: opFinishedAt,
    });
    // And nothing a kept Clear came after, should the web view have
    // reloaded since and forgotten `clearedJustUpdated`.
    const here = ops.filter((op) => !clearedHere(history, op.id)).flatMap((op): JustUpdatedEntry[] => {
      const ending = endingOfOutcome(op.outcome, op.already_updated ?? null, op.follow_up_warnings);
      if (ending === null) return [];
      const key = { instance_id: op.instance_id, kind: op.artifact_kind, name: op.name };
      const adapterId = instancesById.get(op.instance_id)?.adapter_id ?? adapterIdOf(op.instance_id);
      const installed = artifactsById.get(artifactKeyId(key))?.version;
      return [
        {
          id: `op:${op.id}`,
          opId: op.id,
          key,
          adapterId,
          sourceLabel: labels.get(op.instance_id) ?? adapterLabel(t, adapterId),
          name: opName(op),
          version:
            op.artifact_kind === "Model" || ending.kind !== "succeeded"
              ? null
              : installed || updateTargets[op.id] || null,
          finishedAt: opFinishedAt[op.id] ?? null,
          verified: verifiedHere(history, op.id),
          ending,
        },
      ];
    });
    // A failure or one to check, only while its update is still offered.
    const offered = new Set((snapshot?.updates ?? []).map((candidate) => artifactKeyId(candidate.key)));
    const kept = recentUpdates(history, operations ?? [], Date.now(), offered).flatMap((record): JustUpdatedEntry[] => {
      const ending = endingOfRecord(record.result, record.already_updated ?? null, record.follow_up_warnings);
      if (ending === null) return [];
      return [
        {
          id: `history:${record.run}:${record.op_id}`,
          opId: null,
          key: record.key,
          adapterId: record.adapter_id,
          sourceLabel: labels.get(record.key.instance_id) ?? adapterLabel(t, record.adapter_id),
          name: record.display_name,
          version: record.key.kind === "Model" || ending.kind !== "succeeded" ? null : record.to_version,
          finishedAt: record.finished_at,
          verified: record.verified,
          ending,
        },
      ];
    });
    const at = (entry: JustUpdatedEntry) => entry.finishedAt ?? Number.NEGATIVE_INFINITY;
    // Stable: this window's own keep their order among themselves.
    return [...here, ...kept].sort((a, b) => {
      const byTime = at(b) - at(a);
      return Number.isNaN(byTime) ? 0 : byTime;
    });
  }, [
    history,
    snapshot,
    visibleUpdates,
    operationFor,
    operations,
    clearedJustUpdated,
    opFinishedAt,
    instancesById,
    artifactsById,
    opName,
    updateTargets,
    labels,
    t,
  ]);
  // Clear takes this window's off its list, and has the history note the
  // time, so that none of what was shown comes back after a restart.
  const clearJustUpdatedList = () => {
    refocusAfterClear.current = true;
    clearJustUpdated(justUpdated.flatMap((entry) => (entry.opId === null ? [] : [entry.opId])));
    clearHistory.mutate();
  };

  // What each source has to say about this check, one compact line each
  // at the top of the page: not running, not answering, a list it could
  // not download, another copy that runs when its name is typed. What a
  // source lets Banager do at all -- pip being read-only -- is not a
  // notice: every row of such a source says it with its own "View only"
  // chip. How many tools a source has installed is part of what its
  // notice says: a silent source's "these are its last answer" is true
  // only over rows it actually has -- counted as every page counts them
  // (`installedCountByInstance`), not by the updates listed here.
  //
  // Iterates `snapshot.instances`, which is every source any candidate can
  // come from: `refresh` builds `updates` only from instances it also puts
  // in `instances` (crates/banager-core/src/session/refresh.rs).
  //
  // First, the checks that did not finish this round, if any
  // (`unfinishedChecksNotice`): a line like the others, once a band of
  // its own over the page. Then the tools this check could not look up
  // (`failedLookupsNotice`): how many and why, up front with Check Again,
  // not only under the fold of "Can't update here" (walk-2 W2-1) -- of
  // every row listed, whatever the 「显示」 popup shows, as it is about the
  // check.
  const notices = useMemo(() => {
    const installed = installedCountByInstance(snapshot?.artifacts ?? []);
    const unfinished = snapshot ? unfinishedChecksNotice(t, snapshot.errors, snapshot.instances) : null;
    const lookups = failedLookupsNotice(t, visibleUpdates.filter(isFailedLookup));
    return [
      ...(unfinished === null ? [] : [unfinished]),
      ...(lookups === null ? [] : [lookups]),
      ...(snapshot?.instances ?? []).flatMap((instance) =>
        sourceNoticesFor(instance, sourceLabelFor(instance.id), installed.get(instance.id) ?? 0),
      ),
    ];
  }, [snapshot, sourceLabelFor, t]);
  // Two lines or more fold into one (`SourceNotices`).
  const noticeFold = useNoticeFold(notices.length);

  // How many rows can only say that Banager could not check them, the
  // tool's own words being hidden while "Show technical details" is off:
  // an uncheckable row with a `Message` (`saysWhyInToolWords`, which the
  // count of tools that could not be checked starts from too). The page says once, over those
  // rows, how many have hidden check details and where to see them -- a
  // `NonRegistrySource` row already says its own reason. It claims a
  // diagnosis only where the tool's own words give one a person knows, the
  // same for every one of these rows (`sharedCannotCheckCause`): no
  // network, a full disk. Any other words are the only thing that tells
  // "this Mac is offline" from "that index is refusing you"
  // (`lookup_failure_reason`, crates/banager-core/src/adapters/mod.rs),
  // and they are precisely what is hidden.
  const hiddenReasonRows = settings?.show_technical_details ? [] : otherRows.filter(saysWhyInToolWords);
  const hiddenReasonCount = hiddenReasonRows.length;
  const hiddenReasonCause = sharedCannotCheckCause(hiddenReasonRows);

  // 「最近的更新记录」 comes after the updates still to install, as the App
  // Store's Update History comes under Pending: what is to be done
  // first, what was done after. An update that finishes in this window
  // keeps its tick in its own row, where it was pressed, until the check
  // after it lands (`justUpdatedOps`); only then does it move down to the
  // top of 「最近的更新记录」, as 「今天…」. With nothing to install, it is the
  // only list, right under the notices.
  const items = useMemo<ListItem[]>(() => {
    const recent = justUpdated.length > 0 ? [{ type: "justUpdated", count: justUpdated.length } as const] : [];
    const pending = actionableRows.length > 0 || otherRows.length > 0;
    return [
      ...(notices.length > 0 ? [{ type: "notices", count: notices.length } as const] : []),
      ...(pending ? [] : recent),
      ...actionableRows.map((candidate): ListItem => ({ type: "update", candidate, updatable: true })),
      // Only the AI coding tools shown, and none of them listed: a line
      // that says so, where the rows would be.
      ...(show !== "all" && actionableRows.length === 0 && otherRows.length === 0
        ? [{ type: "showEmpty" } as const]
        : []),
      ...(otherRows.length > 0
        ? [{ type: "section", count: otherRows.length, expanded: showCantUpdate } as const]
        : []),
      ...(showCantUpdate && hiddenReasonCount > 0
        ? [{ type: "summary", count: hiddenReasonCount, cause: hiddenReasonCause } as const]
        : []),
      ...(showCantUpdate
        ? otherRows.map((candidate): ListItem => ({ type: "update", candidate, updatable: false }))
        : []),
      ...(pending ? recent : []),
    ];
  }, [notices.length, justUpdated.length, actionableRows, otherRows, showCantUpdate, hiddenReasonCount, show]);

  // Once Clear has taken 「最近的更新记录」 away, its button with it, the focus
  // goes to the list's first row, or, with no list, the page's title --
  // not the window's body, from where the next Tab would start over at
  // the sidebar. The history's own lines go when it answers, so this
  // waits for the section to be gone.
  useEffect(() => {
    if (!refocusAfterClear.current || justUpdated.length > 0) return;
    refocusAfterClear.current = false;
    const focus = document.activeElement;
    if (focus !== null && focus !== document.body && focus.isConnected) return;
    if (visibleUpdates.length > 0 && items.some(keyboardRow)) listHandle.current?.focusFirst();
    else focusOrFallback(null);
  });

  // Once a row's ⋯ has hidden its update -- Skip this version, Remind me
  // in 30 days, Don't remind me -- and the row is gone with its ⋯, the
  // focus goes to the row after it, as a Mac list's selection does, rather
  // than staying on the page's title (where the ⋯ left it as it went,
  // `Menu`), from where the keyboard would start over in the toolbar.
  // Only while the focus is still lost: not once the user has moved it.
  // Forgotten once the settings have changed with the row still listed, so
  // that a later removal by something else moves no focus.
  useEffect(() => {
    const hidden = refocusAfterHide.current;
    if (hidden === null) return;
    if (items.some((item) => listItemKey(item) === hidden.gone)) {
      if (settings !== hidden.base) refocusAfterHide.current = null;
      return;
    }
    refocusAfterHide.current = null;
    const focus = document.activeElement;
    const lost =
      focus === null || focus === document.body || !focus.isConnected || focus.hasAttribute("data-focus-fallback");
    if (!lost) return;
    if (hidden.next !== null && items.some((item) => listItemKey(item) === hidden.next)) {
      listHandle.current?.focusKey(hidden.next);
    } else if (items.some(keyboardRow)) listHandle.current?.focusFirst();
  });

  // The names the list shows under more than one source (spec R3), whose
  // rows say their source's name after the tool's.
  const namedTwice = useMemo(
    () =>
      namesUnderSeveralSources(
        items.flatMap((item) =>
          item.type === "update" ? [{ name: nameOf(item.candidate), instanceId: item.candidate.key.instance_id }] : [],
        ),
      ),
    [items, nameOf],
  );

  const viewLog = (opId: number) => {
    setFocusedOpId(opId);
    setDrawerOpen(true);
  };

  /**
   * Whether the row may say its tool usually updates itself: an actionable
   * row of a tool with its own installer that updates itself in the
   * background (`auto_updates`, set by the standalone adapter from its
   * recipe; spec D5). The row is real -- it compares the launcher's live
   * version with the published one -- and keeps its button; the chip says
   * the tool usually does this itself. It does so when it runs, and typing
   * its name runs this copy only where no note says otherwise
   * (`NAME_MAY_NOT_RUN_THIS_COPY`; the notice at the top says why): there
   * the row is a plain one, behind and updatable. Only for the standalone
   * adapters: a self-updating Homebrew cask listed by --greedy is a plain
   * row too, since Homebrew, not the app, is what its button drives.
   */
  const saysItUpdatesItself = (
    candidate: UpdateCandidate,
    instance: ManagerInstance | undefined,
  ): boolean =>
    instance !== undefined &&
    instance.adapter_id.startsWith("standalone-") &&
    artifactsById.get(artifactKeyId(candidate.key))?.auto_updates === true &&
    !instance.status.notes.some((note) => NAME_MAY_NOT_RUN_THIS_COPY[note]);

  /**
   * The row's status word, one per `UpdateState`, with its why behind an
   * ⓘ -- or none, for a row that can simply be updated: what its
   * `StatusChip` is drawn from, and its word the row's name says. A `switch` with no
   * default, so a state added to `UpdateState` without a word here fails
   * `tsc`. One word a row (spec §3.4): a read-only source's row that
   * could not be checked either says "Can't check", this check's news and
   * the words the line over these rows counts, and its why says both --
   * that it could not be checked, then that no button will ever appear on
   * it, whatever the next check finds.
   */
  const statusOf = (
    candidate: UpdateCandidate,
    state: UpdateState,
    instance: ManagerInstance | undefined,
  ): StatusChipProps | undefined => {
    const showTechnicalDetails = settings?.show_technical_details ?? false;
    const source = sourceLabelFor(candidate.key.instance_id);
    switch (state.kind) {
      case "actionable": {
        // One word a row: a copy Terminal does not run says so -- updating
        // it changes nothing the user types; an app that updates itself
        // says that; any other update that changes the major version says
        // 「大版本更新」.
        const id = artifactKeyId(candidate.key);
        return (
          notUsedWord(t, artifactsById.get(id), twins.get(id), sourceLabelFor, nameOf(candidate)) ??
          (saysItUpdatesItself(candidate, instance)
            ? { label: t("updates.selfUpdating"), detail: t("clarity.selfUpdatingDetail") }
            : majorVersionWord(t, candidate, nameOf(candidate)))
        );
      }
      case "readOnly":
        return candidate.checkable
          ? { label: t("updates.readOnly"), detail: readOnlyDetail(t, instance) }
          : {
              label: t("updates.cannotCheck"),
              detail: (
                <>
                  {cannotCheckDetail(t, candidate, showTechnicalDetails)}
                  <div className="mt-1.5">{readOnlyDetail(t, instance)}</div>
                </>
              ),
            };
      case "cannotCheck":
        return { label: t("updates.cannotCheck"), detail: cannotCheckDetail(t, candidate, showTechnicalDetails) };
      case "blocked":
        return {
          label: t(UPDATE_BLOCKED_KEYS[state.reason].badge),
          detail: blockedDetail(
            t,
            candidate,
            state.reason,
            instance,
            source,
            showTechnicalDetails,
            artifactsById.get(artifactKeyId(candidate.key)),
          ),
        };
      case "sourceUnavailable":
        return { label: t("updates.sourceUnavailable"), detail: unavailableDetail(t, instance, source) };
    }
  };

  // What a row's two hiding items share: `next` builds the settings to
  // save from the ones on screen.
  function hide(candidate: UpdateCandidate, next: (current: Settings) => Settings) {
    // One save at a time. A second choice while the first save is pending
    // would build its settings from the same stale base, and the later save
    // would overwrite the earlier one. The items are disabled meanwhile;
    // this guard covers a choice that was already on its way.
    if (!settings || saveSettings.isPending) return;
    if (selectedUpdates.includes(artifactKeyId(candidate.key))) {
      toggleUpdate(candidate.key);
    }
    const gone = artifactKeyId(candidate.key);
    const at = items.findIndex((item) => listItemKey(item) === gone);
    const neighbour =
      items.slice(at + 1).find(keyboardRow) ?? items.slice(0, Math.max(at, 0)).reverse().find(keyboardRow);
    refocusAfterHide.current = { gone, next: neighbour === undefined ? null : listItemKey(neighbour), base: settings };
    saveSettings.mutate(next(settings), {
      onError: () => {
        refocusAfterHide.current = null;
      },
    });
  }

  // "Skip this version": hides this update until the source offers another
  // version (`withSkippedVersion`, `hidingRule`).
  const skipVersion = (candidate: UpdateCandidate) =>
    hide(candidate, (current) => ({
      ...current,
      skipped_versions: withSkippedVersion(current.skipped_versions, candidate),
    }));

  // "Remind Me in 30 Days": hides every update of this package for 30
  // days, then lists it again by itself (`withSnoozed`, `hidingRule`).
  const snooze = (candidate: UpdateCandidate) =>
    hide(candidate, (current) => ({
      ...current,
      snoozed_updates: withSnoozed(current.snoozed_updates, candidate),
    }));

  // "Never remind me": hides every update of this package, now and later.
  const neverRemind = (candidate: UpdateCandidate) =>
    hide(candidate, (current) => ({
      ...current,
      ignored_updates: [...current.ignored_updates, candidate.key],
    }));

  /**
   * The row's ⋯ menu: the three ways to stop seeing this update, the
   * lightest first, and -- with technical details on -- the command its
   * chip talks about. "Skip this version" hides it until the source offers
   * another version; "Remind Me in 30 Days" hides every update of this
   * package for 30 days; "Never remind me" hides every update of this
   * package until the user undoes it in Settings. Each item's hint -- a tooltip,
   * and its accessible description -- says what it does. A row whose
   * `target` does not name one release gets only "Never remind me"
   * (`canSkipVersion`): one Banager could not check, whose `target` is its
   * installed version, and a Homebrew cask declared `version :latest`,
   * every release of which is offered as "latest", so that a skip of it
   * would never end. "Copy command" only where the command is known
   * without asking the backend for a plan: a blocked row's unpin command
   * or launcher.
   */
  const menuItems = (
    candidate: UpdateCandidate,
    state: UpdateState,
    instance: ManagerInstance | undefined,
  ): MenuItem[] => {
    const items: MenuItem[] = [];
    if (canSkipVersion(candidate)) {
      items.push({
        id: "skip",
        label: t("updates.skipVersion"),
        hint: t("updates.skipVersionHint"),
        disabled: saveSettings.isPending,
        onSelect: () => skipVersion(candidate),
      });
    }
    // Lasts 30 days, whatever version the source offers meanwhile: on
    // every row, as Never remind me is.
    items.push({
      id: "snooze",
      label: t("updates.snooze"),
      hint: t("updates.snoozeHint"),
      disabled: saveSettings.isPending,
      onSelect: () => snooze(candidate),
    });
    items.push({
      id: "never",
      label: t("updates.neverRemind"),
      hint: t("updates.neverRemindHint"),
      disabled: saveSettings.isPending,
      onSelect: () => neverRemind(candidate),
    });
    const blockedCommand =
      state.kind === "blocked" ? UPDATE_BLOCKED_KEYS[state.reason].command(candidate.key, instance) : "";
    // A reason with nothing to run (`Disabled`) has nothing to copy.
    if (settings?.show_technical_details && blockedCommand !== "") {
      const command = blockedCommand;
      items.push({
        id: "copy",
        label: t("common.copyCommand"),
        // Not a choice about the update, as the two above are.
        separatorBefore: true,
        onSelect: () => copyCommand(command),
      });
    }
    return items;
  };

  if (isLoading) {
    // Before `get_snapshot` answers, the first check is under way too:
    // the same view `SnapshotStatus` shows once it has.
    return <FirstCheck />;
  }
  if (!snapshot || !settings) {
    return null;
  }

  // Over the sentence the page says when it lists nothing, where there is
  // no list for them to be the first row of.
  const noticeLines =
    notices.length > 0 ? (
      <div className="flex flex-col px-5 pb-3">
        <SourceNotices notices={notices} layout="line" fold={noticeFold} grid="checkbox" separator={false} />
      </div>
    ) : null;

  const justUpdatedSection =
    justUpdated.length > 0 ? <JustUpdated entries={justUpdated} onClear={clearJustUpdatedList} /> : null;

  // Two different kinds of empty: the backend found no updates, or it found
  // some and the user has hidden every one (skipped the version it offers,
  // or asked never to be reminded about it). Only the first can mean the
  // machine is up to date -- and only when every source actually answered.
  //
  // The notices go *above* the sentence. "Everything is up to date" over a
  // stopped Ollama or a Homebrew whose catalogue could not be downloaded is
  // precisely the lie this page used to tell: no candidates is exactly
  // what an unreachable source produces, and the page read that silence as
  // good news. When a source did not answer, carries a note that means its
  // updates were not fully checked (`NOTE_LEAVES_UPDATES_UNCHECKED`), or
  // had a check fail this round (a `SourceError`), the sentence drops to
  // what Banager can honestly claim -- nothing to update *in the sources it
  // managed to check*. Not for every notice: one that is information only
  // -- which copy runs when you type a tool's name -- still goes above the
  // sentence, and leaves the sentence alone. A read-only source is one
  // Banager *can* check. The rule is `everySourceChecked` in
  // src/lib/updateState.ts, which the Overview's headline reads too: it
  // may call the Mac up to date only when this page would.
  if (visibleUpdates.length === 0) {
    const upToDate =
      snapshot.updates.length === 0 && everySourceChecked(snapshot.instances, snapshot.errors);
    const refreshedAt = snapshot.refreshed_at;
    // As macOS says an empty list (`EmptyState`): up to date, when the
    // last check was, and Check Again; every update hidden, and where
    // they are; nothing in what could be checked, over the notices that
    // say what could not. When the last check was is a label, as in the
    // toolbar's tooltip and under the Overview's status -- 「上次检查：
    // 刚才」 -- not a sentence, so no full stop.
    const empty =
      snapshot.updates.length > 0 ? (
        <EmptyState
          title={t("updates.allHiddenTitle")}
          description={t("updates.allHiddenDescription")}
          action={{ label: t("updates.showHidden"), onClick: showHiddenUpdates }}
        />
      ) : upToDate ? (
        <EmptyState
          symbol="check"
          title={t("updates.upToDate")}
          description={
            refreshedAt === null ? undefined : elapsedText(t, CHECKED_KEYS, elapsedSince(refreshedAt, now))
          }
          action={{ label: t("header.checkAgain"), onClick: checkAgain, disabled: checking }}
        />
      ) : (
        <EmptyState title={t("updates.noneCheckable")} />
      );
    // The last update to go leaves this: what was just updated, over the
    // sentence that says nothing is left. Taller than the window, the page
    // scrolls in its box (`App`).
    return (
      <div className="flex min-h-full flex-col">
        {noticeLines}
        {justUpdatedSection !== null ? <div className="px-5 pb-4 pt-3">{justUpdatedSection}</div> : null}
        <div className="flex flex-1 flex-col">{empty}</div>
      </div>
    );
  }

  // One row. Its checkbox and Update button come with an Update button's
  // state and no other (`actionable`); `checkable: false` in particular
  // must offer neither -- "Update" on a git-installed crate would run
  // `cargo install --force {name}` against the crates.io crate of the
  // same name, a different package. While an update of it is under way,
  // or has just finished, its progress stands where the button was --
  // except an update that ended without updating on a row that still
  // offers Update: how it ended takes the status word's place, and Retry
  // takes the button's place, opening the confirmation Update opens.
  //
  // A row under "N more can't be updated here" has its name, its line,
  // its status word and its ⋯, and nothing else (spec §3.3): no version
  // to move to and no button to press. It keeps a checkbox's room, and
  // the version's and the button's, empty, as every row here does, so the
  // avatars stay in one column and the status words in another.
  const updateRow = (candidate: UpdateCandidate, updatable: boolean) => {
    const instance = instancesById.get(candidate.key.instance_id);
    // Resolved once per row: the status word and the row's own
    // actionability must agree.
    const state = stateOf(candidate);
    const actionable = state.kind === "actionable";
    const name = nameOf(candidate);
    const source = sourceLabelFor(candidate.key.instance_id);
    const op = operationFor(candidate);
    const progress = op !== null ? progressOf(op) : null;
    const retry = progress !== null && actionable && isRetryable(progress);
    // Stopped where sudo wanted the Mac's password: no Retry (`isRetryable`),
    // and the way on is the command for Terminal in its log. That is said
    // as a button of its own, 「查看步骤」, in the button's place, the word
    // standing where Retry's word would (walk-2 W2-5): a red word alone
    // read as a dead end, and the steps as a log for programmers.
    const passwordSteps = passwordStepsOpId(progress);
    const outcome =
      progress !== null ? <UpdateProgress progress={progress} name={name} onViewLog={viewLog} /> : null;
    // How it ended has the status word's column to itself: it comes back
    // once the outcome clears -- a Retry under way, a newer version offered.
    const endingInStatus = retry || passwordSteps !== null;
    const word = endingInStatus ? undefined : statusOf(candidate, state, instance);
    const status = endingInStatus ? outcome : word === undefined ? undefined : <StatusChip {...word} />;
    // The same, in words, for the row's name.
    const statusText = endingInStatus && progress !== null ? progressWord(t, progress) : word?.label;
    const adapterId = instance?.adapter_id ?? adapterIdOf(candidate.key.instance_id);
    const artifact = artifactsById.get(artifactKeyId(candidate.key));
    const column = updateVersionColumn(t, candidate);
    const action =
      passwordSteps !== null ? (
        <RowAction onClick={() => viewLog(passwordSteps)} ariaLabel={t("needsPassword.viewStepsLabel", { name })}>
          {t("needsPassword.viewSteps")}
        </RowAction>
      ) : progress !== null && !retry ? (
        outcome
      ) : actionable ? (
        <RowAction
          onClick={(event) => void openConfirm([candidate], event.currentTarget, focusRow(candidate))}
          disabled={dialogOpen}
          ariaLabel={retry ? t("updates.retryLabel", { name }) : t("updates.updateLabel", { name })}
        >
          {retry ? t("updates.retry") : t("updates.update")}
        </RowAction>
      ) : null;
    return (
      <ToolRow
        adapterId={adapterId}
        sourceLabel={source}
        // The tool's logo, and a cask's app's own icon once it arrives.
        iconKey={candidate.key}
        name={name}
        // A model's path: the model as the name, where it is from before
        // its line.
        namePath={modelPath(candidate.key, name)}
        showSource={namedTwice.has(nameKey(name))}
        // The same line the Installed page's row has (`toolDescription`):
        // the tool's line in the window's language, the source's
        // description, a standalone tool's summary, or what its source
        // says it is.
        description={toolDescription(
          t,
          {
            description: artifact?.description,
            translated: translatedDescription(candidate.key, adapterId),
            kind: candidate.key.kind,
            path: artifact?.path,
          },
          adapterId,
          source,
        )}
        selectable={
          actionable && !holdsRow(op)
            ? {
                checked: selectedUpdates.includes(artifactKeyId(candidate.key)),
                onToggle: () => toggleUpdate(candidate.key),
                ariaLabel: t("updates.selectRow", { name }),
              }
            : null
        }
        status={status ?? undefined}
        statusText={statusText}
        // Empty under "can't be updated here", but there: its status word
        // stands in the column the rows' above stand in.
        version={updatable ? column.version : null}
        newVersion={updatable ? column.newVersion : undefined}
        // An update under way keeps its progress, wherever its row is now.
        action={updatable ? action : progress !== null ? action : null}
        menu={<Menu label={t("common.moreActions", { name })} items={menuItems(candidate, state, instance)} />}
      />
    );
  };

  // Whether a row has anything in its status column: its word
  // (`statusOf`), or how an update of it ended without updating, with
  // Retry in its button's place (`updateRow`). Where no row the list shows
  // has, the rows give that column's room to their names and descriptions
  // (`VirtualList`'s `statusColumn`).
  const hasStatusWord = (candidate: UpdateCandidate): boolean => {
    const state = stateOf(candidate);
    const op = operationFor(candidate);
    const progress = op !== null ? progressOf(op) : null;
    if (progress !== null && state.kind === "actionable" && isRetryable(progress)) return true;
    if (passwordStepsOpId(progress) !== null) return true;
    return statusOf(candidate, state, instancesById.get(candidate.key.instance_id)) !== undefined;
  };
  const statusColumn = items.some((item) => item.type === "update" && hasStatusWord(item.candidate));

  const startableCount = startableUpdates.length;
  // Whether this Mac has any AI coding tool at all, for what the line in
  // place of none of their rows says.
  const aiToolsInstalled = snapshot.artifacts.some((artifact) => shownBy("ai", artifact));
  // The checkboxes that are ticked, against those there are: the list
  // header's box is ticked for all, a dash for some (`indeterminate`).
  const selectedCount = selectedVisible.length;
  const allSelected = startableCount > 0 && selectedCount === startableCount;
  const toggleAll = () => {
    const keys = startableUpdates.map((u) => u.key);
    if (allSelected) deselectUpdates(keys);
    else selectUpdates(keys);
  };

  return (
    <div className="flex h-full flex-col">
      {/* The page's one action, in the toolbar (spec §3.2, §3.5): the
          rows that are ticked, or else every row it can update -- one
          confirmation for either, the one a row's own Update opens, and
          the one accent-coloured button on the screen. Update all ticks
          its rows first, so the list shows what the confirmation is
          about. Both act on the rows that show a checkbox
          (`startableUpdates`) and on no others: a row in any other
          `UpdateState` -- read-only, could not be checked, blocked, its
          source not answering -- has no checkbox, and a row the user hid
          (skipped, or never to be reminded about) is not listed at all.
          Update all leaves out the rows of a copy Terminal does not run
          (`countedUpdates`, decision U4): ticked by hand, Update selected
          takes them. */}
      <ToolbarItems>
        <ToolShowButton value={show} onChange={setShow} />
        {selectedCount > 0 ? (
          <button
            type="button"
            disabled={dialogOpen}
            onClick={(event) => void openConfirm(selectedVisible, event.currentTarget, focusList)}
            className={BUTTON.regular.default}
          >
            {t("updates.updateSelectedCount", { number: selectedCount })}
          </button>
        ) : (
          <button
            type="button"
            disabled={countedUpdates.length === 0 || dialogOpen}
            onClick={(event) => {
              // Ticked while the sheet asks, and unticked again if it is
              // cancelled: a cancel changes nothing.
              const before = new Set(useUiStore.getState().selectedUpdates);
              const ticked = countedUpdates.map((u) => u.key).filter((key) => !before.has(artifactKeyId(key)));
              selectUpdates(ticked);
              void openConfirm(countedUpdates, event.currentTarget, focusList, () => deselectUpdates(ticked));
            }}
            className={BUTTON.regular.default}
          >
            {/* Never 「更新这0个」: with none to start -- none listed, all of
                them already updating, or only copies Terminal does not
                run -- the plain word, greyed. */}
            {show === "all" || countedUpdates.length === 0
              ? t("updates.updateAll")
              : t("families.updateTheseCount", { count: countedUpdates.length })}
          </button>
        )}
      </ToolbarItems>
      {pageErrors.map(({ candidate, refusal }) => (
        <Refusal
          key={artifactKeyId(candidate.key)}
          text={refusal.text}
          detail={refusal.detail}
          detailTitle={refusal.text}
          className="px-5 pb-2"
        />
      ))}
      {saveSettings.isError ? (
        <p role="alert" className="px-5 pb-2 text-body text-danger-text">
          {settingsSaveSentence(
            t,
            "updates.saveChoiceFailed",
            saveSettings.error.message,
            settings?.show_technical_details ?? false,
          )}
        </p>
      ) : null}
      {/* The list's header, 28 high, over the list and still while it
          scrolls: a box that ticks every row that has one, in the rows'
          checkbox column -- ticked for all, a dash for some -- and, at its
          right, how the last Copy command went. */}
      <div className="flex h-7 shrink-0 items-center gap-3 border-b border-separator px-5">
        <label className="flex min-w-0 items-center gap-3 text-body text-foreground">
          <input
            type="checkbox"
            ref={(box) => {
              if (box !== null) box.indeterminate = selectedCount > 0 && !allSelected;
            }}
            checked={allSelected}
            disabled={startableCount === 0}
            onChange={toggleAll}
            aria-label={t("updates.selectAllLabel")}
            className="h-4 w-4 shrink-0"
          />
          <span aria-hidden="true" className={startableCount === 0 ? "text-tertiary" : undefined}>
            {t("updates.selectAll")}
          </span>
        </label>
        <p role="status" className="ml-auto truncate text-small text-muted">
          {copyStatusText(t, copyStatus)}
        </p>
      </div>
      {/* Virtualized, like the Installed page. A source that cannot reach
          its registry reports one `checkable: false` candidate per
          installed package, so a Mac that is merely offline turns this
          into a list as long as everything it has installed -- and it
          would stall exactly when the user is already confused about why
          nothing could be checked. */}
      <VirtualList
        items={items}
        itemKey={listItemKey}
        estimateSize={estimateSize}
        reusable={reusable}
        keyboardRows={keyboardRow}
        handleRef={listHandle}
        hairlineBefore={hairlineBefore}
        statusColumn={statusColumn}
        renderItem={(item) =>
          item.type === "notices" ? (
            // On the rows' grid: the ⚠︎ in the avatars' column, the
            // words where the names start.
            <div className="px-5">
              <SourceNotices notices={notices} layout="line" fold={noticeFold} grid="checkbox" />
            </div>
          ) : item.type === "justUpdated" ? (
            // Drawn anew each time the list is (`reusable`): it reads the clock.
            // 20 in, as the rows are; a little more room over it under the
            // rows than at the top.
            <div className={`px-5 pb-4 ${items.some(keyboardRow) ? "pt-5" : "pt-3"}`}>
              <JustUpdated entries={justUpdated} onClear={clearJustUpdatedList} />
            </div>
          ) : item.type === "section" ? (
            <CantUpdateHere
              count={item.count}
              expanded={item.expanded}
              onToggle={() => setShowCantUpdate((shown) => !shown)}
            />
          ) : item.type === "showEmpty" ? (
            // As the Installed page says an empty list: one line, 13 in the
            // secondary colour, no symbol.
            <p data-list-empty="" className="px-5 py-10 text-center text-body text-muted">
              {aiToolsInstalled ? t("families.noUpdates") : t("families.none")}
            </p>
          ) : item.type === "summary" ? (
            // Why these rows could not be checked is the tool's own words,
            // hidden while "Show technical details" is off: a button that
            // turns it on, rather than a sentence that says where it is.
            // Where the rows' names start, over the rows it is about.
            <div className="px-5">
              <div className={`flex min-h-8 items-center gap-2 text-small text-muted ${NOTICE_GRID.checkbox.inset}`}>
                <p className="min-w-0">
                  {item.cause === null
                    ? t("updates.cannotCheckSummary", { count: item.count })
                    : // Two sentences as the language spaces them: none after 「。」.
                      t("runtimeGuard.then", {
                        first: t("updates.cannotCheckSummary", { count: item.count }),
                        then: t(FAILURE_CAUSE_KEYS[item.cause].line),
                      })}
                </p>
                <button
                  type="button"
                  disabled={saveSettings.isPending}
                  title={t("updates.showReasonsHint", { setting: t("settings.showTechnicalDetails.label") })}
                  onClick={() => saveSettings.mutate({ ...settings, show_technical_details: true })}
                  className={BUTTON.small.grey}
                >
                  {t("updates.showReasons")}
                </button>
              </div>
            </div>
          ) : (
            updateRow(item.candidate, item.updatable)
          )
        }
      />
      <UpdateConfirmDialog confirm={confirm} />
    </div>
  );
}
