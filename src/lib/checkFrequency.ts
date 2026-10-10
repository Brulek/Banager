/**
 * Settings → Updates' 「检查更新」 popup: 「不自动检查」, 「每天」 or
 * 「每周」, kept in two fields of `Settings` -- `auto_check`, whether the
 * automatic check runs at all, and `auto_check_every`, how often while it
 * does (crates/banager-core/src/settings.rs, `CheckEvery`). Two fields,
 * not one, so that a settings.json written by the Banager that only had
 * the daily check's switch reads as 「每天」 when that switch was on, and
 * one written now reads in that Banager as the daily check, on or off.
 */
import type { CheckEvery, Settings } from "./types";

/** The popup's three choices, in its order. */
export type AutoCheckChoice = "Off" | CheckEvery;

export const AUTO_CHECK_CHOICES: readonly AutoCheckChoice[] = ["Off", "Day", "Week"];

/** The popup's words for each choice, in the "settings" namespace. */
export const AUTO_CHECK_CHOICE_KEYS: Record<AutoCheckChoice, string> = {
  Off: "settings.checkEvery.off",
  Day: "settings.checkEvery.day",
  Week: "settings.checkEvery.week",
};

/** How often the automatic check runs while it is on: "Day" when the settings do not say. */
export function checkEvery(settings: Pick<Settings, "auto_check_every">): CheckEvery {
  return settings.auto_check_every ?? "Day";
}

/** What the popup shows for `settings`. */
export function autoCheckChoice(settings: Pick<Settings, "auto_check" | "auto_check_every">): AutoCheckChoice {
  return settings.auto_check ? checkEvery(settings) : "Off";
}

/**
 * `settings` with `choice` made in the popup. 「不自动检查」 turns the
 * check off, and 「有更新时通知我」 with it, which Settings offers only
 * while the check runs (`notifications_on` in
 * crates/banager-core/src/notify_updates.rs), and keeps how often it ran,
 * which nothing reads while it is off. 「每天」 and 「每周」 turn it on, at
 * that frequency, and leave the notification as it was.
 */
export function withAutoCheckChoice(settings: Settings, choice: AutoCheckChoice): Settings {
  if (choice === "Off") return { ...settings, auto_check: false, notify_updates: false };
  return { ...settings, auto_check: true, auto_check_every: choice };
}
