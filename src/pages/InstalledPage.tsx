import { useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent, ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useCheckAgain, useOperations, useSaveSettings, useSettings, useSizes, useSnapshot } from "../lib/queries";
import { useInstalledSnapshot } from "../lib/inventoryPreview";
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
  withoutHiding,
  leftOutOfUpdateCheck,
  shownSkippedVersion,
  updateStateOf,
  upToDateIsKnown,
} from "../lib/updateState";
import type { HiddenBy } from "../lib/updateState";
import { copyStatusText, useCopyCommand } from "../lib/clipboard";
import { snoozeOf, snoozedUntilText } from "../lib/snooze";
import { useTranslatedDescription } from "../lib/toolDescriptions";
import { listedName, modelPath, nameKey, namesUnderSeveralSources } from "../lib/names";
import { searchMatch, searchNeedle } from "../lib/searchMatch";
import { useSearchTexts } from "../lib/useSearchTexts";
import { SEARCH_SETTLE_MS, useSettled } from "../lib/settled";
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
import { discoverCounts, discoverCovered, discoverNotices, isDiscoverShow, shownBy, type InstalledShow } from "../lib/families";
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
import { InfoDetail, TextWithInfo } from "../components/InfoDetail";
import {
  HOMEBREW_STATUS_CHIP_IDS,
  HomebrewCaveats,
  homebrewMarkLines,
  homebrewStatusChip,
  homepageFact,
  otherVersionsFact,
} from "../components/HomebrewStatus";
import { InspectorCallout, twinAdviceLines, twinVerdict } from "../components/TwinAdvice";
import { uncheckedUpdatesChip } from "../components/UncheckedUpdates";
import { updatesUnchecked } from "../lib/uncheckedStandalone";
import { CommandsGroup, notOnPathChip, twinChip, useTwins } from "../components/CommandFacts";
import { withoutJudgedPathNotices } from "../lib/commands";
import { sizeFact } from "../components/SizeFact";
import { compareBySize, sizeCellOf, sizeOrderOf } from "../lib/sizes";
import { compareByInstalledAt, installedDateCellOf } from "../lib/installedDates";
import { COMMANDS_UNKNOWN_KEYS, commandsKnown } from "../lib/commandsKnown";
import { sizeTotalsOf, sourceTotalText } from "../lib/sizeTotals";
import {
  countedTicks,
  tickable,
  uninstalledWhileBusy,
  uninstallHeld as heldBy,
  uninstallOffered,
  type UninstallHolds,
} from "../lib/batchUninstall";
import { InstalledSelectionHeader, UninstallSelectedButton } from "../components/InstalledSelectionHeader";
import { BatchUninstallSheet, useBatchUninstall } from "../components/BatchUninstallSheet";
import { BatchUninstallResult } from "../components/BatchUninstallResult";
import { ReadOnlySourceLine } from "../components/ReadOnlySourceLine";

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
  /**
   * What the ⓘ says in the inspector's 「状态」 instead of `detail`, where
   * another group of the inspector says part of it already (「装了两份」's
   * which-copy-runs sentence, said by 「在终端里输入时」).
   */
  inspectorDetail?: ReactNode;
}

/**
 * The one word a row shows (spec §3.4: one at most): why its Uninstall
 * waits, where it does -- the one word that explains a disabled button --
 * then 「装了两份」, so that every row the 「装了不止一份」 filter lists says
 * so; else the first of its chips that is not a normal state, in
 * `chipsOf`'s order -- what the source allows, then the tool's own refusal
 * to be removed, then Homebrew's own mark, then that Terminal does not
 * find one of its commands (「终端里找不到」), then where its update stands,
 * then how the user hid it. Never the first check's wait, the same on
 * every row: the list's own line says it once (`PREVIEW_HOLD_ID`).
 */
function rowChipOf(chips: RowChip[]): RowChip | undefined {
  return (
    chips.find((chip) => chip.id === "uninstall-held") ??
    chips.find((chip) => chip.id === "twin") ??
    chips.find((chip) => chip.tone === "neutral" && chip.id !== PREVIEW_HOLD_ID)
  );
}

/**
 * The first check's hold on every Uninstall, in the inspector's 「状态」
 * only: on the list, one line over the rows says it, and each row's
 * button keeps it as its tooltip.
 */
const PREVIEW_HOLD_ID = "uninstall-held-preview";

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
 * shows what it has once it answers or has something -- except where
 * checking again cannot change the answer (`NoPip`, `HttpsHostRefused`).
 * In the list's place, so the source's notice is not said a second time
 * over it.
 */
