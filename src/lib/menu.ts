/**
 * The menu bar's items that act in the page (src-tauri/src/menu.rs):
 * Settings… (⌘,), Check Again (⌘R) and Search (⌘F). Each does what the
 * page's own control for it does, through the same code, so the two
 * cannot drift apart. The menu bar's other items are macOS's own and
 * never reach the page.
 */
import { useEffect } from "react";
import { onMenuCommand, type MenuCommand } from "./api";
import { useCheckAgain } from "./queries";
import { useUiStore } from "../store/ui";

/**
 * Mounted once, by `App`, next to `useOperationEvents`:
 *
 * - Settings… opens Settings, as the sidebar's Settings does (`setPage`).
 * - Check Again is the header's Check again (`useCheckAgain`): the one
 *   refresh, and nothing while one runs.
 * - Search opens the Installed page with its search box focused
 *   (`searchInstalled`), wherever the focus was -- the sidebar, a row, a
 *   field -- since what macOS hands the page is the item, not a key press
 *   on whatever had the focus.
 */
export function useMenuCommands(): void {
  const setPage = useUiStore((s) => s.setPage);
  const searchInstalled = useUiStore((s) => s.searchInstalled);
  const { checkAgain } = useCheckAgain();

  // All three are the same functions from one render to the next, so the
  // window listens once.
  useEffect(() => {
    const run: Record<MenuCommand, () => void> = {
      settings: () => setPage("settings"),
      checkAgain,
      search: searchInstalled,
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
        // The three items then do nothing; the page's own controls still
        // do all they do.
        console.error("listening to the menu bar failed", e);
      });
    return () => {
      cancelled = true;
      stop?.();
    };
  }, [setPage, searchInstalled, checkAgain]);
}
