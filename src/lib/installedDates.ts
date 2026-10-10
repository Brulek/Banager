import { mediumDateText, shortDateText } from "./shortDate";
import type { InstalledArtifact } from "./types";

/**
 * "By Date Installed": the newer first; a row whose source reported no
 * date -- most but Homebrew's report none -- after every row with one, so
 * that 0 means "the same day and second, or neither has one", for the
 * caller to order by name.
 */
export function compareByInstalledAt(a: InstalledArtifact, b: InstalledArtifact): number {
  const left = a.installed_at;
  const right = b.installed_at;
  if (left === right) return 0;
  if (left === null) return 1;
  if (right === null) return -1;
  return right - left;
}

/**
 * The day a tool was installed, as Finder's Date Added column says it,
 * short: 「9月28日」, "Sep 28" this year, and with the year before it --
 * 「2025年4月19日」, "Apr 19, 2025" -- for any other. Local time, as the
 * user's calendar has it.
 */
export function installedDateText(seconds: number, language: string, nowMs: number): string {
  const date = new Date(seconds * 1000);
  const thisYear = date.getFullYear() === new Date(nowMs).getFullYear();
  return thisYear ? shortDateText(date, language) : mediumDateText(date, language);
}

/**
 * What a row shows in its version's place while the list is sorted by
 * date installed, as By Size shows its size there (`sizeCellOf`): the day
 * the order goes by, or 「—」, muted, for a row whose source said none.
 */
export function installedDateCellOf(
  artifact: InstalledArtifact,
  language: string,
  nowMs: number,
): { text: string; muted: boolean } {
  if (artifact.installed_at === null) return { text: "—", muted: true };
  return { text: installedDateText(artifact.installed_at, language, nowMs), muted: false };
}
