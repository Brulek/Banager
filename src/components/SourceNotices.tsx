import { useEffect, useId, useRef, useState } from "react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useOpenOllamaApp, useRefresh } from "../lib/queries";
import { openOllamaErrorDetail, openOllamaErrorMessage, type SourceNoticeSpec } from "../lib/sources";
import { DETAILS_TRIGGER_CLASS, SourceNotice, SourceNoticeLine } from "./SourceNotice";
import { Popover } from "./ui/Popover";

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
   * `line`: one compact line each -- icon, title, a "Details" popover
   * with the description, and the button -- for the top of a list.
   * `block`: the title with the description under it, where there is room
   * to explain -- a tool's detail drawer.
   */
  layout?: "line" | "block";
  /** For `line`: the page's fold (`useNoticeFold`). Without it, every line shows. */
  fold?: NoticeFold;
}

/**
 * Renders the notices `sourceNoticesFor` decided a source needs, and wires
 * each one's action to the mutation that carries it out.
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
 * 「还有 N 条」 at its end for the rest. Pressed, it shows every line in its
 * order, and 「收起」 after the last folds them again. The focus goes with
 * the button, to the one that now says the other thing: the one pressed
 * is gone from where it was. Each line keeps its "Details" and its own
 * button, folded or not.
 */
export function SourceNotices({ notices, layout = "line", fold }: SourceNoticesProps) {
  const { t } = useTranslation();
  const openOllamaApp = useOpenOllamaApp();
  const refresh = useRefresh();
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
  // it, its "Details". Without this a rejected Open Ollama rendered
  // nothing at all -- the same silence the backend used to produce by
  // never reading `open`'s exit status.
  let openOllamaError: ReactNode = undefined;
  if (openOllamaApp.error) {
    const message = openOllamaErrorMessage(t, openOllamaApp.error.message);
    const detail = openOllamaErrorDetail(t, openOllamaApp.error.message);
    openOllamaError =
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

  const noticeView = (notice: SourceNoticeSpec, trailing?: ReactNode) => {
    const title = t(notice.titleKey, notice.values);
    const props = {
      variant: notice.variant,
      title,
      description: t(notice.descriptionKey, notice.values),
      action: notice.action
        ? {
            label: t(notice.action.labelKey),
            onClick:
              notice.action.id === "openOllama"
                ? () => openOllamaApp.mutate()
                : () => refresh.mutate(),
          }
        : undefined,
      // Only the notice whose button failed says so.
      error: notice.action?.id === "openOllama" ? openOllamaError : undefined,
    };
    return layout === "line" ? (
      <SourceNoticeLine
        key={notice.id}
        {...props}
        detailsLabel={t("common.details")}
        detailsAriaLabel={t("common.detailsLabel", { title })}
        trailing={trailing}
      />
    ) : (
      <SourceNotice key={notice.id} {...props} />
    );
  };

  if (layout !== "line" || fold === undefined || notices.length < 2) {
    return <>{notices.map((notice) => noticeView(notice))}</>;
  }

  const { setExpanded } = fold;
  const toggle = (next: boolean) => {
    toggled.current = true;
    setExpanded(next);
  };

  if (!expanded) {
    const shown = notices.find((notice) => notice.variant === "warning") ?? notices[0];
    return (
      <div id={linesId} className="flex flex-col gap-1.5">
        {noticeView(
          shown,
          <button
            ref={toggleRef}
            type="button"
            aria-expanded={false}
            aria-controls={linesId}
            onClick={() => toggle(true)}
            className={DETAILS_TRIGGER_CLASS}
          >
            {t("sourceNotice.more", { count: notices.length - 1 })}
          </button>,
        )}
      </div>
    );
  }

  return (
    <>
      <div id={linesId} className="flex flex-col gap-1.5">
        {notices.map((notice) => noticeView(notice))}
      </div>
      {/* Under the titles, past the icons. */}
      <div className="pl-6">
        <button
          ref={toggleRef}
          type="button"
          aria-expanded={true}
          aria-controls={linesId}
          onClick={() => toggle(false)}
          className={DETAILS_TRIGGER_CLASS}
        >
          {t("sourceNotice.showFewer")}
        </button>
      </div>
    </>
  );
}
