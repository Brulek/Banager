import { Fragment, useId, useState } from "react";
import { AUTO_CHECK_CHOICE_KEYS, autoCheckChoice } from "../lib/checkFrequency";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useCheckAgain, useOpenOllamaApp, useSettings, useSnapshot } from "../lib/queries";
import { isStartupSnapshot } from "../lib/events";
import { useInventoryPreview } from "../lib/inventoryPreview";
import { elapsedSince } from "../lib/format";
import { FAILURE_CAUSE_KEYS, failureCause } from "../lib/failureCause";
import {
  installedCountByInstance,
  instanceLabels,
  NOTHING_FOUND_KEYS,
  nothingFound,
  openOllamaErrorDetail,
  openOllamaErrorMessage,
  sourceNoticesFor,
  unfinishedChecksNotice,
} from "../lib/sources";
import type { SourceNoticeAction, SourceNoticeSpec } from "../lib/sources";
import { namedAsNotChecked, updatesSummary } from "../lib/updateState";
import { notCheckedHeadline } from "../lib/allGood";
import { failedLookupsOf, failedLookupsProblem, unsuccessfulLookupsOf } from "../lib/failedLookups";
import { useSystemFacts } from "../lib/diagnostics";
import { loginPathNotice } from "../lib/loginPathNotice";
import type { UpdatesSummary } from "../lib/updateState";
import type { ManagerInstance, Settings, UpdateCandidate } from "../lib/types";
import { artifactKeyId, useUiStore } from "../store/ui";
import { holdsRow, isUnderway, useUpdateOperationFor, waitsForPassword } from "../components/UpdateProgress";
import { CHECKED_KEYS, elapsedText, useMinuteClock } from "../components/PageHeader";
import { DETAILS_TRIGGER_CLASS } from "../components/SourceNotice";
import { useNoticeValues, useSearchCommand, useShowSourceTool } from "../components/SourceNotices";
import { FilledWarningIcon, StatusSymbol, type StatusSymbolKind } from "../components/StatusSymbol";
import { ChevronIcon, DisclosureIcon, InfoIcon } from "../components/icons";
import { ToolSetupRow } from "../components/ToolSetupRow";
import { Popover } from "../components/ui/Popover";
import { BUTTON, LINK } from "../components/ui/controls";
import { FORM_COLUMN, GROUP, GROUP_ROW, GROUP_WITH_ICONS, SMALL_WRAPPING } from "../components/ui/group";

/** Whatever `useTranslation()`'s `t` needs here; the same convention as `Translate` in src/lib/sources.ts. */
type Translate = (key: string, options?: Record<string, string | number>) => string;

/**
 * The status's title. A `switch` with no default, so a new summary without
 * words here fails `tsc`. With nothing to install and every source checked
 * this time, the all good (decision I22): 「所有工具都是最新的」 where
 * nothing else is listed and every source is one Banager checks, else
 * 「能在这里更新的都已是最新」, what is listed besides said under it. A source
 * not checked this time is named (`notCheckedHeadline`): 「uv这次没检查，其余
 * 都是最新的」. Where no tool was checked at all (`nothingChecked`: every
 * one installed is a row that could not be, whatever the reason -- a
 * lookup that failed this time, or one that fails the same way every
 * time, as behind a proxy whose certificate Banager does not trust), it
 * claims nothing was: 「所有工具都没有检查成功」 (walk-2 review 1.2). Else,
 * where any tool's lookup did not succeed (`unsuccessful`,
 * `unsuccessfulLookupsOf`: no answer, or one checking again will not mend
 * -- a certificate Banager does not trust, an answer that would not parse,
 * a redirect it will not follow), though the others checked fine, no
 * update listed is no news, and nothing is called up to date, the rest
 * included: 「已检查的来源中没有可更新的工具」 (independent review r6, F5).
 * Of a source named as not checked, its rows -- kept from its last
 * answer -- are its own, said under the headline (`namedAsNotChecked`):
 * a lookup of one of them that did not succeed keeps its name there.
 */
