import { useEffect, useId, useRef, useState } from "react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useCheckAgain, useOpenOllamaApp, useSettings, useSnapshot } from "../lib/queries";
import {
  openOllamaErrorDetail,
  openOllamaErrorMessage,
  type SourceNoticeAction,
  type SourceNoticeSpec,
} from "../lib/sources";
import { useUiStore } from "../store/ui";
import { NOTICE_GRID, SourceNotice, SourceNoticeLine, type NoticeGrid } from "./SourceNotice";
import { DisclosureIcon } from "./icons";
import { InfoDetail } from "./InfoDetail";

/**
 * The look of the fold's own buttons, 「还有N个问题」 and 「收起」: a
 * disclosure, not a link and not a push button. Words in the muted colour
 * and a 10pt disclosure triangle -- pointing right while the lines are
 * folded, turned down once they show, as the lists' other disclosures do.
 */
const FOLD_TOGGLE_CLASS = "inline-flex shrink-0 items-center rounded-sm text-body text-muted";

/** The fold's triangle: ▸ while the lines are folded, ▾ once they show. */
function FoldTriangle({ expanded }: { expanded: boolean }) {
  return <DisclosureIcon size={10} className={expanded ? "shrink-0 rotate-90" : "shrink-0"} />;
}

/**
 * A notice's Show (`showTool`): the Installed page with the source's tool
 * selected, its inspector open -- a standalone tool's one row, or else the
 * source's first that no other software brought in -- or, with nothing of
 * the source's listed, the page on that source alone, which says why.
 */
export function useShowSourceTool(): (instanceId: string) => void {
  const { data: snapshot } = useSnapshot();
  const showInstalledTool = useUiStore((s) => s.showInstalledTool);
  const openInstalled = useUiStore((s) => s.openInstalled);
  return (instanceId) => {
    const listed = (snapshot?.artifacts ?? []).filter((artifact) => artifact.key.instance_id === instanceId);
    const tool = listed.find((artifact) => artifact.reason !== "Dependency") ?? listed[0];
    if (tool === undefined) openInstalled(instanceId);
    else showInstalledTool(tool.key);
  };
}

/** Whether a page's notice lines are unfolded, and how to fold or unfold them (`useNoticeFold`). */
export interface NoticeFold {
  expanded: boolean;
  setExpanded(expanded: boolean): void;
}

/**
 * A page's fold over its notice lines, for `SourceNotices`' `fold`:
 * folded to begin with, and folded again whenever the page has a
 * different number of lines -- for good, so that the old number coming
 * back does not unfold them. Each page keeps its own, above its early
 * returns, so the fold outlives what the page draws around the lines:
 * the Updates page draws them over its list and, once no update is left
 * to list, over the sentence that says so, and a fold kept in the lines
 * themselves would be lost between the two.
 */
export function useNoticeFold(count: number): NoticeFold {
  // How many lines there were when they were unfolded; null while folded.
  const [unfoldedAt, setUnfoldedAt] = useState<number | null>(null);
  if (unfoldedAt !== null && unfoldedAt !== count) setUnfoldedAt(null);
  return {
    expanded: unfoldedAt === count,
    setExpanded: (expanded) => setUnfoldedAt(expanded ? count : null),
  };
}

export interface SourceNoticesProps {
  notices: SourceNoticeSpec[];
  /**
   * `line`: one compact line each -- icon, title, an ⓘ with the
   * description in its popover, and the button -- for the top of a list.
   * `block`: the title with the description under it, where there is room
   * to explain -- a tool's inspector.
   */
  layout?: "line" | "block";
  /** For `line`: the page's fold (`useNoticeFold`). Without it, every line shows. */
  fold?: NoticeFold;
  /** For `line`: the list's columns the lines line up with (`NoticeGrid`); the avatar's by default. */
  grid?: NoticeGrid;
  /**
   * For `line`: a hairline under the last line, as under a row -- where the
   * lines are a list's first row, over its tools. Off over a sentence that
   * says the list is empty.
   */
  separator?: boolean;
}

/**
 * Renders the notices `sourceNoticesFor` decided a source needs, and wires
 * each one's action to what carries it out: Open Ollama to its mutation,
 * Check again to the header's (`useCheckAgain`), off while a check runs,
 * and Show to the Installed page's inspector (`useShowSourceTool`).
 *
 * The split is deliberate: `sourceNoticesFor` (src/lib/sources.ts) decides
 * *what* to say from the instance alone and is pure, this decides how to
 * say it, and `SourceNoticeLine` (or `SourceNotice`) draws one. Both the
 * Installed and the Updates page render this same component -- the whole
 * point of the rule being one function is that the two pages cannot end up
 * disagreeing about whether a source has something to say, which is how
 * the Updates page came to announce "Everything is up to date" for a
 * source it had never reached.
 *
 * Under a page's `fold`, two lines or more fold into one rather than
 * stack up over the list: the first warning, or else the first line, with
 * 「还有N个问题」 and a triangle at its end for the rest. Pressed, it shows
 * every line in its order, and 「收起」 after the last folds them again.
 * The focus goes with the button, to the one that now says the other
 * thing: the one pressed is gone from where it was. Each line keeps its
 * ⓘ and its own button, folded or not.
 */
