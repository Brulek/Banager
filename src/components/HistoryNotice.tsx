import { useTranslation } from "react-i18next";
import { useHistory, useHistoryClearState } from "../lib/history";
import type { HistoryView } from "../lib/types";
import { SourceNoticeLine } from "./SourceNotice";

/** Each notice's line, and what its ⓘ says to do. */
const WORDS = {
  unreadable: ["updates.justUpdated.storage.unreadable", "updates.justUpdated.storage.unreadableDetail"],
  malformed: ["updates.justUpdated.storage.malformed", "updates.justUpdated.storage.malformedDetail"],
  newer: ["updates.justUpdated.storage.newer", "updates.justUpdated.storage.newerDetail"],
  writerUnavailable: [
    "updates.justUpdated.storage.writerUnavailable",
    "updates.justUpdated.storage.writerUnavailableDetail",
  ],
  unattached: ["updates.justUpdated.storage.unattached", "updates.justUpdated.storage.unattachedDetail"],
  writeFailed: ["updates.justUpdated.storage.writeFailed", "updates.justUpdated.storage.writeFailedDetail"],
  clearUnsaved: ["updates.justUpdated.storage.clearUnsaved", "updates.justUpdated.storage.clearUnsavedDetail"],
  clearUnconfirmed: [
    "updates.justUpdated.storage.clearUnconfirmed",
    "updates.justUpdated.storage.clearUnconfirmedDetail",
  ],
} as const;

export type HistoryNoticeKey = keyof typeof WORDS;

/**
 * What the Updates page says of where its Update History is kept, by the
 * history's own answer (Rust `HistoryPersistence`): the key under
 * `updates.justUpdated.storage`, or null when there is nothing to say --
 * every change saved, one being written, or no answer yet. Not an answer
 * not yet in (`useHistory`'s data undefined), nor the stand-in for none
 * (`NO_HISTORY`, with no `persistence`): a notice there would flash at
 * every launch for a history that is fine. A write under way says nothing
 * either: it ends in a moment, saved or failed. `clearUnconfirmed`: the
 * last Clear History went unanswered (`useHistoryClearState`).
 */
export function historyNoticeKey(view: HistoryView | undefined, clearUnconfirmed = false): HistoryNoticeKey | null {
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
    // A write that failed, still owed (r24 R74-2): a Clear not in the
    // file is said as such -- what it hid comes back after a restart --
    // and, once that Clear is written, a later record that failed as any
    // other write.
    case "write_failed":
      return view.unsaved_clear ? "clearUnsaved" : "writeFailed";
    case "saved":
    case "pending":
    case undefined:
      return clearUnconfirmed ? "clearUnconfirmed" : null;
  }
}

/**
 * The Updates page's line about its Update History when it is not being
 * saved, or a Clear History was not, over the page and outside
 * `SnapshotStatus` (src/App.tsx): it is said whatever the page below
 * shows -- a list, an empty Mac, the first check, a check that failed --
 * since what it says is true of every one.
 * The short sentence on the line, and what to do in its ⓘ.
 */
export function HistoryNotice() {
  const { t } = useTranslation();
  const { data } = useHistory();
  const clearUnconfirmed = useHistoryClearState((state) => state.unconfirmed);
  const key = historyNoticeKey(data, clearUnconfirmed);
  if (key === null) return null;
  const [titleKey, detailKey] = WORDS[key];
  const title = t(titleKey);
  return (
    <div data-history-notice="" role="status" className="px-5 pb-1">
      <SourceNoticeLine
        variant="warning"
        grid="checkbox"
        title={title}
        description={t(detailKey)}
        detailsAriaLabel={t("common.detailsLabel", { title })}
      />
    </div>
  );
}