function headlineText(
  t: Translate,
  summary: UpdatesSummary,
  unsuccessful: readonly UpdateCandidate[],
  nothingChecked: boolean,
  instances: readonly ManagerInstance[],
): string {
  switch (summary.kind) {
    case "updates":
      return t("overview.updatesAvailable", { count: summary.actionable.length });
    case "updating":
      return t("overview.updating", { count: summary.count });
    case "needsPassword":
      return t("overviewPassword.title", { count: summary.count });
    case "upToDate":
      if (nothingChecked) return t("overview.nothingChecked");
      if (unsuccessful.length > 0) return t("overview.nothingToUpdateChecked");
      return summary.everything ? t("overview.upToDate") : t("overviewAllGood.upToDateHere");
    case "nothingToUpdate":
      if (nothingChecked) return t("overview.nothingChecked");
      if (unsuccessful.some((candidate) => !namedAsNotChecked(summary.notChecked, candidate.key.instance_id))) {
        return t("overview.nothingToUpdateChecked");
      }
      return notCheckedHeadline(t, summary.notChecked, summary.everythingElse, instances);
  }
}

/**
 * The status's symbol for a verdict. A `switch` with no default, as
 * `headlineText`. The green check only for the all good: not over a tool
 * whose lookup did not succeed (`unsuccessful`), whether or not checking
 * again can mend it, nor where no tool could be looked up
 * (`nothingChecked`).
 */
function symbolOf(summary: UpdatesSummary, unsuccessful: number, nothingChecked: boolean): StatusSymbolKind {
  switch (summary.kind) {
    case "updates":
      return "updates";
    case "upToDate":
      return unsuccessful > 0 || nothingChecked ? "quiet" : "upToDate";
    case "updating":
      return "busy";
    // The warning glyph: updates are waiting on the user, in Terminal.
    case "needsPassword":
      return "failed";
    case "nothingToUpdate":
      return "quiet";
  }
}

/**
 * The line under the all good, under a source named as not checked, and
 * under "N updates need your password": what there is besides, in the
 * Updates page's own numbers (`updatesSummary`) -- the updates the user
 * hid, the ones under its "Can't update here", the rows of a copy
 * Terminal does not run that no number counts (「1个终端用不到」, decision
 * U4) -- or null when there is none of that. A source not checked in
 * full, and a check that did not finish, say so in the problems group
 * under the status instead.
 *
 * The Updates page lists no hidden update, and Settings lists them all,
 * under 「已跳过的版本」 and 「不再提醒的工具」: their count is the way
 * there (`showHidden`), a link in the line -- the one link on the page.
 * The rest of it is text: of those it can't update here, how many it
 * could not look up (`unsuccessful`: every lookup that did not succeed,
 * whether or not checking again can mend it) -- what keeps the all good
 * away, said plainly (independent review r6, F5).
 */
function nothingToUpdateLine(
  t: Translate,
  summary: Extract<UpdatesSummary, { kind: "upToDate" | "nothingToUpdate" | "needsPassword" }>,
  showHidden: () => void,
  unsuccessful: number,
): ReactNode {
  const parts: ReactNode[] = [];
  if (summary.hidden > 0) {
    parts.push(
      <button type="button" onClick={showHidden} className={LINK}>
        {t("overview.hiddenCount", { count: summary.hidden })}
      </button>,
    );
  }
  if (summary.cantUpdateHere > 0) {
    parts.push(
      unsuccessful > 0
        ? t("overview.cantUpdateHereUncheckedCount", { number: summary.cantUpdateHere, unchecked: unsuccessful })
        : t("overview.cantUpdateHereCount", { count: summary.cantUpdateHere }),
    );
  }
  if (summary.notUsed > 0) parts.push(t("notUsedCopy.count", { count: summary.notUsed }));
  if (parts.length === 0) return null;
  return parts.map((part, index) => (
    <Fragment key={index}>
      {index > 0 ? t("overview.listSeparator") : null}
      {part}
    </Fragment>
  ));
}

/**
 * One source's problem, a row of the group under the status: the notice
 * the Updates and Installed pages give it (`sourceNoticesFor`, in their
 * words) -- a filled orange ⚠︎ for a warning, a muted ⓘ for news -- its
 * title, its description under it, and on the right its own button where
 * it has one: Open Ollama, Check again, Show (a tool, or the tools that
 * answer to one command), wired as the lists wire
 * theirs (`SourceNotices`), Check again off while a check runs. A press of
 * Open Ollama that failed says so under the description, with its Details.
 */
