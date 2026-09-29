import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import type { KeyboardEvent, ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useCheckAgain, useOperations, useSettings, useSnapshot } from "../lib/queries";
import { artifactKeyId, useUiStore } from "../store/ui";
import {
  ADAPTER_LABEL_KEYS,
  canWrite,
  describeTool,
  instanceLabels,
  isAvailable,
  sourceNoticesFor,
  type SourceNoticeSpec,
  type ToolDescriptionLines,
  uninstallBlockedCopy,
  uninstallHoldKey,
  UPDATE_BLOCKED_KEYS,
  sourceWarningOf,
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
import { formatBytes } from "../lib/format";
import { useTranslatedDescription } from "../lib/toolDescriptions";
import { nameKey, namesUnderSeveralSources } from "../lib/names";
import type { InstalledArtifact, ManagerInstance, OpRequest, UpdateCandidate } from "../lib/types";
import { RowAction, ToolRow } from "../components/ToolRow";
import { StatusChip } from "../components/StatusChip";
import { Menu, type MenuItem } from "../components/ui/Menu";
import { ToolbarPopupButton } from "../components/ui/PopupButton";
import { SourceNotices, useNoticeFold } from "../components/SourceNotices";
import { SourceAvatar } from "../components/SourceAvatar";
import { ToolAvatar } from "../components/ToolAvatar";
import { UninstallDialog } from "../components/UninstallDialog";
import { UpdateConfirmDialog, useUpdateConfirm } from "../components/UpdateConfirm";
import { isRetryable, progressOf, UpdateProgress, useUpdateOperationFor } from "../components/UpdateProgress";
import { useElementWidth, VirtualList, type VirtualListHandle } from "../components/VirtualList";
import { ToolbarItems } from "../components/Toolbar";
import { useRovingRow } from "../components/rovingRows";
import { FirstCheck } from "../components/StatusRing";
import {
  blockedDetail,
  cannotCheckDetail,
  detailLines,
  readOnlyDetail,
  unavailableDetail,
  updateVersionColumn,
} from "../components/updateDetails";
import { COMMAND_SLOT, withCommand } from "../components/withCommand";
import { Refusal } from "../components/SheetParts";
import { CheckIcon, CloseIcon, DisclosureIcon, InfoIcon, SearchIcon, WarningIcon } from "../components/icons";
import { EmptyState } from "../components/EmptyState";
import { BUTTON, ICON_BUTTON } from "../components/ui/controls";

// The virtualizer's first guesses: a row, a source's heading (sorted by
// source), a "N more components" line and the notices' line. Each slot
// then measures itself through `measureElement`.
const ROW_ESTIMATE = 52;
const HEADING_ESTIMATE = 40;
const FOLD_ESTIMATE = 32;
const NOTICES_ESTIMATE = 32;

/** The inspector's width (spec R11), and the hairline to its left. */
const INSPECTOR_WIDTH = 300 + 1;
/**
 * The narrowest the list may be beside the inspector: a row's avatar,
 * name, button and ⋯ with room for a name such as 「Android SDK
 * Platform-Tools」 uncut (`ToolRow`'s `minimal` fit). A window whose page
 * is narrower than this and the inspector -- the 800 at its narrowest --
 * has the inspector lie over the list's right side instead.
 */
const LIST_BESIDE_INSPECTOR = 440;

/** An update the user hid on the Updates page, and how (`hidingRule`). */
interface HiddenUpdate {
  by: HiddenBy;
  candidate: UpdateCandidate;
}

/**
 * One of a row's status words: its word, the why behind its ⓘ (on the
 * row) or under it (in the inspector), and what kind it is -- what the row is
 * and why it can't do something, an update to be had, or up to date. The
 * last two are normal states, which a row does not put in words (spec
 * §3.4): the version column says the first, and silence the second. The
 * inspector lists every one.
 */
interface RowChip {
  id: string;
  label: string;
  detail?: ReactNode;
  /** What the inspector says under the word when the row's ⓘ says nothing: a model's new version. */
  inspectorDetail?: ReactNode;
  tone: "neutral" | "update" | "upToDate";
}

/**
 * The one word a row shows (spec §3.4: one at most): the first of its
 * chips that is not a normal state, in `chipsOf`'s order -- what the
 * source allows, then the tool's own refusal to be removed, why its
 * Uninstall waits, then where its update stands, then how the user hid it.
 */
function rowChipOf(chips: RowChip[]): RowChip | undefined {
  return chips.find((chip) => chip.tone === "neutral");
}

/**
 * One slot in the virtualized list: first, while there is anything to
 * say, what the sources had to say about this check -- the list's first
 * row, which scrolls away with it, as on the Updates page (spec §3.8);
 * then a tool's row; a source's heading, only when the list is sorted by
 * source and shows every source; and a source's "N more components came
 * with other software" line, which unfolds its components under it.
 */
type ListItem =
  | { type: "notices" }
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
    case "notices":
      return "notices";
    case "heading":
      return `heading:${item.instance.id}`;
    case "fold":
      return `fold:${item.instance.id}`;
    case "row":
      return artifactKeyId(item.artifact.key);
  }
}

