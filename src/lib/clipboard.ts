import { useCallback, useEffect, useRef, useState } from "react";

/** What the last copy did: said for a moment, then nothing. */
export type CopyStatus = "copied" | "failed" | null;

/**
 * How long "Copied" or "Couldn't copy" stays on screen -- and, beside it
 * on the Unknown page, "Couldn't show it in Finder".
 */
export const SHOWN_FOR_MS = 2500;

/**
 * A row's "Copy command", and a word about whether it worked -- on the
 * Updates page and on the Installed page alike -- and the Unknown page's
 * "Copy path". The clipboard can refuse (or be missing altogether outside
 * a secure context), and a menu item that did nothing must not look as if
 * it had, so each page shows `status` where it can be read
 * (`role="status"`).
 */
export function useCopyCommand(): { status: CopyStatus; copy: (command: string) => void } {
  const [status, setStatus] = useState<CopyStatus>(null);
  const timerRef = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(timerRef.current), []);

  const copy = useCallback((command: string) => {
    const say = (next: "copied" | "failed") => {
      setStatus(next);
      window.clearTimeout(timerRef.current);
      timerRef.current = window.setTimeout(() => setStatus(null), SHOWN_FOR_MS);
    };
    if (navigator.clipboard === undefined) {
      say("failed");
      return;
    }
    navigator.clipboard.writeText(command).then(
      () => say("copied"),
      () => say("failed"),
    );
  }, []);

  return { status, copy };
}

/** What `status` says beside the button or in the toolbar: 「已拷贝」, 「无法拷贝」, or nothing. */
export function copyStatusText(t: (key: string) => string, status: CopyStatus): string | null {
  return status === "copied" ? t("common.copied") : status === "failed" ? t("common.copyFailed") : null;
}
