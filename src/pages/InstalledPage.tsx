import { useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent, ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useCheckAgain, useOperations, useSaveSettings, useSettings, useSnapshot } from "../lib/queries";
import { artifactKeyId, useUiStore } from "../store/ui";
import {
  ADAPTER_LABEL_KEYS,
  canWrite,
  instanceLabels,
  isAvailable,
  settingsSaveSentence,
  sourceNoticesFor,
  type SourceNoticeSpec,
  toolDescription,
  uninstallBlockedCopy,
  uninstallHoldKey,
  UPDATE_BLOCKED_KEYS,
  sourceWarningOf,
  unfinishedChecksNotice,
} from "../lib/sources";
import {
  hidingRule,
  leftOutOfUpdateCheck,
  shownSkippedVersion,
  skippedVersionId,
  updateStateOf,
  upToDateIsKnown,
} from "../lib/updateState";
import type { HiddenBy } from "../lib/updateState";
import { useCopyCommand } from "../lib/clipboard";
import { formatBytes } from "../lib/format";
import { useTranslatedDescription } from "../lib/toolDescriptions";
import { listedName, modelPath, nameKey, namesUnderSeveralSources } from "../lib/names";
import type { InstalledArtifact, ManagerInstance, OpRequest, OpSummary, UpdateCandidate } from "../lib/types";
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
import { useNarrowerThan, VirtualList, type VirtualListHandle } from "../components/VirtualList";
import { ToolbarItems } from "../components/Toolbar";
import { ToolShowButton } from "../components/ToolShowButton";
import { shownBy } from "../lib/families";
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
import { CloseIcon, DisclosureIcon, SearchIcon } from "../components/icons";
import { EmptyState } from "../components/EmptyState";
import { BUTTON, ICON_BUTTON } from "../components/ui/controls";
import { focusOrFallback } from "../components/ui/focus";
import { GROUP, SMALL_WRAPPING } from "../components/ui/group";
import { InfoDetail } from "../components/InfoDetail";
import {
  HOMEBREW_STATUS_CHIP_IDS,
  HomebrewNotes,
  homebrewStatusChip,
  homepageFact,
} from "../components/HomebrewStatus";
import { CommandsGroup, twinChip, useTwins } from "../components/CommandFacts";

// The virtualizer's first guesses: a row, a source's heading (sorted by
// source), a "N more components" line and the notices' line. Each slot
// then measures itself through `measureElement`.
const ROW_ESTIMATE = 52;
const HEADING_ESTIMATE = 40;
const FOLD_ESTIMATE = 32;
const NOTICES_ESTIMATE = 32;

/**
 * The page's width under which the inspector is 260 wide, not 300 (spec
 * R11): a window under 900 wide, less the sidebar's 208. At the window's
 * narrowest, 800, that leaves the list beside it 331 of the page's 592 --
 * a list of its own, with its own right edge, the rows' selection and
 * hairlines ending at it (`ToolRow`'s narrowest fit), never under the
 * inspector.
 */
const NARROW_INSPECTOR_BELOW = 900 - 208;

/** An update the user hid on the Updates page, and how (`hidingRule`). */
interface HiddenUpdate {
  by: HiddenBy;
  candidate: UpdateCandidate;
}

/**
 * One of a row's status words: its word, the why behind its ⓘ -- on the
 * row, and in the inspector's 「状态」 -- and what kind it is: what the row
 * is and why it can't do something, an update to be had, or up to date.
 * The last two are normal states, which a row does not put in words (spec
 * §3.4): the version column says the first, and silence the second. The
 * inspector lists every one.
 */
interface RowChip {
  id: string;
  label: string;
  detail?: ReactNode;
  /** Its button's accessible name on a row, where the word is the same on many (`StatusChip`'s `ariaLabel`). */
  ariaLabel?: string;
  tone: "neutral" | "update" | "upToDate";
  /**
   * The way back from what the word says, beside it in the inspector's
   * 「状态」: 「取消跳过」 by 「已跳过2.102.0」, 「恢复提醒」 by 「已关闭提醒」.
   */
  undo?: { label: string; ariaLabel: string; onUndo: () => void };
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
 * to say it, the source's name after them. On the rows' grid, as that
 * line is on its page's: the triangle in a 16 slot centred on the
 * avatars' column (20 + 8 + 8 = 36, the 32 avatar's middle), the words
 * where the names start (+ 16 + 20 = 64, the avatar's 32 and 12 past
 * it). One of the rows ↑ and ↓ move between, Space or Enter unfolding it;
 * its focus ring is index.css's, inset as the rows' is.
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
      className="relative flex h-8 w-full items-center px-5 text-left text-body text-muted"
    >
      <span data-disclosure-symbol="" className="ml-2 flex w-4 shrink-0 justify-center">
        <DisclosureIcon size={10} className={`shrink-0 ${expanded ? "rotate-90" : ""}`} />
      </span>
      <span className="ml-5 min-w-0 truncate">
        {t(expanded ? "installed.hideDependencies" : "installed.showDependencies", { count })}
      </span>{" "}
      {source !== null ? <span className="ml-1.5 shrink-0 text-small text-muted">{source}</span> : null}
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

