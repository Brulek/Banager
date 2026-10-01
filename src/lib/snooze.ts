/**
 * The words for an update put off with "Remind Me in 30 Days"
 * (「30天内不提醒」): the date it comes back, as the Installed page's
 * chip and Settings' list of snoozed updates say it -- 「11月1日起恢复提醒」.
 */
import type { TFunction } from "i18next";
import { artifactKeyId } from "../store/ui";
import type { ArtifactKey, Settings, SnoozedUpdate } from "./types";

/** The snooze of `key` in `settings`, if there is one. */
export function snoozeOf(settings: Pick<Settings, "snoozed_updates">, key: ArtifactKey): SnoozedUpdate | undefined {
  const id = artifactKeyId(key);
  return (settings.snoozed_updates ?? []).find((snoozed) => artifactKeyId(snoozed.key) === id);
}

/**
 * `until` (Unix seconds) as a day of the month in `language`: 「11月1日」,
 * "Nov 1" -- the Overview's next-check line's style for a date. A snooze
 * lasts 30 days, so the year goes without saying.
 */
export function snoozeDate(until: number, language: string): string {
  return new Intl.DateTimeFormat(language, { month: "short", day: "numeric" }).format(new Date(until * 1000));
}

/** 「11月1日起恢复提醒」, "Hidden until Nov 1". */
export function snoozedUntilText(t: TFunction, until: number, language: string): string {
  return t("updates.snoozedUntil", { date: snoozeDate(until, language) });
}