/** A slot's first guess at its height. */
function estimateSize(item: ListItem): number {
  if (item.type === "heading") return HEADING_ESTIMATE;
  if (item.type === "fold") return FOLD_ESTIMATE;
  if (item.type === "notices") return NOTICES_ESTIMATE;
  return ROW_ESTIMATE;
}

/** The slots ↑ and ↓ move between (`VirtualList`'s `keyboardRows`): the rows, and the lines that unfold components. */
function keyboardRow(item: ListItem): boolean {
  return item.type === "row" || item.type === "fold";
}

/**
 * The line that unfolds a source's components, 32 high: a 10pt triangle
 * and the words, muted -- the Updates page's 「另有5个无法在这里更新」's
 * look (spec §3.3) -- and, where the list mixes sources with no heading
 * to say it, the source's name after them. One of the rows ↑ and ↓ move
 * between, Space or Enter unfolding it.
 */
function FoldLine({
  count,
  expanded,
  source,
  onToggle,
}: {
  count: number;
  expanded: boolean;
  source: string | null;
  onToggle: () => void;
}) {
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
      className="flex h-8 w-full items-center gap-1.5 px-5 text-left text-body text-muted -outline-offset-3"
    >
      <DisclosureIcon size={10} className={`shrink-0 ${expanded ? "rotate-90" : ""}`} />
      <span className="min-w-0 truncate">
        {t(expanded ? "installed.hideDependencies" : "installed.showDependencies", { count })}
      </span>{" "}
      {source !== null ? <span className="shrink-0 text-small text-muted">{source}</span> : null}
    </button>
  );
}

