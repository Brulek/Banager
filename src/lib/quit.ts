/**
 * The page's part in quitting while an operation is under way
 * (src-tauri/src/quit.rs). On a Mac every way of quitting -- Quit Canager
 * (⌘Q), the Dock's Quit, a logout -- asks Rust first; while an operation is
 * not done, Rust calls the quit off, brings the window back and sends
 * `QUIT_REQUESTED_EVENT`, and the page asks the user
 * (`QuitQuestion`): 「还有 N 个操作没完成」, 「继续等待」 or 「仍然退出」. Rust
 * asks only while the page has said it listens (`askBeforeQuit`), so that a
 * page that could not listen, or has gone, leaves quitting as it was; and
 * Canager quits after all when the page does not say within 2 seconds that
 * the question is on screen (`quitQuestionShown`).
 */
import { useEffect, useRef } from "react";
import { askBeforeQuit, onQuitRequested } from "./api";
import type { OpKind, OpSummary } from "./types";

/**
 * Whether `op`'s command may be running now: Running, or being cancelled,
 * which stops a command already running. A Queued operation has started
 * nothing, and a Verifying one's command has ended: quitting stops them,
 * and leaves no tool half done.
 */
export function commandUnderWay(op: OpSummary): boolean {
  return op.status === "Running" || op.status === "CancelRequested" || op.status === "Cancelling";
}

/**
 * What quitting now may leave half done, by the kind of the operations
 * whose command is under way. A `Record` over `OpKind`, so a kind added to
 * the mirror without words here fails `tsc`.
 */
const HALF_DONE_KEYS: Record<OpKind, string> = {
  Install: "quit.body.Install",
  Uninstall: "quit.body.Uninstall",
  Upgrade: "quit.body.Upgrade",
};

/**
 * The question's line under its title, about `active`, the operations not
 * done, whose number is its `count`: that quitting now stops them -- and,
 * only while a command is under way (`commandUnderWay`), that the tool it
 * works on can be left half done: in the words of its kind when every
 * command under way is of one kind (updated, uninstalled), and in words
 * for any kind when they are of more than one.
 */
export function quitBodyKey(active: OpSummary[]): string {
  const kinds = new Set(active.filter(commandUnderWay).map((op) => op.kind));
  if (kinds.size === 0) return "quit.body.stops";
  if (kinds.size > 1) return "quit.body.mixed";
  const [kind] = kinds;
  return HALF_DONE_KEYS[kind];
}

/**
 * The line the question adds for an operation that has started and that
 * nothing can stop (`runsToItsEnd`): rustup's self update or self
 * uninstall, which its confirmation said could not be cancelled
 * (`operations.noCancelHint`).
 */
export const QUIT_NO_CANCEL_KEYS: Record<OpKind, string> = {
  Install: "quit.noCancel.Install",
  Uninstall: "quit.noCancel.Uninstall",
  Upgrade: "quit.noCancel.Upgrade",
};

/**
 * Calls `onRequest` with the question's number each time Rust asks
 * (`QUIT_REQUESTED_EVENT`), from mount to unmount, and -- once the window
 * listens, not before -- tells Rust to ask from now on (`askBeforeQuit`).
 * Should listening fail, Rust is told nothing, and a quit quits at once, as
 * it did before the page could ask. Unmounted -- as React takes the page
 * down after an error in drawing it that nothing catches -- it tells Rust
 * to stop asking: nobody would be there to answer, and a quit that Rust
 * called off would never happen. Mounted once, by `QuitQuestion`.
 */
export function useQuitRequests(onRequest: (question: number) => void): void {
  // The newest `onRequest`, so that the window listens once, whatever the
  // component hands in from one render to the next.
  const latest = useRef(onRequest);
  useEffect(() => {
    latest.current = onRequest;
  });

  useEffect(() => {
    let stop: (() => void) | undefined;
    let cancelled = false;
    onQuitRequested((question) => {
      // Inert once unmounted, while the listening may still be under way
      // (StrictMode's first mount), as `useMenuCommands` is.
      if (!cancelled) latest.current(question);
    })
      .then((stopListening) => {
        if (cancelled) {
          stopListening();
          return;
        }
        stop = stopListening;
        askBeforeQuit(true).catch((e: unknown) => {
          // Rust then asks nothing: a quit quits at once.
          console.error("ask_before_quit failed", e);
        });
      })
      .catch((e: unknown) => {
        console.error("listening for Quit failed", e);
      });
    return () => {
      cancelled = true;
      if (stop === undefined) return;
      // Told to ask, Rust stops: quitting quits at once again. A mount
      // that never listened -- StrictMode's first, one whose listening
      // failed -- told Rust nothing, and has nothing to take back.
      askBeforeQuit(false).catch((e: unknown) => {
        // Rust then asks a page that is gone, and quits 2 seconds later,
        // when nothing has said that the question is on screen.
        console.error("ask_before_quit failed", e);
      });
      stop();
    };
  }, []);
}