function SourceEmpty({ instance, label }: { instance: ManagerInstance; label: string }) {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const { checkAgain, checking } = useCheckAgain();
  // A source that could not be asked says why even when that is no
  // warning: 「“python3.13”没有附带pip」 rather than 「pip中没有安装任何内容」,
  // which would promise that what is installed with it shows up here.
  const warning =
    sourceWarningOf(instance, label, 0) ??
    unfinishedChecksNotice(t, snapshot?.errors ?? [], snapshot?.instances ?? [], [instance]) ??
    (isAvailable(instance) ? null : (sourceNoticesFor(instance, label, 0)[0] ?? null));
  // No Check Again where checking again cannot change the answer: a Python
  // with no pip, and an https OLLAMA_HOST, which only a new address and a
  // reopened app fix -- their notices have no button either.
  const checkingAgainHelps =
    instance.status.unavailable !== "NoPip" && instance.status.unavailable !== "HttpsHostRefused";
  return (
    <EmptyState
      symbol={warning === null || warning.variant === "info" ? "info" : "warning"}
      title={
        warning === null ? t("installed.sourceEmpty.title", { source: label }) : t(warning.titleKey, warning.values)
      }
      description={
        warning === null
          ? t("installed.sourceEmpty.description", { source: label })
          : t(EMPTY_PAGE_DESCRIPTION_KEYS[warning.descriptionKey] ?? warning.descriptionKey, warning.values)
      }
      {...(checkingAgainHelps ? { action: { label: t("header.checkAgain"), onClick: checkAgain, disabled: checking } } : {})}
    />
  );
}

/**
 * What the list says when the 「显示」 popup's choice has nothing to show:
 * on every source, and on one, named.
 */
const SHOW_NONE_KEYS: Record<Exclude<InstalledShow, "all">, { none: string; noneInSource: string }> = {
  ai: { none: "families.none", noneInSource: "families.noneInSource" },
  twins: { none: "twinsFilter.none", noneInSource: "twinsFilter.noneInSource" },
  notOnPath: { none: "families.notOnPathNone", noneInSource: "families.notOnPathNoneInSource" },
  brewRetired: { none: "families.brewRetiredNone", noneInSource: "families.brewRetiredNoneInSource" },
  otherVersions: { none: "otherVersionsShow.none", noneInSource: "otherVersionsShow.noneInSource" },
};

/** The size order under any sort but By Size: one map, so it never changes. */
const NO_SIZE_ORDER: Map<string, number> = new Map();

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
 * its Update. Not a dialog: the list stays in reach beside
 * it, and Escape or pressing the row again closes it.
 */
