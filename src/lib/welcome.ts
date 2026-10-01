/**
 * Help's 「欢迎使用Banager」 (src-tauri/src/menu.rs): the welcome sheet
 * (src/components/WelcomeSheet.tsx) again, after the first launch showed
 * it. The first launch decides from settings.json's `welcome_seen`; this
 * is only the menu's way in, and it reads and writes no setting.
 */
import { create } from "zustand";

/** Whether Help's item asked for the sheet and it has not been closed since. */
export const useWelcomeAgain = create<{ open: boolean }>(() => ({ open: false }));

/** Shows the welcome sheet over whatever page is showing. */
export function openWelcomeSheet(): void {
  useWelcomeAgain.setState({ open: true });
}
