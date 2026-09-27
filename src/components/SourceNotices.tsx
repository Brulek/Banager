import { useTranslation } from "react-i18next";
import { useOpenOllamaApp, useRefresh } from "../lib/queries";
import { openOllamaErrorMessage, type SourceNoticeSpec } from "../lib/sources";
import { SourceNotice, SourceNoticeLine } from "./SourceNotice";

export interface SourceNoticesProps {
  notices: SourceNoticeSpec[];
  /**
   * `banner`: the title, the description and the button in a box of its
   * own (the Installed page). `line`: one compact line each -- icon,
   * title, a "Details" popover with the description, and the button --
   * for the top of the Updates page.
   */
  layout?: "banner" | "line";
}

/**
 * Renders the notices `sourceNoticesFor` decided a source needs, and wires
 * each one's action to the mutation that carries it out.
 *
 * The split is deliberate: `sourceNoticesFor` (src/lib/sources.ts) decides
 * *what* to say from the instance alone and is pure, this decides how to
 * say it, and `SourceNotice` (or `SourceNoticeLine`) draws one. Both the
 * Installed and the Updates page render this same component -- the whole
 * point of the rule being one function is that the two pages cannot end up
 * disagreeing about whether a source has something to say, which is how
 * the Updates page came to announce "Everything is up to date" for a
 * source it had never reached.
 */
export function SourceNotices({ notices, layout = "banner" }: SourceNoticesProps) {
  const { t } = useTranslation();
  const openOllamaApp = useOpenOllamaApp();
  const refresh = useRefresh();

  return (
    <>
      {notices.map((notice) => {
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
          // Only the notice whose button failed says so. Without this a
          // rejected Open Ollama rendered nothing at all -- the same
          // silence the backend used to produce by never reading `open`'s
          // exit status.
          error:
            notice.action?.id === "openOllama" && openOllamaApp.error
              ? openOllamaErrorMessage(t, openOllamaApp.error.message)
              : undefined,
        };
        return layout === "line" ? (
          <SourceNoticeLine
            key={notice.id}
            {...props}
            detailsLabel={t("sourceNotice.details")}
            detailsAriaLabel={t("sourceNotice.detailsLabel", { title })}
          />
        ) : (
          <SourceNotice key={notice.id} {...props} />
        );
      })}
    </>
  );
}