/**
 * The inspector's content, scrolling only when it is taller than the pane.
 * A pane that always scrolled would cut off a status word's ⓘ panel at its
 * edge -- 260 wide, it is as wide as the narrow inspector itself -- where
 * it may otherwise stand over the list's edge, as a popover does. Measured
 * from the content's own height, which an open panel does not add to.
 */
function InspectorScroll({ children }: { children: ReactNode }) {
  const scroller = useRef<HTMLDivElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const [scrolls, setScrolls] = useState(false);
  useLayoutEffect(() => {
    const box = scroller.current;
    const inner = content.current;
    if (box === null || inner === null) return;
    const measure = () => setScrolls(inner.offsetHeight > box.clientHeight);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(box);
    observer.observe(inner);
    return () => observer.disconnect();
  }, []);
  return (
    <div
      ref={scroller}
      data-inspector-scroll={scrolls ? "" : undefined}
      className={`min-h-0 flex-1 ${scrolls ? "overflow-y-auto" : ""}`}
    >
      <div ref={content} data-inspector-content="" className="px-5 pb-5 pt-5">
        {children}
      </div>
    </div>
  );
}

/**
 * One fact in the inspector's group: its label, and its value -- which
 * selects, to be copied (a version into a search, a path into Terminal),
 * unless it is the status words, which are buttons.
 */
interface InspectorFact {
  term: string;
  value: ReactNode;
  selectable: boolean;
}

/**
 * The inspector's facts, as a Mac's info pane groups them (cork-package-
 * info.png, System Settings' About): one grouped container -- the group
 * fill, corners of 10 -- a row each, 28 high, the label on the left, 13 in
 * the secondary colour, and the value on the right, 13 in the label colour
 * with figures of one width; a hairline in the group's separator colour
 * between each two, 10 in from either side (`GROUP`). A value too long for
 * its line -- a path -- wraps under itself, still at the right.
 */
function FactsGroup({ facts }: { facts: InspectorFact[] }) {
  return (
    <dl data-facts="" className={`mt-4 ${GROUP}`}>
      {facts.map((fact) => (
        <div key={fact.term} className="flex min-h-7 items-start justify-between gap-3 px-2.5 py-1.5 text-body">
          <dt className="shrink-0 whitespace-nowrap text-muted">{fact.term}</dt>
          <dd
            className={`min-w-0 break-words text-right tabular-nums text-foreground ${
              fact.selectable ? "select-text" : ""
            }`}
          >
            {fact.value}
          </dd>
        </div>
      ))}
    </dl>
  );
}

/**
 * Where a warning's own sentence opens by saying its title again --
 * 「uv没有响应，无法列出…」 under 「uv没有响应」 -- the sentence the empty
 * page says under that title instead: only what the title does not.
 */
const EMPTY_PAGE_DESCRIPTION_KEYS: Record<string, string> = {
  "sourceNotice.unreachable.detail": "installed.sourceEmpty.unreachable",
};

/**
 * The page on one source that has nothing to list (spec R8): why, in the
 * words of its first warning -- 「uv没有响应」 over 「无法列出它安装的内
 * 容。请稍后重新检查。」, the notice's sentence less what its title
 * has just said (`EMPTY_PAGE_DESCRIPTION_KEYS`) -- or, for a source that
 * answered but whose check did not finish this round, 「部分检查未完成」
 * over 「pipx这次未检查完，更新可能还没全部列出。」
 * (`unfinishedChecksNotice`): nothing listed may be only what it did not
 * get to; for one that answered in full, that
 * nothing is installed with it; and Check again, the header's, which
 * shows what it has once it answers or has something. In the list's
 * place, so the source's notice is not said a second time over it.
 */
