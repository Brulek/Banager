import { useEffect, useRef, useState, type KeyboardEvent, type UIEvent } from "react";
import { useTranslation } from "react-i18next";
import { useUiStore } from "../store/ui";
import { useOperations } from "../lib/queries";
import { outcomeArgs, outcomeKey } from "../lib/format";
import { ScrollArea } from "./ui/ScrollArea";

const NEAR_BOTTOM_PX = 32;

// Everything inside the drawer that a keyboard can land on. Used only to
// find the ends of the ring for the Tab wrap below; the browser handles
// every step in between.
const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

export function LogDrawer() {
  const { t } = useTranslation();
  const drawerOpen = useUiStore((s) => s.drawerOpen);
  const setDrawerOpen = useUiStore((s) => s.setDrawerOpen);
  const focusedOpId = useUiStore((s) => s.focusedOpId);
  const logs = useUiStore((s) => s.logs);
  const { data: operations } = useOperations();
  const viewportRef = useRef<HTMLDivElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const [stickToBottom, setStickToBottom] = useState(true);

  const visibleLogs = logs.filter((l) => l.opId === focusedOpId);
  const operation = (operations ?? []).find((op) => op.id === focusedOpId);

  useEffect(() => {
    const viewport = viewportRef.current;
    if (viewport && stickToBottom) {
      viewport.scrollTop = viewport.scrollHeight;
    }
  }, [visibleLogs.length, stickToBottom]);

  // Opening the drawer moves focus into it, and closing it hands focus
  // back to whatever opened it -- the "show the log" button the user just
  // pressed, which is where they expect to be standing afterwards. Without
  // this, closing the drawer drops focus on the document body and a
  // keyboard user restarts their journey from the top of the window.
  useEffect(() => {
    if (!drawerOpen) {
      return;
    }
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    panelRef.current?.focus();
    return () => {
      opener?.focus();
    };
  }, [drawerOpen]);

  if (!drawerOpen) {
    return null;
  }

  function handleScroll(event: UIEvent<HTMLDivElement>) {
    const el = event.currentTarget;
    const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    setStickToBottom(distanceFromBottom <= NEAR_BOTTOM_PX);
  }

  function handleKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    // Escape is how every other panel on this platform closes. The drawer
    // announced itself as a dialog and then ignored the one key a
    // VoiceOver or keyboard user reaches for to dismiss it.
    if (event.key === "Escape") {
      event.preventDefault();
      setDrawerOpen(false);
      return;
    }
    if (event.key !== "Tab") {
      return;
    }
    // Keep Tab inside the drawer. It sits over the page it belongs to, so
    // tabbing out of it lands the cursor on controls the user cannot see
    // and cannot tell they are on.
    const panel = panelRef.current;
    if (!panel) {
      return;
    }
    const stops = Array.from(panel.querySelectorAll<HTMLElement>(FOCUSABLE));
    if (stops.length === 0) {
      event.preventDefault();
      panel.focus();
      return;
    }
    const first = stops[0];
    const last = stops[stops.length - 1];
    const active = document.activeElement;
    if (event.shiftKey) {
      // Backwards off the front of the ring -- and off the panel itself,
      // which is where focus starts when the drawer opens.
      if (active === first || active === panel || !panel.contains(active)) {
        event.preventDefault();
        last.focus();
      }
      return;
    }
    if (active === last) {
      event.preventDefault();
      first.focus();
    }
  }

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={t("operations.logDrawerTitle")}
      ref={panelRef}
      tabIndex={-1}
      onKeyDown={handleKeyDown}
      className="fixed inset-x-0 bottom-12 top-1/2 border-t border-[var(--color-border)] bg-[var(--color-background)]"
    >
      <div className="flex items-center justify-between border-b border-[var(--color-border)] px-4 py-2">
        <p className="text-sm font-medium text-[var(--color-foreground)]">
          {t("operations.logDrawerTitle")}
        </p>
        <button
          type="button"
          onClick={() => setDrawerOpen(false)}
          className="text-sm text-[var(--color-muted)]"
        >
          {t("common.close")}
        </button>
      </div>
      <ScrollArea className="h-[calc(100%-96px)]" ref={viewportRef} onViewportScroll={handleScroll}>
        <div role="log" className="px-4 py-2 font-mono text-xs">
          {visibleLogs.map((line) => (
            <p
              key={line.seq}
              className={line.stream === "Stderr" ? "text-[var(--color-danger)]" : undefined}
            >
              {line.line}
            </p>
          ))}
        </div>
      </ScrollArea>
      {operation?.outcome ? (
        <div className="border-t border-[var(--color-border)] px-4 py-2 text-sm">
          {t(`operations.outcome.${outcomeKey(operation.outcome)}`, outcomeArgs(operation.outcome))}
        </div>
      ) : null}
    </div>
  );
}