function ProblemRow({ notice }: { notice: SourceNoticeSpec }) {
  const { t } = useTranslation();
  // In the lists' words, when its source last answered included.
  const values = useNoticeValues([notice])(notice);
  const openOllamaApp = useOpenOllamaApp();
  const { data: settings } = useSettings();
  const { checkAgain, checking } = useCheckAgain();
  const showTool = useShowSourceTool();
  const searchCommand = useSearchCommand();
  const openInstalled = useUiStore((s) => s.openInstalled);
  const setInstalledShow = useUiStore((s) => s.setInstalledShow);
  const action = notice.action;
  const opensOllama = action?.id === "openOllama";
  // The button's description is the row's title, as on the lists
  // (`SourceNoticeLine`): several rows can each have a 查看, and a screen
  // reader's list of buttons tells them apart by what each is about.
  const titleId = useId();
  // What the button does, as the lists wire theirs (`SourceNotices`), one
  // case per action: one added to `SourceNoticeAction` without a case here
  // fails `tsc` at the `never`. 查看 (`showList`) is the Installed page's
  // own line, which no source's notice here carries; given one, it would
  // open that page on the list it names.
  const press = (pressed: SourceNoticeAction): (() => void) => {
    switch (pressed.id) {
      case "openOllama":
        return () => openOllamaApp.mutate();
      case "showTool":
        return () => showTool(pressed.instanceId);
      case "searchCommand":
        return () => searchCommand(pressed.command);
      case "checkAgain":
        return checkAgain;
      case "showList":
        return () => {
          openInstalled(null);
          setInstalledShow(pressed.show);
        };
      default: {
        const unhandled: never = pressed;
        return unhandled;
      }
    }
  };

  let error: ReactNode = null;
  if (opensOllama && openOllamaApp.error) {
    const message = openOllamaErrorMessage(
      t,
      openOllamaApp.error.message,
      settings?.show_technical_details ?? false,
    );
    const detail = openOllamaErrorDetail(t, openOllamaApp.error.message);
    error =
      detail === null ? (
        message
      ) : (
        <>
          {message}{" "}
          <Popover
            trigger={t("common.details")}
            triggerLabel={t("common.detailsLabel", { title: message })}
            triggerClassName={DETAILS_TRIGGER_CLASS}
          >
            {detail}
          </Popover>
        </>
      );
  }

  return (
    <li className="flex min-h-11.5 items-center gap-4 px-2.5 py-2">
      <div className="flex min-w-0 flex-1 items-center gap-2">
        {notice.variant === "warning" ? (
          <FilledWarningIcon size={16} className="text-warning" />
        ) : (
          <InfoIcon size={16} className="shrink-0 text-muted" />
        )}
        <div className="min-w-0">
          <p id={titleId} className="text-body text-foreground">
            {t(notice.titleKey, values)}
          </p>
          <p className={`${SMALL_WRAPPING} text-muted`}>{t(notice.descriptionKey, values)}</p>
          {/* A <div>: the error's own "Details" panel is one. */}
          {error !== null ? (
            <div role="alert" className="text-small text-danger-text">
              {error}
            </div>
          ) : null}
        </div>
      </div>
      {action ? (
        <button
          type="button"
          onClick={press(action)}
          disabled={action.id === "checkAgain" && checking}
          aria-describedby={titleId}
          className={BUTTON.regular.grey}
        >
          {/* Named for what it shows, from the notice's values: Show
              “codex” (walk-3 W3-5). */}
          {t(action.labelKey, values)}
        </button>
      ) : null}
    </li>
  );
}

/**
 * The group of problems under the daily check: one row per source that
 * has something to say (`ProblemRow`). The warnings, which want something
 * of the user, are rows of their own, first. The news -- a list still
 * downloading, what typing a command runs -- is not a fault, and five
 * rows of it at once read as a broken Mac: it folds into one last row,
 * 「另有N条提示」 in the muted colour after a 10pt disclosure triangle, as
 * the Installed page folds the components that came with other software.
 * Pressed, the row stays where it is, turns its triangle down and says
 * 「收起N条提示」, and the notes show under it, each with its own button;
 * pressed again, they fold. With no warning, the group is that one row.
 */
