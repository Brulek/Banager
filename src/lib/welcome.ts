/**
 * Help's 「欢迎使用Banager」 (src-tauri/src/menu.rs): the welcome sheet
 * (src/components/WelcomeSheet.tsx) again, after the first launch showed
 * it. The first launch decides from settings.json's `welcome_seen`; this
 * is only the menu's way in, and it reads and writes no setting.
 */
import { create } from "zustand";

/**
 * Whether Help's item asked for the sheet and it has not been closed since;
 * and why saving that it was seen failed the last time it was closed, as
 * `set_settings` rejected, until a later close saves it: said over the page
 * (`SettingsNotice`), since the sheet is gone by then.
 */
export const useWelcomeAgain = create<{ open: boolean; saveError: string | null }>(() => ({
  open: false,
  saveError: null,
}));

/** Shows the welcome sheet over whatever page is showing. */
export function openWelcomeSheet(): void {
  useWelcomeAgain.setState({ open: true });
}
