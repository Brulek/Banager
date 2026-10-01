/**
 * The menu bar's items that act in the page (src-tauri/src/menu.rs):
 * Settings… (⌘,); the View menu's Overview, Updates, Installed and Other
 * Programs (⌘1 to ⌘4); Check Again (⌘R) and Search (⌘F); Help's Check
 * Tool Setup and Copy Diagnostic Info. Each does what the page's own
 * control for it does,
 * through the same code, so the two cannot drift apart. The menu bar's other items are macOS's own and never
 * reach the page.
 */
import { useEffect } from "react";
import { onMenuCommand, type MenuCommand } from "./api";
import { useDiagnosticsReveal } from "./diagnostics";
import { openToolSetupSheet } from "./toolSetupCheck";
import { useCheckAgain } from "./queries";
import { useUiStore } from "../store/ui";

/**
 * Mounted once, by `App`, next to `useOperationEvents`:
 *
 * - Settings… opens Settings, and Overview, Updates, Installed and Other
 *   Programs their pages, each as its row in the sidebar does (`openPage`):
 *   Installed on everything installed, whatever source it was showing.
 * - Check Again is the header's Check again (`useCheckAgain`): the one
 *   refresh, and nothing while one runs.
 * - Search opens the Installed page with its search box focused
 *   (`searchInstalled`), wherever the focus was -- the sidebar, a row, a
 *   field -- since what macOS hands the page is the item, not a key press
 *   on whatever had the focus.
 * - Check Tool Setup… opens the sheet Settings' 「检查…」 opens
 *   (`ToolSetupSheet`), over whatever page is showing: it only reads.
 * - Copy Diagnostic Info… opens Settings on its 「拷贝诊断信息」 button,
 *   brought into view and focused (`useDiagnosticsReveal`): the item
 *   copies nothing itself. WKWebView may not take a menu item's event as
 *   the click a clipboard write needs, and a copy that failed silently
 *   would leave the user pasting nothing; the button's click always
 *   counts, and says 「已拷贝」 beside itself. Hence the ellipsis: the item
 *   leads to one more step.
 */
export function useMenuCommands(): void {
  const openPage = useUiStore((s) => s.openPage);
  const searchInstalled = useUiStore((s) => s.searchInstalled);
  const { checkAgain } = useCheckAgain();

  // These are the same functions from one render to the next, so the
  // window listens once.
  useEffect(() => {
    const run: Record<MenuCommand, () => void> = {
      settings: () => openPage("settings"),
      overview: () => openPage("overview"),
      updates: () => openPage("updates"),
      installed: () => openPage("installed"),
      unknown: () => openPage("unknown"),
      checkAgain,
      search: searchInstalled,
      checkToolSetup: openToolSetupSheet,
      copyDiagnostics: () => {
        openPage("settings");
        useDiagnosticsReveal.setState({ reveal: true });
      },
    };
    let stop: (() => void) | undefined;
    let cancelled = false;
    onMenuCommand((command) => {
      // Inert once unmounted, while the listening may still be under way
      // (StrictMode's first mount), as `useOperationEvents` is.
      if (!cancelled) run[command]();
    })
      .then((stopListening) => {
        if (cancelled) {
          stopListening();
        } else {
          stop = stopListening;
        }
      })
      .catch((e: unknown) => {
        // The items then do nothing; the page's own controls still do all
        // they do.
        console.error("listening to the menu bar failed", e);
      });
    return () => {
      cancelled = true;
      stop?.();
    };
  }, [openPage, searchInstalled, checkAgain]);
}