function ProblemsGroup({ problems }: { problems: SourceNoticeSpec[] }) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const warnings = problems.filter((notice) => notice.variant === "warning");
  const notes = problems.filter((notice) => notice.variant !== "warning");
  return (
    <ul aria-label={t("overview.attentionLabel")} className={GROUP_WITH_ICONS}>
      {warnings.map((notice) => (
        <ProblemRow key={notice.id} notice={notice} />
      ))}
      {notes.length > 0 ? (
        <li>
          <button
            type="button"
            aria-expanded={expanded}
            onClick={() => setExpanded(!expanded)}
            className="flex min-h-9 w-full items-center gap-2 px-2.5 py-1.5 text-left text-body text-muted"
          >
            {/* The triangle centred in the symbols' 16 column, the words
                where the rows' titles start. */}
            <span className="flex w-4 shrink-0 justify-center">
              <DisclosureIcon size={10} className={expanded ? "shrink-0 rotate-90" : "shrink-0"} />
            </span>
            {t(expanded ? "overview.hideNotes" : "overview.moreNotes", { count: notes.length })}
          </button>
        </li>
      ) : null}
      {expanded
        ? notes.map((notice) => <ProblemRow key={notice.id} notice={notice} />)
        : null}
    </ul>
  );
}

/**
 * The status row, the first group's one row: a 48 symbol, the title in
 * 13 bold with a line under it in 11 muted, and on the right the row's one
 * button. `alert`: the title and its line are read out as they appear --
 * mounted afresh, so that a screen reader says them. The line is a <div>:
 * a 「详情」 in it opens a panel that is one.
 */
function StatusRow({
  symbol,
  title,
  line,
  button,
  alert = false,
}: {
  symbol: StatusSymbolKind;
  title: string;
  line: ReactNode;
  button: ReactNode;
  alert?: boolean;
}) {
  return (
    <div className={GROUP}>
      <div data-status={symbol} className="flex items-center gap-3 px-2.5 py-2.5">
        <StatusSymbol kind={symbol} />
        <div key={alert ? "alert" : "status"} role={alert ? "alert" : undefined} className="min-w-0 flex-1">
          <h2 className="text-title text-foreground">{title}</h2>
          {line !== null ? (
            <div data-status-line="" className={`${SMALL_WRAPPING} text-muted`}>
              {line}
            </div>
          ) : null}
        </div>
        {button}
      </div>
    </div>
  );
}

/**
 * The automatic check, as Software Update shows its automatic updates --
 * the choice Settings' popup shows, 「不自动检查」, 「每天」 or 「每周」
 * (src/lib/checkFrequency.ts) -- a row that opens Settings, where it is
 * changed. A screen reader hears it as a button named by both,
 * 「检查更新：每周」, and what pressing it does: it is no popup.
 */
function AutoCheckRow({ settings }: { settings: Settings }) {
  const { t } = useTranslation();
  const setPage = useUiStore((s) => s.setPage);
  const hintId = useId();
  // 「检查更新的频率」 here, not Settings' 「检查更新」: on its own, over a
  // button-like row, that reads as a button that checks now. In English
  // Settings' own "Check for updates", with the choice beside it as
  // there: "Update checks" was a second name for one setting (walk-3 W3-14).
  const label = t("overviewMore.autoCheckLabel");
  const value = t(AUTO_CHECK_CHOICE_KEYS[autoCheckChoice(settings)]);
  return (
    <div className={GROUP}>
      <button
        type="button"
        onClick={() => setPage("settings")}
        aria-label={t("overview.autoCheckRowLabel", { label, value })}
        aria-describedby={hintId}
        className={`${GROUP_ROW} w-full text-left`}
      >
        <span className="min-w-0 truncate text-body text-foreground">{label}</span>
        <span className="flex shrink-0 items-center gap-1 text-body text-muted">
          <span>{value}</span>
          <ChevronIcon size={14} className="text-tertiary" />
        </span>
        <span id={hintId} hidden>
          {t("overview.autoCheckOpensSettings")}
        </span>
      </button>
      <ToolSetupRow />
    </div>
  );
}