/** An absolute date in the user's language: the day a tool was installed, which "3 days ago" would blur. */
function formatDate(seconds: number, language: string): string {
  return new Intl.DateTimeFormat(language, { dateStyle: "medium" }).format(new Date(seconds * 1000));
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

/** A chip's word on a row, or in the inspector: up to date with a quiet tick before it. */
function RowChipView({ chip, withDetail }: { chip: RowChip; withDetail: boolean }) {
  if (chip.tone === "upToDate") {
    return (
      <span className="inline-flex items-center gap-1 whitespace-nowrap text-small text-muted">
        <CheckIcon size={13} className="shrink-0 text-success" />
        {chip.label}
      </span>
    );
  }
  return <StatusChip label={chip.label} detail={withDetail ? chip.detail : undefined} />;
}

/**
 * The page on one source that has nothing to list (spec R8): why, in the
 * words of its first warning -- 「uv没有响应」 over 「uv没有响应，无法列出
 * 它安装的内容。」 -- or, for a source that answered, that nothing is
 * installed with it; and Check again, the header's, which shows what it
 * has once it answers or has something. In the list's place, so the
 * source's notice is not said a second time over it.
 */
function SourceEmpty({ instance, label }: { instance: ManagerInstance; label: string }) {
  const { t } = useTranslation();
  const { checkAgain, checking } = useCheckAgain();
  const warning = sourceWarningOf(instance, label, 0);
  return (
    <EmptyState
      icon={
        warning === null ? (
          <InfoIcon size={36} className="text-tertiary" />
        ) : (
          <WarningIcon size={36} className="text-tertiary" />
        )
      }
      title={
        warning === null ? t("installed.sourceEmpty.title", { source: label }) : t(warning.titleKey, warning.values)
      }
      description={
        warning === null
          ? t("installed.sourceEmpty.description", { source: label })
          : t(warning.descriptionKey, warning.values)
      }
      action={{ label: t("header.checkAgain"), onClick: checkAgain, disabled: checking }}
    />
  );
}

/**
 * 已安装: everything the sources list, to find and to uninstall
 * (docs/superpowers/2026-09-27-ui-redesign.md, 已安装页;
 * docs/superpowers/2026-09-29-aesthetics-spec.md §3.3, R8, R11).
 *
 * Its search field and its sort are in the window's toolbar, as a Mac
 * app's are (`ToolbarItems`); which source it shows is the sidebar's to
 * say -- 「已安装」 for every one, a source's row under 「来源」 for that
 * one alone (`installedFilter`), whose name then titles the window.
 *
 * Then one list, its first line what the sources had to say this time
 * (`SourceNoticeLine`, as on the Updates page), folded into one while
 * there are two or more (`SourceNotices`). By name, it is one flat list,
 * each row naming its source with the avatar's mark and, where two
 * sources list one name, in words: a tool is found by its name. By
 * source, it is grouped under a heading per source -- only while every
 * source is shown; one source's list needs none. Either way, what other
 * software brought in is folded into one line per source,
 * 「另有14个随其他软件安装的组件」, which unfolds them under it.
 *
 * Each row: what it is, its version, one status word -- the why behind
 * its ⓘ -- Uninstall where the source and the tool allow it (disabled,
 * with a word saying why, while the source refuses one for now), and a ⋯
 * menu. Pressing the row, or moving to it with ↑ ↓, selects it, and the
 * inspector on the right shows it: everything a row has no room for, and
 * its Update (`Inspector`). Not a dialog: the list stays in reach beside
 * it, and Escape or pressing the row again closes it.
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
  // A tool's line in the window's language: Chinese in Chinese, and
  // English in English for an npm, PyPI or crates.io package.
  const translatedDescription = useTranslatedDescription();
  // The toolbar's search field, once it is drawn there.
  const [searchBox, setSearchBox] = useState<HTMLInputElement | null>(null);
  const searchFocusRequested = useUiStore((s) => s.searchFocusRequested);
  const searchFocused = useUiStore((s) => s.searchFocused);

  // Uninstall is destructive, so a button only *targets* an artifact;
  // UninstallDialog is what plans it, shows what it would change and what
  // would break, with the exact command a click away, and submits (Global
  // Constraints, spec §6).
  const [uninstallTarget, setUninstallTarget] = useState<{
    request: OpRequest;
    displayName: string;
  } | null>(null);
  // What opened the uninstall dialog -- a row's Uninstall, or the
  // inspector's -- which gets the focus back when it closes.
  const uninstallOpener = useRef<HTMLElement | null>(null);
  // The operation an uninstall just started. Its log opens once the dialog
  // has closed and given the focus back to what opened it, so that the log
  // drawer, which hands the focus back to what had it as it opened, hands
  // it back there too.
  const startedUninstall = useRef<number | null>(null);
  // The row selected, by artifact key id, and the source the page showed
  // when it was: looked up in the snapshot each time, so the inspector
  // shows what the last check found, and kept through a check by the
  // tool's key (spec R11). Another source in the sidebar starts with
  // nothing selected.
  const [selection, setSelection] = useState<{ id: string; filter: string | null } | null>(null);
  const listHandle = useRef<VirtualListHandle | null>(null);
  // The page's width: whether the inspector fits beside the list or lies
  // over its right side (`LIST_BESIDE_INSPECTOR`).
  const [pageBox, setPageBox] = useState<HTMLDivElement | null>(null);
  const pageWidth = useElementWidth(pageBox);

  const showTechnicalDetails = settings?.show_technical_details ?? false;
  const inspectorTitleId = useId();

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
  // A tool that is gone -- uninstalled, or no longer listed -- is no
  // longer selected, for good, so the same name listed again later does
  // not open the inspector.
  useEffect(() => {
    if (snapshot && selection !== null && !artifactsById.has(selection.id)) setSelection(null);
  }, [snapshot, selection, artifactsById]);

  // The source's name in the user's language, as the sidebar lists it --
  // with where it is after it where this Mac has two of its kind
  // (`instanceLabels`): the headings, the rows' avatars and words, the
  // `{{source}}` in a sentence.
  const labels = useMemo(() => instanceLabels(t, snapshot?.instances ?? []), [t, snapshot]);
  const labelOf = useCallback(
    (instance: ManagerInstance): string => {
      const label = labels.get(instance.id);
      if (label !== undefined) return label;
      const labelKey = ADAPTER_LABEL_KEYS[instance.adapter_id];
      return labelKey ? t(labelKey) : instance.adapter_id;
    },
    [labels, t],
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

  // The Updates page's own confirmation, for the inspector's Update: the same
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

  // How much each source has installed: the number its row in the
  // sidebar shows.
  const countByInstance = useMemo(() => {
    const counts = new Map<string, number>();
    for (const artifact of snapshot?.artifacts ?? []) {
      const id = artifact.key.instance_id;
      counts.set(id, (counts.get(id) ?? 0) + 1);
    }
    return counts;
  }, [snapshot]);

  // The source the sidebar's row for it opened the page on. It stays,
  // with nothing installed or with nothing its source could list, and
  // says why (`sourceEmpty`): never reset to everything behind the
  // user's back (spec R8), which would leave the sidebar's row selected
  // over a list of every source's. Only a source this Mac no longer has
  // -- gone from the snapshot, and from the sidebar -- is dropped, and
  // the page shows everything under 「已安装」 again.
  const activeFilter = filter !== null && instancesById.has(filter) ? filter : null;
  useEffect(() => {
    if (snapshot && filter !== null && activeFilter === null) setFilter(null);
  }, [snapshot, filter, activeFilter, setFilter]);
  const selectedId = selection !== null && selection.filter === activeFilter ? selection.id : null;
  // Another source, or every source again: nothing selected, for good.
  useEffect(() => {
    setSelection((was) => (was === null || was.filter === activeFilter ? was : null));
  }, [activeFilter]);
  // Headings only while the list is sorted by source and shows every
  // source; a "N more components" line names its source only where the
  // list mixes sources and has no heading saying it.
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

  const rowItems = useMemo<ListItem[]>(() => {
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

  // The names the list shows under more than one source (spec R3), whose
  // rows say their source's name after the tool's.
  const namedTwice = useMemo(
    () =>
      namesUnderSeveralSources(
        rowItems.flatMap((item) =>
          item.type === "row" ? [{ name: item.artifact.display_name, instanceId: item.instance.id }] : [],
        ),
      ),
    [rowItems],
  );

  // What each source in view has to say about this check, a line each
  // (`sourceNoticesFor`, the rule the Updates page and the Overview read):
  // not running, not answering, a list it could not download, another
  // copy that runs instead. Whether a silent source has rows here is part
  // of what its notice says -- "what's listed for uv is from the last time
  // it answered" over rows it has, "can't show what it has installed" over
  // none -- and a search that hides its rows does not make it have none.
  // Then a source with rows here whose version Canager has not been
  // tested with. Two lines or more fold into one (`SourceNotices`).
  const notices = useMemo(
    () => [
      ...instancesInView.flatMap((instance) =>
        sourceNoticesFor(instance, labelOf(instance), countByInstance.get(instance.id) ?? 0),
      ),
      ...instancesInView
        .filter((instance) => instance.unverified_version !== null && (countByInstance.get(instance.id) ?? 0) > 0)
        .map(
          (instance): SourceNoticeSpec => ({
            id: `${instance.id}:untested`,
            variant: "info",
            titleKey: "installed.unverifiedVersion",
            descriptionKey: "installed.unverifiedVersionDetail",
            values: { source: labelOf(instance), version: instance.unverified_version ?? "" },
          }),
        ),
    ],
    [instancesInView, labelOf, countByInstance],
  );
  const noticeFold = useNoticeFold(notices.length);
  // The notices are the list's first line while it has rows to be the
  // first of; with none, they stand over the sentence that says so.
  const items = useMemo<ListItem[]>(
    () => (rowItems.length > 0 && notices.length > 0 ? [{ type: "notices" }, ...rowItems] : rowItems),
    [rowItems, notices.length],
  );

  // The menu bar's Search (⌘F, `searchInstalled`): the field takes the
  // focus as soon as it is in the toolbar, its text selected to be typed
  // over, wherever the focus was. Still loading, or before the toolbar
  // has its slot, the field is not there yet, and the request waits for it.
  useEffect(() => {
    if (!searchFocusRequested || searchBox === null) return;
    searchBox.focus();
    searchBox.select();
    searchFocused();
  }, [searchFocusRequested, searchBox, searchFocused]);

  if (isLoading) {
    // Before `get_snapshot` answers, the first check is under way too:
    // the same view `SnapshotStatus` shows once it has.
    return <FirstCheck />;
  }
  if (!snapshot) {
    return null;
  }

  // Uninstall where both the source and the tool allow it: a source that
  // is read-only offers none, nor does a package the tool refuses to
  // remove (`uninstall_blocked`) -- each row says why with its own chip.
  // `Session::issue_plan` refuses both in Rust whatever this page shows
  // (spec §2.5).
  const canUninstall = (artifact: InstalledArtifact, instance: ManagerInstance): boolean =>
    canWrite(instance) && artifact.uninstall_blocked === null;
  // Why such an Uninstall stays, disabled, for now, or null when it does
  // not: the source did not answer the last check -- its rows are last
  // time's, carried forward, and `Session::issue_plan` refuses to plan on
  // it (spec §2.5) -- which says what to do by why it did not
  // (`unavailableDetail`), or it refuses to plan one until a note of its
  // goes away -- a Homebrew updating its list (`uninstallHoldKey`). The
  // row's 「暂时不能卸载」 chip says it, the same chip for both; the button
  // comes back with the check that finds the source answering, or clears
  // the note.
  const uninstallHoldDetail = (
    artifact: InstalledArtifact,
    instance: ManagerInstance,
    label: string,
  ): ReactNode | null => {
    if (!canUninstall(artifact, instance)) return null;
    if (!isAvailable(instance)) return unavailableDetail(t, instance, label);
    const holdKey = uninstallHoldKey(instance);
    return holdKey === null ? null : detailLines([t(holdKey)]);
  };
  // Held as above, or while an uninstall of this one is under way.
  const uninstallHeld = (artifact: InstalledArtifact, instance: ManagerInstance): boolean =>
    canUninstall(artifact, instance) &&
    (!isAvailable(instance) || uninstallHoldKey(instance) !== null || uninstallUnderway(artifact) !== null);
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

  // Selects a row, and the inspector shows it: its ⋯ menu's Details, and
  // ↑ ↓ (`VirtualList`'s `onKeyboardMove`).
  const select = (artifact: InstalledArtifact) =>
    setSelection({ id: artifactKeyId(artifact.key), filter: activeFilter });
  // Pressing a row: selects it, or -- the one selected -- closes the
  // inspector, the focus staying on the row (`ToolRow` put it there).
  const pressRow = (artifact: InstalledArtifact) => {
    const id = artifactKeyId(artifact.key);
    setSelection(id === selectedId ? null : { id, filter: activeFilter });
  };
  // Escape: the inspector closes, and the focus goes back to its row,
  // wherever it was -- on the row, or in the inspector.
  const closeInspector = () => {
    const id = selectedId;
    setSelection(null);
    if (id !== null) listHandle.current?.focusKey(id);
  };
  // Escape inside the list or the inspector, when nothing nearer has it: a
  // status word's ⓘ, the ⋯ menu and a text field close or clear with it
  // themselves (a dialog opened from here is outside both, in React's
  // tree as in the page's).
  const onEscape = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key !== "Escape" || event.defaultPrevented || selectedId === null) return;
    if (document.querySelector("[data-popup-open]") !== null) return;
    if ((event.target as HTMLElement).closest('[role="menu"], input:not([type="checkbox"]), textarea') !== null) return;
    event.preventDefault();
    closeInspector();
  };

  // What a row says it is, and what its details show under that: the
  // source's own words, where the line is their translation.
  const describe = (artifact: InstalledArtifact, instance: ManagerInstance, label: string): ToolDescriptionLines =>
    describeTool(
      t,
      {
        description: artifact.description,
        translated: translatedDescription(artifact.key, instance.adapter_id),
        kind: artifact.key.kind,
        path: artifact.path,
      },
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
    const holdDetail = uninstallHoldDetail(artifact, instance, label);
    if (holdDetail !== null) {
      chips.push({
        id: "uninstall-held",
        label: t("installed.uninstallHold.label"),
        detail: holdDetail,
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
            // The inspector's facts give the version it moves to, except a
            // model's, which has no version to give: "a new version".
            inspectorDetail: listed.channel === "Digest" ? t("updates.newBuild") : undefined,
            tone: "update",
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
            // The Updates page's chip for the same row, word for word.
            label: t("updates.sourceUnavailable"),
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
      { id: "details", label: t("common.details"), onSelect: () => select(artifact) },
    ];
    const command = commandOf(artifact, instance);
    if (showTechnicalDetails && command !== null) {
      items.push({ id: "copy", label: t("common.copyCommand"), onSelect: () => copyCommand(command) });
    }
    return items;
  };

  const toolRow = (artifact: InstalledArtifact, instance: ManagerInstance, label: string) => {
    const name = artifact.display_name;
    const chip = rowChipOf(chipsOf(artifact, instance, label));
    // Where an update is listed, the version it moves to, as the Updates
    // page's row says it ("7.1 → 7.2"), in place of an "Update available"
    // word; a hidden one leaves the version installed.
    const listed = listedUpdates.get(artifactKeyId(artifact.key));
    const change = listed !== undefined && listed.checkable ? updateVersionColumn(t, listed) : null;
    return (
      <ToolRow
        adapterId={instance.adapter_id}
        sourceLabel={label}
        // The tool's logo, and a cask's app's own icon once it arrives.
        iconKey={artifact.key}
        name={name}
        showSource={namedTwice.has(nameKey(name))}
        description={describe(artifact, instance, label).line}
        status={chip === undefined ? undefined : <RowChipView chip={chip} withDetail />}
        version={change?.version ?? versionOf(artifact)}
        newVersion={change?.newVersion}
        action={
          canUninstall(artifact, instance) ? (
            <RowAction
              disabled={uninstallHeld(artifact, instance)}
              onClick={(event) => uninstall(artifact, event.currentTarget)}
            >
              {uninstallUnderway(artifact) ?? t("installed.uninstall")}
            </RowAction>
          ) : null
        }
        menu={<Menu label={t("common.moreActions", { name })} items={menuItems(artifact, instance)} />}
        onOpen={() => pressRow(artifact)}
        openLabel={t("common.detailsLabel", { title: name })}
        selected={artifactKeyId(artifact.key) === selectedId}
      />
    );
  };

  // The log drawer at the foot of the window, for an operation just
  // started or finished. The inspector stays: it is no dialog, and the
  // log opens over the page beside it.
  const openLog = (opId: number) => {
    setFocusedOpId(opId);
    setDrawerOpen(true);
  };

  const details = selectedId === null ? undefined : artifactsById.get(selectedId);
  const detailsInstance = details === undefined ? undefined : instancesById.get(details.key.instance_id);

  /**
   * The inspector (spec R11; cork-package-info.png): a pane on the right,
   * 300 wide and the page's full height, the list narrowed beside it -- no
   * dialog, no dimming, the list still in reach. From the top: the tool's
   * icon at 48, its name and its source; all of its description, and
   * under it, quieter, the source's own words where the line is their
   * translation; its facts -- its version and the one an update would
   * bring, when it was installed, its size, and where it is (with
   * technical details on) -- each one selectable, to be copied; every
   * status word with its why in full; what its source had to say this
   * time; and at its foot what can be done -- Uninstall, and Update where
   * the Updates page offers one, through that page's own confirmation;
   * while that update runs, its progress where the button was, and once it
   * has ended without updating, how it ended beside Retry.
   *
   * Where the page is too narrow for it and a list whose names stay whole
   * (`LIST_BESIDE_INSPECTOR`) -- the 800 of the window at its narrowest --
   * it lies over the list's right side instead, with a floating thing's
   * shadow along its edge, and the list keeps its width under it.
   */
  const inspector = (artifact: InstalledArtifact, instance: ManagerInstance, overlay: boolean) => {
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
    // Each value selects (`select-text` on its `<dd>`), to be copied: a
    // version into a search, a path into Terminal or Finder's Go to Folder.
    const facts: Array<{ term: string; value: ReactNode }> = [];
    if (version !== null) facts.push({ term: t("installed.version"), value: version });
    if (newer !== null) facts.push({ term: t("installed.newVersion"), value: newer });
    if (artifact.installed_at !== null) {
      facts.push({ term: t("installed.installedOn"), value: formatDate(artifact.installed_at, i18n.language) });
    }
    if (artifact.size_bytes !== null) {
      facts.push({ term: t("installed.size"), value: formatBytes(artifact.size_bytes) });
    }
    // Where it is, only while technical details are on, and only where the
    // source said: an app's bundle, a program's file, a tool's own folder.
    if (showTechnicalDetails && artifact.path !== null) {
      facts.push({
        term: t("installed.location"),
        value: <code className="break-all font-mono text-small">{artifact.path}</code>,
      });
    }
    const sourceNotices = sourceNoticesFor(instance, label, countByInstance.get(instance.id) ?? 0);
    const { line, original } = describe(artifact, instance, label);
    const removable = canUninstall(artifact, instance);
    const refusals = confirm.pageErrors.filter((item) => artifactKeyId(item.candidate.key) === id);
    return (
      <aside
        aria-labelledby={inspectorTitleId}
        data-inspector={overlay ? "overlay" : "beside"}
        onKeyDown={onEscape}
        className={`flex w-75 shrink-0 flex-col bg-content ${
          overlay ? "absolute inset-y-0 right-0 z-20 shadow-menu" : "border-l border-separator"
        }`}
      >
        <div className="min-h-0 flex-1 overflow-y-auto px-5 pb-4 pt-5">
          <div className="flex items-center gap-3">
            <ToolAvatar adapterId={instance.adapter_id} sourceLabel={label} iconKey={artifact.key} size="lg" />
            <div className="min-w-0 flex-1">
              <h2 id={inspectorTitleId} className="break-words text-title text-foreground">
                {name}
              </h2>
              {label === name ? null : <p className="truncate text-small text-muted">{label}</p>}
            </div>
            {/* Escape and pressing the row again close it too; this is
                the way that shows. */}
            <button
              type="button"
              aria-label={t("common.close")}
              onClick={closeInspector}
              className={`${ICON_BUTTON} -mr-2 self-start`}
            >
              <CloseIcon size={16} />
            </button>
          </div>
          <p className="mt-4 break-words text-body-long text-foreground">{line}</p>
          {original !== null ? (
            <p data-original-description="" className="mt-1 break-words text-small text-muted">
              {original}
            </p>
          ) : null}
          {facts.length > 0 ? (
            // Labels 72 wide -- wider only for one that would wrap, such as
            // "Date Installed" -- as a Mac's info pane lines its values up.
            <dl className="mt-4 grid grid-cols-[minmax(4.5rem,auto)_1fr] gap-x-3 gap-y-1.5 text-body">
              {facts.map((fact) => (
                <div key={fact.term} className="contents">
                  <dt className="whitespace-nowrap text-muted">{fact.term}</dt>
                  <dd className="min-w-0 select-text break-words tabular-nums text-foreground">{fact.value}</dd>
                </div>
              ))}
            </dl>
          ) : null}
          {chips.length > 0 ? (
            <ul data-status-list="" className="mt-4 flex flex-col gap-3">
              {chips.map((chip) => (
                <li key={chip.id} className="flex flex-col items-start gap-1">
                  <RowChipView chip={chip} withDetail={false} />
                  {chip.detail !== undefined ? (
                    <div className="text-body-long text-foreground">{chip.detail}</div>
                  ) : chip.inspectorDetail !== undefined ? (
                    <p className="break-words text-body-long text-foreground">{chip.inspectorDetail}</p>
                  ) : null}
                </li>
              ))}
            </ul>
          ) : null}
          {sourceNotices.length > 0 ? (
            <div className="mt-4 flex flex-col gap-2">
              <SourceNotices notices={sourceNotices} layout="block" />
            </div>
          ) : null}
          {/* This tool's own refusal only: the update that failed to start
              may have been pressed for another tool. */}
          {refusals.map((item) => {
            const text = t("updates.planFailed", { message: item.planError });
            return <Refusal key={id} text={text} detail={item.planErrorDetail} detailTitle={text} className="mt-4" />;
          })}
        </div>
        {removable || updatable ? (
          // Uninstall at the left, grey -- offered, not recommended, and
          // not red (`RowAction`) -- and Update, the default, at the right,
          // as a Mac's dialog footer sets them.
          <div className="flex shrink-0 items-center gap-2 px-5 pb-5 pt-3">
            {removable ? (
              <button
                type="button"
                disabled={uninstallHeld(artifact, instance)}
                onClick={(event) => uninstall(artifact, event.currentTarget)}
                className={BUTTON.regular.grey}
              >
                {uninstallUnderway(artifact) ?? t("installed.uninstall")}
              </button>
            ) : null}
            <span className="flex-1" />
            {progress !== null ? <UpdateProgress progress={progress} name={name} onViewLog={openLog} /> : null}
            {/* As on the Updates page's row: an update that ended without
                updating keeps how it ended, with Retry in Update's place. */}
            {updatable && listed !== undefined && (progress === null || isRetryable(progress)) ? (
              <button
                type="button"
                onClick={(event) => void confirm.openConfirm([listed], event.currentTarget)}
                disabled={confirm.dialogOpen}
                className={BUTTON.regular.default}
              >
                {progress === null ? t("updates.update") : t("updates.retry")}
              </button>
            ) : null}
          </div>
        ) : null}
      </aside>
    );
  };

  // The page on one source that has nothing to list says why in the
  // list's place (`SourceEmpty`), and its notice is not said over it.
  const sourceEmpty = activeFilter !== null && (countByInstance.get(activeFilter) ?? 0) === 0;
  // The inspector beside the list, or -- the page too narrow for both --
  // over its right side. Not measured (jsdom), beside.
  const overlay = pageWidth !== null && pageWidth - INSPECTOR_WIDTH < LIST_BESIDE_INSPECTOR;

  return (
    <div ref={setPageBox} className="relative flex h-full">
      {/* The page's own controls, in the window's toolbar (spec §3.2):
          how the last Copy command went, for a moment; the sort, a grey
          popup button; and the search field, 200 wide. */}
      <ToolbarItems>
        <p role="status" className="max-w-40 truncate text-small text-muted empty:hidden">
          {copyStatus === "copied" ? t("common.copied") : copyStatus === "failed" ? t("common.copyFailed") : null}
        </p>
        <ToolbarPopupButton
          label={t("installed.sortLabel")}
          value={sort}
          options={[
            { value: "name", label: t("installed.sortByName") },
            { value: "source", label: t("installed.sortBySource") },
          ]}
          onChange={setSort}
        />
        <span className="relative flex h-6 w-50 shrink-0 items-center">
          <SearchIcon size={14} className="pointer-events-none absolute left-2 text-muted" />
          <input
            ref={setSearchBox}
            type="search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("installed.filterPlaceholder")}
            aria-label={t("installed.filterLabel")}
            // A tool's name is no word: no red underline under "ffmpeg",
            // as a web page's text field would draw, and nothing
            // corrected as it is typed.
            spellCheck={false}
            className="h-6 w-full appearance-none rounded-control bg-fill-subtle pl-7 pr-2 text-body text-foreground placeholder:text-muted [&::-webkit-search-decoration]:appearance-none"
          />
        </span>
      </ToolbarItems>
      <div className="flex min-w-0 flex-1 flex-col" onKeyDown={onEscape}>
        {/* Virtualized: a Mac with Homebrew's components unfolded lists
            hundreds of rows. */}
        <VirtualList
          items={items}
          itemKey={listItemKey}
          estimateSize={estimateSize}
          keyboardRows={keyboardRow}
          onKeyboardMove={(item) => {
            if (item.type === "row") select(item.artifact);
          }}
          handleRef={listHandle}
          anchorKey={selectedId}
          renderItem={(item) =>
            item.type === "notices" ? (
              <div className="px-5">
                <SourceNotices notices={notices} layout="line" fold={noticeFold} />
              </div>
            ) : item.type === "heading" ? (
              // A group's heading, as a Mac's grouped list sets one: 13
              // bold, how many in the secondary colour after it, and the
              // source's mark at 16 -- no pill.
              <h2 className="flex h-10 items-end gap-2 px-5 pb-2 text-title text-foreground">
                <SourceAvatar adapterId={item.instance.adapter_id} label={item.label} size="xs" />
                <span className="min-w-0 truncate">{item.label}</span>{" "}
                <span className="shrink-0 text-body font-normal tabular-nums text-muted">{item.count}</span>
              </h2>
            ) : item.type === "fold" ? (
              <FoldLine
                count={item.count}
                expanded={item.expanded}
                source={mixed ? item.label : null}
                onToggle={() => toggleDependencies(item.instance.id)}
              />
            ) : (
              toolRow(item.artifact, item.instance, item.label)
            )
          }
          empty={
            sourceEmpty ? (
              <SourceEmpty instance={instancesById.get(activeFilter)!} label={sourceLabelFor(activeFilter)} />
            ) : (
              <>
                {notices.length > 0 ? (
                  <div className="px-5">
                    <SourceNotices notices={notices} layout="line" fold={noticeFold} />
                  </div>
                ) : null}
                <p className="px-5 py-10 text-center text-body text-muted">
                  {needle !== ""
                    ? t("installed.noMatches", { query: query.trim() })
                    : t("emptyStates.nothingInstalled.title")}
                </p>
              </>
            )
          }
        />
      </div>
      {details !== undefined && detailsInstance !== undefined ? inspector(details, detailsInstance, overlay) : null}
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
    </div>
  );
}
