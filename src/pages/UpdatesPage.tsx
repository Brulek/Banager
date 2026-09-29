import { useCallback, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useCheckAgain, useOperations, useSnapshot, useSettings, useSaveSettings } from "../lib/queries";
import { elapsedSince } from "../lib/format";
import { useUiStore, artifactKeyId } from "../store/ui";
import {
  ADAPTER_LABEL_KEYS,
  adapterIdOf,
  adapterLabel,
  settingsSaveErrorMessage,
  sourceNoticesFor,
  toolDescription,
  UPDATE_BLOCKED_KEYS,
} from "../lib/sources";
import { warningMessage } from "../lib/warnings";
import { useCopyCommand } from "../lib/clipboard";
import { useOperationName } from "../lib/operations";
import { useTranslatedDescription } from "../lib/toolDescriptions";
import { nameKey, namesUnderSeveralSources } from "../lib/names";
import { JustUpdated, justUpdatedOps, type JustUpdatedEntry } from "../components/JustUpdated";
import { RowAction, ToolRow } from "../components/ToolRow";
import { StatusChip } from "../components/StatusChip";
import { Menu, type MenuItem } from "../components/ui/Menu";
import { SourceNotices, useNoticeFold } from "../components/SourceNotices";
import { UpdateConfirmDialog, useUpdateConfirm } from "../components/UpdateConfirm";
import { Refusal } from "../components/SheetParts";
import { VirtualList } from "../components/VirtualList";
import { ToolbarItems } from "../components/Toolbar";
import { useRovingRow } from "../components/rovingRows";
import { FirstCheck } from "../components/StatusRing";
import { EmptyState } from "../components/EmptyState";
import { CHECKED_KEYS, elapsedText, useMinuteClock } from "../components/PageHeader";
import {
  holdsRow,
  isRetryable,
  isUnderway,
  progressOf,
  UpdateProgress,
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
import { DisclosureIcon } from "../components/icons";
import { BUTTON } from "../components/ui/controls";
import type {
  InstanceNote,
  InstalledArtifact,
  ManagerInstance,
  Settings,
  UpdateCandidate,
} from "../lib/types";
import {
  actionableUpdatesOf,
  canSkipVersion,
  everySourceChecked,
  notHidden,
  updateStateOf,
  withSkippedVersion,
} from "../lib/updateState";
import type { UpdateState } from "../lib/updateState";

// The virtualizer's first guesses: a row, the "N more can't be updated
// here" line and the line under it, a notice's line, and "Just updated"
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
 * How many rows have a checkbox, as the page says it over its list and
 * the window's toolbar under its title (`useUpdatesHeadline`): 「10个可更新」.
 * With none, not "0 updates": the rows under "Can't update here" are real,
 * and simply not Canager's to update. While some are updating, how many,
 * and how many more have a checkbox.
 */
export function updatesHeadline(t: Translate, updatingCount: number, startableCount: number): string {
  if (updatingCount > 0) {
    return startableCount > 0
      ? `${t("overview.updating", { count: updatingCount })}${t("overview.listSeparator")}${t("updates.alsoCount", { count: startableCount })}`
      : t("overview.updating", { count: updatingCount });
  }
  return startableCount === 0 ? t("updates.noneActionable") : t("updates.count", { count: startableCount });
}

/**
 * The page's headline (`updatesHeadline`) from outside it, for the
 * toolbar's subtitle: the same rows counted the same way -- those with a
 * checkbox (`useStartableUpdates`), and those an update is installing now
 * (`isUnderway`) -- so the two can never say different numbers. Null
 * until the snapshot and the settings are in, and while the page lists
 * nothing at all and says so in a sentence of its own.
 */
export function useUpdatesHeadline(): string | null {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const { data: settings } = useSettings();
  const operationFor = useUpdateOperationFor();
  const startable = useStartableUpdates();
  const listed = useMemo(
    () =>
      snapshot && settings
        ? { any: notHidden(snapshot.updates, settings).length > 0, actionable: actionableUpdatesOf(snapshot, settings) }
        : undefined,
    [snapshot, settings],
  );
  if (listed === undefined || startable === undefined || !listed.any) return null;
  const updating = listed.actionable.filter((candidate) => isUnderway(operationFor(candidate))).length;
  return updatesHeadline(t, updating, startable.length);
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
 * list's first row, which scrolls away with it (spec §3.8) -- then, while
 * there is anything in it, "Just updated" (`JustUpdated`).
 */
type ListItem =
  | { type: "notices"; count: number }
  | { type: "justUpdated"; count: number }
  | { type: "update"; candidate: UpdateCandidate; updatable: boolean }
  | { type: "section"; count: number; expanded: boolean }
  | { type: "summary"; count: number };

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

/**
 * The line that discloses the rows that can't be updated here, 32 high:
 * a 10pt triangle and the words, muted (spec §3.3; cork-outdated-zh.png).
 * One of the rows ↑ and ↓ move between, Space or Enter opening it.
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
      className="relative flex h-8 w-full items-center gap-1.5 px-5 text-left text-body text-muted"
    >
      <DisclosureIcon size={10} className={`shrink-0 transition-transform ${expanded ? "rotate-90" : ""}`} />
      {t("updates.cantUpdateHere", { number: count })}
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
 * run this instance's copy: this copy is not on the PATH Canager sees, so
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
  const setFocusedOpId = useUiStore((s) => s.setFocusedOpId);
  const setDrawerOpen = useUiStore((s) => s.setDrawerOpen);
  const operationFor = useUpdateOperationFor();
  const { data: operations } = useOperations();
  const opName = useOperationName(operations);
  const updateTargets = useUiStore((s) => s.updateTargets);
  const opFinishedAt = useUiStore((s) => s.opFinishedAt);
  const clearedJustUpdated = useUiStore((s) => s.clearedJustUpdated);
  const clearJustUpdated = useUiStore((s) => s.clearJustUpdated);
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

  // The source's name in the user's language: the chip beside a row's
  // name, the `{{source}}` in its detail, and the one refusal that can
  // reach a real person verbatim otherwise (`planErrorMessage`'s
  // NotActionable case). A stale snapshot's own read-only/unavailable
  // state cannot be trusted for *which* reason applies -- that is exactly
  // what went stale -- but the instance's adapter, and therefore its
  // label, does not change underneath it, so this is safe to read from the
  // same snapshot.
  const sourceLabelFor = useCallback(
    (instanceId: string): string => {
      const instance = instancesById.get(instanceId);
      if (!instance) return instanceId;
      const labelKey = ADAPTER_LABEL_KEYS[instance.adapter_id];
      return labelKey ? t(labelKey) : instance.adapter_id;
    },
    [instancesById, t],
  );

  // The name a row shows: the one the Installed page shows for the same
  // software ("Microsoft Visual Studio Code", "Claude Code"), or the
  // package's own name where the snapshot has no entry for it.
  const nameOf = useCallback(
    (candidate: UpdateCandidate): string =>
      artifactsById.get(artifactKeyId(candidate.key))?.display_name || candidate.key.name,
    [artifactsById],
  );

  // By name, as the user reads it: case and accents aside, and "node@22"
  // after "node@9". The key breaks a tie between two sources' same-named
  // packages, so the order never depends on the snapshot's.
  const compareRows = useMemo(() => {
    const collator = new Intl.Collator(i18n.language, { numeric: true, sensitivity: "base" });
    return (a: UpdateCandidate, b: UpdateCandidate) =>
      collator.compare(nameOf(a), nameOf(b)) ||
      collator.compare(artifactKeyId(a.key), artifactKeyId(b.key));
  }, [i18n.language, nameOf]);

  // The confirmation every Update on this page opens -- a row's own,
  // Update selected and Update all: one batch flow, shared with the
  // Installed page's detail (`useUpdateConfirm`).
  const confirm = useUpdateConfirm({ nameOf, compare: compareRows, sourceLabelFor });
  const { openConfirm, dialogOpen, pageErrors } = confirm;

  const stateOf = (candidate: UpdateCandidate): UpdateState =>
    updateStateOf(candidate, instancesById.get(candidate.key.instance_id));

  // The rows Canager can update from here: every listed update whose row
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
  // crates/canager-core/src/session/plans.rs for the package), so a stale
  // snapshot costs an error message, not a wrong command.
  const actionableUpdates = useMemo(
    () => (snapshot && settings ? actionableUpdatesOf(snapshot, settings) : []),
    [snapshot, settings],
  );

  // The rows of it an update does not take (`holdsRow`): one under way,
  // or one that worked and still says so, stands in its row with no
  // checkbox -- Rust queues a second update of the same tool behind the
  // first, so a second click could only repeat it. These are every row
  // that shows a checkbox, the header's "N updates", and what Select all,
  // Invert selection and Update all hand to the store, so none of them can
  // tick a row the user could not tick by hand. `useStartableUpdates`,
  // which the update notification's report, the sidebar's count and the
  // Dock's badge read too (`useUpdateCount`).
  const startableUpdates = useStartableUpdates() ?? NO_UPDATES;

  // The list's two parts, each by name: the rows with an Update button,
  // and everything else listed -- pinned, read-only, could not be checked,
  // updating itself, its source not answering -- under "Can't update
  // here". Two numbers, never one: folding the second into the first
  // would promise buttons that are not there, and leaving it out would
  // call six listed pip packages "0 updates".
  const { actionableRows, otherRows } = useMemo(() => {
    const actionableIds = new Set(actionableUpdates.map((u) => artifactKeyId(u.key)));
    return {
      actionableRows: [...actionableUpdates].sort(compareRows),
      otherRows: visibleUpdates
        .filter((u) => !actionableIds.has(artifactKeyId(u.key)))
        .sort(compareRows),
    };
  }, [actionableUpdates, visibleUpdates, compareRows]);

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
    return actionableRows.filter((u) => {
      const id = artifactKeyId(u.key);
      return startableIds.has(id) && selectedUpdates.includes(id);
    });
  }, [actionableRows, startableUpdates, selectedUpdates]);

  // "Just updated": this session's updates that worked, once their rows
  // have gone (`justUpdatedOps`). Out of every count, and of Select all:
  // nothing in it has a checkbox or a button.
  //
  // The version is the one the snapshot now lists for the tool -- what is
  // installed, read back after the update -- or, where it lists none, the
  // one the update was for. A model's is a digest, and is not shown.
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
    return ops.map((op) => {
      const key = { instance_id: op.instance_id, kind: op.artifact_kind, name: op.name };
      const adapterId = instancesById.get(op.instance_id)?.adapter_id ?? adapterIdOf(op.instance_id);
      const installed = artifactsById.get(artifactKeyId(key))?.version;
      return {
        opId: op.id,
        key,
        adapterId,
        sourceLabel: adapterLabel(t, adapterId),
        name: opName(op),
        version: op.artifact_kind === "Model" ? null : installed || updateTargets[op.id] || null,
        finishedAt: opFinishedAt[op.id] ?? null,
      };
    });
  }, [
    visibleUpdates,
    operationFor,
    operations,
    clearedJustUpdated,
    opFinishedAt,
    instancesById,
    artifactsById,
    opName,
    updateTargets,
    t,
  ]);
  const clearJustUpdatedList = () => clearJustUpdated(justUpdated.map((entry) => entry.opId));

  // What each source has to say about this check, one compact line each
  // at the top of the page: not running, not answering, a list it could
  // not download, another copy that runs when its name is typed. What a
  // source lets Canager do at all -- pip being read-only -- is not a
  // notice: every row of such a source says it with its own "View only"
  // chip. How many rows a source has is part of what its notice says: a
  // silent source's "what's listed for it is last time's" is true only
  // over rows it actually has.
  //
  // Iterates `snapshot.instances`, which is every source any candidate can
  // come from: `refresh` builds `updates` only from instances it also puts
  // in `instances` (crates/canager-core/src/session/refresh.rs).
  const notices = useMemo(() => {
    const rowsByInstance = new Map<string, number>();
    for (const update of visibleUpdates) {
      const id = update.key.instance_id;
      rowsByInstance.set(id, (rowsByInstance.get(id) ?? 0) + 1);
    }
    return (snapshot?.instances ?? []).flatMap((instance) =>
      sourceNoticesFor(instance, sourceLabelFor(instance.id), rowsByInstance.get(instance.id) ?? 0),
    );
  }, [snapshot, visibleUpdates, sourceLabelFor]);
  // Two lines or more fold into one (`SourceNotices`).
  const noticeFold = useNoticeFold(notices.length);

  // How many rows can only say that Canager could not check them, the
  // tool's own words being hidden while "Show technical details" is off:
  // an uncheckable row with a `Message`. The page says once, over those
  // rows, where to see why, counting these rows and no others -- a
  // `NonRegistrySource` row already says its own reason. It claims no
  // diagnosis: the first line of the tool's stderr is the only thing that
  // tells "this Mac is offline" from "that index is refusing you"
  // (`lookup_failure_reason`, crates/canager-core/src/adapters/mod.rs),
  // and it is precisely what is hidden.
  const hiddenReasonCount = settings?.show_technical_details
    ? 0
    : otherRows.filter(
        (candidate) =>
          !candidate.checkable &&
          candidate.warnings.some((warning) => warningMessage(warning) !== null),
      ).length;

  const items = useMemo<ListItem[]>(
    () => [
      ...(notices.length > 0 ? [{ type: "notices", count: notices.length } as const] : []),
      ...(justUpdated.length > 0 ? [{ type: "justUpdated", count: justUpdated.length } as const] : []),
      ...actionableRows.map((candidate): ListItem => ({ type: "update", candidate, updatable: true })),
      ...(otherRows.length > 0
        ? [{ type: "section", count: otherRows.length, expanded: showCantUpdate } as const]
        : []),
      ...(showCantUpdate && hiddenReasonCount > 0
        ? [{ type: "summary", count: hiddenReasonCount } as const]
        : []),
      ...(showCantUpdate
        ? otherRows.map((candidate): ListItem => ({ type: "update", candidate, updatable: false }))
        : []),
    ],
    [notices.length, justUpdated.length, actionableRows, otherRows, showCantUpdate, hiddenReasonCount],
  );

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
   * ⓘ -- or none, for a row that can simply be updated. A `switch` with no
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
  ): ReactNode | undefined => {
    const showTechnicalDetails = settings?.show_technical_details ?? false;
    const source = sourceLabelFor(candidate.key.instance_id);
    switch (state.kind) {
      case "actionable":
        return saysItUpdatesItself(candidate, instance) ? <StatusChip label={t("updates.selfUpdating")} /> : undefined;
      case "readOnly":
        return candidate.checkable ? (
          <StatusChip label={t("updates.readOnly")} detail={readOnlyDetail(t, instance)} />
        ) : (
          <StatusChip
            label={t("updates.cannotCheck")}
            detail={
              <>
                {cannotCheckDetail(t, candidate, showTechnicalDetails)}
                <div className="mt-1.5">{readOnlyDetail(t, instance)}</div>
              </>
            }
          />
        );
      case "cannotCheck":
        return (
          <StatusChip label={t("updates.cannotCheck")} detail={cannotCheckDetail(t, candidate, showTechnicalDetails)} />
        );
      case "blocked":
        return (
          <StatusChip
            label={t(UPDATE_BLOCKED_KEYS[state.reason].badge)}
            detail={blockedDetail(t, candidate, state.reason, instance, source, showTechnicalDetails)}
          />
        );
      case "sourceUnavailable":
        return (
          <StatusChip label={t("updates.sourceUnavailable")} detail={unavailableDetail(t, instance, source)} />
        );
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
    saveSettings.mutate(next(settings));
  }

  // "Skip this version": hides this update until the source offers another
  // version (`withSkippedVersion`, `hidingRule`).
  const skipVersion = (candidate: UpdateCandidate) =>
    hide(candidate, (current) => ({
      ...current,
      skipped_versions: withSkippedVersion(current.skipped_versions, candidate),
    }));

  // "Never remind me": hides every update of this package, now and later.
  const neverRemind = (candidate: UpdateCandidate) =>
    hide(candidate, (current) => ({
      ...current,
      ignored_updates: [...current.ignored_updates, candidate.key],
    }));

  /**
   * The row's ⋯ menu: the two ways to stop seeing this update, the lighter
   * one first, and -- with technical details on -- the command its chip
   * talks about. "Skip this version" hides it until the source offers
   * another version; "Never remind me" hides every update of this package
   * until the user undoes it in Settings. Each item's hint -- a tooltip,
   * and its accessible description -- says what it does. A row whose
   * `target` does not name one release gets only "Never remind me"
   * (`canSkipVersion`): one Canager could not check, whose `target` is its
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
    items.push({
      id: "never",
      label: t("updates.neverRemind"),
      hint: t("updates.neverRemindHint"),
      disabled: saveSettings.isPending,
      onSelect: () => neverRemind(candidate),
    });
    if (settings?.show_technical_details && state.kind === "blocked") {
      const command = UPDATE_BLOCKED_KEYS[state.reason].command(candidate.key, instance);
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
        <SourceNotices notices={notices} layout="line" fold={noticeFold} />
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
  // what Canager can honestly claim -- nothing to update *in the sources it
  // managed to check*. Not for every notice: one that is information only
  // -- which copy runs when you type a tool's name -- still goes above the
  // sentence, and leaves the sentence alone. A read-only source is one
  // Canager *can* check. The rule is `everySourceChecked` in
  // src/lib/updateState.ts, which the Overview's headline reads too: it
  // may call the Mac up to date only when this page would.
  if (visibleUpdates.length === 0) {
    const upToDate =
      snapshot.updates.length === 0 && everySourceChecked(snapshot.instances, snapshot.errors);
    const refreshedAt = snapshot.refreshed_at;
    // As macOS says an empty list (`EmptyState`): up to date, when the
    // last check was, and Check Again; every update hidden, and where
    // they are; nothing in what could be checked, over the notices that
    // say what could not.
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
            refreshedAt === null
              ? undefined
              : t("updates.lastCheckedSentence", {
                  when: elapsedText(t, CHECKED_KEYS, elapsedSince(refreshedAt, now)),
                })
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
  // to move to and no button to press. It keeps a checkbox's room, as
  // every row here does, so the avatars stay in one column.
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
    const outcome =
      progress !== null ? <UpdateProgress progress={progress} name={name} onViewLog={viewLog} /> : null;
    // How it ended has the status word's column to itself: it comes back
    // once the outcome clears -- a Retry under way, a newer version offered.
    const status = retry ? outcome : statusOf(candidate, state, instance);
    const adapterId = instance?.adapter_id ?? candidate.key.instance_id.split(":")[0];
    const artifact = artifactsById.get(artifactKeyId(candidate.key));
    const column = updateVersionColumn(t, candidate);
    const action =
      progress !== null && !retry ? (
        outcome
      ) : actionable ? (
        <RowAction onClick={(event) => void openConfirm([candidate], event.currentTarget)} disabled={dialogOpen}>
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
        version={updatable ? column.version : undefined}
        newVersion={updatable ? column.newVersion : undefined}
        // An update under way keeps its progress, wherever its row is now.
        action={updatable ? action : progress !== null ? action : undefined}
        menu={<Menu label={t("common.moreActions", { name })} items={menuItems(candidate, state, instance)} />}
      />
    );
  };

  const startableCount = startableUpdates.length;
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
          them all first, as Select all would, so the list shows what the
          confirmation is about. Both act on the rows that show a checkbox
          (`startableUpdates`) and on no others: a row in any other
          `UpdateState` -- read-only, could not be checked, blocked, its
          source not answering -- has no checkbox, and a row the user hid
          (skipped, or never to be reminded about) is not listed at all. */}
      <ToolbarItems>
        {selectedCount > 0 ? (
          <button
            type="button"
            disabled={dialogOpen}
            onClick={(event) => void openConfirm(selectedVisible, event.currentTarget)}
            className={BUTTON.regular.default}
          >
            {t("updates.updateSelectedCount", { number: selectedCount })}
          </button>
        ) : (
          <button
            type="button"
            disabled={startableCount === 0 || dialogOpen}
            onClick={(event) => {
              selectUpdates(startableUpdates.map((u) => u.key));
              void openConfirm(startableUpdates, event.currentTarget);
            }}
            className={BUTTON.regular.default}
          >
            {t("updates.updateAll")}
          </button>
        )}
      </ToolbarItems>
      {pageErrors.map((item) => {
        const text = t("updates.planFailed", { message: item.planError });
        return (
          <Refusal
            key={artifactKeyId(item.candidate.key)}
            text={text}
            detail={item.planErrorDetail}
            detailTitle={text}
            className="px-5 pb-2"
          />
        );
      })}
      {saveSettings.isError ? (
        <p role="alert" className="px-5 pb-2 text-body text-danger-text">
          {t("updates.saveChoiceFailed", {
            message: settingsSaveErrorMessage(t, saveSettings.error.message),
          })}
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
          {copyStatus === "copied" ? t("common.copied") : copyStatus === "failed" ? t("common.copyFailed") : null}
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
        renderItem={(item) =>
          item.type === "notices" ? (
            <div className="px-5">
              <SourceNotices notices={notices} layout="line" fold={noticeFold} />
            </div>
          ) : item.type === "justUpdated" ? (
            // Drawn anew each time the list is (`reusable`): it reads the clock.
            // 20 in, as the rows are.
            <div className="px-5 pb-4 pt-3">
              <JustUpdated entries={justUpdated} onClear={clearJustUpdatedList} />
            </div>
          ) : item.type === "section" ? (
            <CantUpdateHere
              count={item.count}
              expanded={item.expanded}
              onToggle={() => setShowCantUpdate((shown) => !shown)}
            />
          ) : item.type === "summary" ? (
            // Why these rows could not be checked is the tool's own words,
            // hidden while "Show technical details" is off: a button that
            // turns it on, rather than a sentence that says where it is.
            <div className="flex min-h-8 items-center gap-2 px-5 text-small text-muted">
              <p className="min-w-0">{t("updates.cannotCheckSummary", { count: item.count })}</p>
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
          ) : (
            updateRow(item.candidate, item.updatable)
          )
        }
      />
      <UpdateConfirmDialog confirm={confirm} />
    </div>
  );
}