/**
 * The first page, laid out as System Settings' Software Update (spec R1):
 * a column of groups, as wide as Settings' and at its top.
 *
 * First, one row: where the updates stand (`StatusRow`). The Updates
 * page's own verdict (`updatesSummary`) as its title -- how many updates
 * it offers to install, "Everything is up to date" only when that page
 * would say so, and a plain "Nothing to update" when there is nothing to
 * install but that is not the same thing ("No updates in the sources
 * checked" where a source was not checked in full) -- with a symbol for it
 * on the left (`StatusSymbol`) and a line under it: when the sources were
 * last checked, or what there is instead of updates
 * (`nothingToUpdateLine`). On the right, always one button, the one thing
 * to do: Review Updates, the default button, which opens the Updates page
 * with all of them selected; a grey one where that page lists only what
 * cannot be updated here; See Progress while they install; otherwise a
 * grey Check Again, as macOS's empty states offer, off while a check runs.
 * The number of updates is said once, in the title. Where the check found
 * nothing to show (`nothingFound`) -- no source, or nothing installed --
 * the row says that, a muted ⓘ beside it, its sentence under it with
 * 「详情」 on what Banager works with, and a grey Check Again.
 *
 * While a check runs, the row keeps what the last one found -- its symbol,
 * its title, its button -- and only its line says 「正在检查…」: the
 * toolbar's ⟳ is already turning. When the last check failed --
 * `startupRefreshError`, which every refresh sets or clears -- the row
 * says so instead, as an alert, with its reason, and Check Again becomes
 * its default button: what is listed is from the check before it. Before
 * the first check has answered, the same row, turning, says 「正在检查…」
 * and why that takes a while -- the startup placeholder is not an answer
 * (`isStartupSnapshot`) -- so that nothing on the page moves when the
 * answer comes. Once that check has listed what is installed, while it
 * still checks for updates (`useInventoryPreview`), its line says how many
 * tools it found, and a grey See Tools opens the Installed page on them.
 *
 * Second, the daily check, on or off (`AutoCheckRow`).
 *
 * Last, only when a source has something to say, a group of one row for
 * each: its first warning, or else its first notice (`sourceNoticesFor`:
 * not running, not answering, a list it could not download, another
 * program that runs instead), with its own button (`ProblemRow`) -- the
 * warnings first, the notes folded into one row after them
 * (`ProblemsGroup`). The checks that did not finish this round are its
 * first row (`unfinishedChecksNotice`), as they are the lists' first line;
 * then the tools the check could not look up (`failedLookupsProblem`):
 * that updates may be missing, why and what to do, with Check Again --
 * and the status's line says how many, in place of when the check was,
 * which alone read as a check that had worked (walk-2 W2-1). What a source lets Banager do at all,
 * pip being read-only, is not news here; both lists say it on each of its
 * rows. The sources themselves are in the sidebar.
 */
