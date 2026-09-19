import { useEffect, useRef, useState, type UIEvent } from "react";
import { useTranslation } from "react-i18next";
import { useUiStore } from "../store/ui";
import { useOperations } from "../lib/queries";
import { outcomeArgs, outcomeKey } from "../lib/format";
import { ScrollArea } from "./ui/ScrollArea";

const NEAR_BOTTOM_PX = 32;

export function LogDrawer() {
  const { t } = useTranslation();
  const drawerOpen = useUiStore((s) => s.drawerOpen);
  const setDrawerOpen = useUiStore((s) => s.setDrawerOpen);
  const focusedOpId = useUiStore((s) => s.focusedOpId);
  const logs = useUiStore((s) => s.logs);
  const { data: operations } = useOperations();
  const viewportRef = useRef<HTMLDivElement>(null);
  const [stickToBottom, setStickToBottom] = useState(true);

  const visibleLogs = logs.filter((l) => l.opId === focusedOpId);
  const operation = (operations ?? []).find((op) => op.id === focusedOpId);

  useEffect(() => {
    const viewport = viewportRef.current;
    if (viewport && stickToBottom) {
      viewport.scrollTop = viewport.scrollHeight;
    }
  }, [visibleLogs.length, stickToBottom]);

  if (!drawerOpen) {
    return null;
  }

  function handleScroll(event: UIEvent<HTMLDivElement>) {
    const el = event.currentTarget;
    const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    setStickToBottom(distanceFromBottom <= NEAR_BOTTOM_PX);
  }

  return (
    <div
      role="dialog"
      aria-label={t("operations.logDrawerTitle")}
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