function SourceEmpty({ instance, label }: { instance: ManagerInstance; label: string }) {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const { checkAgain, checking } = useCheckAgain();
  const warning =
    sourceWarningOf(instance, label, 0) ??
    unfinishedChecksNotice(t, snapshot?.errors ?? [], snapshot?.instances ?? [], [instance]);
  return (
    <EmptyState
      symbol={warning === null ? "info" : "warning"}
      title={
        warning === null ? t("installed.sourceEmpty.title", { source: label }) : t(warning.titleKey, warning.values)
      }
      description={
        warning === null
          ? t("installed.sourceEmpty.description", { source: label })
          : t(EMPTY_PAGE_DESCRIPTION_KEYS[warning.descriptionKey] ?? warning.descriptionKey, warning.values)
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
  const saveSettings = useSaveSettings();
  const query = useUiStore((s) => s.query);
  const setQuery = useUiStore((s) => s.setQuery);
  const filter = useUiStore((s) => s.installedFilter);
  const setFilter = useUiStore((s) => s.setInstalledFilter);
  const sort = useUiStore((s) => s.installedSort);
  const setSort = useUiStore((s) => s.setInstalledSort);
  const show = useUiStore((s) => s.installedShow);
  const setShow = useUiStore((s) => s.setInstalledShow);
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
  // The toolbar's search field, once it is drawn there: a ref, not state,
  // which would draw the page -- and every row of its list in sight --
  // again as the field is handed over.
  const searchBox = useRef<HTMLInputElement | null>(null);
  const searchFocusRequested = useUiStore((s) => s.searchFocusRequested);

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
  // The page's width: whether the inspector beside the list is 300 wide or
  // 260 (`NARROW_INSPECTOR_BELOW`). Not measured (jsdom), 300.
  const [attachPage, narrowInspector] = useNarrowerThan(NARROW_INSPECTOR_BELOW);

  const showTechnicalDetails = settings?.show_technical_details ?? false;
  const inspectorTitleId = useId();

  // Where the focus goes once 取消跳过 or 恢复提醒 has gone with the state
  // it undid: the inspector's Update, which that leaves in reach, or else
  // the page's title -- not the window's body, from where the next Tab
  // would start over at the sidebar.
  const inspectorUpdate = useRef<HTMLButtonElement>(null);
  const refocusAfterUndo = useRef(false);
  // The inspector's heading, the tool's name: where Enter on a row puts the
  // focus (`enterRow`), once the inspector shows that row, and where it
  // goes once the inspector's Update or Uninstall has started what it
  // offered and gives way to its progress.
  const inspectorHeading = useRef<HTMLHeadingElement>(null);
  const focusInspectorOnce = useRef(false);
  useEffect(() => {
    if (!focusInspectorOnce.current) return;
    focusInspectorOnce.current = false;
    inspectorHeading.current?.focus();
  });
  // The tool whose 取消跳过 or 恢复提醒 could not be saved, and why, in the
  // backend's words (worded where it is said, `settingsSaveSentence`):
  // said in its inspector, and only there, until it is pressed again.
  const [undoFailed, setUndoFailed] = useState<{ id: string; raw: string } | null>(null);
  useEffect(() => {
    if (!refocusAfterUndo.current) return;
    refocusAfterUndo.current = false;
    const focus = document.activeElement;
    if (focus === null || focus === document.body || !focus.isConnected) focusOrFallback(inspectorUpdate.current);
  });

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
  // with which one it is after it where this Mac has two of its kind,
  // 「Homebrew（Intel）」 (`instanceLabels`): the headings, the rows' avatars
  // and words, the `{{source}}` in a sentence.
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
  // Each tool's other copies, installed by another source (`twinChip`).
  const twins = useTwins(snapshot?.artifacts);

  // By name, as the user reads it -- a model's as its row shows it
  // (`listedName`) -- case and accents aside, and "node@22" after
  // "node@9"; the key breaks a tie, so the order never depends on the
  // snapshot's. The Updates page sorts the same way.
  const collator = useMemo(
    () => new Intl.Collator(i18n.language, { numeric: true, sensitivity: "base" }),
    [i18n.language],
  );
  const compareArtifacts = useCallback(
    (a: InstalledArtifact, b: InstalledArtifact) =>
      collator.compare(listedName(a.key, a.display_name), listedName(b.key, b.display_name)) ||
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
  // ones the user hid there, with how. So a pinned package, one Banager
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
  // A notice's Show (`showInstalledTool`): its tool selected, and the
  // focus on its row, scrolled into sight -- once the snapshot is here to
  // find it in. After the effect above, so a source changed along with it
  // does not unselect it again. A tool no longer listed selects nothing.
  const inspectRequested = useUiStore((s) => s.inspectRequested);
  useEffect(() => {
    if (inspectRequested === null || !snapshot) return;
    useUiStore.getState().inspectAnswered();
    if (!artifactsById.has(inspectRequested)) return;
    setSelection({ id: inspectRequested, filter: activeFilter });
    listHandle.current?.focusKey(inspectRequested);
  }, [inspectRequested, snapshot, artifactsById, activeFilter]);
  // Headings only while the list is sorted by source and shows every
  // source; a "N more components" line names its source only where the
  // list mixes sources and has no heading saying it.
  const grouped = sort === "source" && activeFilter === null;
  const mixed = activeFilter === null && !grouped;

  // What the search box asks for, by the name a row shows or the
  // package's own name ("visual-studio-code" finds "Microsoft Visual
  // Studio Code").
  const needle = query.trim().toLowerCase();

  // The rows the search matches, by source -- of the AI coding tools
  // alone while the 「显示」 popup says so (`shownBy`).
  const matchingByInstance = useMemo(() => {
    const byInstance = new Map<string, InstalledArtifact[]>();
    for (const artifact of snapshot?.artifacts ?? []) {
      const matches =
        shownBy(show, artifact) &&
        (needle === "" ||
          artifact.display_name.toLowerCase().includes(needle) ||
          artifact.key.name.toLowerCase().includes(needle));
      if (!matches) continue;
      const list = byInstance.get(artifact.key.instance_id) ?? [];
      list.push(artifact);
      byInstance.set(artifact.key.instance_id, list);
    }
    return byInstance;
  }, [snapshot, needle, show]);

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
  // Then a source with rows here whose version Banager has not been
  // tested with. Two lines or more fold into one (`SourceNotices`). First
  // of all, the checks of the sources in view that did not finish this
  // round (`unfinishedChecksNotice`): a line like the others, once a band
  // of its own over the page.
  const unfinished = useMemo(
    () =>
      snapshot
        ? unfinishedChecksNotice(
            t,
            snapshot.errors,
            snapshot.instances,
            activeFilter === null ? undefined : instancesInView,
          )
        : null,
    [t, snapshot, activeFilter, instancesInView],
  );
  const notices = useMemo(
    () => [
      ...(unfinished === null ? [] : [unfinished]),
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
    [unfinished, instancesInView, labelOf, countByInstance],
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
  // has its slot, the field is not there yet, and the request waits for
  // it: the field answers it as it is drawn (`attachSearch`).
  const answerSearch = useCallback(() => {
    const field = searchBox.current;
    const store = useUiStore.getState();
    if (field === null || !store.searchFocusRequested) return;
    field.focus();
    field.select();
    store.searchFocused();
  }, []);
  const attachSearch = useCallback(
    (field: HTMLInputElement | null) => {
      searchBox.current = field;
      answerSearch();
    },
    [answerSearch],
  );
  useEffect(() => {
    if (searchFocusRequested) answerSearch();
  }, [searchFocusRequested, answerSearch]);

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
  const uninstallOp = (artifact: InstalledArtifact): OpSummary | undefined => {
    const id = artifactKeyId(artifact.key);
    return (operations ?? []).find(
      (op) =>
        op.kind === "Uninstall" &&
        op.status !== "Done" &&
        artifactKeyId({ instance_id: op.instance_id, kind: op.artifact_kind, name: op.name }) === id,
    );
  };
  const uninstallUnderway = (artifact: InstalledArtifact): string | null => {
    const op = uninstallOp(artifact);
    if (op === undefined) return null;
    return op.status === "Queued" ? t("installed.uninstallQueued") : t("installed.uninstalling");
  };
  // A row's Uninstall's accessible name: what it says, with the tool's
  // name in it, its words first -- 「卸载git…」, 「正在卸载git…」 -- as
  // every row has one.
  const uninstallName = (artifact: InstalledArtifact): string => {
    const name = artifact.display_name;
    const op = uninstallOp(artifact);
    if (op === undefined) return t("installed.uninstallLabel", { name });
    return op.status === "Queued"
      ? t("installed.uninstallQueuedLabel", { name })
      : t("installed.uninstallingLabel", { name });
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
  // Enter on a row: the inspector shows it -- opened on it, if it was not
  // -- and the focus goes to its heading, from where Tab reaches its
  // controls rather than every control of the list first.
  const enterRow = (artifact: InstalledArtifact) => {
    const id = artifactKeyId(artifact.key);
    if (id === selectedId && inspectorHeading.current !== null) {
      inspectorHeading.current.focus();
      return;
    }
    focusInspectorOnce.current = true;
    setSelection({ id, filter: activeFilter });
  };
  // Where the focus goes once an Uninstall pressed at `opener` has started:
  // the button stays, off, as 「正在卸载…」, and a button that is off takes
  // no focus -- so to its row, or to the inspector's heading.
  const afterUninstall = (opener: HTMLElement | null): HTMLElement | null => {
    if (opener === null) return null;
    const row = opener.closest<HTMLElement>("[data-row-focus]");
    if (row !== null) return row;
    return opener.closest("[data-inspector]") !== null ? inspectorHeading.current : opener;
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

  // What a row says it is, and its inspector: the line in the window's
  // language where there is one, else the source's own words -- one line.
  const describe = (artifact: InstalledArtifact, instance: ManagerInstance, label: string): string =>
    toolDescription(
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

  // The Settings page's 「取消跳过」 and 「恢复提醒」 for one update, from its
  // inspector: the skip that hides it -- the one `hidingRule` matched, by
  // the version it offers -- or its never-remind, out of the settings,
  // which lists it again. One save at a time, as the Updates page's hiding
  // items: a second built from the same settings would undo the first.
  const undoHiding = ({ by, candidate }: HiddenUpdate) => {
    if (!settings || saveSettings.isPending) return;
    const id = artifactKeyId(candidate.key);
    const skip = skippedVersionId({ key: candidate.key, version: candidate.target });
    setUndoFailed(null);
    saveSettings.mutate(
      by === "ignored"
        ? { ...settings, ignored_updates: settings.ignored_updates.filter((key) => artifactKeyId(key) !== id) }
        : { ...settings, skipped_versions: settings.skipped_versions.filter((s) => skippedVersionId(s) !== skip) },
      {
        onSuccess: () => {
          refocusAfterUndo.current = true;
        },
        onError: (error) => setUndoFailed({ id, raw: error.message }),
      },
    );
  };

  // How an update the user hid on the Updates page reads here: how it was
  // hidden, where "Update available" would promise one that page no
  // longer lists, with the way back (`undo`), as Settings words it. A skip
  // names the version skipped -- the skip is about that one -- except an
  // Ollama model's, a digest, never shown (`shownSkippedVersion`). A
  // `switch` with no default, so a new `HiddenBy` without a chip here
  // fails `tsc`.
  const hiddenChip = (hidden: HiddenUpdate): RowChip => {
    const { by, candidate } = hidden;
    const name = nameOf(candidate);
    switch (by) {
      case "ignored":
        return {
          id: "hidden",
          label: t("installed.updateIgnored"),
          detail: detailLines([t("updates.neverRemindHint")]),
          tone: "neutral",
          undo: {
            label: t("settings.ignoredUpdates.unignore"),
            ariaLabel: t("settings.ignoredUpdates.unignoreAriaLabel", { name }),
            onUndo: () => undoHiding(hidden),
          },
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
          undo: {
            label: t("settings.skippedVersions.unskip"),
            ariaLabel:
              version === null
                ? t("settings.skippedVersions.unskipNewBuildAriaLabel", { name })
                : t("settings.skippedVersions.unskipAriaLabel", { name, version }),
            onUndo: () => undoHiding(hidden),
          },
        };
      }
    }
  };

  /**
   * A row's chips, each with its why: what its source lets Banager do,
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
        ariaLabel: t("installed.uninstallHold.ariaLabel", { name: artifact.display_name }),
        detail: holdDetail,
        tone: "neutral",
      });
    }
    // Homebrew's own mark: 「已停用」 or 「已弃用」.
    const homebrewChip = homebrewStatusChip(t, artifact);
    if (homebrewChip !== null) chips.push(homebrewChip);
    if (listed !== undefined) {
      // A `switch` with no default, so a state added to `UpdateState`
      // without a chip here fails `tsc`.
      const state = updateStateOf(listed, instance);
      switch (state.kind) {
        case "actionable":
          // The inspector's 「新版本」 gives the version it moves to --
          // except a model's, a digest, which the word alone says.
          chips.push({ id: "update", label: t("installed.updateAvailable"), tone: "update" });
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
      !leftOutOfUpdateCheck(artifact, settings?.include_self_updating ?? false) &&
      // Disabled: no update will come, which 「已是最新」 would blur.
      (artifact.facts.homebrew?.disabled ?? null) === null
    ) {
      chips.push({ id: "up-to-date", label: t("installed.upToDate"), tone: "upToDate" });
    }
    // 「装了两份」: last, so the row says it only when nothing above has.
    const twin = twinChip(t, artifact, twins.get(id), sourceLabelFor);
    if (twin !== null) chips.push(twin);
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
  // command a chip talks about, in a group of its own under a hairline, as
  // the Updates page's menu sets it apart.
  const menuItems = (artifact: InstalledArtifact, instance: ManagerInstance): MenuItem[] => {
    const items: MenuItem[] = [
      { id: "details", label: t("common.details"), onSelect: () => select(artifact) },
    ];
    const command = commandOf(artifact, instance);
    if (showTechnicalDetails && command !== null) {
      items.push({
        id: "copy",
        label: t("common.copyCommand"),
        separatorBefore: true,
        onSelect: () => copyCommand(command),
      });
    }
    return items;
  };

  // Whether any row the list shows has a word (`rowChipOf`): where none
  // has, the rows give the status word's column to their names and
  // descriptions (`VirtualList`'s `statusColumn`).
  const statusColumn = items.some(
    (item) => item.type === "row" && rowChipOf(chipsOf(item.artifact, item.instance, item.label)) !== undefined,
  );

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
        // A model's path: the model as the name, where it is from before
        // its line; whole in the inspector.
        namePath={modelPath(artifact.key, name)}
        showSource={namedTwice.has(nameKey(name))}
        description={describe(artifact, instance, label)}
        status={
          chip === undefined ? undefined : <StatusChip label={chip.label} detail={chip.detail} ariaLabel={chip.ariaLabel} />
        }
        statusText={chip?.label}
        version={change?.version ?? versionOf(artifact)}
        newVersion={change?.newVersion}
        action={
          canUninstall(artifact, instance) ? (
            <RowAction
              disabled={uninstallHeld(artifact, instance)}
              onClick={(event) => uninstall(artifact, event.currentTarget)}
              ariaLabel={uninstallName(artifact)}
            >
              {uninstallUnderway(artifact) ?? t("installed.uninstall")}
            </RowAction>
          ) : null
        }
        menu={<Menu label={t("common.moreActions", { name })} items={menuItems(artifact, instance)} />}
        onOpen={() => pressRow(artifact)}
        openLabel={t("common.detailsLabel", { title: name })}
        onEnter={() => enterRow(artifact)}
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
   * the page's full height, the list narrowed beside it -- no dialog, no
   * dimming, the list still in reach -- 300 wide, or 260 in a window under
   * 900 (`NARROW_INSPECTOR_BELOW`). From the top, as a Mac's info pane:
   *
   * - the tool's icon at 48, its name beside it (15/20 semibold, wrapping)
   *   and its source under that, 11 in the secondary colour;
   * - all of its description, 13/18, wrapping as a pane's text does --
   *   never cut to a line as a row's: its line in the window's language
   *   where it has one, and else its source's own words, never both
   *   (`toolDescription`);
   * - its facts in one group (`FactsGroup`): its version and the one an
   *   update would bring, when it was installed, its size, where it is
   *   (with technical details on), and 「状态」, every status word it has --
   *   「可更新」, 「已是最新」, 「已跳过2.102.0」 -- each with its why behind
   *   an ⓘ, as on a row, and a hidden update's way back after its word,
   *   Settings' own 「取消跳过」 or 「恢复提醒」;
   * - 16 under the group, at its right, what can be done: Update, the
   *   default, rightmost, through the Updates page's own confirmation, and
   *   Uninstall…, grey, to its left; while that update runs, its progress
   *   where the button was, and once it has ended without updating, how it
   *   ended beside Retry. Not pinned to the pane's foot: under what it acts
   *   on, whatever the pane's height;
   * - an update that could not start, and what its source had to say this
   *   time.
   */
  const inspector = (artifact: InstalledArtifact, instance: ManagerInstance, narrow: boolean) => {
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
    // version a row Banager could not check carries as its target.
    const newer =
      candidate !== undefined &&
      candidate.checkable &&
      candidate.channel !== "Digest" &&
      candidate.target !== "" &&
      candidate.target !== artifact.version
        ? candidate.target
        : null;
    const facts: InspectorFact[] = [];
    if (version !== null) facts.push({ term: t("installed.version"), value: version, selectable: true });
    if (newer !== null) facts.push({ term: t("installed.newVersion"), value: newer, selectable: true });
    if (artifact.installed_at !== null) {
      facts.push({
        term: t("installed.installedOn"),
        value: formatDate(artifact.installed_at, i18n.language),
        selectable: true,
      });
    }
    if (artifact.size_bytes !== null) {
      facts.push({ term: t("installed.size"), value: formatBytes(artifact.size_bytes), selectable: true });
    }
    // Where it is, only while technical details are on, and only where the
    // source said: an app's bundle, a program's file, a tool's own folder.
    if (showTechnicalDetails && artifact.path !== null) {
      facts.push({
        term: t("installed.location"),
        value: <code className="break-all font-mono text-small">{artifact.path}</code>,
        selectable: true,
      });
    }
    const homepage = homepageFact(t, artifact.homepage, copyCommand);
    if (homepage !== null) facts.push(homepage);
    // Where its update stands, and what it is: a row of the group, each
    // word 13 in the label colour as the other values, its why behind an
    // ⓘ after it -- not a line of its own under the facts -- and after a
    // hidden update's, the way back, a small grey button (under the word
    // where the pane is too narrow for both).
    if (chips.length > 0) {
      facts.push({
        term: t("installed.status"),
        value: (
          <ul data-status-list="" className="flex flex-col items-end gap-1">
            {chips.map((chip) => (
              <li key={chip.id} className="flex flex-wrap items-center justify-end gap-1">
                <span data-status-word="">{chip.label}</span>
                {chip.detail !== undefined && !HOMEBREW_STATUS_CHIP_IDS.has(chip.id) ? (
                  <InfoDetail label={t("common.detailsLabel", { title: chip.label })}>{chip.detail}</InfoDetail>
                ) : null}
                {chip.undo !== undefined ? (
                  <button
                    type="button"
                    aria-label={chip.undo.ariaLabel}
                    onClick={chip.undo.onUndo}
                    className={`ml-1 shrink-0 ${BUTTON.small.grey}`}
                  >
                    {chip.undo.label}
                  </button>
                ) : null}
              </li>
            ))}
          </ul>
        ),
        selectable: false,
      });
    }
    // Without a Show: it would show what the inspector shows already.
    const sourceNotices = sourceNoticesFor(instance, label, countByInstance.get(instance.id) ?? 0).map((notice) =>
      notice.action?.id === "showTool" ? { ...notice, action: undefined } : notice,
    );
    const line = describe(artifact, instance, label);
    const removable = canUninstall(artifact, instance);
    const refusals = confirm.pageErrors.filter((item) => artifactKeyId(item.candidate.key) === id);
    return (
      <aside
        aria-labelledby={inspectorTitleId}
        data-inspector={narrow ? "narrow" : "wide"}
        onKeyDown={onEscape}
        className={`flex ${narrow ? "w-65" : "w-75"} shrink-0 flex-col border-l border-separator bg-content`}
      >
        <InspectorScroll>
          <div className="flex items-start gap-3">
            <ToolAvatar adapterId={instance.adapter_id} sourceLabel={label} iconKey={artifact.key} size="lg" />
            <div className="min-w-0 flex-1 self-center">
              <h2
                ref={inspectorHeading}
                id={inspectorTitleId}
                // Focused by script only (`enterRow`), for a screen reader to
                // start at, and with no ring, as the page's title has none: it
                // is no control, and a ring round a name reads as a text field.
                // The next Tab is the pane's first control, which rings.
                tabIndex={-1}
                className="break-words text-section text-foreground outline-none"
              >
                {name}
              </h2>
              {label === name ? null : <p className="mt-0.5 break-words text-small text-muted">{label}</p>}
            </div>
            {/* Escape and pressing the row again close it too; this is
                the way that shows. */}
            <button
              type="button"
              // Not "Close" alone: the operation bar's × is one too.
              aria-label={t("installed.closeDetails")}
              onClick={closeInspector}
              className={`${ICON_BUTTON} -mr-2 self-start`}
            >
              <CloseIcon size={16} />
            </button>
          </div>
          <p data-description="" className="mt-4 whitespace-normal break-words text-body-long text-foreground">
            {line}
          </p>
          {facts.length > 0 ? <FactsGroup facts={facts} /> : null}
          <CommandsGroup artifact={artifact} artifacts={snapshot?.artifacts ?? []} sourceLabelFor={sourceLabelFor} />
          <HomebrewNotes artifact={artifact} />
          {/* 取消跳过 or 恢复提醒 could not be saved: the word is still true. */}
          {undoFailed !== null && undoFailed.id === id ? (
            <p role="alert" className={`mt-2 ${SMALL_WRAPPING} text-danger-text`}>
              {settingsSaveSentence(t, "updates.saveChoiceFailed", undoFailed.raw, showTechnicalDetails)}
            </p>
          ) : null}
          {removable || updatable ? (
            // Under what they act on, at the right, the default rightmost,
            // as a Mac's pane and a dialog's footer set their buttons.
            // Uninstall is grey -- offered, not recommended, and not red
            // (`RowAction`).
            <div data-inspector-actions="" className="mt-4 flex flex-wrap items-center justify-end gap-2">
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
              {progress !== null ? <UpdateProgress progress={progress} name={name} onViewLog={openLog} /> : null}
              {/* As on the Updates page's row: an update that ended without
                  updating keeps how it ended, with Retry in Update's place. */}
              {updatable && listed !== undefined && (progress === null || isRetryable(progress)) ? (
                <button
                  ref={inspectorUpdate}
                  type="button"
                  onClick={(event) =>
                    void confirm.openConfirm([listed], event.currentTarget, () => inspectorHeading.current?.focus())
                  }
                  disabled={confirm.dialogOpen}
                  className={BUTTON.regular.default}
                >
                  {progress === null ? t("updates.update") : t("updates.retry")}
                </button>
              ) : null}
            </div>
          ) : null}
          {/* This tool's own refusal only: the update that failed to start
              may have been pressed for another tool. */}
          {refusals.map(({ refusal }) => (
            <Refusal key={id} text={refusal.text} detail={refusal.detail} detailTitle={refusal.text} className="mt-4" />
          ))}
          {sourceNotices.length > 0 ? (
            <div className="mt-4 flex flex-col gap-2">
              <SourceNotices notices={sourceNotices} layout="block" />
            </div>
          ) : null}
        </InspectorScroll>
      </aside>
    );
  };

  // The page on one source that has nothing to list says why in the
  // list's place (`SourceEmpty`), and its notice is not said over it.
  const sourceEmpty = activeFilter !== null && (countByInstance.get(activeFilter) ?? 0) === 0;
  return (
    <div ref={attachPage} className="relative flex h-full">
      {/* The page's own controls, in the window's toolbar (spec §3.2):
          how the last Copy command went, for a moment; what it shows
          (every tool, or the AI coding tools) and the sort, grey popup
          buttons; and the search field, 200 wide. */}
      <ToolbarItems>
        <p role="status" className="max-w-40 truncate text-small text-muted empty:hidden">
          {copyStatus === "copied" ? t("common.copied") : copyStatus === "failed" ? t("common.copyFailed") : null}
        </p>
        <ToolShowButton value={show} onChange={setShow} />
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
            ref={attachSearch}
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
        {/* That nothing matches, for a screen reader, as it becomes so: one
            node for as long as the page is open, beside the field typed
            in, where the list's own line (below) is a new one each time. */}
        <p role="status" data-search-status="" className="sr-only">
          {needle !== "" && items.length === 0 && !sourceEmpty ? t("installed.noMatches", { query: query.trim() }) : null}
        </p>
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
          statusColumn={statusColumn}
          // A hairline over the next row only, and not over the selection.
          hairlineBefore={(next) => next.type === "row" && artifactKeyId(next.artifact.key) !== selectedId}
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
              // The one line in the middle of the list's area, 13 in the
              // secondary colour, no symbol (spec §3.9): 「没有找到“xxx”」.
              // The notices, if any, keep the list's first line over it.
              <div className="flex h-full flex-col">
                {notices.length > 0 ? (
                  <div className="px-5">
                    <SourceNotices notices={notices} layout="line" fold={noticeFold} />
                  </div>
                ) : null}
                <p
                  data-list-empty=""
                  // Said by the search's status, as it becomes so: not twice.
                  aria-hidden={needle !== "" ? true : undefined}
                  className="flex flex-1 items-center justify-center px-5 text-center text-body text-muted"
                >
                  {needle !== ""
                    ? t("installed.noMatches", { query: query.trim() })
                    : show === "ai"
                      ? activeFilter === null
                        ? t("families.none")
                        : t("families.noneInSource", { source: sourceLabelFor(activeFilter) })
                      : t("emptyStates.nothingInstalled.title")}
                </p>
              </div>
            )
          }
        />
      </div>
      {details !== undefined && detailsInstance !== undefined ? inspector(details, detailsInstance, narrowInspector) : null}
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
            uninstallOpener.current = afterUninstall(uninstallOpener.current);
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