export function OverviewPage() {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const { data: settings } = useSettings();
  const setPage = useUiStore((s) => s.setPage);
  const showHiddenUpdates = useUiStore((s) => s.showHiddenUpdates);
  const selectUpdates = useUiStore((s) => s.selectUpdates);
  const checkFailure = useUiStore((s) => s.startupRefreshError);
  const { checkAgain, checking } = useCheckAgain();
  const operationFor = useUpdateOperationFor();
  const refreshedAt = snapshot?.refreshed_at ?? null;
  const now = useMinuteClock(refreshedAt);
  // What the first check has found installed, while it still checks for
  // updates: how many, and the way to them -- never a word on updates.
  const preview = useInventoryPreview();
  const openPage = useUiStore((s) => s.openPage);
  // Whether the login shell's `PATH` was read: where it was not, sources
  // Terminal finds may be missing, said first among the problems.
  const { data: facts } = useSystemFacts();

  if (!snapshot || !settings || isStartupSnapshot(snapshot)) {
    const seeTools =
      preview === null ? null : (
        <button type="button" onClick={() => openPage("installed")} className={BUTTON.regular.grey}>
          {t("inventoryPreview.seeTools")}
        </button>
      );
    const line =
      preview === null
        ? t("common.firstCheckDetail")
        : t("inventoryPreview.overviewLine", { count: preview.artifacts.length });
    return (
      <div className={FORM_COLUMN}>
        <StatusRow symbol="busy" title={t("common.checking")} line={line} button={seeTools} />
        {settings ? <AutoCheckRow settings={settings} /> : null}
      </div>
    );
  }

  // Each source by the name the sidebar gives it (`instanceLabels`): where
  // this Mac has two Homebrews, which one a line is about, 「Homebrew（Intel）
  // 没有响应」, never two lines of one name.
  const labels = instanceLabels(t, snapshot.instances);
  const labelOf = (instance: ManagerInstance): string => labels.get(instance.id) ?? instance.adapter_id;

  const installedByInstance = installedCountByInstance(snapshot.artifacts);

  const summary = updatesSummary(
    snapshot,
    settings,
    (candidate) => holdsRow(operationFor(candidate)),
    (candidate) => isUnderway(operationFor(candidate)),
    (candidate) => waitsForPassword(operationFor(candidate)),
  );

  // First, the checks that did not finish this round, as the lists' first
  // line says them (`unfinishedChecksNotice`); then each source's first
  // warning, or else its first notice: a warning must not fold away with
  // the notes (`ProblemsGroup`) behind one of its own.
  const unfinished = unfinishedChecksNotice(t, snapshot.errors, snapshot.instances);
  // The tools this check could not look up, as the Updates page counts
  // them: never in the headline's number, which is what can be updated.
  const lookupsFailed = failedLookupsOf(snapshot.updates, settings);
  const lookups = failedLookupsProblem(t, lookupsFailed);
  // Every tool whose lookup did not succeed, whether or not checking again
  // can mend it: what keeps the all good away (independent review r6, F5).
  // Only those checking again can mend (`lookupsFailed`) offer Check Again.
  const unsuccessfulRows = unsuccessfulLookupsOf(snapshot.updates, settings);
  const unsuccessful = unsuccessfulRows.length;
  // Whether no installed tool was checked at all: each is a row that could
  // not be (`checkable: false`), whatever the reason.
  const unchecked = new Set(
    snapshot.updates.filter((candidate) => !candidate.checkable).map((candidate) => artifactKeyId(candidate.key)),
  );
  const nothingChecked =
    snapshot.artifacts.length > 0 && snapshot.artifacts.every((artifact) => unchecked.has(artifactKeyId(artifact.key)));
  const loginPath = loginPathNotice(facts);
  const problems: SourceNoticeSpec[] = [
    ...(loginPath === null ? [] : [loginPath]),
    ...(unfinished === null ? [] : [unfinished]),
    ...(lookups === null ? [] : [lookups]),
    ...snapshot.instances.flatMap((instance) => {
      const notices = sourceNoticesFor(instance, labelOf(instance), installedByInstance.get(instance.id) ?? 0);
      const notice = notices.find((each) => each.variant === "warning") ?? notices[0];
      return notice === undefined ? [] : [notice];
    }),
  ];

  // A check that failed says so until one works, but not while the next
  // one runs.
  const failed = !checking && checkFailure !== null;
  // A check that found nothing to show -- no source, or nothing installed
  // (`nothingFound`, the rule the lists' empty states read): said in this
  // row, as every other state is, not in a view of its own in the middle
  // of the page.
  const found = nothingFound(t, snapshot);
  const lastChecked = refreshedAt === null ? null : elapsedText(t, CHECKED_KEYS, elapsedSince(refreshedAt, now));
  let line: ReactNode = null;
  if (failed) {
    // Why, in a person's words, where the message says (spec R10): 「网络
    // 连接失败，请检查网络连接后重试。」, 「Homebrew正在更新软件清单，请稍后
    // 再试。」. Otherwise its own words only with technical details on --
    // they name commands and are often English -- and else only when to
    // try again.
    const cause = failureCause(checkFailure);
    line =
      cause !== null
        ? t(FAILURE_CAUSE_KEYS[cause].line)
        : settings.show_technical_details
          ? t("emptyStates.loadFailed.description", { message: checkFailure })
          : t("overview.checkFailedTryLater");
  } else if (checking) {
    line = t("common.checking");
  } else if (found !== null) {
    // Its one sentence, and what Banager works with and where it looks
    // behind 「详情」, a link: the page's one.
    const title = t(NOTHING_FOUND_KEYS[found].title);
    line = (
      <>
        {t(NOTHING_FOUND_KEYS[found].description)}{" "}
        <Popover
          trigger={t("common.details")}
          triggerLabel={t("common.detailsLabel", { title })}
          triggerClassName={DETAILS_TRIGGER_CLASS}
        >
          {t("emptyStates.supportedList")}
        </Popover>
      </>
    );
  } else if (summary.kind === "upToDate" || summary.kind === "nothingToUpdate" || summary.kind === "needsPassword") {
    line = nothingToUpdateLine(t, summary, showHiddenUpdates, unsuccessful) ?? lastChecked;
  } else if (summary.kind === "updating") {
    // How many wait for the password, as the Updates page's headline goes
    // on after its "Updating N tools": 「13个需要输入密码」.
    line = summary.password > 0 ? t("updates.needPasswordCount", { count: summary.password }) : null;
  } else {
    // Those waiting for the password, as the Updates page's headline says
    // them after its count; then, where part of the check failed, how many
    // were not looked up -- not when the check was: 「上次检查：刚才」 alone
    // read as a check that had worked.
    const parts: string[] = [];
    if (summary.kind === "updates" && summary.password > 0) {
      parts.push(t("updates.needPasswordCount", { count: summary.password }));
    }
    if (lookupsFailed.length > 0) parts.push(t("updates.lookupsFailedTitle", { count: lookupsFailed.length }));
    line = parts.length > 0 ? parts.join(t("overview.listSeparator")) : lastChecked;
  }

  const checkAgainButton = (kind: "grey" | "default") => (
    <button type="button" onClick={checkAgain} disabled={checking} className={BUTTON.regular[kind]}>
      {t("header.checkAgain")}
    </button>
  );
  let button: ReactNode;
  if (failed) {
    // The one thing to do now: check again.
    button = checkAgainButton("default");
  } else if (found !== null) {
    // Nothing to review: check again, once something is installed.
    button = checkAgainButton("grey");
  } else if (summary.kind === "updates") {
    button = (
      <button
        type="button"
        onClick={() => {
          // Every row the Updates page's Update All would tick
          // (`countedUpdatesOf`: not a copy Terminal does not run), and no
          // other; any row selected earlier stays as it was.
          selectUpdates(summary.actionable.map((candidate) => candidate.key));
          setPage("updates");
        }}
        className={BUTTON.regular.default}
      >
        {t("overview.reviewUpdates")}
      </button>
    );
  } else if (summary.kind === "needsPassword") {
    // Each row has the steps for Terminal (「查看步骤」): the Updates page.
    button = (
      <button type="button" onClick={() => setPage("updates")} className={BUTTON.regular.default}>
        {t("overview.reviewUpdates")}
      </button>
    );
  } else if (summary.kind === "updating") {
    // Where each one's progress is: in its own row.
    button = (
      <button type="button" onClick={() => setPage("updates")} className={BUTTON.regular.grey}>
        {t("overview.seeProgress")}
      </button>
    );
  } else if (
    (summary.kind === "upToDate" || summary.kind === "nothingToUpdate") &&
    (summary.cantUpdateHere > 0 || summary.notUsed > 0)
  ) {
    // The Updates page has rows to show, every one under "Can't update
    // here" or of a copy Terminal does not run, which Update All leaves
    // unticked: nothing to select, only a page to open.
    button = (
      <button type="button" onClick={() => setPage("updates")} className={BUTTON.regular.grey}>
        {t("overview.reviewUpdates")}
      </button>
    );
  } else {
    button = checkAgainButton("grey");
  }

  return (
    <div className={FORM_COLUMN}>
      <StatusRow
        symbol={failed ? "failed" : found !== null ? "info" : symbolOf(summary, unsuccessful, nothingChecked)}
        title={
          failed
            ? t("header.checkFailed")
            : found !== null
              ? t(NOTHING_FOUND_KEYS[found].title)
              : headlineText(t, summary, unsuccessfulRows, nothingChecked, snapshot.instances)
        }
        line={line}
        button={button}
        alert={failed}
      />

      <AutoCheckRow settings={settings} />

      {problems.length > 0 ? <ProblemsGroup problems={problems} /> : null}
    </div>
  );
}
