import { useTranslation } from "react-i18next";
import { useHistory } from "../lib/history";
import type { HistoryView } from "../lib/types";
import { SourceNoticeLine } from "./SourceNotice";

/**
 * What the Updates page says of where its Update History is kept, by the
 * history's own answer (Rust `HistoryPersistence`): the key under
 * `updates.justUpdated.storage`, or null when there is nothing to say --
 * every change saved, one being written, or no answer yet. Not an answer
 * not yet in (`useHistory`'s data undefined), nor the stand-in for none
 * (`NO_HISTORY`, with no `persistence`): a notice there would flash at
 * every launch for a history that is fine.
 */
export function historyNoticeKey(view: HistoryView | undefined): string | null {
  switch (view?.persistence) {
    // A file that is there and is left as it is for this launch (r24
    // R74-1), and the launches that keep their records in memory only.
    case "unreadable":
      return "unreadable";
    case "malformed":
      return "malformed";
    case "newer":
      return "newer";
    case "writer_unavailable":
      return "writerUnavailable";
    case "unattached":
      return "unattached";
    case "write_failed":
    case "saved":
    case "pending":
    case undefined:
      return null;
  }
}

/**
 * The Updates page's line about its Update History when it is not being
 * saved, over the page and outside `SnapshotStatus` (src/App.tsx): it is
 * said whatever the page below shows -- a list, an empty Mac, the first
 * check, a check that failed -- since what it says is true of every one.
 * The short sentence on the line, and what to do in its ⓘ.
 */
export function HistoryNotice() {
  const { t } = useTranslation();
  const { data } = useHistory();
  const key = historyNoticeKey(data);
  if (key === null) return null;
  const title = t(`updates.justUpdated.storage.${key}`);
  return (
    <div data-history-notice="" role="status" className="px-5 pb-1">
      <SourceNoticeLine
        variant="warning"
        grid="checkbox"
        title={title}
        description={t(`updates.justUpdated.storage.${key}Detail`)}
        detailsAriaLabel={t("common.detailsLabel", { title })}
      />
    </div>
  );
}
