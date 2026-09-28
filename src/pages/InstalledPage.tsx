import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useOperations, useSettings, useSnapshot } from "../lib/queries";
import { artifactKeyId, useUiStore } from "../store/ui";
import {
  ADAPTER_LABEL_KEYS,
  canWrite,
  isAvailable,
  sourceNoticesFor,
  toolDescription,
  uninstallBlockedCopy,
  uninstallHoldKey,
  UPDATE_BLOCKED_KEYS,
} from "../lib/sources";
import {
  hidingRule,
  leftOutOfUpdateCheck,
  shownSkippedVersion,
  updateStateOf,
  upToDateIsKnown,
} from "../lib/updateState";
import type { HiddenBy } from "../lib/updateState";
import { useCopyCommand } from "../lib/clipboard";
import type { InstalledArtifact, ManagerInstance, OpRequest, UpdateCandidate } from "../lib/types";
import { RowAction, ToolRow } from "../components/ToolRow";
import { StatusChip } from "../components/StatusChip";
import { Menu, type MenuItem } from "../components/ui/Menu";
import { Drawer } from "../components/ui/Drawer";
import { ChipRow } from "../components/ui/ChipRow";
import { SourceNotices } from "../components/SourceNotices";
import { SourceNoticeLine } from "../components/SourceNotice";
import { SourceAvatar } from "../components/SourceAvatar";
import { ToolAvatar } from "../components/ToolAvatar";
import { UninstallDialog } from "../components/UninstallDialog";
import { UpdateConfirmDialog, useUpdateConfirm } from "../components/UpdateConfirm";
import { isRetryable, progressOf, UpdateProgress, useUpdateOperationFor } from "../components/UpdateProgress";
import {
  blockedDetail,
  cannotCheckDetail,
  detailLines,
  readOnlyDetail,
  unavailableDetail,
} from "../components/updateDetails";
import { COMMAND_SLOT, withCommand } from "../components/withCommand";
import { Refusal } from "../components/SheetParts";
import { CheckIcon, ChevronIcon, SearchIcon } from "../components/icons";

// The virtualizer's first guesses: a row, a source's heading (sorted by
// source), and a "N more components" line. Each slot then measures itself
// through `measureElement`.
const ROW_ESTIMATE = 60;
const HEADING_ESTIMATE = 44;
const FOLD_ESTIMATE = 40;

/** An update the user hid on the Updates page, and how (`hidingRule`). */
interface HiddenUpdate {
  by: HiddenBy;
  candidate: UpdateCandidate;
}

/**
 * One of a row's chips: its word, the why behind its ⓘ (on the row) or
 * under it (in the drawer), and how it looks -- grey for what the row is
 * and why it can't do something, the accent for an update to be had, and
 * a quiet tick for up to date.
 */
interface RowChip {
  id: string;
  label: string;
  detail?: ReactNode;
  /** What the drawer says under the chip when the row's ⓘ says nothing: a model's newer build. */
  drawerDetail?: ReactNode;
  tone: "neutral" | "accent" | "upToDate";
}

/**
 * One slot in the virtualized list: a tool's row; a source's heading, only
 * when the list is sorted by source and shows every source; and a
 * source's "N more components came with other software" line, which
 * unfolds its components under it.
 */
type ListItem =
  | { type: "heading"; instance: ManagerInstance; label: string; count: number }
  | { type: "row"; artifact: InstalledArtifact; instance: ManagerInstance; label: string }
  | { type: "fold"; instance: ManagerInstance; label: string; count: number; expanded: boolean };

/**
 * A slot's identity: its React key, and the key the virtualizer files the
 * slot's measured height under -- the same string, so a height stays with
 * the row it was measured from when a row above it goes (the Updates
 * page's `listItemKey` has the story). An artifact key id has a `|` in it,
 * and neither of the other two does.
 */
function listItemKey(item: ListItem): string {
  switch (item.type) {
    case "heading":
      return `heading:${item.instance.id}`;
    case "fold":
      return `fold:${item.instance.id}`;
    case "row":
      return artifactKeyId(item.artifact.key);
  }
}

/**
 * The version a row shows: the installed one, technical details on or
 * off, as the Updates page's rows show theirs. Not an Ollama model's: its
 * `version` is the local manifest digest /api/tags reports, not a version
 * number, and no hash goes in front of this audience. Nothing where the
 * source reported none.
 */
function versionOf(artifact: InstalledArtifact): string | null {
  if (artifact.key.kind === "Model" || artifact.version === "") return null;
  return artifact.version;
}

/** A chip on a row, or its word in the drawer. */
function RowChipView({ chip, withDetail }: { chip: RowChip; withDetail: boolean }) {
  if (chip.tone === "upToDate") {
    return (
      <span className="inline-flex items-center gap-1 whitespace-nowrap text-small text-muted">
        <CheckIcon size={13} className="shrink-0 text-success" />
        {chip.label}
      </span>
    );
  }
  return (
    <StatusChip label={chip.label} detail={withDetail ? chip.detail : undefined} tone={chip.tone} />
  );
}

