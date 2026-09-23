import { useTranslation } from "react-i18next";
import { useOpenOllamaApp, useRefresh } from "../lib/queries";
import { openOllamaErrorMessage, type SourceNoticeSpec } from "../lib/sources";
import { SourceNotice } from "./SourceNotice";

export interface SourceNoticesProps {
  notices: SourceNoticeSpec[];
}

/**
 * Renders the banners `sourceNoticesFor` decided a source needs, and wires
 * each one's action to the mutation that carries it out.
 *
 * The split is deliberate: `sourceNoticesFor` (src/lib/sources.ts) decides
 * *what* to say from the instance alone and is pure, this decides how to
 * say it, and `SourceNotice` draws one. Both the Installed and the Updates
 * page render this same component -- the whole point of the rule being one
 * function is that the two pages cannot end up disagreeing about whether a
 * source has something to say, which is how the Updates page came to
 * announce "Everything is up to date" for a source it had never reached.
 */
export function SourceNotices({ notices }: SourceNoticesProps) {
  const { t } = useTranslation();
  const openOllamaApp = useOpenOllamaApp();
  const refresh = useRefresh();

  return (
    <>
      {notices.map((notice) => (
        <SourceNotice
          key={notice.id}
          variant={notice.variant}
          title={t(notice.titleKey, notice.values)}
          description={t(notice.descriptionKey, notice.values)}
          action={
            notice.action
              ? {
                  label: t(notice.action.labelKey),
                  onClick:
                    notice.action.id === "openOllama"
                      ? () => openOllamaApp.mutate()
                      : () => refresh.mutate(),
                }
              : undefined
          }
          // Only the notice whose button failed says so. Without this a
          // rejected Open Ollama rendered nothing at all -- the same
          // silence the backend used to produce by never reading `open`'s
          // exit status.
          error={
            notice.action?.id === "openOllama" && openOllamaApp.error
              ? openOllamaErrorMessage(t, openOllamaApp.error.message)
              : undefined
          }
        />
      ))}
    </>
  );
}
