/**
 * Help's 「常见问题」 (src-tauri/src/menu.rs): the questions people who
 * live with command-line tools without being programmers actually ask
 * (research 13-user-pains, the round-5 walkthrough), each answered in two
 * to four plain sentences, for the sheet that lists them
 * (src/components/FaqSheet.tsx). Each answer says only what the code does:
 * the track's report ties every sentence to the lines that do it. Where
 * Banager has a place to act on the answer, the question carries it
 * (`FaqView`), and the sheet's 查看 goes there, as the setup check's does.
 */
import { create } from "zustand";
import type { InstalledShow } from "./families";

/** One question's name: its words are `faq.questions.<id>` in src/i18n. */
export type FaqId =
  | "notFound"
  | "cantUpdate"
  | "twins"
  | "password"
  | "leftBehind"
  | "changesMac"
  | "otherPrograms"
  | "sizes"
  | "majorUpdate"
  | "autoCheck";

/**
 * Where a question's 查看 goes, the sheet closing first: the Installed page
 * on every source with one of its 「显示」 choices; the Installed page on
 * everything, its largest tools first; the Updates page; Other Programs; or
 * Settings.
 */
export type FaqView =
  | { kind: "installed"; show: InstalledShow }
  | { kind: "installedBySize" }
  | { kind: "updates" }
  | { kind: "unknown" }
  | { kind: "settings" };

export interface FaqItem {
  id: FaqId;
  /** Its 查看, or null where there is no one place to act on the answer. */
  view: FaqView | null;
}

/**
 * The questions, in the sheet's order: the wall a newcomer hits first --
 * a command Terminal does not find -- then updating, two copies, the
 * password, uninstalling, what Banager touches, Other Programs, sizes,
 * major updates, and the daily check.
 */
export const FAQ_ITEMS: readonly FaqItem[] = [
  { id: "notFound", view: { kind: "installed", show: "notOnPath" } },
  { id: "cantUpdate", view: { kind: "updates" } },
  { id: "twins", view: { kind: "installed", show: "twins" } },
  { id: "password", view: null },
  { id: "leftBehind", view: { kind: "installed", show: "all" } },
  { id: "changesMac", view: null },
  { id: "otherPrograms", view: { kind: "unknown" } },
  { id: "sizes", view: { kind: "installedBySize" } },
  { id: "majorUpdate", view: { kind: "updates" } },
  { id: "autoCheck", view: { kind: "settings" } },
];

/** Whether the sheet is open: Help's 「常见问题」 opens it, over any page. */
export const useFaqSheet = create<{ open: boolean }>(() => ({ open: false }));

/** Opens the sheet over whatever page is showing. */
export function openFaqSheet(): void {
  useFaqSheet.setState({ open: true });
}