/**
 * 已安装: everything the sources list, to find and to uninstall
 * (docs/superpowers/2026-09-27-ui-redesign.md, 已安装页).
 *
 * At the top a search box and the sort, and a row of filters: 「全部」 and
 * one per source with something installed, each with how much -- an
 * Overview tile opens the page on its own (`openInstalled`). One line
 * however many sources there are, which scrolls sideways when they do not
 * fit (`ChipRow`), so the list keeps its room at 800×600. Under them,
 * one line per thing a source had to say this time (`SourceNoticeLine`,
 * as on the Updates page).
 *
 * Then one list. By name, it is one flat list, each row naming its source
 * with the avatar and, where the list mixes sources, a chip: a tool is
 * found by its name, and at the window's default 800×600 a heading per
 * source would take the room of a row each for nothing the avatars do not
 * already say. By source, it is grouped under a heading per source -- only
 * while every source is shown; one source's list needs none. Either way,
 * what other software brought in is folded into one line per source,
 * 「另有 14 个被其它软件带来的组件」, which unfolds them under it.
 *
 * Each row: what it is, its version, its chips -- the why behind an ⓘ --
 * Uninstall where the source and the tool allow it (disabled, with a chip
 * saying why, while the source refuses one for now), and a ⋯ menu.
 * Pressing the row itself opens its details in a drawer from the right:
 * everything a row has no room for, and its Update.
 */