export function InstalledPage() {
  const { t, i18n } = useTranslation();
  // While the first check is still checking for updates, its list
  // (`preview`): no update in it, and nothing to do on it yet.
  const { data: snapshot, isLoading, preview } = useInstalledSnapshot();
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
  // The rows ticked for 「卸载所选」 (`InstalledSelectionHeader`).
  const selectedUninstalls = useUiStore((s) => s.selectedUninstalls);
  const toggleUninstall = useUiStore((s) => s.toggleUninstall);
  const keepUninstalls = useUiStore((s) => s.keepUninstalls);
  const operationFor = useUpdateOperationFor();
  const { data: operations } = useOperations();
  // How much each tool takes on disk, measured after each check (`sizeFact`).
  const { data: sizes } = useSizes();
  // Only while the list is sorted by size: under any other sort, sizes
  // coming in after a check do not rebuild the list's rows.
  const sizeOrder = useMemo(
    () => (sort === "size" ? sizeOrderOf(sizes, snapshot?.artifacts ?? []) : NO_SIZE_ORDER),
    [sort, sizes, snapshot],
  );
  // Why 「装了不止一份」 or 「终端里找不到」 cannot say 「没有发现…」: the
  // commands are not judged yet, or were not this time (`commandsKnown`).
  // Nor 「Homebrew已停用或弃用」 or 「保留了其他版本」 while the first
  // check's list is all there is: its rows carry no Homebrew facts yet.
  const commandsUnknown = useMemo(() => {
    if ((show === "brewRetired" || show === "otherVersions") && preview) return COMMANDS_UNKNOWN_KEYS.previewing;
    if (show !== "twins" && show !== "notOnPath") return null;
    const known = commandsKnown(snapshot?.artifacts ?? [], preview, show === "twins" ? "names" : "verdicts");
    return known === "known" ? null : COMMANDS_UNKNOWN_KEYS[known];
  }, [show, snapshot, preview]);
  // What each source takes together, under its heading (`sizeTotalsOf`).
  const sizeTotals = useMemo(() => sizeTotalsOf(sizes, snapshot), [sizes, snapshot]);
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
  // Its tick too, for good: a tool installed again later never comes back
  // ticked for removal.
  useEffect(() => {
    if (snapshot) keepUninstalls(new Set(artifactsById.keys()));
  }, [snapshot, artifactsById, keepUninstalls]);

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
  // 「卸载所选」's sheet: the ticked rows' uninstalls, previewed together.
  const batchUninstall = useBatchUninstall();

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
  // Studio Code"), by a word of its line in either language ("编程"), or
  // by a command it puts on the Mac ("rg" finds ripgrep; `searchMatch`),
  // through each tool's words, made once for the list (`useSearchTexts`).
  const needle = searchNeedle(query);
  const searchTexts = useSearchTexts(snapshot, needle !== "");
  const found = (artifact: InstalledArtifact, text: string) =>
    searchMatch(artifact, text, searchTexts?.get(artifactKeyId(artifact.key)));
  // A heading's 「约4.1 GB」, only while it counts the whole source: no search, every tool shown.
  const sourceTotalOf = (instanceId: string): string | null => {
    const total = needle === "" && show === "all" ? sizeTotals.bySource.get(instanceId) : undefined;
    return total === undefined ? null : sourceTotalText(t, total);
  };

  // The rows the search matches, by source -- of the AI coding tools
  // alone, or of those installed more than once, while the 「显示」 popup
  // says so (`shownBy`).
  const matchingByInstance = useMemo(() => {
    const byInstance = new Map<string, InstalledArtifact[]>();
    for (const artifact of snapshot?.artifacts ?? []) {
      const matches =
        shownBy(show, artifact, twins) &&
        searchMatch(artifact, needle, searchTexts?.get(artifactKeyId(artifact.key))) !== null;
      if (!matches) continue;
      const list = byInstance.get(artifact.key.instance_id) ?? [];
      list.push(artifact);
      byInstance.set(artifact.key.instance_id, list);
    }
    return byInstance;
  }, [snapshot, needle, show, twins, searchTexts]);

  // The sources in view: the filter's, or every one.
  const instancesInView = useMemo(
    () => (snapshot?.instances ?? []).filter((instance) => activeFilter === null || instance.id === activeFilter),
    [snapshot, activeFilter],
  );
  // How many of the sources in view's tools 「终端里找不到」,
  // 「Homebrew已停用或弃用」 and 「保留了其他版本」 show, search aside
  // (`discoverCounts`).
  // And how many of those a source's own notice already names, whose
  // line over the list would only say it again (`discoverCovered`).
  const { discover, discoverNamed } = useMemo(() => {
    const inView = (snapshot?.artifacts ?? []).filter(
      (artifact) => activeFilter === null || artifact.key.instance_id === activeFilter,
    );
    return { discover: discoverCounts(inView), discoverNamed: discoverCovered(inView, instancesInView) };
  }, [snapshot, activeFilter, instancesInView]);

  // How many of them 「装了不止一份」 shows, for the popup's number.
  const twinsInView = useMemo(
    () =>
      (snapshot?.artifacts ?? []).filter(
        (artifact) =>
          (activeFilter === null || artifact.key.instance_id === activeFilter) && twins.has(artifactKeyId(artifact.key)),
      ).length,
    [snapshot, activeFilter, twins],
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
      // Under 「终端里找不到」, 「Homebrew已停用或弃用」 or 「保留了其他版本」
      // nothing folds: a retired formula, or one keeping another version,
      // is often a component, and the count the popup and the notice said
      // is the rows 查看 shows (`isDiscoverShow`).
      const folding = !isDiscoverShow(show);
      const primary = artifacts.filter((a) => !folding || a.reason !== "Dependency").sort(compareArtifacts);
      const dependencies = folding ? artifacts.filter((a) => a.reason === "Dependency").sort(compareArtifacts) : [];
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
      // By size: the largest first, then by name, a row with no size last.
      const bySize = (a: ListItem, b: ListItem) =>
        a.type === "row" && b.type === "row"
          ? compareBySize(sizeOrder, a.artifact, b.artifact) || compareArtifacts(a.artifact, b.artifact)
          : 0;
      // By date installed: the newest first, then by name, a row with no
      // date last.
      const byDate = (a: ListItem, b: ListItem) =>
        a.type === "row" && b.type === "row"
          ? compareByInstalledAt(a.artifact, b.artifact) || compareArtifacts(a.artifact, b.artifact)
          : 0;
      result.push(...rows.sort(sort === "size" ? bySize : sort === "date" ? byDate : byName), ...folds);
    }
    return result;
  }, [instancesInView, matchingByInstance, labelOf, compareArtifacts, expandedDependencies, grouped, sort, sizeOrder, show]);

  // Whatever hides the selected tool closes its details, as a Mac list's
  // selection goes with a row its filter hides: details of a tool the list
  // no longer shows would be about nothing the user can see. A 「显示」
  // choice -- from the popup, or a notice's 查看 -- a search, or a new check
  // that takes the tool out of the choice while it is still installed (its
  // other copy uninstalled under 「装了不止一份」, its folder put on `PATH`
  // under 「终端里找不到」). A tool gone from the check altogether is
  // unselected above. A fold closed over a component hides nothing: the
  // tool is still among those the list holds, a notice's Show may select
  // one there, and the fold's line says it is in it.
  // A search hides it once the user has stopped typing (`useSettled`): a
  // letter that hides it and the Backspace that brings it back, or a
  // longer name typed through a word that matches it no more than its
  // start, close nothing. The search on screen and the settled one must
  // both hide it, so a search cleared all at once (a notice's Show,
  // Escape in the field) never closes the details of a tool it shows.
  // Where the focus went with what hid it -- 查看's line, gone once the
  // choice is made, or the closed details -- it goes to the list's first
  // row, or, with none, the page's title: never the window's body, from
  // where the next Tab would start over at the sidebar. The popup, or the
  // search field typed in, keeps the focus. A 「显示」 choice that hides
  // nothing still places a focus it took with it; anything else that hides
  // nothing moves no focus.
  const settledNeedle = useSettled(needle, SEARCH_SETTLE_MS);
  const shownBefore = useRef(show);
  useEffect(() => {
    const showChanged = shownBefore.current !== show;
    shownBefore.current = show;
    const selected = selectedId === null ? undefined : artifactsById.get(selectedId);
    const hidden =
      selected !== undefined &&
      (!shownBy(show, selected, twins) ||
        (found(selected, needle) === null && found(selected, settledNeedle) === null));
    if (hidden) setSelection(null);
    if (!showChanged && !hidden) return;
    const focus = document.activeElement;
    const lost =
      focus === null ||
      focus === document.body ||
      !focus.isConnected ||
      (hidden && focus.closest("[data-inspector]") !== null);
    if (!lost) return;
    if (rowItems.some(keyboardRow)) listHandle.current?.focusFirst();
    else focusOrFallback(null);
  }, [show, selectedId, artifactsById, twins, needle, settledNeedle, rowItems, searchTexts]);

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
      // Last, while every tool is shown: how many Terminal can't find, and
      // how many Homebrew disabled or deprecated, each with a 查看 that
      // shows them (`discoverNotices`).
      ...discoverNotices(show, discover, discoverNamed),
    ],
    [unfinished, instancesInView, labelOf, countByInstance, show, discover, discoverNamed],
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
    uninstallOffered(artifact, instance);
  // Why such an Uninstall stays, disabled, for now, or null when it does
  // not: the source did not answer the last check -- its rows are last
  // time's, carried forward, and `Session::issue_plan` refuses to plan on
  // it (spec §2.5) -- which says what to do by why it did not
  // (`unavailableDetail`), or it refuses to plan one until a note of its
  // goes away -- a Homebrew updating its list (`uninstallHoldKey`). The
  // row's 「暂时不能卸载」 chip says it, the same chip for both; the button
  // comes back with the check that finds the source answering, or clears
  // the note. The first check's list holds it too, said by the same chip
  // after the row's own words (`chipsOf`).
  const uninstallHoldDetail = (
    artifact: InstalledArtifact,
    instance: ManagerInstance,
    label: string,
  ): ReactNode | null => {
    if (!canUninstall(artifact, instance)) return null;
    if (!isAvailable(instance)) return unavailableDetail(t, instance, label);
    const holdKey = uninstallHoldKey(instance);
    if (holdKey !== null) return detailLines([t(holdKey)]);
    return null;
  };
  // Held as above -- on the first check's list too, until that check is
  // done -- or while an uninstall of this one is under way.
  const holdsOf = (artifact: InstalledArtifact): UninstallHolds => ({
    preview,
    underway: uninstallOp(artifact) !== undefined,
    uninstalled: uninstalledIds.has(artifactKeyId(artifact.key)),
  });
  const uninstallHeld = (artifact: InstalledArtifact, instance: ManagerInstance): boolean =>
    heldBy(artifact, instance, holdsOf(artifact));
  // An uninstall of this one already queued or running: its Uninstall
  // stays, disabled, and says which.
  // Indexed once a draw: every row of the list asks, not only those in
  // sight (whether it can be ticked), and the operations run to hundreds.
  // The first in the backend's order, newest first, as a search of it finds.
  const activeUninstalls = new Map<string, OpSummary>();
  for (const op of operations ?? []) {
    if (op.kind !== "Uninstall" || op.status === "Done") continue;
    const id = artifactKeyId({ instance_id: op.instance_id, kind: op.artifact_kind, name: op.name });
    if (!activeUninstalls.has(id)) activeUninstalls.set(id, op);
  }
  const uninstallOp = (artifact: InstalledArtifact): OpSummary | undefined =>
    activeUninstalls.get(artifactKeyId(artifact.key));
  // A batch's tool already uninstalled while another operation on its
  // source runs: its row is the last check's, carried forward until the
  // source is read again (`uninstalledWhileBusy`), and says so.
  const uninstalledIds = uninstalledWhileBusy(operations ?? []);
  const uninstallUnderway = (artifact: InstalledArtifact): string | null => {
    const op = uninstallOp(artifact);
    if (op === undefined) {
      return uninstalledIds.has(artifactKeyId(artifact.key)) ? t("batchUninstall.uninstalledHold") : null;
    }
    return op.status === "Queued" ? t("installed.uninstallQueued") : t("installed.uninstalling");
  };
  // A row's Uninstall's accessible name: what it says, with the tool's
  // name in it, its words first -- 「卸载git…」, 「正在卸载git…」 -- as
  // every row has one.
  const uninstallName = (artifact: InstalledArtifact): string => {
    const name = artifact.display_name;
    const op = uninstallOp(artifact);
    if (op === undefined) {
      return uninstalledIds.has(artifactKeyId(artifact.key))
        ? t("batchUninstall.uninstalledLabel", { name })
        : t("installed.uninstallLabel", { name });
    }
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
    setUndoFailed(null);
    saveSettings.mutate(
      withoutHiding(settings, by, candidate),
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
      case "snoozed": {
        // Its date, from the settings that hide it; a snooze no longer
        // there (saved away meanwhile) says only that it is put off.
        const until = settings === undefined ? undefined : snoozeOf(settings, candidate.key)?.until;
        return {
          id: "hidden",
          label:
            until === undefined ? t("updates.snooze") : snoozedUntilText(t, until, i18n.language),
          detail: detailLines([t("updates.snoozeHint")]),
          tone: "neutral",
          undo: {
            label: t("settings.ignoredUpdates.unignore"),
            ariaLabel: t("settings.ignoredUpdates.unignoreAriaLabel", { name }),
            onUndo: () => undoHiding(hidden),
          },
        };
      }
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
    // whatever the next check finds -- said once over the list instead on
    // the page of that one source (`ReadOnlySourceLine`), not on every row.
    if (!canWrite(instance) && activeFilter !== instance.id) {
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
    const homebrewChip = homebrewStatusChip(t, artifact, i18n.language);
    if (homebrewChip !== null) chips.push(homebrewChip);
    // 「装了两份」: after what the source and the tool allow, and before
    // where its update stands, which the Updates page says too.
    const twin = twinChip(t, artifact, twins.get(id), sourceLabelFor);
    if (twin !== null) chips.push(twin);
    // 「终端里找不到」: after 「装了两份」, which says it of the shared command
    // already, and before where its update stands -- an update does not
    // help a tool Terminal does not find.
    const notOnPath = notOnPathChip(t, artifact);
    if (notOnPath !== null) chips.push(notOnPath);
    // The first check's list holds every Uninstall until that check is
    // done (the button stays disabled, `uninstallHeld`). The same for
    // every row: the list's line says it once, and the row says its own
    // word, Homebrew's mark among them (`rowChipOf`).
    if (holdDetail === null && preview && canUninstall(artifact, instance)) {
      chips.push({
        id: PREVIEW_HOLD_ID,
        label: t("installed.uninstallHold.label"),
        ariaLabel: t("installed.uninstallHold.ariaLabel", { name: artifact.display_name }),
        detail: detailLines([t("inventoryPreview.uninstallHold")]),
        tone: "neutral",
      });
    }
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
          // "Pinned" once, with the unpin command, above; a package
          // Homebrew disabled says 「已停用」 once, in Homebrew's own line
          // above, which already says no more updates come.
          if (
            !(state.reason === "Pinned" && artifact.uninstall_blocked === "Pinned") &&
            !(state.reason === "Disabled" && homebrewChip !== null)
          ) {
            chips.push({
              id: "update-blocked",
              label: t(UPDATE_BLOCKED_KEYS[state.reason].badge),
              detail: blockedDetail(t, listed, state.reason, instance, label, showTechnicalDetails, artifact),
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
    } else if (updatesUnchecked(instance)) {
      // Codex's own install: no check was made, so not 「已是最新」.
      const unchecked = uncheckedUpdatesChip(t, artifact, instance, label);
      if (unchecked !== null) chips.push(unchecked);
    } else if (
      // The first check's list knows nothing of updates yet.
      !preview &&
      upToDateIsKnown(instance, snapshot.errors) &&
      !leftOutOfUpdateCheck(artifact, settings?.include_self_updating ?? false) &&
      // Disabled: no update will come, which 「已是最新」 would blur.
      (artifact.facts.homebrew?.disabled ?? null) === null
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
    const command = state.kind === "blocked" ? UPDATE_BLOCKED_KEYS[state.reason].command(listed.key, instance) : "";
    return command === "" ? null : command;
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
    // Found by a command alone, which the row names: 「命令：rg」.
    const match = needle === "" ? null : found(artifact, needle);
    // Where an update is listed, the version it moves to, as the Updates
    // page's row says it ("7.1 → 7.2"), in place of an "Update available"
    // word; a hidden one leaves the version installed.
    const listed = listedUpdates.get(artifactKeyId(artifact.key));
    const change = listed !== undefined && listed.checkable ? updateVersionColumn(t, listed) : null;
    // By Size: the size the order goes by in the version's place, as
    // Finder's Size column shows it -- muted while it is measured, or
    // 「—」 for none. The version is in the inspector.
    // By Date Installed, the same: the day it was installed, or 「—」.
    const sortCell =
      sort === "size"
        ? sizeCellOf(t, sizes, artifact)
        : sort === "date"
          ? installedDateCellOf(artifact, i18n.language, Date.now())
          : null;
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
        descriptionNote={match?.by === "command" ? t("installed.commandMatch", { command: match.command }) : undefined}
        status={
          chip === undefined ? undefined : <StatusChip label={chip.label} detail={chip.detail} ariaLabel={chip.ariaLabel} />
        }
        statusText={chip?.label}
        version={
          sortCell !== null ? (
            // 「—」 for none is drawn only: a screen reader hears no size or
            // date, as for a row with no version.
            <span
              data-size-cell={sort === "size" ? "" : undefined}
              data-date-cell={sort === "date" ? "" : undefined}
              aria-hidden={sortCell.text === "—" ? true : undefined}
              className={sortCell.muted ? "text-muted" : undefined}
            >
              {sortCell.text}
            </span>
          ) : (
            (change?.version ?? versionOf(artifact))
          )
        }
        // By Size or Date Installed, what the column says, with its term, in
        // the row's name.
        versionText={
          sortCell === null || sortCell.text === "—"
            ? undefined
            : sort === "date"
              ? t("installed.dateRowName", { date: sortCell.text })
              : t("sizes.rowName", { size: sortCell.text })
        }
        newVersion={sortCell !== null ? undefined : change?.newVersion}
        // A box exactly where Uninstall is there and enabled; the column's
        // room on every other row, so the avatars stay in line.
        selectable={
          tickable(artifact, instance, holdsOf(artifact))
            ? {
                checked: selectedUninstalls.includes(artifactKeyId(artifact.key)),
                onToggle: () => toggleUninstall(artifact.key),
                ariaLabel: t("batchUninstall.selectRow", { name }),
              }
            : null
        }
        action={
          canUninstall(artifact, instance) ? (
            <RowAction
              disabled={uninstallHeld(artifact, instance)}
              // The first check's hold, which the row has no word for.
              title={preview ? t("inventoryPreview.uninstallHold") : undefined}
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
    // The facts as a Mac's info pane orders them: versions, what it takes
    // on disk -- its own, then its other versions' -- when it came, where
    // it is, its site, and last its state.
    const facts: InspectorFact[] = [];
    if (version !== null) facts.push({ term: t("installed.version"), value: version, selectable: true });
    if (newer !== null) facts.push({ term: t("installed.newVersion"), value: newer, selectable: true });
    const size = sizeFact(t, artifact, sizes);
    if (size !== null) facts.push(size);
    const otherVersions = otherVersionsFact(t, artifact, sizes);
    if (otherVersions !== null) facts.push(otherVersions);
    if (artifact.installed_at !== null) {
      facts.push({
        term: t("installed.installedOn"),
        value: formatDate(artifact.installed_at, i18n.language),
        selectable: true,
      });
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
    const homepage = homepageFact(t, artifact.homepage);
    if (homepage !== null) facts.push(homepage);
    // What asks something of the user, in the callout under the
    // description (`InspectorCallout`): what Homebrew's mark means, and
    // which copy of a tool installed more than once Terminal runs. The
    // facts' 「状态」 then leaves those words out: said once, up there.
    const markLines = homebrewMarkLines(t, artifact, i18n.language);
    const twinLines = twinAdviceLines(
      t,
      artifact,
      twins.get(id),
      sourceLabelFor,
      canUninstall(artifact, instance),
    );
    // 「终端里找不到」 is the 「在终端里输入时」 group's to say, with its folder
    // and Copy Path: said once, down there.
    const statusChips = chips.filter(
      (chip) =>
        !HOMEBREW_STATUS_CHIP_IDS.has(chip.id) &&
        !(chip.id === "twin" && twinLines !== null) &&
        chip.id !== "not-on-path",
    );
    // Where its update stands, and what it is: a row of the group, each
    // word 13 in the label colour as the other values, its why behind an
    // ⓘ after it -- not a line of its own under the facts -- and after a
    // hidden update's, the way back, a small grey button (under the word
    // where the pane is too narrow for both).
    if (statusChips.length > 0) {
      facts.push({
        term: t("installed.status"),
        value: (
          <ul data-status-list="" className="flex flex-col items-end gap-1">
            {statusChips.map((chip) => (
              <li key={chip.id} className="flex flex-wrap items-center justify-end gap-1">
                <span data-status-word="">{chip.label}</span>
                {chip.detail !== undefined ? (
                  <InfoDetail label={t("common.detailsLabel", { title: chip.label })}>
                    {chip.inspectorDetail ?? chip.detail}
                  </InfoDetail>
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
    // Nor the launcher's PATH sentence when 「在终端里输入时」 says it better.
    const sourceNotices = withoutJudgedPathNotices(
      sourceNoticesFor(instance, label, countByInstance.get(instance.id) ?? 0),
      artifact,
    ).map((notice) => (notice.action?.id === "showTool" ? { ...notice, action: undefined } : notice));
    const line = describe(artifact, instance, label);
    const removable = canUninstall(artifact, instance);
    const refusals = confirm.pageErrors.filter((item) => artifactKeyId(item.candidate.key) === id);
    return (
      <aside
        // One per tool: what a section of it holds for itself -- Homebrew's
        // notes opened, a Copy button's 「已拷贝」 -- stays with the tool it
        // was for, not the next one selected.
        key={id}
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
          {/* What it is (the heading, the description), then what asks
              something of the user -- Homebrew's mark, which copy runs --
              its facts -- versions, how big, when, where -- ending on its
              state, 「状态」; then the commands it gives Terminal, and last
              the technical: Homebrew's own notes, in English, folded. */}
          <InspectorCallout>
            {[
              ...markLines,
              ...(twinLines ?? []).map((text) => (
                <p key={text} data-twin-advice="" className="text-body-long text-foreground">
                  {text}
                </p>
              )),
            ]}
          </InspectorCallout>
          {facts.length > 0 ? <FactsGroup facts={facts} /> : null}
          <CommandsGroup artifact={artifact} artifacts={snapshot?.artifacts ?? []} sourceLabelFor={sourceLabelFor} />
          <HomebrewCaveats artifact={artifact} />
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
                  // Grey on a copy Terminal does not run: updating it
                  // leaves the command as it was, so it is not the one
                  // to press (the callout above says which copy runs).
                  className={
                    twinVerdict(artifact, twins.get(id))?.kind === "unused" ? BUTTON.regular.grey : BUTTON.regular.default
                  }
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
  // The page on one source Banager can only list (pip): why, over the
  // list, in place of a 全选 that could tick nothing (`ReadOnlySourceLine`).
  const viewed = activeFilter === null ? undefined : instancesById.get(activeFilter);
  const readOnlySource = viewed !== undefined && !canWrite(viewed) ? viewed : null;
  // The rows the list shows that can be ticked, in its order, and those of
  // them that are: what 「卸载所选」 acts on (`countedTicks`).
  const shownTickable = rowItems.flatMap((item) =>
    item.type === "row" && tickable(item.artifact, item.instance, holdsOf(item.artifact)) ? [item.artifact] : [],
  );
  const counted = countedTicks(
    shownTickable.map((artifact) => ({ id: artifactKeyId(artifact.key), artifact })),
    selectedUninstalls,
  ).map(({ artifact }) => artifact);
  return (
    <div ref={attachPage} className="relative flex h-full">
      {/* The page's own controls, in the window's toolbar (spec §3.2):
          how the last Copy command went, for a moment; what it shows
          (every tool, or the AI coding tools) and the sort, grey popup
          buttons; and the search field, 200 wide. */}
      <ToolbarItems>
        <p role="status" className="max-w-40 truncate text-small text-muted empty:hidden">
          {copyStatusText(t, copyStatus)}
        </p>
        <ToolShowButton twins value={show} onChange={setShow} counts={discover} twinsCount={twinsInView} />
        <ToolbarPopupButton
          label={t("installed.sortLabel")}
          value={sort}
          options={[
            { value: "name", label: t("installed.sortByName") },
            { value: "source", label: t("installed.sortBySource") },
            { value: "size", label: t("sizes.sortBySize") },
            { value: "date", label: t("installed.sortByDate") },
          ]}
          onChange={setSort}
        />
        {/* 128 wide, not 200, while 「卸载（3）…」 stands beside it in a
            narrow window: the page's title keeps its room. */}
        <span className={`relative flex h-6 ${narrowInspector && counted.length > 0 ? "w-32" : "w-50"} shrink-0 items-center`}>
          <SearchIcon size={14} className="pointer-events-none absolute left-2 text-muted" />
          <input
            ref={attachSearch}
            type="search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("installed.filterPlaceholder")}
            aria-label={t("installed.filterLabel")}
            // What it searches by, as a tooltip: a command finds its tool.
            title={t("installed.searchHint")}
            // A tool's name is no word: no red underline under "ffmpeg",
            // as a web page's text field would draw, and nothing
            // corrected as it is typed.
            spellCheck={false}
            className="h-6 w-full appearance-none rounded-control bg-fill-subtle pl-7 pr-2 text-body text-foreground placeholder:text-muted [&::-webkit-search-decoration]:appearance-none"
          />
        </span>
        {/* The ticked rows the list shows, once something is ticked. Once
            everything has started, the button goes with the ticks, and the
            focus to the list. */}
        <UninstallSelectedButton
          count={counted.length}
          sheetOpen={batchUninstall.sheetOpen}
          compact={narrowInspector}
          onOpen={(opener) =>
            batchUninstall.open(
              counted.map((artifact) => ({
                artifact,
                instance: instancesById.get(artifact.key.instance_id)!,
                name: artifact.display_name,
              })),
              opener,
              () => listHandle.current?.focusFirst(),
            )
          }
        />
        {/* That nothing matches, for a screen reader, as it becomes so: one
            node for as long as the page is open, beside the field typed
            in, where the list's own line (below) is a new one each time. */}
        <p role="status" data-search-status="" className="sr-only">
          {needle !== "" && items.length === 0 && !sourceEmpty ? t("installed.noMatches", { query: query.trim() }) : null}
        </p>
      </ToolbarItems>
      <div className="flex min-w-0 flex-1 flex-col" onKeyDown={onEscape}>
        {/* The first check's hold on every Uninstall, said once over the
            list, as Software Update's one status line -- not as a word on
            each of its rows (`PREVIEW_HOLD_ID`), and not folded away among
            the sources' notices. */}
        {preview ? (
          <p data-preview-hold="" className="shrink-0 px-5 pt-2 text-small text-muted">
            <TextWithInfo text={t("clarity.previewHold")} label={t("common.detailsLabel", { title: t("clarity.previewHold") })}>
              {t("clarity.previewHoldDetail")}
            </TextWithInfo>
          </p>
        ) : null}
        {/* What the last batch did not uninstall, once it has all run. */}
        <BatchUninstallResult />
        {sourceEmpty ? null : readOnlySource !== null && shownTickable.length === 0 ? (
          <ReadOnlySourceLine instance={readOnlySource} />
        ) : (
          <InstalledSelectionHeader shown={shownTickable} counted={counted} sizes={sizes} />
        )}
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
              // source's mark at 16 -- no pill. With the whole source's
              // size, a tooltip says what it holds: other versions, which
              // the rows' own sizes leave out.
              <h2
                className="flex h-10 items-end gap-2 px-5 pb-2 text-title text-foreground"
                title={sourceTotalOf(item.instance.id) === null ? undefined : t("sizeTotals.note")}
              >
                <SourceAvatar adapterId={item.instance.adapter_id} label={item.label} size="xs" />
                <span className="min-w-0 truncate">{item.label}</span>{" "}
                {/* 「Homebrew · 33个 · 约2.6 GB」: a count with its unit,
                    Ollama's in models. */}
                {/* -ml-1: the 8 of the heading's gap less a space's 4, so
                    「·」 stands a space from the name, as from the count. */}
                <span className="-ml-1 shrink-0 text-body font-normal tabular-nums text-muted">
                  {[
                    t(item.instance.adapter_id === "ollama" ? "clarity.modelCount" : "clarity.headingCount", {
                      count: item.count,
                    }),
                    sourceTotalOf(item.instance.id),
                  ]
                    .filter((part) => part !== null)
                    .map((part) => `· ${part}`)
                    .join(" ")}
                </span>
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
                    : show === "all"
                      ? t("emptyStates.nothingInstalled.title")
                      : commandsUnknown !== null
                        ? t(commandsUnknown)
                        : activeFilter === null
                          ? t(SHOW_NONE_KEYS[show].none)
                          : t(SHOW_NONE_KEYS[show].noneInSource, { source: sourceLabelFor(activeFilter) })}
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
      <BatchUninstallSheet uninstall={batchUninstall} />
    </div>
  );
}