export function SourceNotices({ notices, layout = "line", fold, grid = "avatar", separator = true }: SourceNoticesProps) {
  const { t } = useTranslation();
  const openOllamaApp = useOpenOllamaApp();
  const { data: settings } = useSettings();
  // The header's Check again, and off when that one is: pressed while a
  // check runs, it would queue a second one after it.
  const { checkAgain, checking } = useCheckAgain();
  const showTool = useShowSourceTool();
  const linesId = useId();
  const toggleRef = useRef<HTMLButtonElement>(null);
  // Set by the fold's own button, and only by it: the lines folding up
  // because their number changed leave the focus where it was.
  const toggled = useRef(false);
  const expanded = fold?.expanded ?? false;
  useEffect(() => {
    if (!toggled.current) return;
    toggled.current = false;
    toggleRef.current?.focus();
  }, [expanded]);

  // Why Open Ollama did nothing, and, when the sentence leaves one for
  // it, its ⓘ. Without this a rejected Open Ollama rendered
  // nothing at all -- the same silence the backend used to produce by
  // never reading `open`'s exit status.
  let openOllamaError: ReactNode = undefined;
  if (openOllamaApp.error) {
    const message = openOllamaErrorMessage(
      t,
      openOllamaApp.error.message,
      settings?.show_technical_details ?? false,
    );
    const detail = openOllamaErrorDetail(t, openOllamaApp.error.message);
    openOllamaError =
      detail === null ? (
        message
      ) : (
        <>
          {message}
          <InfoDetail label={t("common.detailsLabel", { title: message })}>{detail}</InfoDetail>
        </>
      );
  }

  const button = (action: SourceNoticeAction) => {
    const label = t(action.labelKey);
    switch (action.id) {
      case "openOllama":
        return { label, onClick: () => openOllamaApp.mutate() };
      case "showTool":
        return { label, onClick: () => showTool(action.instanceId) };
      case "checkAgain":
        return { label, onClick: checkAgain, disabled: checking };
    }
  };

  const noticeView = (notice: SourceNoticeSpec, trailing?: ReactNode) => {
    const title = t(notice.titleKey, notice.values);
    const props = {
      variant: notice.variant,
      title,
      description: t(notice.descriptionKey, notice.values),
      action: notice.action ? button(notice.action) : undefined,
      // Only the notice whose button failed says so.
      error: notice.action?.id === "openOllama" ? openOllamaError : undefined,
    };
    return layout === "line" ? (
      <SourceNoticeLine
        key={notice.id}
        {...props}
        detailsAriaLabel={t("common.detailsLabel", { title })}
        trailing={trailing}
        grid={grid}
      />
    ) : (
      <SourceNotice key={notice.id} {...props} />
    );
  };

  if (layout !== "line") {
    return <>{notices.map((notice) => noticeView(notice))}</>;
  }
  const columns = NOTICE_GRID[grid];
  // The lines, over a hairline from where their words start to the
  // container's right edge -- 20 from the list's, as a row's -- which
  // index.css hides where a row's would be hidden: under a list's last
  // slot, and over anything but a row.
  const lines = (content: ReactNode) => (
    <div className="relative flex flex-col">
      {content}
      {separator ? (
        <span
          aria-hidden="true"
          data-row-separator=""
          className={`pointer-events-none absolute bottom-0 right-0 h-px bg-separator ${columns.hairline}`}
        />
      ) : null}
    </div>
  );

  if (fold === undefined || notices.length < 2) {
    return lines(notices.map((notice) => noticeView(notice)));
  }

  const { setExpanded } = fold;
  const toggle = (next: boolean) => {
    toggled.current = true;
    setExpanded(next);
  };

  if (!expanded) {
    const shown = notices.find((notice) => notice.variant === "warning") ?? notices[0];
    return lines(
      <div id={linesId} className="flex flex-col">
        {noticeView(
          shown,
          // Set apart from the line's ⓘ and button by more than the gap
          // between those two: it is not one of them.
          <button
            ref={toggleRef}
            type="button"
            aria-expanded={false}
            aria-controls={linesId}
            onClick={() => toggle(true)}
            className={`${FOLD_TOGGLE_CLASS} ml-3 gap-1`}
          >
            {t("sourceNotice.more", { count: notices.length - 1 })}
            <FoldTriangle expanded={false} />
          </button>,
        )}
      </div>,
    );
  }

  return lines(
    <>
      <div id={linesId} className="flex flex-col">
        {notices.map((notice) => noticeView(notice))}
      </div>
      {/* Its own line under the last, as high as a notice's and on the
          same grid: the triangle centred in the icons' column, the words
          where the titles start. */}
      <div className="flex h-8 items-center">
        <button
          ref={toggleRef}
          type="button"
          aria-expanded={true}
          aria-controls={linesId}
          onClick={() => toggle(false)}
          className={FOLD_TOGGLE_CLASS}
        >
          <span data-notice-symbol="" className={`flex shrink-0 justify-center ${columns.symbol}`}>
            <FoldTriangle expanded={true} />
          </span>
          <span className={columns.gap}>{t("sourceNotice.showFewer")}</span>
        </button>
      </div>
    </>,
  );
}