export function InstalledPage() {
  const { t, i18n } = useTranslation();
  const { data: snapshot, isLoading } = useSnapshot();
  const { data: settings } = useSettings();
  const query = useUiStore((s) => s.query);
  const setQuery = useUiStore((s) => s.setQuery);
  const filter = useUiStore((s) => s.installedFilter);
  const setFilter = useUiStore((s) => s.setInstalledFilter);
  const sort = useUiStore((s) => s.installedSort);
  const setSort = useUiStore((s) => s.setInstalledSort);
  const expandedDependencies = useUiStore((s) => s.expandedDependencies);
  const toggleDependencies = useUiStore((s) => s.toggleDependencies);
  const setFocusedOpId = useUiStore((s) => s.setFocusedOpId);
  const setDrawerOpen = useUiStore((s) => s.setDrawerOpen);
  const operationFor = useUpdateOperationFor();
  const { data: operations } = useOperations();
  const { status: copyStatus, copy: copyCommand } = useCopyCommand();
  const listRef = useRef<HTMLDivElement>(null);

  // Uninstall is destructive, so a button only *targets* an artifact;
  // UninstallDialog is what plans it, shows what it would change and what
  // would break, with the exact command a click away, and submits (Global
  // Constraints, spec §6).
  const [uninstallTarget, setUninstallTarget] = useState<{
    request: OpRequest;
    displayName: string;
  } | null>(null);
  // What opened the uninstall dialog -- a row's Uninstall, or the
  // drawer's -- which gets the focus back when it closes.
  const uninstallOpener = useRef<HTMLElement | null>(null);
  // The operation an uninstall just started. Its log opens once the dialog
  // has closed and given the focus back to what opened it, so that the log
  // drawer, which hands the focus back to what had it as it opened, hands
  // it back there too.
  const startedUninstall = useRef<number | null>(null);
  // The row whose details are open, by artifact key id; looked up in the
  // snapshot each time, so the drawer shows what the last check found.
  const [detailsId, setDetailsId] = useState<string | null>(null);
  // What opened those details: the row itself, or its ⋯ menu's button.
  const detailsOpener = useRef<HTMLElement | null>(null);
  // Set when the drawer closes for the log drawer to open: the focus goes
  // there, not back to the row.
  const leaveFocusOnClose = useRef(false);

  const showTechnicalDetails = settings?.show_technical_details ?? false;

  const instancesById = useMemo(() => {
    const byId = new Map<string, ManagerInstance>();
    for (const instance of snapshot?.instances ?? []) byId.set(instance.id, instance);
    return byId;
  }, [snapshot]);

  const artifactsById = useMemo(() => {
    const byId = new Map<string, InstalledArtifact>();
    for (const artifact of snapshot?.artifacts ?? []) byId.set(artifactKeyId(artifact.key), artifact);
    return byId;
  }, [snapshot]);
  // A tool that is gone -- uninstalled, or no longer listed -- closes its
  // drawer for good, so the same name listed again later does not open it.
  useEffect(() => {
    if (snapshot && detailsId !== null && !artifactsById.has(detailsId)) setDetailsId(null);
  }, [snapshot, detailsId, artifactsById]);

  // The source's name in the user's language: the filters, the rows'
  // chips and avatars, the `{{source}}` in a sentence.
  const labelOf = useCallback(
    (instance: ManagerInstance): string => {
      const labelKey = ADAPTER_LABEL_KEYS[instance.adapter_id];
      return labelKey ? t(labelKey) : instance.adapter_id;
    },
    [t],
  );
  const sourceLabelFor = useCallback(
    (instanceId: string): string => {
      const instance = instancesById.get(instanceId);
      return instance ? labelOf(instance) : instanceId;
    },
    [instancesById, labelOf],
  );

  // By name, as the user reads it: case and accents aside, and "node@22"
  // after "node@9"; the key breaks a tie, so the order never depends on
  // the snapshot's. The Updates page sorts the same way.
  const collator = useMemo(
    () => new Intl.Collator(i18n.language, { numeric: true, sensitivity: "base" }),
    [i18n.language],
  );
  const compareArtifacts = useCallback(
    (a: InstalledArtifact, b: InstalledArtifact) =>
      collator.compare(a.display_name, b.display_name) ||
      collator.compare(artifactKeyId(a.key), artifactKeyId(b.key)),
    [collator],
  );
  const nameOf = useCallback(
    (candidate: UpdateCandidate): string =>
      artifactsById.get(artifactKeyId(candidate.key))?.display_name || candidate.key.name,
    [artifactsById],
  );
  const compareCandidates = useCallback(
    (a: UpdateCandidate, b: UpdateCandidate) => collator.compare(nameOf(a), nameOf(b)),
    [collator, nameOf],
  );

  // The Updates page's own confirmation, for the drawer's Update: the same
  // plan, command, warnings and submission (`useUpdateConfirm`).
  const confirm = useUpdateConfirm({ nameOf, compare: compareCandidates, sourceLabelFor });

  // Every update in the snapshot, split by the rule the Updates page lists
  // by (`hidingRule`, src/lib/updateState.ts): the ones it lists, and the
  // ones the user hid there, with how. So a pinned package, one Canager
  // could not check and one the user ignored are never "Update available"
  // here while the Updates page offers none of them.
  const { listedUpdates, hiddenUpdates } = useMemo(() => {
    const hiddenBy = hidingRule(settings ?? { ignored_updates: [], skipped_versions: [] });
    const listed = new Map<string, UpdateCandidate>();
    const hidden = new Map<string, HiddenUpdate>();
    for (const candidate of snapshot?.updates ?? []) {
      const by = hiddenBy(candidate);
      if (by === null) listed.set(artifactKeyId(candidate.key), candidate);
      else hidden.set(artifactKeyId(candidate.key), { by, candidate });
    }
    return { listedUpdates: listed, hiddenUpdates: hidden };
  }, [snapshot, settings]);

  // How much each source has installed: its filter's count, the number an
  // Overview tile shows for it.
  const countByInstance = useMemo(() => {
    const counts = new Map<string, number>();
    for (const artifact of snapshot?.artifacts ?? []) {
      const id = artifact.key.instance_id;
      counts.set(id, (counts.get(id) ?? 0) + 1);
    }
    return counts;
  }, [snapshot]);

  // One filter per source with something installed, in the snapshot's
  // order -- the Overview's tiles' order.
  const filterSources = useMemo(
    () => (snapshot?.instances ?? []).filter((instance) => (countByInstance.get(instance.id) ?? 0) > 0),
    [snapshot, countByInstance],
  );
  // A filter that names a source with nothing installed any more shows
  // everything -- and is dropped, so a source that gets something again
  // does not filter the page by surprise.
  const activeFilter = filter !== null && (countByInstance.get(filter) ?? 0) > 0 ? filter : null;
  useEffect(() => {
    if (snapshot && filter !== null && activeFilter === null) setFilter(null);
  }, [snapshot, filter, activeFilter, setFilter]);
  // Headings only while the list is sorted by source and shows every
  // source; a row names its source with a chip only where the list mixes
  // sources and has no heading saying it.
  const grouped = sort === "source" && activeFilter === null;
  const mixed = activeFilter === null && !grouped;

  // What the search box asks for, by the name a row shows or the
  // package's own name ("visual-studio-code" finds "Microsoft Visual
  // Studio Code").
  const needle = query.trim().toLowerCase();

  // The rows the search matches, by source.
  const matchingByInstance = useMemo(() => {
    const byInstance = new Map<string, InstalledArtifact[]>();
    for (const artifact of snapshot?.artifacts ?? []) {
      const matches =
        needle === "" ||
        artifact.display_name.toLowerCase().includes(needle) ||
        artifact.key.name.toLowerCase().includes(needle);
      if (!matches) continue;
      const list = byInstance.get(artifact.key.instance_id) ?? [];
      list.push(artifact);
      byInstance.set(artifact.key.instance_id, list);
    }
    return byInstance;
  }, [snapshot, needle]);

  // The sources in view: the filter's, or every one.
  const instancesInView = useMemo(
    () => (snapshot?.instances ?? []).filter((instance) => activeFilter === null || instance.id === activeFilter),
    [snapshot, activeFilter],
  );

  const items = useMemo<ListItem[]>(() => {
    const result: ListItem[] = [];
    const rows: ListItem[] = [];
    const folds: ListItem[] = [];
    for (const instance of instancesInView) {
      const artifacts = matchingByInstance.get(instance.id) ?? [];
      if (artifacts.length === 0) continue;
      const label = labelOf(instance);
      const row = (artifact: InstalledArtifact): ListItem => ({ type: "row", artifact, instance, label });
      // `!== "Dependency"`, not `=== "Requested"`: pip can only ever report
      // Unknown or Dependency (its `--not-required` marks a leaf, which is
      // not the same as "the user asked for it"), so keying off "Requested"
      // would fold every pip package away.
      const primary = artifacts.filter((a) => a.reason !== "Dependency").sort(compareArtifacts);
      const dependencies = artifacts.filter((a) => a.reason === "Dependency").sort(compareArtifacts);
      // The fold line stays in both states, so a source's components fold
      // back up the way they unfolded.
      const expanded = expandedDependencies.includes(instance.id);
      const fold: ListItem[] =
        dependencies.length === 0
          ? []
          : [
              { type: "fold", instance, label, count: dependencies.length, expanded },
              ...(expanded ? dependencies.map(row) : []),
            ];
      if (grouped) {
        result.push({ type: "heading", instance, label, count: artifacts.length }, ...primary.map(row), ...fold);
      } else {
        rows.push(...primary.map(row));
        folds.push(...fold);
      }
    }
    if (!grouped) {
      const byName = (a: ListItem, b: ListItem) =>
        a.type === "row" && b.type === "row" ? compareArtifacts(a.artifact, b.artifact) : 0;
      result.push(...rows.sort(byName), ...folds);
    }
    return result;
  }, [instancesInView, matchingByInstance, labelOf, compareArtifacts, expandedDependencies, grouped]);

  // What each source in view has to say about this check, a line each
  // (`sourceNoticesFor`, the rule the Updates page and the Overview read):
  // not running, not answering, a list it could not download, another
  // copy that runs instead. Whether a silent source has rows here is part
  // of what its notice says -- "what's listed for uv is from the last time
  // it answered" over rows it has, "can't show what it has installed" over
  // none -- and a search that hides its rows does not make it have none.
  // Then a source whose version Canager has not been tested with.
  const notices = useMemo(
    () =>
      instancesInView.flatMap((instance) =>
        sourceNoticesFor(instance, labelOf(instance), countByInstance.get(instance.id) ?? 0),
      ),
    [instancesInView, labelOf, countByInstance],
  );
  const untested = instancesInView.filter(
    (instance) => instance.unverified_version !== null && (countByInstance.get(instance.id) ?? 0) > 0,
  );

  const getItemKey = useCallback((index: number) => listItemKey(items[index]), [items]);
  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => listRef.current,
    estimateSize: (index) => {
      const item = items[index];
      if (item?.type === "heading") return HEADING_ESTIMATE;
      if (item?.type === "fold") return FOLD_ESTIMATE;
      return ROW_ESTIMATE;
    },
    getItemKey,
  });

  if (isLoading) {
    return <p className="p-4 text-sm text-[var(--color-muted)]">{t("common.loading")}</p>;
  }
  if (!snapshot) {
    return null;
  }

  // Uninstall where both the source and the tool allow it: a source that
  // is read-only, or did not answer the last check -- its rows are last
  // time's, carried forward -- offers none, nor does a package the tool
  // refuses to remove (`uninstall_blocked`). `Session::issue_plan` refuses
  // all three in Rust whatever this page shows (spec §2.5).
  const canUninstall = (artifact: InstalledArtifact, instance: ManagerInstance): boolean =>
    canWrite(instance) && isAvailable(instance) && artifact.uninstall_blocked === null;
  // Such an Uninstall stays, disabled, while the source refuses to plan
  // one until a note of its goes away -- a Homebrew updating its list
  // (`uninstallHoldKey`) -- with a chip saying why. It comes back with the
  // refresh that clears the note.
  const uninstallHeld = (artifact: InstalledArtifact, instance: ManagerInstance): boolean =>
    canUninstall(artifact, instance) && (uninstallHoldKey(instance) !== null || uninstallUnderway(artifact) !== null);
  // An uninstall of this one already queued or running: its Uninstall
  // stays, disabled, and says which.
  const uninstallUnderway = (artifact: InstalledArtifact): string | null => {
    const id = artifactKeyId(artifact.key);
    const op = (operations ?? []).find(
      (op) =>
        op.kind === "Uninstall" &&
        op.status !== "Done" &&
        artifactKeyId({ instance_id: op.instance_id, kind: op.artifact_kind, name: op.name }) === id,
    );
    if (op === undefined) return null;
    return op.status === "Queued" ? t("installed.uninstallQueued") : t("installed.uninstalling");
  };

  // `opener` is the button pressed, passed rather than read off the focus:
  // a click in WebKit does not focus a button.
  const uninstall = (artifact: InstalledArtifact, opener: HTMLElement) => {
    uninstallOpener.current = opener;
    setUninstallTarget({
      request: {
        kind: "Uninstall",
        instance_id: artifact.key.instance_id,
        artifact_kind: artifact.key.kind,
        name: artifact.key.name,
      },
      displayName: artifact.display_name,
    });
  };

  // The row itself and its ⋯ menu's Details both put the focus on what
  // was pressed before they get here (`ToolRow`, `Menu`).
  const openDetails = (artifact: InstalledArtifact) => {
    detailsOpener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setDetailsId(artifactKeyId(artifact.key));
  };

  const describe = (artifact: InstalledArtifact, instance: ManagerInstance, label: string): string =>
    toolDescription(
      t,
      { description: artifact.description, kind: artifact.key.kind, path: artifact.path },
      instance.adapter_id,
      label,
    );

  // How an update the user hid on the Updates page reads here: how it was
  // hidden, where "Update available" would promise one that page no
  // longer lists. A skip names the version skipped -- the skip is about
  // that one -- except an Ollama model's, a digest, never shown
  // (`shownSkippedVersion`). A `switch` with no default, so a new
  // `HiddenBy` without a chip here fails `tsc`.
  const hiddenChip = ({ by, candidate }: HiddenUpdate): RowChip => {
    switch (by) {
      case "ignored":
        return {
          id: "hidden",
          label: t("installed.updateIgnored"),
          detail: detailLines([t("updates.neverRemindHint")]),
          tone: "neutral",
        };
      case "skipped": {
        const version = shownSkippedVersion({ key: candidate.key, version: candidate.target });
        return {
          id: "hidden",
          label:
            version === null
              ? t("installed.updateSkippedNewBuild")
              : t("installed.updateSkipped", { version }),
          detail: detailLines([t("updates.skipVersionHint")]),
          tone: "neutral",
        };
      }
    }
  };

  /**
   * A row's chips, each with its why: what its source lets Canager do,
   * the tool's own refusal to remove it, and where its update stands --
   * by `updateStateOf` for an update the Updates page lists, the way that
   * page's row reads, so "Update available" here is exactly an Update
   * button there. With no update listed, 「已是最新」 only where this round's
   * check reached its source in full and nothing about it failed
   * (`upToDateIsKnown`): a source that did not answer, one whose check
   * failed, a Homebrew still updating its list or one that could not,
   * leave last round's rows and updates, which no one checked this time.
   * Such a row says nothing about updates; its source's notice says why.
   * Nor on a cask Homebrew's check left out because Settings' "Show apps
   * that update themselves" is off (`leftOutOfUpdateCheck`).
   */
  const chipsOf = (artifact: InstalledArtifact, instance: ManagerInstance, label: string): RowChip[] => {
    const chips: RowChip[] = [];
    const id = artifactKeyId(artifact.key);
    const listed = listedUpdates.get(id);
    const hidden = hiddenUpdates.get(id);
    const cannotCheck = (candidate: UpdateCandidate): RowChip => ({
      id: "cannot-check",
      label: t("updates.cannotCheck"),
      detail: cannotCheckDetail(t, candidate, showTechnicalDetails),
      tone: "neutral",
    });
    // View only: the fact that no button ever appears on this row,
    // whatever the next check finds.
    if (!canWrite(instance)) {
      chips.push({ id: "read-only", label: t("updates.readOnly"), detail: readOnlyDetail(t, instance), tone: "neutral" });
    }
    if (artifact.uninstall_blocked !== null) {
      const copy = uninstallBlockedCopy(artifact.uninstall_blocked, instance.adapter_id);
      chips.push({
        id: "uninstall-blocked",
        label: t(copy.badge),
        detail: detailLines([
          withCommand(t(copy.description, { command: COMMAND_SLOT, source: label }), copy.command(artifact.key, instance)),
        ]),
        tone: "neutral",
      });
    }
    // Why the row's Uninstall is disabled for now.
    const holdKey = canUninstall(artifact, instance) ? uninstallHoldKey(instance) : null;
    if (holdKey !== null) {
      chips.push({
        id: "uninstall-held",
        label: t("installed.uninstallHold.label"),
        detail: detailLines([t(holdKey)]),
        tone: "neutral",
      });
    }
    if (listed !== undefined) {
      // A `switch` with no default, so a state added to `UpdateState`
      // without a chip here fails `tsc`.
      const state = updateStateOf(listed, instance);
      switch (state.kind) {
        case "actionable":
          chips.push({
            id: "update",
            label: t("installed.updateAvailable"),
            // The drawer's facts give the version it moves to, except a
            // model's, which has no version to give: "a newer build".
            drawerDetail: listed.channel === "Digest" ? t("updates.newBuild") : undefined,
            tone: "accent",
          });
          break;
        case "readOnly":
          // "View only" is said above; that this check found nothing is
          // its own news.
          if (!listed.checkable) chips.push(cannotCheck(listed));
          break;
        case "cannotCheck":
          chips.push(cannotCheck(listed));
          break;
        case "blocked":
          // A pinned package that is also pinned against its update says
          // "Pinned" once, with the unpin command, above.
          if (!(state.reason === "Pinned" && artifact.uninstall_blocked === "Pinned")) {
            chips.push({
              id: "update-blocked",
              label: t(UPDATE_BLOCKED_KEYS[state.reason].badge),
              detail: blockedDetail(t, listed, state.reason, instance, label, showTechnicalDetails),
              tone: "neutral",
            });
          }
          break;
        case "sourceUnavailable":
          // The newer version is one an earlier check found, carried
          // forward; the source's notice says it did not answer.
          chips.push({
            id: "update-unavailable",
            label: t("installed.updateSourceUnavailable"),
            detail: unavailableDetail(t, instance, label),
            tone: "neutral",
          });
          break;
      }
    } else if (hidden !== undefined) {
      chips.push(hiddenChip(hidden));
    } else if (
      upToDateIsKnown(instance, snapshot.errors) &&
      !leftOutOfUpdateCheck(artifact, settings?.include_self_updating ?? false)
    ) {
      chips.push({ id: "up-to-date", label: t("installed.upToDate"), tone: "upToDate" });
    }
    return chips;
  };

  // The command a row's chips talk about, known without asking for a
  // plan: the unpin command of a pinned package, or the launcher of a tool
  // that updates itself. An uninstall's own command needs a plan, which
  // its dialog shows, so a row has none to copy.
  const commandOf = (artifact: InstalledArtifact, instance: ManagerInstance): string | null => {
    if (artifact.uninstall_blocked !== null) {
      const command = uninstallBlockedCopy(artifact.uninstall_blocked, instance.adapter_id).command(
        artifact.key,
        instance,
      );
      if (command !== "") return command;
    }
    const listed = listedUpdates.get(artifactKeyId(artifact.key));
    if (listed === undefined) return null;
    const state = updateStateOf(listed, instance);
    return state.kind === "blocked" ? UPDATE_BLOCKED_KEYS[state.reason].command(listed.key, instance) : null;
  };

  // The ⋯ menu: the details, and -- with technical details on -- the
  // command a chip talks about.
  const menuItems = (artifact: InstalledArtifact, instance: ManagerInstance): MenuItem[] => {
    const items: MenuItem[] = [
      { id: "details", label: t("common.details"), onSelect: () => openDetails(artifact) },
    ];
    const command = commandOf(artifact, instance);
    if (showTechnicalDetails && command !== null) {
      items.push({ id: "copy", label: t("common.copyCommand"), onSelect: () => copyCommand(command) });
    }
    return items;
  };

  const toolRow = (artifact: InstalledArtifact, instance: ManagerInstance, label: string) => {
    const name = artifact.display_name;
    const chips = chipsOf(artifact, instance, label);
    return (
      <ToolRow
        adapterId={instance.adapter_id}
        sourceLabel={label}
        // The tool's logo, and a cask's app's own icon once it arrives.
        iconKey={artifact.key}
        name={name}
        // A tool with its own installer is its own source: the chip would
        // only say its name again.
        nameChip={mixed && label !== name ? label : undefined}
        description={describe(artifact, instance, label)}
        status={
          chips.length > 0
            ? chips.map((chip) => <RowChipView key={chip.id} chip={chip} withDetail />)
            : undefined
        }
        version={versionOf(artifact)}
        action={
          canUninstall(artifact, instance) ? (
            // Offered, not recommended: the quiet look (`RowAction`).
            <RowAction
              tone="quiet"
              disabled={uninstallHeld(artifact, instance)}
              onClick={(event) => uninstall(artifact, event.currentTarget)}
            >
              {uninstallUnderway(artifact) ?? t("installed.uninstall")}
            </RowAction>
          ) : null
        }
        menu={<Menu label={t("common.moreActions", { name })} items={menuItems(artifact, instance)} />}
        onOpen={() => openDetails(artifact)}
        openLabel={t("common.detailsLabel", { title: name })}
      />
    );
  };

  // The log drawer at the foot of the window, for an operation just
  // started or finished. The details drawer, a modal over the page, closes
  // first: the log drawer is not inside it, and would be out of reach. The
  // row that opened the details takes the focus on the way, where the
  // details drawer lets it -- after an uninstall's dialog has closed over
  // it -- so that the log drawer gives it back to the row, not to a button
  // that went with the details. From inside the details, the drawer keeps
  // the focus to itself until it closes.
  const openLog = (opId: number) => {
    if (detailsId !== null) {
      leaveFocusOnClose.current = true;
      if (detailsOpener.current?.isConnected) detailsOpener.current.focus();
      setDetailsId(null);
    }
    setFocusedOpId(opId);
    setDrawerOpen(true);
  };

  const details = detailsId === null ? undefined : artifactsById.get(detailsId);
  const detailsInstance = details === undefined ? undefined : instancesById.get(details.key.instance_id);

  /**
   * A row's details: all of its description, its version and the one an
   * update would bring, where it is (with technical details on), every
   * chip with its why in full, what its source had to say this time, and
   * what can be done -- Uninstall, and Update where the Updates page
   * offers one, through that page's own confirmation; while that update
   * runs, its progress where the button was, and once it has ended
   * without updating, how it ended beside Retry.
   */
  const detailsDrawer = (artifact: InstalledArtifact, instance: ManagerInstance) => {
    const label = labelOf(instance);
    const name = artifact.display_name;
    const chips = chipsOf(artifact, instance, label);
    const id = artifactKeyId(artifact.key);
    const listed = listedUpdates.get(id);
    const candidate = listed ?? hiddenUpdates.get(id)?.candidate;
    const updatable = listed !== undefined && updateStateOf(listed, instance).kind === "actionable";
    const op = listed !== undefined && updatable ? operationFor(listed) : null;
    const progress = op !== null ? progressOf(op) : null;
    const version = versionOf(artifact);
    // The version a listed or hidden update would bring: said in numbers
    // only where it is one -- not a model's digest, not the installed
    // version a row Canager could not check carries as its target.
    const newer =
      candidate !== undefined &&
      candidate.checkable &&
      candidate.channel !== "Digest" &&
      candidate.target !== "" &&
      candidate.target !== artifact.version
        ? candidate.target
        : null;
    const facts: Array<{ term: string; value: ReactNode }> = [];
    if (version !== null) facts.push({ term: t("installed.version"), value: version });
    if (newer !== null) facts.push({ term: t("installed.newVersion"), value: newer });
    // Where it is, only while technical details are on, and only where the
    // source said: an app's bundle, a program's file, a tool's own folder.
    if (showTechnicalDetails && artifact.path !== null) {
      facts.push({
        term: t("installed.location"),
        value: <code className="break-all font-mono text-small">{artifact.path}</code>,
      });
    }
    const sourceNotices = sourceNoticesFor(instance, label, countByInstance.get(instance.id) ?? 0);
    const removable = canUninstall(artifact, instance);
    const footer =
      removable || updatable ? (
        <>
          {removable ? (
            // As quiet as the row's, beside the accent of Update.
            <button
              type="button"
              data-tone="quiet"
              disabled={uninstallHeld(artifact, instance)}
              onClick={(event) => uninstall(artifact, event.currentTarget)}
              className="rounded-button border border-border bg-surface px-3.5 py-1.5 text-body font-medium text-muted outline-none transition-colors hover:border-danger/40 hover:bg-danger/10 hover:text-danger focus-visible:border-danger/40 focus-visible:text-danger focus-visible:ring-2 focus-visible:ring-danger/40 disabled:opacity-50 disabled:hover:border-border disabled:hover:bg-surface disabled:hover:text-muted"
            >
              {uninstallUnderway(artifact) ?? t("installed.uninstall")}
            </button>
          ) : null}
          {progress !== null ? <UpdateProgress progress={progress} name={name} onViewLog={openLog} /> : null}
          {/* As on the Updates page's row: an update that ended without
              updating keeps how it ended, with Retry in Update's place. */}
          {updatable && listed !== undefined && (progress === null || isRetryable(progress)) ? (
            <button
              type="button"
              onClick={(event) => void confirm.openConfirm([listed], event.currentTarget)}
              disabled={confirm.dialogOpen}
              className="rounded-button bg-accent px-4 py-1.5 text-body font-semibold text-accent-foreground outline-none transition-colors hover:bg-accent-hover focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 focus-visible:ring-offset-surface disabled:opacity-50"
            >
              {progress === null ? t("updates.update") : t("updates.retry")}
            </button>
          ) : null}
        </>
      ) : undefined;
    return (
      <Drawer
        open
        onOpenChange={(open) => {
          if (!open) setDetailsId(null);
        }}
        title={name}
        subtitle={label === name ? undefined : label}
        leading={<ToolAvatar adapterId={instance.adapter_id} sourceLabel={label} iconKey={artifact.key} />}
        description={describe(artifact, instance, label)}
        closeLabel={t("common.close")}
        onCloseAutoFocus={(event) => {
          if (leaveFocusOnClose.current) {
            leaveFocusOnClose.current = false;
            event.preventDefault();
          }
        }}
        footer={footer}
      >
        {facts.length > 0 ? (
          <dl className="mt-4 grid grid-cols-[auto_1fr] gap-x-6 gap-y-1.5 text-body">
            {facts.map((fact) => (
              <div key={fact.term} className="contents">
                <dt className="text-muted">{fact.term}</dt>
                <dd className="min-w-0 tabular-nums text-foreground">{fact.value}</dd>
              </div>
            ))}
          </dl>
        ) : null}
        {chips.length > 0 ? (
          <ul className="mt-5 flex flex-col gap-3">
            {chips.map((chip) => (
              <li key={chip.id} className="flex flex-col items-start gap-1">
                <RowChipView chip={chip} withDetail={false} />
                {chip.detail !== undefined ? (
                  <div className="text-body text-foreground">{chip.detail}</div>
                ) : chip.drawerDetail !== undefined ? (
                  <p className="break-words text-body text-foreground">{chip.drawerDetail}</p>
                ) : null}
              </li>
            ))}
          </ul>
        ) : null}
        {sourceNotices.length > 0 ? (
          <div className="mt-5 flex flex-col gap-2">
            <SourceNotices notices={sourceNotices} layout="block" />
          </div>
        ) : null}
        {/* This tool's own refusal only: the update that failed to start
            may have been pressed in another tool's drawer. */}
        {confirm.pageErrors
          .filter((item) => artifactKeyId(item.candidate.key) === id)
          .map((item) => {
            const text = t("updates.planFailed", { message: item.planError });
            return (
              <Refusal
                key={id}
                text={text}
                detail={item.planErrorDetail}
                detailTitle={text}
                className="mt-4"
              />
            );
          })}
      </Drawer>
    );
  };

  const filterChip = (key: string, label: ReactNode, count: number, pressed: boolean, onPress: () => void) => (
    <button
      key={key}
      type="button"
      aria-pressed={pressed}
      onClick={onPress}
      className={`inline-flex h-6 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-full border px-2 text-small font-medium outline-none transition-colors focus-visible:ring-2 focus-visible:ring-accent ${
        pressed
          ? "border-accent bg-accent text-accent-foreground"
          : "border-border bg-surface text-foreground hover:bg-hover"
      }`}
    >
      {label}{" "}
      <span className={`font-normal tabular-nums ${pressed ? "text-accent-foreground/80" : "text-muted"}`}>{count}</span>
    </button>
  );

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 flex-col gap-2.5 px-6 pb-3">
        <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
          <div className="relative min-w-40 max-w-sm flex-1">
            <SearchIcon
              size={15}
              className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted"
            />
            <input
              type="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t("installed.filterPlaceholder")}
              aria-label={t("installed.filterLabel")}
              className="h-8 w-full rounded-button border border-border bg-surface pl-8 pr-2.5 text-body text-foreground outline-none placeholder:text-muted focus-visible:ring-2 focus-visible:ring-accent"
            />
          </div>
          <p role="status" className="text-small text-muted">
            {copyStatus === "copied"
              ? t("common.copied")
              : copyStatus === "failed"
                ? t("common.copyFailed")
                : null}
          </p>
          <div role="group" aria-label={t("installed.sortLabel")} className="ml-auto flex items-center gap-2">
            <span aria-hidden="true" className="text-small text-muted">
              {t("installed.sortLabel")}
            </span>
            <div className="flex rounded-button bg-hover p-0.5">
              {(["name", "source"] as const).map((option) => (
                <button
                  key={option}
                  type="button"
                  aria-pressed={sort === option}
                  onClick={() => setSort(option)}
                  className="rounded-[6px] px-2.5 py-1 text-small font-medium text-muted outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-accent aria-pressed:bg-surface aria-pressed:text-foreground aria-pressed:shadow-sm"
                >
                  {t(option === "name" ? "installed.sortByName" : "installed.sortBySource")}
                </button>
              ))}
            </div>
          </div>
        </div>
        {filterSources.length > 0 ? (
          <ChipRow label={t("installed.filterBySource")}>
            {filterChip("all", t("installed.all"), snapshot.artifacts.length, activeFilter === null, () =>
              setFilter(null),
            )}
            {filterSources.map((instance) =>
              filterChip(
                instance.id,
                <>
                  <SourceAvatar adapterId={instance.adapter_id} label={labelOf(instance)} size="xs" />
                  {labelOf(instance)}
                </>,
                countByInstance.get(instance.id) ?? 0,
                activeFilter === instance.id,
                () => setFilter(instance.id),
              ),
            )}
          </ChipRow>
        ) : null}
      </div>
      {notices.length > 0 || untested.length > 0 ? (
        <div className="flex shrink-0 flex-col gap-1.5 px-6 pb-3">
          <SourceNotices notices={notices} layout="line" />
          {untested.map((instance) => {
            const title = t("installed.unverifiedVersion", {
              source: labelOf(instance),
              version: instance.unverified_version ?? "",
            });
            return (
              <SourceNoticeLine
                key={`${instance.id}:untested`}
                variant="info"
                title={title}
                description={t("installed.unverifiedVersionDetail")}
                detailsLabel={t("common.details")}
                detailsAriaLabel={t("common.detailsLabel", { title })}
              />
            );
          })}
        </div>
      ) : null}
      {/* Virtualized: a Mac with Homebrew's components unfolded lists
          hundreds of rows. */}
      <div ref={listRef} className="min-h-0 flex-1 overflow-y-auto px-3 pb-4">
        {items.length === 0 ? (
          <p className="px-3 py-10 text-center text-body text-muted">
            {needle !== ""
              ? t("installed.noMatches", { query: query.trim() })
              : t("emptyStates.nothingInstalled.title")}
          </p>
        ) : (
          <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
            {virtualizer.getVirtualItems().map((virtualRow) => {
              const item = items[virtualRow.index];
              return (
                // No fixed height on the slot: each reports its real
                // height back through `measureElement` instead.
                <div
                  key={virtualRow.key}
                  data-index={virtualRow.index}
                  data-list-slot=""
                  ref={virtualizer.measureElement}
                  style={{
                    position: "absolute",
                    top: 0,
                    left: 0,
                    width: "100%",
                    transform: `translateY(${virtualRow.start}px)`,
                  }}
                >
                  {item.type === "heading" ? (
                    <h2 className="flex items-center gap-2 px-3 pb-1.5 pt-4 text-body font-semibold text-foreground">
                      <SourceAvatar adapterId={item.instance.adapter_id} label={item.label} size="xs" />
                      {item.label}{" "}
                      <span className="font-normal tabular-nums text-muted">{item.count}</span>
                    </h2>
                  ) : item.type === "fold" ? (
                    <div className="pt-1">
                      <button
                        type="button"
                        aria-expanded={item.expanded}
                        onClick={() => toggleDependencies(item.instance.id)}
                        className="flex w-full items-center gap-1.5 rounded-button px-3 py-2 text-left text-body text-muted outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-accent"
                      >
                        <ChevronIcon
                          size={14}
                          className={`shrink-0 transition-transform ${item.expanded ? "rotate-90" : ""}`}
                        />
                        {t(item.expanded ? "installed.hideDependencies" : "installed.showDependencies", {
                          count: item.count,
                        })}{" "}
                        {mixed ? (
                          <span className="shrink-0 rounded-full border border-border px-1.5 text-[11px] leading-4 text-muted">
                            {item.label}
                          </span>
                        ) : null}
                      </button>
                    </div>
                  ) : (
                    toolRow(item.artifact, item.instance, item.label)
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>
      {uninstallTarget ? (
        <UninstallDialog
          open
          onOpenChange={(open) => {
            if (!open) setUninstallTarget(null);
          }}
          request={uninstallTarget.request}
          displayName={uninstallTarget.displayName}
          returnFocusTo={uninstallOpener}
          onSubmitted={(opId) => {
            startedUninstall.current = opId;
            setUninstallTarget(null);
          }}
          onClosed={() => {
            const opId = startedUninstall.current;
            startedUninstall.current = null;
            if (opId !== null) openLog(opId);
          }}
        />
      ) : null}
      <UpdateConfirmDialog confirm={confirm} />
      {details !== undefined && detailsInstance !== undefined ? detailsDrawer(details, detailsInstance) : null}
    </div>
  );
}
