/**
 * The menu bar's items that act in the page (src-tauri/src/menu.rs):
 * Settings… (⌘,); the View menu's Overview, Updates, Installed and Other
 * Programs (⌘1 to ⌘4); Check Again (⌘R) and Search (⌘F); Help's Copy
 * Diagnostic Info. Each does what the page's own control for it does,
 * through the same code, so the two cannot drift apart. The menu bar's other items are macOS's own and never
 * reach the page.
 */
import { useEffect } from "react";
import { onMenuCommand, type MenuCommand } from "./api";
import { useCopyDiagnostics, useDiagnosticsStatus } from "./diagnostics";
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
 * - Copy Diagnostic Info copies what Settings' button of that name copies,
 *   without the list of tools whatever its checkbox says, at once -- in
 *   the turn the item is chosen in, which is what lets the page write the
 *   clipboard -- then opens Settings, where 「已拷贝」 (or 「无法拷贝」)
 *   shows under that button, beside the checkbox that adds the list.
 *   Opening Settings is deliberate, beyond what the item's name says: the
 *   window has no other place to say whether the copy worked, and if the
 *   webview refused a write not started by a click, the button there is
 *   one click away. Whether WKWebView lets a menu item's event write the
 *   clipboard is still to be tried in a real window.
 */
export function useMenuCommands(): void {
  const openPage = useUiStore((s) => s.openPage);
  const searchInstalled = useUiStore((s) => s.searchInstalled);
  const { checkAgain } = useCheckAgain();
  const copyDiagnostics = useCopyDiagnostics();

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
      copyDiagnostics: () => {
        copyDiagnostics(false);
        openPage("settings");
        useDiagnosticsStatus.setState({ reveal: true });
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
  }, [openPage, searchInstalled, checkAgain, copyDiagnostics]);
}
